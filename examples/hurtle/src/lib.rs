//! Hurtle is a Wordle-style game. The Canvas Guest handles input and drawing; this Server Guest
//! owns each player's hidden answer, validates guesses, and applies the final penalty as the poster.

#[cfg(test)]
mod tests;

mod badges;
mod pdk;

use chittr_cartridge::{
    kv,
    poster::{self, BanDuration},
    push::{push_render, PushTarget},
    timer,
};
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

const ANSWERS: &str = include_str!("../cartridge/assets/answers.txt");
// The five-letter list 0.1.x drew from, in its original order, so the word that was live when a
// Chitt upgraded stays the same until the next rotation.
const LEGACY_ANSWERS: &str = include_str!("../cartridge/assets/answers-5.txt");
// Every word a guess may be, from the ENABLE list: twelve letters, and five for games started on the
// five-letter list. Each is sorted, one word to a line, every line the same width.
const GUESSES_12: &str = include_str!("../cartridge/assets/guesses-12.txt");
const GUESSES_5: &str = include_str!("../cartridge/assets/guesses-5.txt");
const MAX_GUESSES: usize = 6;
// The stored Chitt text is separate from the Liquid card. Keep creation and upgrades in sync.
const CHITT_CONTENT: &str = "Hurtle: guess the hidden twelve-letter word in six tries. You start with six health; each wrong guess, invalid word or duplicate costs one health and spills blood. Unscored guesses give no letter clues. At zero health, your death leaves three splatters. The word changes every 24 hours. You get one attempt per word. Starting opts you in: if you fail, you will be suspended from the site for one hour.";
// Every word in ANSWERS has this length (the tests hold the list to it). A game keeps the length
// of its own answer, so a game started on the five-letter list can still be finished.
#[cfg(test)]
const WORD_LENGTH: usize = 12;
// The first day drawn from ANSWERS. A Chitt that was already running when it upgraded keeps its
// live five-letter word until the timer rotates to the next day; a new Chitt starts on twelve.
const TWELVE_FROM_KEY: &str = "twelve-letters-from-day";
const START_CONTROL: &str = "start";
const RESET_CONTROL: &str = "reset-day";
const RESET_KEY: &str = "admin-reset-next-day";
const RESET_LOCK_KEY: &str = "admin-reset-worker";
const RESET_TIMER: &str = "admin-reset-cleanup";
// Stay comfortably below the 500-host-call and 1-second Control limits. Larger stores resume on
// a timer; the durable reset marker blocks play and initialization until the whole store is clear.
const RESET_BATCH: usize = 128;
const RESET_DELAY: u64 = 10;
const DAY_SECONDS: u64 = 24 * 60 * 60;
const DAILY_TIMER: &str = "daily-word";
const DAILY_DAY_KEY: &str = "daily-day";
const DAILY_TIMER_READY_KEY: &str = "daily-timer-ready";
const DAILY_PENALTY_TTL: u64 = 2 * DAY_SECONDS;
const MAX_CHARACTER_TAG_BYTES: usize = 32;
const MAX_VISIBLE_STAINS: usize = 64;
const DEFAULT_BLOOD_COLOR: u32 = 0x9b2638ff;
// Names the card lists under each heading; the rest are counted.
const ROLL_SHOWN: usize = 24;
// Rows a list shows before it scrolls: the template fixes each row at 16 px with a 4 px gap in a
// 128 px box, so six fit with half of a seventh peeking out. Host style filters strip <style>, so
// the server, which knows the count, tells the card when to fade the list and say it scrolls.
const ROLL_VISIBLE: usize = 6;
const SURVIVOR_COLOR: u32 = 0xc5a16aff;

// One atomic ledger per word: hits are deduplicated separately from deaths. Older ledgers have
// one stain per victim and no hit records. Only bounded colours/seeds and counts reach viewers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BloodLedger {
    victims: BTreeSet<String>,
    recent: Vec<Bloodstain>,
    #[serde(default)]
    hits: BTreeSet<String>,
    #[serde(default)]
    total: Option<u64>,
}

impl BloodLedger {
    fn splatter_count(&self) -> u64 {
        self.total.unwrap_or(self.victims.len() as u64)
    }
}

