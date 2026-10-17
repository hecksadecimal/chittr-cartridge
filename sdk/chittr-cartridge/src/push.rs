//! Render Pushes: a Server Guest tells its Chitt's Observers that its card has changed, so their
//! clients fetch a fresh render. The push is queued during the Hook call and published only after the call
//! commits; a call that is canceled, traps or times out publishes none.
//!
//! `push_render` is not allowed in `render` (its reply is `not_allowed`). A Hook call may queue at most 20
//! distinct targets; past that a new target fails with `quota_exceeded`, which the Failures log records.
//! Pushing the same target again in one call is free.

use crate::ffi;
use crate::kv::call;
use serde_json::{json, Value};

pub use crate::kv::{KvError as PushError, KvResult as PushResult};

/// Who a push is for: every Observer of the Chitt, or the one Observer who owns `character_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushTarget {
    All,
    Character(String),
}

impl PushTarget {
    fn to_value(&self) -> Value {
        match self {
            PushTarget::All => json!({}),
            PushTarget::Character(id) => json!({ "characterId": id }),
        }
    }
}

/// Queues a Render Push for `target`. Returns `Ok(())` when it is queued, or the host's refusal.
pub fn push_render(target: PushTarget) -> PushResult<()> {
    let request = target.to_value();
    call(unsafe { ffi::push_render(request.to_string()) })?;
    Ok(())
}
