//! The Guest modules: `server.wasm` and `canvas.wasm` must compile, export only what their ABI allows, and (for
//! canvas) declare memory within the cap. The server's V8 compile is authoritative; wasmparser's validation stands in
//! for it here, so compile-error text differs and the corpus matches those by prefix.

use wasmparser::{ExternalKind, Parser, Payload, TypeRef, Validator};

use crate::error::{fail, CartridgeError, Result};

/// Hook names the Server Guest may export in the validator's order (ABI v1).
pub const HOOK_NAMES: [&str; 10] = [
    "init",
    "migrate",
    "render",
    "on_control",
    "on_reply",
    "on_reaction",
    "on_rechitt",
    "on_view",
    "on_timer",
    "on_canvas_message",
];

/// Hook names only a v2 Server Guest may export.
pub const HOOK_NAMES_V2: [&str; 2] = ["on_file", "render_config"];

/// The exports a canvas guest must have, and the one it may have .
pub const CANVAS_REQUIRED_EXPORTS: [&str; 2] = ["start", "frame"];
pub const CANVAS_OPTIONAL_EXPORTS: [&str; 1] = ["event"];

/// The canvas memory cap in wasm pages: 64 MiB.
pub const CANVAS_MEMORY_PAGES: u64 = 1024;
const PAGE_BYTES: f64 = 64.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Function,
    Table,
    Memory,
    Global,
    Tag,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Table => "table",
            Kind::Memory => "memory",
            Kind::Global => "global",
            Kind::Tag => "tag",
        }
    }
}

fn compile(file: &str, bytes: &[u8]) -> Result<()> {
    Validator::new()
        .validate_all(bytes)
        .map(|_| ())
        .or_else(|error| fail(file, format!("is not a valid WebAssembly module: {error}")))
}

fn read_exports(file: &str, bytes: &[u8]) -> Result<Vec<(String, Kind)>> {
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload
            .or_else(|error| fail(file, format!("is not a valid WebAssembly module: {error}")))?;
        if let Payload::ExportSection(section) = payload {
            for export in section {
                let export = export.or_else(|error| {
                    fail(file, format!("is not a valid WebAssembly module: {error}"))
                })?;
                let kind = match export.kind {
                    ExternalKind::Func | ExternalKind::FuncExact => Kind::Function,
                    ExternalKind::Table => Kind::Table,
                    ExternalKind::Memory => Kind::Memory,
                    ExternalKind::Global => Kind::Global,
                    ExternalKind::Tag => Kind::Tag,
                };
                exports.push((export.name.to_string(), kind));
            }
        }
    }
    Ok(exports)
}

/// Compiles `server.wasm`, checks every export is a known Hook function (or `memory`), and returns the Hooks in export
/// order. `render` must be among them.
pub fn check_server(bytes: &[u8], abi: u32) -> Result<Vec<String>> {
    const FILE: &str = "server.wasm";
    compile(FILE, bytes)?;
    let allowed: Vec<&str> = if abi >= 2 {
        HOOK_NAMES
            .iter()
            .chain(HOOK_NAMES_V2.iter())
            .copied()
            .collect()
    } else {
        HOOK_NAMES.to_vec()
    };
    let mut hooks = Vec::new();
    for (name, kind) in &read_exports(FILE, bytes)? {
        if *kind == Kind::Memory && name == "memory" {
            continue;
        }
        if *kind != Kind::Function {
            return fail(
                FILE,
                format!(
                    r#"export "{name}" is a {}; only functions and "memory" may be exported"#,
                    kind.name()
                ),
            );
        }
        if !allowed.contains(&name.as_str()) {
            return fail(
                FILE,
                format!(
                    r#"export "{name}" is not a known Hook; known Hooks are {}"#,
                    allowed.join(", ")
                ),
            );
        }
        hooks.push(name.clone());
    }
    if !hooks.iter().any(|name| name == "render") {
        return fail(FILE, r#"must export the "render" Hook"#);
    }
    Ok(hooks)
}

/// Compiles `canvas.wasm`, checks its exports (start, frame, optional event, memory), and caps its declared memory.
pub fn check_canvas(bytes: &[u8]) -> Result<()> {
    const FILE: &str = "canvas.wasm";
    compile(FILE, bytes)?;
    let exports = read_exports(FILE, bytes)?;
    let mut found: Vec<&str> = Vec::new();
    let mut memory = false;
    let allowed: Vec<&str> = CANVAS_REQUIRED_EXPORTS
        .iter()
        .chain(CANVAS_OPTIONAL_EXPORTS.iter())
        .copied()
        .collect();
    for (name, kind) in &exports {
        if *kind == Kind::Memory && name == "memory" {
            memory = true;
            continue;
        }
        if *kind != Kind::Function {
            return fail(
                FILE,
                format!(
                    r#"export "{name}" is a {}; only functions and "memory" may be exported"#,
                    kind.name()
                ),
            );
        }
        if !allowed.contains(&name.as_str()) {
            return fail(
                FILE,
                format!(
                    r#"export "{name}" is not a canvas export; canvas exports are {}"#,
                    allowed.join(", ")
                ),
            );
        }
        found.push(name.as_str());
    }
    for name in CANVAS_REQUIRED_EXPORTS {
        if !found.contains(&name) {
            return fail(FILE, format!(r#"must export the "{name}" export"#));
        }
    }
    if !memory {
        return fail(FILE, r#"must export "memory""#);
    }
    check_memories(FILE, bytes)
}

/// Every memory the module declares or imports must start within the cap, and any declared maximum must be within it too.
fn check_memories(file: &str, bytes: &[u8]) -> Result<()> {
    let cap_error = |rule: String| -> CartridgeError { CartridgeError::new(file, rule) };
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload
            .map_err(|error| cap_error(format!("is not a valid WebAssembly module: {error}")))?;
        let memories: Vec<wasmparser::MemoryType> = match payload {
            Payload::ImportSection(section) => {
                let mut out = Vec::new();
                for group in section {
                    let group = group.map_err(|error| {
                        cap_error(format!("is not a valid WebAssembly module: {error}"))
                    })?;
                    for item in group {
                        let (_, import) = item.map_err(|error| {
                            cap_error(format!("is not a valid WebAssembly module: {error}"))
                        })?;
                        if let TypeRef::Memory(memory) = import.ty {
                            out.push(memory);
                        }
                    }
                }
                out
            }
            Payload::MemorySection(section) => {
                let mut out = Vec::new();
                for memory in section {
                    out.push(memory.map_err(|error| {
                        cap_error(format!("is not a valid WebAssembly module: {error}"))
                    })?);
                }
                out
            }
            _ => continue,
        };
        for memory in memories {
            if memory.memory64 {
                return Err(cap_error("64-bit memories are not supported".to_string()));
            }
            if memory.initial > CANVAS_MEMORY_PAGES {
                return Err(cap_error(format!(
                    "the module asks for {} pages ({} MiB) to start; the cap is {CANVAS_MEMORY_PAGES} pages",
                    memory.initial,
                    memory.initial as f64 * PAGE_BYTES / MIB
                )));
            }
            if let Some(maximum) = memory.maximum {
                if maximum > CANVAS_MEMORY_PAGES {
                    return Err(cap_error(format!(
                        "the module's memory maximum is {maximum} pages ({} MiB); the cap is {CANVAS_MEMORY_PAGES} pages",
                        maximum as f64 * PAGE_BYTES / MIB
                    )));
                }
            }
        }
    }
    Ok(())
}