// Who fell and who survived each day, in the order they finished. An entry learns its mention and
// colour when the player is seen; entries rebuilt by `migrate` from before the roll existed start
// without them. Only mentions and colours ever reach the card, never IDs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DailyRoll {
    #[serde(default)]
    fallen: Vec<RollEntry>,
    #[serde(default)]
    survivors: Vec<RollEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RollEntry {
    id: String,
    #[serde(default)]
    mention: Option<String>,
    #[serde(default)]
    color: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bloodstain {
    color: u32,
    seed: u32,
}
// A first-guess win gets a distinct, personalized congratulation rather than the ordinary win copy.
const FIRST_GUESS_WIN_MESSAGES: &[&str] = &[
    "A perfect first strike! Today's word falls in one guess.",
    "Flawless! You read the word on your very first attempt.",
    "One guess, one victory. The executioner never had a chance.",
    "A stunning opening move—you solved the daily word at once.",
    "Straight to the answer. That first guess was all you needed.",
    "Incredible aim! The first guess ends the game in your favour.",
    "The word yields immediately. What a first-guess triumph!",
    "No second thought needed—you nailed it on the first try.",
];
const WIN_MESSAGES: &[&str] = &[
    "Congratulations on not dying!",
    "You live to see another day. The executioner is furious.",
    "You escaped the chopping block with all limbs attached. Well played!",
    "Murder postponed: you guessed the word in time.",
    "Against all odds, you remain among the living. Congratulations!",
    "Your head remains firmly attached. A remarkable achievement.",
    "You solved the word and kept your head. A triumph!",
    "The executioner leaves empty-handed. Better luck to them tomorrow.",
    "You may continue living. Try not to squander it.",
    "Another day, another word, and you still have all your limbs.",
    "You dodged a gruesome fate. Nicely done!",
    "The word has been guessed; your life has been spared!",
];
const EXECUTED_MESSAGES: &[&str] = &[
    "The executioner caught you. You have been killed!",
    "Your luck ran out; the executioner made sure you did too.",
    "The word won. You were brutally murdered.",
    "The executioner has done their grisly work. Rest in pieces.",
    "You missed your final chance. You have been executed!",
    "Your guesses ran out, and so did your life.",
    "The sentence was carried out: you have been slain.",
    "You failed the word and were dispatched in cold blood.",
    "Execution confirmed. The executioner claims another victim.",
    "The final guess missed; the blade did not.",
    "You have met a rather untimely and violent end.",
    "No word, no mercy. You have been murdered.",
];
const EXECUTION_REFUSED_MESSAGES: &[&str] = &[
    "The executioner was stopped at the last moment. You escaped death!",
    "You lost, but the would-be murderer was turned away.",
    "The executioner lacked permission to finish the job. You live!",
    "The sentence could not be carried out; you survive this round.",
    "Your guesses ran out, but the blade never fell.",
    "The executioner was denied entry. Consider yourself lucky.",
    "The word won, but the execution was refused. You live to guess again.",
    "You lost this round; the attempted murder was blocked.",
    "The site intervened before the executioner could reach you.",
    "The executioner was overruled. Your life is spared—for now.",
    "Your death was prevented by a technicality. How fortunate.",
    "You ran out of guesses, but escaped with your life.",
];
const EXECUTION_UNCONFIRMED_MESSAGES: &[&str] = &[
    "Your fate is uncertain; the executioner's work could not be confirmed.",
    "The execution order is awaiting confirmation. Are you alive?",
    "We cannot yet confirm whether you escaped the blade.",
    "The executioner's actions remain a mystery. Check your pulse.",
];
const EXECUTION_SUBMITTING_MESSAGES: &[&str] = &[
    "You ran out of guesses. The executioner is coming for you.",
    "The word escaped you; the executioner is on their way.",
    "Six misses. Prepare to meet your grisly fate.",
    "Your guesses ran out. The blade is already falling.",
];

// Stored with the accepted guess before the shared ledger is updated, so a retry preserves the
// hit's colour and geometry. Legacy attempts have no wounds and are not splattered retroactively.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Wound {
    guess: usize,
    stains: Vec<Bloodstain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Guess {
    word: String,
    marks: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Game {
    #[serde(default)]
    day: i64,
    answer: String,
    guesses: Vec<Guess>,
    complete: bool,
    won: bool,
    penalty_pending: bool,
    penalty_applied: bool,
    penalty_error: Option<String>,
    // Whether a canvas has finished showing this loss, so a new canvas knows to play it again.
    #[serde(default)]
    loss_seen: bool,
    // Only a newly completed first-guess win snapshots the configured reward. Old games load
    // without one; changing the setting cannot retroactively award or swap their badges.
    #[serde(default)]
    first_guess_badge: Option<badges::BadgeReward>,
    #[serde(default)]
    wounds: Vec<Wound>,
}

impl Game {
    fn new(day: i64, answer: String) -> Self {
        Self {
            day,
            answer,
            guesses: Vec::new(),
            complete: false,
            won: false,
            penalty_pending: false,
            penalty_applied: false,
            penalty_error: None,
            loss_seen: false,
            first_guess_badge: None,
            wounds: Vec::new(),
        }
    }

    fn health(&self) -> usize {
        MAX_GUESSES.saturating_sub(
            self.guesses
                .iter()
                .filter(|guess| guess.word != self.answer)
                .count(),
        )
    }
}

pub(crate) fn init(input: InitInput) -> Result<InitOutput, Error> {
    let result = initialize_daily_word(input.chitt.created_at.timestamp());
    Ok(match result {
        Ok(_day) => InitOutput {
            content: CHITT_CONTENT.to_string(),
            ..Default::default()
        },
        Err(error) => InitOutput {
            content: String::new(),
            error: Some(error),
            ..Default::default()
        },
    })
}

pub(crate) fn on_timer(input: TimerInput) -> Result<TimerOutput, Error> {
    let result = (|| {
        if input.name == RESET_TIMER {
            // A delayed delivery from a previous reset must not touch the next reset's work.
            if let Some(day) = reset_day()? {
                if input
                    .payload
                    .as_ref()
                    .and_then(|payload| payload.get("day"))
                    .and_then(Value::as_i64)
                    == Some(day)
                {
                    return continue_reset(day);
                }
            }
        } else if input.name == DAILY_TIMER && reset_day()?.is_none() {
            initialize_daily_word(input.chitt.created_at.timestamp())?;
            return rotate_daily_word(&input.payload);
        }
        Ok(())
    })();
    Ok(TimerOutput {
        error: result.err(),
    })
}

// A new Cartridge Version keeps the live word, every player's attempt, the blood and the rotation
// timer; it only makes sure a Chitt from before the daily word existed gets one, and that today's
// roll holds everyone who finished before the roll existed. It also refreshes the stored Chitt
// description: init is not called again on upgrade, and changing the template cannot update it.
pub(crate) fn migrate(input: MigrateInput) -> Result<MigrateOutput, Error> {
    let result = match reset_day() {
        Ok(Some(day)) => continue_reset(day),
        Ok(None) => {
            initialize_daily_word(input.chitt.created_at.timestamp()).and_then(backfill_roll)
        }
        Err(error) => Err(error),
    }
    .and_then(|_| {
        // No post_id means this Live Chitt, using the poster's own edit authority (not an admin
        // action). The host defers the edit until the swap commits. Do not change its OOC flag.
        poster::edit_chitt(poster::EditArgs {
            content: CHITT_CONTENT.to_string(),
            quirks: Some(false),
            ..Default::default()
        })
        .map_err(poster_fail)
    });
    Ok(MigrateOutput {
        error: result.err(),
    })
}

pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    let is_admin = input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.is_admin);
    match reset_day() {
        Ok(Some(day)) => return Ok(render_reset(day, input.observer.is_some(), is_admin)),
        Ok(None) => {}
        Err(error) => return Ok(render_error(error, input.observer.is_some())),
    }
    let day = match initialize_daily_word(input.chitt.created_at.timestamp()) {
        Ok(day) => day,
        Err(error) => return Ok(render_error(error, input.observer.is_some())),
    };
    // A suspended player still sees their own board, so a loss plays out to the end; the guess and
    // start paths refuse their input instead.
    let suspended = input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.is_suspended);
    let game = match input.observer.as_ref() {
        Some(observer) => match read_game(&observer.character_id) {
            Ok(game) => game.filter(|game| game.day == day),
            Err(error) => return Ok(render_error(error, true)),
        },
        None => None,
    };
    let state = match input.observer.as_ref() {
        Some(observer) => player_canvas_state(game.as_ref(), day, observer),
        None => shared_canvas_state(None, day),
    };
    let state = match state {
        Ok(state) => state,
        Err(error) => return Ok(render_error(error, input.observer.is_some())),
    };
    let roll = match input.observer.as_ref() {
        Some(observer) => learn_roll_name(day, observer),
        None => read_roll(day).map(|(_, roll)| roll),
    };
    let roll = match roll {
        Ok(roll) => roll,
        Err(error) => return Ok(render_error(error, input.observer.is_some())),
    };
    let reward = game
        .as_ref()
        .and_then(|game| game.first_guess_badge.as_ref());
    let mut data = object(json!({
        "badgeRewardStatus": reward.map(|reward| reward.status),
        "badgeRewardError": reward.and_then(|reward| reward.error.as_deref()),
        "obituary": roll_names(&roll.fallen, DEFAULT_BLOOD_COLOR),
        "obituaryCount": roll.fallen.len(),
        "obituaryMore": roll.fallen.len().saturating_sub(roll_names(&roll.fallen, 0).len()),
        "obituaryScrolls": roll_names(&roll.fallen, 0).len() > ROLL_VISIBLE,
        "survivors": roll_names(&roll.survivors, SURVIVOR_COLOR),
        "survivorsCount": roll.survivors.len(),
        "survivorsMore": roll.survivors.len().saturating_sub(roll_names(&roll.survivors, 0).len()),
        "survivorsScrolls": roll_names(&roll.survivors, 0).len() > ROLL_VISIBLE,
        "failureCount": state["blood"]["fallen"],
        "splatterCount": state["blood"]["total"],
        "health": state["health"],
        "maxHealth": MAX_GUESSES,
        "bloodStains": template_stains(&state),
        "signedIn": input.observer.is_some(),
        "isAdmin": is_admin,
        "resetting": false,
        "started": game.is_some(),
        "complete": game.as_ref().is_some_and(|game| game.complete),
        "won": game.as_ref().is_some_and(|game| game.won),
        "isSuspended": suspended,
        "wordLength": state["length"],
    }));
    if is_admin {
        let badge_id = match badges::configured_id() {
            Ok(id) => id,
            Err(error) => return Ok(render_error(error, true)),
        };
        data.insert(
            "firstGuessBadgeEnabled".to_string(),
            json!(badge_id.is_some()),
        );
        data.insert(
            "firstGuessBadgeId".to_string(),
            json!(badge_id.unwrap_or_default()),
        );
    }
    Ok(RenderOutput {
        template: "card".to_string(),
        data,
        canvas: Some(state),
        error: None,
    })
}

pub(crate) fn on_control(input: ControlInput) -> Result<ControlOutput, Error> {
    let result = if input.control == RESET_CONTROL {
        reset_game(input)
    } else if input.control == badges::CONTROL {
        badges::configure(input)
    } else {
        start_game(input)
    };
    Ok(ControlOutput {
        error: result.err(),
    })
}

