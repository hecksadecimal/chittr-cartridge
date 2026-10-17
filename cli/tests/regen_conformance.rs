//! Regenerates `conformance/cases` from the table below. The corpus is data for any validator (see
//! `conformance/README.md`), and this generator is how it was written. It is ignored by default, so the normal test
//! run only reads the corpus:
//!
//!     cargo test -p chittr-cartridge --test regen_conformance -- --ignored

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chittr_cartridge_cli::pack;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde_json::{json, Map, Value};

type Files = Vec<(String, Vec<u8>)>;

const MIB: u64 = 1024 * 1024;
const SENTINEL: &str = "cartridge: not a zip archive";

enum Content {
    Files(Files),
    Raw(Vec<u8>),
}

enum Want {
    Ok,
    /// The exact `file: rule`. A trailing `*` matches a prefix: the rule text of a compiler error differs by engine.
    Err(String),
}

struct Case {
    id: String,
    content: Content,
    limits: Vec<(&'static str, u64)>,
    want: Want,
}

fn ok() -> Want {
    Want::Ok
}

fn bad(text: impl Into<String>) -> Want {
    Want::Err(text.into())
}

fn files(id: &str, files: Files, want: Want) -> Case {
    Case {
        id: id.into(),
        content: Content::Files(files),
        limits: vec![],
        want,
    }
}

fn raw_limits(id: &str, bytes: Vec<u8>, limits: &[(&'static str, u64)], want: Want) -> Case {
    Case {
        id: id.into(),
        content: Content::Raw(bytes),
        limits: limits.to_vec(),
        want,
    }
}

fn files_limits(id: &str, files: Files, limits: &[(&'static str, u64)], want: Want) -> Case {
    Case {
        id: id.into(),
        content: Content::Files(files),
        limits: limits.to_vec(),
        want,
    }
}

fn raw(id: &str, bytes: Vec<u8>, want: Want) -> Case {
    Case {
        id: id.into(),
        content: Content::Raw(bytes),
        limits: vec![],
        want,
    }
}

// ─── WebAssembly fixtures, compiled from WAT ─────────────────────────────────

fn wasm(source: &str) -> Vec<u8> {
    wat::parse_str(source).expect("valid WAT")
}

fn server_wasm() -> Vec<u8> {
    wasm(
        r#"(module (memory (export "memory") 1) (func (export "render") (result i32) (i32.const 0)))"#,
    )
}

fn canvas_wasm() -> Vec<u8> {
    wasm(
        r#"(module (memory (export "memory") 1 1024) (func (export "start")) (func (export "frame")))"#,
    )
}

// ─── Shared file sets ────────────────────────────────────────────────────────

const CARD: &str = "<section><lc-author></lc-author><p>Hello</p></section>";

fn json_bytes(value: Value) -> Vec<u8> {
    value.to_string().into_bytes()
}

fn card(extra: &str) -> Vec<u8> {
    format!("<section><lc-author></lc-author><p>Hello</p>{extra}</section>").into_bytes()
}

fn base() -> Files {
    vec![
        (
            "manifest.json".into(),
            json_bytes(json!({"v": 1, "name": "Case", "version": "0.1.0"})),
        ),
        ("server.wasm".into(), server_wasm()),
        ("templates/card.liquid".into(), CARD.as_bytes().to_vec()),
    ]
}

fn put(mut files: Files, name: &str, bytes: Vec<u8>) -> Files {
    match files.iter_mut().find(|(existing, _)| existing == name) {
        Some(entry) => entry.1 = bytes,
        None => files.push((name.to_string(), bytes)),
    }
    files
}

fn without(files: Files, name: &str) -> Files {
    files
        .into_iter()
        .filter(|(existing, _)| existing != name)
        .collect()
}

fn with_manifest(value: Value) -> Files {
    put(base(), "manifest.json", json_bytes(value))
}

fn with_manifest_and_template(value: Value, template: &[u8]) -> Files {
    put(
        with_manifest(value),
        "templates/card.liquid",
        template.to_vec(),
    )
}

fn with_canvas(files: Files, template: &[u8]) -> Files {
    let files = put(files, "canvas.wasm", canvas_wasm());
    put(files, "templates/card.liquid", template.to_vec())
}

fn with_manifest_fonts(fonts: Value) -> Files {
    let mut files =
        with_manifest(json!({"v": 1, "name": "Case", "version": "0.1.0", "fonts": fonts}));
    files = put(files, "assets/mono.woff2", b"wOF2fake".to_vec());
    files
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("deflate");
    encoder.finish().expect("deflate")
}

// ─── A raw zip writer, for the layouts a packer won't write ──────────────────

struct RawEntry {
    name: String,
    method: u16,
    flags: u16,
    payload: Vec<u8>,
    declared: u32,
    cd_compressed: u32,
    cd_declared: u32,
    local_offset: Option<u32>,
}

fn entry(name: &str, method: u16, payload: Vec<u8>, declared: u32) -> RawEntry {
    RawEntry {
        name: name.into(),
        method,
        flags: 0,
        cd_compressed: payload.len() as u32,
        cd_declared: declared,
        payload,
        declared,
        local_offset: None,
    }
}

fn stored(name: &str, data: &[u8]) -> RawEntry {
    entry(name, 0, data.to_vec(), data.len() as u32)
}

fn deflated(name: &str, data: &[u8]) -> RawEntry {
    entry(name, 8, deflate(data), data.len() as u32)
}

fn build_raw(entries: &[RawEntry]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for item in entries {
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&item.flags.to_le_bytes());
        out.extend_from_slice(&item.method.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&(item.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&item.declared.to_le_bytes());
        out.extend_from_slice(&(item.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(item.name.as_bytes());
        out.extend_from_slice(&item.payload);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&item.flags.to_le_bytes());
        central.extend_from_slice(&item.method.to_le_bytes());
        central.extend_from_slice(&[0u8; 8]);
        central.extend_from_slice(&item.cd_compressed.to_le_bytes());
        central.extend_from_slice(&item.cd_declared.to_le_bytes());
        central.extend_from_slice(&(item.name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0u8; 12]);
        central.extend_from_slice(&item.local_offset.unwrap_or(offset).to_le_bytes());
        central.extend_from_slice(item.name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The archive's byte offset of the first occurrence of `signature`.
fn find(bytes: &[u8], signature: &[u8]) -> usize {
    bytes
        .windows(signature.len())
        .position(|window| window == signature)
        .expect("signature present")
}

fn set_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// The byte size of a raw archive's end-of-central-directory record, which has no comment.
const EOCD: usize = 22;

// ─── The corpus ──────────────────────────────────────────────────────────────

/// The first cumulative size that goes over `limit`, as the validator reports it.
fn first_over(sizes: &[u64], limit: u64) -> u64 {
    let mut total = 0;
    for size in sizes {
        total += size;
        if total > limit {
            return total;
        }
    }
    panic!("no entry goes over {limit}");
}

fn cases() -> Vec<Case> {
    let base_sizes = {
        let files = base();
        files
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .collect::<Vec<_>>()
    };
    let manifest_size = base_sizes[0];
    let server_size = base_sizes[1];
    let card_size = base_sizes[2];
    let oversized_manifest = {
        let mut bytes = json_bytes(json!({"v": 1, "name": "Big", "version": "1.0.0"}));
        bytes.extend(std::iter::repeat_n(b' ', 16_400));
        bytes
    };
    let long_name = "a".repeat(81);
    let long_version = "1".repeat(41);
    let long_template_name = format!("templates/{}.liquid", "a".repeat(65));
    let many_templates: Files = {
        let mut files = base();
        for i in 0..65 {
            files.push((format!("templates/t{i:02}.liquid"), b"<p>x</p>".to_vec()));
        }
        files
    };
    let big_text = vec![b'x'; 150 * 1024];
    let two_big_templates: Files = {
        let mut files = base();
        files.push(("templates/big1.liquid".into(), big_text.clone()));
        files.push(("templates/big2.liquid".into(), big_text.clone()));
        files
    };
    let bomb_payload_entries = {
        let mut entries = vec![
            stored(
                "manifest.json",
                &json_bytes(json!({"v": 1, "name": "Case", "version": "0.1.0"})),
            ),
            stored("server.wasm", &server_wasm()),
            stored("templates/card.liquid", CARD.as_bytes()),
        ];
        for i in 0..5 {
            let mut item = entry(
                &format!("assets/bomb{i}.bin"),
                8,
                Vec::new(),
                (15 * MIB) as u32,
            );
            item.cd_compressed = 0;
            entries.push(item);
        }
        entries
    };
    let bomb_total = first_over(
        &[
            manifest_size,
            server_size,
            card_size,
            15 * MIB,
            15 * MIB,
            15 * MIB,
            15 * MIB,
            15 * MIB,
        ],
        64 * MIB,
    );
    let mut cases = Vec::new();

    // Layout, archive and limits.
    cases.push(files("ok-minimal", base(), ok()));
    cases.push(files(
        "ok-canvas-placed",
        with_canvas(
            base(),
            &card("<lc-canvas width=\"320\" height=\"180\" label=\"Results\"></lc-canvas>"),
        ),
        ok(),
    ));
    cases.push(files(
        "ok-canvas-self-closing",
        with_canvas(base(), &card("<lc-canvas/>")),
        ok(),
    ));
    cases.push(files(
        "ok-assets-fonts-nested",
        {
            let files = with_manifest_fonts(json!({"Mono": "assets/mono.woff2"}));
            let files = put(files, "assets/img/logo.png", vec![0x89, b'P', b'N', b'G']);
            put(
                files,
                "templates/card.liquid",
                card("<img src=\"{{ 'img/logo.png' | asset_url }}\">"),
            )
        },
        ok(),
    ));
    cases.push(files(
        "ok-server-all-hooks",
        put(
            base(),
            "server.wasm",
            wasm(
                r#"(module (memory (export "memory") 1)
             (func (export "init")) (func (export "migrate")) (func (export "render"))
             (func (export "on_control")) (func (export "on_reply")) (func (export "on_reaction"))
             (func (export "on_rechitt")) (func (export "on_view")) (func (export "on_timer"))
             (func (export "on_canvas_message")))"#,
            ),
        ),
        ok(),
    ));
    cases.push(files("ok-canvas-event-export", with_canvas(base(), &card("<lc-canvas></lc-canvas>")).into_iter().map(|(name, bytes)| {
        if name == "canvas.wasm" {
            (name, wasm(r#"(module (memory (export "memory") 1 1024) (func (export "start")) (func (export "frame")) (func (export "event")))"#))
        } else {
            (name, bytes)
        }
    }).collect(), ok()));
    cases.push(files("ok-canvas-memory-unbounded", with_canvas(base(), &card("<lc-canvas></lc-canvas>")).into_iter().map(|(name, bytes)| {
        if name == "canvas.wasm" {
            (name, wasm(r#"(module (memory (export "memory") 1) (func (export "start")) (func (export "frame")))"#))
        } else {
            (name, bytes)
        }
    }).collect(), ok()));
    cases.push(files(
        "ok-unknown-filter-and-render-tag",
        put(
            base(),
            "templates/card.liquid",
            card("{{ 'x' | nosuchfilter }}{% render 'part' %}"),
        ),
        ok(),
    ));
    cases.push(files_limits(
        "ok-files-at-limit",
        base(),
        &[("files", 3)],
        ok(),
    ));
    cases.push(raw(
        "ok-directory-entries",
        build_raw(&[
            stored("templates/", b""),
            stored("assets/", b""),
            stored(
                "manifest.json",
                &json_bytes(json!({"v": 1, "name": "Case", "version": "0.1.0"})),
            ),
            stored("server.wasm", &server_wasm()),
            stored("templates/card.liquid", CARD.as_bytes()),
        ]),
        ok(),
    ));
    cases.push(files(
        "missing-manifest",
        without(base(), "manifest.json"),
        bad("manifest.json: is required but missing"),
    ));
    cases.push(files(
        "missing-server",
        without(base(), "server.wasm"),
        bad("server.wasm: is required but missing"),
    ));
    cases.push(files(
        "missing-templates",
        without(base(), "templates/card.liquid"),
        bad("cartridge: has no templates/*.liquid files"),
    ));
    cases.push(files(
        "unknown-root-file",
        put(base(), "notes.txt", b"hi".to_vec()),
        bad(format!("notes.txt: is not an allowed file; {ALLOWED}")),
    ));
    cases.push(files(
        "unknown-template-extension",
        put(base(), "templates/card.html", b"<p>hi</p>".to_vec()),
        bad(format!(
            "templates/card.html: is not an allowed file; {ALLOWED}"
        )),
    ));
    cases.push(files(
        "nested-template-refused",
        put(base(), "templates/sub/card.liquid", b"<p>hi</p>".to_vec()),
        bad(format!(
            "templates/sub/card.liquid: is not an allowed file; {ALLOWED}"
        )),
    ));
    cases.push(raw(
        "src-directory-refused",
        build_raw(&[stored("src/", b""), stored("manifest.json", b"{}")]),
        bad(format!("src/: is not an allowed location; {ALLOWED}")),
    ));
    cases.push(raw(
        "nested-template-directory-refused",
        build_raw(&[
            stored("templates/sub/", b""),
            stored("manifest.json", b"{}"),
        ]),
        bad(format!(
            "templates/sub/: is not an allowed location; {ALLOWED}"
        )),
    ));
    cases.push(raw(
        "directory-with-content",
        build_raw(&[
            entry("assets/", 0, b"abcd".to_vec(), 4),
            stored("manifest.json", b"{}"),
        ]),
        bad("assets/: is a directory but has content"),
    ));
    cases.push(raw(
        "backslash-name",
        build_raw(&[stored("templates\\card.liquid", b"<p>x</p>")]),
        bad("templates\\card.liquid: must be a relative path with forward slashes"),
    ));
    cases.push(raw(
        "absolute-name",
        build_raw(&[stored("/manifest.json", b"{}")]),
        bad("/manifest.json: must be a relative path with forward slashes"),
    ));
    cases.push(raw(
        "dotdot-segment",
        build_raw(&[stored("assets/../server.wasm", b"x")]),
        bad("assets/../server.wasm: contains an empty, '.' or '..' path segment"),
    ));
    cases.push(raw(
        "empty-segment",
        build_raw(&[stored("assets//a.txt", b"x")]),
        bad("assets//a.txt: contains an empty, '.' or '..' path segment"),
    ));
    cases.push(raw(
        "dot-segment",
        build_raw(&[stored("./manifest.json", b"{}")]),
        bad("./manifest.json: contains an empty, '.' or '..' path segment"),
    ));
    cases.push(raw(
        "duplicate-entry",
        build_raw(&[stored("server.wasm", b"x"), stored("server.wasm", b"y")]),
        bad("server.wasm: appears more than once in the archive"),
    ));
    cases.push(raw(
        "encrypted-entry",
        {
            let mut item = stored("manifest.json", b"{}");
            item.flags = 1;
            build_raw(&[item])
        },
        bad("manifest.json: is encrypted; encrypted entries are not supported"),
    ));
    cases.push(raw("unsupported-method", build_raw(&[entry("assets/a.bin", 12, b"abc".to_vec(), 3)]), bad("assets/a.bin: uses compression method 12; only deflate (8) and stored (0) are supported")));
    cases.push(raw(
        "zip64-end-record",
        {
            let mut bytes = build_raw(&[stored("manifest.json", b"{}")]);
            let eocd = bytes.len() - EOCD;
            set_u32(&mut bytes, eocd + 16, 0xffff_ffff);
            bytes
        },
        bad("cartridge: ZIP64 archives are not supported"),
    ));
    cases.push(raw(
        "zip64-entry",
        {
            let mut item = stored("manifest.json", b"{}");
            item.local_offset = Some(0xffff_ffff);
            build_raw(&[item])
        },
        bad("cartridge: ZIP64 archives are not supported"),
    ));
    cases.push(raw(
        "not-a-zip",
        b"this is plainly not a zip archive, nothing in it is a central directory".to_vec(),
        bad(SENTINEL),
    ));
    cases.push(raw(
        "central-directory-corrupt",
        {
            let mut bytes = build_raw(&[stored("manifest.json", b"{}")]);
            let at = find(&bytes, &[0x50, 0x4b, 0x01, 0x02]);
            bytes[at + 3] = 0x09;
            bytes
        },
        bad("cartridge: central directory is corrupt"),
    ));
    cases.push(raw(
        "central-directory-past-end",
        {
            let mut bytes = build_raw(&[stored("manifest.json", b"{}")]);
            let eocd = bytes.len() - EOCD;
            let size = (bytes.len() * 2) as u32;
            set_u32(&mut bytes, eocd + 12, size);
            bytes
        },
        bad("cartridge: central directory runs past the end of the archive"),
    ));
    cases.push(raw(
        "local-header-corrupt",
        {
            let mut bytes = build_raw(&[stored("manifest.json", b"{}")]);
            bytes[3] = 0x05;
            bytes
        },
        bad("manifest.json: local file header is corrupt"),
    ));
    cases.push(raw(
        "data-past-end",
        {
            let mut item = stored("manifest.json", b"{}");
            item.cd_compressed = 1_000_000;
            build_raw(&[item])
        },
        bad("manifest.json: data runs past the end of the archive"),
    ));
    cases.push(raw(
        "stored-size-mismatch",
        {
            let mut item = stored("server.wasm", &server_wasm());
            item.cd_declared += 10;
            item.declared += 10;
            build_raw(&[item])
        },
        bad("server.wasm: stored size does not match its declared size"),
    ));
    cases.push(raw(
        "inflate-past-declared",
        {
            let mut item = deflated("templates/card.liquid", &vec![b'a'; 2000]);
            item.cd_declared = 100;
            build_raw(&[item])
        },
        bad("templates/card.liquid: inflates past its declared size of 100 bytes"),
    ));
    cases.push(raw(
        "inflated-size-mismatch",
        {
            let mut item = deflated("templates/card.liquid", b"<p>short</p>");
            item.cd_declared = 50;
            build_raw(&[item])
        },
        bad("templates/card.liquid: inflated to 12 bytes but declares 50"),
    ));
    cases.push(raw(
        "corrupt-deflate-stream",
        build_raw(&[entry(
            "templates/card.liquid",
            8,
            vec![0xff, 0xff, 0xff, 0xff],
            10,
        )]),
        bad("templates/card.liquid: could not be inflated*"),
    ));
    cases.push(raw(
        "zip-bomb-understated-size",
        build_raw(&[{
            let mut item = deflated("assets/bomb.bin", &vec![0u8; 1024 * 1024]);
            item.cd_declared = 1000;
            item.declared = 1000;
            item
        }]),
        bad("assets/bomb.bin: inflates past its declared size of 1000 bytes"),
    ));
    cases.push(raw(
        "zip-bomb-declared-total",
        build_raw(&bomb_payload_entries),
        bad(format!(
            "cartridge: files inflate to {bomb_total} bytes; the limit is {}",
            64 * MIB
        )),
    ));
    cases.push(files(
        "templates-too-many",
        many_templates,
        bad("cartridge: has 66 templates; the limit is 64"),
    ));
    cases.push(files(
        "templates-too-large",
        two_big_templates,
        bad(format!(
            "cartridge: templates are {} bytes; the limit is 262144",
            2 * 150 * 1024 + CARD.len()
        )),
    ));
    cases.push(files(
        "template-name-too-long",
        put(base(), &long_template_name, b"<p>x</p>".to_vec()),
        bad(format!(
            "{long_template_name}: template name is longer than 64 characters"
        )),
    ));
    {
        // Committed as bytes, so every validator sees the same archive whatever packer it was written with.
        let archive = pack::zip(&base()).expect("pack");
        let limit = archive.len() as u64 - 1;
        cases.push(raw_limits(
            "archive-over-limit",
            archive.clone(),
            &[("archiveBytes", limit)],
            bad(format!(
                "cartridge: archive is {} bytes; the limit is {limit}",
                archive.len()
            )),
        ));
    }
    cases.push(files_limits(
        "files-over-limit",
        base(),
        &[("files", 2)],
        bad("cartridge: archive has 3 files; the limit is 2"),
    ));
    cases.push(files_limits(
        "inflated-over-limit",
        base(),
        &[("inflatedBytes", 100)],
        bad(format!(
            "cartridge: files inflate to {} bytes; the limit is 100",
            first_over(&[manifest_size, server_size, card_size], 100)
        )),
    ));
    cases.push(files_limits(
        "template-bytes-over-limit",
        base(),
        &[("templateBytes", 4)],
        bad(format!(
            "cartridge: templates are {card_size} bytes; the limit is 4"
        )),
    ));
    cases.push(files_limits(
        "template-files-over-limit",
        put(base(), "templates/extra.liquid", b"<p>x</p>".to_vec()),
        &[("templateFiles", 1)],
        bad("cartridge: has 2 templates; the limit is 1"),
    ));
    cases.push(files_limits(
        "wasm-over-limit",
        base(),
        &[("wasmBytes", 10)],
        bad(format!(
            "server.wasm: is {server_size} bytes; the limit is 10"
        )),
    ));
    cases.push(files_limits(
        "asset-over-limit",
        put(base(), "assets/big.bin", vec![0u8; 11]),
        &[("assetBytes", 10)],
        bad("assets/big.bin: is 11 bytes; the limit is 10"),
    ));
    cases.push(files(
        "manifest-over-limit",
        put(base(), "manifest.json", oversized_manifest.clone()),
        bad(format!(
            "manifest.json: is {} bytes; the limit is 16384",
            oversized_manifest.len()
        )),
    ));

    // Manifest keys, version, name and version fields, fonts.
    cases.push(files(
        "manifest-not-utf8",
        put(base(), "manifest.json", vec![0xff, 0xfe, 0x00]),
        bad("manifest.json: is not valid UTF-8 JSON"),
    ));
    cases.push(files(
        "manifest-not-json",
        put(base(), "manifest.json", b"{".to_vec()),
        bad("manifest.json: is not valid UTF-8 JSON"),
    ));
    cases.push(files(
        "manifest-not-object",
        put(base(), "manifest.json", b"[]".to_vec()),
        bad("manifest.json: must be a JSON object"),
    ));
    cases.push(files("manifest-unknown-key", with_manifest(json!({"v": 1, "name": "A", "version": "1", "color": "red"})), bad(r#"manifest.json: has an unknown key "color"; allowed keys are v, name, version, fonts, controls"#)));
    cases.push(files(
        "manifest-missing-v",
        with_manifest(json!({"name": "A", "version": "1"})),
        bad(r#"manifest.json: is missing "v""#),
    ));
    cases.push(files(
        "manifest-v-2",
        with_manifest(json!({"v": 2, "name": "A", "version": "1"})),
        bad(r#"manifest.json: "v" must be 1"#),
    ));
    cases.push(files(
        "manifest-v-string",
        with_manifest(json!({"v": "1", "name": "A", "version": "1"})),
        bad(r#"manifest.json: "v" must be 1"#),
    ));
    cases.push(files(
        "manifest-missing-name",
        with_manifest(json!({"v": 1, "version": "1"})),
        bad(r#"manifest.json: is missing "name""#),
    ));
    cases.push(files(
        "manifest-name-not-string",
        with_manifest(json!({"v": 1, "name": 5, "version": "1"})),
        bad(r#"manifest.json: "name" must be a string"#),
    ));
    cases.push(files(
        "manifest-name-blank",
        with_manifest(json!({"v": 1, "name": "   ", "version": "1"})),
        bad(r#"manifest.json: "name" must not be empty"#),
    ));
    cases.push(files(
        "manifest-name-too-long",
        with_manifest(json!({"v": 1, "name": long_name, "version": "1"})),
        bad(r#"manifest.json: "name" is longer than 80 characters"#),
    ));
    cases.push(files(
        "manifest-missing-version",
        with_manifest(json!({"v": 1, "name": "A"})),
        bad(r#"manifest.json: is missing "version""#),
    ));
    cases.push(files(
        "manifest-version-too-long",
        with_manifest(json!({"v": 1, "name": "A", "version": long_version})),
        bad(r#"manifest.json: "version" is longer than 40 characters"#),
    ));
    cases.push(files(
        "manifest-fonts-not-object",
        with_manifest(json!({"v": 1, "name": "A", "version": "1", "fonts": []})),
        bad(r#"manifest.json: "fonts" must be an object mapping font names to asset paths"#),
    ));
    cases.push(files("font-name-invalid", with_manifest_fonts(json!({"1Mono": "assets/mono.woff2"})), bad(r#"manifest.json: font name "1Mono" must start with a letter and use only letters, digits, "-" or "_" (at most 32 characters)"#)));
    cases.push(files(
        "font-path-not-string",
        with_manifest_fonts(json!({"Mono": 3})),
        bad(r#"manifest.json: font "Mono" must be a path string"#),
    ));
    cases.push(files(
        "font-path-outside-assets",
        with_manifest_fonts(json!({"Mono": "fonts/mono.woff2"})),
        bad(r#"manifest.json: font "Mono" path "fonts/mono.woff2" must be a file under assets/"#),
    ));
    cases.push(files("font-path-traversal", with_manifest_fonts(json!({"Mono": "assets/../mono.woff2"})), bad(r#"manifest.json: font "Mono" path "assets/../mono.woff2" must be a file under assets/"#)));
    cases.push(files("font-not-in-archive", with_manifest_fonts(json!({"Mono": "assets/missing.woff2"})), bad(r#"manifest.json: font "Mono" names "assets/missing.woff2", which the archive does not carry"#)));
    cases.push(files(
        "ok-font",
        with_manifest_fonts(json!({"Mono": "assets/mono.woff2"})),
        ok(),
    ));

    // Controls.
    let vote = json!({"value": {"enum": ["a", "b"]}});
    let manifest_with_control = |control: Value| {
        with_manifest(json!({"v": 1, "name": "A", "version": "1", "controls": {"vote": control}}))
    };
    cases.push(files(
        "controls-not-object",
        with_manifest(json!({"v": 1, "name": "A", "version": "1", "controls": []})),
        bad(r#"manifest.json: "controls" must be an object mapping Control names to declarations"#),
    ));
    cases.push(files("control-name-invalid", with_manifest(json!({"v": 1, "name": "A", "version": "1", "controls": {"Vote": vote.clone()}})), bad(r#"manifest.json: Control name "Vote" must start with a lowercase letter and use only lowercase letters, digits, "-" or "_" (at most 32 characters)"#)));
    cases.push(files(
        "control-declaration-not-object",
        manifest_with_control(json!(1)),
        bad(r#"manifest.json: Control "vote" must be { "value": ... } or { "fields": ... }"#),
    ));
    cases.push(files(
        "control-value-and-fields",
        manifest_with_control(
            json!({"value": {"enum": ["a"]}, "fields": {"t": {"type": "string"}}}),
        ),
        bad(r#"manifest.json: Control "vote" must declare exactly one of "value" or "fields""#),
    ));
    cases.push(files(
        "control-empty-declaration",
        manifest_with_control(json!({})),
        bad(r#"manifest.json: Control "vote" must declare exactly one of "value" or "fields""#),
    ));
    cases.push(files(
        "enum-extra-key",
        manifest_with_control(json!({"value": {"enum": ["a"], "extra": 1}})),
        bad(r#"manifest.json: Control "vote" value must be { "enum": [...] }"#),
    ));
    cases.push(files(
        "enum-empty",
        manifest_with_control(json!({"value": {"enum": []}})),
        bad(r#"manifest.json: Control "vote" enum must be a non-empty array"#),
    ));
    let many: Vec<String> = (0..101).map(|i| format!("v{i}")).collect();
    cases.push(files(
        "enum-too-many",
        manifest_with_control(json!({"value": {"enum": many}})),
        bad(r#"manifest.json: Control "vote" enum has 101 values; the limit is 100"#),
    ));
    cases.push(files(
        "enum-value-empty",
        manifest_with_control(json!({"value": {"enum": [""]}})),
        bad(r#"manifest.json: Control "vote" enum values must be strings of 1 to 64 characters"#),
    ));
    cases.push(files(
        "enum-value-too-long",
        manifest_with_control(json!({"value": {"enum": ["x".repeat(65)]}})),
        bad(r#"manifest.json: Control "vote" enum values must be strings of 1 to 64 characters"#),
    ));
    cases.push(files(
        "enum-value-repeated",
        manifest_with_control(json!({"value": {"enum": ["a", "a"]}})),
        bad(r#"manifest.json: Control "vote" enum repeats "a""#),
    ));
    let many_controls: Map<String, Value> = (0..33)
        .map(|i| (format!("c{i}"), json!({"value": {"enum": ["a"]}})))
        .collect();
    cases.push(files(
        "controls-too-many",
        with_manifest(json!({"v": 1, "name": "A", "version": "1", "controls": many_controls})),
        bad(r#"manifest.json: "controls" declares 33 Controls; the limit is 32"#),
    ));
    cases.push(files(
        "fields-empty",
        manifest_with_control(json!({"fields": {}})),
        bad(r#"manifest.json: Control "vote" must declare 1 to 16 fields"#),
    ));
    let many_fields: Map<String, Value> = (0..17)
        .map(|i| (format!("f{i}"), json!({"type": "string"})))
        .collect();
    cases.push(files(
        "fields-too-many",
        manifest_with_control(json!({"fields": many_fields})),
        bad(r#"manifest.json: Control "vote" must declare 1 to 16 fields"#),
    ));
    cases.push(files("field-name-invalid", manifest_with_control(json!({"fields": {"1x": {"type": "string"}}})), bad(r#"manifest.json: Control "vote" field "1x" must start with a letter and use only letters, digits, "-" or "_""#)));
    cases.push(files(
        "field-not-object",
        manifest_with_control(json!({"fields": {"t": 1}})),
        bad(r#"manifest.json: Control "vote" field "t" must be an object with a "type""#),
    ));
    cases.push(files(
        "field-unknown-key",
        manifest_with_control(json!({"fields": {"t": {"type": "string", "pattern": "x"}}})),
        bad(r#"manifest.json: Control "vote" field "t" has an unknown key "pattern""#),
    ));
    cases.push(files("field-bad-type", manifest_with_control(json!({"fields": {"t": {"type": "date"}}})), bad(r#"manifest.json: Control "vote" field "t" type must be "string", "number" or "boolean""#)));
    cases.push(files("field-maxlength-on-number", manifest_with_control(json!({"fields": {"n": {"type": "number", "maxLength": 5}}})), bad(r#"manifest.json: Control "vote" field "n" can set maxLength only when its type is "string""#)));
    cases.push(files("field-maxlength-out-of-range", manifest_with_control(json!({"fields": {"t": {"type": "string", "maxLength": 0}}})), bad(r#"manifest.json: Control "vote" field "t" maxLength must be an integer from 1 to 2000"#)));
    cases.push(files("field-min-on-string", manifest_with_control(json!({"fields": {"t": {"type": "string", "min": 1}}})), bad(r#"manifest.json: Control "vote" field "t" can set min and max only when its type is "number""#)));
    cases.push(files(
        "field-min-not-number",
        manifest_with_control(json!({"fields": {"n": {"type": "number", "min": "1"}}})),
        bad(r#"manifest.json: Control "vote" field "n" min must be a finite number"#),
    ));
    cases.push(files(
        "field-min-above-max",
        manifest_with_control(json!({"fields": {"n": {"type": "number", "min": 5, "max": 1}}})),
        bad(r#"manifest.json: Control "vote" field "n" min is larger than max"#),
    ));
    let controls_ok = json!({"v": 1, "name": "A", "version": "1", "controls": {
        "vote": {"value": {"enum": ["a", "b"]}},
        "comment": {"fields": {"text": {"type": "string", "maxLength": 280}, "stars": {"type": "number", "min": 1, "max": 5}, "yes": {"type": "boolean"}}}
    }});
    cases.push(files("ok-controls", with_manifest_and_template(controls_ok.clone(), &card("<button data-lc-control=\"vote\" data-lc-value=\"a\">a</button><form data-lc-control=\"comment\"></form>")), ok()));
    cases.push(files(
        "ok-control-attribute-quoting",
        with_manifest_and_template(
            controls_ok,
            &card(
                "<button data-lc-control='vote'>a</button><button data-lc-control=vote>b</button>",
            ),
        ),
        ok(),
    ));
    cases.push(files("template-undeclared-control", put(base(), "templates/card.liquid", card("<button data-lc-control=\"vote\">a</button>")), bad(r#"templates/card.liquid: uses the undeclared Control "vote"; declare it under "controls" in manifest.json"#)));
    cases.push(files("template-dynamic-control", with_manifest_and_template(json!({"v": 1, "name": "A", "version": "1", "controls": {"vote": vote.clone()}}), &card("<button data-lc-control=\"{{ name }}\">a</button>")), bad("templates/card.liquid: binds a Control whose name comes from Liquid; data-lc-control must name the Control literally")));
    cases.push(files("template-empty-control-name", put(base(), "templates/card.liquid", card("<button data-lc-control=\"\">a</button>")), bad(r#"templates/card.liquid: uses the undeclared Control ""; declare it under "controls" in manifest.json"#)));

    // Templates.
    cases.push(files(
        "template-not-utf8",
        put(base(), "templates/card.liquid", vec![0xff, 0xfe]),
        bad("templates/card.liquid: is not valid UTF-8"),
    ));
    cases.push(files(
        "template-unclosed-if",
        put(
            base(),
            "templates/card.liquid",
            b"{% if x %}<p>x</p>".to_vec(),
        ),
        bad("templates/card.liquid: does not parse as LiquidJS*"),
    ));
    cases.push(files(
        "template-unknown-tag",
        put(base(), "templates/card.liquid", b"{% nosuchtag %}".to_vec()),
        bad("templates/card.liquid: does not parse as LiquidJS*"),
    ));

    // Canvas placement.
    cases.push(files("canvas-not-placed", put(base(), "canvas.wasm", canvas_wasm()), bad("canvas.wasm: is not placed by any template; add <lc-canvas> to a template or remove canvas.wasm")));
    cases.push(files(
        "canvas-placed-without-canvas",
        put(
            base(),
            "templates/card.liquid",
            card("<lc-canvas></lc-canvas>"),
        ),
        bad("templates/card.liquid: places <lc-canvas> but the Cartridge has no canvas.wasm"),
    ));

    // Server exports.
    cases.push(files(
        "server-not-wasm",
        put(base(), "server.wasm", b"not wasm at all".to_vec()),
        bad("server.wasm: is not a valid WebAssembly module: *"),
    ));
    cases.push(files(
        "server-missing-render",
        put(
            base(),
            "server.wasm",
            wasm(r#"(module (memory (export "memory") 1) (func (export "init")))"#),
        ),
        bad(r#"server.wasm: must export the "render" Hook"#),
    ));
    cases.push(files("server-unknown-export", put(base(), "server.wasm", wasm(r#"(module (func (export "render")) (func (export "frobnicate")))"#)), bad(r#"server.wasm: export "frobnicate" is not a known Hook; known Hooks are init, migrate, render, on_control, on_reply, on_reaction, on_rechitt, on_view, on_timer, on_canvas_message"#)));
    cases.push(files(
        "server-global-export",
        put(
            base(),
            "server.wasm",
            wasm(r#"(module (func (export "render")) (global (export "g") i32 (i32.const 0)))"#),
        ),
        bad(r#"server.wasm: export "g" is a global; only functions and "memory" may be exported"#),
    ));
    cases.push(files(
        "server-table-export",
        put(
            base(),
            "server.wasm",
            wasm(r#"(module (func (export "render")) (table (export "t") 1 funcref))"#),
        ),
        bad(r#"server.wasm: export "t" is a table; only functions and "memory" may be exported"#),
    ));
    cases.push(files(
        "server-memory-other-name",
        put(
            base(),
            "server.wasm",
            wasm(r#"(module (func (export "render")) (memory (export "mem") 1))"#),
        ),
        bad(
            r#"server.wasm: export "mem" is a memory; only functions and "memory" may be exported"#,
        ),
    ));

    // Canvas exports and memory.
    let canvas_with = |source: &str| {
        with_canvas(base(), &card("<lc-canvas></lc-canvas>"))
            .into_iter()
            .map(|(name, bytes)| {
                if name == "canvas.wasm" {
                    (name, wasm(source))
                } else {
                    (name, bytes)
                }
            })
            .collect::<Files>()
    };
    cases.push(files(
        "canvas-not-wasm",
        put(base(), "canvas.wasm", b"not wasm either".to_vec()),
        bad("canvas.wasm: is not a valid WebAssembly module: *"),
    ));
    cases.push(files(
        "canvas-missing-frame",
        canvas_with(r#"(module (memory (export "memory") 1) (func (export "start")))"#),
        bad(r#"canvas.wasm: must export the "frame" export"#),
    ));
    cases.push(files(
        "canvas-missing-start",
        canvas_with(r#"(module (memory (export "memory") 1) (func (export "frame")))"#),
        bad(r#"canvas.wasm: must export the "start" export"#),
    ));
    cases.push(files("canvas-extra-export", canvas_with(r#"(module (memory (export "memory") 1) (func (export "start")) (func (export "frame")) (func (export "render")))"#), bad("canvas.wasm: export \"render\" is not a canvas export; canvas exports are start, frame, event")));
    cases.push(files(
        "canvas-no-memory",
        canvas_with(r#"(module (func (export "start")) (func (export "frame")))"#),
        bad(r#"canvas.wasm: must export "memory""#),
    ));
    cases.push(files("canvas-global-export", canvas_with(r#"(module (memory (export "memory") 1) (func (export "start")) (func (export "frame")) (global (export "g") i32 (i32.const 0)))"#), bad(r#"canvas.wasm: export "g" is a global; only functions and "memory" may be exported"#)));
    cases.push(files("canvas-memory-too-large", canvas_with(r#"(module (memory (export "memory") 1025) (func (export "start")) (func (export "frame")))"#), bad("canvas.wasm: the module asks for 1025 pages (64.0625 MiB) to start; the cap is 1024 pages")));
    cases.push(files("canvas-memory-max-too-large", canvas_with(r#"(module (memory (export "memory") 1 1025) (func (export "start")) (func (export "frame")))"#), bad("canvas.wasm: the module's memory maximum is 1025 pages (64.0625 MiB); the cap is 1024 pages")));
    cases.push(files("canvas-imported-memory-too-large", canvas_with(r#"(module (import "env" "memory" (memory 1025)) (export "memory" (memory 0)) (func (export "start")) (func (export "frame")))"#), bad("canvas.wasm: the module asks for 1025 pages (64.0625 MiB) to start; the cap is 1024 pages")));
    cases.push(files("ok-canvas-memory-at-cap", canvas_with(r#"(module (memory (export "memory") 1 1024) (func (export "start")) (func (export "frame")) (func (export "event")))"#), ok()));

    cases
}

const ALLOWED: &str = "Cartridges may contain only manifest.json, server.wasm, canvas.wasm, templates/*.liquid and assets/**";

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance/cases")
}

fn write_case(root: &Path, case: &Case) {
    let dir = root.join(&case.id);
    fs::create_dir_all(&dir).expect("case dir");
    match &case.content {
        Content::Files(files) => {
            for (name, bytes) in files {
                let path = dir.join("files").join(name);
                fs::create_dir_all(path.parent().expect("parent")).expect("files dir");
                fs::write(&path, bytes).expect("file");
            }
        }
        Content::Raw(bytes) => fs::write(dir.join("archive.zip"), bytes).expect("archive"),
    }
    let mut expected = Map::new();
    match &case.want {
        Want::Ok => {
            expected.insert("ok".into(), json!(true));
        }
        Want::Err(error) => {
            expected.insert("ok".into(), json!(false));
            expected.insert("error".into(), json!(error));
        }
    }
    if !case.limits.is_empty() {
        let limits: Map<String, Value> = case
            .limits
            .iter()
            .map(|(key, value)| (key.to_string(), json!(value)))
            .collect();
        expected.insert("limits".into(), Value::Object(limits));
    }
    let text = serde_json::to_string_pretty(&Value::Object(expected)).expect("json") + "\n";
    fs::write(dir.join("expected.json"), text).expect("expected.json");
}

#[test]
#[ignore = "regenerates conformance/cases; run with --ignored"]
fn regenerate_conformance_corpus() {
    let cases = cases();
    let mut ids: Vec<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), cases.len(), "case ids are unique");
    let root = corpus_root();
    if root.exists() {
        fs::remove_dir_all(&root).expect("clear corpus");
    }
    fs::create_dir_all(&root).expect("corpus root");
    for case in &cases {
        write_case(&root, case);
    }
    eprintln!("wrote {} cases to {}", cases.len(), root.display());
}
