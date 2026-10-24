//! The vendored Schism Tracker C engine owns global state; one instance per
//! Canvas Guest module. GPL-2.0; see ../../engine/LICENSE.
use std::ffi::{c_char, c_int, CString};

pub const SCREEN_WIDTH: u32 = 640;
pub const SCREEN_HEIGHT: u32 = 400;

unsafe extern "C" {
    fn schism_wasm_set_time(milliseconds: u64);
    fn schism_wasm_boot() -> c_int;
    fn schism_wasm_frame();
    fn schism_wasm_framebuffer() -> *const u8;
    fn schism_wasm_palette() -> *const u32;
    fn schism_wasm_frame_sequence() -> u32;
    fn schism_wasm_palette_sequence() -> u32;
    fn schism_wasm_show_pattern_editor();

    fn schism_wasm_set_keymod(modifiers: u32);
    fn schism_wasm_key(down: c_int, sym: u32, scancode: u32, modifiers: u32, text: *const c_char);
    fn schism_wasm_mouse_move(x: c_int, y: c_int);
    fn schism_wasm_mouse_button(button: c_int, down: c_int, x: c_int, y: c_int);
    fn schism_wasm_focus(focused: c_int);

    fn schism_wasm_audio_rate() -> u32;
    fn schism_wasm_audio_channels() -> u32;
    fn schism_wasm_audio_available() -> usize;
    fn schism_wasm_audio_read(destination: *mut u8, length: usize) -> usize;
    fn schism_wasm_audio_clear();

    fn schism_wasm_load_module(name: *const c_char, data: *const u8, length: usize) -> c_int;
    fn schism_wasm_song_playing() -> c_int;
    fn schism_wasm_stop_song();
}

pub struct Engine {
    frame_sequence: u32,
    palette_sequence: u32,
}

impl Engine {
    /// Boots the original tracker. The engine starts on the pattern editor with
    /// an empty song; modules arrive later through `load_module`.
    pub fn boot() -> Result<Self, String> {
        #[cfg(target_arch = "wasm32")]
        crate::allocator::init();
        // SAFETY: called once per guest instance, before any other engine call.
        if unsafe { schism_wasm_boot() } != 0 {
            return Err("Schism Tracker initialization failed".into());
        }
        // SAFETY: the engine is initialized and the event loop has started.
        unsafe { schism_wasm_show_pattern_editor() };
        Ok(Self {
            frame_sequence: 0,
            palette_sequence: 0,
        })
    }

    /// Runs one engine iteration at the host's absolute page time. Schism's
    /// timer backend reads this clock, so audio and key repeat stay in step
    /// with the browser rather than a WASI clock.
    pub fn tick(&mut self, page_time_ms: f64) {
        let millis = if page_time_ms.is_finite() && page_time_ms > 0.0 {
            page_time_ms.min(u64::MAX as f64) as u64
        } else {
            0
        };
        // SAFETY: single-threaded guest, called only after boot.
        unsafe { schism_wasm_set_time(millis) };
        unsafe { schism_wasm_frame() };
    }

    /// The framebuffer and palette sequences let the guest skip a presentation
    /// when Schism repainted nothing. The engine increments them in its blit
    /// and palette callbacks.
    pub fn sequences(&self) -> (u32, u32) {
        // SAFETY: the engine owns fixed arrays for the module's lifetime.
        unsafe { (schism_wasm_frame_sequence(), schism_wasm_palette_sequence()) }
    }

    pub fn image(&self) -> (&[u8], &[u32]) {
        // SAFETY: the initialized engine owns a fixed 640x400 buffer and a
        // 256-entry palette; no tick occurs while these borrows are used.
        unsafe {
            (
                std::slice::from_raw_parts(
                    schism_wasm_framebuffer(),
                    (SCREEN_WIDTH * SCREEN_HEIGHT) as usize,
                ),
                std::slice::from_raw_parts(schism_wasm_palette(), 256),
            )
        }
    }

    pub fn key(&mut self, sym: u32, scancode: u32, modifiers: u32, text: Option<&str>, down: bool) {
        let text = text.and_then(|text| CString::new(text).ok());
        // SAFETY: bounded keysym/scancode values and a NUL-terminated UTF-8
        // string that lives for the duration of the call.
        unsafe {
            schism_wasm_key(
                c_int::from(down),
                sym,
                scancode,
                modifiers,
                text.as_ref().map_or(std::ptr::null(), |text| text.as_ptr()),
            )
        };
    }

    /// Keeps Schism's modifier state current even for mouse events, which read
    /// it from the events backend rather than the event payload.
    pub fn set_keymod(&mut self, modifiers: u32) {
        // SAFETY: plain bitmask assignment in the backend.
        unsafe { schism_wasm_set_keymod(modifiers) };
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        // SAFETY: coordinates are clamped to the engine's native screen.
        unsafe { schism_wasm_mouse_move(x, y) };
    }

    pub fn mouse_button(&mut self, button: i32, down: bool, x: i32, y: i32) {
        // SAFETY: button is one of the three the backend understands.
        unsafe { schism_wasm_mouse_button(button, c_int::from(down), x, y) };
    }

    pub fn focus(&mut self, focused: bool) {
        // SAFETY: plain event push in the backend.
        unsafe { schism_wasm_focus(c_int::from(focused)) };
    }

    pub fn audio_rate(&self) -> u32 {
        // SAFETY: returns the open device's rate, or the 48 kHz default.
        unsafe { schism_wasm_audio_rate() }
    }

    pub fn audio_channels(&self) -> u32 {
        // SAFETY: returns the open device's channel count, or stereo.
        unsafe { schism_wasm_audio_channels() }
    }

    pub fn audio_available(&self) -> usize {
        // SAFETY: reads the backend ring buffer length.
        unsafe { schism_wasm_audio_available() }
    }

    pub fn audio_read(&mut self, destination: &mut [u8]) -> usize {
        if destination.is_empty() {
            return 0;
        }
        // SAFETY: destination has at least `len` writable bytes.
        unsafe { schism_wasm_audio_read(destination.as_mut_ptr(), destination.len()) }
    }

    pub fn audio_clear(&mut self) {
        // SAFETY: resets the internal ring buffer.
        unsafe { schism_wasm_audio_clear() };
    }

    pub fn load_module(&mut self, name: &str, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return false;
        }
        let name = CString::new(name.replace('\0', "")).unwrap_or_default();
        // SAFETY: the loader copies the module into the song before returning.
        unsafe { schism_wasm_load_module(name.as_ptr(), bytes.as_ptr(), bytes.len()) != 0 }
    }

    pub fn song_playing(&self) -> bool {
        // SAFETY: reads the player mode.
        unsafe { schism_wasm_song_playing() != 0 }
    }

    pub fn stop_song(&mut self) {
        // SAFETY: stops playback and leaves the song loaded.
        unsafe { schism_wasm_stop_song() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_boots_draws_and_advances_its_clock() {
        let _guard = crate::test_lock();
        let mut engine = Engine::boot().expect("boot vendored engine");
        engine.tick(0.0);
        let (first, _) = engine.sequences();
        for step in 1..=8 {
            engine.tick(step as f64 * 16.0);
        }
        let (last, _) = engine.sequences();
        assert!(last > first, "the engine should present frames");
        let (pixels, palette) = engine.image();
        assert_eq!(pixels.len(), (SCREEN_WIDTH * SCREEN_HEIGHT) as usize);
        assert_eq!(palette.len(), 256);
    }
}
