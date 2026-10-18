//! The in-memory host behind `crate::testing`. It follows the rules of the Chittr host: key, value and timer
//! sizes, the KV quotas, TTLs, the host-call cap per Hook call, the Poster Action quotas and deferrals, and the
//! Render Push and timer rules. It does not model timing, concurrency, permissions, or the Chitt's own state.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::envelope::{ChittRef, TimerInput};
use crate::logging::LogLevel;
use crate::push::PushTarget;

/// The clock every fresh test starts at: 2023-11-14T22:13:20Z.
pub(crate) const START_TIME: u64 = 1_700_000_000;

const KEY_BYTES_MAX: usize = 1024;
const VALUE_BYTES_MAX: usize = 1024 * 1024;
const KEYS_MAX: usize = 1_000_000;
const BYTES_MAX: usize = 64 * 1024 * 1024;
const LIST_MAX: usize = 1000;
const READ_BYTES_MAX: usize = 4 * 1024 * 1024;
const TTL_MAX: u64 = i32::MAX as u64;
const BY_MAX: i64 = 1 << 53;
const TIMER_NAME_BYTES_MAX: usize = 256;
const TIMER_PAYLOAD_BYTES_MAX: usize = 16 * 1024;
const TIMER_OUTSTANDING_MAX: usize = 100;
const TIMER_DELAY_MIN: u64 = 10;
const TIMER_DELAY_MAX: u64 = 30 * 24 * 60 * 60;
const PUSH_TARGETS_MAX: usize = 20;
const ACTIONS_PER_MINUTE: usize = 30;
const ASSET_BYTES_MAX: usize = 4 * 1024 * 1024;
const FILE_READ_MAX: usize = 64 * 1024;

/// The kind of Hook a test is running, which sets the limits its host calls face. Set it with
/// `TestHost::begin_hook`. The default, `Background`, is the class of `on_timer`, `on_view` and the canvas hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookClass {
    /// `init` and `migrate`: Poster Actions are deferred until the Chitt commits.
    Lifecycle,
    /// `render`: 100 host calls, one Poster Action, and no Render Pushes.
    Render,
    /// `on_control`.
    Control,
    /// `on_reply`, `on_reaction` and `on_rechitt` when they gate: Poster Actions are deferred.
    Gate,
    /// `on_timer`, `on_view` and `on_canvas_message`.
    #[default]
    Background,
}

impl HookClass {
    fn host_call_limit(self) -> u32 {
        match self {
            HookClass::Render => 100,
            _ => 500,
        }
    }

    fn action_limit(self) -> u32 {
        match self {
            HookClass::Render => 1,
            _ => 5,
        }
    }

    fn defers_actions(self) -> bool {
        matches!(self, HookClass::Lifecycle | HookClass::Gate)
    }
}

/// One Poster Action the guest made: the host function, its request, the reply the guest saw, and when.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedAction {
    pub action: String,
    pub args: Value,
    pub reply: Value,
    pub at: u64,
}

/// A timer that is scheduled and has not fired yet.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingTimer {
    pub name: String,
    pub due_at: u64,
    pub payload: Value,
}

/// One log line the guest wrote, as the host keeps it (cut to 512 bytes by the SDK).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedLog {
    pub level: LogLevel,
    pub message: String,
}

struct Entry {
    value: Vec<u8>,
    expires_at: Option<u64>,
}

struct Timer {
    due: u64,
    payload: Value,
}

pub(crate) struct State {
    now: u64,
    class: HookClass,
    host_calls: u32,
    call_actions: u32,
    call_pushes: BTreeSet<Option<String>>,
    kv: BTreeMap<String, Entry>,
    timers: BTreeMap<String, Timer>,
    assets: BTreeMap<String, Vec<u8>>,
    files: BTreeMap<String, Vec<u8>>,
    refusals: BTreeMap<String, (String, String)>,
    actions: Vec<RecordedAction>,
    pushes: Vec<PushTarget>,
    logs: Vec<RecordedLog>,
    next_id: u64,
}

