//! The validator: `validate` runs the server's checks, in the same order, and returns the
//! first rule the archive breaks.

use std::collections::{HashMap, HashSet};

use crate::archive;
use crate::controls::ControlDeclarations;
use crate::error::{fail, CartridgeError, Result};
use crate::limits::Limits;
use crate::manifest::{self, Manifest};
use crate::{templates, wasm};

/// A Cartridge that passed every check.
#[derive(Debug)]
pub struct Cartridge {
    pub manifest: Manifest,
    pub server_wasm: Vec<u8>,
    pub canvas_wasm: Option<Vec<u8>>,
    /// Template name (without `templates/` and `.liquid`) and its source, in archive order.
    pub templates: Vec<(String, String)>,
    /// Asset paths relative to `assets/`.
    pub assets: Vec<String>,
    /// The Hooks the Server Guest exports, in export order.
    pub hooks: Vec<String>,
    /// Problems that don't stop an upload, in the order they were found. A Cartridge with warnings still validates.
    pub warnings: Vec<String>,
}

/// The warning for a Server Guest with no `init` Hook. Publishing needs one: Chittr calls `init` to get the Chitt's text.
pub const NO_INIT_WARNING: &str = "server.wasm: no init Hook — Chittr refuses to publish without one (it supplies the Chitt's text)";

/// Validates a `.cartridge` archive with the server's default limits.
pub fn validate(bytes: &[u8]) -> Result<Cartridge> {
    validate_with(bytes, &Limits::default())
}

/// Validates a `.cartridge` archive with the given limits.
pub fn validate_with(bytes: &[u8], limits: &Limits) -> Result<Cartridge> {
    if bytes.len() as u64 > limits.archive_bytes {
        return fail(
            "cartridge",
            format!(
                "archive is {} bytes; the limit is {}",
                bytes.len(),
                limits.archive_bytes
            ),
        );
    }
    let entries = archive::read_central_directory(bytes, limits)?;
    let plan = archive::plan_entries(&entries, limits)?;

    let mut contents: HashMap<&str, Vec<u8>> = HashMap::new();
    for entry in &plan.files {
        contents.insert(entry.name.as_str(), archive::read_entry(bytes, entry)?);
    }
    let asset_names: HashSet<String> = plan.assets.iter().map(|entry| entry.name.clone()).collect();

    let manifest_bytes = required(&contents, "manifest.json")?;
    let manifest = manifest::parse(manifest_bytes, &asset_names)?;
    let server_wasm = required(&contents, "server.wasm")?;
    let hooks = wasm::check_server(server_wasm, manifest.abi)?;
    let mut warnings = Vec::new();
    if !hooks.iter().any(|name| name == "init") {
        warnings.push(NO_INIT_WARNING.to_string());
    }
    let canvas_wasm = contents.get("canvas.wasm");
    if let Some(canvas) = canvas_wasm {
        wasm::check_canvas(canvas)?;
    }
    if manifest.abi >= 2 && manifest.capabilities.declares_canvas() && canvas_wasm.is_none() {
        return fail(
            "manifest.json",
            r#"declares "capabilities.canvas" but the Cartridge has no canvas.wasm"#,
        );
    }

    let templates = parse_templates(&plan.templates, &contents, &manifest.controls, manifest.abi)?;
    check_canvas_placement(&templates, canvas_wasm.is_some())?;

    let assets = plan
        .assets
        .iter()
        .map(|entry| entry.name["assets/".len()..].to_string())
        .collect();
    Ok(Cartridge {
        manifest,
        server_wasm: server_wasm.to_vec(),
        canvas_wasm: canvas_wasm.cloned(),
        templates,
        assets,
        hooks,
        warnings,
    })
}

fn required<'a>(contents: &'a HashMap<&str, Vec<u8>>, name: &str) -> Result<&'a [u8]> {
    match contents.get(name) {
        Some(bytes) => Ok(bytes.as_slice()),
        None => fail(name, "is required but missing"),
    }
}

fn parse_templates(
    templates: &[(String, archive::Entry)],
    contents: &HashMap<&str, Vec<u8>>,
    controls: &ControlDeclarations,
    abi: u32,
) -> Result<Vec<(String, String)>> {
    if templates.is_empty() {
        return fail("cartridge", "has no templates/*.liquid files");
    }
    let mut out = Vec::new();
    for (name, entry) in templates {
        let file = format!("templates/{name}.liquid");
        let source = std::str::from_utf8(&contents[entry.name.as_str()])
            .map_err(|_| CartridgeError::new(file.as_str(), "is not valid UTF-8"))?;
        if abi < 2 {
            if name == "config" {
                return fail(
                    file.as_str(),
                    r#"is only allowed when "v" is 2"#,
                );
            }
            if source.contains("<lc-file") {
                return fail(file.as_str(), r#"<lc-file> is only allowed when "v" is 2"#);
            }
        }
        templates::parse(&file, source)?;
        let config_surface = name == "config";
        templates::check_controls(&file, source, controls, config_surface)?;
        if abi >= 2 {
            templates::check_file_pickers(&file, source)?;
        }
        out.push((name.clone(), source.to_string()));
    }
    Ok(out)
}

/// `<lc-canvas>` needs canvas.wasm, and canvas.wasm needs a template that places `<lc-canvas>`.
fn check_canvas_placement(templates: &[(String, String)], has_canvas: bool) -> Result<()> {
    let names: Vec<&str> = templates.iter().map(|(name, _)| name.as_str()).collect();
    let placing: Vec<&str> = js_key_order(&names)
        .into_iter()
        .filter(|name| {
            templates
                .iter()
                .any(|(template, source)| template == name && templates::places_canvas(source))
        })
        .collect();
    if !has_canvas {
        if let Some(first) = placing.first() {
            return fail(
                format!("templates/{first}.liquid"),
                "places <lc-canvas> but the Cartridge has no canvas.wasm",
            );
        }
        return Ok(());
    }
    if placing.is_empty() {
        return fail(
            "canvas.wasm",
            "is not placed by any template; add <lc-canvas> to a template or remove canvas.wasm",
        );
    }
    Ok(())
}

/// The order `Object.keys` gives: canonical array-index names first, ascending, then the rest in insertion order.
fn js_key_order<'a>(names: &[&'a str]) -> Vec<&'a str> {
    let mut indexed: Vec<(u32, &str)> = names
        .iter()
        .filter_map(|name| {
            let index: u32 = name.parse().ok()?;
            (index.to_string() == *name && index < u32::MAX).then_some((index, *name))
        })
        .collect();
    indexed.sort();
    let mut out: Vec<&str> = indexed.into_iter().map(|(_, name)| name).collect();
    let rest: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| !out.contains(name))
        .collect();
    out.extend(rest);
    out
}
