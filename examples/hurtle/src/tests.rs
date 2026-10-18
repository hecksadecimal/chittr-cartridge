mod badges;
mod health;
mod reset;

use chittr_cartridge::testing::{self, HookClass, TestHost};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::json;

use crate::pdk::types::*;
use crate::{
    current_day, migrate, on_canvas_message, on_control, on_timer, read_game, render, Game, Guess,
    DAILY_TIMER, DAY_SECONDS,
};

fn wire<T: DeserializeOwned>(input: impl Serialize) -> T {
    serde_json::from_value(serde_json::to_value(input).expect("input serializes"))
        .expect("input matches the generated ABI type")
}

fn setup() -> (TestHost, testing::ChittRef) {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Lifecycle);
    let output = crate::init(wire(testing::init(&chitt))).expect("init returns");
    assert!(output.error.is_none());
    (host, chitt)
}

#[test]
fn player_facing_copy_addresses_you_without_character_terminology() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Lifecycle);
    let content = crate::init(wire(testing::init(&chitt))).unwrap().content;
    let template = include_str!("../cartridge/templates/card.liquid");
    assert!(!content.to_lowercase().contains("character"));
    assert!(!template.to_lowercase().contains("character"));
    assert!(content.contains("you will be suspended"));
    assert!(template.contains("you will be suspended"));
    let mention_error = crate::character_mention(&wire(testing::observer("player-1"))).unwrap_err();
    assert_eq!(mention_error.message, "your mention could not be formatted");
}

#[test]
fn start_button_has_an_explicit_high_contrast_neutral_palette() {
    let template = include_str!("../cartridge/templates/card.liquid");
    let button = template
        .split("<button ")
        .nth(1)
        .unwrap()
        .split("</button>")
        .next()
        .unwrap();
    assert!(button.contains("data-lc-control=\"start\""));
    assert!(button.contains("background-color: #303030; color: #f5f5f5;"));
    assert!(button.contains("I accept the risk — start Hurtle"));
}

// The host draws its own controls in its theme's ink, so they sit on the host's surface; only the
// chamber is dark, with a solid colour under its gradient and its own ink on every line of text.
#[test]
fn the_card_reads_on_light_and_dark_themes() {
    let template = include_str!("../cartridge/templates/card.liquid");
    let outer = template.lines().next().unwrap();
    assert!(!outer.contains("background") && !outer.contains("color: #"));
    let chamber_start = template.find("background-color: #1b1b21").unwrap();
    let chamber_end = template.find("<lc-reply>").unwrap();
    let chamber = &template[chamber_start..chamber_end];
    assert!(chamber.contains("color: #eee3cf; color-scheme: dark"));
    for host_element in ["<lc-author>", "<lc-menu>"] {
        assert!(template.find(host_element).unwrap() < chamber_start);
    }
    assert!(template.find("<lc-reactions>").unwrap() > chamber_end);
    for text in ["<h2 ", "<p ", "<strong "] {
        for tag in chamber.split(text).skip(1) {
            let style = tag.split('>').next().unwrap();
            assert!(
                style.contains("color: #"),
                "{text} without its own ink: {style}"
            );
        }
    }
}

#[test]
fn render_initializes_the_daily_word_when_replacing_an_older_version() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Render);

    let output = render(wire(testing::render(
        &chitt,
        Some(testing::observer("player-1")),
    )))
    .expect("render returns");

    assert!(output.error.is_none());
    assert_eq!(output.data["started"], false);
    assert!(!output.data.contains_key("currentWord"));
    assert!(host.kv_text("daily-day").is_some());
    let timers = host.timers();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].name, DAILY_TIMER);
    assert_eq!(timers[0].due_at, host.now() + DAY_SECONDS);
}

#[test]
fn a_suspended_character_cannot_start_or_submit_guesses() {
    let (host, chitt) = setup();
    let mut observer = testing::observer("player-1");
    observer.is_suspended = true;

    host.begin_hook(HookClass::Control);
    let start = on_control(wire(testing::control(
        &chitt,
        Some(observer.clone()),
        "start",
        "start",
    )))
    .expect("the control Hook returns an error");
    let start_error = start.error.expect("suspended player is blocked");
    assert_eq!(start_error.code, "character_suspended");
    assert_eq!(
        start_error.message,
        "you are suspended and cannot play Hurtle"
    );
    assert!(read_game("player-1").expect("game state reads").is_none());

    start_game(&host, &chitt);
    let output = guess_as(&host, &chitt, observer, "crane");
    let guess_error = output.error.expect("suspended player is blocked");
    assert_eq!(guess_error.code, "character_suspended");
    assert_eq!(
        guess_error.message,
        "you are suspended and cannot play Hurtle"
    );
    assert!(read_game("player-1")
        .expect("game state reads")
        .expect("started game remains")
        .guesses
        .is_empty());
}

#[test]
fn suspended_character_still_sees_their_board_marked_suspended() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    assert!(guess(&host, &chitt, &wrong_word()).error.is_none());
    let mut observer = testing::observer("player-1");
    observer.is_suspended = true;

    host.begin_hook(HookClass::Render);
    let output = render(wire(testing::render(&chitt, Some(observer))))
        .expect("render returns for suspended Character");

    assert!(output.error.is_none());
    assert_eq!(output.data["isSuspended"], true);
    let state = output.canvas.expect("canvas state");
    assert_eq!(state["suspended"], true);
    assert_eq!(state["started"], true);
    assert_eq!(state["guesses"].as_array().map(Vec::len), Some(1));

    host.begin_hook(HookClass::Render);
    let player = render(wire(testing::render(
        &chitt,
        Some(testing::observer("player-1")),
    )))
    .unwrap();
    assert_eq!(player.canvas.unwrap()["suspended"], false);
}

