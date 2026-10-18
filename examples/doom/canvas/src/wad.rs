//! Runtime validation for WADs the viewer selected through the card's file pickers.
//! The bytes stay in the Canvas Guest and are never embedded in the cartridge, so no
//! game data is checked into or shipped with source control.
//!
//! The checks mirror the engine's in-memory loader (`W_SetMemory`, `W_AddMemory` and
//! `W_InitMultipleFiles`) so a malformed or incomplete selection is reported before
//! `doom_boot`, rather than trapping the guest.

/// The host picker cap, and the largest WAD this guest will assemble.
const MAX_BYTES: usize = 32 * 1024 * 1024;

fn word(bytes: &[u8], offset: usize) -> Result<usize, String> {
    let data = bytes.get(offset..offset + 4).ok_or("Truncated WAD")?;
    Ok(u32::from_le_bytes(data.try_into().unwrap()) as usize)
}

fn short_name(name: &[u8; 8]) -> &[u8] {
    &name[..name.iter().position(|b| *b == 0).unwrap_or(8)]
}

/// Checks the common header and every directory entry. Returns the lump count and
/// directory offset after proving every lump stays inside the file.
fn container(bytes: &[u8]) -> Result<(usize, usize), String> {
    if bytes.len() > MAX_BYTES || bytes.len() < 12 {
        return Err("WAD must be between 12 bytes and 32 MiB".into());
    }
    if &bytes[..4] != b"IWAD" && &bytes[..4] != b"PWAD" {
        return Err("Expected an IWAD or PWAD".into());
    }
    let count = word(bytes, 4)?;
    let directory = word(bytes, 8)?;
    if count == 0 || count > 100_000 {
        return Err("Invalid WAD lump count".into());
    }
    let end = count
        .checked_mul(16)
        .and_then(|size| directory.checked_add(size))
        .ok_or("WAD directory overflow")?;
    if directory < 12 || end > bytes.len() {
        return Err("WAD directory is out of bounds".into());
    }
    for index in 0..count {
        let offset = directory + index * 16;
        let position = word(bytes, offset)?;
        let size = word(bytes, offset + 4)?;
        let payload_end = position.checked_add(size).ok_or("WAD lump overflow")?;
        if position > bytes.len() || payload_end > bytes.len() {
            return Err(format!("Lump {index} is out of bounds"));
        }
    }
    Ok((count, directory))
}

/// Checks that `bytes` is a bounded base WAD the engine can boot. A base is an IWAD
/// or a complete PWAD with the palette, title image, and a Doom I or Doom II map.
pub fn validate_base(bytes: &[u8]) -> Result<(), String> {
    let (count, directory) = container(bytes)?;
    let mut has_first_map = false;
    let mut has_doom_two_map = false;
    let mut has_palette = false;
    let mut has_title = false;
    for index in 0..count {
        let offset = directory + index * 16;
        let size = word(bytes, offset + 4)?;
        let name: [u8; 8] = bytes[offset + 8..offset + 16].try_into().unwrap();
        let short = short_name(&name);
        has_first_map |= short == b"E1M1";
        has_doom_two_map |= short == b"MAP01";
        has_palette |= short == b"PLAYPAL" && size >= 14 * 768;
        has_title |= short == b"TITLEPIC";
    }
    if !has_first_map && !has_doom_two_map {
        return Err("Base WAD needs E1M1 or MAP01".into());
    }
    if !has_palette {
        return Err("Base WAD needs a full 14-palette PLAYPAL".into());
    }
    if !has_title {
        return Err("Base WAD needs TITLEPIC".into());
    }
    Ok(())
}

/// Checks that `bytes` is a bounded PWAD/IWAD that can be appended as a mod. Mods do
/// not need a map or palette: they may replace only a few lumps in the base.
pub fn validate_mod(bytes: &[u8]) -> Result<(), String> {
    container(bytes).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(lumps: &[(&[u8], usize)]) -> Vec<u8> {
        let mut bytes = b"PWAD\0\0\0\0\0\0\0\0".to_vec();
        let mut entries = Vec::new();
        for (name, size) in lumps {
            entries.push((bytes.len(), *size, *name));
            bytes.resize(bytes.len() + size, 0);
        }
        let directory = bytes.len();
        bytes[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&(directory as u32).to_le_bytes());
        for (position, size, name) in entries {
            bytes.extend_from_slice(&(position as u32).to_le_bytes());
            bytes.extend_from_slice(&(size as u32).to_le_bytes());
            let mut padded = [0; 8];
            padded[..name.len()].copy_from_slice(name);
            bytes.extend_from_slice(&padded);
        }
        bytes
    }

    fn complete() -> Vec<u8> {
        fixture(&[
            (b"PLAYPAL", 14 * 768),
            (b"TITLEPIC", 320 * 200),
            (b"E1M1", 0),
            (b"THINGS", 10),
        ])
    }

    #[test]
    fn accepts_a_complete_episode_one_wad() {
        assert!(validate_base(&complete()).is_ok());
        // A complete WAD may be larger and carry other episodes.
        let mut larger = fixture(&[
            (b"PLAYPAL", 14 * 768),
            (b"TITLEPIC", 4),
            (b"E1M1", 0),
            (b"E2M1", 0),
        ]);
        assert!(validate_base(&larger).is_ok());
        larger[..4].copy_from_slice(b"IWAD");
        assert!(validate_base(&larger).is_ok());
    }

    #[test]
    fn accepts_a_doom_two_map_as_the_base() {
        let wad = fixture(&[
            (b"PLAYPAL", 14 * 768),
            (b"TITLEPIC", 4),
            (b"MAP01", 0),
        ]);
        assert!(validate_base(&wad).is_ok());
    }

    #[test]
    fn mods_only_need_a_valid_container() {
        assert!(validate_mod(&fixture(&[(b"PLAYPAL", 14 * 768)])).is_ok());
        assert!(validate_mod(&fixture(&[(b"MAP01", 0)])).is_ok());
        assert!(validate_mod(b"PWAD\0\0\0\0\0\0\0\0").is_err());
        assert!(validate_mod(b"XXXX\0\0\0\0\0\0\0\0").is_err());
    }

    #[test]
    fn rejects_truncated_headers_directories_and_lumps() {
        assert!(validate_base(b"IWAD").is_err());
        assert!(validate_base(b"XXXX\0\0\0\0\0\0\0\0").is_err());
        let mut bytes = complete();
        bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate_base(&bytes).is_err());
        let mut bytes = complete();
        let directory = word(&bytes, 8).unwrap();
        bytes[directory..directory + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate_base(&bytes).is_err());
    }

    #[test]
    fn rejects_missing_or_incomplete_game_data() {
        assert!(validate_base(&fixture(&[(b"PLAYPAL", 14 * 768), (b"TITLEPIC", 4)])).is_err());
        assert!(validate_base(&fixture(&[(b"PLAYPAL", 14 * 768), (b"E1M1", 0)])).is_err());
        assert!(validate_base(&fixture(&[(b"TITLEPIC", 4), (b"E1M1", 0)])).is_err());
        // The engine requires all fourteen palettes, not just one.
        assert!(validate_base(&fixture(&[
            (b"PLAYPAL", 768),
            (b"TITLEPIC", 4),
            (b"E1M1", 0)
        ]))
        .is_err());
    }
}
