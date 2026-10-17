//! The poll Cartridge's Server Guest. Its Hook inputs and outputs are the types `xtp plugin init` generates from
//! `docs/cartridges/v1/server.xtp.yaml` (see the README). The host calls go through the SDK (`chittr_cartridge`), so
//! the `testing` feature can fake them and `src/tests.rs` runs the Hooks under `cargo test`.
//!
//! `init` gives the Chitt its text and schedules a timer that closes the poll. `render` reads the options from
//! `assets/toppings.txt` with `asset()`, counts one view in the Chitt's KV store, and draws the card from
//! `templates/card.liquid` with each option's tally and the viewer's own vote (and "closed" once the poll is). The
//! same tallies go to the Canvas Guest as `canvas` state, which draws them as a bar chart.
//!
//! The `vote` Control takes one option id. `on_control` keeps one vote per Character, under `vote:<characterId>`, and
//! keeps the tallies under `tally:<option id>`. The manifest's `vote` enum must list the same ids as the options in
//! `toppings.txt`: each id is the label in lowercase, spaces as "-".
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

use chittr_cartridge::{host, kv, poster, push, timer};
use extism_pdk::Error;
use pdk::types::*;
use serde_json::{json, Map, Value};

const QUESTION: &str = "Which pizza topping is best?";
const VIEWS_KEY: &str = "views";
const VOTE_CONTROL: &str = "vote";
const CLOSE_CONTROL: &str = "close";
/// Set by either closer. While it is set, replies are refused.
const CLOSED_KEY: &str = "closed";
/// Claimed once, before the announcement is posted. A close that finds it claimed changes nothing.
const ANNOUNCED_KEY: &str = "announced";
const CLOSE_TIMER: &str = "close";
/// The poll closes an hour after it is published.
const CLOSES_AFTER_SECONDS: u64 = 60 * 60;

/// Publishing is refused when the close timer can't be scheduled: a poll that never closes is a bug.
pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(match timer::schedule_timer(CLOSE_TIMER, CLOSES_AFTER_SECONDS, Value::Null) {
        Ok(()) => InitOutput { content: QUESTION.to_string(), ..Default::default() },
        Err(error) => InitOutput { content: String::new(), error: Some(kv_fail(error)), ..Default::default() },
    })
}

/// The close timer fired: close the poll and announce its result, through the same path as the `close` Control.
/// Timers are at least once, so a repeat finds the announcement claimed and changes nothing.
pub(crate) fn on_timer(input: TimerInput) -> Result<TimerOutput, Error> {
    let result = if input.name == CLOSE_TIMER { close_poll(&input.chitt.id) } else { Ok(()) };
    Ok(TimerOutput { error: result.err() })
}

pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    let options = match options() {
        Ok(options) => options,
        Err(error) => {
            return Ok(RenderOutput {
                template: String::new(),
                data: Map::new(),
                canvas: None,
                error: Some(error),
            })
        }
    };
    let ids: Vec<String> = options.iter().map(|(id, _)| id.clone()).collect();
    let tallies = tallies(&ids);
    let total: i64 = tallies.iter().sum();
    let voted = input.observer.as_ref().and_then(|observer| vote_of(&observer.character_id));
    let rows: Vec<Value> = options
        .iter()
        .zip(&tallies)
        .map(|((id, label), votes)| {
            json!({
                "id": id,
                "label": label,
                "votes": votes,
                "percent": percent(*votes, total),
                "voted": voted.as_deref() == Some(id.as_str()),
            })
        })
        .collect();
    let bars: Vec<Value> = options
        .iter()
        .zip(&tallies)
        .map(|((_, label), votes)| json!({ "label": label, "votes": votes }))
        .collect();
    // A failed KV call leaves the count off the card; the poll still renders. Treat a failed read as open.
    let views = kv_incr(VIEWS_KEY, 1).ok();
    let closed = is_closed();
    Ok(RenderOutput {
        template: "card".to_string(),
        data: object(json!({
            "question": QUESTION,
            "options": rows,
            "signedIn": input.observer.is_some(),
            "total": total,
            "views": views,
            "closed": closed,
        })),
        canvas: Some(object(json!({ "bars": bars }))),
        error: None,
    })
}

/// A click on a bar of the Canvas Guest. The message names the bar by its index in the options; the reply
/// is that option's tally and share, sent back only to the canvas that asked. Nothing is drawn again.
pub(crate) fn on_canvas_message(input: CanvasMessageInput) -> Result<CanvasMessageOutput, Error> {
    Ok(match bar_reply(&input.message) {
        Ok(reply) => CanvasMessageOutput { reply: Some(reply), rerender: None, error: None },
        Err(error) => CanvasMessageOutput { reply: None, rerender: None, error: Some(error) },
    })
}