#[test]
fn migration_refreshes_the_stored_description_with_the_same_text_as_init() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Lifecycle);
    let initial = crate::init(wire(testing::init(&chitt))).unwrap();
    assert!(initial.error.is_none());
    assert!(initial.content.contains("twelve-letter word"));
    assert!(host.actions().is_empty(), "init returns content directly");

    for _ in 0..2 {
        upgrade_live_chitt_again(&host, &chitt);
        let actions = host.actions();
        let edit = actions.last().unwrap();
        assert_eq!(edit.action, "edit_chitt");
        assert_eq!(edit.args["content"], initial.content);
        assert_eq!(edit.args["quirks"], false);
        assert!(edit.args.get("postId").is_none(), "edits this Live Chitt");
        assert!(edit.args.get("ooc").is_none(), "preserves the OOC setting");
        assert_eq!(
            edit.reply["deferred"], true,
            "edit waits for the swap to commit"
        );
    }
    assert_eq!(host.actions().len(), 2, "one edit per successful migration");
}

#[test]
fn a_failed_state_migration_does_not_queue_a_description_edit() {
    let (host, chitt) = setup();
    host.put_kv(crate::DAILY_DAY_KEY, b"invalid date", None)
        .unwrap();
    host.begin_hook(HookClass::Lifecycle);
    let output = migrate(wire(testing::migrate(
        &chitt,
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.6".to_string(),
            sha256: "old".to_string(),
        },
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.7".to_string(),
            sha256: "new".to_string(),
        },
    )))
    .unwrap();
    assert_eq!(output.error.unwrap().code, "invalid_state");
    assert!(host.actions().is_empty());
}

#[test]
fn a_new_version_keeps_the_word_attempts_blood_and_timer() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    for word in six_wrong_guesses() {
        assert!(guess(&host, &chitt, &word).error.is_none());
    }
    let day = current_day().expect("daily word is initialized");
    let timers = host.timers();
    let before = read_game("player-1").unwrap().unwrap();

    host.begin_hook(HookClass::Lifecycle);
    let input = testing::migrate(
        &chitt,
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.1.0".to_string(),
            sha256: "old".to_string(),
        },
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.1.1".to_string(),
            sha256: "new".to_string(),
        },
    );
    let output = migrate(wire(input)).expect("migration returns");
    assert!(output.error.is_none());
    let actions = host.actions();
    let edit = actions.last().unwrap();
    assert_eq!(edit.action, "edit_chitt");
    assert_eq!(edit.args["content"], crate::CHITT_CONTENT);

    assert_eq!(current_day().unwrap(), day);
    let after = read_game("player-1").unwrap().unwrap();
    assert_eq!(after.day, day);
    assert_eq!(after.answer, before.answer);
    assert_eq!(
        serde_json::to_value(&after.guesses).unwrap(),
        serde_json::to_value(&before.guesses).unwrap()
    );
    assert!(after.complete);
    let timers_after = host.timers();
    assert_eq!(timers_after.len(), timers.len());
    assert_eq!(timers_after[0].due_at, timers[0].due_at);
    assert_eq!(timers_after[0].payload, timers[0].payload);

    // The player's one attempt is spent, and the stain they left is still on the wall.
    start_game(&host, &chitt);
    let restarted = read_game("player-1").unwrap().unwrap();
    assert!(restarted.complete);
    assert_eq!(restarted.guesses.len(), 6);
    host.begin_hook(HookClass::Render);
    let render = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(render.data["failureCount"], 1);
}

fn start_game(host: &TestHost, chitt: &testing::ChittRef) {
    host.begin_hook(HookClass::Control);
    let input = testing::control(chitt, Some(testing::observer("player-1")), "start", "start");
    let output = on_control(wire(input)).expect("the Hook returns");
    assert!(output.error.is_none());
}

fn guess(host: &TestHost, chitt: &testing::ChittRef, word: &str) -> CanvasMessageOutput {
    guess_as(host, chitt, player_observer(), word)
}

fn guess_as(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: chittr_cartridge::envelope::Observer,
    word: &str,
) -> CanvasMessageOutput {
    host.begin_hook(HookClass::Background);
    let input = testing::canvas_message(
        chitt,
        Some(observer),
        json!({ "type": "guess", "word": word }),
        Some(1),
    );
    on_canvas_message(wire(input)).expect("the Hook returns")
}

fn player_observer() -> chittr_cartridge::envelope::Observer {
    let mut observer = testing::observer("player-1");
    observer
        .profile
        .insert("trolltag".to_string(), json!("player"));
    observer
        .profile
        .insert("discriminator".to_string(), json!("1234"));
    observer
}

fn wrong_word() -> String {
    six_wrong_guesses().remove(0)
}

fn six_wrong_guesses() -> Vec<String> {
    let answer = crate::daily_answer(current_day().expect("daily word is initialized")).unwrap();
    crate::ANSWERS
        .lines()
        .map(str::to_string)
        .filter(|word| *word != answer)
        .take(6)
        .collect()
}

#[test]
fn losing_posts_the_result_before_applying_a_one_hour_suspension() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);

    let guesses = six_wrong_guesses();
    assert_eq!(guesses.len(), 6);
    for word in &guesses {
        let output = guess(&host, &chitt, word);
        assert!(output.error.is_none());
    }

    let actions = host.actions();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action, "post_reply");
    assert_eq!(actions[0].args["postId"], "chitt-1");
    assert_eq!(actions[1].action, "suspend_character");
    assert_eq!(actions[1].args["characterId"], "player-1");
    assert_eq!(actions[1].args["duration"], "1h");
    assert!(actions[1].args["reason"]
        .as_str()
        .unwrap()
        .contains("Hurtle"));
    let reply = actions[0].args["content"].as_str().unwrap();
    assert!(reply.starts_with("@player#1234\n\n"));
    assert!(reply.contains("6/6\n\n"));
    assert_eq!(reply.matches('\n').count(), 11);
    let submitting_phrase = crate::EXECUTION_SUBMITTING_MESSAGES[crate::phrase_index(
        "player-1",
        current_day().expect("daily word is initialized"),
        crate::EXECUTION_SUBMITTING_MESSAGES.len(),
    )];
    assert!(reply.contains(submitting_phrase));
    let game = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert!(game.complete);
    assert!(!game.won);
    assert!(game.penalty_applied);
}

