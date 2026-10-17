//! Helpers shared by the integration tests: finding the optional tools, and the repository's paths.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The `xtp` CLI, from `XTP`, the PATH, or `~/.local/bin`. None when it isn't installed.
pub fn xtp() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XTP") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("xtp");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let home = std::env::var_os("HOME")?;
    let candidate = PathBuf::from(home).join(".local/bin/xtp");
    candidate.is_file().then_some(candidate)
}

/// Whether rustc's sysroot has the wasm32-unknown-unknown standard library.
pub fn wasm32_installed() -> bool {
    let output = Command::new("rustc").args(["--print", "sysroot"]).output();
    match output {
        Ok(output) if output.status.success() => {
            let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Path::new(&sysroot)
                .join("lib/rustlib/wasm32-unknown-unknown")
                .is_dir()
        }
        _ => false,
    }
}

/// Generates a fresh scaffold of a schema with `xtp plugin init`, into `dir`.
pub fn scaffold(xtp: &Path, schema: &str, dir: &Path) {
    let status = Command::new(xtp)
        .arg("plugin")
        .arg("init")
        .arg("--schema-file")
        .arg(repo().join("docs/cartridges/v1").join(schema))
        .args(["--template", "Rust", "--name", "scaffold", "-y", "--path"])
        .arg(dir)
        .status()
        .expect("run xtp");
    assert!(status.success(), "xtp plugin init for {schema} failed");
}
