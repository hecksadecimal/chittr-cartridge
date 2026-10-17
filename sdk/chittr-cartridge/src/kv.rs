//! The Chitt KV store for Server Guests. Each Chitt has its own store; a guest
//! can't name another Chitt's keys.
//!
//! Keys are UTF-8 strings of at most 1 KiB. Values are bytes of at most 1 MiB. Counters are `i64`.
//! A write with `ttl_seconds` sets or replaces the key's expiry; a write without one keeps it.
//!
//! The wire ABI is one JSON object in and one out, per host function. Every function here is one host
//! call, and a Hook call may make 100 of them in `render` and 500 in any other Hook. Past
//! that, calls fail with the code `rate_limited`. Host calls cost about 0.5 ms each and slow down inside
//! one Hook call, so prefer `get_many` and `get_all` to a loop of `get`s.
//!
//! Failures come back as `KvError` and never trap the guest. A lost race (`set_if_absent` or
//! `compare_and_swap` that doesn't win) is `Ok(false)`, not an error.
//!
//! Error codes: `unavailable`, `timeout`, `quota_exceeded`, `key_too_large`, `value_too_large`,
//! `not_integer`, `rate_limited`, `invalid_argument`. Quotas per Chitt: 1,000,000 keys and 64 MiB.
//! `list` and `get_all` return at most 1,000 entries and say so with `truncated`.
//! `by` in `incr` must be within 2^53 in magnitude, since the JSON reader on the host is a JS number.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use extism_pdk::host_fn;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[host_fn]
extern "ExtismHost" {
    fn kv_get(request: String) -> String;
    fn kv_set(request: String) -> String;
    fn kv_set_if_absent(request: String) -> String;
    fn kv_compare_and_swap(request: String) -> String;
    fn kv_delete(request: String) -> String;
    fn kv_incr(request: String) -> String;
    fn kv_list(request: String) -> String;
    fn kv_mget(request: String) -> String;
    fn kv_get_all(request: String) -> String;
}

/// A KV failure. `code` is one of the codes in the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvError {
    pub code: String,
    pub message: String,
}

impl KvError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for KvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for KvError {}

pub type KvResult<T> = Result<T, KvError>;

/// A stored value and its remaining TTL in whole seconds (`None` when it has no expiry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub value: Vec<u8>,
    pub ttl_seconds: Option<u64>,
}

/// The keys starting with a prefix. `truncated` is true when there were more than 1,000.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub keys: Vec<String>,
    pub truncated: bool,
}

/// Every entry of the store, by key. `truncated` is true when there were more than 1,000.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub entries: BTreeMap<String, Vec<u8>>,
    pub truncated: bool,
}

/// Reads `key`: its value and remaining TTL, or `None` when it has no live value.
pub fn get(key: &str) -> KvResult<Option<Entry>> {
    let reply = call(unsafe { kv_get(json!({ "key": key }).to_string()) })?;
    match reply.get("value") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => parse_entry(&reply).map(Some),
    }
}

/// Sets `key` to `value`. Pass `ttl_seconds` to set or replace the expiry; `None` keeps it.
pub fn set(key: &str, value: impl AsRef<[u8]>, ttl_seconds: Option<u64>) -> KvResult<()> {
    call(unsafe {
        kv_set(json!({ "key": key, "value": b64(value), "ttlSeconds": ttl_seconds }).to_string())
    })?;
    Ok(())
}

/// Sets `key` only if it has no live value. `Ok(false)` means it already had one.
pub fn set_if_absent(
    key: &str,
    value: impl AsRef<[u8]>,
    ttl_seconds: Option<u64>,
) -> KvResult<bool> {
    let reply = call(unsafe {
        kv_set_if_absent(
            json!({ "key": key, "value": b64(value), "ttlSeconds": ttl_seconds }).to_string(),
        )
    })?;
    flag(&reply, "set")
}

/// Sets `key` to `value` if its live value equals `expected`. `None` means "no live value".
/// `Ok(false)` means the value didn't match: a lost race. It keeps the key's expiry.
pub fn compare_and_swap(
    key: &str,
    expected: Option<&[u8]>,
    value: impl AsRef<[u8]>,
) -> KvResult<bool> {
    let request = json!({ "key": key, "expected": expected.map(b64), "value": b64(value) });
    let reply = call(unsafe { kv_compare_and_swap(request.to_string()) })?;
    flag(&reply, "swapped")
}

