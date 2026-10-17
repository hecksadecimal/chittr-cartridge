//! Host functions a Canvas Guest may call (ABI `v: 1`, docs/cartridges/v1/abi.md). Each takes one JSON
//! object and returns one JSON object, which may carry `error: { code, message }`. The exception is
//! `submit`, which takes the command buffer as binary and returns nothing.

use extism_pdk::{host_fn, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::encode::CommandBuffer;
use super::envelope::{CanvasError, Empty};

mod imports {
    use super::*;

    #[host_fn]
    extern "ExtismHost" {
        pub fn submit(bytes: Vec<u8>);
        pub fn request_frame(input: Json<Empty>) -> Json<HostReply>;
        pub fn set_fps(input: Json<SetFpsInput>) -> Json<HostReply>;
        pub fn measure_text(input: Json<MeasureTextInput>) -> Json<MeasureTextReply>;
        pub fn send(input: Json<SendInput>) -> Json<HostReply>;
        pub fn load_image(input: Json<PathInput>) -> Json<HandleReply>;
        pub fn load_sound(input: Json<PathInput>) -> Json<HandleReply>;
        pub fn play_sound(input: Json<PlaySoundInput>) -> Json<PlayReply>;
        pub fn stop_sound(input: Json<StopSoundInput>) -> Json<HostReply>;
    }
}

/// The input of `load_image` and `load_sound`: a path under the Cartridge's `assets/`, such as `pizza.webp`.
#[derive(Serialize)]
pub struct PathInput {
    pub path: String,
}

/// The input of `play_sound`: the sound's handle, and how loud and whether it loops. The wire names are `volume` and `loop`.
#[derive(Serialize)]
pub struct PlaySoundInput {
    pub handle: u32,
    pub volume: f32,
    #[serde(rename = "loop")]
    pub looped: bool,
}

/// The input of `stop_sound`: the playback id that `play_sound` returned.
#[derive(Serialize)]
pub struct StopSoundInput {
    pub id: u32,
}

/// How a sound plays: its volume (0 to 1, clamped by the host) and whether it loops until stopped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayOptions {
    pub volume: f32,
    pub looped: bool,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            volume: 1.0,
            looped: false,
        }
    }
}

/// The reply of `load_image` and `load_sound`: the handle, or an error.
#[derive(Debug, Deserialize)]
pub struct HandleReply {
    #[serde(default)]
    pub handle: Option<u32>,
    #[serde(default)]
    pub error: Option<CanvasError>,
}

/// The reply of `play_sound`: the playback id, or an error (`busy` past eight sounds, or while the sound is not loaded).
#[derive(Debug, Deserialize)]
pub struct PlayReply {
    #[serde(default)]
    pub id: Option<u32>,
    #[serde(default)]
    pub error: Option<CanvasError>,
}

/// The input of `send`: one JSON message of at most 4 KiB, with an optional sequence number that the
/// reply event echoes back. The answer arrives later as a `reply` or `error` event.
#[derive(Serialize)]
pub struct SendInput {
    pub message: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
}

/// Largest buffer one `submit` may carry; the encoder checks the same limit before the call.
pub use super::encode::SUBMIT_BYTES_MAX;

#[derive(Serialize)]
pub struct SetFpsInput {
    pub fps: u32,
}

#[derive(Serialize)]
pub struct MeasureTextInput {
    pub family: String,
    pub size: f32,
    pub weight: f32,
    pub text: String,
}

/// The reply of a host function that only reports success or failure.
#[derive(Debug, Deserialize)]
pub struct HostReply {
    #[serde(default)]
    pub error: Option<CanvasError>,
}

