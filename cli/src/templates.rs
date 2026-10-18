//! Templates: they must parse as Liquid, and the Controls and `<lc-canvas>` they use must match the manifest and the
//! Canvas Guest. The parse uses the Rust `liquid` crate, which is best effort. The server's LiquidJS parser is
//! authoritative at upload.
//!
//! Liquid's parser here is told about every filter name the template uses (LiquidJS accepts unknown filters at parse
//! time and fails only at render), and about the tags the host's parser knows that the Rust crate lacks. `layout`,
//! `block` and the multi-line `liquid` tag are not supported, so a template that uses them can be refused here.

use std::collections::HashSet;
use std::io::Write;
use std::sync::LazyLock;

use liquid_core::parser::{FilterArguments, ParameterReflection};
use liquid_core::Value;
use liquid_core::{
    Filter, FilterReflection, Language, ParseFilter, ParseTag, Renderable, Runtime, TagReflection,
    TagTokenIter, ValueView,
};
use regex::Regex;

use crate::controls::{valid_control_name, ControlDeclarations};
use crate::error::{fail, js_len, Result};

/// Tags LiquidJS knows and the Rust crate does not: they parse here as no-ops.
const STUB_TAGS: [&str; 3] = ["echo", "include", "render"];

static FILTER_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\|\s*([A-Za-z_][A-Za-z0-9_]*)").expect("valid regex"));
static CONTROL_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)data-lc-control\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#)
        .expect("valid regex")
});
static LC_CANVAS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<lc-canvas[\s/>]").expect("valid regex"));
static LC_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<lc-file\b([^>]*)>").expect("valid regex"));
static LC_FILE_ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([a-zA-Z-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#).expect("valid regex")
});

/// File pickers allowed per template.
const FILE_PICKERS_MAX: usize = 8;
const ACCEPT_ENTRIES_MAX: usize = 16;
const ACCEPT_CHARS_MAX: usize = 64;
const LABEL_CHARS_MAX: usize = 200;

/// Parses one template as Liquid. The error is the message to report under the template's file name.
pub fn parse(file: &str, source: &str) -> Result<()> {
    let parser =
        parser_for(source).map_err(|message| crate::error::CartridgeError::new(file, message))?;
    match parser.parse(source) {
        Ok(_) => Ok(()),
        Err(error) => fail(
            file,
            format!(
                "does not parse as LiquidJS: {}",
                one_line(&error.to_string())
            ),
        ),
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parser_for(source: &str) -> std::result::Result<liquid::Parser, String> {
    let mut builder = liquid::ParserBuilder::with_stdlib();
    let mut seen = HashSet::new();
    for captures in FILTER_NAME.captures_iter(source) {
        let name = captures[1].to_string();
        if seen.insert(name.clone()) {
            builder = builder.filter(StubFilter { name });
        }
    }
    for tag in STUB_TAGS {
        builder = builder.tag(StubTag {
            name: tag.to_string(),
        });
    }
    builder
        .build()
        .map_err(|error| format!("could not build the Liquid parser: {error}"))
}

/// Every Control name a template binds with `data-lc-control`, and whether any name is built at render time.
pub fn control_names(source: &str) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut dynamic = false;
    for captures in CONTROL_ATTRIBUTE.captures_iter(source) {
        let raw = captures
            .get(1)
            .or(captures.get(2))
            .or(captures.get(3))
            .map_or("", |m| m.as_str());
        let name = raw.trim().to_string();
        if name.contains('{') {
            dynamic = true;
        } else {
            names.push(name);
        }
    }
    (names, dynamic)
}

/// A template may bind only Controls the manifest declares, and the Control's audience must match the surface:
/// public Controls on `card.liquid` (and other public templates), non-public Controls only in `config.liquid`.
pub fn check_controls(
    file: &str,
    source: &str,
    declared: &ControlDeclarations,
    config_surface: bool,
) -> Result<()> {
    let (names, dynamic) = control_names(source);
    if dynamic {
        return fail(file, "binds a Control whose name comes from Liquid; data-lc-control must name the Control literally");
    }
    for name in names {
        let Some((_, control)) = declared.iter().find(|(declared, _)| *declared == name) else {
            return fail(
                file,
                format!(
                    r#"uses the undeclared Control "{name}"; declare it under "controls" in manifest.json"#
                ),
            );
        };
        if config_surface && control.audience.is_public() {
            return fail(
                file,
                format!(
                    r#"uses the public Control "{name}" in a configuration template; give it an "audience" of owner, admin or owner_or_admin, or move it to card.liquid"#
                ),
            );
        }
        if !config_surface && !control.audience.is_public() {
            return fail(
                file,
                format!(
                    r#"uses the non-public Control "{name}" outside the configuration template; a Control with audience "{}" belongs in config.liquid"#,
                    control.audience.as_str()
                ),
            );
        }
    }
    Ok(())
}

/// Validates the `<lc-file>` pickers in a v2 template: their attributes, their names and how many there are.
pub fn check_file_pickers(file: &str, source: &str) -> Result<()> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut count = 0usize;
    for tag in LC_FILE.captures_iter(source) {
        count += 1;
        if count > FILE_PICKERS_MAX {
            return fail(
                file,
                format!("places more than {FILE_PICKERS_MAX} <lc-file> pickers"),
            );
        }
        let mut name: Option<String> = None;
        let mut target: Option<String> = None;
        for attribute in LC_FILE_ATTR.captures_iter(&tag[1]) {
            let key = attribute[1].to_ascii_lowercase();
            let value = attribute
                .get(2)
                .or(attribute.get(3))
                .or(attribute.get(4))
                .map_or("", |m| m.as_str());
            match key.as_str() {
                "name" => name = Some(value.to_string()),
                "target" => target = Some(value.to_string()),
                "accept" => {
                    let entries: Vec<&str> = value.split(',').map(str::trim).collect();
                    if entries.is_empty()
                        || entries.len() > ACCEPT_ENTRIES_MAX
                        || entries
                            .iter()
                            .any(|entry| entry.is_empty() || js_len(entry) > ACCEPT_CHARS_MAX)
                    {
                        return fail(file, "<lc-file> accept must be 1 to 16 comma-separated extensions or MIME types of at most 64 characters");
                    }
                }
                "max-bytes" => {
                    if value.parse::<u64>().ok().filter(|n| *n > 0).is_none() {
                        return fail(file, "<lc-file> max-bytes must be a positive whole number");
                    }
                }
                "label" => {
                    if js_len(value) > LABEL_CHARS_MAX {
                        return fail(file, "<lc-file> label is at most 200 characters");
                    }
                }
                other => {
                    return fail(file, format!("<lc-file> has an unknown attribute \"{other}\""));
                }
            }
        }
        let Some(name) = name else {
            return fail(file, "<lc-file> needs a name");
        };
        if !valid_control_name(&name) {
            return fail(file, format!("<lc-file> name \"{name}\" must start with a lowercase letter and use only lowercase letters, digits, \"-\" or \"_\" (at most 32 characters)"));
        }
        if !seen.insert(name.clone()) {
            return fail(file, format!("<lc-file> repeats the name \"{name}\""));
        }
        match target.as_deref() {
            Some("client") | Some("server") => {}
            Some(other) => {
                return fail(file, format!("<lc-file> target must be \"client\" or \"server\", not \"{other}\""))
            }
            None => return fail(file, "<lc-file> needs a target of \"client\" or \"server\""),
        }
    }
    Ok(())
}

