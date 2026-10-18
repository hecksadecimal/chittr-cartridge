//! The poll Cartridge's Server Guest. Its Hook inputs and outputs are the types `xtp plugin init` generates from
//! `docs/cartridges/v1/server.xtp.yaml` (see the README). The host calls go through the SDK (`chittr_cartridge`), so
//! the `testing` feature can fake them and `src/tests.rs` runs the Hooks under `cargo test`.
//!
//! `init` gives the Chitt a waiting message. A site admin configures its question and answer choices once from the card;
//! that starts the close timer. Until then, the card tells everyone to wait. `render` reads the saved poll from the
//! Chitt's KV store and draws each choice's tally and the viewer's own vote.
//!
//! The Canvas Guest draws each dynamic answer as a transparent button and sends its option id to `on_canvas_message`.
//! The Server Guest validates the vote and stores one vote per Character under `vote:<characterId>`, with tallies by id.
//!
//! The `on_reply` gate refuses replies once the poll is closed, with a message the composer shows.
//!
//! The poll closes once, by either closer: the close timer (`on_timer`) or the poster's `close` Control. Each one
//! announces the leading option as a reply from the poster, and rewrites the Chitt's own text with `edit_chitt`. The
//! announcement happens once, and a timer retry after a failed announcement is safe. The `close` Control's card
//! button is not in the template yet, so today it is reached through the Control endpoint only.
//!
//! The generated bindings name every Hook, so `src/pdk.rs` exports only the Hooks defined here (see the README).
//! Failures are returned as the output's `error` field, never as a trap, as the ABI asks.

#[cfg(test)]
mod tests;

mod pdk;

use chittr_cartridge::{kv, poster, push, timer};
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

const WAITING_MESSAGE: &str = "Please wait until the poll is ready.";
const CONFIG_KEY: &str = "poll-config";
const VIEWS_KEY: &str = "views";
const CONFIGURE_CONTROL: &str = "configure";
const CLOSE_CONTROL: &str = "close";
const MAX_QUESTION_CHARS: usize = 200;
const MAX_OPTIONS_TEXT_CHARS: usize = 1000;
const MAX_OPTION_LABEL_CHARS: usize = 80;
const MAX_OPTION_ID_CHARS: usize = 64;
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 10;
/// Set by either closer. While it is set, replies are refused.
const CLOSED_KEY: &str = "closed";
/// Claimed once, before the announcement is posted. A close that finds it claimed changes nothing.
const ANNOUNCED_KEY: &str = "announced";
const CLOSE_TIMER: &str = "close";
/// The poll closes an hour after an admin configures it.
const CLOSES_AFTER_SECONDS: u64 = 60 * 60;

/// The poll is not active until a site admin configures it from the rendered card.
pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput {
        content: WAITING_MESSAGE.to_string(),
        ..Default::default()
    })
}

/// Refreshes the Live Chitt's stored body from its saved poll question on upgrades and rollbacks.
pub(crate) fn migrate(_input: MigrateInput) -> Result<MigrateOutput, Error> {
    let content = match configured_poll() {
        Ok(Some(poll)) => poll.question,
        Ok(None) => WAITING_MESSAGE.to_string(),
        Err(error) => {
            return Ok(MigrateOutput { error: Some(error) });
        }
    };
    let result = edit_chitt(&content);
    Ok(MigrateOutput {
        error: result.err(),
    })
}

/// The close timer fired: close the poll and announce its result, through the same path as the `close` Control.
/// Timers are at least once, so a repeat finds the announcement claimed and changes nothing.
pub(crate) fn on_timer(input: TimerInput) -> Result<TimerOutput, Error> {
    let result = if input.name == CLOSE_TIMER {
        match configured_poll() {
            Ok(Some(_)) => close_poll(&input.chitt.id),
            Ok(None) => Ok(()),
            Err(error) => Err(error),
        }
    } else {
        Ok(())
    };
    Ok(TimerOutput {
        error: result.err(),
    })
}

pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    let is_admin = input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.is_admin);
    let poll = match configured_poll() {
        Ok(Some(poll)) => poll,
        Ok(None) => {
            return Ok(RenderOutput {
                template: "card".to_string(),
                data: object(json!({
                    "ready": false,
                    "waitMessage": WAITING_MESSAGE,
                    "isAdmin": is_admin,
                })),
                canvas: None,
                error: None,
            });
        }
        Err(error) => {
            return Ok(RenderOutput {
                template: String::new(),
                data: Map::new(),
                canvas: None,
                error: Some(error),
            });
        }
    };
    let options = &poll.options;
    let ids: Vec<String> = options.iter().map(|option| option.id.clone()).collect();
    let tallies = tallies(&ids);
    let total: i64 = tallies.iter().sum();
    let voted = input
        .observer
        .as_ref()
        .and_then(|observer| vote_of(&observer.character_id));
    let rows: Vec<Value> = options
        .iter()
        .zip(&tallies)
        .map(|(option, votes)| {
            json!({
                "id": option.id,
                "label": option.label,
                "votes": votes,
                "percent": percent(*votes, total),
                "voted": voted.as_deref() == Some(option.id.as_str()),
            })
        })
        .collect();
    // A failed KV call leaves the count off the card; the poll still renders. Treat a failed read as open.
    let views = count_view(input.observer.as_ref());
    let closed = is_closed();
    Ok(RenderOutput {
        template: "card".to_string(),
        data: object(json!({
            "ready": true,
            "question": poll.question.clone(),
            "canvasHeight": poll.options.len() * 64 + 32,
            "options": rows.clone(),
            "signedIn": input.observer.is_some(),
            "total": total,
            "views": views,
            "closed": closed,
        })),
        canvas: Some(object(json!({
            "options": rows,
            "signedIn": input.observer.is_some(),
            "closed": closed,
        }))),
        error: None,
    })
}

pub(crate) fn on_canvas_message(input: CanvasMessageInput) -> Result<CanvasMessageOutput, Error> {
    let result = canvas_vote(&input);
    Ok(match result {
        Ok(()) => CanvasMessageOutput {
            reply: Some(object(json!({ "voted": true }))),
            rerender: Some(true),
            error: None,
        },
        Err(error) => CanvasMessageOutput {
            reply: None,
            rerender: None,
            error: Some(error),
        },
    })
}

fn canvas_vote(input: &CanvasMessageInput) -> Result<(), HookError> {
    let observer = input
        .observer
        .as_ref()
        .ok_or_else(|| fail("logged_out", "log in to vote"))?;
    let option_id = input
        .message
        .get("optionId")
        .and_then(Value::as_str)
        .ok_or_else(|| fail("bad_input", "a vote names one poll option"))?;
    cast_vote(observer, option_id)
}

/// A closed poll takes no more replies. The poll's own closing reply skips this gate: a Chitt's own actions
/// don't run its own Hooks. A KV read that fails lets the reply through, as every gate does on failure.
pub(crate) fn on_reply(_input: ReplyGateInput) -> Result<GateOutput, Error> {
    Ok(match kv_read(CLOSED_KEY) {
        Ok(Some(_)) => GateOutput {
            allow: Some(false),
            message: Some("The poll is closed, so it takes no more replies.".to_string()),
            error: None,
        },
        _ => GateOutput {
            allow: Some(true),
            message: None,
            error: None,
        },
    })
}

pub(crate) fn on_control(input: ControlInput) -> Result<ControlOutput, Error> {
    let result = match input.control.as_str() {
        CONFIGURE_CONTROL => configure(input),
        CLOSE_CONTROL => close(input),
        _ => Err(fail("unknown_control", "this poll has no such Control")),
    };
    Ok(ControlOutput {
        error: result.err(),
    })
}

fn configure(input: ControlInput) -> Result<(), HookError> {
    if !input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.is_admin)
    {
        return Err(fail(
            "forbidden",
            "only site admins can configure this poll",
        ));
    }
    if configured_poll()?.is_some() {
        return Err(fail(
            "poll_already_ready",
            "this poll has already been configured",
        ));
    }
    let fields = input.fields.as_ref().filter(|fields| fields.len() == 2);
    let question = fields
        .and_then(|fields| fields.get("question"))
        .and_then(Value::as_str)
        .filter(|_| input.value.is_none())
        .ok_or_else(|| fail("bad_input", "supply a poll question and answer choices"))?
        .trim();
    let raw_options = fields
        .and_then(|fields| fields.get("options"))
        .and_then(Value::as_str)
        .ok_or_else(|| fail("bad_input", "supply a poll question and answer choices"))?
        .trim();
    if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
        return Err(fail(
            "bad_input",
            "the question must be 1 to 200 characters",
        ));
    }
    if raw_options.chars().count() > MAX_OPTIONS_TEXT_CHARS {
        return Err(fail("bad_input", "the answer choices are too long"));
    }
    let labels: Vec<&str> = raw_options
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&labels.len()) {
        return Err(fail(
            "bad_input",
            "enter 2 to 10 answer choices, one per line",
        ));
    }
    let mut seen = HashSet::new();
    let mut options = Vec::with_capacity(labels.len());
    for label in labels {
        if label.chars().count() > MAX_OPTION_LABEL_CHARS {
            return Err(fail(
                "bad_input",
                "each answer choice must be at most 80 characters",
            ));
        }
        let id = slug(label);
        if id.is_empty() || id.len() > MAX_OPTION_ID_CHARS || !seen.insert(id.clone()) {
            return Err(fail("bad_input", "answer choices must have distinct names"));
        }
        options.push(PollOption {
            id,
            label: label.to_string(),
        });
    }
    let poll = PollConfig {
        question: question.to_string(),
        options,
    };
    let encoded =
        serde_json::to_vec(&poll).map_err(|error| fail("invalid_state", error.to_string()))?;

    // Schedule before publishing the one-time configuration. If the timer cannot be scheduled,
    // leave the poll in its waiting state rather than create a poll that never closes.
    timer::schedule_timer(CLOSE_TIMER, CLOSES_AFTER_SECONDS, Value::Null).map_err(kv_fail)?;
    if !kv_set_if_absent(CONFIG_KEY, &encoded)? {
        return Err(fail(
            "poll_already_ready",
            "this poll has already been configured",
        ));
    }
    if let Err(error) = edit_chitt(&poll.question) {
        let _ = kv::delete(CONFIG_KEY);
        let _ = timer::cancel_timer(CLOSE_TIMER);
        return Err(error);
    }
    // A refused push doesn't undo the configuration; the next render will still show the poll.
    let _ = push_render_all();
    Ok(())
}