// Authorization precedes every read/write, including retries of an interrupted cleanup. Hiding a
// button is not authorization, nor is being the Chitt's poster.
fn reset_game(input: ControlInput) -> Result<(), HookError> {
    if !input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.is_admin)
    {
        return Err(fail("forbidden", "only site admins can reset Hurtle"));
    }
    if input.value.as_deref() != Some("reset") {
        return Err(fail("unknown_control", "Hurtle has no such control value"));
    }
    with_reset_lock(|| begin_reset(input.chitt.created_at.timestamp()))
}

fn begin_reset(created_at: i64) -> Result<(), HookError> {
    if let Some(day) = reset_day()? {
        return reset_batch(day);
    }
    let day = initialize_daily_word(created_at)?
        .checked_add(1)
        .ok_or_else(|| fail("invalid_state", "the daily word date cannot advance"))?;
    // Arrange recovery before deleting anything. The marker has no TTL: an interrupted reset must
    // never fall back to the Chitt's creation date or reopen partially deleted attempts.
    schedule_reset(day)?;
    kv::set_if_absent(RESET_KEY, day.to_string().as_bytes(), None)
        .map_err(|error| fail("storage_error", error.to_string()))?;
    reset_batch(reset_day()?.ok_or_else(|| fail("invalid_state", "the reset marker is missing"))?)
}

// Timers are at-least-once and two admins can click together. Serialize reset workers; the lease
// outlives even a lifecycle Hook's 10-second deadline, then expires after a crash so recovery can
// retry. The persistent marker never expires.
fn with_reset_lock(operation: impl FnOnce() -> Result<(), HookError>) -> Result<(), HookError> {
    if !kv::set_if_absent(RESET_LOCK_KEY, b"working", Some(30))
        .map_err(|error| fail("storage_error", error.to_string()))?
    {
        return Err(fail(
            "reset_in_progress",
            "another Hurtle reset worker is already running",
        ));
    }
    let result = operation();
    let released =
        kv::delete(RESET_LOCK_KEY).map_err(|error| fail("storage_error", error.to_string()));
    result.and(released.map(|_| ()))
}

fn reset_day() -> Result<Option<i64>, HookError> {
    kv::get(RESET_KEY)
        .map_err(|error| fail("storage_error", error.to_string()))?
        .map(|entry| {
            std::str::from_utf8(&entry.value)
                .ok()
                .and_then(|value| value.parse().ok())
                .ok_or_else(|| fail("invalid_state", "the reset date is invalid"))
        })
        .transpose()
}

fn schedule_reset(day: i64) -> Result<(), HookError> {
    timer::schedule_timer(RESET_TIMER, RESET_DELAY, json!({ "day": day }))
        .map_err(|error| fail("timer_error", error.to_string()))
}

fn continue_reset(day: i64) -> Result<(), HookError> {
    with_reset_lock(|| {
        if reset_day()? != Some(day) {
            return Ok(());
        }
        reset_batch(day)
    })
}

fn is_reset_key(key: &str) -> bool {
    key == RESET_KEY || key == RESET_LOCK_KEY
}

fn reset_batch(day: i64) -> Result<(), HookError> {
    schedule_reset(day)?;
    timer::cancel_timer(DAILY_TIMER).map_err(|error| fail("timer_error", error.to_string()))?;
    let listing = kv::list("").map_err(|error| fail("storage_error", error.to_string()))?;
    // Re-list from the start after each batch. This works even above list's 1,000-key cap, without
    // a cursor or a snapshot that could itself exceed the value-size limit. Delete unknown keys too.
    for key in listing
        .keys
        .iter()
        .filter(|key| !is_reset_key(key))
        .take(RESET_BATCH)
    {
        kv::delete(key).map_err(|error| fail("storage_error", error.to_string()))?;
    }
    let remaining = kv::list("").map_err(|error| fail("storage_error", error.to_string()))?;
    if remaining.truncated || remaining.keys.iter().any(|key| !is_reset_key(key)) {
        return push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()));
    }
    // Only fresh scheduling metadata is recreated. Keep the marker until every step succeeds, so
    // a retry can safely discard partial initialization and finish the SAME next day.
    kv::set(DAILY_DAY_KEY, day.to_string().as_bytes(), None)
        .map_err(|error| fail("storage_error", error.to_string()))?;
    kv::set(TWELVE_FROM_KEY, day.to_string().as_bytes(), None)
        .map_err(|error| fail("storage_error", error.to_string()))?;
    schedule_daily_word(day)?;
    kv::delete(RESET_KEY).map_err(|error| fail("storage_error", error.to_string()))?;
    timer::cancel_timer(RESET_TIMER).map_err(|error| fail("timer_error", error.to_string()))?;
    push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()))
}

fn render_reset(day: i64, signed_in: bool, is_admin: bool) -> RenderOutput {
    let mut state = canvas_state(None);
    state.insert("round".to_string(), json!(day));
    state.insert("length".to_string(), json!(answer_for(day, day).len()));
    state.insert(
        "blood".to_string(),
        json!({ "total": 0, "fallen": 0, "stains": [] }),
    );
    RenderOutput {
        template: "card".to_string(),
        data: object(json!({
            "signedIn": signed_in, "isAdmin": is_admin, "resetting": true,
            "started": false, "complete": false, "won": false,
            "obituary": [], "survivors": [], "obituaryCount": 0, "survivorsCount": 0,
            "obituaryMore": 0, "survivorsMore": 0, "obituaryScrolls": false, "survivorsScrolls": false,
            "failureCount": 0, "splatterCount": 0, "bloodStains": [], "wordLength": state["length"],
        })),
        canvas: Some(state),
        error: None,
    }
}

fn start_game(input: ControlInput) -> Result<(), HookError> {
    if input.control != START_CONTROL || input.value.as_deref() != Some("start") {
        return Err(fail("unknown_control", "Hurtle has no such control"));
    }
    let observer = input
        .observer
        .ok_or_else(|| fail("logged_out", "log in to play and accept the penalty"))?;
    if observer.is_suspended {
        return Err(fail(
            "character_suspended",
            "you are suspended and cannot play Hurtle",
        ));
    }
    let day = initialize_daily_word(input.chitt.created_at.timestamp())?;
    let game = Game::new(day, daily_answer(day)?);
    let bytes =
        serde_json::to_vec(&game).map_err(|error| fail("invalid_state", error.to_string()))?;
    let key = game_key(&observer.character_id);
    let Some((previous, previous_game)) = read_game_version(&observer.character_id)? else {
        if !kv::set_if_absent(&key, &bytes, None)
            .map_err(|error| fail("storage_error", error.to_string()))?
        {
            return Err(fail(
                "game_updated",
                "the game changed elsewhere; refresh the card",
            ));
        }
        return Ok(());
    };
    if previous_game.day != day {
        let swapped = kv::compare_and_swap(&key, Some(&previous), &bytes)
            .map_err(|error| fail("storage_error", error.to_string()))?;
        if !swapped {
            return Err(fail(
                "game_updated",
                "the game changed elsewhere; refresh the card",
            ));
        }
    }
    Ok(())
}

pub(crate) fn on_canvas_message(input: CanvasMessageInput) -> Result<CanvasMessageOutput, Error> {
    let seen = input.message.get("type").and_then(Value::as_str) == Some("seen");
    let result = handle_message(input);
    Ok(match result {
        Ok(state) => CanvasMessageOutput {
            reply: Some(state),
            rerender: Some(!seen),
            error: None,
        },
        Err(error) => CanvasMessageOutput {
            reply: None,
            rerender: None,
            error: Some(error),
        },
    })
}

