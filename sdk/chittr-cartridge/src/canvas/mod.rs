//! The Canvas Guest API (ABI `v: 1`, enabled with the `canvas` feature). A Canvas2D-like context over the
//! binary command buffer: draw calls are encoded into a `CommandBuffer`, and `Canvas2D::submit` hands the
//! whole frame to the host in one call. The encoder is pure and tested natively; the host
//! functions are the only part that needs the Extism runtime.
//!
//! ```ignore
//! use chittr_cartridge::canvas::prelude::*;
//!
//! #[plugin_fn]
//! pub fn frame(Json(_input): Json<FrameInput>) -> FnResult<Json<Reply>> {
//!     let mut c = Canvas2D::new();
//!     c.fill_color(rgba(16, 24, 32, 255)).fill_rect(0.0, 0.0, 320.0, 180.0);
//!     Ok(Json(c.submit().into()))
//! }
//! ```

use std::ops::{Deref, DerefMut};

pub mod encode;
pub mod envelope;
pub mod host;

pub use encode::{
    rgba, CommandBuffer, Composite, FillRule, LineCap, LineJoin, TextAlign, TextBaseline,
};
pub use envelope::{CanvasError, EventInput, FrameInput, Reply, StartInput};
pub use host::{
    load_image, load_sound, measure_text, play_sound, request_frame, send, set_fps, stop_sound,
    PlayOptions, TextMetrics,
};

/// A Canvas2D-like context: every drawing method of `CommandBuffer`, plus `submit`.
#[derive(Default)]
pub struct Canvas2D {
    buffer: CommandBuffer,
}

impl Canvas2D {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends the ops encoded so far to the canvas, and starts the next frame's buffer empty.
    pub fn submit(&mut self) -> Result<(), CanvasError> {
        host::submit(&mut self.buffer)
    }
}

impl Deref for Canvas2D {
    type Target = CommandBuffer;

    fn deref(&self) -> &CommandBuffer {
        &self.buffer
    }
}

impl DerefMut for Canvas2D {
    fn deref_mut(&mut self) -> &mut CommandBuffer {
        &mut self.buffer
    }
}

pub mod prelude {
    pub use super::encode::{
        rgba, Composite, FillRule, LineCap, LineJoin, TextAlign, TextBaseline,
    };
    pub use super::envelope::{CanvasError, EventInput, FrameInput, Reply, StartInput};
    pub use super::host::{
        load_image, load_sound, measure_text, play_sound, request_frame, send, set_fps, stop_sound,
        PlayOptions, TextMetrics,
    };
    pub use super::{Canvas2D, CommandBuffer};
    pub use crate::extism_pdk::{plugin_fn, FnResult, Json};
    pub use crate::serde_json::{json, Value};
}
