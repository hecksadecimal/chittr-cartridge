//! Runs every case in `conformance/cases` through the validator. The corpus format is in `conformance/README.md`.

use std::fs;
use std::path::{Path, PathBuf};

use chittr_cartridge_cli::limits::Limits;
use chittr_cartridge_cli::pack;
use chittr_cartridge_cli::validate::validate_with;
use serde_json::Value;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance/cases")
}

/// An expected error matches exactly, or as a prefix when it ends in `*`.
fn matches(expected: &str, actual: &str) -> bool {
    match expected.strip_suffix('*') {
        Some(prefix) => actual.starts_with(prefix),
        None => expected == actual,
    }
}

fn run_case(dir: &Path) -> Result<(), String> {
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(dir.join("expected.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("expected.json: {e}"))?;
    let limits = match expected.get("limits").and_then(Value::as_object) {
        Some(overrides) => Limits::default().with_overrides(overrides)?,
        None => Limits::default(),
    };
    let bytes = match fs::read(dir.join("archive.zip")) {
        Ok(bytes) => bytes,
        Err(_) => pack::zip(&pack::collect(&dir.join("files")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    };
    let result = validate_with(&bytes, &limits);
    match (expected.get("ok").and_then(Value::as_bool), result) {
        (Some(true), Ok(_)) => Ok(()),
        (Some(false), Err(error)) => {
            let want = expected.get("error").and_then(Value::as_str).unwrap_or("");
            if matches(want, &error.to_string()) {
                Ok(())
            } else {
                Err(format!("expected `{want}`, got `{error}`"))
            }
        }
        (Some(true), Err(error)) => Err(format!("expected ok, got `{error}`")),
        (Some(false), Ok(_)) => Err(format!(
            "expected `{}`, got ok",
            expected.get("error").and_then(Value::as_str).unwrap_or("")
        )),
        (None, _) => Err("expected.json has no \"ok\"".to_string()),
    }
}

#[test]
fn every_conformance_case_passes() {
    let mut dirs: Vec<PathBuf> = fs::read_dir(corpus())
        .expect("conformance/cases exists; regenerate it with the regen_conformance test")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    assert!(
        dirs.len() >= 100,
        "the corpus has {} cases; expected at least 100",
        dirs.len()
    );
    let failures: Vec<String> = dirs
        .iter()
        .filter_map(|dir| {
            run_case(dir)
                .err()
                .map(|error| format!("{}: {error}", dir.file_name().unwrap().to_string_lossy()))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        dirs.len(),
        failures.join("\n")
    );
}
