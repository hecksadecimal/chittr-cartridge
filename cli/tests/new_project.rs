//! `new` writes projects that build for wasm32 and pass `check` with no edits. The build tests are skipped when the
//! wasm32-unknown-unknown target is not installed. The projects use the SDK from this checkout, since the GitHub
//! default needs the public repository's `main` to carry the SDK.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use chittr_cartridge_cli::new::{self, NewProject, Sdk};
use chittr_cartridge_cli::{pack, validate};
use tempfile::TempDir;

fn sdk_path() -> std::path::PathBuf {
    support::repo().join("sdk/chittr-cartridge")
}

fn create(parent: &Path, name: &str, canvas: bool) -> std::path::PathBuf {
    new::create(
        parent,
        &NewProject {
            name: name.into(),
            canvas,
            sdk: Sdk::Path(sdk_path()),
        },
    )
    .expect("create project")
}

fn files_of(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                walk(&entry.path(), &relative, out);
            } else {
                out.push(relative);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

#[test]
fn server_project_has_the_expected_files() {
    let tmp = TempDir::new().unwrap();
    let root = create(tmp.path(), "demo", false);
    assert_eq!(
        files_of(&root),
        vec![
            ".gitignore",
            "Cargo.toml",
            "README.md",
            "build.sh",
            "cartridge/assets/.gitkeep",
            "cartridge/manifest.json",
            "cartridge/templates/card.liquid",
            "src/lib.rs",
            "src/pdk.rs",
        ]
    );
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("chrono = { version = \"0.4\", default-features = false"));
    assert!(manifest.contains("[profile.release]"));
    let card = fs::read_to_string(root.join("cartridge/templates/card.liquid")).unwrap();
    assert!(card.contains("<lc-author>") && card.contains("<lc-menu>"));
    assert!(!card.contains("<lc-canvas"));
}

#[test]
fn canvas_project_adds_the_guest_and_its_placement() {
    let tmp = TempDir::new().unwrap();
    let root = create(tmp.path(), "demo", true);
    let files = files_of(&root);
    assert!(files.contains(&"canvas/Cargo.toml".to_string()));
    assert!(files.contains(&"canvas/src/pdk.rs".to_string()));
    let card = fs::read_to_string(root.join("cartridge/templates/card.liquid")).unwrap();
    assert!(card.contains("<lc-canvas"));
}

#[test]
fn default_sdk_is_the_git_repository() {
    let tmp = TempDir::new().unwrap();
    let root = new::create(
        tmp.path(),
        &NewProject {
            name: "demo".into(),
            canvas: true,
            sdk: Sdk::Git,
        },
    )
    .unwrap();
    let server = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let canvas = fs::read_to_string(root.join("canvas/Cargo.toml")).unwrap();
    assert!(server.contains(r#"{ package = "chittr-cartridge-sdk", git = "https://github.com/hecksadecimal/chittr-cartridge" }"#));
    assert!(canvas.contains(r#"default-features = false, features = ["canvas"]"#));
}

#[test]
fn refuses_bad_names_and_existing_directories() {
    let tmp = TempDir::new().unwrap();
    assert!(new::validate_name("1demo").is_err());
    assert!(new::validate_name("my cartridge").is_err());
    assert!(new::validate_name("my-cartridge_2").is_ok());
    create(tmp.path(), "demo", false);
    let again = new::create(
        tmp.path(),
        &NewProject {
            name: "demo".into(),
            canvas: false,
            sdk: Sdk::Path(sdk_path()),
        },
    );
    assert!(again.is_err());
}

#[test]
fn build_script_leaves_the_sdk_to_the_environment() {
    // The script never picks an SDK itself: a machine-specific SDK path would break on every other machine. A user
    // who needs one sets SDKROOT in their environment, and cargo sees it.
    let tmp = TempDir::new().unwrap();
    let root = create(tmp.path(), "demo", true);
    let build = fs::read_to_string(root.join("build.sh")).unwrap();
    assert!(!build.contains("export SDKROOT"));
    assert!(!build.contains("/Library/Developer"));
    assert!(!build.contains("MacOSX"));
}

fn build_and_check(canvas: bool) {
    if !support::wasm32_installed() {
        eprintln!("skipped: the wasm32-unknown-unknown target is not installed");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let root = create(tmp.path(), "demo", canvas);
    let status = Command::new("sh")
        .arg("build.sh")
        .current_dir(&root)
        .status()
        .expect("run build.sh");
    assert!(status.success(), "build.sh failed for canvas={canvas}");
    assert!(root.join("cartridge/server.wasm").is_file());
    assert_eq!(root.join("cartridge/canvas.wasm").is_file(), canvas);
    let archive = pack::archive_bytes(&root.join("cartridge")).expect("archive");
    if let Err(error) = validate::validate(&archive) {
        panic!("check failed for canvas={canvas}: {error}");
    }
}

#[test]
fn server_project_builds_and_checks_without_edits() {
    build_and_check(false);
}

#[test]
fn canvas_project_builds_and_checks_without_edits() {
    build_and_check(true);
}

/// A Server Guest with no `init` still validates: the missing Hook is a warning, because `render` is the only Hook the
/// format requires. Chittr refuses to publish it, so the warning says so.
#[test]
fn a_missing_init_is_a_warning_and_not_an_error() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../conformance/cases/ok-minimal/files"
    );
    let archive = pack::archive_bytes(std::path::Path::new(dir)).expect("archive");
    let cartridge = validate::validate(&archive).expect("the minimal case is a valid Cartridge");
    assert_eq!(
        cartridge.warnings,
        vec![validate::NO_INIT_WARNING.to_string()]
    );

    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../conformance/cases/ok-server-all-hooks/files"
    );
    let archive = pack::archive_bytes(std::path::Path::new(dir)).expect("archive");
    let cartridge = validate::validate(&archive).expect("the all-hooks case is a valid Cartridge");
    assert!(cartridge.warnings.is_empty());
}
