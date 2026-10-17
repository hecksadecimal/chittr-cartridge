//! `manifest.json`: its keys, `v`, `name`, `version`, `fonts` and `controls`.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::controls::{self, ControlDeclarations};
use crate::error::{fail, js_len, Result};

const MANIFEST_VERSION: f64 = 1.0;
const KEYS: [&str; 5] = ["v", "name", "version", "fonts", "controls"];
const NAME_MAX: usize = 80;
const VERSION_MAX: usize = 40;

static FONT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_-]{0,31}$").expect("valid regex"));

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    /// Font name to its file, relative to `assets/`, in manifest order.
    pub fonts: Vec<(String, String)>,
    pub controls: ControlDeclarations,
}

/// Validates `manifest.json`. `assets` holds the archive's asset entries, as `assets/...` names.
pub fn parse(bytes: &[u8], assets: &HashSet<String>) -> Result<Manifest> {
    const FILE: &str = "manifest.json";
    let text = std::str::from_utf8(bytes).ok();
    let value: Value = match text.and_then(|text| serde_json::from_str(text).ok()) {
        Some(value) => value,
        None => return fail(FILE, "is not valid UTF-8 JSON"),
    };
    let Some(map) = value.as_object() else {
        return fail(FILE, "must be a JSON object");
    };
    if let Some(key) = map.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return fail(
            FILE,
            format!(
                r#"has an unknown key "{key}"; allowed keys are {}"#,
                KEYS.join(", ")
            ),
        );
    }
    match map.get("v") {
        None => return fail(FILE, r#"is missing "v""#),
        Some(v) if v.as_f64() != Some(MANIFEST_VERSION) => return fail(FILE, r#""v" must be 1"#),
        Some(_) => {}
    }
    let name = require_string(map, "name", NAME_MAX)?;
    let version = require_string(map, "version", VERSION_MAX)?;
    let fonts = parse_fonts(map.get("fonts"), assets)?;
    let controls = controls::parse(map.get("controls")).or_else(|message| fail(FILE, message))?;
    Ok(Manifest {
        name,
        version,
        fonts,
        controls,
    })
}

fn require_string(map: &Map<String, Value>, key: &str, max: usize) -> Result<String> {
    const FILE: &str = "manifest.json";
    let Some(value) = map.get(key) else {
        return fail(FILE, format!(r#"is missing "{key}""#));
    };
    let Some(text) = value.as_str() else {
        return fail(FILE, format!(r#""{key}" must be a string"#));
    };
    if text.trim().is_empty() {
        return fail(FILE, format!(r#""{key}" must not be empty"#));
    }
    if js_len(text) > max {
        return fail(FILE, format!(r#""{key}" is longer than {max} characters"#));
    }
    Ok(text.to_string())
}

/// Font names must be identifier-safe; each file must be an archive asset under `assets/`.
fn parse_fonts(value: Option<&Value>, assets: &HashSet<String>) -> Result<Vec<(String, String)>> {
    const FILE: &str = "manifest.json";
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(map) = value.as_object() else {
        return fail(
            FILE,
            r#""fonts" must be an object mapping font names to asset paths"#,
        );
    };
    let mut fonts = Vec::new();
    for (name, path) in map {
        if !FONT_NAME.is_match(name) {
            return fail(
                FILE,
                format!(
                    r#"font name "{name}" must start with a letter and use only letters, digits, "-" or "_" (at most 32 characters)"#
                ),
            );
        }
        let Some(path) = path.as_str() else {
            return fail(FILE, format!(r#"font "{name}" must be a path string"#));
        };
        let relative = path
            .strip_prefix("assets/")
            .filter(|relative| is_asset_path(relative));
        let Some(relative) = relative else {
            return fail(
                FILE,
                format!(r#"font "{name}" path "{path}" must be a file under assets/"#),
            );
        };
        if !assets.contains(path) {
            return fail(
                FILE,
                format!(r#"font "{name}" names "{path}", which the archive does not carry"#),
            );
        }
        fonts.push((name.clone(), relative.to_string()));
    }
    Ok(fonts)
}

/// A path relative to `assets/`: non-empty segments, no `.` or `..`, no backslash, no leading slash, no control characters.
pub fn is_asset_path(path: &str) -> bool {
    let length = js_len(path);
    if length == 0 || length > 512 {
        return false;
    }
    if path.starts_with('/')
        || path.contains('\\')
        || path.chars().any(|c| (c as u32) <= 0x1f || c as u32 == 0x7f)
    {
        return false;
    }
    path.split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}
