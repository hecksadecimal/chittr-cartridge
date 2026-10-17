//! Guest logging. `log!(level, "...", args)` sends one line to the host over Extism's own
//! logging imports, so it needs no host function of its own. The host keeps each line for the failure
//! log and the Chitt's log ring; lines are capped at 512 bytes, and the SDK cuts them first.
//!
//! ```ignore
//! use chittr_cartridge::prelude::*;
//!
//! log!(LogLevel::Info, "rendering {}", input.chitt.id);
//! ```

pub use extism_pdk::LogLevel;

/// The longest line the host keeps, in UTF-8 bytes.
pub const LOG_LINE_MAX_BYTES: usize = 512;

/// Sends one line at `level`, cut to `LOG_LINE_MAX_BYTES` on a character boundary. Used by `log!`.
#[doc(hidden)]
pub fn __emit(level: LogLevel, message: &str) {
    let mut end = message.len().min(LOG_LINE_MAX_BYTES);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    extism_pdk::log!(level, "{}", &message[..end]);
}

/// Logs one line: `log!(level, "format {}", args)`. The level is an `extism_pdk::LogLevel`.
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)+) => {
        $crate::logging::__emit($level, &::std::format!($($arg)+))
    };
}
