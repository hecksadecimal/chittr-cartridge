//! Host functions a Server Guest may call (ABI `v: 1`). Enabled with the `server` feature.
//!
//! `asset(path)` reads a file the Cartridge carries under `assets/`. It is read-only. Like every host function, it takes
//! one JSON object and returns one: `{ "bytes": "<base64>" }` on success, or `{ "error": { code, message } }` (codes
//! `not_found`, `too_large` (over 4 MiB) and `invalid`). The SDK decodes the bytes before it returns them.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;

mod imports {
    use extism_pdk::host_fn;

    #[host_fn]
    extern "ExtismHost" {
        pub fn asset(request: String) -> String;
    }
}

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
    let text = unsafe { imports::asset(request) }.map_err(|error| AssetError {
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
