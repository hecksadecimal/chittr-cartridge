//! `manifest.json`: its keys, `v`, `name`, `version`, `fonts` and `controls`.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::controls::{self, ControlDeclarations};
use crate::error::{fail, js_len, Result};

const KEYS: [&str; 6] = ["v", "name", "version", "fonts", "controls", "capabilities"];
const NAME_MAX: usize = 80;
const VERSION_MAX: usize = 40;

/// Capability ids a Server Guest may declare (ABI v2).
const SERVER_CAPABILITIES: &[&str] = &["files.server", "clock.monotonic"];
/// Capability ids a Canvas Guest may declare (ABI v2).
const CANVAS_CAPABILITIES: &[&str] = &[
    "files.client",
    "framebuffer.rgba8",
    "framebuffer.indexed8",
    "clock.monotonic",
    "input.relative-pointer",
    "audio.pcm",
];

static FONT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_-]{0,31}$").expect("valid regex"));

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    /// The Cartridge ABI version: 1 or 2.
    pub abi: u32,
    pub name: String,
    pub version: String,
    /// Font name to its file, relative to `assets/`, in manifest order.
    pub fonts: Vec<(String, String)>,
    pub controls: ControlDeclarations,
    /// The declared capabilities (ABI v2). Empty for v1.
    pub capabilities: Capabilities,
}

/// The capabilities a v2 manifest declares, per guest and split into required and optional.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub server_require: Vec<String>,
    pub server_optional: Vec<String>,
    pub canvas_require: Vec<String>,
    pub canvas_optional: Vec<String>,
}

impl Capabilities {
    /// Whether a `canvas` capability block was declared at all.
    pub fn declares_canvas(&self) -> bool {
        !self.canvas_require.is_empty() || !self.canvas_optional.is_empty()
    }
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
    let abi = match map.get("v") {
        None => return fail(FILE, r#"is missing "v""#),
        Some(v) => match v.as_f64() {
            Some(version) if version == 1.0 => 1,
            Some(version) if version == 2.0 => 2,
            _ => return fail(FILE, r#""v" must be 1 or 2"#),
        },
    };
    let name = require_string(map, "name", NAME_MAX)?;
    let version = require_string(map, "version", VERSION_MAX)?;
    let fonts = parse_fonts(map.get("fonts"), assets)?;
    let controls = controls::parse(map.get("controls")).or_else(|message| fail(FILE, message))?;
    let capabilities =
        parse_capabilities(map.get("capabilities"), abi).or_else(|message| fail(FILE, message))?;
    Ok(Manifest {
        abi,
        name,
        version,
        fonts,
        controls,
        capabilities,
    })
}

/// Parses the manifest's `capabilities` (ABI v2). A v1 manifest must not carry them.
fn parse_capabilities(value: Option<&Value>, abi: u32) -> std::result::Result<Capabilities, String> {
    if abi < 2 {
        if value.is_some() {
            return Err(r#""capabilities" is only allowed when "v" is 2"#.to_string());
        }
        return Ok(Capabilities::default());
    }
    let Some(value) = value else {
        return Ok(Capabilities::default());
    };
    let Some(map) = value.as_object() else {
        return Err(
            r#""capabilities" must be an object mapping a guest to its capabilities"#.to_string(),
        );
    };
    let mut out = Capabilities::default();
    for (guest, spec) in map {
        let allowed: &[&str] = match guest.as_str() {
            "server" => SERVER_CAPABILITIES,
            "canvas" => CANVAS_CAPABILITIES,
            other => {
                return Err(format!(
                    r#""capabilities" has an unknown guest "{other}"; allowed guests are server, canvas"#
                ))
            }
        };
        let Some(spec) = spec.as_object() else {
            return Err(format!(r#""capabilities.{guest}" must be an object"#));
        };
        if let Some(unknown) = spec
            .keys()
            .find(|key| key.as_str() != "require" && key.as_str() != "optional")
        {
            return Err(format!(
                r#""capabilities.{guest}" has an unknown key "{unknown}"; allowed keys are require, optional"#
            ));
        }
        let require = capability_list(guest, "require", spec.get("require"), allowed)?;
        let optional = capability_list(guest, "optional", spec.get("optional"), allowed)?;
        if let Some(shared) = require.iter().find(|id| optional.contains(id)) {
            return Err(format!(
                r#""capabilities.{guest}" lists "{shared}" as both required and optional"#
            ));
        }
        match guest.as_str() {
            "server" => {
                out.server_require = require;
                out.server_optional = optional;
            }
            _ => {
                out.canvas_require = require;
                out.canvas_optional = optional;
            }
        }
    }
    Ok(out)
}

fn capability_list(
    guest: &str,
    key: &str,
    value: Option<&Value>,
    allowed: &[&str],
) -> std::result::Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(format!(r#""capabilities.{guest}.{key}" must be an array"#));
    };
    let mut out = Vec::new();
    for item in items {
        let Some(id) = item.as_str() else {
            return Err(format!(r#""capabilities.{guest}.{key}" entries must be strings"#));
        };
        if !allowed.contains(&id) {
            return Err(format!(
                r#""capabilities.{guest}.{key}" has an unknown capability "{id}"; allowed ids are {}"#,
                allowed.join(", ")
            ));
        }
        if out.iter().any(|existing| existing == id) {
            return Err(format!(r#""capabilities.{guest}.{key}" repeats "{id}""#));
        }
        out.push(id.to_string());
    }
    Ok(out)
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