/// Whether the template places `<lc-canvas>`.
pub fn places_canvas(source: &str) -> bool {
    LC_CANVAS.is_match(source)
}

/// A filter the validator accepts by name and does not run.
#[derive(Clone)]
struct StubFilter {
    name: String,
}

impl FilterReflection for StubFilter {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "accepted by the validator; the server's filter runs at render"
    }

    fn positional_parameters(&self) -> &'static [ParameterReflection] {
        &[]
    }

    fn keyword_parameters(&self) -> &'static [ParameterReflection] {
        &[]
    }
}

impl ParseFilter for StubFilter {
    fn parse(&self, _arguments: FilterArguments) -> liquid_core::Result<Box<dyn Filter>> {
        Ok(Box::new(Passthrough))
    }

    fn reflection(&self) -> &dyn FilterReflection {
        self
    }
}

#[derive(Debug)]
struct Passthrough;

impl std::fmt::Display for Passthrough {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("passthrough")
    }
}

impl Filter for Passthrough {
    fn evaluate(
        &self,
        input: &dyn ValueView,
        _runtime: &dyn Runtime,
    ) -> liquid_core::Result<Value> {
        Ok(input.to_value())
    }
}

/// A tag the validator accepts and renders as nothing.
#[derive(Clone)]
struct StubTag {
    name: String,
}

impl TagReflection for StubTag {
    fn tag(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "accepted by the validator; the server's tag runs at render"
    }
}

impl ParseTag for StubTag {
    fn parse(
        &self,
        _arguments: TagTokenIter,
        _options: &Language,
    ) -> liquid_core::Result<Box<dyn Renderable>> {
        Ok(Box::new(Nothing))
    }

    fn reflection(&self) -> &dyn TagReflection {
        self
    }
}

#[derive(Debug)]
struct Nothing;

impl Renderable for Nothing {
    fn render_to(
        &self,
        _writer: &mut dyn Write,
        _runtime: &dyn Runtime,
    ) -> liquid_core::Result<()> {
        Ok(())
    }
}
