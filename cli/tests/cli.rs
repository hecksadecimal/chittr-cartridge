//! The `chittr-cartridge` binary: its commands are listed, and the four examples check when they are built.

mod support;

use std::path::Path;
use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_chittr-cartridge"))
}

#[test]
fn help_lists_every_command() {
    let output = binary().arg("--help").output().expect("run --help");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    for command in ["check", "pack", "new", "sync", "watch"] {
        assert!(
            text.contains(command),
            "--help does not list {command}:\n{text}"
        );
    }
}

#[test]
fn check_passes_the_built_examples() {
    for example in ["hello", "poll", "snake", "paint"] {
        let cartridge = support::repo()
            .join("examples")
            .join(example)
            .join("cartridge");
        if !cartridge.join("server.wasm").is_file() {
            eprintln!("skipped {example}: run sh scripts/build-examples.sh first");
            continue;
        }
        let output = binary()
            .arg("check")
            .arg(&cartridge)
            .output()
            .expect("run check");
        assert!(
            output.status.success(),
            "{example}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn check_reports_file_and_rule_for_an_invalid_cartridge() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance/cases/unknown-root-file/files");
    let output = binary().arg("check").arg(&dir).output().expect("run check");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("notes.txt: is not an allowed file;")
    );
}
