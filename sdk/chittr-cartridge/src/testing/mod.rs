//! Testing support for Server Guests: an in-memory host, and the envelope builders a test feeds to a Hook.
//!
//! Enable the `testing` feature in a `[dev-dependencies]` entry and the SDK's host functions answer from this fake
//! on a native target, so Hook logic runs under `cargo test`. The wasm build never uses it. The fake keeps the
//! Chittr host's rules: KV keys, values, quotas and TTLs, timers, Render Pushes, the host-call cap per Hook call,
//! the Poster Action quotas and deferrals, and the asset reads. It does not check permissions or the Chitt's
//! state, and it does not model timing. Scripted refusals (`TestHost::refuse_action`) cover the failures a test
//! wants to see.
//!
//! ```ignore
//! use chittr_cartridge::testing::{self, TestHost};
//!
//! #[test]
//! fn init_schedules_nothing_it_cannot() {
//!     let host = TestHost::new();
//!     let chitt = testing::chitt("chitt-1", "poster-1");
//!     let output = my_cartridge::init(testing::init(&chitt));
//!     assert!(!output.content.is_empty());
//!     assert_eq!(host.timers().len(), 1);
//! }
//! ```
//!
//! Each test thread has its own fake host, so tests run in parallel without sharing state. `TestHost::new` resets
//! the thread's fake, so start every test with it.

pub mod envelopes;
mod fake;

pub use self::envelopes::*;
#[doc(hidden)]
pub use self::fake::{host_call, record_log};
pub use self::fake::{HookClass, PendingTimer, RecordedAction, RecordedLog};

pub use crate::envelope::ChittRef;
use crate::kv::KvResult;
use crate::push::PushTarget;

/// The fake host for the current test thread. Build one with `TestHost::new`, which resets the thread's fake host to
/// an empty store at the fixed time 1700000000 (2023-11-14T22:13:20Z). The handle is a view: it holds no state.
#[derive(Debug)]
pub struct TestHost {
    _private: (),
}

impl Default for TestHost {
    fn default() -> Self {
        Self::new()
    }
}

impl TestHost {
    /// Resets this thread's fake host and returns a handle to it.
    pub fn new() -> Self {
        fake::reset();
        Self { _private: () }
    }

    /// The fake clock, in seconds since the Unix epoch.
    pub fn now(&self) -> u64 {
        fake::now()
    }

    /// Sets the fake clock.
    pub fn set_now(&self, seconds: u64) {
        fake::set_now(seconds);
    }

    /// Moves the fake clock forward. Expired KV entries stop being live, and timers become due.
    pub fn advance(&self, seconds: u64) {
        fake::set_now(self.now() + seconds);
    }

    /// Sets the kind of Hook the next host calls belong to. Call it at the top of each Hook under test: it resets the
    /// per-call counters (host calls, Poster Actions, Render Push targets) and sets the limits they face.
    pub fn begin_hook(&self, class: HookClass) {
        fake::set_class(class);
    }

    /// Writes a KV entry directly, as the host would store it, to set up a test. Quotas apply.
    pub fn put_kv(
        &self,
        key: &str,
        value: impl AsRef<[u8]>,
        ttl_seconds: Option<u64>,
    ) -> KvResult<()> {
        fake::put_kv(key, value.as_ref().to_vec(), ttl_seconds)
    }

    /// The live value at `key`, or `None` when it has none.
    pub fn kv_value(&self, key: &str) -> Option<Vec<u8>> {
        fake::kv_entry(key).map(|(value, _)| value)
    }

    /// The live value at `key` as UTF-8 text, or `None` when it has none or it is not text.
    pub fn kv_text(&self, key: &str) -> Option<String> {
        self.kv_value(key)
            .and_then(|value| String::from_utf8(value).ok())
    }

    /// The seconds left before `key` expires, or `None` when it has no live value or no expiry.
    pub fn kv_ttl(&self, key: &str) -> Option<u64> {
        fake::kv_entry(key).and_then(|(_, ttl)| ttl)
    }

    /// Every live key, sorted.
    pub fn kv_keys(&self) -> Vec<String> {
        fake::kv_keys()
    }

    /// Adds a Cartridge asset at `path` (relative to `assets/`), for `asset` to read.
    pub fn add_asset(&self, path: &str, bytes: impl AsRef<[u8]>) {
        fake::add_asset(path, bytes.as_ref().to_vec());
    }

    /// The timers scheduled and not yet fired, earliest first.
    pub fn timers(&self) -> Vec<PendingTimer> {
        fake::timers()
    }

    /// Fires the timer named `name` as the host would: it is removed, the clock moves to its due time, and the
    /// `on_timer` input for it is returned. `None` when no such timer is pending.
    pub fn fire_timer(&self, chitt: &ChittRef, name: &str) -> Option<crate::envelope::TimerInput> {
        fake::fire_timer(chitt, name)
    }

    /// Fires every timer due at the current clock, earliest first, and returns their `on_timer` inputs.
    pub fn fire_due_timers(&self, chitt: &ChittRef) -> Vec<crate::envelope::TimerInput> {
        fake::fire_due(chitt)
    }

    /// Makes every later call of the Poster Action `action` (for example `post_reply`) answer the error `code` with
    /// `message`. Deferred actions are not affected: they are recorded as deferred.
    pub fn refuse_action(&self, action: &str, code: &str, message: &str) {
        fake::refuse_action(action, code, message);
    }

    /// Undoes `refuse_action` for `action`, so later calls succeed again.
    pub fn allow_action(&self, action: &str) {
        fake::allow_action(action);
    }

    /// Every Poster Action the guest made, in order, with the reply it saw.
    pub fn actions(&self) -> Vec<RecordedAction> {
        fake::actions()
    }

    /// Every Render Push the guest queued, in order. A push to every Observer is `PushTarget::All`.
    pub fn pushes(&self) -> Vec<PushTarget> {
        fake::pushes()
    }

    /// Every log line the guest wrote, in order.
    pub fn logs(&self) -> Vec<RecordedLog> {
        fake::logs()
    }
}
