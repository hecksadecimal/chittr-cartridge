//! The JSON shapes of the Canvas Guest's exports and of its errors (ABI `v: 1`, docs/cartridges/v1/abi.md).
//! Field names are camelCase on the wire. Every export takes one of the inputs here and returns a `Reply`.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An error a host function or the guest reports: `{ code, message }` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasError {
    pub code: String,
    pub message: String,
}

impl CanvasError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for CanvasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CanvasError {}

/// The input of a host function that takes nothing: `request_frame({})`.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Empty {}

/// Every export's return: `{}`, or `{ error }` for a failure the guest reports itself. An `error` counts as a
/// crash on the host.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasError>,
}

impl Reply {
    pub fn ok() -> Self {
        Self::default()
    }

    pub fn failed(error: CanvasError) -> Self {
        Self { error: Some(error) }
    }
}

impl From<Result<(), CanvasError>> for Reply {
    fn from(result: Result<(), CanvasError>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(error) => Self::failed(error),
        }
    }
}

/// Input to `start`. `state` is the render's `canvas` value, or null. Width and height are backing-store pixels.
/// In ABI v2 it also carries the effective capabilities and resource limits; a v1 host omits both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInput {
    pub width: f32,
    pub height: f32,
    pub dpr: f32,
    pub prefers_reduced_motion: bool,
    #[serde(default)]
    pub state: Value,
    pub fullscreen: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<CanvasLimits>,
}

/// The effective v2 resource limits for a Canvas Guest. Read them rather than hard-coding the defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasLimits {
    pub max_file_bytes: u64,
    pub max_file_read_bytes: u64,
    pub max_file_handles: u32,
    pub max_file_bytes_per_canvas: u64,
    pub max_pickers: u32,
    pub max_framebuffer_pixels: u32,
    pub max_palette_entries: u32,
    pub max_relative_pointer_delta: u32,
    pub max_pcm_chunk_bytes: u32,
    pub pcm_queue_ms: u32,
}

/// Input to `frame`: the frame's time in ms since the page's time origin, and the time since the last frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameInput {
    pub t: f64,
    pub dt: f64,
}

/// Input to `event`: `type` names the event, and the rest of its fields are beside it. Types in v2: `state`
/// (with `state`), `resize` (`width`, `height`, `fullscreen`), `visibility` (`visible`) and `file` (`name`, `file`).
/// Unknown types are ignored by the guest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventInput {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// Metadata for a client-local file the host delivered (ABI v2). `id` is an opaque handle scoped to this canvas
/// instance; the bytes never travel in an envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// A pointer-locked mouse's coalesced movement (ABI v2, `input.relative-pointer`). The deltas are already clamped by
/// the host; there are no canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelativePointer {
    pub movement_x: f64,
    pub movement_y: f64,
    pub buttons: u32,
}

impl EventInput {
    /// The relative movement of a pointer-locked mouse, or `None` for an absolute pointer or another event.
    pub fn relative_pointer(&self) -> Option<RelativePointer> {
        if self.kind != "pointer" || self.rest.get("relative") != Some(&Value::Bool(true)) {
            return None;
        }
        Some(RelativePointer {
            movement_x: self.rest.get("movementX")?.as_f64()?,
            movement_y: self.rest.get("movementY")?.as_f64()?,
            buttons: self.rest.get("buttons").and_then(Value::as_u64).unwrap_or(0) as u32,
        })
    }

    /// Whether this is the host's `input_reset` notification (ABI v2). A guest should clear held keys and buttons.
    pub fn input_reset(&self) -> bool {
        self.kind == "input_reset"
    }

    /// The picker name and file metadata of a `file` event, or `None` for any other event.
    pub fn file(&self) -> Option<(String, FileRef)> {
        if self.kind != "file" {
            return None;
        }
        let name = self.rest.get("name")?.as_str()?.to_string();
        let file = serde_json::from_value(self.rest.get("file")?.clone()).ok()?;
        Some((name, file))
    }
}