fn handle_message(input: CanvasMessageInput) -> Result<Map<String, Value>, HookError> {
    let observer = input
        .observer
        .ok_or_else(|| fail("logged_out", "log in to play"))?;
    let kind = input
        .message
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| fail("bad_input", "send a guess"))?;
    // Saying the death has been shown is not play, so it is the one message a suspended
    // Character may send: their loss is what suspended them.
    if kind == "seen" {
        let day = initialize_daily_word(input.chitt.created_at.timestamp())?;
        return acknowledge_loss(&observer, day);
    }
    if observer.is_suspended {
        return Err(fail(
            "character_suspended",
            "you are suspended and cannot play Hurtle",
        ));
    }
    let day = initialize_daily_word(input.chitt.created_at.timestamp())?;
    if kind != "guess" {
        return Err(fail("bad_input", "send a guess"));
    }
    let word = input
        .message
        .get("word")
        .and_then(Value::as_str)
        .ok_or_else(|| fail("bad_input", "a guess must be a word"))?
        .trim()
        .to_ascii_lowercase();
    if !word.bytes().all(|byte| byte.is_ascii_lowercase()) {
        return Err(fail("bad_guess", "guesses must be letters only"));
    }
    let key = game_key(&observer.character_id);
    let (previous, mut game) = read_game_version(&observer.character_id)?
        .ok_or_else(|| fail("not_started", "start Hurtle before guessing"))?;
    if word.len() != game.answer.len() {
        return Err(fail(
            "bad_guess",
            format!("guesses must be {} letters", game.answer.len()),
        ));
    }
    if game.day != day {
        return Err(fail(
            "not_started",
            "today's word is ready; start a new game",
        ));
    }
    if game.complete {
        if game.won {
            announce_win(&input.chitt.id, &observer, &mut game)?;
        } else {
            announce_loss(&input.chitt.id, &observer, &game)?;
            if game.penalty_pending {
                apply_penalty(&observer.character_id, &mut game)?;
                write_game(&key, &game)?;
            }
        }
        return player_canvas_state(Some(&game), day, &observer);
    }
    // A repeated word or a syntactically valid word outside the dictionary consumes a turn and
    // health, but receives no score marks at all. Only fresh, listed words reveal letter feedback.
    let unscored = game.guesses.iter().any(|guess| guess.word == word) || !is_word(&word);
    let marks = if unscored {
        Vec::new()
    } else {
        score_guess(&game.answer, &word)
    };
    let won = !unscored && word == game.answer;
    game.guesses.push(Guess { word, marks });
    if won {
        game.complete = true;
        game.won = true;
        if game.guesses.len() == 1 {
            game.first_guess_badge = badges::snapshot()?;
        }
    } else {
        if game.guesses.len() == MAX_GUESSES {
            game.complete = true;
            game.penalty_pending = true;
        }
        game.wounds
            .push(wound_for(&observer, game.day, game.guesses.len()));
    }
    write_game_if_unchanged(&key, &previous, &game)?;

    if game.won {
        announce_win(&input.chitt.id, &observer, &mut game)?;
    } else if game.complete {
        announce_loss(&input.chitt.id, &observer, &game)?;
        if game.penalty_pending {
            apply_penalty(&observer.character_id, &mut game)?;
            write_game(&key, &game)?;
        }
    } else {
        record_damage(&observer, &game)?;
    }
    player_canvas_state(Some(&game), day, &observer)
}

fn acknowledge_loss(observer: &Observer, day: i64) -> Result<Map<String, Value>, HookError> {
    let key = game_key(&observer.character_id);
    for _ in 0..4 {
        let Some((previous, mut game)) = read_game_version(&observer.character_id)? else {
            return player_canvas_state(None, day, observer);
        };
        if game.day != day || !game.complete || game.won || game.loss_seen {
            return player_canvas_state(Some(&game).filter(|game| game.day == day), day, observer);
        }
        game.loss_seen = true;
        let bytes =
            serde_json::to_vec(&game).map_err(|error| fail("invalid_state", error.to_string()))?;
        if kv::compare_and_swap(&key, Some(&previous), &bytes)
            .map_err(|error| fail("storage_error", error.to_string()))?
        {
            return player_canvas_state(Some(&game), day, observer);
        }
    }
    Err(fail(
        "game_updated",
        "your game changed elsewhere; refresh and retry",
    ))
}

fn announce_win(chitt_id: &str, observer: &Observer, game: &mut Game) -> Result<(), HookError> {
    // Reward independently of the reply: a refused grant must not hide the win or its announcement,
    // and a refused reply must not prevent the reward. The saved game owns the award claim.
    record_damage(observer, game)?;
    let reward_was_refused = game
        .first_guess_badge
        .as_ref()
        .is_some_and(|reward| reward.status == badges::BadgeRewardStatus::Failed);
    badges::award(observer, game)?;
    if enter_roll(game.day, observer, false)? {
        push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()))?;
    }
    let key = win_announcement_key(&observer.character_id, game.day);
    let claimed = kv::set_if_absent(&key, b"pending", Some(DAILY_PENALTY_TTL))
        .map_err(|error| fail("storage_error", error.to_string()))?;
    if !claimed {
        let status = kv::get(&key).map_err(|error| fail("storage_error", error.to_string()))?;
        return match status.as_ref().map(|entry| entry.value.as_slice()) {
            Some(b"posted") => {
                let recovered = reward_was_refused
                    && game
                        .first_guess_badge
                        .as_ref()
                        .is_some_and(|reward| reward.status == badges::BadgeRewardStatus::Granted);
                if recovered {
                    announce_badge_recovery(chitt_id, observer, game.day)?;
                }
                Ok(())
            }
            Some(b"pending") => Ok(()),
            Some(b"failed") => Err(fail(
                "win_announcement_failed",
                "the winning reply could not be posted",
            )),
            _ => Err(fail(
                "invalid_state",
                "the winning reply status could not be confirmed",
            )),
        };
    }

    let result = win_reply(observer, game).and_then(|content| {
        poster::post_reply(chitt_id, &content)
            .map(|_| ())
            .map_err(poster_fail)
    });
    match result {
        Ok(()) => kv::set(&key, b"posted", Some(DAILY_PENALTY_TTL))
            .map_err(|error| fail("storage_error", error.to_string())),
        Err(error) => {
            kv::set(&key, b"failed", Some(DAILY_PENALTY_TTL))
                .map_err(|storage_error| fail("storage_error", storage_error.to_string()))?;
            Err(error)
        }
    }
}

fn badge_recovery_key(character_id: &str, day: i64) -> String {
    format!("badge-recovery:{character_id}:{day}")
}

// If an explicit refusal was corrected after the win reply, announce the recovered grant once.
// Transport-unknown outcomes remain Pending and never enter this path.
fn announce_badge_recovery(chitt_id: &str, observer: &Observer, day: i64) -> Result<(), HookError> {
    let key = badge_recovery_key(&observer.character_id, day);
    let mut claimed = false;
    for _ in 0..4 {
        let previous = kv::get(&key).map_err(|error| fail("storage_error", error.to_string()))?;
        match previous.as_ref().map(|entry| entry.value.as_slice()) {
            Some(b"posted" | b"pending") => return Ok(()),
            Some(b"failed") => {
                if kv::compare_and_swap(&key, Some(b"failed"), b"pending")
                    .map_err(|error| fail("storage_error", error.to_string()))?
                {
                    claimed = true;
                    break;
                }
            }
            _ => {
                if kv::set_if_absent(&key, b"pending", Some(DAILY_PENALTY_TTL))
                    .map_err(|error| fail("storage_error", error.to_string()))?
                {
                    claimed = true;
                    break;
                }
            }
        }
    }
    if !claimed {
        return Err(fail(
            "game_updated",
            "the badge announcement changed elsewhere; retry",
        ));
    }
    let result = poster::post_reply(
        chitt_id,
        "Your first-guess badge has now been awarded! Check your badges for your reward.",
    )
    .map(|_| ())
    .map_err(poster_fail);
    match result {
        Ok(()) => kv::set(&key, b"posted", Some(DAILY_PENALTY_TTL))
            .map_err(|error| fail("storage_error", error.to_string())),
        Err(error) => {
            kv::set(&key, b"failed", Some(DAILY_PENALTY_TTL))
                .map_err(|storage_error| fail("storage_error", storage_error.to_string()))?;
            Err(error)
        }
    }
}

