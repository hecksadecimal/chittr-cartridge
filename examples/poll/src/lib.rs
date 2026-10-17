//! The poll Cartridge's Server Guest, written against the bindings `xtp plugin init` generates from
//! `docs/cartridges/v1/server.xtp.yaml` (see the README). `init` gives the Chitt its text and schedules a timer
//! that closes the poll. `on_timer` marks it closed in the KV store when that timer fires. `render` reads the
//! options from `assets/toppings.txt` with `asset()`, counts one view in the Chitt's KV store, and draws the card
//! from `templates/card.liquid` with each option's tally and the viewer's own vote (and "closed" once the poll is).
//! The same tallies go to the Canvas Guest as `canvas` state, which draws them as a bar chart.
//!
//! The `vote` Control takes one option id. `on_control` keeps one vote per Character, under
//! `vote:<characterId>`, and keeps the tallies under `tally:<option id>`. The manifest's `vote` enum must
//! list the same ids as the options in `toppings.txt`: each id is the label in lowercase, spaces as "-".
//!
//! The `on_reply` gate refuses replies once the poll is closed, with a message the composer shows.
//!
//! The `close` Control is the poster's. It closes the poll once: it announces the leading option
//! as a reply from the poster, and rewrites the Chitt's own text with `edit_chitt`. Its card button is not
//! in the template yet, so today it is reached through the Control endpoint only.
//!
//! The generated bindings name every Hook, so `src/pdk.rs` exports only the Hooks defined here (see the README).
//! Failures are returned as the output's `error` field, never as a trap, as the ABI asks.

mod pdk;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use extism_pdk::Error;
use pdk::types::*;
use serde::Serialize;
use serde_json::{json, Map, Value};

const QUESTION: &str = "Which pizza topping is best?";
const VIEWS_KEY: &str = "views";
const VOTE_CONTROL: &str = "vote";
const CLOSE_CONTROL: &str = "close";
/// The key both closers write: the close timer (`on_timer`) and the poster's `close` Control.
const CLOSED_KEY: &str = "closed";
const CLOSE_TIMER: &str = "close";
/// The poll closes an hour after it is published.
const CLOSES_AFTER_SECONDS: i32 = 60 * 60;

/// Publishing is refused when the close timer can't be scheduled: a poll that never closes is a bug.
pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(match schedule_close_timer() {
        Ok(()) => InitOutput { content: QUESTION.to_string(), ..Default::default() },
        Err(error) => InitOutput { content: String::new(), error: Some(error), ..Default::default() },
    })
}

fn schedule_close_timer() -> Result<(), HookError> {
    let reply = pdk::schedule_timer(ScheduleTimerInput {
        name: CLOSE_TIMER.to_string(),
        delay_seconds: CLOSES_AFTER_SECONDS,
        payload: None,
    })
    .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => Ok(()),
    }
}