impl State {
    fn fresh() -> Self {
        Self {
            now: START_TIME,
            class: HookClass::default(),
            host_calls: 0,
            call_actions: 0,
            call_pushes: BTreeSet::new(),
            kv: BTreeMap::new(),
            timers: BTreeMap::new(),
            assets: BTreeMap::new(),
            files: BTreeMap::new(),
            refusals: BTreeMap::new(),
            actions: Vec::new(),
            pushes: Vec::new(),
            logs: Vec::new(),
            next_id: 1,
        }
    }

    fn begin_hook(&mut self, class: HookClass) {
        self.class = class;
        self.host_calls = 0;
        self.call_actions = 0;
        self.call_pushes.clear();
    }

    /// Drops the entries whose expiry has passed. Every KV call starts with it.
    fn prune(&mut self) {
        let now = self.now;
        self.kv
            .retain(|_, entry| entry.expires_at.is_none_or(|at| at > now));
    }

    fn kv_bytes(&self) -> usize {
        self.kv
            .iter()
            .map(|(key, entry)| key.len() + entry.value.len())
            .sum()
    }

    /// Stores `value` under `key`. With no `ttl` the key keeps its expiry, as the host does.
    fn write(&mut self, key: &str, value: Vec<u8>, ttl: Option<u64>) -> Result<(), Value> {
        if value.len() > VALUE_BYTES_MAX {
            return Err(fail("value_too_large", "a value is at most 1048576 bytes"));
        }
        let old = self
            .kv
            .get(key)
            .map(|entry| (key.len() + entry.value.len(), entry.expires_at));
        let keys = self.kv.len() + usize::from(old.is_none());
        let bytes = self.kv_bytes() - old.map_or(0, |(size, _)| size) + key.len() + value.len();
        if keys > KEYS_MAX || bytes > BYTES_MAX {
            return Err(fail(
                "quota_exceeded",
                "this Live Chitt's KV store is full (1,000,000 keys or 64 MiB)",
            ));
        }
        let expires_at = match ttl {
            Some(seconds) => Some(self.now + seconds),
            None => old.and_then(|(_, expires_at)| expires_at),
        };
        self.kv.insert(key.to_string(), Entry { value, expires_at });
        Ok(())
    }

    fn call(&mut self, name: &str, request: &str) -> Value {
        let args: Value = serde_json::from_str(request).unwrap_or(Value::Null);
        if name != "asset" {
            self.host_calls += 1;
            let limit = self.class.host_call_limit();
            if self.host_calls > limit {
                return fail(
                    "rate_limited",
                    format!("this Hook call has made more than {limit} host calls"),
                );
            }
        }
        match name {
            "kv_get" => self.kv_get(&args),
            "kv_set" => self.kv_set(&args),
            "kv_set_if_absent" => self.kv_set_if_absent(&args),
            "kv_compare_and_swap" => self.kv_compare_and_swap(&args),
            "kv_delete" => self.kv_delete(&args),
            "kv_incr" => self.kv_incr(&args),
            "kv_list" => self.kv_list(&args),
            "kv_mget" => self.kv_mget(&args),
            "kv_get_all" => self.kv_get_all(),
            "schedule_timer" => self.schedule_timer(&args),
            "cancel_timer" => self.cancel_timer(&args),
            "push_render" => self.push_render(&args),
            "asset" => self.asset(&args),
            "file_read" => self.file_read(&args),
            "file_close" => self.file_close(&args),
            _ => self.poster_action(name, args),
        }
    }

    fn kv_get(&mut self, args: &Value) -> Value {
        let key = match key_of(args) {
            Ok(key) => key,
            Err(error) => return error,
        };
        self.prune();
        match self.kv.get(&key) {
            None => json!({ "value": null }),
            Some(entry) => {
                let mut reply = json!({ "value": STANDARD.encode(&entry.value) });
                if let Some(at) = entry.expires_at {
                    reply["ttlSeconds"] = json!(at - self.now);
                }
                reply
            }
        }
    }