#[test]
fn solving_the_word_does_not_suspend_the_player() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let answer = crate::daily_answer(current_day().expect("daily word is initialized")).unwrap();

    let output = guess_as(&host, &chitt, player_observer(), &answer);
    assert!(output.error.is_none());
    let actions = host.actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "post_reply");
    assert_eq!(actions[0].args["postId"], "chitt-1");
    assert_eq!(
        actions[0].args["content"],
        format!(
            "@player#1234\n\n{}\n\nNo first-guess badge is configured for today's word.\n\n1/6\n\n{}",
            crate::FIRST_GUESS_WIN_MESSAGES[crate::phrase_index(
                "player-1",
                current_day().unwrap(),
                crate::FIRST_GUESS_WIN_MESSAGES.len()
            )],
            ["🟩"; crate::WORD_LENGTH].concat()
        )
    );
    let repeated = guess_as(&host, &chitt, player_observer(), &answer);
    assert!(repeated.error.is_none());
    assert_eq!(
        host.actions()
            .iter()
            .filter(|action| action.action == "post_reply")
            .count(),
        1
    );
    let game = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert!(game.complete);
    assert!(game.won);
}

#[test]
fn a_winning_reply_shows_the_attempt_count_and_each_guess_pattern() {
    let mut game = Game::new(1, "crane".to_string());
    game.guesses = vec![
        Guess {
            word: "first".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "second".to_string(),
            marks: vec![0, 1, 0, 0, 0],
        },
        Guess {
            word: "third".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "fourth".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "fifth".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "winner".to_string(),
            marks: vec![2, 0, 0, 0, 1],
        },
    ];
    game.complete = true;
    game.won = true;

    let winner: Observer = wire(player_observer());
    let reply = crate::win_reply(&winner, &game).expect("reply formats");
    assert_eq!(
        reply,
        format!(
            "@player#1234\n\n{}\n\n6/6\n\n🟩⬛🟩⬛⬛\n⬛🟨⬛⬛⬛\n🟩⬛🟩⬛⬛\n🟩⬛🟩⬛⬛\n🟩⬛🟩⬛⬛\n🟩⬛⬛⬛🟨",
            crate::WIN_MESSAGES[crate::phrase_index("player-1", 1, crate::WIN_MESSAGES.len())]
        )
    );
}

#[test]
fn a_failure_reply_shows_execution_status_and_guess_patterns() {
    let mut game = Game::new(1, "crane".to_string());
    game.guesses = vec![
        Guess {
            word: "first".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "second".to_string(),
            marks: vec![0, 1, 0, 0, 0],
        },
        Guess {
            word: "third".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "fourth".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "fifth".to_string(),
            marks: vec![2, 0, 2, 0, 0],
        },
        Guess {
            word: "loser".to_string(),
            marks: vec![2, 0, 0, 0, 1],
        },
    ];
    game.complete = true;
    game.penalty_applied = true;

    let loser: Observer = wire(player_observer());
    let executed = crate::failure_reply(&loser, &game).expect("reply formats");
    game.penalty_applied = false;
    game.penalty_error = Some("The one-hour suspension was not applied: forbidden".to_string());
    let execution_refused = crate::failure_reply(&loser, &game).expect("reply formats");
    game.penalty_error =
        Some("The suspension action is pending and could not be confirmed.".to_string());
    let execution_unconfirmed = crate::failure_reply(&loser, &game).expect("reply formats");
    let board = "6/6\n\n🟩⬛🟩⬛⬛\n⬛🟨⬛⬛⬛\n🟩⬛🟩⬛⬛\n🟩⬛🟩⬛⬛\n🟩⬛🟩⬛⬛\n🟩⬛⬛⬛🟨";
    assert!(executed.starts_with("@player#1234\n\n"));
    assert!(executed.contains("6/6\n\n"));
    assert!(executed.contains(
        crate::EXECUTED_MESSAGES
            [crate::phrase_index("player-1", 1, crate::EXECUTED_MESSAGES.len())]
    ));
    assert!(execution_refused.contains("6/6\n\n"));
    assert!(execution_refused.contains(
        crate::EXECUTION_REFUSED_MESSAGES
            [crate::phrase_index("player-1", 1, crate::EXECUTION_REFUSED_MESSAGES.len())]
    ));
    assert!(execution_unconfirmed.contains(
        crate::EXECUTION_UNCONFIRMED_MESSAGES
            [crate::phrase_index("player-1", 1, crate::EXECUTION_UNCONFIRMED_MESSAGES.len())]
    ));
    assert!(executed.ends_with(board));
    assert!(execution_refused.ends_with(board));
    assert!(execution_unconfirmed.ends_with(board));

    for phrase in crate::WIN_MESSAGES
        .iter()
        .chain(crate::EXECUTED_MESSAGES)
        .chain(crate::EXECUTION_REFUSED_MESSAGES)
        .chain(crate::EXECUTION_UNCONFIRMED_MESSAGES)
        .chain(crate::EXECUTION_SUBMITTING_MESSAGES)
    {
        assert!(
            !phrase.to_ascii_lowercase().contains("suspension")
                && !phrase.to_ascii_lowercase().contains("suspend"),
            "reply phrase must use the Hurtle death theme, not suspension wording: {phrase}"
        );
    }
}