/// The close timer fired: record it in the KV store, where `render` and `on_control` read it. Timers are at
/// least once, so this only ever writes the same value again.
pub(crate) fn on_timer(input: TimerInput) -> Result<TimerOutput, Error> {
    let result = if input.name == CLOSE_TIMER { kv_write(CLOSED_KEY, b"1") } else { Ok(()) };
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

/// Closes the poll, as its poster, once. A second close changes nothing.
fn close(input: ControlInput) -> Result<(), HookError> {
    let observer = input.observer.ok_or_else(|| fail("logged_out", "log in to close the poll"))?;
    if !observer.is_poster {
        return Err(fail("forbidden", "only the poster can close the poll"));
    }
    if !kv_set_if_absent(CLOSED_KEY, b"1")? {
        return Ok(());
    }
    let options = options()?;
    let ids: Vec<String> = options.iter().map(|(id, _)| id.clone()).collect();
    let (label, votes) = options
        .iter()
        .zip(tallies(&ids))
        .max_by_key(|(_, votes)| *votes)
        .map(|((_, label), votes)| (label.clone(), votes))
        .ok_or_else(|| fail("invalid", "the poll has no options"))?;
    let noun = if votes == 1 { "vote" } else { "votes" };
    post_reply(&input.chitt.id, &format!("The poll is closed. {label} leads with {votes} {noun}."))?;
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
    match kv_read_many(&keys) {
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
    if total <= 0 { 0 } else { votes * 100 / total }
}

// Host calls. Each returns the host's failure as a HookError with the code the SDK reports for it, so the
// Hooks above hand it back in their `error` field.

/// A failure the Hook reports itself: a code and a message.
fn fail(code: &str, message: impl Into<String>) -> HookError {
    HookError { code: code.to_string(), message: message.into() }
}

/// The host call itself failed, before it answered.
fn host_failed(error: Error) -> HookError {
    fail("unavailable", error.to_string())
}

/// The code of a generated error enum, as its wire string.
fn code_of<T: Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn kv_error(error: KvError) -> HookError {
    fail(&code_of(&error.code), error.message)
}

fn kv_read(key: &str) -> Result<Option<Vec<u8>>, HookError> {
    let reply = pdk::kv_get(KvKeyInput { key: key.to_string() }).map_err(host_failed)?;
    if let Some(error) = reply.error {
        return Err(kv_error(error));
    }
    reply.value.map(|value| decode(&value)).transpose()
}

/// Reads several keys in one host call. The result lines up with `keys`; `None` means no live value.
fn kv_read_many(keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, HookError> {
    let reply = pdk::kv_mget(KvMgetInput { keys: keys.to_vec() }).map_err(host_failed)?;
    if let Some(error) = reply.error {
        return Err(kv_error(error));
    }
    let entries = reply.entries.ok_or_else(malformed)?;
    if entries.len() != keys.len() {
        return Err(fail("invalid", "the host answered with the wrong number of entries"));
    }
    entries.iter().map(|entry| entry.value.as_deref().map(decode).transpose()).collect()
}

fn kv_write(key: &str, value: &[u8]) -> Result<(), HookError> {
    let reply = pdk::kv_set(KvSetInput { key: key.to_string(), value: STANDARD.encode(value), ttl_seconds: None })
        .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => Ok(()),
    }
}

/// Sets `key` only if it has no live value. `Ok(false)` means it already had one.
fn kv_set_if_absent(key: &str, value: &[u8]) -> Result<bool, HookError> {
    let reply = pdk::kv_set_if_absent(KvSetInput {
        key: key.to_string(),
        value: STANDARD.encode(value),
        ttl_seconds: None,
    })
    .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => reply.set.ok_or_else(malformed),
    }
}

fn kv_incr(key: &str, by: i64) -> Result<i64, HookError> {
    let reply = pdk::kv_incr(KvIncrInput { key: key.to_string(), by: Some(by), ttl_seconds: None }).map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => reply.value.ok_or_else(malformed),
    }
}

/// A host reply that lacks the field its call promises.
fn malformed() -> HookError {
    fail("invalid", "the host's reply is missing a field")
}

fn decode(text: &str) -> Result<Vec<u8>, HookError> {
    STANDARD.decode(text).map_err(|_| fail("invalid", "a stored value is not base64"))
}

/// Records a vote or a close: the poster's reply to the Chitt, through its Poster Action.
fn post_reply(post_id: &str, content: &str) -> Result<(), HookError> {
    let reply = pdk::post_reply(PostReplyInput {
        post_id: post_id.to_string(),
        content: content.to_string(),
        ooc: None,
    })
    .map_err(host_failed)?;
    poster_result(reply.error)
}

/// Rewrites the Chitt's own text, as its poster.
fn edit_chitt(content: &str) -> Result<(), HookError> {
    let reply = pdk::edit_chitt(EditChittInput {
        content: content.to_string(),
        ooc: None,
        post_id: None,
        quirks: None,
    })
    .map_err(host_failed)?;
    poster_result(reply.error)
}

fn poster_result(error: Option<PosterError>) -> Result<(), HookError> {
    match error {
        Some(error) => Err(fail(&code_of(&error.code), error.message)),
        None => Ok(()),
    }
}

/// Queues a Render Push for every Observer of this Chitt.
fn push_render_all() -> Result<(), HookError> {
    let reply = pdk::push_render(PushRenderInput { character_id: None }).map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(fail(&code_of(&error.code), error.message)),
        None => Ok(()),
    }
}

/// Reads one of the Cartridge's files under `assets/`.
fn asset(path: &str) -> Result<Vec<u8>, HookError> {
    let reply = pdk::asset(AssetInput { path: path.to_string() })
        .map_err(|error| fail("host", error.to_string()))?;
    if let Some(error) = reply.error {
        return Err(fail(&code_of(&error.code), error.message));
    }
    let encoded = reply.bytes.ok_or_else(|| fail("invalid", "the host sent no bytes"))?;
    decode(&encoded)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
