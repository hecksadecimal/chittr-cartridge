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

const DEFAULT_SKILL: u8 = 2;
const FILE_READ_MAX: u32 = 64 * 1024;
/// Bounded chunks read per frame while a viewer's WAD arrives. Four 64 KiB reads
/// leave ample room under the 250 ms canvas-call deadline on slower phones.
const WAD_CHUNKS_PER_FRAME: usize = 4;
const SKILLS: [&str; 5] = ["baby", "easy", "medium", "hard", "nightmare"];

/// The v2 presentation path the host granted. Indexed8 is smaller on the wire;
/// RGBA8 is the fallback when the host does not offer indexed8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presentation {
    Indexed,
    Rgba,
}

#[allow(dead_code)]
struct WadLoad {
    id: String,
    size: u64,
    bytes: Vec<u8>,
    offset: u64,
}

impl WadLoad {
    fn progress_percent(&self) -> u32 {
        if self.size == 0 {
            100
        } else {
            ((self.bytes.len() as u64 * 100) / self.size).min(99) as u32
        }
    }
}

struct Game {
    width: u32,
    height: u32,
    armed: bool,
    paused: bool,
    loading: bool,
    start_when_ready: bool,
    accumulator: f64,
    error: Option<String>,
    skill: u8,
    presentation: Presentation,
    max_file_bytes: u64,
    max_file_read_bytes: u32,
    wad_load: Option<WadLoad>,
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
            start_when_ready: false,
            accumulator: 0.0,
            error: None,
            skill: DEFAULT_SKILL,
            presentation: Presentation::Indexed,
            max_file_bytes: 16 * 1024 * 1024,
            max_file_read_bytes: 64 * 1024,
            wad_load: None,
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
        // v2 discovery: use the limits the host reports instead of hard-coding them.
        game.max_file_bytes = input.limits.max_file_bytes.max(0) as u64;
        game.max_file_read_bytes = input
            .limits
            .max_file_read_bytes
            .clamp(1, FILE_READ_MAX as i64) as u32;
        game.skill = skill_from_state(input.state.as_ref()).unwrap_or(DEFAULT_SKILL);
        if !has_capability(&input.capabilities, "files.client") {
            game.error =
                Some("This host does not provide the files.client capability DOOM needs".into());
        } else if let Some(presentation) =
            presentation_for(&input.capabilities, input.limits.max_palette_entries)
        {
            game.presentation = presentation;
        } else {
            game.error = Some(
                "This host does not grant a usable framebuffer path (indexed8 with 256 palette entries, or rgba8)"
                    .into(),
            );
        }
        if game.error.is_none() {
            if input.limits.max_framebuffer_pixels < 320 * 200 {
                game.error = Some("This host's framebuffer limit is too small for DOOM".into());
            } else if game.presentation == Presentation::Indexed
                && input.limits.max_palette_entries < 256
            {
                game.error = Some("This host's palette limit is too small for DOOM".into());
            }
        }
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
            if game.wad_load.is_some() {
                match game.pump_wad_load() {
                    Ok(true) => {
                        // The viewer may have pressed Space while the file was still
                        // arriving. Start on the next frame, after the read completes.
                        if game.start_when_ready {
                            game.start_when_ready = false;
                            game.loading = true;
                        }
                    }
                    Ok(false) => {}
                    Err(error) => game.error = Some(error),
                }
            } else {
                game.loading = false;
                match game.wad.take() {
                    Some(wad) => match engine::Engine::boot(wad, game.skill) {
                        Ok(engine) => game.engine = Some(engine),
                        Err(error) => game.error = Some(error),
                    },
                    None => game.error = Some("Choose a WAD file before starting".into()),
                }
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
            "state" => {
                // A render changed the owner-configured default. It applies as soon as the
                // engine has not booted yet; an existing engine keeps its original skill.
                if let Some(skill) = input
                    .rest
                    .get("state")
                    .and_then(Value::as_object)
                    .and_then(|state| skill_from_state(Some(state)))
                {
                    #[cfg(feature = "engine")]
                    if game.engine.is_none() {
                        game.skill = skill;
                    }
                    #[cfg(not(feature = "engine"))]
                    {
                        game.skill = skill;
                    }
                }
            }
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
                    if game.engine.is_none() && game.error.is_none() {
                        if game.wad.is_some() {
                            game.loading = true;
                        } else if game.wad_load.is_some() {
                            // The WAD is still arriving; start as soon as it is validated.
                            game.start_when_ready = true;
                        }
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
                if game.engine.is_none() {
                    if let Some((name, file)) = input.file() {
                        if name == "wad" {
                            if let Some(previous) = game.wad_load.take() {
                                let _ = chittr_cartridge::canvas::host::file_close(&previous.id);
                            }
                            game.wad = None;
                            game.start_when_ready = false;
                            game.loading = false;
                            if file.size == 0 {
                                let _ = chittr_cartridge::canvas::host::file_close(&file.id);
                                game.error = Some("The selected WAD is empty".into());
                            } else if file.size > game.max_file_bytes {
                                let _ = chittr_cartridge::canvas::host::file_close(&file.id);
                                game.error = Some(format!(
                                    "The WAD is {} bytes; this host's limit is {}",
                                    file.size, game.max_file_bytes
                                ));
                            } else {
                                game.wad_load = Some(WadLoad {
                                    id: file.id,
                                    size: file.size,
                                    bytes: Vec::with_capacity(file.size as usize),
                                    offset: 0,
                                });
                                game.loading = true;
                                game.error = None;
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

impl Game {
    /// Advances the viewer's WAD read by a bounded number of host calls. Returns
    /// `true` when the file is complete, validated, and stored for boot.
    #[cfg(all(feature = "engine", target_arch = "wasm32"))]
    fn pump_wad_load(&mut self) -> Result<bool, String> {
        let Some(mut load) = self.wad_load.take() else {
            self.loading = false;
            return Ok(true);
        };
        let chunk_size = self.max_file_read_bytes.clamp(1, FILE_READ_MAX);
        let mut complete = false;
        for _ in 0..WAD_CHUNKS_PER_FRAME {
            let chunk = match chittr_cartridge::canvas::host::file_read(
                &load.id,
                load.offset,
                chunk_size,
            ) {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = chittr_cartridge::canvas::host::file_close(&load.id);
                    self.loading = false;
                    return Err(format!("{}: {}", error.code, error.message));
                }
            };
            if chunk.bytes.is_empty() && !chunk.eof {
                let _ = chittr_cartridge::canvas::host::file_close(&load.id);
                self.loading = false;
                return Err("The WAD read made no progress".into());
            }
            load.offset += chunk.bytes.len() as u64;
            load.bytes.extend_from_slice(&chunk.bytes);
            if load.bytes.len() as u64 > self.max_file_bytes {
                let _ = chittr_cartridge::canvas::host::file_close(&load.id);
                self.loading = false;
                return Err(format!(
                    "The WAD is larger than this host's {}-byte limit",
                    self.max_file_bytes
                ));
            }
            if chunk.eof {
                complete = true;
                break;
            }
        }
        if !complete {
            self.wad_load = Some(load);
            return Ok(false);
        }
        // The handle is call-scoped anyway; closing early releases the host buffer.
        let _ = chittr_cartridge::canvas::host::file_close(&load.id);
        if let Err(error) = wad::validate(&load.bytes) {
            self.loading = false;
            return Err(error);
        }
        self.wad = Some(load.bytes);
        self.wad_load = None;
        self.loading = false;
        Ok(true)
    }

    #[cfg(not(all(feature = "engine", target_arch = "wasm32")))]
    fn pump_wad_load(&mut self) -> Result<bool, String> {
        // Native tests never begin a host file load.
        self.wad_load = None;
        self.loading = false;
        Ok(true)
    }
}

fn number(rest: &Map<String, Value>, key: &str) -> Option<f64> {
    rest.get(key).and_then(Value::as_f64)
}

fn has_capability(capabilities: &[String], id: &str) -> bool {
    capabilities.iter().any(|granted| granted == id)
}

/// Prefer the guest's native indexed path; fall back to RGBA when the host grants only
/// that, or when the host's palette limit cannot hold DOOM's 256 entries.
fn presentation_for(capabilities: &[String], palette_entries: i32) -> Option<Presentation> {
    if has_capability(capabilities, "framebuffer.indexed8") && palette_entries >= 256 {
        Some(Presentation::Indexed)
    } else if has_capability(capabilities, "framebuffer.rgba8") {
        Some(Presentation::Rgba)
    } else {
        None
    }
}

/// The difficulty from the render's `canvas` state, as an engine `skill_t` index.
fn skill_from_state(state: Option<&Map<String, Value>>) -> Option<u8> {
    state?.get("skill").and_then(skill_from_value)
}

fn skill_from_value(value: &Value) -> Option<u8> {
    match value {
        Value::Number(value) => value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| (*value as usize) < SKILLS.len()),
        Value::String(value) => {
            let value = value.trim().to_ascii_lowercase();
            if let Ok(number) = value.parse::<u8>() {
                return ((number as usize) < SKILLS.len()).then_some(number);
            }
            SKILLS
                .iter()
                .position(|name| *name == value)
                .map(|index| index as u8)
        }
        _ => None,
    }
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
            // One v2 scaled blit presents the engine's native 320x200 frame. Indexed8
            // (0x53) is the preferred path; RGBA8 (0x52) covers hosts that grant only it.
            match self.presentation {
                Presentation::Indexed => {
                    canvas.blit_indexed_scaled(
                        destination(self.width, self.height),
                        320,
                        200,
                        false,
                        &palette_rgba(palette),
                        indices,
                    );
                }
                Presentation::Rgba => {
                    let pixels = rgba_frame(indices, palette);
                    canvas.blit_rgba_scaled(
                        destination(self.width, self.height),
                        320,
                        200,
                        false,
                        &pixels,
                    );
                }
            }
            submit(&mut canvas)?;
            if self.paused {
                self.message(&mut canvas, "Paused", "Click the canvas to resume");
                submit(&mut canvas)?;
            }
            return Ok(());
        }
        let (title, message): (&str, String) = if self.loading {
            ("DOOM", self.loading_message())
        } else {
            let (title, message) = self.idle_message();
            (title, message.to_string())
        };
        self.message(&mut canvas, title, &message);
        submit(&mut canvas)
    }

    fn loading_message(&self) -> String {
        match &self.wad_load {
            Some(load) => format!("Loading DOOM… {}%", load.progress_percent()),
            None => "Loading DOOM…".to_string(),
        }
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

/// Expands one indexed frame to RGBA8 for the `blitRgbaScaled` fallback.
#[cfg(any(feature = "engine", test))]
fn rgba_frame(indices: &[u8], palette: &[u8]) -> Vec<u8> {
    let palette = palette_rgba(palette);
    let mut rgba = Vec::with_capacity(indices.len() * 4);
    for &index in indices {
        let colour = palette.get(index as usize).copied().unwrap_or(0);
        rgba.extend_from_slice(&colour.to_be_bytes());
    }
    rgba
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

    fn start_input(capabilities: Vec<&str>) -> CanvasStartInput {
        CanvasStartInput {
            width: 320,
            height: 200,
            dpr: 2.0,
            prefers_reduced_motion: false,
            state: Some(
                serde_json::json!({ "skill": 4 })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            fullscreen: false,
            capabilities: capabilities.into_iter().map(str::to_string).collect(),
            limits: CanvasLimits {
                max_file_bytes: 1234,
                max_file_bytes_per_canvas: 4321,
                max_file_handles: 4,
                max_file_read_bytes: 4096,
                max_framebuffer_pixels: 64000,
                max_palette_entries: 256,
                max_pickers: 8,
            },
        }
    }

    #[test]
    fn start_uses_discovered_limits_and_the_owner_difficulty() {
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec!["files.client", "framebuffer.indexed8"])).unwrap();
        GAME.with(|g| {
            let game = g.borrow();
            assert_eq!(game.skill, 4);
            assert_eq!(game.max_file_bytes, 1234);
            assert_eq!(game.max_file_read_bytes, 4096);
            assert!(game.error.is_none());
        });
    }

    #[test]
    fn start_falls_back_to_rgba_when_indexed_is_not_granted() {
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec!["files.client", "framebuffer.rgba8"])).unwrap();
        GAME.with(|g| {
            let game = g.borrow();
            assert_eq!(game.presentation, Presentation::Rgba);
            assert!(game.error.is_none());
        });
    }

    #[test]
    fn rgba_fallback_expands_palette_indices() {
        let mut palette = vec![0; 6];
        palette[..3].copy_from_slice(&[1, 2, 3]);
        palette[3..6].copy_from_slice(&[4, 5, 6]);
        let rgba = rgba_frame(&[0, 1, 255], &palette);
        assert_eq!(&rgba[0..4], &[1, 2, 3, 255]);
        assert_eq!(&rgba[4..8], &[4, 5, 6, 255]);
        assert_eq!(&rgba[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn start_reports_a_missing_required_capability() {
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec!["files.client"])).unwrap();
        GAME.with(|g| {
            let error = g.borrow().error.clone().unwrap();
            assert!(error.contains("framebuffer path"));
        });
    }

    #[test]
    fn wad_loading_progress_stays_below_complete_until_eof() {
        let load = WadLoad {
            id: "f".to_string(),
            size: 100,
            bytes: vec![0; 50],
            offset: 50,
        };
        assert_eq!(load.progress_percent(), 50);
        let full = WadLoad {
            bytes: vec![0; 100],
            ..load
        };
        assert_eq!(full.progress_percent(), 99);
    }

    #[test]
    fn difficulty_state_accepts_numbers_and_names_but_rejects_out_of_range() {
        assert_eq!(skill_from_value(&serde_json::json!(4)), Some(4));
        assert_eq!(skill_from_value(&serde_json::json!("Nightmare")), Some(4));
        assert_eq!(skill_from_value(&serde_json::json!("0")), Some(0));
        assert_eq!(skill_from_value(&serde_json::json!(5)), None);
        assert_eq!(skill_from_value(&serde_json::json!("impossible")), None);
        assert_eq!(
            presentation_for(
                &[
                    "files.client".to_string(),
                    "framebuffer.indexed8".to_string()
                ],
                256
            ),
            Some(Presentation::Indexed)
        );
        assert_eq!(
            presentation_for(
                &[
                    "files.client".to_string(),
                    "framebuffer.indexed8".to_string(),
                    "framebuffer.rgba8".to_string()
                ],
                255
            ),
            Some(Presentation::Rgba)
        );
        assert_eq!(
            presentation_for(
                &["files.client".to_string(), "framebuffer.rgba8".to_string()],
                0
            ),
            Some(Presentation::Rgba)
        );
        assert_eq!(presentation_for(&["files.client".to_string()], 256), None);
    }
}