/// Deletes `key`. Returns whether it had a live value.
pub fn delete(key: &str) -> KvResult<bool> {
    let reply = call(unsafe { kv_delete(json!({ "key": key }).to_string()) })?;
    flag(&reply, "deleted")
}

/// Adds `by` to the integer at `key` (0 when it has none) and returns the new value.
/// Fails with `not_integer` when the value there is not an integer.
pub fn incr(key: &str, by: i64, ttl_seconds: Option<u64>) -> KvResult<i64> {
    let request = json!({ "key": key, "by": by, "ttlSeconds": ttl_seconds });
    let reply = call(unsafe { kv_incr(request.to_string()) })?;
    reply
        .get("value")
        .and_then(Value::as_i64)
        .ok_or_else(malformed)
}

/// The keys starting with `prefix` (up to 1,000).
pub fn list(prefix: &str) -> KvResult<Listing> {
    let reply = call(unsafe { kv_list(json!({ "prefix": prefix }).to_string()) })?;
    let keys = reply
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(malformed)?
        .iter()
        .map(|key| key.as_str().map(str::to_string).ok_or_else(malformed))
        .collect::<KvResult<Vec<_>>>()?;
    Ok(Listing {
        keys,
        truncated: truncated(&reply),
    })
}

/// Reads several keys in one host call. The result lines up with `keys`; `None` means no live value.
pub fn get_many(keys: &[&str]) -> KvResult<Vec<Option<Vec<u8>>>> {
    let reply = call(unsafe { kv_mget(json!({ "keys": keys }).to_string()) })?;
    let entries = reply
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(malformed)?;
    if entries.len() != keys.len() {
        return Err(malformed());
    }
    entries
        .iter()
        .map(|entry| match entry.get("value") {
            None | Some(Value::Null) => Ok(None),
            Some(value) => unb64(value.as_str().ok_or_else(malformed)?).map(Some),
        })
        .collect()
}

/// Every entry of the store (up to 1,000).
pub fn get_all() -> KvResult<Snapshot> {
    let reply = call(unsafe { kv_get_all(json!({}).to_string()) })?;
    let entries = reply
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(malformed)?;
    let entries = entries
        .iter()
        .map(|entry| {
            let key = entry
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(malformed)?;
            let value = entry
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(malformed)?;
            Ok((key.to_string(), unb64(value)?))
        })
        .collect::<KvResult<BTreeMap<_, _>>>()?;
    Ok(Snapshot {
        entries,
        truncated: truncated(&reply),
    })
}

// ---- wire helpers ----

/// Unwraps one host reply. A failed host call, or a reply that isn't the ABI's shape, is `unavailable`.
pub(crate) fn call<E: std::fmt::Display>(raw: Result<String, E>) -> KvResult<Map<String, Value>> {
    let text = raw.map_err(|error| KvError::new("unavailable", error.to_string()))?;
    let reply: Value = serde_json::from_str(&text)
        .map_err(|error| KvError::new("unavailable", format!("malformed reply: {error}")))?;
    let Value::Object(mut fields) = reply else {
        return Err(malformed());
    };
    if let Some(error) = fields.remove("error") {
        let code = error
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the host reported a failure");
        return Err(KvError::new(code, message));
    }
    Ok(fields)
}

pub(crate) fn flag(reply: &Map<String, Value>, field: &str) -> KvResult<bool> {
    reply
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(malformed)
}

fn truncated(reply: &Map<String, Value>) -> bool {
    reply
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn parse_entry(reply: &Map<String, Value>) -> KvResult<Entry> {
    let value = unb64(
        reply
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(malformed)?,
    )?;
    Ok(Entry {
        value,
        ttl_seconds: reply.get("ttlSeconds").and_then(Value::as_u64),
    })
}

fn malformed() -> KvError {
    KvError::new("unavailable", "malformed KV reply")
}

fn b64(bytes: impl AsRef<[u8]>) -> String {
    STANDARD.encode(bytes)
}

fn unb64(text: &str) -> KvResult<Vec<u8>> {
    STANDARD.decode(text).map_err(|_| malformed())
}
