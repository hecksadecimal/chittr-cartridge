//! Chittr Cartridge SDK for Server Guests (ABI `v: 1` and `v: 2`).
//!
//! Enable the `server` feature (on by default) to get the typed Hook envelopes and the
//! `#[hook]` attribute. A Hook is written as a plain function over typed input and output;
//! the macro turns it into the Extism export the host calls, with JSON in and out.
//!
//! ```ignore
//! use chittr_cartridge::prelude::*;
//!
//! #[hook]
//! fn init(_input: InitInput) -> InitOutput {
//!     InitOutput::new("What should we have for lunch?")
//! }
//! ```

#[cfg(feature = "server")]
pub mod envelope;

#[cfg(feature = "server")]
pub mod host;

#[cfg(feature = "server")]
pub mod logging;

#[cfg(feature = "server")]
#[doc(hidden)]
pub mod __private;

#[cfg(feature = "server")]
pub mod kv;

#[cfg(feature = "server")]
pub(crate) mod ffi;

#[cfg(feature = "server")]
pub mod timer;

#[cfg(feature = "server")]
pub mod poster;

#[cfg(feature = "server")]
pub mod push;

#[cfg(feature = "server")]
pub use chittr_cartridge_macros::hook;

/// Test support for Server Guests: an in-memory host and envelope builders (`testing` feature).
#[cfg(feature = "testing")]
pub mod testing;

/// The Canvas Guest API (ABI v1): a Canvas2D-like context over the command buffer (`canvas` feature).
#[cfg(feature = "canvas")]
pub mod canvas;

#[cfg(any(feature = "server", feature = "canvas"))]
pub use extism_pdk;
#[cfg(any(feature = "server", feature = "canvas"))]
pub use serde;
#[cfg(any(feature = "server", feature = "canvas"))]
pub use serde_json;

/// The ABI version every v1 host-to-guest payload carries.
pub const ABI_VERSION: u32 = 1;

/// The ABI version every v2 host-to-guest payload carries.
pub const ABI_VERSION_V2: u32 = 2;

/// Names of the Hooks a Server Guest can export. `#[hook]` rejects any other name. `on_file` and `render_config` are
/// ABI v2; a v1 archive that exports them is refused at upload.
pub const HOOK_NAMES: &[&str] = &[
    "init",
    "render",
    "migrate",
    "on_control",
    "on_reply",
    "on_reaction",
    "on_rechitt",
    "on_view",
    "on_timer",
    "on_canvas_message",
    "on_file",
    "render_config",
];

#[cfg(feature = "server")]
pub mod prelude {
    pub use crate::envelope::{
        CanvasMessageInput, CanvasMessageOutput, CartridgeVersionRef, ChittRef, ConfigAccess,
        ControlInput, ControlOutput, FileInput, FileOutput, FileRef, GateResult, HookError,
        InitInput, InitOutput, MigrateInput, MigrateOutput, Observer, ReactionGateInput,
        RechittGateInput, RenderConfigInput, RenderConfigOutput, RenderInput, RenderOutput,
        ReplyGateInput, ServerLimits, TimerInput, TimerOutput, ViewInput, ViewOutput,
    };
    pub use crate::hook;
    pub use crate::host::{asset, file_close, file_read, AssetError, FileChunk, FileError};
    pub use crate::log;
    pub use crate::logging::LogLevel;
    pub use crate::serde_json::{json, Value};
}
