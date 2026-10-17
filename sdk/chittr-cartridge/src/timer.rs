//! Timers for Server Guests. A Hook schedules a named timer, and the host runs the Server
//! Guest's `on_timer` Hook when it fires, at least once.
//!
//! `schedule_timer` takes a delay of 10 seconds to 30 days. Scheduling a name that already has a timer
//! replaces it. A Chitt may hold at most 100 timers outstanding; a new name past that fails with the
//! code `quota_exceeded`. The payload is JSON of at most 16 KiB and comes back as `payload` in `on_timer`.
//! Timers are held while their Chitt is disabled, and on re-enable each overdue timer fires once.
//!
//! Failures use the KV error codes (see `kv`): `invalid_argument`, `quota_exceeded`, `unavailable`,
//! `timeout`, `rate_limited`. The codes are reported as `TimerError`, the same type as `KvError`.

use crate::kv::{call, flag};
use serde::Serialize;
use serde_json::json;

mod imports {
    use extism_pdk::host_fn;

    #[host_fn]
    extern "ExtismHost" {
        pub fn schedule_timer(request: String) -> String;
        pub fn cancel_timer(request: String) -> String;
    }
}

pub use crate::kv::{KvError as TimerError, KvResult as TimerResult};

/// Schedules `name` to fire after `delay_seconds`, replacing any timer of that name. `payload` is
/// serialized to JSON and returned to `on_timer`; pass `Value::Null` for none.
pub fn schedule_timer(name: &str, delay_seconds: u64, payload: impl Serialize) -> TimerResult<()> {
    let payload = serde_json::to_value(payload).map_err(|error| TimerError {
        code: "invalid_argument".to_string(),
        message: error.to_string(),
    })?;
    let request = json!({ "name": name, "delaySeconds": delay_seconds, "payload": payload });
    call(unsafe { imports::schedule_timer(request.to_string()) })?;
    Ok(())
}

/// Cancels the timer named `name`. Returns whether one was outstanding.
pub fn cancel_timer(name: &str) -> TimerResult<bool> {
    let reply = call(unsafe { imports::cancel_timer(json!({ "name": name }).to_string()) })?;
    flag(&reply, "cancelled")
}