fn announce_loss(chitt_id: &str, observer: &Observer, game: &Game) -> Result<(), HookError> {
    // The fatal guess leaves three splatters even if the suspension is refused.
    let bled = record_damage(observer, game)?;
    // One push covers the fresh stains and a fresh name on the obituary.
    if enter_roll(game.day, observer, true)? && !bled {
        push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()))?;
    }
    let key = loss_announcement_key(&observer.character_id, game.day);
    let claimed = kv::set_if_absent(&key, b"pending", Some(DAILY_PENALTY_TTL))
        .map_err(|error| fail("storage_error", error.to_string()))?;
    if !claimed {
        let status = kv::get(&key).map_err(|error| fail("storage_error", error.to_string()))?;
        return match status.as_ref().map(|entry| entry.value.as_slice()) {
            Some(b"posted" | b"pending") => Ok(()),
            Some(b"failed") => Err(fail(
                "loss_announcement_failed",
                "the failure reply could not be posted",
            )),
            _ => Err(fail(
                "invalid_state",
                "the failure reply status could not be confirmed",
            )),
        };
    }

    let result = failure_reply(observer, game).and_then(|content| {
        poster::post_reply(chitt_id, &content)
            .map(|_| ())
            .map_err(poster_fail)
    });
    match result {
        Ok(()) => kv::set(&key, b"posted", Some(DAILY_PENALTY_TTL))
            .map_err(|error| fail("storage_error", error.to_string())),
        Err(error) => {
            kv::set(&key, b"failed", Some(DAILY_PENALTY_TTL))
                .map_err(|storage_error| fail("storage_error", storage_error.to_string()))?;
            Err(error)
        }
    }
}

fn win_reply(observer: &Observer, game: &Game) -> Result<String, HookError> {
    let mention = character_mention(observer)?;
    let first_try = game.complete && game.won && game.guesses.len() == 1;
    let phrase = if first_try {
        FIRST_GUESS_WIN_MESSAGES[phrase_index(
            &observer.character_id,
            game.day,
            FIRST_GUESS_WIN_MESSAGES.len(),
        )]
    } else {
        WIN_MESSAGES[phrase_index(&observer.character_id, game.day, WIN_MESSAGES.len())]
    };
    let reward_notice =
        if first_try {
            match game.first_guess_badge.as_ref() {
                Some(reward) if reward.status == badges::BadgeRewardStatus::Granted => {
                    "\n\nCheck your badges for your reward!"
                }
                Some(reward) if reward.status == badges::BadgeRewardStatus::Failed => {
                    // The host explicitly refused the grant; show the real reason in the thread reply
                    // so a missing badge isn't a silent failure.
                    return Ok(format!(
                    "{mention}\n\n{phrase}\n\nThe badge could not be assigned: {}\n\n{}/{}\n\n{}",
                    reward.error.as_deref().unwrap_or("the host refused the grant"),
                    game.guesses.len(),
                    MAX_GUESSES,
                    guess_breakdown(game)
                ));
                }
                Some(reward) if reward.status == badges::BadgeRewardStatus::Pending => {
                    "\n\nYour reward is pending confirmation; please check your badges shortly."
                }
                _ => "\n\nNo first-guess badge is configured for today's word.",
            }
        } else {
            ""
        };
    Ok(format!(
        "{mention}\n\n{phrase}{reward_notice}\n\n{}/{}\n\n{}",
        game.guesses.len(),
        MAX_GUESSES,
        guess_breakdown(game)
    ))
}

fn failure_reply(observer: &Observer, game: &Game) -> Result<String, HookError> {
    let mention = character_mention(observer)?;
    let messages = if game.penalty_pending {
        EXECUTION_SUBMITTING_MESSAGES
    } else if game.penalty_applied {
        EXECUTED_MESSAGES
    } else if game
        .penalty_error
        .as_deref()
        .is_some_and(|error| error.contains("pending and could not be confirmed"))
    {
        EXECUTION_UNCONFIRMED_MESSAGES
    } else {
        EXECUTION_REFUSED_MESSAGES
    };
    let phrase = messages[phrase_index(&observer.character_id, game.day, messages.len())];
    Ok(format!(
        "{mention}\n\n{phrase}\n\n{}/{}\n\n{}",
        game.guesses.len(),
        MAX_GUESSES,
        guess_breakdown(game)
    ))
}

fn character_mention(observer: &Observer) -> Result<String, HookError> {
    if observer.trolltag.is_empty()
        || observer.trolltag.len() > MAX_CHARACTER_TAG_BYTES
        || !observer.trolltag.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_alphabetic()
            } else {
                byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
            }
        })
        || observer.discriminator.len() != 4
        || !observer
            .discriminator
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        return Err(fail("bad_observer", "your mention could not be formatted"));
    }
    let mention = if observer.discriminator == "0000" {
        format!("@{}", observer.trolltag)
    } else {
        format!("@{}#{}", observer.trolltag, observer.discriminator)
    };
    Ok(mention)
}

