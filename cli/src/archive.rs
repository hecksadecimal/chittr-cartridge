//! Reads a `.cartridge` the way the server's validator does: the central directory first, so every name, size and
//! offset is checked before any byte is inflated; then each allowed entry, with its declared size enforced while it
//! inflates. Only deflate (8) and stored (0) entries are accepted. ZIP64 and encrypted entries are refused.

use std::collections::HashSet;
use std::io::Read;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::{fail, Result};
use crate::limits::Limits;

const EOCD_SIG: u32 = 0x0605_4b50;
const CD_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const EOCD_MIN: usize = 22;
const EOCD_MAX_SEARCH: usize = EOCD_MIN + 0xffff;
const ALLOWED: &str = "Cartridges may contain only manifest.json, server.wasm, canvas.wasm, templates/*.liquid and assets/**";

static TEMPLATE_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^templates/([A-Za-z0-9_-]+)\.liquid$").expect("valid regex"));

/// One central-directory entry, before any of its data is read.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub compressed_size: u64,
    pub declared_size: u64,
    pub local_offset: u64,
}

/// The entries the validator will read, grouped by class, in archive order.
#[derive(Debug, Default)]
pub struct Plan {
    pub files: Vec<Entry>,
    pub templates: Vec<(String, Entry)>,
    pub assets: Vec<Entry>,
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Finds the end-of-central-directory record, searching back over at most one maximal comment.
fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < EOCD_MIN {
        return None;
    }
    let lowest = bytes.len().saturating_sub(EOCD_MAX_SEARCH);
    let mut at = bytes.len() - EOCD_MIN;
    loop {
        if u32_at(bytes, at) == EOCD_SIG {
            return Some(at);
        }
        if at == lowest {
            return None;
        }
        at -= 1;
    }
}

/// Reads every central-directory entry, checking the archive-wide rules first.
pub fn read_central_directory(bytes: &[u8], limits: &Limits) -> Result<Vec<Entry>> {
    let Some(eocd) = find_eocd(bytes) else {
        return fail("cartridge", "not a zip archive");
    };
    let count = u16_at(bytes, eocd + 10) as u64;
    let size = u32_at(bytes, eocd + 12) as u64;
    let offset = u32_at(bytes, eocd + 16) as u64;
    if count == 0xffff || size == 0xffff_ffff || offset == 0xffff_ffff {
        return fail("cartridge", "ZIP64 archives are not supported");
    }
    if count > limits.files {
        return fail(
            "cartridge",
            format!("archive has {count} files; the limit is {}", limits.files),
        );
    }
    if offset + size > bytes.len() as u64 {
        return fail(
            "cartridge",
            "central directory runs past the end of the archive",
        );
    }

    let mut entries = Vec::new();
    let mut position = offset as usize;
    for _ in 0..count {
        if position + 46 > bytes.len() || u32_at(bytes, position) != CD_SIG {
            return fail("cartridge", "central directory is corrupt");
        }
        let flags = u16_at(bytes, position + 8) as u16;
        let method = u16_at(bytes, position + 10) as u16;
        let compressed = u32_at(bytes, position + 20);
        let declared = u32_at(bytes, position + 24);
        let name_len = u16_at(bytes, position + 28);
        let extra_len = u16_at(bytes, position + 30);
        let comment_len = u16_at(bytes, position + 32);
        let local = u32_at(bytes, position + 42);
        let name_start = position + 46;
        let next = name_start + name_len + extra_len + comment_len;
        if next > bytes.len() {
            return fail("cartridge", "central directory is corrupt");
        }
        if compressed == 0xffff_ffff || declared == 0xffff_ffff || local == 0xffff_ffff {
            return fail("cartridge", "ZIP64 archives are not supported");
        }
        let name = String::from_utf8_lossy(&bytes[name_start..name_start + name_len]).into_owned();
        entries.push(Entry {
            name,
            method,
            flags,
            compressed_size: compressed as u64,
            declared_size: declared as u64,
            local_offset: local as u64,
        });
        position = next;
    }
    Ok(entries)
}

