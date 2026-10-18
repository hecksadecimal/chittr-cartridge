// Canvas Guest for the original DOOM engine. GPL-2.0; see ../../engine/LICENSE.TXT.
#[cfg(all(feature = "engine", target_arch = "wasm32"))]
mod allocator;
#[cfg(feature = "engine")]
mod engine;
#[allow(dead_code, non_camel_case_types, clippy::let_unit_value)]
mod pdk;
#[cfg(any(all(feature = "engine", target_arch = "wasm32"), test))]
mod wad;

use chittr_cartridge::canvas::{rgba, Canvas2D, EventInput, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde_json::{Map, Value};
use std::cell::RefCell;

#[cfg(any(feature = "engine", test))]
const TIC_MS: f64 = 1000.0 / 35.0;

struct Game {
    width: u32,
    height: u32,
    armed: bool,
    paused: bool,
    loading: bool,
    accumulator: f64,
    error: Option<String>,
    #[cfg(feature = "engine")]
    wad: Option<Vec<u8>>,
    #[cfg(feature = "engine")]
    engine: Option<engine::Engine>,
    #[cfg(feature = "engine")]
    keys: Keys,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            width: 320,
            height: 200,
            armed: false,
            paused: false,
            loading: false,
            accumulator: 0.0,
            error: None,
            #[cfg(feature = "engine")]
            wad: None,
            #[cfg(feature = "engine")]
            engine: None,
            #[cfg(feature = "engine")]
            keys: Keys::default(),
        }
    }
}

thread_local! { static GAME: RefCell<Game> = RefCell::new(Game::default()); }

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|state| {
        let mut game = state.borrow_mut();
        game.width = dimension(input.width as f64);
        game.height = dimension(input.height as f64);
        set_fps(if input.prefers_reduced_motion { 15 } else { 35 })?;
        game.draw()
    });
    Ok(reply(result))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|state| {
        // Only the engine-enabled build mutates the simulation during a frame.
        #[allow(unused_mut)]
        let mut game = state.borrow_mut();
        #[cfg(feature = "engine")]
        if game.loading && !game.paused && game.error.is_none() {
            game.loading = false;
            match game.wad.take() {
                Some(wad) => match engine::Engine::boot(wad) {
                    Ok(engine) => game.engine = Some(engine),
                    Err(error) => game.error = Some(error),
                },
                None => game.error = Some("Choose a WAD file before starting".into()),
            }
        } else if game.engine.is_some() && !game.paused && game.error.is_none() {
            let steps = simulation_steps(&mut game.accumulator, input.dt);
            for _ in 0..steps {
                if let Err(error) = game.engine.as_mut().unwrap().tick() {
                    game.error = Some(error);
                    break;
                }
            }
        }
        #[cfg(not(feature = "engine"))]
        let _ = input;
        game.draw()?;
        if game.animating() {
            request_frame()?;
        }
        Ok(())
    });
    Ok(reply(result))
}