fn guess_breakdown(game: &Game) -> String {
    game.guesses
        .iter()
        .map(|guess| {
            if guess.marks.is_empty() {
                "— no clues —".to_string()
            } else {
                guess
                    .marks
                    .iter()
                    .map(|mark| match mark {
                        2 => "🟩",
                        1 => "🟨",
                        _ => "⬛",
                    })
                    .collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn phrase_index(character_id: &str, day: i64, length: usize) -> usize {
    let hash = character_id
        .bytes()
        .chain(day.to_le_bytes())
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    (hash % length as u64) as usize
}

fn apply_penalty(character_id: &str, game: &mut Game) -> Result<(), HookError> {
    let key = penalty_key(character_id, game.day);
    let claimed = kv::set_if_absent(&key, b"pending", Some(DAILY_PENALTY_TTL))
        .map_err(|error| fail("storage_error", error.to_string()))?;
    if !claimed {
        let status = kv::get(&key).map_err(|error| fail("storage_error", error.to_string()))?;
        match status.as_ref().map(|entry| entry.value.as_slice()) {
            Some(b"applied") => {
                game.penalty_applied = true;
                game.penalty_pending = false;
            }
            Some(b"failed") => {
                game.penalty_pending = false;
                if game.penalty_error.is_none() {
                    game.penalty_error =
                        Some("The suspension action previously failed.".to_string());
                }
            }
            _ => {
                game.penalty_pending = false;
                game.penalty_error = Some(
                    "The suspension action is pending and could not be confirmed.".to_string(),
                );
            }
        }
        return Ok(());
    }

    match poster::suspend_character(
        character_id,
        BanDuration::OneHour,
        "Hurtle: the player failed to guess the word in six tries.",
    ) {
        Ok(()) => {
            game.penalty_applied = true;
            game.penalty_pending = false;
            kv::set(&key, b"applied", None)
                .map_err(|error| fail("storage_error", error.to_string()))?;
        }
        Err(error) => {
            game.penalty_pending = false;
            game.penalty_error = Some(format!("The one-hour suspension was not applied: {error}"));
            kv::set(&key, b"failed", None)
                .map_err(|storage_error| fail("storage_error", storage_error.to_string()))?;
        }
    }
    Ok(())
}

fn read_game(character_id: &str) -> Result<Option<Game>, HookError> {
    read_game_version(character_id).map(|entry| entry.map(|(_, game)| game))
}

fn read_game_version(character_id: &str) -> Result<Option<(Vec<u8>, Game)>, HookError> {
    let Some(entry) = kv::get(&game_key(character_id))
        .map_err(|error| fail("storage_error", error.to_string()))?
    else {
        return Ok(None);
    };
    serde_json::from_slice(&entry.value)
        .map(|game| Some((entry.value, game)))
        .map_err(|error| fail("invalid_state", error.to_string()))
}

fn write_game(key: &str, game: &Game) -> Result<(), HookError> {
    let bytes =
        serde_json::to_vec(game).map_err(|error| fail("invalid_state", error.to_string()))?;
    kv::set(key, &bytes, None).map_err(|error| fail("storage_error", error.to_string()))
}

fn write_game_if_unchanged(key: &str, previous: &[u8], game: &Game) -> Result<(), HookError> {
    let bytes =
        serde_json::to_vec(game).map_err(|error| fail("invalid_state", error.to_string()))?;
    let swapped = kv::compare_and_swap(key, Some(previous), &bytes)
        .map_err(|error| fail("storage_error", error.to_string()))?;
    if swapped {
        Ok(())
    } else {
        Err(fail(
            "game_updated",
            "the game changed elsewhere; refresh the card before guessing again",
        ))
    }
}

fn game_key(character_id: &str) -> String {
    format!("game:{character_id}")
}

fn penalty_key(character_id: &str, day: i64) -> String {
    format!("penalty:{character_id}:{day}")
}

fn win_announcement_key(character_id: &str, day: i64) -> String {
    format!("win:{character_id}:{day}")
}

fn loss_announcement_key(character_id: &str, day: i64) -> String {
    format!("loss:{character_id}:{day}")
}

fn initialize_daily_word(created_at: i64) -> Result<i64, HookError> {
    if reset_day()?.is_some() {
        return Err(fail(
            "reset_in_progress",
            "Hurtle is clearing its data; the next word will be ready shortly",
        ));
    }
    let day = created_at.div_euclid(DAY_SECONDS as i64);
    let initialized = kv::set_if_absent(DAILY_DAY_KEY, day.to_string().as_bytes(), None)
        .map_err(|error| fail("storage_error", error.to_string()))?;
    let current = if initialized { day } else { current_day()? };
    let twelve_from = if initialized { current } else { current + 1 };
    kv::set_if_absent(TWELVE_FROM_KEY, twelve_from.to_string().as_bytes(), None)
        .map_err(|error| fail("storage_error", error.to_string()))?;
    if kv::get(DAILY_TIMER_READY_KEY)
        .map_err(|error| fail("storage_error", error.to_string()))?
        .is_none()
    {
        schedule_daily_word(current)?;
    }
    Ok(current)
}

fn answer_for(day: i64, twelve_from: i64) -> &'static str {
    let list = if day < twelve_from {
        LEGACY_ANSWERS
    } else {
        ANSWERS
    };
    let answers: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .collect();
    answers[day.rem_euclid(answers.len() as i64) as usize]
}

fn daily_answer(day: i64) -> Result<String, HookError> {
    let twelve_from =
        match kv::get(TWELVE_FROM_KEY).map_err(|error| fail("storage_error", error.to_string()))? {
            Some(entry) => std::str::from_utf8(&entry.value)
                .ok()
                .and_then(|value| value.parse().ok())
                .ok_or_else(|| fail("invalid_state", "the word length switch is invalid"))?,
            None => day,
        };
    Ok(answer_for(day, twelve_from).to_string())
}

fn current_day() -> Result<i64, HookError> {
    let entry = kv::get(DAILY_DAY_KEY)
        .map_err(|error| fail("storage_error", error.to_string()))?
        .ok_or_else(|| {
            fail(
                "daily_word_unavailable",
                "the daily word is not initialized",
            )
        })?;
    std::str::from_utf8(&entry.value)
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| fail("invalid_state", "the daily word date is invalid"))
}

fn schedule_daily_word(day: i64) -> Result<(), HookError> {
    timer::schedule_timer(DAILY_TIMER, DAY_SECONDS, json!({ "day": day + 1 }))
        .map_err(|error| fail("timer_error", error.to_string()))?;
    kv::set(DAILY_TIMER_READY_KEY, day.to_string().as_bytes(), None)
        .map_err(|error| fail("storage_error", error.to_string()))
}

fn rotate_daily_word(payload: &Option<Map<String, Value>>) -> Result<(), HookError> {
    let next_day = payload
        .as_ref()
        .and_then(|payload| payload.get("day"))
        .and_then(Value::as_i64)
        .ok_or_else(|| fail("bad_timer", "the daily word timer has no next day"))?;
    let current = current_day()?;
    if next_day < current {
        return Ok(());
    }
    if next_day == current {
        let ready = kv::get(DAILY_TIMER_READY_KEY)
            .map_err(|error| fail("storage_error", error.to_string()))?;
        if ready.is_some_and(|entry| entry.value == current.to_string().as_bytes()) {
            // Replaying the old timer after an admin reset must not postpone the new deadline.
            return Ok(());
        }
    }
    let day = advance_day(next_day)?;
    schedule_daily_word(day)?;
    push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()))
}

fn advance_day(next_day: i64) -> Result<i64, HookError> {
    for _ in 0..3 {
        let entry = kv::get(DAILY_DAY_KEY)
            .map_err(|error| fail("storage_error", error.to_string()))?
            .ok_or_else(|| {
                fail(
                    "daily_word_unavailable",
                    "the daily word is not initialized",
                )
            })?;
        let current = std::str::from_utf8(&entry.value)
            .ok()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| fail("invalid_state", "the daily word date is invalid"))?;
        if current >= next_day {
            return Ok(current);
        }
        if kv::compare_and_swap(
            DAILY_DAY_KEY,
            Some(&entry.value),
            next_day.to_string().as_bytes(),
        )
        .map_err(|error| fail("storage_error", error.to_string()))?
        {
            return Ok(next_day);
        }
    }
    Err(fail(
        "daily_word_updated",
        "the daily word changed concurrently; retry the timer",
    ))
}

fn blood_key(day: i64) -> String {
    format!("blood:{day}")
}

fn read_blood(day: i64) -> Result<BloodLedger, HookError> {
    match kv::get(&blood_key(day)).map_err(|error| fail("storage_error", error.to_string()))? {
        Some(entry) => serde_json::from_slice(&entry.value)
            .map_err(|error| fail("invalid_state", error.to_string())),
        None => Ok(BloodLedger::default()),
    }
}

fn wound_for(observer: &Observer, day: i64, guess: usize) -> Wound {
    let fatal = guess == MAX_GUESSES;
    Wound {
        guess,
        stains: (0..if fatal { 3 } else { 1 })
            .map(|splat| Bloodstain {
                color: profile_color(&observer.text_color),
                // Keep the first death seed compatible with the roll's legacy colour backfill.
                seed: if fatal && splat == 0 {
                    stain_seed(&observer.character_id, day)
                } else {
                    stain_seed(
                        &format!("{}:hit:{guess}:{splat}", observer.character_id),
                        day,
                    )
                },
            })
            .collect(),
    }
}