#[test]
fn the_daily_word_rotates_after_24_hours_and_players_can_start_again() {
    let (host, chitt) = setup();
    let day = current_day().expect("daily word is initialized");
    let answer = crate::daily_answer(day).unwrap();
    start_game(&host, &chitt);

    host.advance(DAY_SECONDS);
    let timer = host
        .fire_timer(&chitt, DAILY_TIMER)
        .expect("the daily timer is scheduled");
    host.begin_hook(HookClass::Background);
    let output = on_timer(wire(timer)).expect("the timer Hook returns");
    assert!(output.error.is_none());
    let timers = host.timers();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].due_at, host.now() + DAY_SECONDS);

    let next_day = current_day().expect("daily word advanced");
    assert_eq!(next_day, day + 1);
    assert_ne!(crate::daily_answer(next_day).unwrap(), answer);

    host.begin_hook(HookClass::Render);
    let output = render(wire(testing::render(
        &chitt,
        Some(testing::observer("player-1")),
    )))
    .expect("render returns");
    assert_eq!(output.data["started"], false);

    start_game(&host, &chitt);
    let game = read_game("player-1")
        .expect("game state reads")
        .expect("new daily game exists");
    assert_eq!(game.day, next_day);
    assert_eq!(game.answer, crate::daily_answer(next_day).unwrap());
}

#[test]
fn starting_again_does_not_grant_another_attempt_on_the_same_daily_word() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);

    let guesses = six_wrong_guesses();
    assert!(guess(&host, &chitt, &guesses[0]).error.is_none());
    start_game(&host, &chitt);
    let game = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert_eq!(game.guesses.len(), 1);
    assert_eq!(game.guesses[0].word, guesses[0]);

    for word in &guesses[1..] {
        assert!(guess(&host, &chitt, word).error.is_none());
    }
    let completed = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert!(completed.complete);
    assert_eq!(completed.guesses.len(), 6);

    start_game(&host, &chitt);
    let after_restart = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert!(after_restart.complete);
    assert_eq!(after_restart.guesses.len(), 6);
    let actions = host.actions();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action, "post_reply");
    assert_eq!(actions[1].action, "suspend_character");
    let reply = actions[0].args["content"].as_str().unwrap();
    assert!(reply.contains("6/6\n\n"));
    assert!(reply.contains(
        crate::EXECUTION_SUBMITTING_MESSAGES[crate::phrase_index(
            "player-1",
            current_day().unwrap(),
            crate::EXECUTION_SUBMITTING_MESSAGES.len()
        )]
    ));
}

#[test]
fn a_poster_without_suspension_permission_gets_an_explicit_failure_state() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    host.refuse_action("suspend_character", "forbidden", "permission denied");

    let guesses = six_wrong_guesses();
    for word in &guesses {
        let output = guess(&host, &chitt, word);
        assert!(output.error.is_none());
    }

    let game = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert!(!game.penalty_applied);
    assert!(!game.penalty_pending);
    assert!(game
        .penalty_error
        .as_deref()
        .unwrap()
        .contains("permission denied"));
    let actions = host.actions();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action, "post_reply");
    assert_eq!(actions[1].action, "suspend_character");
    let reply = actions[0].args["content"].as_str().unwrap();
    assert!(reply.contains("6/6\n\n"));
    assert!(reply.contains(
        crate::EXECUTION_SUBMITTING_MESSAGES[crate::phrase_index(
            "player-1",
            current_day().unwrap(),
            crate::EXECUTION_SUBMITTING_MESSAGES.len()
        )]
    ));
}

#[test]
fn only_real_words_are_accepted_and_scored_against_the_current_word() {
    let (host, chitt) = setup();
    assert_eq!(
        guess(&host, &chitt, "zzzzz").error.unwrap().code,
        "not_started"
    );
    start_game(&host, &chitt);
    assert_eq!(guess(&host, &chitt, "no").error.unwrap().code, "bad_guess");
    let answer = crate::daily_answer(current_day().expect("daily word is initialized")).unwrap();
    // Correctly formatted non-words consume turns and health, but never receive score marks.
    for nonsense in ["zzzzzzzzzzzz", "abbreviatiom"] {
        let reply = guess(&host, &chitt, nonsense).reply.unwrap();
        assert_eq!(
            reply["health"],
            if nonsense == "zzzzzzzzzzzz" { 5 } else { 4 }
        );
        assert_eq!(
            reply["guesses"].as_array().unwrap().last().unwrap()["marks"],
            json!([])
        );
        assert!(reply["answer"].is_null());
    }
    assert!(host.actions().is_empty());

    let word = wrong_word();
    let word = word.as_str();
    let output = guess(&host, &chitt, word);
    assert!(output.error.is_none());
    assert_eq!(
        output.reply.expect("guess returns updated canvas state")["guesses"][2]["marks"],
        json!(crate::score_guess(&answer, word))
    );
    let game = read_game("player-1")
        .expect("game state reads")
        .expect("game exists");
    assert_eq!(game.guesses.len(), 3);
    assert_eq!(game.guesses[2].word, word);
}