fn cast_vote(observer: &Observer, choice: &str) -> Result<(), HookError> {
    let poll = configured_poll()?.ok_or_else(|| fail("poll_not_ready", WAITING_MESSAGE))?;
    if is_closed() {
        return Err(fail("poll_closed", "this poll is closed"));
    }
    if !poll.options.iter().any(|option| option.id == choice) {
        return Err(fail("unknown_option", "that option is not on the poll"));
    }

    let key = format!("vote:{}", observer.character_id);
    let previous = vote_of(&observer.character_id);
    if previous.as_deref() == Some(choice) {
        return Ok(());
    }
    if let Some(previous) = previous {
        kv_incr(&format!("tally:{previous}"), -1)?;
    }
    kv_incr(&format!("tally:{choice}"), 1)?;
    kv_write(&key, choice.as_bytes())?;
    // The tallies changed, so every Observer's card is stale. A refused push doesn't undo the vote.
    let _ = push_render_all();
    Ok(())
}

/// Whether the poll is closed, by either closer. A failed read counts as open: the poll stays usable.
fn is_closed() -> bool {
    kv_read(CLOSED_KEY).ok().flatten().is_some()
}

/// Closes the poll, as its poster. A second close changes nothing.
fn close(input: ControlInput) -> Result<(), HookError> {
    let observer = input
        .observer
        .ok_or_else(|| fail("logged_out", "log in to close the poll"))?;
    if !observer.is_poster {
        return Err(fail("forbidden", "only the poster can close the poll"));
    }
    close_poll(&input.chitt.id)
}

/// Closes the poll and announces its result, once. Both closers come here: the close timer and the poster's `close`
/// Control. The closed flag turns replies away. The announcement is claimed with `set_if_absent` before the posts, so
/// a second close changes nothing. The claim is released when the posts fail, so a retried timer can still announce.
fn close_poll(chitt_id: &str) -> Result<(), HookError> {
    let poll = configured_poll()?.ok_or_else(|| fail("poll_not_ready", WAITING_MESSAGE))?;
    kv_write(CLOSED_KEY, b"1")?;
    if !kv_set_if_absent(ANNOUNCED_KEY, b"1")? {
        return Ok(());
    }
    announce(chitt_id, &poll).inspect_err(|_| {
        let _ = kv::delete(ANNOUNCED_KEY);
    })
}

/// Posts the leading option as the poster's reply, and rewrites the Chitt's own text.
fn announce(chitt_id: &str, poll: &PollConfig) -> Result<(), HookError> {
    let ids: Vec<String> = poll
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect();
    let (label, votes) = poll
        .options
        .iter()
        .zip(tallies(&ids))
        .max_by_key(|(_, votes)| *votes)
        .map(|(option, votes)| (option.label.clone(), votes))
        .ok_or_else(|| fail("invalid", "the poll has no options"))?;
    let noun = if votes == 1 { "vote" } else { "votes" };
    post_reply(
        chitt_id,
        &format!("The poll is closed. {label} leads with {votes} {noun}."),
    )?;
    edit_chitt(&format!("{} Closed: {label} won.", poll.question))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PollConfig {
    question: String,
    options: Vec<PollOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PollOption {
    id: String,
    label: String,
}

/// The poll settings are saved once by an admin. Missing settings mean the poll is not ready.
fn configured_poll() -> Result<Option<PollConfig>, HookError> {
    let Some(bytes) = kv_read(CONFIG_KEY)? else {
        return Ok(None);
    };
    let poll: PollConfig = serde_json::from_slice(&bytes).map_err(|error| {
        fail(
            "invalid_state",
            format!("the poll configuration is invalid: {error}"),
        )
    })?;
    if poll.question.trim().is_empty()
        || poll.options.len() < MIN_OPTIONS
        || poll.options.len() > MAX_OPTIONS
        || poll
            .options
            .iter()
            .any(|option| option.id.is_empty() || option.label.trim().is_empty())
    {
        return Err(fail("invalid_state", "the poll configuration is invalid"));
    }
    Ok(Some(poll))
}

fn slug(label: &str) -> String {
    let mut id = String::new();
    let mut separator = false;
    for ch in label.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if separator && !id.is_empty() {
                id.push('-');
            }
            id.push(ch);
            separator = false;
        } else {
            separator = true;
        }
    }
    id
}