// Reconcile saved hits, not the incoming word: retries and later guesses recover a partially
// completed write without repeating damage. Only deaths enter `victims` and the obituary.
fn record_damage(observer: &Observer, game: &Game) -> Result<bool, HookError> {
    if game.wounds.is_empty() && !(game.complete && !game.won) {
        return Ok(false);
    }
    let key = blood_key(game.day);
    for _ in 0..8 {
        let previous = kv::get(&key).map_err(|error| fail("storage_error", error.to_string()))?;
        let mut blood: BloodLedger = previous.as_ref().map_or_else(
            || Ok(BloodLedger::default()),
            |entry| {
                serde_json::from_slice(&entry.value)
                    .map_err(|error| fail("invalid_state", error.to_string()))
            },
        )?;
        let mut total = blood.splatter_count();
        let mut changed = false;
        for wound in &game.wounds {
            if blood
                .hits
                .insert(format!("{}:{}", observer.character_id, wound.guess))
            {
                total += wound.stains.len() as u64;
                blood.recent.extend(wound.stains.iter().cloned());
                changed = true;
            }
        }
        if game.complete && !game.won {
            changed |= blood.victims.insert(observer.character_id.clone());
        }
        if !changed {
            return Ok(false);
        }
        blood.total = Some(total);
        let excess = blood.recent.len().saturating_sub(MAX_VISIBLE_STAINS);
        blood.recent.drain(..excess);
        let bytes =
            serde_json::to_vec(&blood).map_err(|error| fail("invalid_state", error.to_string()))?;
        let stored = match previous.as_ref() {
            Some(entry) => kv::compare_and_swap(&key, Some(&entry.value), &bytes),
            None => kv::set_if_absent(&key, &bytes, Some(DAILY_PENALTY_TTL)),
        }
        .map_err(|error| fail("storage_error", error.to_string()))?;
        if stored {
            push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()))?;
            return Ok(true);
        }
    }
    Err(fail(
        "game_updated",
        "the chamber changed elsewhere; refresh and retry",
    ))
}

fn roll_key(day: i64) -> String {
    format!("roll:{day}")
}

fn read_roll(day: i64) -> Result<(Option<Vec<u8>>, DailyRoll), HookError> {
    match kv::get(&roll_key(day)).map_err(|error| fail("storage_error", error.to_string()))? {
        Some(entry) => {
            let roll = serde_json::from_slice(&entry.value)
                .map_err(|error| fail("invalid_state", error.to_string()))?;
            Ok((Some(entry.value), roll))
        }
        None => Ok((None, DailyRoll::default())),
    }
}

// Applies `change` to the day's roll atomically, retrying if another Hook wrote first. `change`
// says whether it changed anything, so an unchanged roll costs no write.
fn update_roll(
    day: i64,
    change: impl Fn(&mut DailyRoll) -> bool,
) -> Result<(bool, DailyRoll), HookError> {
    let key = roll_key(day);
    for _ in 0..8 {
        let (previous, mut roll) = read_roll(day)?;
        if !change(&mut roll) {
            return Ok((false, roll));
        }
        let bytes =
            serde_json::to_vec(&roll).map_err(|error| fail("invalid_state", error.to_string()))?;
        let stored = match previous.as_ref() {
            Some(previous) => kv::compare_and_swap(&key, Some(previous), &bytes),
            None => kv::set_if_absent(&key, &bytes, Some(DAILY_PENALTY_TTL)),
        }
        .map_err(|error| fail("storage_error", error.to_string()))?;
        if stored {
            return Ok((true, roll));
        }
    }
    Err(fail(
        "game_updated",
        "the roll changed elsewhere; refresh and retry",
    ))
}

// Adds the entry, or brings an existing one up to date: the mention follows a renamed player, but
// the colour stays the one they finished in, as their blood does. Returns whether anything changed.
fn upsert(list: &mut Vec<RollEntry>, entry: RollEntry) -> bool {
    let Some(existing) = list.iter_mut().find(|existing| existing.id == entry.id) else {
        list.push(entry);
        return true;
    };
    let before = existing.clone();
    if entry.mention.is_some() {
        existing.mention = entry.mention;
    }
    if existing.color.is_none() {
        existing.color = entry.color;
    }
    *existing != before
}

fn roll_entry(observer: &Observer) -> RollEntry {
    RollEntry {
        id: observer.character_id.clone(),
        mention: character_mention(observer).ok(),
        color: Some(profile_color(&observer.text_color)),
    }
}

// Returns whether the roll changed, so the caller can push the card to everyone.
fn enter_roll(day: i64, observer: &Observer, fell: bool) -> Result<bool, HookError> {
    let entry = roll_entry(observer);
    let (changed, _) = update_roll(day, |roll| {
        upsert(
            if fell {
                &mut roll.fallen
            } else {
                &mut roll.survivors
            },
            entry.clone(),
        )
    })?;
    Ok(changed)
}

// A player on the roll without a name (rebuilt by `migrate`) gets one the next time they view the
// card, and a changed trolltag or colour follows them. Anyone else costs a single read.
fn learn_roll_name(day: i64, observer: &Observer) -> Result<DailyRoll, HookError> {
    let (_, roll) = read_roll(day)?;
    let entry = roll_entry(observer);
    let stale = |list: &[RollEntry]| {
        list.iter().any(|existing| {
            existing.id == entry.id
                && ((entry.mention.is_some() && existing.mention != entry.mention)
                    || existing.color.is_none())
        })
    };
    if !stale(&roll.fallen) && !stale(&roll.survivors) {
        return Ok(roll);
    }
    let (_, roll) = update_roll(day, |roll| {
        let mut changed = false;
        for list in [&mut roll.fallen, &mut roll.survivors] {
            if list.iter().any(|existing| existing.id == entry.id) {
                changed |= upsert(list, entry.clone());
            }
        }
        changed
    })?;
    Ok(roll)
}

// The newest named entries first, as the card shows them; IDs never leave the server. `fallback`
// colours an entry whose colour is not known yet.
fn roll_names(list: &[RollEntry], fallback: u32) -> Vec<Value> {
    list.iter()
        .rev()
        .filter_map(|entry| {
            let mention = entry.mention.as_ref()?;
            let color = entry.color.unwrap_or(fallback);
            Some(json!({ "mention": mention, "color": format!("#{:06x}", color >> 8) }))
        })
        .take(ROLL_SHOWN)
        .collect()
}

// Players who finished today before the roll existed: the fallen from the blood ledger, with the
// colour their stain was left in, and the survivors from their win announcements.
fn backfill_roll(day: i64) -> Result<(), HookError> {
    let blood = read_blood(day)?;
    let suffix = format!(":{day}");
    let winners: Vec<String> = kv::list("win:")
        .map_err(|error| fail("storage_error", error.to_string()))?
        .keys
        .iter()
        .filter_map(|key| key.strip_prefix("win:")?.strip_suffix(&suffix))
        .map(str::to_string)
        .collect();
    update_roll(day, |roll| {
        let mut changed = false;
        for id in &blood.victims {
            if !roll.fallen.iter().any(|entry| entry.id == *id) {
                let seed = stain_seed(id, day);
                let color = blood
                    .recent
                    .iter()
                    .find(|stain| stain.seed == seed)
                    .map(|stain| stain.color);
                roll.fallen.push(RollEntry {
                    id: id.clone(),
                    mention: None,
                    color,
                });
                changed = true;
            }
        }
        for id in &winners {
            if !roll.survivors.iter().any(|entry| entry.id == *id) {
                roll.survivors.push(RollEntry {
                    id: id.clone(),
                    mention: None,
                    color: None,
                });
                changed = true;
            }
        }
        changed
    })?;
    Ok(())
}

// Never forward profile strings into styles. Accept RGB hex, force opaque alpha and use a
// blood-red fallback for missing or malformed colours (including CSS functions and markup).
fn profile_color(text: &str) -> u32 {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return DEFAULT_BLOOD_COLOR;
    }
    let expanded = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return DEFAULT_BLOOD_COLOR,
    };
    u32::from_str_radix(&expanded, 16)
        .map(|rgb| (rgb << 8) | 0xff)
        .unwrap_or(DEFAULT_BLOOD_COLOR)
}

fn stain_seed(character_id: &str, day: i64) -> u32 {
    phrase_index(character_id, day, u32::MAX as usize) as u32
}