    fn kv_set(&mut self, args: &Value) -> Value {
        self.prune();
        let stored = key_of(args).and_then(|key| {
            let value = bytes_of(args, "value")?;
            let ttl = ttl_of(args)?;
            self.write(&key, value, ttl)
        });
        stored.map_or_else(|error| error, |()| json!({}))
    }

    fn kv_set_if_absent(&mut self, args: &Value) -> Value {
        self.prune();
        let set = key_of(args).and_then(|key| {
            let value = bytes_of(args, "value")?;
            let ttl = ttl_of(args)?;
            if self.kv.contains_key(&key) {
                return Ok(false);
            }
            self.write(&key, value, ttl).map(|()| true)
        });
        set.map_or_else(|error| error, |set| json!({ "set": set }))
    }

    fn kv_compare_and_swap(&mut self, args: &Value) -> Value {
        self.prune();
        let swapped = key_of(args).and_then(|key| {
            let expected = match args.get("expected") {
                None | Some(Value::Null) => None,
                Some(_) => Some(bytes_of(args, "expected")?),
            };
            let value = bytes_of(args, "value")?;
            let current = self.kv.get(&key).map(|entry| entry.value.clone());
            if current != expected {
                return Ok(false);
            }
            self.write(&key, value, None).map(|()| true)
        });
        swapped.map_or_else(|error| error, |swapped| json!({ "swapped": swapped }))
    }

    fn kv_delete(&mut self, args: &Value) -> Value {
        self.prune();
        match key_of(args) {
            Ok(key) => json!({ "deleted": self.kv.remove(&key).is_some() }),
            Err(error) => error,
        }
    }

    fn kv_incr(&mut self, args: &Value) -> Value {
        self.prune();
        let value = key_of(args).and_then(|key| {
            let by = args.get("by").and_then(Value::as_i64).unwrap_or(1);
            if by.checked_abs().is_none_or(|magnitude| magnitude > BY_MAX) {
                return Err(fail(
                    "invalid_argument",
                    "by must be within 2^53 in magnitude",
                ));
            }
            let ttl = ttl_of(args)?;
            let current = match self.kv.get(&key) {
                None => 0,
                Some(entry) => std::str::from_utf8(&entry.value)
                    .ok()
                    .and_then(|text| text.parse::<i64>().ok())
                    .ok_or_else(|| {
                        fail("not_integer", "the value at this key is not an integer")
                    })?,
            };
            let next = current
                .checked_add(by)
                .ok_or_else(|| fail("invalid_argument", "the counter would overflow"))?;
            self.write(&key, next.to_string().into_bytes(), ttl)?;
            Ok(next)
        });
        value.map_or_else(|error| error, |value| json!({ "value": value }))
    }

    fn kv_list(&mut self, args: &Value) -> Value {
        self.prune();
        let prefix = args.get("prefix").and_then(Value::as_str).unwrap_or("");
        let keys: Vec<&String> = self
            .kv
            .keys()
            .filter(|key| key.starts_with(prefix))
            .collect();
        let truncated = keys.len() > LIST_MAX;
        let keys: Vec<&String> = keys.into_iter().take(LIST_MAX).collect();
        json!({ "keys": keys, "truncated": truncated })
    }

    fn kv_mget(&mut self, args: &Value) -> Value {
        self.prune();
        let keys: Vec<String> = match args.get("keys").and_then(Value::as_array) {
            Some(keys) => keys
                .iter()
                .filter_map(|key| key.as_str().map(str::to_string))
                .collect(),
            None => return invalid("keys must be a list of strings"),
        };
        if keys.len() > LIST_MAX {
            return invalid("keys holds at most 1000 keys");
        }
        let values: Vec<Option<&Vec<u8>>> = keys
            .iter()
            .map(|key| self.kv.get(key).map(|e| &e.value))
            .collect();
        let total: usize = values.iter().flatten().map(|value| value.len()).sum();
        if total > READ_BYTES_MAX {
            return fail(
                "invalid_argument",
                "the values of these keys total more than 4194304 bytes; read fewer keys per call",
            );
        }
        let entries: Vec<Value> = values
            .into_iter()
            .map(|value| json!({ "value": value.map(|bytes| STANDARD.encode(bytes)) }))
            .collect();
        json!({ "entries": entries })
    }