fn bar_reply(message: &Map<String, Value>) -> Result<Map<String, Value>, HookError> {
    let bar = message
        .get("bar")
        .and_then(Value::as_u64)
        .ok_or_else(|| fail("bad_input", "a message names one bar"))? as usize;
    let options = options()?;
    let (_, label) = options
        .get(bar)
        .ok_or_else(|| fail("unknown_bar", "that bar is not on the poll"))?;
    let ids: Vec<String> = options.iter().map(|(id, _)| id.clone()).collect();
    let tallies = tallies(&ids);
    let total: i64 = tallies.iter().sum();
    let votes = tallies[bar];
    Ok(object(json!({ "label": label, "votes": votes, "percent": percent(votes, total) })))
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
        _ => GateOutput { allow: Some(true), message: None, error: None },
    })
}

pub(crate) fn on_control(input: ControlInput) -> Result<ControlOutput, Error> {
    let result = match input.control.as_str() {
        VOTE_CONTROL => vote(input),
        CLOSE_CONTROL => close(input),
        _ => Err(fail("unknown_control", "this poll has no such Control")),
    };
    Ok(ControlOutput { error: result.err() })
}

fn vote(input: ControlInput) -> Result<(), HookError> {
    if is_closed() {
        return Err(fail("poll_closed", "this poll is closed"));
    }
    let observer = input.observer.ok_or_else(|| fail("logged_out", "log in to vote"))?;
    let choice = input.value.ok_or_else(|| fail("bad_input", "a vote names one option"))?;
    if !options()?.iter().any(|(id, _)| *id == choice) {
        return Err(fail("unknown_option", "that option is not on the poll"));
    }

    let key = format!("vote:{}", observer.character_id);
    let previous = vote_of(&observer.character_id);
    if previous.as_deref() == Some(choice.as_str()) {
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
    let observer = input.observer.ok_or_else(|| fail("logged_out", "log in to close the poll"))?;
    if !observer.is_poster {
        return Err(fail("forbidden", "only the poster can close the poll"));
    }
    close_poll(&input.chitt.id)
}

/// Closes the poll and announces its result, once. Both closers come here: the close timer and the poster's `close`
/// Control. The closed flag turns replies away. The announcement is claimed with `set_if_absent` before the posts, so
/// a second close changes nothing. The claim is released when the posts fail, so a retried timer can still announce.
fn close_poll(chitt_id: &str) -> Result<(), HookError> {
    kv_write(CLOSED_KEY, b"1")?;
    if !kv_set_if_absent(ANNOUNCED_KEY, b"1")? {
        return Ok(());
    }
    announce(chitt_id).inspect_err(|_| {
        let _ = kv::delete(ANNOUNCED_KEY);
    })
}

/// Posts the leading option as the poster's reply, and rewrites the Chitt's own text.
fn announce(chitt_id: &str) -> Result<(), HookError> {
    let options = options()?;
    let ids: Vec<String> = options.iter().map(|(id, _)| id.clone()).collect();
    let (label, votes) = options
        .iter()
        .zip(tallies(&ids))
        .max_by_key(|(_, votes)| *votes)
        .map(|((_, label), votes)| (label.clone(), votes))
        .ok_or_else(|| fail("invalid", "the poll has no options"))?;
    let noun = if votes == 1 { "vote" } else { "votes" };
    post_reply(chitt_id, &format!("The poll is closed. {label} leads with {votes} {noun}."))?;
    edit_chitt(&format!("{QUESTION} Closed: {label} won."))
}

/// The poll's options from `assets/toppings.txt`: each option's id and its label, in file order.
fn options() -> Result<Vec<(String, String)>, HookError> {
    let bytes = asset("toppings.txt")?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|label| (slug(label), label.to_string()))
        .collect())
}

fn slug(label: &str) -> String {
    label.to_lowercase().split_whitespace().collect::<Vec<_>>().join("-")
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
    HookError { code: code.to_string(), message: message.into() }
}

/// A KV, timer or Render Push failure: the host's code and message.
fn kv_fail(error: kv::KvError) -> HookError {
    fail(&error.code, error.message)
}

fn poster_fail(error: poster::PosterError) -> HookError {
    fail(&error.code, error.message)
}

fn asset_fail(error: host::AssetError) -> HookError {
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
    poster::post_reply(post_id, content).map(|_| ()).map_err(poster_fail)
}

/// Rewrites the Chitt's own text, as its poster.
fn edit_chitt(content: &str) -> Result<(), HookError> {
    let args = poster::EditArgs { content: content.to_string(), ..Default::default() };
    poster::edit_chitt(args).map_err(poster_fail)
}

/// Queues a Render Push for every Observer of this Chitt.
fn push_render_all() -> Result<(), HookError> {
    push::push_render(push::PushTarget::All).map_err(kv_fail)
}

/// Reads one of the Cartridge's files under `assets/`.
fn asset(path: &str) -> Result<Vec<u8>, HookError> {
    host::asset(path).map_err(asset_fail)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