// The state one player's canvas sees: the shared chamber, their board, whether they are suspended,
// and, until a canvas has shown it, their unseen death and the seeds of its three splatters.
fn player_canvas_state(
    game: Option<&Game>,
    day: i64,
    observer: &Observer,
) -> Result<Map<String, Value>, HookError> {
    let mut state = shared_canvas_state(game, day)?;
    state.insert("suspended".to_string(), json!(observer.is_suspended));
    if state["freshLoss"] == true {
        let seeds: Vec<u32> = game
            .and_then(|game| game.wounds.iter().find(|wound| wound.guess == MAX_GUESSES))
            .map(|wound| wound.stains.iter().map(|stain| stain.seed).collect())
            .unwrap_or_else(|| vec![stain_seed(&observer.character_id, day)]);
        state.insert("ownStain".to_string(), json!(seeds.first()));
        state.insert("ownStains".to_string(), json!(seeds));
    }
    Ok(state)
}

fn shared_canvas_state(game: Option<&Game>, day: i64) -> Result<Map<String, Value>, HookError> {
    let blood = read_blood(day)?;
    let mut state = canvas_state(game);
    if game.is_none() {
        state.insert("length".to_string(), json!(daily_answer(day)?.len()));
    }
    state.insert("round".to_string(), json!(day));
    state.insert(
        "blood".to_string(),
        json!({
            "total": blood.splatter_count(),
            "fallen": blood.victims.len(),
            "stains": blood.recent,
        }),
    );
    Ok(state)
}

fn template_stains(state: &Map<String, Value>) -> Vec<Value> {
    state["blood"]["stains"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|stain| {
            let color = stain["color"]
                .as_u64()
                .unwrap_or(u64::from(DEFAULT_BLOOD_COLOR)) as u32;
            let seed = stain["seed"].as_u64().unwrap_or(0);
            json!({
                "color": format!("#{:06x}", color >> 8),
                "left": seed % 100,
                "size": 18 + seed % 29,
                "drip": 14 + (seed / 100) % 54,
            })
        })
        .collect()
}

fn canvas_state(game: Option<&Game>) -> Map<String, Value> {
    let guesses = game.map_or(&[][..], |game| game.guesses.as_slice());
    object(json!({
        "started": game.is_some(),
        "health": game.map(Game::health),
        "maxHealth": MAX_GUESSES,
        "guesses": guesses,
        "complete": game.is_some_and(|game| game.complete),
        "won": game.is_some_and(|game| game.won),
        "answer": game.filter(|game| game.won).map(|game| &game.answer),
        "penaltyPending": game.is_some_and(|game| game.penalty_pending),
        "penaltyApplied": game.is_some_and(|game| game.penalty_applied),
        "penaltyError": game.and_then(|game| game.penalty_error.as_deref()),
        "freshLoss": game.is_some_and(|game| game.complete && !game.won && !game.loss_seen),
        "length": game.map(|game| game.answer.len()),
    }))
}

// The lists are fixed-width records, so a lookup is a binary search over the embedded bytes
// rather than a parse of 150 KB on every guess.
fn is_word(word: &str) -> bool {
    let list = match word.len() {
        12 => GUESSES_12,
        5 => GUESSES_5,
        _ => return false,
    }
    .as_bytes();
    let width = word.len() + 1;
    let (mut low, mut high) = (0, list.len() / width);
    while low < high {
        let middle = (low + high) / 2;
        let entry = &list[middle * width..middle * width + word.len()];
        match entry.cmp(word.as_bytes()) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

fn score_guess(answer: &str, guess: &str) -> Vec<u8> {
    let answer = answer.as_bytes();
    let guess = guess.as_bytes();
    let mut marks = vec![0; answer.len()];
    let mut remaining = [0u8; 26];
    for index in 0..answer.len() {
        if guess[index] == answer[index] {
            marks[index] = 2;
        } else {
            remaining[(answer[index] - b'a') as usize] += 1;
        }
    }
    for index in 0..answer.len() {
        if marks[index] == 0 {
            let letter = (guess[index] - b'a') as usize;
            if remaining[letter] > 0 {
                marks[index] = 1;
                remaining[letter] -= 1;
            }
        }
    }
    marks
}

fn object(value: Value) -> Map<String, Value> {
    value
        .as_object()
        .cloned()
        .expect("Hurtle's generated JSON values are objects")
}

fn poster_fail(error: impl std::fmt::Display) -> HookError {
    fail("poster_action_failed", error.to_string())
}

fn fail(code: &str, message: impl Into<String>) -> HookError {
    HookError {
        code: code.to_string(),
        message: message.into(),
    }
}

fn render_error(error: HookError, signed_in: bool) -> RenderOutput {
    RenderOutput {
        template: "card".to_string(),
        data: object(
            json!({ "signedIn": signed_in, "started": false, "complete": false, "won": false }),
        ),
        canvas: Some(canvas_state(None)),
        error: Some(error),
    }
}

#[cfg(test)]
mod logic_tests {
    use super::*;

    #[test]
    fn repeated_letters_are_scored_without_overcounting() {
        assert_eq!(score_guess("apple", "allee"), [2, 1, 0, 0, 2]);
    }

    #[test]
    fn guess_lists_are_sorted_fixed_width_and_hold_every_answer() {
        for (list, width) in [(GUESSES_12, 12), (GUESSES_5, 5)] {
            let words: Vec<&str> = list.lines().collect();
            assert_eq!(
                list.len(),
                words.len() * (width + 1),
                "one word to a line, newline-ended"
            );
            assert!(words
                .iter()
                .all(|word| word.len() == width && word.bytes().all(|b| b.is_ascii_lowercase())));
            assert!(
                words.windows(2).all(|pair| pair[0] < pair[1]),
                "sorted and unique"
            );
        }
        for answer in ANSWERS.lines().chain(LEGACY_ANSWERS.lines()) {
            let answer = answer.trim();
            if !answer.is_empty() {
                assert!(is_word(answer), "{answer} is an answer but not a guess");
            }
        }
        assert!(is_word("abbreviation"));
        assert!(!is_word("qqqqqqqqqqqq"));
        assert!(!is_word("aaaaa"));
        assert!(is_word("crane"));
        // The first and last records are reachable.
        assert!(is_word(GUESSES_12.lines().next().unwrap()));
        assert!(is_word(GUESSES_12.lines().last().unwrap()));
        assert!(is_word(GUESSES_5.lines().last().unwrap()));
    }

    #[test]
    fn consecutive_days_have_different_answers() {
        for list in [ANSWERS, LEGACY_ANSWERS] {
            let count = list.lines().filter(|word| !word.trim().is_empty()).count() as i64;
            let twelve_from = if list == ANSWERS { i64::MIN } else { i64::MAX };
            for day in 0..count {
                assert_ne!(
                    answer_for(day, twelve_from),
                    answer_for(day + 1, twelve_from)
                );
            }
        }
    }

    #[test]
    fn the_lists_switch_from_five_to_twelve_letters_on_the_cutover_day() {
        assert!(ANSWERS
            .lines()
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .all(|word| word.len() == WORD_LENGTH && word.bytes().all(|b| b.is_ascii_lowercase())));
        assert!(LEGACY_ANSWERS
            .lines()
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .all(|word| word.len() == 5));
        assert_eq!(answer_for(20_400, 20_401).len(), 5);
        assert_eq!(answer_for(20_401, 20_401).len(), 12);
    }

    #[test]
    fn canvas_state_reveals_the_answer_only_after_a_win() {
        let mut game = Game::new(1, "crane".to_string());
        game.complete = true;
        assert!(canvas_state(Some(&game))["answer"].is_null());

        game.won = true;
        assert_eq!(canvas_state(Some(&game))["answer"], game.answer);
    }
}