pub(crate) fn event(input: EventInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|state| {
        let mut game = state.borrow_mut();
        match input.kind.as_str() {
            "resize" => {
                if let (Some(width), Some(height)) =
                    (number(&input.rest, "width"), number(&input.rest, "height"))
                {
                    game.width = dimension(width);
                    game.height = dimension(height);
                }
            }
            "blur" => game.pause(),
            "visibility" if flag(&input.rest, "visible") == Some(false) => game.pause(),
            "pointer" if text(&input.rest, "action") == Some("down") => {
                game.armed = true;
                if game.paused {
                    game.paused = false;
                    game.accumulator = 0.0;
                }
            }
            "key" => {
                let down = text(&input.rest, "action") == Some("down");
                let code = text(&input.rest, "code");
                if down && code == Some("Space") && game.armed && !game.paused {
                    #[cfg(feature = "engine")]
                    if game.engine.is_none() && game.error.is_none() && game.wad.is_some() {
                        game.loading = true;
                    }
                }
                #[cfg(feature = "engine")]
                if !game.paused && game.engine.is_some() {
                    if let Some((key, down)) = code.and_then(|code| game.keys.event(code, down)) {
                        game.engine.as_mut().unwrap().key(key, down);
                    }
                }
            }
            "file" =>
            {
                #[cfg(all(feature = "engine", target_arch = "wasm32"))]
                if game.engine.is_none() && !game.loading {
                    if let Some((name, file)) = input.file() {
                        if name == "wad" {
                            match read_wad(&file) {
                                Ok(wad) => {
                                    game.wad = Some(wad);
                                    game.error = None;
                                }
                                Err(message) => game.error = Some(message),
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        game.draw()?;
        if game.animating() {
            request_frame()?;
        }
        Ok(())
    });
    Ok(reply(result))
}

/// Reads a WAD the viewer picked into memory in bounded 64 KiB chunks, validates the
/// container, and releases the handle. The bytes never leave the Canvas Guest.
#[cfg(all(feature = "engine", target_arch = "wasm32"))]
fn read_wad(file: &chittr_cartridge::canvas::FileRef) -> Result<Vec<u8>, String> {
    const CHUNK: u32 = 64 * 1024;
    const MAX_BYTES: u64 = 16 * 1024 * 1024;
    if file.size > MAX_BYTES {
        let _ = chittr_cartridge::canvas::host::file_close(&file.id);
        return Err(format!(
            "The WAD is {} bytes; the limit is {MAX_BYTES}",
            file.size
        ));
    }
    let result = (|| -> Result<Vec<u8>, String> {
        let mut bytes = Vec::with_capacity(file.size as usize);
        let mut offset = 0u64;
        loop {
            let chunk = chittr_cartridge::canvas::host::file_read(&file.id, offset, CHUNK)
                .map_err(|error| format!("{}: {}", error.code, error.message))?;
            if chunk.bytes.is_empty() && !chunk.eof {
                return Err("The WAD read made no progress".into());
            }
            offset += chunk.bytes.len() as u64;
            let done = chunk.eof;
            bytes.extend_from_slice(&chunk.bytes);
            if done {
                break;
            }
        }
        wad::validate(&bytes)?;
        Ok(bytes)
    })();
    // A re-selection revokes the handle anyway; closing early releases the host buffer.
    let _ = chittr_cartridge::canvas::host::file_close(&file.id);
    result
}

fn number(rest: &Map<String, Value>, key: &str) -> Option<f64> {
    rest.get(key).and_then(Value::as_f64)
}

fn text<'a>(rest: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    rest.get(key).and_then(Value::as_str)
}

fn flag(rest: &Map<String, Value>, key: &str) -> Option<bool> {
    rest.get(key).and_then(Value::as_bool)
}

impl Game {
    fn animating(&self) -> bool {
        #[cfg(feature = "engine")]
        {
            (self.loading || self.engine.is_some()) && !self.paused && self.error.is_none()
        }
        #[cfg(not(feature = "engine"))]
        {
            false
        }
    }

    fn pause(&mut self) {
        self.paused = true;
        self.accumulator = 0.0;
        #[cfg(feature = "engine")]
        {
            self.keys.held.clear();
            if let Some(engine) = &mut self.engine {
                engine.release_keys();
            }
        }
    }

    fn draw(&self) -> Result<(), String> {
        let mut canvas = Canvas2D::new();
        canvas.fill_color(rgba(8, 8, 8, 255)).fill_rect(
            0.0,
            0.0,
            self.width as f32,
            self.height as f32,
        );
        if let Some(error) = &self.error {
            self.message(&mut canvas, "DOOM stopped", error);
            return submit(&mut canvas);
        }
        #[cfg(feature = "engine")]
        if let Some(engine) = &self.engine {
            let (indices, palette) = engine.image();
            // One 0x53 op presents the engine's native 320x200 indexed frame. The
            // host scales it (nearest-neighbour) into the fitted, letterboxed
            // destination and expands the current palette, so palette flashes show.
            canvas.blit_indexed_scaled(
                destination(self.width, self.height),
                320,
                200,
                false,
                &palette_rgba(palette),
                indices,
            );
            submit(&mut canvas)?;
            if self.paused {
                self.message(&mut canvas, "Paused", "Click the canvas to resume");
                submit(&mut canvas)?;
            }
            return Ok(());
        }
        let (title, message) = if let Some(error) = &self.error {
            ("DOOM could not start", error.as_str())
        } else if self.loading {
            ("DOOM", "Loading DOOM…")
        } else {
            self.idle_message()
        };
        self.message(&mut canvas, title, message);
        submit(&mut canvas)
    }

    fn idle_message(&self) -> (&'static str, &'static str) {
        #[cfg(feature = "engine")]
        {
            if self.wad.is_none() {
                return (
                    "DOOM",
                    "Choose your WAD above, then click the canvas and press Space",
                );
            }
            if self.armed {
                return ("DOOM", "Press Space to start · first episode · silent");
            }
            ("DOOM", "Click the canvas, then press Space to start")
        }
        #[cfg(not(feature = "engine"))]
        {
            (
                "DOOM — scaffold build",
                "Rebuild with the engine feature to play",
            )
        }
    }

    fn message(&self, canvas: &mut Canvas2D, title: &str, message: &str) {
        let scale = (self.width as f32 / 320.0).min(self.height as f32 / 200.0);
        let cx = self.width as f32 / 2.0;
        let cy = self.height as f32 / 2.0;
        canvas
            .fill_color(rgba(8, 8, 8, 230))
            .fill_rect(0.0, cy - 45.0 * scale, self.width as f32, 90.0 * scale)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_color(rgba(210, 70, 45, 255))
            .font("sans-serif", 28.0 * scale, 900.0)
            .fill_text(title, cx, cy - 18.0 * scale)
            .fill_color(rgba(225, 225, 225, 255))
            .font("sans-serif", 10.0 * scale, 400.0)
            .fill_text(message, cx, cy + 20.0 * scale);
    }
}

fn dimension(value: f64) -> u32 {
    if value.is_finite() {
        value.clamp(1.0, 2048.0) as u32
    } else {
        320
    }
}

#[cfg(any(feature = "engine", test))]
fn simulation_steps(accumulator: &mut f64, dt: f64) -> usize {
    let dt = if dt.is_finite() {
        dt.clamp(0.0, TIC_MS * 3.0)
    } else {
        0.0
    };
    *accumulator += dt;
    let steps = ((*accumulator / TIC_MS).floor() as usize).min(3);
    *accumulator -= steps as f64 * TIC_MS;
    steps
}

#[cfg(any(feature = "engine", test))]
fn viewport(width: u32, height: u32) -> (u32, u32) {
    let scale = (width as f64 / 320.0).min(height as f64 / 200.0);
    (
        (320.0 * scale).floor().max(1.0) as u32,
        (200.0 * scale).floor().max(1.0) as u32,
    )
}

/// The fitted, letterboxed destination rectangle in backing-store pixels.
#[cfg(any(feature = "engine", test))]
fn destination(width: u32, height: u32) -> [f32; 4] {
    let (w, h) = viewport(width, height);
    [
        (width - w) as f32 / 2.0,
        (height - h) as f32 / 2.0,
        w as f32,
        h as f32,
    ]
}

/// Expands the engine's current 256-entry RGB palette to `0xRRGGBBAA` for `blitIndexedScaled`.
#[cfg(any(feature = "engine", test))]
fn palette_rgba(palette: &[u8]) -> Vec<u32> {
    palette
        .chunks_exact(3)
        .map(|rgb| u32::from_be_bytes([rgb[0], rgb[1], rgb[2], 0xff]))
        .collect()
}

#[cfg(any(feature = "engine", test))]
#[derive(Default)]
struct Keys {
    held: std::collections::BTreeSet<String>,
}

#[cfg(any(feature = "engine", test))]
impl Keys {
    fn event(&mut self, code: &str, down: bool) -> Option<(u8, bool)> {
        let key = doom_key(code)?;
        if down {
            self.held.insert(code.to_string());
            // Repeats still reach the original menu responder.
            Some((key, true))
        } else if self.held.remove(code)
            && !self.held.iter().any(|code| doom_key(code) == Some(key))
        {
            Some((key, false))
        } else {
            None
        }
    }
}

#[cfg(any(feature = "engine", test))]
fn doom_key(code: &str) -> Option<u8> {
    Some(match code {
        "ArrowUp" | "KeyW" => 0xad,
        "ArrowDown" | "KeyS" => 0xaf,
        "ArrowLeft" | "KeyA" => 0xac,
        "ArrowRight" | "KeyD" => 0xae,
        "Space" | "KeyJ" => 0x9d,
        "ControlLeft" | "ControlRight" | "KeyF" => 32,
        "ShiftLeft" | "ShiftRight" => 0xb6,
        "AltLeft" | "AltRight" => 0xb8,
        "Escape" | "KeyM" => 27,
        "KeyQ" => b',',
        "KeyE" => b'.',
        "Enter" => 13,
        "Tab" => 9,
        "Backspace" => 127,
        "Pause" | "KeyP" => 0xff,
        _ if code.starts_with("Digit") && code.len() == 6 => code.as_bytes()[5],
        _ if code.starts_with("Key") && code.len() == 4 => code.as_bytes()[3].to_ascii_lowercase(),
        _ => return None,
    })
}

fn reply(result: Result<(), String>) -> CanvasReply {
    CanvasReply {
        error: result.err().map(|message| HookError {
            code: "doom".into(),
            message,
        }),
    }
}

#[cfg(target_arch = "wasm32")]
fn submit(canvas: &mut Canvas2D) -> Result<(), String> {
    canvas.submit().map_err(|error| error.message)
}
#[cfg(not(target_arch = "wasm32"))]
fn submit(canvas: &mut Canvas2D) -> Result<(), String> {
    canvas.clear();
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn request_frame() -> Result<(), String> {
    chittr_cartridge::canvas::request_frame().map_err(|error| error.message)
}
#[cfg(not(target_arch = "wasm32"))]
fn request_frame() -> Result<(), String> {
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn set_fps(fps: u32) -> Result<(), String> {
    chittr_cartridge::canvas::set_fps(fps).map_err(|error| error.message)
}
#[cfg(not(target_arch = "wasm32"))]
fn set_fps(_fps: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simulation_catches_up_at_35_hz_but_never_spins() {
        let mut accumulator = 0.0;
        assert_eq!(simulation_steps(&mut accumulator, TIC_MS / 2.0), 0);
        assert_eq!(simulation_steps(&mut accumulator, TIC_MS / 2.0), 1);
        assert_eq!(simulation_steps(&mut accumulator, 60_000.0), 3);
        assert_eq!(simulation_steps(&mut accumulator, f64::NAN), 0);
        assert_eq!(simulation_steps(&mut accumulator, -100.0), 0);
    }

    #[test]
    fn palette_expansion_packs_opaque_rgba_in_index_order() {
        let mut palette = vec![0; 768];
        palette[..3].copy_from_slice(&[23, 42, 67]);
        palette[3..6].copy_from_slice(&[255, 0, 128]);
        let rgba = palette_rgba(&palette);
        assert_eq!(rgba.len(), 256);
        assert_eq!(rgba[0], 0x172a_43ff);
        assert_eq!(rgba[1], 0xff00_80ff);
        assert_eq!(rgba[2], 0x0000_00ff);
    }

    #[test]
    fn fullscreen_fits_without_integer_rounding_or_a_scale_cap() {
        assert_eq!(viewport(1920, 1080), (1728, 1080));
        assert_eq!(viewport(2048, 2048), (2048, 1280));
        assert_eq!(viewport(1000, 700), (1000, 625));
        assert_eq!(viewport(500, 300), (480, 300));
    }

    #[test]
    fn indexed_presentation_stays_inside_the_backing_store_and_submit_limit() {
        for (width, height) in [(320, 200), (640, 400), (2048, 2048), (100, 40), (1, 1)] {
            let [dx, dy, dw, dh] = destination(width, height);
            assert!(dw >= 1.0 && dh >= 1.0);
            assert!(dx >= 0.0 && dy >= 0.0);
            assert!(dx + dw <= width as f32 + 0.5);
            assert!(dy + dh <= height as f32 + 0.5);
        }
        assert_eq!(destination(1920, 1080), [96.0, 0.0, 1728.0, 1080.0]);
        assert_eq!(destination(1000, 700), [0.0, 37.5, 1000.0, 625.0]);
        // The source is the engine's fixed 320x200 indexed framebuffer: one 0x53
        // op (indices plus a 256-entry palette) is far below the 4 MiB buffer.
        assert!(320 * 200 + 256 * 4 + 64 < 4 * 1024 * 1024);
    }

    #[test]
    fn physical_aliases_do_not_release_each_other() {
        let mut keys = Keys::default();
        assert_eq!(keys.event("KeyW", true), Some((0xad, true)));
        assert_eq!(keys.event("ArrowUp", true), Some((0xad, true)));
        assert_eq!(keys.event("ArrowUp", false), None);
        assert_eq!(keys.event("KeyW", false), Some((0xad, false)));
        assert_eq!(keys.event("Space", true), Some((0x9d, true)));
        assert_eq!(keys.event("KeyJ", true), Some((0x9d, true)));
        assert_eq!(keys.event("Space", false), None);
        assert_eq!(keys.event("KeyJ", false), Some((0x9d, false)));
        assert_eq!(keys.event("KeyJ", false), None);
        assert_eq!(keys.event("ControlLeft", true), Some((32, true)));
        assert_eq!(keys.event("KeyF", true), Some((32, true)));
        assert_eq!(keys.event("ControlLeft", false), None);
        assert_eq!(keys.event("KeyF", false), Some((32, false)));
    }

    #[test]
    fn input_uses_dom_codes_and_original_doom_keys() {
        assert_eq!(doom_key("ArrowUp"), Some(0xad));
        assert_eq!(doom_key("Space"), Some(0x9d));
        assert_eq!(doom_key("ControlLeft"), Some(32));
        assert_eq!(doom_key("KeyF"), Some(32));
        assert_eq!(doom_key("Digit2"), Some(b'2'));
        assert_eq!(doom_key("F12"), None);
    }

    #[test]
    fn focus_alone_does_not_arm_or_start_the_game() {
        GAME.with(|g| *g.borrow_mut() = Game::default());
        event(EventInput {
            kind: "focus".into(),
            rest: Map::new(),
        })
        .unwrap();
        GAME.with(|g| assert!(!g.borrow().armed));
        event(EventInput {
            kind: "pointer".into(),
            rest: serde_json::json!({ "action": "down" })
                .as_object()
                .unwrap()
                .clone(),
        })
        .unwrap();
        GAME.with(|g| assert!(g.borrow().armed));
    }
}