    fn kv_get_all(&mut self) -> Value {
        self.prune();
        let mut entries = Vec::new();
        let mut total = 0;
        let mut truncated = false;
        for (key, entry) in &self.kv {
            if entries.len() == LIST_MAX || total + entry.value.len() > READ_BYTES_MAX {
                truncated = true;
                break;
            }
            total += entry.value.len();
            entries.push(json!({ "key": key, "value": STANDARD.encode(&entry.value) }));
        }
        json!({ "entries": entries, "truncated": truncated })
    }

    fn schedule_timer(&mut self, args: &Value) -> Value {
        let name = match args.get("name").and_then(Value::as_str) {
            Some(name) if name.len() <= TIMER_NAME_BYTES_MAX => name.to_string(),
            _ => return invalid("name is a string of at most 256 bytes"),
        };
        let delay = match args.get("delaySeconds").and_then(Value::as_u64) {
            Some(delay) if (TIMER_DELAY_MIN..=TIMER_DELAY_MAX).contains(&delay) => delay,
            _ => return invalid("delaySeconds must be from 10 to 2592000"),
        };
        let payload = args.get("payload").cloned().unwrap_or(Value::Null);
        if serde_json::to_string(&payload).map_or(true, |json| json.len() > TIMER_PAYLOAD_BYTES_MAX)
        {
            return invalid("payload is at most 16384 bytes of JSON");
        }
        if !self.timers.contains_key(&name) && self.timers.len() >= TIMER_OUTSTANDING_MAX {
            return fail(
                "quota_exceeded",
                "a Live Chitt holds at most 100 timers outstanding",
            );
        }
        let due = self.now + delay;
        self.timers.insert(name, Timer { due, payload });
        json!({})
    }

    fn cancel_timer(&mut self, args: &Value) -> Value {
        match args.get("name").and_then(Value::as_str) {
            Some(name) => json!({ "cancelled": self.timers.remove(name).is_some() }),
            None => invalid("name must be a string"),
        }
    }

    fn push_render(&mut self, args: &Value) -> Value {
        if self.class == HookClass::Render {
            return fail("not_allowed", "push_render is not allowed in render");
        }
        let target = args
            .get("characterId")
            .and_then(Value::as_str)
            .map(str::to_string);
        if !self.call_pushes.contains(&target) {
            if self.call_pushes.len() >= PUSH_TARGETS_MAX {
                return fail(
                    "quota_exceeded",
                    "a Hook call may queue at most 20 distinct push targets",
                );
            }
            self.call_pushes.insert(target.clone());
        }
        self.pushes.push(match target {
            None => PushTarget::All,
            Some(id) => PushTarget::Character(id),
        });
        json!({})
    }

    fn asset(&mut self, args: &Value) -> Value {
        let path = args.get("path").and_then(Value::as_str).unwrap_or("");
        let bad_path =
            path.is_empty() || path.starts_with('/') || path.split('/').any(|part| part == "..");
        if bad_path {
            return fail("not_found", "no such file, or a bad path");
        }
        match self.assets.get(path) {
            None => fail("not_found", format!("there is no asset at {path}")),
            Some(bytes) if bytes.len() > ASSET_BYTES_MAX => {
                fail("too_large", "the asset is over 4 MiB")
            }
            Some(bytes) => json!({ "bytes": STANDARD.encode(bytes) }),
        }
    }

    /// `file_read` (ABI v2): one bounded chunk of a delivered file, clipped at EOF.
    fn file_read(&mut self, args: &Value) -> Value {
        let handle = match args.get("handle").and_then(Value::as_str) {
            Some(handle) => handle,
            None => return fail("invalid", "handle must be a string"),
        };
        let Some(bytes) = self.files.get(handle) else {
            return fail("not_found", "no such file handle");
        };
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
        let length = args.get("length").and_then(Value::as_u64).unwrap_or(0);
        if length == 0 || length > FILE_READ_MAX as u64 {
            return fail("too_large", "length must be an integer from 1 to 65536");
        }
        let size = bytes.len() as u64;
        if offset > size {
            return fail("invalid", "offset is past the end of the file");
        }
        let start = offset as usize;
        let end = (offset + length).min(size) as usize;
        let chunk = &bytes[start..end];
        json!({ "bytes": STANDARD.encode(chunk), "eof": end as u64 == size })
    }