#[test]
fn wrong_guesses_and_deaths_leave_shared_profile_coloured_blood_without_duplicates() {
    let (host, chitt) = setup();
    host.refuse_action("suspend_character", "forbidden", "permission denied");
    let mut first = player_observer();
    first
        .profile
        .insert("textColor".to_string(), json!("#0088CC"));
    start_game(&host, &chitt);
    let words = six_wrong_guesses();
    for word in &words[..5] {
        assert!(guess_as(&host, &chitt, first.clone(), word).error.is_none());
    }
    assert!(crate::read_blood(current_day().unwrap())
        .unwrap()
        .victims
        .is_empty());
    let output = guess_as(&host, &chitt, first.clone(), &words[5]);
    assert!(output.error.is_none());
    let reply = output.reply.unwrap();
    assert_eq!(reply["blood"]["total"], 8);
    assert_eq!(reply["blood"]["fallen"], 1);
    assert_eq!(reply["blood"]["stains"][0]["color"], 0x0088ccffu32);
    assert_eq!(
        host.pushes(),
        vec![chittr_cartridge::push::PushTarget::All; 6]
    );

    // A retry (even with a changed profile) preserves the original colour and count.
    first
        .profile
        .insert("textColor".to_string(), json!("#abcdef"));
    let retried = guess_as(&host, &chitt, first, &words[5]).reply.unwrap();
    assert_eq!(reply["blood"], retried["blood"]);
    assert_eq!(host.pushes().len(), 6);

    let mut second = testing::observer("player-2");
    second
        .profile
        .insert("trolltag".to_string(), json!("anotherPlayer"));
    second
        .profile
        .insert("textColor".to_string(), json!("#a3c"));
    host.begin_hook(HookClass::Control);
    assert!(on_control(wire(testing::control(
        &chitt,
        Some(second.clone()),
        "start",
        "start"
    )))
    .unwrap()
    .error
    .is_none());
    for word in &words {
        assert!(guess_as(&host, &chitt, second.clone(), word)
            .error
            .is_none());
    }
    host.begin_hook(HookClass::Render);
    let public = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(public.data["failureCount"], 2);
    assert_eq!(public.data["bloodStains"][0]["color"], "#0088cc");
    assert_eq!(public.data["bloodStains"][8]["color"], "#aa33cc");
    let public_state = public.canvas.unwrap();
    assert_eq!(public_state["blood"]["total"], 16);
    assert_eq!(public_state["blood"]["fallen"], 2);
    assert_eq!(
        public_state["blood"]["stains"].as_array().unwrap().len(),
        16
    );
    assert_eq!(public_state["guesses"], json!([]));
    let encoded = serde_json::to_string(&public_state).unwrap();
    assert!(!encoded.contains("player-1") && !encoded.contains("player-2"));
    assert!(!encoded.contains(&crate::daily_answer(current_day().unwrap()).unwrap()));

    let mut suspended = testing::observer("player-1");
    suspended.is_suspended = true;
    host.begin_hook(HookClass::Render);
    let suspended_render = render(wire(testing::render(&chitt, Some(suspended)))).unwrap();
    assert_eq!(suspended_render.data["failureCount"], 2);
}

#[test]
fn wins_and_invalid_guesses_leave_the_chamber_clean() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    assert!(guess(&host, &chitt, "!").error.is_some());
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    let reply = guess(&host, &chitt, &answer).reply.unwrap();
    assert_eq!(
        reply["blood"],
        json!({ "total": 0, "fallen": 0, "stains": [] })
    );
}

#[test]
fn blood_resets_with_the_daily_word_and_returning_players_can_leave_new_stains() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    for word in six_wrong_guesses() {
        assert!(guess(&host, &chitt, &word).error.is_none());
    }
    let timer = host.fire_timer(&chitt, DAILY_TIMER).unwrap();
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(timer)).unwrap().error.is_none());
    host.begin_hook(HookClass::Render);
    let clean = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(clean.data["failureCount"], 0);
    assert_eq!(clean.canvas.unwrap()["blood"]["stains"], json!([]));
    start_game(&host, &chitt);
    for word in six_wrong_guesses() {
        assert!(guess(&host, &chitt, &word).error.is_none());
    }
    assert_eq!(
        crate::read_blood(current_day().unwrap())
            .unwrap()
            .victims
            .len(),
        1
    );
}

#[test]
fn public_blood_payload_is_bounded_without_losing_the_count_or_deduplication() {
    let (host, _chitt) = setup();
    let day = current_day().unwrap();
    for index in 0..70 {
        host.begin_hook(HookClass::Background);
        let mut observer = testing::observer(&format!("player-{index}"));
        observer
            .profile
            .insert("textColor".to_string(), json!("#ff8800"));
        let observer: Observer = wire(observer);
        let mut game = Game::new(day, crate::daily_answer(day).unwrap());
        game.wounds.push(crate::wound_for(&observer, day, 1));
        crate::record_damage(&observer, &game).unwrap();
    }
    let state = crate::shared_canvas_state(None, day).unwrap();
    assert_eq!(state["blood"]["total"], 70);
    assert_eq!(state["blood"]["stains"].as_array().unwrap().len(), 64);
    host.begin_hook(HookClass::Background);
    let observer: Observer = wire(testing::observer("player-0"));
    let mut game = Game::new(day, crate::daily_answer(day).unwrap());
    game.wounds.push(crate::wound_for(&observer, day, 1));
    crate::record_damage(&observer, &game).unwrap();
    assert_eq!(
        crate::shared_canvas_state(None, day).unwrap()["blood"]["total"],
        70
    );
}

#[test]
fn profile_colours_are_validated_before_becoming_canvas_or_css_values() {
    assert_eq!(crate::profile_color("#Ab12eF"), 0xab12efff);
    assert_eq!(crate::profile_color("#08c"), 0x0088ccff);
    assert_eq!(crate::profile_color(" 0088cc "), 0x0088ccff);
    assert_eq!(crate::profile_color("#000000"), 0x000000ff);
    for invalid in [
        "",
        "red",
        "#1234",
        "#12345678",
        "#gggggg",
        "url(bad)",
        "\"; color: red",
        "#ééé",
    ] {
        assert_eq!(crate::profile_color(invalid), crate::DEFAULT_BLOOD_COLOR);
    }
}

