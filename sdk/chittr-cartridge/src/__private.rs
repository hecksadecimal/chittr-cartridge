//! Runtime helpers the `#[hook]` macro expands to. Not part of the author API.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::envelope::{ErrorEnvelope, HookError};

/// Runs a Hook that cannot fail on its own: reads JSON input, writes JSON output.
pub fn run<I: DeserializeOwned, O: Serialize>(hook: fn(I) -> O) -> i32 {
    match read_input::<I>() {
        Ok(input) => write_output(&hook(input)),
        Err(error) => write_error(HookError::new("bad_input", error.to_string())),
    }
}

/// Runs a Hook that returns `Result<O, HookError>`. An `Err` becomes `{ "error": ... }`.
pub fn run_fallible<I: DeserializeOwned, O: Serialize>(hook: fn(I) -> Result<O, HookError>) -> i32 {
    match read_input::<I>() {
        Ok(input) => match hook(input) {
            Ok(output) => write_output(&output),
            Err(error) => write_error(error),
        },
        Err(error) => write_error(HookError::new("bad_input", error.to_string())),
    }
}

fn read_input<I: DeserializeOwned>() -> Result<I, serde_json::Error> {
    serde_json::from_slice(&extism_pdk::input_bytes())
}

fn write_output<O: Serialize>(output: &O) -> i32 {
    match serde_json::to_string(output) {
        Ok(json) => emit(&json),
        Err(error) => write_error(HookError::new("internal", error.to_string())),
    }
}

fn write_error(error: HookError) -> i32 {
    // A handled failure is still a successful call; the host reads the `error` key.
    match serde_json::to_string(&ErrorEnvelope { error }) {
        Ok(json) => emit(&json),
        Err(_) => 1,
    }
}

fn emit(json: &str) -> i32 {
    match extism_pdk::output(json) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
