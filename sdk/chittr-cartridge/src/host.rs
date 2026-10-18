//! Host functions a Server Guest may call (ABI `v: 1`). Enabled with the `server` feature.
//!
//! `asset(path)` reads a file the Cartridge carries under `assets/`. It is read-only. Like every host function, it takes
//! one JSON object and returns one: `{ "bytes": "<base64>" }` on success, or `{ "error": { code, message } }` (codes
//! `not_found`, `too_large` (over 4 MiB) and `invalid`). The SDK decodes the bytes before it returns them.

use crate::ffi;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;

/// A failed `asset(path)` call. `code` is the host's code, such as `not_found`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssetError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AssetError {}

/// The host's reply to `asset`: exactly one of the two fields.
#[derive(Debug, Deserialize)]
struct AssetReply {
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    error: Option<AssetError>,
}

/// Reads one Cartridge Asset by its path relative to `assets/`, e.g. `"img/logo.png"`.
pub fn asset(path: &str) -> Result<Vec<u8>, AssetError> {
    let request = json!({ "path": path }).to_string();
    let text = unsafe { ffi::asset(request) }.map_err(|error| AssetError {
        code: "host".to_string(),
        message: error.to_string(),
    })?;
    let reply: AssetReply = serde_json::from_str(&text).map_err(|error| AssetError {
        code: "invalid".to_string(),
        message: error.to_string(),
    })?;
    if let Some(error) = reply.error {
        return Err(error);
    }
    let encoded = reply.bytes.ok_or_else(|| AssetError {
        code: "invalid".to_string(),
        message: "the host sent neither bytes nor an error".to_string(),
    })?;
    STANDARD.decode(encoded).map_err(|error| AssetError {
        code: "invalid".to_string(),
        message: error.to_string(),
    })
}

/// A failed `file_read` or `file_close` call (v2). `code` is the host's code: `invalid`, `not_found` (unknown,
/// closed, expired or foreign handle), `too_large` (a read over the chunk cap) or `not_supported`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FileError {}

/// The host's reply to `file_read`: `bytes` and `eof`, or an error.
#[derive(Debug, Deserialize)]
struct FileReadReply {
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    eof: bool,
    #[serde(default)]
    error: Option<FileError>,
}

/// One bounded chunk of a file a picker delivered (v2). `bytes` is at most 64 KiB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChunk {
    pub bytes: Vec<u8>,
    pub eof: bool,
}

/// Reads at most 64 KiB of a file the `on_file` Hook received, starting at `offset` (v2). The handle is scoped to
/// the current `on_file` call. The result clips at EOF: an offset at the end returns empty bytes with `eof` true.
pub fn file_read(handle: &str, offset: u64, length: u32) -> Result<FileChunk, FileError> {
    let request = json!({ "handle": handle, "offset": offset, "length": length }).to_string();
    let text = unsafe { ffi::file_read(request) }.map_err(|error| FileError {
        code: "host".to_string(),
        message: error.to_string(),
    })?;
    let reply: FileReadReply = serde_json::from_str(&text).map_err(|error| FileError {
        code: "invalid".to_string(),
        message: error.to_string(),
    })?;
    if let Some(error) = reply.error {
        return Err(error);
    }
    let encoded = reply.bytes.ok_or_else(|| FileError {
        code: "invalid".to_string(),
        message: "the host sent neither bytes nor an error".to_string(),
    })?;
    let bytes = STANDARD.decode(encoded).map_err(|error| FileError {
        code: "invalid".to_string(),
        message: error.to_string(),
    })?;
    Ok(FileChunk {
        bytes,
        eof: reply.eof,
    })
}

/// Releases a file handle early (v2). Closing an unknown or already-closed handle is harmless.
pub fn file_close(handle: &str) -> Result<(), FileError> {
    let request = json!({ "handle": handle }).to_string();
    let text = unsafe { ffi::file_close(request) }.map_err(|error| FileError {
        code: "host".to_string(),
        message: error.to_string(),
    })?;
    let reply: serde_json::Value = serde_json::from_str(&text).map_err(|error| FileError {
        code: "invalid".to_string(),
        message: error.to_string(),
    })?;
    if let Some(error) = reply.get("error") {
        return Err(serde_json::from_value(error.clone()).map_err(|error| FileError {
            code: "invalid".to_string(),
            message: error.to_string(),
        })?);
    }
    Ok(())
}