#[test]
fn render_accepts_a_decimal_observer_age() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Render);
    let mut envelope =
        serde_json::to_value(testing::render(&chitt, Some(testing::observer("player-1"))))
            .expect("render envelope serializes");
    envelope["observer"]["ageSweeps"] = json!(9.7);

    let input: RenderInput =
        serde_json::from_value(envelope).expect("decimal age matches the Observer ABI");
    let output = render(input).expect("render returns");
    assert!(output.error.is_none());
}

fn send_seen(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: chittr_cartridge::envelope::Observer,
) -> CanvasMessageOutput {
    host.begin_hook(HookClass::Background);
    let input = testing::canvas_message(chitt, Some(observer), json!({ "type": "seen" }), None);
    on_canvas_message(wire(input)).expect("the Hook returns")
}

#[test]
fn a_new_canvas_replays_an_unseen_death_until_one_reports_it_shown() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let mut last = None;
    for word in six_wrong_guesses() {
        last = guess(&host, &chitt, &word).reply;
    }
    let reply = last.expect("the losing guess replies");
    assert_eq!(reply["freshLoss"], true);
    let own = reply["ownStain"].as_u64().expect("the player's stain seed");
    assert_eq!(reply["blood"]["stains"][5]["seed"].as_u64(), Some(own));
    assert_eq!(reply["ownStains"].as_array().unwrap().len(), 3);

    // The suspension that follows the loss re-renders the card for a suspended player.
    let mut suspended = player_observer();
    suspended.is_suspended = true;
    for _ in 0..2 {
        host.begin_hook(HookClass::Render);
        let state = render(wire(testing::render(&chitt, Some(suspended.clone()))))
            .unwrap()
            .canvas
            .unwrap();
        assert_eq!(state["freshLoss"], true);
        assert_eq!(state["ownStain"].as_u64(), Some(own));
        assert_eq!(state["suspended"], true);
    }

    // A suspended player may report the death shown, but still may not guess.
    let seen = send_seen(&host, &chitt, suspended.clone());
    assert!(seen.error.is_none());
    assert_eq!(seen.rerender, Some(false));
    let seen_state = seen.reply.unwrap();
    assert_eq!(seen_state["freshLoss"], false);
    assert!(seen_state.get("ownStain").is_none());
    assert!(read_game("player-1").unwrap().unwrap().loss_seen);
    assert_eq!(
        guess_as(&host, &chitt, suspended.clone(), "crane")
            .error
            .unwrap()
            .code,
        "character_suspended"
    );

    host.begin_hook(HookClass::Render);
    let state = render(wire(testing::render(&chitt, Some(suspended.clone()))))
        .unwrap()
        .canvas
        .unwrap();
    assert_eq!(state["freshLoss"], false);
    assert!(send_seen(&host, &chitt, suspended).error.is_none());
}

#[test]
fn wins_and_unfinished_games_have_no_death_to_show() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let reply = guess(&host, &chitt, &wrong_word()).reply.unwrap();
    assert_eq!(reply["freshLoss"], false);
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    let won = guess(&host, &chitt, &answer).reply.unwrap();
    assert_eq!(won["freshLoss"], false);
    let seen = send_seen(&host, &chitt, player_observer());
    assert!(seen.error.is_none());
    assert!(!read_game("player-1").unwrap().unwrap().loss_seen);
}

#[test]
fn new_games_use_twelve_letter_words_and_refuse_other_lengths() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let game = read_game("player-1").unwrap().unwrap();
    assert_eq!(game.answer.len(), 12);
    for short in ["crane", "elevenletter".get(..11).unwrap(), "thirteenchars"] {
        let error = guess(&host, &chitt, short)
            .error
            .expect("wrong length is refused");
        assert_eq!(error.code, "bad_guess");
        assert_eq!(error.message, "guesses must be 12 letters");
    }
    let reply = guess(&host, &chitt, &wrong_word()).reply.unwrap();
    assert_eq!(reply["length"], 12);
    assert_eq!(reply["guesses"][0]["marks"].as_array().unwrap().len(), 12);

    host.begin_hook(HookClass::Render);
    let public = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(public.canvas.unwrap()["length"], 12);
}

// A game saved by 0.1.x has a five-letter answer and five marks a guess; it must still load and finish.
#[test]
fn a_five_letter_game_from_before_the_upgrade_can_still_be_finished() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let day = current_day().unwrap();
    let saved = json!({
        "day": day, "answer": "crane",
        "guesses": [{ "word": "slate", "marks": [0, 0, 2, 0, 2] }],
        "complete": false, "won": false,
        "penalty_pending": false, "penalty_applied": false, "penalty_error": null
    });
    host.begin_hook(HookClass::Background);
    chittr_cartridge::kv::set("game:player-1", saved.to_string().as_bytes(), None).unwrap();
    let old = read_game("player-1").unwrap().expect("old game loads");
    assert_eq!(old.guesses[0].marks, vec![0, 0, 2, 0, 2]);

    assert_eq!(
        guess(&host, &chitt, "qqqqqqqqqqqq").error.unwrap().message,
        "guesses must be 5 letters"
    );
    let reply = guess(&host, &chitt, "crane").reply.unwrap();
    assert_eq!(reply["length"], 5);
    assert_eq!(reply["won"], true);
}

#[test]
fn scoring_handles_repeated_letters_in_long_words() {
    assert_eq!(
        crate::score_guess("decapitation", "deceptionist"),
        vec![2, 2, 2, 0, 2, 1, 1, 1, 1, 2, 0, 1]
    );
}

