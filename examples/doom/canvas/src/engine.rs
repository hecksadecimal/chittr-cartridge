//! The original C engine owns global state; one instance per Canvas Guest module.
//! GPL-2.0; see ../../engine/LICENSE.TXT.
use std::ffi::{c_char, c_int, CStr};
use std::sync::Arc;

unsafe extern "C" {
    fn doom_boot(
        wads: *const *const u8,
        lengths: *const c_int,
        count: c_int,
        skill: c_int,
    ) -> c_int;
    fn doom_tick();
    fn doom_pixels() -> *const u8;
    fn doom_palette() -> *const u8;
    fn doom_key(key: c_int, down: c_int);
    fn doom_mouse(dx: c_int, dy: c_int, buttons: c_int);
    fn doom_pop_sound_event(event: *mut SoundEvent) -> c_int;
    fn doom_error() -> *const c_char;
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
#[allow(dead_code)]
pub struct SoundEvent {
    pub name: [u8; 8],
    pub volume: u8,
}

impl SoundEvent {
    #[allow(dead_code)]
    pub fn name(&self) -> Option<&str> {
        let end = self.name.iter().position(|byte| *byte == 0).unwrap_or(8);
        std::str::from_utf8(&self.name[..end]).ok()
    }
}

pub struct Engine {
    // C retains pointers into these allocations throughout the instance's lifetime.
    _wads: Vec<Arc<Vec<u8>>>,
}

impl Engine {
    /// Boots the original engine over a base WAD and optional mod WADs the viewer
    /// selected at runtime. The bytes are validated by the caller and retained here
    /// for the engine's lifetime; later WADs override earlier lumps. `skill` is the
    /// original `skill_t` index (0–4), chosen by the owner's default.
    pub fn boot(wads: Vec<Arc<Vec<u8>>>, skill: u8) -> Result<Self, String> {
        if wads.is_empty() {
            return Err("Choose a WAD file before starting".into());
        }
        #[cfg(target_arch = "wasm32")]
        crate::allocator::init();
        let pointers: Vec<*const u8> = wads.iter().map(|wad| wad.as_ptr()).collect();
        let lengths: Vec<c_int> = wads.iter().map(|wad| wad.len() as c_int).collect();
        // SAFETY: validated WADs, retained in self; pointers stay valid while the
        // Arc allocations live. The engine boots once per module.
        if unsafe {
            doom_boot(
                pointers.as_ptr(),
                lengths.as_ptr(),
                wads.len() as c_int,
                skill.into(),
            )
        } != 0
        {
            return Err(last_error().unwrap_or_else(|| "DOOM initialization failed".into()));
        }
        Ok(Self { _wads: wads })
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

    pub fn mouse(&mut self, dx: f64, dy: f64, buttons: u32) {
        let clamp = |value: f64| {
            if value.is_finite() {
                value.clamp(-2000.0, 2000.0) as c_int
            } else {
                0
            }
        };
        // SAFETY: initialized engine, bounded relative movement and button bits.
        unsafe { doom_mouse(clamp(dx), clamp(dy), (buttons & 7) as c_int) };
    }

    pub fn release_input(&mut self) {
        self.release_keys();
        self.mouse(0.0, 0.0, 0);
    }

    /// Drains one high-level sound event queued by the C engine.
    #[allow(dead_code)]
    pub fn pop_sound(&self) -> Option<SoundEvent> {
        let mut event = SoundEvent::default();
        // SAFETY: the engine wrote only its fixed queue; the struct is repr(C).
        (unsafe { doom_pop_sound_event(&mut event) } != 0).then_some(event)
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
        crate::wad::validate_base(&wad).expect("validate the locally supplied WAD");
        let wad = Arc::new(wad);
        let mut engine = Engine::boot(vec![wad], 2).expect("boot original engine");
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
