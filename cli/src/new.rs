//! `new`: writes a Cartridge project from the templates the binary carries. The Server Guest is always written; the
//! Canvas Guest and its placement in the card are added with `--canvas`. The generated bindings in `src/pdk.rs` were
//! produced by `xtp plugin init` and pruned by `sync`, so a fresh project builds and checks with no edits.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use include_dir::{include_dir, Dir, File};

static TEMPLATES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// The SDK's repository. A generated project depends on its `chittr-cartridge-sdk` package from here by default.
pub const SDK_GIT: &str = "https://github.com/hecksadecimal/chittr-cartridge";

/// Where a generated project gets the Cartridge SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sdk {
    /// This repository on GitHub, at its default branch.
    Git,
    /// A local checkout of the SDK crate (`sdk/chittr-cartridge`), for developing the SDK itself.
    Path(PathBuf),
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub name: String,
    /// The Cartridge ABI to target: 1 or 2.
    pub abi: u32,
    pub canvas: bool,
    pub sdk: Sdk,
}

/// Creates `parent/<name>` from the templates and returns its path. Refuses an existing directory.
pub fn create(parent: &Path, project: &NewProject) -> Result<PathBuf> {
    validate_name(&project.name)?;
    if project.abi != 1 && project.abi != 2 {
        bail!("the ABI version must be 1 or 2 (got {})", project.abi);
    }
    let root = parent.join(&project.name);
    if root.exists() {
        bail!("{} already exists", root.display());
    }
    let sdk = sdk_spec(&project.sdk)?;
    let canvas_placement = if project.canvas {
        "  <lc-canvas width=\"320\" height=\"180\" label=\"A starter canvas: a plain frame, drawn by the Canvas Guest\"></lc-canvas>\n"
    } else {
        ""
    };
    let canvas_readme = if project.canvas {
        "The Canvas Guest is `canvas/src/lib.rs`. It draws the `<lc-canvas>` placed in the card, and `build.sh` builds it too.\n\n"
    } else {
        ""
    };
    let abi = project.abi.to_string();
    let substitutions = [
        ("__NAME__", project.name.as_str()),
        ("__ABI__", abi.as_str()),
        ("__SDK_DEP__", &format!("{{ package = \"chittr-cartridge-sdk\", {sdk} }}")),
        ("__SDK_CANVAS_DEP__", &format!("{{ package = \"chittr-cartridge-sdk\", {sdk}, default-features = false, features = [\"canvas\"] }}")),
        ("__CANVAS_PLACEMENT__", canvas_placement),
        ("__CANVAS_README__", canvas_readme),
    ];

    for file in all_files(&TEMPLATES) {
        let relative = file.path().to_str().context("template path is UTF-8")?;
        if relative.starts_with("v2/") {
            // The v2 bindings are injected after the tree is written.
            continue;
        }
        let (group, rest) = relative
            .split_once('/')
            .context("template outside a group")?;
        if group == "canvas" && !project.canvas {
            continue;
        }
        let target = match group {
            "server" => rest.to_string(),
            "canvas" => format!("canvas/{rest}"),
            other => bail!("unknown template group {other}"),
        };
        let target = output_name(&target);
        let mut text = std::str::from_utf8(file.contents())
            .context("templates are UTF-8")?
            .to_string();
        for (placeholder, value) in &substitutions {
            text = text.replace(placeholder, value);
        }
        let path = root.join(&target);
        fs::create_dir_all(path.parent().context("file has a parent")?)
            .with_context(|| format!("{}", path.display()))?;
        fs::write(&path, text).with_context(|| format!("{}", path.display()))?;
        if target == "build.sh" {
            make_executable(&path)?;
        }
    }
    if project.abi >= 2 {
        write_binding(&root.join("src/pdk.rs"), "v2/server/pdk.rs")?;
        if project.canvas {
            write_binding(&root.join("canvas/src/pdk.rs"), "v2/canvas/pdk.rs")?;
        }
    }
    Ok(root)
}

/// Writes a v2 binding over the tree's v1 one.
fn write_binding(dest: &Path, source: &str) -> Result<()> {
    let file = TEMPLATES
        .get_file(source)
        .with_context(|| format!("the v2 template {source} is missing"))?;
    fs::write(dest, file.contents()).with_context(|| format!("{}", dest.display()))
}

fn sdk_spec(sdk: &Sdk) -> Result<String> {
    match sdk {
        Sdk::Git => Ok(format!("git = \"{SDK_GIT}\"")),
        Sdk::Path(path) => {
            let absolute = path
                .canonicalize()
                .with_context(|| format!("--sdk-path {}", path.display()))?;
            if !absolute.join("Cargo.toml").is_file() {
                bail!("--sdk-path {} has no Cargo.toml; point it at the sdk/chittr-cartridge directory", absolute.display());
            }
            let text = absolute.to_string_lossy();
            if text.contains('"') {
                bail!("--sdk-path {text} contains a quote, which can't go in a Cargo.toml path");
            }
            Ok(format!("path = \"{text}\""))
        }
    }
}

/// A Cargo package name: a letter, then letters, digits, `-` or `_`.
pub fn validate_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        bail!("project name `{name}` must start with a letter and use only letters, digits, '-' or '_'");
    }
    Ok(())
}

/// Maps a template's file name to the file it becomes. Templates carry `.tmpl` on the files Cargo or the
/// README would otherwise treat as their own, and names without a leading dot for the dotfiles.
fn output_name(path: &str) -> String {
    let path = path.strip_suffix(".tmpl").unwrap_or(path);
    match path {
        "gitignore" => ".gitignore".to_string(),
        other => match other.rsplit_once('/') {
            Some((dir, "gitkeep")) => format!("{dir}/.gitkeep"),
            _ if other == "gitkeep" => ".gitkeep".to_string(),
            _ => other.to_string(),
        },
    }
}

fn all_files<'a>(dir: &'a Dir<'a>) -> Vec<&'a File<'a>> {
    let mut out: Vec<&File> = dir.files().collect();
    for child in dir.dirs() {
        out.extend(all_files(child));
    }
    out.sort_by(|a, b| a.path().cmp(b.path()));
    out
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("{}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}
