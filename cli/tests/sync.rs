//! `sync` against fresh `xtp plugin init` scaffolds: it reproduces each example's bindings, and the bindings the
//! `new` templates carry. Skipped when `xtp` isn't installed.

mod support;

use std::fs;
use std::path::Path;

use chittr_cartridge_cli::sync;
use tempfile::TempDir;

fn copy_lib(from: &Path, project: &Path) {
    fs::create_dir_all(project.join("src")).expect("src dir");
    fs::copy(from, project.join("src/lib.rs")).expect("copy lib.rs");
}

#[test]
fn sync_reproduces_each_example_pdk() {
    let Some(xtp) = support::xtp() else {
        eprintln!("skipped: xtp is not installed");
        return;
    };
    let tmp = TempDir::new().expect("tempdir");
    let server = tmp.path().join("server");
    let canvas = tmp.path().join("canvas");
    support::scaffold(&xtp, "server.xtp.yaml", &server);
    support::scaffold(&xtp, "canvas.xtp.yaml", &canvas);

    let examples = support::repo().join("examples");
    let cases = [
        ("hello", "src", "server"),
        ("poll", "src", "server"),
        ("snake", "src", "server"),
        ("paint", "src", "server"),
        ("poll", "canvas", "canvas"),
        ("snake", "canvas", "canvas"),
        ("paint", "canvas", "canvas"),
    ];
    for (example, part, guest) in cases {
        // A Server Guest lives at examples/<name>, a Canvas Guest at examples/<name>/canvas.
        let guest_dir = if part == "canvas" {
            examples.join(example).join("canvas")
        } else {
            examples.join(example)
        };
        let project = tmp.path().join(format!("{example}-{part}"));
        copy_lib(&guest_dir.join("src/lib.rs"), &project);
        let scaffold = if guest == "server" { &server } else { &canvas };
        sync::sync(scaffold, &project)
            .unwrap_or_else(|error| panic!("{example} {part}: {error:#}"));
        let produced = fs::read_to_string(project.join("src/pdk.rs")).expect("pdk.rs");
        let committed = fs::read_to_string(guest_dir.join("src/pdk.rs")).expect("example pdk.rs");
        assert!(
            produced == committed,
            "{example}/{part}: sync did not reproduce src/pdk.rs"
        );
    }
}

#[test]
fn new_templates_carry_the_pruned_bindings() {
    let Some(xtp) = support::xtp() else {
        eprintln!("skipped: xtp is not installed");
        return;
    };
    let tmp = TempDir::new().expect("tempdir");
    let server = tmp.path().join("server");
    let canvas = tmp.path().join("canvas");
    support::scaffold(&xtp, "server.xtp.yaml", &server);
    support::scaffold(&xtp, "canvas.xtp.yaml", &canvas);
    let templates = support::repo().join("cli/templates");

    let project = tmp.path().join("server-project");
    copy_lib(&templates.join("server/src/lib.rs"), &project);
    sync::sync(&server, &project).expect("sync server");
    assert!(
        fs::read_to_string(project.join("src/pdk.rs")).unwrap()
            == fs::read_to_string(templates.join("server/src/pdk.rs")).unwrap()
    );

    let project = tmp.path().join("canvas-project");
    copy_lib(&templates.join("canvas/src/lib.rs"), &project);
    sync::sync(&canvas, &project).expect("sync canvas");
    assert!(
        fs::read_to_string(project.join("src/pdk.rs")).unwrap()
            == fs::read_to_string(templates.join("canvas/src/pdk.rs")).unwrap()
    );
}

#[test]
fn sync_refuses_a_project_missing_a_required_hook() {
    let Some(xtp) = support::xtp() else {
        eprintln!("skipped: xtp is not installed");
        return;
    };
    let tmp = TempDir::new().expect("tempdir");
    let server = tmp.path().join("server");
    support::scaffold(&xtp, "server.xtp.yaml", &server);
    let project = tmp.path().join("project");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("src/lib.rs"), "fn init() {}\n").unwrap();
    let error = sync::sync(&server, &project).expect_err("render is required");
    assert!(
        error
            .to_string()
            .contains("must define render (server Guest)"),
        "{error:#}"
    );
}