// A Chitt that ran 0.1.x has the daily word and its timer, but no record of the switch to twelve.
fn upgrade_live_chitt(host: &TestHost, chitt: &testing::ChittRef) {
    host.begin_hook(HookClass::Lifecycle);
    assert!(chittr_cartridge::kv::delete(crate::TWELVE_FROM_KEY).unwrap());
    let input = testing::migrate(
        chitt,
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.1.1".to_string(),
            sha256: "old".to_string(),
        },
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.0".to_string(),
            sha256: "new".to_string(),
        },
    );
    assert!(migrate(wire(input)).unwrap().error.is_none());
}

#[test]
fn an_upgraded_chitt_keeps_its_live_five_letter_word_until_the_next_rotation() {
    let (host, chitt) = setup();
    let day = current_day().unwrap();
    upgrade_live_chitt(&host, &chitt);

    let today = crate::daily_answer(day).unwrap();
    assert_eq!(today, crate::answer_for(day, i64::MAX));
    assert_eq!(today.len(), 5);
    host.begin_hook(HookClass::Render);
    let card = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(card.data["wordLength"], 5);
    assert_eq!(card.canvas.unwrap()["length"], 5);

    // Someone who starts after the upgrade still gets today's five-letter word.
    start_game(&host, &chitt);
    assert_eq!(read_game("player-1").unwrap().unwrap().answer, today);
    let reply = guess(&host, &chitt, &today).reply.unwrap();
    assert_eq!(reply["won"], true);
    assert_eq!(reply["length"], 5);

    host.advance(DAY_SECONDS);
    let timer = host.fire_timer(&chitt, DAILY_TIMER).unwrap();
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(timer)).unwrap().error.is_none());
    let tomorrow = crate::daily_answer(day + 1).unwrap();
    assert_eq!(tomorrow.len(), 12);
    host.begin_hook(HookClass::Render);
    let card = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(card.data["wordLength"], 12);
    start_game(&host, &chitt);
    assert_eq!(read_game("player-1").unwrap().unwrap().answer, tomorrow);

    // A second upgrade does not move the switch.
    host.begin_hook(HookClass::Lifecycle);
    let again = testing::migrate(
        &chitt,
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.0".to_string(),
            sha256: "new".to_string(),
        },
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.1".to_string(),
            sha256: "newer".to_string(),
        },
    );
    assert!(migrate(wire(again)).unwrap().error.is_none());
    assert_eq!(crate::daily_answer(day + 1).unwrap(), tomorrow);
    assert_eq!(crate::daily_answer(day).unwrap(), today);
}

#[test]
fn a_new_chitt_starts_on_twelve_letters() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Render);
    let card = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(card.data["wordLength"], 12);
    assert_eq!(
        crate::daily_answer(current_day().unwrap()).unwrap().len(),
        12
    );
}

fn named(id: &str, trolltag: &str, color: &str) -> chittr_cartridge::envelope::Observer {
    let mut observer = testing::observer(id);
    observer
        .profile
        .insert("trolltag".to_string(), json!(trolltag));
    observer
        .profile
        .insert("discriminator".to_string(), json!("0000"));
    observer
        .profile
        .insert("textColor".to_string(), json!(color));
    observer
}

fn start_as(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: &chittr_cartridge::envelope::Observer,
) {
    host.begin_hook(HookClass::Control);
    let input = testing::control(chitt, Some(observer.clone()), "start", "start");
    assert!(on_control(wire(input)).unwrap().error.is_none());
}

fn lose_as(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: &chittr_cartridge::envelope::Observer,
) {
    start_as(host, chitt, observer);
    for word in six_wrong_guesses() {
        assert!(guess_as(host, chitt, observer.clone(), &word)
            .error
            .is_none());
    }
}

fn win_as(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: &chittr_cartridge::envelope::Observer,
) {
    start_as(host, chitt, observer);
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    assert!(guess_as(host, chitt, observer.clone(), &answer)
        .error
        .is_none());
}

fn card(host: &TestHost, chitt: &testing::ChittRef) -> serde_json::Map<String, serde_json::Value> {
    host.begin_hook(HookClass::Render);
    render(wire(testing::render(chitt, None))).unwrap().data
}

#[test]
fn the_obituary_and_survivors_list_todays_finishers_newest_first() {
    let (host, chitt) = setup();
    host.refuse_action("suspend_character", "forbidden", "permission denied");
    let empty = card(&host, &chitt);
    assert_eq!(empty["obituary"], json!([]));
    assert_eq!(empty["survivors"], json!([]));
    assert_eq!(empty["obituaryMore"], 0);

    lose_as(&host, &chitt, &named("player-1", "firstVictim", "#0088cc"));
    win_as(&host, &chitt, &named("player-2", "luckyOne", "#3a5"));
    lose_as(&host, &chitt, &named("player-3", "secondVictim", "#a3c"));
    // A retry of the final guess does not list anyone twice.
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    guess_as(
        &host,
        &chitt,
        named("player-2", "luckyOne", "#3a5"),
        &answer,
    );

    let data = card(&host, &chitt);
    assert_eq!(
        data["obituary"],
        json!([
            { "mention": "@secondVictim", "color": "#aa33cc" },
            { "mention": "@firstVictim", "color": "#0088cc" },
        ])
    );
    assert_eq!(
        data["survivors"],
        json!([{ "mention": "@luckyOne", "color": "#33aa55" }])
    );
    assert_eq!(data["obituaryMore"], 0);
    assert_eq!(data["survivorsMore"], 0);
    let encoded = serde_json::to_string(&data).unwrap();
    assert!(!encoded.contains("player-1") && !encoded.contains("player-2"));
}