    /// `file_close` (ABI v2): releases a handle; unknown handles are harmless.
    fn file_close(&mut self, args: &Value) -> Value {
        if let Some(handle) = args.get("handle").and_then(Value::as_str) {
            self.files.remove(handle);
        }
        json!({})
    }

    fn poster_action(&mut self, name: &str, args: Value) -> Value {
        let limit = self.class.action_limit();
        let minute_start = self.now.saturating_sub(60);
        let recent = self
            .actions
            .iter()
            .filter(|action| {
                action.at >= minute_start
                    && action.reply.get("error").and_then(|e| e.get("code"))
                        != Some(&json!("rate_limited"))
            })
            .count();
        let reply = if self.call_actions >= limit {
            fail(
                "rate_limited",
                format!("this Hook call has made more than {limit} Poster Actions"),
            )
        } else if recent >= ACTIONS_PER_MINUTE {
            fail(
                "rate_limited",
                "this Chitt has made 30 Poster Actions in the last minute",
            )
        } else {
            self.call_actions += 1;
            self.run_action(name)
        };
        self.actions.push(RecordedAction {
            action: name.to_string(),
            args,
            reply: reply.clone(),
            at: self.now,
        });
        reply
    }

    fn run_action(&mut self, name: &str) -> Value {
        if self.class.defers_actions() {
            return json!({ "ok": true, "deferred": true });
        }
        if let Some((code, message)) = self.refusals.get(name) {
            return fail(code, message.clone());
        }
        let id = self.next_id;
        match name {
            "post_reply" | "post_chitt" => {
                self.next_id += 1;
                json!({ "ok": true, "postId": format!("post-{id}") })
            }
            "rechitt" => {
                self.next_id += 1;
                json!({ "ok": true, "rechittId": format!("rechitt-{id}") })
            }
            "react" => json!({ "ok": true, "action": "added" }),
            "unreact" => json!({ "ok": true, "action": "removed" }),
            "follow" => json!({ "ok": true, "action": "followed" }),
            "unfollow" => json!({ "ok": true, "action": "unfollowed" }),
            _ => json!({ "ok": true }),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::fresh());
}

/// Runs `f` on this thread's fake host.
pub(crate) fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|state| f(&mut state.borrow_mut()))
}

/// Replaces this thread's fake host with a fresh one.
pub(crate) fn reset() {
    with(|state| *state = State::fresh());
}

/// Answers one host function call with its reply JSON. Called by the SDK's host functions under `testing`.
#[doc(hidden)]
pub fn host_call(name: &str, request: &str) -> String {
    with(|state| state.call(name, request)).to_string()
}

/// Records one guest log line. Called by `log!` under `testing`.
#[doc(hidden)]
pub fn record_log(level: LogLevel, message: &str) {
    with(|state| {
        state.logs.push(RecordedLog {
            level,
            message: message.to_string(),
        })
    });
}

/// The fake's clock, in seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    with(|state| state.now)
}

pub(crate) fn set_now(seconds: u64) {
    with(|state| state.now = seconds);
}

pub(crate) fn set_class(class: HookClass) {
    with(|state| state.begin_hook(class));
}

pub(crate) fn put_kv(
    key: &str,
    value: Vec<u8>,
    ttl: Option<u64>,
) -> Result<(), crate::kv::KvError> {
    with(|state| {
        state.prune();
        state.write(key, value, ttl)
    })
    .map_err(|error| kv_error(&error))
}

pub(crate) fn kv_entry(key: &str) -> Option<(Vec<u8>, Option<u64>)> {
    with(|state| {
        state.prune();
        let now = state.now;
        state
            .kv
            .get(key)
            .map(|entry| (entry.value.clone(), entry.expires_at.map(|at| at - now)))
    })
}

