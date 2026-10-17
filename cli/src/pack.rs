//! Packs a Cartridge directory into a `.cartridge` zip with `manifest.json` at the root.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

/// Archive entries as `(name, bytes)`, sorted by name.
pub type Files = Vec<(String, Vec<u8>)>;

/// Collects a directory's files, recursively. Dot-entries (`.DS_Store`, `.gitkeep`, `.git`) are not part of a Cartridge.
pub fn collect(dir: &Path) -> anyhow::Result<Files> {
    let mut found = BTreeMap::new();
    walk(dir, "", &mut found).with_context(|| format!("reading {}", dir.display()))?;
    Ok(found.into_iter().collect())
}

fn walk(dir: &Path, prefix: &str, found: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with('.') {
            continue;
        }
        let relative = if prefix.is_empty() {
            file_name
        } else {
            format!("{prefix}/{file_name}")
        };
        let path = entry.path();
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            walk(&path, &relative, found)?;
        } else if metadata.is_file() {
            found.insert(relative, fs::read(&path)?);
        }
    }
    Ok(())
}

/// Writes the files as a deflate zip, the way the server's packer does, at level 9.
pub fn zip(files: &Files) -> anyhow::Result<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(9));
    for (name, bytes) in files {
        writer.start_file(name.as_str(), options)?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

/// The archive bytes for a path: a `.cartridge` file as it is, or a directory packed in memory.
pub fn archive_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    if path.is_file() {
        return fs::read(path).with_context(|| format!("reading {}", path.display()));
    }
    zip(&collect(path)?)
}

/// Where `pack` writes by default. A directory called `cartridge` belongs to its project, so the archive goes inside the
/// project: `examples/poll/cartridge` gives `examples/poll/poll.cartridge`. Any other directory gets a sibling archive
/// named after it.
pub fn default_out(dir: &Path) -> PathBuf {
    if dir.file_name().is_some_and(|name| name == "cartridge") {
        let project = dir.parent().unwrap_or(Path::new("."));
        let name = project
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cartridge".to_string());
        return project.join(format!("{name}.cartridge"));
    }
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cartridge".to_string());
    dir.parent()
        .unwrap_or(Path::new("."))
        .join(format!("{name}.cartridge"))
}