#[test]
fn migrate_rebuilds_todays_roll_and_names_fill_in_as_players_return() {
    let (host, chitt) = setup();
    host.refuse_action("suspend_character", "forbidden", "permission denied");
    let victim = named("player-1", "firstVictim", "#0088cc");
    let survivor = named("player-2", "luckyOne", "#3a5");
    lose_as(&host, &chitt, &victim);
    win_as(&host, &chitt, &survivor);
    // As if they finished on a version from before the roll existed.
    host.begin_hook(HookClass::Lifecycle);
    assert!(chittr_cartridge::kv::delete(&format!("roll:{}", current_day().unwrap())).unwrap());

    upgrade_live_chitt(&host, &chitt);
    let data = card(&host, &chitt);
    assert_eq!(data["obituary"], json!([]));
    assert_eq!(data["obituaryMore"], 1);
    assert_eq!(data["survivorsMore"], 1);

    // Each player's mention appears once they next view the card; the victim keeps the colour
    // of their stain even though their profile colour has changed since.
    host.begin_hook(HookClass::Render);
    let mut returning = victim.clone();
    returning
        .profile
        .insert("textColor".to_string(), json!("#ffffff"));
    render(wire(testing::render(&chitt, Some(returning)))).unwrap();
    let data = card(&host, &chitt);
    assert_eq!(
        data["obituary"],
        json!([{ "mention": "@firstVictim", "color": "#0088cc" }])
    );
    assert_eq!(data["obituaryMore"], 0);
    assert_eq!(data["survivorsMore"], 1);

    host.begin_hook(HookClass::Render);
    render(wire(testing::render(&chitt, Some(survivor)))).unwrap();
    let data = card(&host, &chitt);
    assert_eq!(
        data["survivors"],
        json!([{ "mention": "@luckyOne", "color": "#33aa55" }])
    );

    // Running the migration again adds nobody twice.
    upgrade_live_chitt_again(&host, &chitt);
    let data = card(&host, &chitt);
    assert_eq!(data["obituary"].as_array().unwrap().len(), 1);
    assert_eq!(data["survivors"].as_array().unwrap().len(), 1);
}

fn upgrade_live_chitt_again(host: &TestHost, chitt: &testing::ChittRef) {
    host.begin_hook(HookClass::Lifecycle);
    let input = testing::migrate(
        chitt,
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.2".to_string(),
            sha256: "a".to_string(),
        },
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.2.3".to_string(),
            sha256: "b".to_string(),
        },
    );
    assert!(migrate(wire(input)).unwrap().error.is_none());
}

// A long roll scrolls inside the card instead of stretching the Live Chitt. Host style filters
// strip <style>, so everything is inline and the server says when a list scrolls.
#[test]
fn both_lists_scroll_within_a_fixed_height() {
    let template = include_str!("../cartridge/templates/card.liquid");
    assert!(!template.contains("<style"));
    let lists: Vec<&str> = template.split("<ul ").skip(1).collect();
    assert_eq!(lists.len(), 2);
    for list in lists {
        let style = list.split('>').next().unwrap();
        assert!(
            style.contains("max-height: 128px; overflow-y: auto"),
            "{style}"
        );
        let row = list
            .split("<li ")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            row.contains("flex: none") && row.contains("height: 16px"),
            "{row}"
        );
    }
    for name in ["obituary", "survivors"] {
        assert!(template.contains(&format!("{{% if {name}Scrolls %}}")));
        assert!(template.contains(&format!("{{{{ {name}Count }}}}")));
    }
    assert!(template.contains("↓ Scroll for more"));
}

fn set_roll(host: &TestHost, fallen: Vec<serde_json::Value>) {
    host.begin_hook(HookClass::Background);
    chittr_cartridge::kv::set(
        &format!("roll:{}", current_day().unwrap()),
        json!({ "fallen": fallen, "survivors": [] })
            .to_string()
            .as_bytes(),
        None,
    )
    .unwrap();
}

#[test]
fn a_list_says_it_scrolls_only_once_it_outgrows_six_rows() {
    let (host, chitt) = setup();
    for (count, scrolls) in [(0, false), (6, false), (7, true), (30, true)] {
        set_roll(
            &host,
            (0..count)
                .map(|index| json!({ "id": format!("p{index}"), "mention": format!("@victim{index}"), "color": 0x9b2638ffu32 }))
                .collect(),
        );
        let data = card(&host, &chitt);
        assert_eq!(data["obituaryScrolls"], scrolls, "{count} names");
        assert_eq!(data["obituaryCount"], count);
        assert_eq!(data["survivorsScrolls"], false);
    }
    // Names not known yet are counted, not listed, so they never make the list scroll.
    set_roll(
        &host,
        (0..10)
            .map(|index| json!({ "id": format!("p{index}") }))
            .collect(),
    );
    let data = card(&host, &chitt);
    assert_eq!(data["obituaryScrolls"], false);
    assert_eq!(data["obituaryMore"], 10);
}

#[test]
fn a_long_roll_shows_the_newest_names_and_counts_the_rest() {
    let (host, chitt) = setup();
    let day = current_day().unwrap();
    let fallen: Vec<_> = (0..30)
        .map(|index| json!({ "id": format!("p{index}"), "mention": format!("@victim{index}"), "color": 0x9b2638ffu32 }))
        .collect();
    host.begin_hook(HookClass::Background);
    chittr_cartridge::kv::set(
        &format!("roll:{day}"),
        json!({ "fallen": fallen, "survivors": [] })
            .to_string()
            .as_bytes(),
        None,
    )
    .unwrap();
    let data = card(&host, &chitt);
    let shown = data["obituary"].as_array().unwrap();
    assert_eq!(shown.len(), 24);
    assert_eq!(shown[0]["mention"], "@victim29");
    assert_eq!(data["obituaryMore"], 6);
}