pub(crate) fn kv_keys() -> Vec<String> {
    with(|state| {
        state.prune();
        state.kv.keys().cloned().collect()
    })
}

pub(crate) fn add_asset(path: &str, bytes: Vec<u8>) {
    with(|state| state.assets.insert(path.to_string(), bytes));
}

/// Registers a delivered file under `handle`, as a picker would (ABI v2).
pub(crate) fn put_file(handle: &str, bytes: Vec<u8>) {
    with(|state| state.files.insert(handle.to_string(), bytes));
}

pub(crate) fn timers() -> Vec<PendingTimer> {
    with(|state| {
        let mut timers: Vec<PendingTimer> = state
            .timers
            .iter()
            .map(|(name, timer)| PendingTimer {
                name: name.clone(),
                due_at: timer.due,
                payload: timer.payload.clone(),
            })
            .collect();
        timers.sort_by_key(|timer| timer.due_at);
        timers
    })
}

/// Removes the named timer, moves the clock to its due time, and returns the `on_timer` input for it.
pub(crate) fn fire_timer(chitt: &ChittRef, name: &str) -> Option<TimerInput> {
    let (due, payload) = with(|state| {
        let timer = state.timers.remove(name)?;
        state.now = state.now.max(timer.due);
        Some((timer.due, timer.payload))
    })?;
    Some(crate::testing::envelopes::timer_due(
        chitt, name, payload, due,
    ))
}

/// Fires every timer that is due at the current clock, earliest first.
pub(crate) fn fire_due(chitt: &ChittRef) -> Vec<TimerInput> {
    let now = now();
    let mut due: Vec<(u64, String)> = with(|state| {
        state
            .timers
            .iter()
            .filter(|(_, timer)| timer.due <= now)
            .map(|(name, timer)| (timer.due, name.clone()))
            .collect()
    });
    due.sort();
    due.into_iter()
        .filter_map(|(_, name)| fire_timer(chitt, &name))
        .collect()
}

pub(crate) fn refuse_action(action: &str, code: &str, message: &str) {
    with(|state| {
        state
            .refusals
            .insert(action.to_string(), (code.to_string(), message.to_string()));
    });
}

pub(crate) fn allow_action(action: &str) {
    with(|state| state.refusals.remove(action));
}

pub(crate) fn actions() -> Vec<RecordedAction> {
    with(|state| state.actions.clone())
}

pub(crate) fn pushes() -> Vec<PushTarget> {
    with(|state| state.pushes.clone())
}

pub(crate) fn logs() -> Vec<RecordedLog> {
    with(|state| state.logs.clone())
}

fn fail(code: &str, message: impl Into<String>) -> Value {
    json!({ "error": { "code": code, "message": message.into() } })
}

fn invalid(message: &str) -> Value {
    fail("invalid_argument", message)
}

fn key_of(args: &Value) -> Result<String, Value> {
    let key = args
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("key must be a string"))?;
    if key.len() > KEY_BYTES_MAX {
        return Err(fail("key_too_large", "a key is at most 1024 bytes"));
    }
    Ok(key.to_string())
}

fn bytes_of(args: &Value, field: &str) -> Result<Vec<u8>, Value> {
    args.get(field)
        .and_then(Value::as_str)
        .and_then(|text| STANDARD.decode(text).ok())
        .ok_or_else(|| invalid(&format!("{field} must be base64")))
}

fn ttl_of(args: &Value) -> Result<Option<u64>, Value> {
    match args.get("ttlSeconds") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_u64() {
            Some(seconds) if (1..=TTL_MAX).contains(&seconds) => Ok(Some(seconds)),
            _ => Err(invalid(
                "ttlSeconds must be an integer from 1 to 2147483647",
            )),
        },
    }
}

fn kv_error(error: &Value) -> crate::kv::KvError {
    let code = error["error"]["code"].as_str().unwrap_or("unavailable");
    let message = error["error"]["message"]
        .as_str()
        .unwrap_or("the fake host refused the write");
    crate::kv::KvError {
        code: code.to_string(),
        message: message.to_string(),
    }
}