/// The reply of `measure_text`: the advance width and the ascent and descent, all in px.
#[derive(Debug, Deserialize)]
pub struct MeasureTextReply {
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub ascent: f32,
    #[serde(default)]
    pub descent: f32,
    #[serde(default)]
    pub error: Option<CanvasError>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

fn check(reply: HostReply) -> Result<(), CanvasError> {
    match reply.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Sends one frame's command buffer to the canvas. The buffer is cleared on success, so the next frame
/// starts empty; nothing is cleared on the canvas itself (immediate mode, as on Canvas2D).
pub fn submit(buffer: &mut CommandBuffer) -> Result<(), CanvasError> {
    if buffer.len() > SUBMIT_BYTES_MAX {
        return Err(CanvasError::new(
            "too_large",
            format!(
                "the buffer is {} bytes; the limit is {}",
                buffer.len(),
                SUBMIT_BYTES_MAX
            ),
        ));
    }
    unsafe { imports::submit(buffer.bytes().to_vec()) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?;
    buffer.clear();
    Ok(())
}

/// Asks for one more frame. The host paces frames to the requested rate and its caps.
pub fn request_frame() -> Result<(), CanvasError> {
    let reply = unsafe { imports::request_frame(Json(Empty {})) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?;
    check(reply.0)
}

/// Sets the requested frame rate, 1 to 60. Reduced motion caps it at 15, and the host clamps out-of-range values.
pub fn set_fps(fps: u32) -> Result<(), CanvasError> {
    let reply = unsafe { imports::set_fps(Json(SetFpsInput { fps })) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?;
    check(reply.0)
}

/// Sends `message` (JSON, at most 4 KiB) to the Server Guest's `on_canvas_message`. Up to 4 messages may be in flight;
/// past that this returns a `busy` error. The answer arrives as a `reply` event, or an `error` event with the same
/// `seq`.
pub fn send(message: Value, seq: Option<u32>) -> Result<(), CanvasError> {
    let reply = unsafe { imports::send(Json(SendInput { message, seq })) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?;
    check(reply.0)
}

fn handle_of(reply: HandleReply) -> Result<u32, CanvasError> {
    match (reply.handle, reply.error) {
        (_, Some(error)) => Err(error),
        (Some(handle), None) => Ok(handle),
        (None, None) => Err(CanvasError::new("host", "the host gave no handle")),
    }
}

/// Asks the host to load the image at `path` (under the Cartridge's `assets/`). Returns its handle at once; the image
/// is drawable once the guest's `image_loaded` event arrives. Draw it with `Canvas2D::draw_image`.
pub fn load_image(path: &str) -> Result<u32, CanvasError> {
    let reply = unsafe {
        imports::load_image(Json(PathInput {
            path: path.to_string(),
        }))
    }
    .map_err(|error| CanvasError::new("host", error.to_string()))?;
    handle_of(reply.0)
}

/// Asks the host to load the sound at `path`. Returns its handle at once; the sound is playable once `sound_loaded` arrives.
pub fn load_sound(path: &str) -> Result<u32, CanvasError> {
    let reply = unsafe {
        imports::load_sound(Json(PathInput {
            path: path.to_string(),
        }))
    }
    .map_err(|error| CanvasError::new("host", error.to_string()))?;
    handle_of(reply.0)
}

/// Plays a loaded sound and returns its playback id, for `stop_sound`. While the speaker is off, or the canvas is paused,
/// the call is dropped: it still returns an id, and nothing plays. Eight sounds may play at once; a ninth is a `busy` error.
pub fn play_sound(handle: u32, options: PlayOptions) -> Result<u32, CanvasError> {
    let input = PlaySoundInput {
        handle,
        volume: options.volume,
        looped: options.looped,
    };
    let reply = unsafe { imports::play_sound(Json(input)) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?
        .0;
    match (reply.id, reply.error) {
        (_, Some(error)) => Err(error),
        (Some(id), None) => Ok(id),
        (None, None) => Err(CanvasError::new("host", "the host gave no playback id")),
    }
}

/// Stops one playback. An id that is not playing is a no-op.
pub fn stop_sound(id: u32) -> Result<(), CanvasError> {
    let reply = unsafe { imports::stop_sound(Json(StopSoundInput { id })) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?;
    check(reply.0)
}

/// Measures `text` in the given manifest font (or sans-serif) at `size` px and `weight`.
pub fn measure_text(
    family: &str,
    size: f32,
    weight: f32,
    text: &str,
) -> Result<TextMetrics, CanvasError> {
    let input = MeasureTextInput {
        family: family.to_string(),
        size,
        weight,
        text: text.to_string(),
    };
    let reply = unsafe { imports::measure_text(Json(input)) }
        .map_err(|error| CanvasError::new("host", error.to_string()))?
        .0;
    if let Some(error) = reply.error {
        return Err(error);
    }
    Ok(TextMetrics {
        width: reply.width,
        ascent: reply.ascent,
        descent: reply.descent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shapes of the media functions, as the host reads them.
    #[test]
    fn media_inputs_match_the_abi_shapes() {
        assert_eq!(
            serde_json::to_value(PathInput {
                path: "pizza.webp".into()
            })
            .unwrap(),
            serde_json::json!({ "path": "pizza.webp" })
        );
        assert_eq!(
            serde_json::to_value(PlaySoundInput {
                handle: 2,
                volume: 0.5,
                looped: true
            })
            .unwrap(),
            serde_json::json!({ "handle": 2, "volume": 0.5, "loop": true })
        );
        assert_eq!(
            serde_json::to_value(StopSoundInput { id: 7 }).unwrap(),
            serde_json::json!({ "id": 7 })
        );
    }

    #[test]
    fn media_replies_read_a_handle_an_id_or_an_error() {
        let handle: HandleReply = serde_json::from_str(r#"{ "handle": 3 }"#).unwrap();
        assert_eq!(handle_of(handle).unwrap(), 3);
        let refused: HandleReply =
            serde_json::from_str(r#"{ "error": { "code": "invalid", "message": "no" } }"#).unwrap();
        assert_eq!(handle_of(refused).unwrap_err().code, "invalid");
        let busy: PlayReply =
            serde_json::from_str(r#"{ "error": { "code": "busy", "message": "eight" } }"#).unwrap();
        assert_eq!(busy.error.unwrap().code, "busy");
    }

    #[test]
    fn play_options_default_to_full_volume_and_no_loop() {
        assert_eq!(
            PlayOptions::default(),
            PlayOptions {
                volume: 1.0,
                looped: false
            }
        );
    }
}
