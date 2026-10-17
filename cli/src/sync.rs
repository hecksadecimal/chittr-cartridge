//! Syncs an `xtp plugin init` scaffold's bindings into a Cartridge's Rust project. A port of the server's scaffold
//! sync script.
//!
//! The stock template generates a `#[no_mangle]` wrapper for every export of the schema, and the host calls any Hook it
//! finds exported. So only `src/pdk.rs` is copied, with each wrapper removed for a Hook the project's `src/lib.rs` does
//! not define. `src/lib.rs` is the author's: it is created from the scaffold only when missing. `Cargo.toml`, `xtp.toml`
//! and the rest are left alone.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;

static WRAPPER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("(?m)    #\\[no_mangle\\]\n    pub extern \"C\" fn (\\w+)\\(\\) -> i32 \\{\n")
        .expect("valid regex")
});
static DEFINED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(?:pub(?:\(crate\))?\s+)?fn\s+(\w+)\s*[(<]").expect("valid regex")
});
const WRAPPER_END: &str = "\n    }\n";

/// One export wrapper in a generated `src/pdk.rs`, with the byte span it occupies (its trailing blank line included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrapper {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// Every export wrapper in a generated `src/pdk.rs`, in order.
pub fn wrappers(source: &str) -> Result<Vec<Wrapper>> {
    let mut spans = Vec::new();
    for captures in WRAPPER.captures_iter(source) {
        let whole = captures.get(0).expect("whole match");
        let name = captures[1].to_string();
        let close = source[whole.end()..]
            .find(WRAPPER_END)
            .map(|offset| whole.end() + offset)
            .ok_or_else(|| anyhow!("export {name} has no closing brace"))?;
        let mut end = close + WRAPPER_END.len();
        if source.as_bytes().get(end) == Some(&b'\n') {
            end += 1;
        }
        spans.push(Wrapper {
            name,
            start: whole.start(),
            end,
        });
    }
    Ok(spans)
}

/// What a sync kept and removed, and the Guest kind the scaffold is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncResult {
    pub kind: &'static str,
    pub kept: Vec<String>,
    pub removed: Vec<String>,
}

/// Writes `project/src/pdk.rs` from the scaffold's, keeping only the exports the project's `src/lib.rs` defines.
pub fn sync(scaffold: &Path, project: &Path) -> Result<SyncResult> {
    let source = fs::read_to_string(scaffold.join("src").join("pdk.rs"))
        .with_context(|| format!("{}/src/pdk.rs", scaffold.display()))?;
    let spans = wrappers(&source)?;
    let exported: Vec<String> = spans.iter().map(|span| span.name.clone()).collect();
    let kind = if exported.iter().any(|name| name == "start") {
        "canvas"
    } else {
        "server"
    };

    let lib = project.join("src").join("lib.rs");
    if !lib.exists() {
        fs::create_dir_all(lib.parent().expect("src dir"))
            .with_context(|| format!("{}", lib.display()))?;
        fs::copy(scaffold.join("src").join("lib.rs"), &lib)
            .with_context(|| format!("{}", lib.display()))?;
    }
    let lib_source = fs::read_to_string(&lib).with_context(|| format!("{}", lib.display()))?;
    let defined: HashSet<String> = DEFINED
        .captures_iter(&lib_source)
        .map(|captures| captures[1].to_string())
        .collect();

    let required: &[&str] = if kind == "canvas" {
        &["start", "frame"]
    } else {
        &["render"]
    };
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|name| !defined.contains(*name))
        .collect();
    if !missing.is_empty() {
        bail!(
            "{}: must define {} ({kind} Guest)",
            lib.display(),
            missing.join(", ")
        );
    }

    let mut out = String::new();
    let mut cursor = 0;
    for span in &spans {
        if defined.contains(&span.name) {
            continue;
        }
        out.push_str(&source[cursor..span.start]);
        cursor = span.end;
    }
    out.push_str(&source[cursor..]);
    let pdk = project.join("src").join("pdk.rs");
    fs::write(&pdk, out).with_context(|| format!("{}", pdk.display()))?;

    Ok(SyncResult {
        kind,
        kept: exported
            .iter()
            .filter(|name| defined.contains(*name))
            .cloned()
            .collect(),
        removed: exported
            .iter()
            .filter(|name| !defined.contains(*name))
            .cloned()
            .collect(),
    })
}