/// Checks each entry's name, class and size before anything is inflated.
pub fn plan_entries(entries: &[Entry], limits: &Limits) -> Result<Plan> {
    let mut seen = HashSet::new();
    let mut inflated_total: u64 = 0;
    let mut plan = Plan::default();
    for entry in entries {
        let name = entry.name.as_str();
        if !seen.insert(name) {
            return fail(name, "appears more than once in the archive");
        }
        if entry.flags & 0x0001 != 0 {
            return fail(name, "is encrypted; encrypted entries are not supported");
        }
        if entry.method != 0 && entry.method != 8 {
            return fail(
                name,
                format!(
                    "uses compression method {}; only deflate (8) and stored (0) are supported",
                    entry.method
                ),
            );
        }
        check_path(name)?;
        if name.ends_with('/') {
            if entry.declared_size != 0 {
                return fail(name, "is a directory but has content");
            }
            if name != "templates/" && name != "assets/" && !name.starts_with("assets/") {
                return fail(name, format!("is not an allowed location; {ALLOWED}"));
            }
            continue;
        }
        if name == "manifest.json" {
            limit_file(entry, limits.manifest_bytes)?;
            plan.files.push(entry.clone());
        } else if name == "server.wasm" || name == "canvas.wasm" {
            limit_file(entry, limits.wasm_bytes)?;
            plan.files.push(entry.clone());
        } else if let Some(captures) = TEMPLATE_PATH.captures(name) {
            let template = captures[1].to_string();
            if template.len() > 64 {
                return fail(name, "template name is longer than 64 characters");
            }
            plan.files.push(entry.clone());
            plan.templates.push((template, entry.clone()));
        } else if name.starts_with("assets/") {
            limit_file(entry, limits.asset_bytes)?;
            plan.files.push(entry.clone());
            plan.assets.push(entry.clone());
        } else {
            return fail(name, format!("is not an allowed file; {ALLOWED}"));
        }
        inflated_total += entry.declared_size;
        if inflated_total > limits.inflated_bytes {
            return fail(
                "cartridge",
                format!(
                    "files inflate to {inflated_total} bytes; the limit is {}",
                    limits.inflated_bytes
                ),
            );
        }
    }
    if plan.templates.len() as u64 > limits.template_files {
        return fail(
            "cartridge",
            format!(
                "has {} templates; the limit is {}",
                plan.templates.len(),
                limits.template_files
            ),
        );
    }
    let template_total: u64 = plan
        .templates
        .iter()
        .map(|(_, entry)| entry.declared_size)
        .sum();
    if template_total > limits.template_bytes {
        return fail(
            "cartridge",
            format!(
                "templates are {template_total} bytes; the limit is {}",
                limits.template_bytes
            ),
        );
    }
    Ok(plan)
}

fn limit_file(entry: &Entry, max: u64) -> Result<()> {
    if entry.declared_size > max {
        return fail(
            entry.name.as_str(),
            format!("is {} bytes; the limit is {max}", entry.declared_size),
        );
    }
    Ok(())
}

/// Rejects absolute paths, backslashes, and empty, `.` or `..` segments.
fn check_path(name: &str) -> Result<()> {
    if name.starts_with('/') || name.contains('\\') {
        return fail(name, "must be a relative path with forward slashes");
    }
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    if trimmed
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return fail(name, "contains an empty, '.' or '..' path segment");
    }
    Ok(())
}

/// Reads one allowed entry's bytes. A deflated entry stops as soon as it would pass its declared size.
pub fn read_entry(bytes: &[u8], entry: &Entry) -> Result<Vec<u8>> {
    let name = entry.name.as_str();
    let local = entry.local_offset as usize;
    if local + 30 > bytes.len() || u32_at(bytes, local) != LOCAL_SIG {
        return fail(name, "local file header is corrupt");
    }
    let data_start = local + 30 + u16_at(bytes, local + 26) + u16_at(bytes, local + 28);
    let data_end = data_start + entry.compressed_size as usize;
    if data_end > bytes.len() {
        return fail(name, "data runs past the end of the archive");
    }
    let compressed = &bytes[data_start..data_end];
    let declared = entry.declared_size;

    if entry.method == 0 {
        if compressed.len() as u64 != declared {
            return fail(name, "stored size does not match its declared size");
        }
        return Ok(compressed.to_vec());
    }

    let mut decoder = flate2::read::DeflateDecoder::new(compressed);
    let mut out: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = match decoder.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => return fail(name, format!("could not be inflated: {error}")),
        };
        if out.len() as u64 + read as u64 > declared {
            return fail(
                name,
                format!("inflates past its declared size of {declared} bytes"),
            );
        }
        out.extend_from_slice(&chunk[..read]);
    }
    if out.len() as u64 != declared {
        return fail(
            name,
            format!("inflated to {} bytes but declares {declared}", out.len()),
        );
    }
    Ok(out)
}
