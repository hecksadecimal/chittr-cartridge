//! The limits the server enforces. The manifest cap is the validator's own: the spec leaves JSON size unbounded.

use serde_json::{Map, Value};

pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * KIB;

/// Every limit the validator enforces. Defaults are the server's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub archive_bytes: u64,
    pub inflated_bytes: u64,
    pub asset_bytes: u64,
    pub wasm_bytes: u64,
    pub files: u64,
    pub template_files: u64,
    pub template_bytes: u64,
    pub manifest_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            archive_bytes: 32 * MIB,
            inflated_bytes: 64 * MIB,
            asset_bytes: 16 * MIB,
            wasm_bytes: 4 * MIB,
            files: 1_000,
            template_files: 64,
            template_bytes: 256 * KIB,
            manifest_bytes: 16 * KIB,
        }
    }
}

impl Limits {
    /// Applies overrides keyed by the server validator's limit names (the keys the conformance corpus uses).
    /// The conformance corpus uses this to exercise a limit with a small archive.
    pub fn with_overrides(mut self, overrides: &Map<String, Value>) -> Result<Self, String> {
        for (key, value) in overrides {
            let n = value
                .as_u64()
                .ok_or_else(|| format!("limit {key} must be a whole number"))?;
            match key.as_str() {
                "archiveBytes" => self.archive_bytes = n,
                "inflatedBytes" => self.inflated_bytes = n,
                "assetBytes" => self.asset_bytes = n,
                "wasmBytes" => self.wasm_bytes = n,
                "files" => self.files = n,
                "templateFiles" => self.template_files = n,
                "templateBytes" => self.template_bytes = n,
                "manifestBytes" => self.manifest_bytes = n,
                other => return Err(format!("unknown limit {other}")),
            }
        }
        Ok(self)
    }
}