/// Each option's tally, zero when the count is missing or the KV store is unavailable.
fn tallies(ids: &[String]) -> Vec<i64> {
    let keys: Vec<String> = ids.iter().map(|id| format!("tally:{id}")).collect();
    let names: Vec<&str> = keys.iter().map(String::as_str).collect();
    match kv::get_many(&names) {
        Ok(values) => values.iter().map(|value| count(value.as_deref())).collect(),
        Err(_) => vec![0; ids.len()],
    }
}

fn count(value: Option<&[u8]>) -> i64 {
    value
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| text.parse::<i64>().ok())
        .unwrap_or(0)
}

/// Counts each signed-in Character at most once. Anonymous renders don't represent a stable Character.
fn count_view(observer: Option<&Observer>) -> Option<i64> {
    let current_count = || {
        kv_read(VIEWS_KEY)
            .ok()
            .flatten()
            .map(|value| count(Some(&value)))
    };
    let Some(observer) = observer else {
        return current_count();
    };
    let key = format!("viewed:{}", observer.character_id);
    match kv_set_if_absent(&key, b"1") {
        Ok(true) => match kv_incr(VIEWS_KEY, 1) {
            Ok(total) => Some(total),
            Err(_) => {
                let _ = kv::delete(&key);
                None
            }
        },
        Ok(false) | Err(_) => current_count(),
    }
}

/// The option id this Character voted for, if it has voted.
fn vote_of(character_id: &str) -> Option<String> {
    let entry = kv_read(&format!("vote:{character_id}")).ok().flatten()?;
    String::from_utf8(entry).ok()
}

fn percent(votes: i64, total: i64) -> i64 {
    if total <= 0 {
        0
    } else {
        votes * 100 / total
    }
}

// Host calls. The SDK answers each one, and each failure becomes the HookError the Hook hands back.

/// A failure the Hook reports itself: a code and a message.
fn fail(code: &str, message: impl Into<String>) -> HookError {
    HookError {
        code: code.to_string(),
        message: message.into(),
    }
}

/// A KV, timer or Render Push failure: the host's code and message.
fn kv_fail(error: kv::KvError) -> HookError {
    fail(&error.code, error.message)
}

fn poster_fail(error: poster::PosterError) -> HookError {
    fail(&error.code, error.message)
}

fn kv_read(key: &str) -> Result<Option<Vec<u8>>, HookError> {
    Ok(kv::get(key).map_err(kv_fail)?.map(|entry| entry.value))
}

fn kv_write(key: &str, value: &[u8]) -> Result<(), HookError> {
    kv::set(key, value, None).map_err(kv_fail)
}

/// Sets `key` only if it has no live value. `Ok(false)` means it already had one.
fn kv_set_if_absent(key: &str, value: &[u8]) -> Result<bool, HookError> {
    kv::set_if_absent(key, value, None).map_err(kv_fail)
}

fn kv_incr(key: &str, by: i64) -> Result<i64, HookError> {
    kv::incr(key, by, None).map_err(kv_fail)
}

/// Records the poster's reply to the Chitt, through its Poster Action.
fn post_reply(post_id: &str, content: &str) -> Result<(), HookError> {
    poster::post_reply(post_id, content)
        .map(|_| ())
        .map_err(poster_fail)
}

/// Rewrites the Chitt's own text, as its poster.
fn edit_chitt(content: &str) -> Result<(), HookError> {
    let args = poster::EditArgs {
        content: content.to_string(),
        ..Default::default()
    };
    poster::edit_chitt(args).map_err(poster_fail)
}

/// Queues a Render Push for every Observer of this Chitt.
fn push_render_all() -> Result<(), HookError> {
    push::push_render(push::PushTarget::All).map_err(kv_fail)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
