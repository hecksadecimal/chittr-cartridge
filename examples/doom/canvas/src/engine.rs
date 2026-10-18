//! The original C engine owns global state; one instance per Canvas Guest module.
//! GPL-2.0; see ../../engine/LICENSE.TXT.
use std::ffi::{c_char, c_int, CStr};

unsafe extern "C" {
    fn doom_boot(wad: *const u8, len: c_int) -> c_int;
    fn doom_tick();
    fn doom_pixels() -> *const u8;
    fn doom_palette() -> *const u8;
    fn doom_key(key: c_int, down: c_int);
    fn doom_error() -> *const c_char;
}

pub struct Engine {
    // C retains a pointer into this allocation throughout the instance's lifetime.
    _wad: Vec<u8>,
}

impl Engine {
    /// Boots the original engine over a WAD the viewer selected at runtime. The
    /// bytes are validated by the caller and retained here for the engine's lifetime.
    pub fn boot(wad: Vec<u8>) -> Result<Self, String> {
        #[cfg(target_arch = "wasm32")]
        crate::allocator::init();
        // SAFETY: validated WAD, retained in self; engine boots once per module.
        if unsafe { doom_boot(wad.as_ptr(), wad.len() as c_int) } != 0 {
            return Err(last_error().unwrap_or_else(|| "DOOM initialization failed".into()));
        }
        Ok(Self { _wad: wad })
    }

    pub fn tick(&mut self) -> Result<(), String> {
        // SAFETY: called only after boot, on the single guest thread.
        unsafe { doom_tick() };
        last_error().map_or(Ok(()), Err)
    }

    pub fn key(&mut self, key: u8, down: bool) {
        // SAFETY: initialized engine, bounded original DOOM key code.
        unsafe { doom_key(key.into(), i32::from(down)) };
    }

    pub fn release_keys(&mut self) {
        for key in 0..=255 {
            self.key(key, false);
        }
    }

    pub fn image(&self) -> (&[u8], &[u8]) {
        // SAFETY: the initialized engine owns fixed-size framebuffer/palette arrays;
        // no tick occurs while these borrows are used.
        unsafe {
            (
                std::slice::from_raw_parts(doom_pixels(), 320 * 200),
                std::slice::from_raw_parts(doom_palette(), 768),
            )
        }
    }
}

fn last_error() -> Option<String> {
    // SAFETY: the bridge always returns a static NUL-terminated buffer.
    let text = unsafe { CStr::from_ptr(doom_error()) }.to_string_lossy();
    if text.is_empty() {
        None
    } else {
        Some(text.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine feature is tested against a locally supplied WAD. The WAD is
    /// git-ignored, so the test skips (rather than fails) when none is present.
    fn local_wad() -> Option<Vec<u8>> {
        let path = std::env::var_os("DOOM_WAD")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../cartridge/assets/doom1.wad")
            });
        std::fs::read(path).ok()
    }

    #[test]
    fn supplied_wad_boots_and_advances_the_original_engine() {
        let Some(wad) = local_wad() else {
            eprintln!("skipping: no local WAD (set DOOM_WAD or add cartridge/assets/doom1.wad)");
            return;
        };
        crate::wad::validate(&wad).expect("validate the locally supplied WAD");
        let mut engine = Engine::boot(wad).expect("boot original engine");
        let first = engine.image().0.to_vec();
        engine.key(0xad, true);
        for _ in 0..35 {
            engine.tick().unwrap();
        }
        engine.release_keys();
        let (pixels, palette) = engine.image();
        assert_eq!(pixels.len(), 64000);
        assert_eq!(palette.len(), 768);
        assert_ne!(pixels, first);
    }
}
