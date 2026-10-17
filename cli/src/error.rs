//! The rule a Cartridge breaks, formatted as `file: rule` the way the server reports it.

use std::fmt;

/// A rule the archive broke. `file` names the entry, or `cartridge` for archive-wide rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeError {
    pub file: String,
    pub rule: String,
}

impl CartridgeError {
    pub fn new(file: impl Into<String>, rule: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            rule: rule.into(),
        }
    }
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.rule)
    }
}

impl std::error::Error for CartridgeError {}

pub type Result<T, E = CartridgeError> = std::result::Result<T, E>;

/// Returns the error for `file` and `rule`, for `return fail(...)`.
pub fn fail<T>(file: impl Into<String>, rule: impl Into<String>) -> Result<T> {
    Err(CartridgeError::new(file, rule))
}

/// The length JavaScript counts: UTF-16 code units. The server's limits are measured this way.
pub fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}
