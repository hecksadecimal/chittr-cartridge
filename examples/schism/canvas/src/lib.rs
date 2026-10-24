// Canvas Guest for the original Schism Tracker engine. GPL-2.0; see ../../engine/LICENSE.
#[cfg(all(feature = "engine", target_arch = "wasm32"))]
mod allocator;
#[cfg(feature = "engine")]
mod audio;
#[cfg(feature = "engine")]
mod engine;
#[allow(dead_code, non_camel_case_types, clippy::let_unit_value)]
mod pdk;

#[cfg(feature = "engine")]
use chittr_cartridge::canvas::FileRef as ClientFileRef;
use chittr_cartridge::canvas::{rgba, Canvas2D, EventInput, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde_json::{Map, Value};
use std::cell::RefCell;
#[cfg(any(feature = "engine", test))]
use std::collections::BTreeSet;

const NATIVE_WIDTH: u32 = 640;
const NATIVE_HEIGHT: u32 = 400;
const FILE_READ_MAX: u32 = 64 * 1024;
const MODULE_CHUNKS_PER_FRAME: usize = 4;
const MAX_MODULE_BYTES: u64 = 16 * 1024 * 1024;
const FPS: u32 = 60;
const NOTICE_MS: f64 = 6000.0;

/// The v2 presentation path the host granted. Indexed8 is smaller on the wire
/// and keeps Schism's 8-bit palette exact; RGBA8 is the fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presentation {
    Indexed,
    Rgba,
}

struct ModuleLoad {
    id: String,
    name: String,
    size: u64,
    bytes: Vec<u8>,
    offset: u64,
}

impl ModuleLoad {
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
    paused: bool,
    error: Option<String>,
    notice: Option<String>,
    notice_until: f64,
    now: f64,
    max_file_bytes: u64,
    max_file_read_bytes: u32,
    max_pcm_chunk: usize,
    presentation: Presentation,
    module_load: Option<ModuleLoad>,
    module_name: Option<String>,
    #[cfg(feature = "engine")]
    engine: Option<engine::Engine>,
    #[cfg(feature = "engine")]
    audio: Option<audio::Audio>,
    #[cfg(feature = "engine")]
    keys: Keys,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            width: NATIVE_WIDTH,
            height: NATIVE_HEIGHT,
            paused: false,
            error: None,
            notice: None,
            notice_until: 0.0,
            now: 0.0,
            max_file_bytes: MAX_MODULE_BYTES,
            max_file_read_bytes: FILE_READ_MAX,
            max_pcm_chunk: 64 * 1024,
            presentation: Presentation::Indexed,
            module_load: None,
            module_name: None,
            #[cfg(feature = "engine")]
            engine: None,
            #[cfg(feature = "engine")]
            audio: None,
            #[cfg(feature = "engine")]
            keys: Keys::default(),
        }
    }
}

thread_local! { static GAME: RefCell<Game> = RefCell::new(Game::default()); }

/// The vendored tracker keeps process-wide C globals, so tests that boot it or
/// exercise the shared `GAME` cell must not run concurrently.
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|state| {
        let mut game = state.borrow_mut();
        game.begin(input)
    });
    Ok(reply(result))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|state| {
        let mut game = state.borrow_mut();
        game.now = finite(input.t).max(0.0);
        #[cfg(feature = "engine")]
        if game.module_load.is_some() && !game.paused && game.error.is_none() {
            if let Err(error) = game.pump_module_load() {
                game.error = Some(error);
            }
        }
        #[cfg(feature = "engine")]
        if !game.paused && game.error.is_none() {
            let now = game.now;
            if let Some(engine) = &mut game.engine {
                engine.tick(now);
                game.pump_audio();
            }
        }
        #[cfg(not(feature = "engine"))]
        let _ = input.dt;
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
            "focus" => game.resume(),
            "input_reset" => game.release_input(),
            "visibility" if flag(&input.rest, "visible") == Some(false) => game.pause(),
            "visibility" if flag(&input.rest, "visible") == Some(true) => game.resume(),
            "pointer" => game.pointer(&input.rest),
            "key" => game.key(&input.rest),
            "file" =>
            {
                #[cfg(all(feature = "engine", target_arch = "wasm32"))]
                if let Some((name, file)) = input.file() {
                    game.accept_module(&name, &file);
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
    fn begin(&mut self, input: CanvasStartInput) -> Result<(), String> {
        *self = Self::default();
        self.width = dimension(input.width as f64);
        self.height = dimension(input.height as f64);
        self.max_file_bytes = input.limits.max_file_bytes.max(0) as u64;
        self.max_file_read_bytes = input
            .limits
            .max_file_read_bytes
            .clamp(1, FILE_READ_MAX as i64) as u32;
        self.max_pcm_chunk = input.limits.max_pcm_chunk_bytes.clamp(2, 65536) as usize;

        if !has_capability(&input.capabilities, "files.client") {
            self.error =
                Some("This host does not provide the files.client capability Schism needs".into());
        } else if !has_capability(&input.capabilities, "audio.pcm") {
            self.error =
                Some("This host does not provide the audio.pcm capability Schism needs".into());
        } else if input.limits.max_framebuffer_pixels < (NATIVE_WIDTH * NATIVE_HEIGHT) as i32 {
            self.error = Some(format!(
                "This host's framebuffer limit ({} pixels) is too small for Schism's 640×400 screen",
                input.limits.max_framebuffer_pixels
            ));
        } else if let Some(presentation) =
            presentation_for(&input.capabilities, input.limits.max_palette_entries)
        {
            self.presentation = presentation;
        } else {
            self.error = Some(
                "This host does not grant a usable framebuffer path (indexed8 with 256 palette entries, or rgba8)"
                    .into(),
            );
        }

        set_fps(FPS)?;

        #[cfg(feature = "engine")]
        if self.error.is_none() {
            match engine::Engine::boot() {
                Ok(engine) => {
                    if has_capability(&input.capabilities, "audio.pcm") {
                        self.audio = Some(audio::Audio::new(&engine, self.max_pcm_chunk));
                    }
                    self.engine = Some(engine);
                }
                Err(error) => self.error = Some(error),
            }
        }

        self.show_notice(
            "Click the canvas to focus · F5 play · F8 stop · F10 menu",
            0.0,
        );
        Ok(())
    }

    #[cfg(feature = "engine")]
    fn pump_module_load(&mut self) -> Result<bool, String> {
        let Some(mut load) = self.module_load.take() else {
            return Ok(true);
        };
        let chunk_size = self.max_file_read_bytes.clamp(1, FILE_READ_MAX);
        let mut complete = false;
        for _ in 0..MODULE_CHUNKS_PER_FRAME {
            let chunk = match chittr_cartridge::canvas::host::file_read(
                &load.id,
                load.offset,
                chunk_size,
            ) {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = chittr_cartridge::canvas::host::file_close(&load.id);
                    return Err(format!("{}: {}", error.code, error.message));
                }
            };
            if chunk.bytes.is_empty() && !chunk.eof {
                let _ = chittr_cartridge::canvas::host::file_close(&load.id);
                return Err("The module read made no progress".into());
            }
            load.offset += chunk.bytes.len() as u64;
            load.bytes.extend_from_slice(&chunk.bytes);
            if load.bytes.len() as u64 > self.max_file_bytes {
                let _ = chittr_cartridge::canvas::host::file_close(&load.id);
                return Err(format!(
                    "The module is larger than this host's {}-byte limit",
                    self.max_file_bytes
                ));
            }
            if chunk.eof {
                complete = true;
                break;
            }
        }
        if !complete {
            self.module_load = Some(load);
            return Ok(false);
        }
        let _ = chittr_cartridge::canvas::host::file_close(&load.id);
        let loaded = self
            .engine
            .as_mut()
            .is_some_and(|engine| engine.load_module(&load.name, &load.bytes));
        if loaded {
            self.module_name = Some(load.name.clone());
            self.error = None;
            self.show_notice(&format!("Loaded {}", load.name), self.now);
            Ok(true)
        } else {
            Err(format!(
                "Schism Tracker could not load {}; it may not be a supported module",
                load.name
            ))
        }
    }

    #[cfg(not(feature = "engine"))]
    fn pump_module_load(&mut self) -> Result<bool, String> {
        self.module_load = None;
        Ok(true)
    }

    #[cfg(all(feature = "engine", target_arch = "wasm32"))]
    fn accept_module(&mut self, name: &str, file: &ClientFileRef) {
        if name != "module" {
            let _ = chittr_cartridge::canvas::host::file_close(&file.id);
            return;
        }
        #[cfg(feature = "engine")]
        if self.engine.is_none() {
            let _ = chittr_cartridge::canvas::host::file_close(&file.id);
            return;
        }
        if let Some(previous) = self.module_load.take() {
            let _ = chittr_cartridge::canvas::host::file_close(&previous.id);
        }
        if file.size <= 0 {
            let _ = chittr_cartridge::canvas::host::file_close(&file.id);
            self.error = Some("The selected module is empty".into());
            return;
        }
        let limit = self.max_file_bytes.min(MAX_MODULE_BYTES);
        if file.size as u64 > limit {
            let _ = chittr_cartridge::canvas::host::file_close(&file.id);
            self.error = Some(format!(
                "The module is {} bytes; the limit is {}",
                file.size, limit
            ));
            return;
        }
        self.notice = None;
        self.error = None;
        self.module_load = Some(ModuleLoad {
            id: file.id.clone(),
            name: file.filename.clone(),
            size: file.size as u64,
            bytes: Vec::with_capacity(file.size as usize),
            offset: 0,
        });
    }

    #[cfg(all(feature = "engine", target_arch = "wasm32"))]
    fn pump_audio(&mut self) {
        let (Some(engine), Some(audio)) = (&mut self.engine, &mut self.audio) else {
            return;
        };
        let Some(pcm) = audio.pump(engine) else {
            return;
        };
        if let Ok(reply) = chittr_cartridge::canvas::host::audio_write(
            "s16le",
            audio.rate(),
            audio.channels() as u8,
            &pcm,
        ) {
            audio.set_queued_ms(reply.queued_ms);
        }
    }

    #[cfg(not(all(feature = "engine", target_arch = "wasm32")))]
    fn pump_audio(&mut self) {}

    #[cfg(feature = "engine")]
    fn key(&mut self, rest: &Map<String, Value>) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        let action = text(rest, "action");
        let down = action == Some("down");
        let code = text(rest, "code").unwrap_or("");
        let key = text(rest, "key").unwrap_or("");

        // The host does not send modifier state, so the guest tracks it. Caps,
        // Num and Scroll lock are latching and cannot be read back from the DOM.
        if down {
            match code {
                "CapsLock" => self.keys.caps = !self.keys.caps,
                "NumLock" => self.keys.num = !self.keys.num,
                "ScrollLock" => self.keys.scroll = !self.keys.scroll,
                _ => {}
            }
        }
        if let Some(bit) = modifier_bit(code) {
            if down {
                self.keys.mods |= bit;
            } else {
                self.keys.mods &= !bit;
            }
        }
        let mods = self.keys.modstate();
        engine.set_keymod(mods);
        // The host reserves Escape and never delivers it. F10 becomes Escape so
        // the tracker's main menu (and backing out of dialogs) stays reachable.
        if is_menu_key(code, self.keys.mods) {
            let first_press = down && self.keys.held.insert("Escape".to_string());
            if !down {
                self.keys.held.remove("Escape");
            }
            if first_press || !down {
                engine.set_keymod(0);
                engine.key(
                    keysym_for("Escape", "Escape"),
                    scancode_for("Escape").unwrap(),
                    0,
                    None,
                    down,
                );
            }
            return;
        }
        let Some(scancode) = scancode_for(code) else {
            return;
        };
        if down {
            self.keys.held.insert(code.to_string());
        } else {
            self.keys.held.remove(code);
        }
        let sym = keysym_for(code, key);
        let raw_text = if down { text_for(key) } else { None };
        engine.key(sym, scancode, mods, raw_text, down);
    }

    #[cfg(not(feature = "engine"))]
    fn key(&mut self, _rest: &Map<String, Value>) {}

    #[cfg(feature = "engine")]
    fn pointer(&mut self, rest: &Map<String, Value>) {
        if rest.get("relative").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let action = text(rest, "action").unwrap_or("move");
        if action == "down" && self.paused {
            self.resume();
        }
        let Some(engine) = &mut self.engine else {
            return;
        };
        let css_width = number(rest, "width").filter(|value| *value > 0.0);
        let css_height = number(rest, "height").filter(|value| *value > 0.0);
        let (x, y) = pointer_position(
            number(rest, "x").unwrap_or(0.0),
            number(rest, "y").unwrap_or(0.0),
            css_width.unwrap_or(self.width as f64),
            css_height.unwrap_or(self.height as f64),
        );
        let buttons = rest.get("buttons").and_then(Value::as_u64).unwrap_or(0) as u32;

        engine.set_keymod(self.keys.modstate());
        engine.mouse_move(x, y);
        let previous = self.keys.buttons;
        let (released, pressed) = match action {
            "down" => (previous & !buttons, buttons & !previous),
            "up" => (previous & !buttons, buttons & !previous),
            "cancel" => (previous, 0),
            _ => (previous & !buttons, buttons & !previous),
        };
        for (mask, button) in [(1u32, 0i32), (4, 1), (2, 2)] {
            if released & mask != 0 {
                engine.mouse_button(button, false, x, y);
            }
            if pressed & mask != 0 {
                engine.mouse_button(button, true, x, y);
            }
        }
        self.keys.buttons = if action == "cancel" { 0 } else { buttons };
        if let Some(engine) = &mut self.engine {
            engine.mouse_move(x, y);
        }
    }

    #[cfg(not(feature = "engine"))]
    fn pointer(&mut self, _rest: &Map<String, Value>) {}

    fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.paused = true;
        self.release_input();
        #[cfg(feature = "engine")]
        if let Some(engine) = &mut self.engine {
            engine.focus(false);
            if let Some(audio) = &mut self.audio {
                audio.clear(engine);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = chittr_cartridge::canvas::host::audio_stop();
    }

    fn resume(&mut self) {
        if !self.paused {
            return;
        }
        self.paused = false;
        #[cfg(feature = "engine")]
        if let Some(engine) = &mut self.engine {
            engine.focus(true);
        }
    }

    #[cfg(feature = "engine")]
    fn release_input(&mut self) {
        let held: Vec<String> = std::mem::take(&mut self.keys.held).into_iter().collect();
        if let Some(engine) = &mut self.engine {
            for code in held {
                if let Some(scancode) = scancode_for(&code) {
                    engine.key(keysym_for(&code, ""), scancode, 0, None, false);
                }
            }
            for (mask, button) in [(1u32, 0i32), (2, 2), (4, 1)] {
                if self.keys.buttons & mask != 0 {
                    engine.mouse_button(button, false, 0, 0);
                }
            }
            self.keys.mods = 0;
            self.keys.caps = false;
            self.keys.num = false;
            self.keys.scroll = false;
            self.keys.buttons = 0;
            engine.set_keymod(0);
        }
    }

    #[cfg(not(feature = "engine"))]
    fn release_input(&mut self) {}

    fn animating(&self) -> bool {
        #[cfg(feature = "engine")]
        {
            !self.paused
                && self.error.is_none()
                && (self.engine.is_some() || self.module_load.is_some())
        }
        #[cfg(not(feature = "engine"))]
        {
            false
        }
    }

    fn show_notice(&mut self, message: &str, now: f64) {
        self.notice = Some(message.to_string());
        self.notice_until = now + NOTICE_MS;
    }

    fn draw(&self) -> Result<(), String> {
        let mut canvas = Canvas2D::new();
        canvas.fill_color(rgba(8, 10, 10, 255)).fill_rect(
            0.0,
            0.0,
            self.width as f32,
            self.height as f32,
        );
        if let Some(error) = &self.error {
            self.message(&mut canvas, "Schism Tracker stopped", error);
            return submit(&mut canvas);
        }
        #[cfg(feature = "engine")]
        if let Some(engine) = &self.engine {
            let (indices, palette) = engine.image();
            match self.presentation {
                Presentation::Indexed => {
                    canvas.blit_indexed_scaled(
                        destination(self.width, self.height),
                        NATIVE_WIDTH,
                        NATIVE_HEIGHT,
                        false,
                        palette,
                        indices,
                    );
                }
                Presentation::Rgba => {
                    let pixels = rgba_frame(indices, palette);
                    canvas.blit_rgba_scaled(
                        destination(self.width, self.height),
                        NATIVE_WIDTH,
                        NATIVE_HEIGHT,
                        false,
                        &pixels,
                    );
                }
            }
            submit(&mut canvas)?;
            if let Some(load) = &self.module_load {
                self.loading_banner(&mut canvas, load);
                submit(&mut canvas)?;
            }
            if let Some(notice) = self.active_notice() {
                self.notice_banner(&mut canvas, notice);
                submit(&mut canvas)?;
            }
            if self.paused {
                self.message(&mut canvas, "Paused", "Click the canvas to resume");
                submit(&mut canvas)?;
            }
            return Ok(());
        }
        let message = if let Some(load) = &self.module_load {
            format!("Loading {}… {}%", load.name, load.progress_percent())
        } else {
            "Preparing the tracker engine…".to_string()
        };
        self.message(&mut canvas, "Schism Tracker", &message);
        submit(&mut canvas)
    }

    fn active_notice(&self) -> Option<&str> {
        let notice = self.notice.as_deref()?;
        if self.now > self.notice_until {
            return None;
        }
        Some(notice)
    }

    fn loading_banner(&self, canvas: &mut Canvas2D, load: &ModuleLoad) {
        let scale = (self.width as f32 / NATIVE_WIDTH as f32)
            .min(self.height as f32 / NATIVE_HEIGHT as f32)
            .max(1.0);
        let height = 20.0 * scale;
        let cx = self.width as f32 / 2.0;
        let name: String = load.name.chars().take(40).collect();
        canvas
            .fill_color(rgba(8, 10, 10, 225))
            .fill_rect(0.0, self.height as f32 - height, self.width as f32, height)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_color(rgba(160, 210, 190, 255))
            .font("monospace", 11.0 * scale, 700.0)
            .fill_text(
                &format!("Loading {name}… {}%", load.progress_percent()),
                cx,
                self.height as f32 - height / 2.0,
            );
    }

    fn notice_banner(&self, canvas: &mut Canvas2D, notice: &str) {
        let scale = (self.width as f32 / NATIVE_WIDTH as f32)
            .min(self.height as f32 / NATIVE_HEIGHT as f32)
            .max(1.0);
        let height = 20.0 * scale;
        let cx = self.width as f32 / 2.0;
        canvas
            .fill_color(rgba(8, 10, 10, 220))
            .fill_rect(0.0, self.height as f32 - height, self.width as f32, height)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_color(rgba(210, 190, 120, 255))
            .font("monospace", 10.0 * scale, 400.0)
            .fill_text(notice, cx, self.height as f32 - height / 2.0);
    }

    fn message(&self, canvas: &mut Canvas2D, title: &str, message: &str) {
        let scale = (self.width as f32 / NATIVE_WIDTH as f32)
            .min(self.height as f32 / NATIVE_HEIGHT as f32)
            .max(0.5);
        let cx = self.width as f32 / 2.0;
        let cy = self.height as f32 / 2.0;
        canvas
            .fill_color(rgba(8, 10, 10, 235))
            .fill_rect(0.0, cy - 52.0 * scale, self.width as f32, 104.0 * scale)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_color(rgba(120, 210, 180, 255))
            .font("monospace", 26.0 * scale, 900.0)
            .fill_text(title, cx, cy - 20.0 * scale)
            .fill_color(rgba(220, 225, 220, 255))
            .font("monospace", 10.0 * scale, 400.0)
            .fill_text(message, cx, cy + 20.0 * scale);
        if let Some(notice) = &self.notice {
            canvas
                .fill_color(rgba(210, 190, 120, 255))
                .font("monospace", 7.0 * scale, 400.0)
                .fill_text(notice, cx, cy + 38.0 * scale);
        }
    }
}

#[cfg(any(feature = "engine", test))]
#[derive(Default)]
struct Keys {
    held: BTreeSet<String>,
    mods: u32,
    caps: bool,
    num: bool,
    scroll: bool,
    buttons: u32,
}

#[cfg(any(feature = "engine", test))]
impl Keys {
    fn modstate(&self) -> u32 {
        let mut mods = self.mods;
        if self.caps {
            mods |= KEYMOD_CAPS;
        }
        if self.num {
            mods |= KEYMOD_NUM;
        }
        if self.scroll {
            mods |= KEYMOD_SCROLL;
        }
        mods
    }
}

const KEYMOD_LCTRL: u32 = 1 << 0;
const KEYMOD_RCTRL: u32 = 1 << 1;
const KEYMOD_LSHIFT: u32 = 1 << 2;
const KEYMOD_RSHIFT: u32 = 1 << 3;
const KEYMOD_LALT: u32 = 1 << 4;
const KEYMOD_RALT: u32 = 1 << 5;
const KEYMOD_LGUI: u32 = 1 << 6;
const KEYMOD_RGUI: u32 = 1 << 7;
const KEYMOD_NUM: u32 = 1 << 8;
const KEYMOD_CAPS: u32 = 1 << 9;
const KEYMOD_SCROLL: u32 = 1 << 11;

fn modifier_bit(code: &str) -> Option<u32> {
    Some(match code {
        "ControlLeft" => KEYMOD_LCTRL,
        "ControlRight" => KEYMOD_RCTRL,
        "ShiftLeft" => KEYMOD_LSHIFT,
        "ShiftRight" => KEYMOD_RSHIFT,
        "AltLeft" => KEYMOD_LALT,
        "AltRight" => KEYMOD_RALT,
        "MetaLeft" => KEYMOD_LGUI,
        "MetaRight" => KEYMOD_RGUI,
        _ => return None,
    })
}

/// The host owns Escape, so unmodified F10 stands in for it. Locks are not
/// modifiers here: Caps Lock must not stop the menu from opening.
fn is_menu_key(code: &str, modifiers: u32) -> bool {
    code == "F10" && modifiers == 0
}

/// USB HID keyboard page scancodes, matching Schism's `SCHISM_SCANCODE_*`.
fn scancode_for(code: &str) -> Option<u32> {
    Some(match code {
        "KeyA" => 4,
        "KeyB" => 5,
        "KeyC" => 6,
        "KeyD" => 7,
        "KeyE" => 8,
        "KeyF" => 9,
        "KeyG" => 10,
        "KeyH" => 11,
        "KeyI" => 12,
        "KeyJ" => 13,
        "KeyK" => 14,
        "KeyL" => 15,
        "KeyM" => 16,
        "KeyN" => 17,
        "KeyO" => 18,
        "KeyP" => 19,
        "KeyQ" => 20,
        "KeyR" => 21,
        "KeyS" => 22,
        "KeyT" => 23,
        "KeyU" => 24,
        "KeyV" => 25,
        "KeyW" => 26,
        "KeyX" => 27,
        "KeyY" => 28,
        "KeyZ" => 29,
        "Digit1" => 30,
        "Digit2" => 31,
        "Digit3" => 32,
        "Digit4" => 33,
        "Digit5" => 34,
        "Digit6" => 35,
        "Digit7" => 36,
        "Digit8" => 37,
        "Digit9" => 38,
        "Digit0" => 39,
        "Enter" => 40,
        "Escape" => 41,
        "Backspace" => 42,
        "Tab" => 43,
        "Space" => 44,
        "Minus" => 45,
        "Equal" => 46,
        "BracketLeft" => 47,
        "BracketRight" => 48,
        "Backslash" => 49,
        "Semicolon" => 51,
        "Quote" => 52,
        "Backquote" => 53,
        "Comma" => 54,
        "Period" => 55,
        "Slash" => 56,
        "CapsLock" => 57,
        "F1" => 58,
        "F2" => 59,
        "F3" => 60,
        "F4" => 61,
        "F5" => 62,
        "F6" => 63,
        "F7" => 64,
        "F8" => 65,
        "F9" => 66,
        "F10" => 67,
        "F11" => 68,
        "F12" => 69,
        "PrintScreen" => 70,
        "ScrollLock" => 71,
        "Pause" => 72,
        "Insert" => 73,
        "Home" => 74,
        "PageUp" => 75,
        "Delete" => 76,
        "End" => 77,
        "PageDown" => 78,
        "ArrowRight" => 79,
        "ArrowLeft" => 80,
        "ArrowDown" => 81,
        "ArrowUp" => 82,
        "NumLock" => 83,
        "NumpadDivide" => 84,
        "NumpadMultiply" => 85,
        "NumpadSubtract" => 86,
        "NumpadAdd" => 87,
        "NumpadEnter" => 88,
        "Numpad1" => 89,
        "Numpad2" => 90,
        "Numpad3" => 91,
        "Numpad4" => 92,
        "Numpad5" => 93,
        "Numpad6" => 94,
        "Numpad7" => 95,
        "Numpad8" => 96,
        "Numpad9" => 97,
        "Numpad0" => 98,
        "NumpadDecimal" => 99,
        "NumpadEqual" => 103,
        "ControlLeft" => 224,
        "ShiftLeft" => 225,
        "AltLeft" => 226,
        "MetaLeft" => 227,
        "ControlRight" => 228,
        "ShiftRight" => 229,
        "AltRight" => 230,
        "MetaRight" => 231,
        _ => return None,
    })
}

/// Schism's keysyms mirror SDL2: printable characters use their code point,
/// while navigation keys are the scancode with bit 30 set.
fn keysym_for(code: &str, key: &str) -> u32 {
    let named = match code {
        "Enter" => Some(13),
        "Escape" => Some(27),
        "Backspace" => Some(8),
        "Tab" => Some(9),
        "Delete" => Some(0x7f),
        "Insert" => Some(0x4000_0049),
        "Home" => Some(0x4000_004a),
        "PageUp" => Some(0x4000_004b),
        "End" => Some(0x4000_004d),
        "PageDown" => Some(0x4000_004e),
        "ArrowRight" => Some(0x4000_004f),
        "ArrowLeft" => Some(0x4000_0050),
        "ArrowDown" => Some(0x4000_0051),
        "ArrowUp" => Some(0x4000_0052),
        "CapsLock" => Some(0x4000_0039),
        "PrintScreen" => Some(0x4000_0046),
        "ScrollLock" => Some(0x4000_0047),
        "Pause" => Some(0x4000_0048),
        "NumLock" => Some(0x4000_0053),
        "NumpadDivide" => Some(0x4000_0054),
        "NumpadMultiply" => Some(0x4000_0055),
        "NumpadSubtract" => Some(0x4000_0056),
        "NumpadAdd" => Some(0x4000_0057),
        "NumpadEnter" => Some(0x4000_0058),
        "Numpad1" => Some(0x4000_0059),
        "Numpad2" => Some(0x4000_005a),
        "Numpad3" => Some(0x4000_005b),
        "Numpad4" => Some(0x4000_005c),
        "Numpad5" => Some(0x4000_005d),
        "Numpad6" => Some(0x4000_005e),
        "Numpad7" => Some(0x4000_005f),
        "Numpad8" => Some(0x4000_0060),
        "Numpad9" => Some(0x4000_0061),
        "Numpad0" => Some(0x4000_0062),
        "NumpadDecimal" => Some(0x4000_0063),
        "NumpadEqual" => Some(0x4000_0067),
        "F1" => Some(0x4000_003a),
        "F2" => Some(0x4000_003b),
        "F3" => Some(0x4000_003c),
        "F4" => Some(0x4000_003d),
        "F5" => Some(0x4000_003e),
        "F6" => Some(0x4000_003f),
        "F7" => Some(0x4000_0040),
        "F8" => Some(0x4000_0041),
        "F9" => Some(0x4000_0042),
        "F10" => Some(0x4000_0043),
        "F11" => Some(0x4000_0044),
        "F12" => Some(0x4000_0045),
        "ControlLeft" => Some(0x4000_00e0),
        "ShiftLeft" => Some(0x4000_00e1),
        "AltLeft" => Some(0x4000_00e2),
        "MetaLeft" => Some(0x4000_00e3),
        "ControlRight" => Some(0x4000_00e4),
        "ShiftRight" => Some(0x4000_00e5),
        "AltRight" => Some(0x4000_00e6),
        "MetaRight" => Some(0x4000_00e7),
        _ => None,
    };
    if let Some(sym) = named {
        return sym;
    }
    if let Some(sym) = scancode_for(code) {
        // Printable punctuation and alphanumerics keep their character so
        // Schism's layout translation can see the shifted form.
        if let Some(ch) = single_char(key) {
            return ch as u32;
        }
        return (sym | 0x4000_0000) as u32;
    }
    single_char(key).map(|ch| ch as u32).unwrap_or(0)
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() || ch.is_control() {
        return None;
    }
    Some(ch)
}

/// Text input is the DOM's `key` for printable keys. Named keys return
/// strings such as "Enter" and are deliberately not forwarded as text.
fn text_for(key: &str) -> Option<&str> {
    let ch = single_char(key)?;
    Some(if ch == '\u{a0}' { " " } else { key })
}

fn pointer_position(x: f64, y: f64, width: f64, height: f64) -> (i32, i32) {
    let map = |value: f64, css: f64, native: u32| {
        if !value.is_finite() || !css.is_finite() || css <= 0.0 {
            return 0;
        }
        (value / css * native as f64)
            .floor()
            .clamp(0.0, native.saturating_sub(1) as f64) as i32
    };
    (map(x, width, NATIVE_WIDTH), map(y, height, NATIVE_HEIGHT))
}

fn has_capability(capabilities: &[String], id: &str) -> bool {
    capabilities.iter().any(|granted| granted == id)
}

fn presentation_for(capabilities: &[String], palette_entries: i32) -> Option<Presentation> {
    if has_capability(capabilities, "framebuffer.indexed8") && palette_entries >= 256 {
        Some(Presentation::Indexed)
    } else if has_capability(capabilities, "framebuffer.rgba8") {
        Some(Presentation::Rgba)
    } else {
        None
    }
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

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn dimension(value: f64) -> u32 {
    if value.is_finite() {
        value.clamp(1.0, 2048.0) as u32
    } else {
        NATIVE_WIDTH
    }
}

#[cfg(any(feature = "engine", test))]
fn destination(width: u32, height: u32) -> [f32; 4] {
    let scale = (width as f64 / NATIVE_WIDTH as f64).min(height as f64 / NATIVE_HEIGHT as f64);
    let w = (NATIVE_WIDTH as f64 * scale).floor().max(1.0);
    let h = (NATIVE_HEIGHT as f64 * scale).floor().max(1.0);
    [
        (width as f64 - w) as f32 / 2.0,
        (height as f64 - h) as f32 / 2.0,
        w as f32,
        h as f32,
    ]
}

fn rgba_frame(indices: &[u8], palette: &[u32]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(indices.len() * 4);
    for &index in indices {
        let colour = palette.get(index as usize).copied().unwrap_or(0);
        rgba.extend_from_slice(&colour.to_be_bytes());
    }
    rgba
}

fn reply(result: Result<(), String>) -> CanvasReply {
    CanvasReply {
        error: result.err().map(|message| HookError {
            code: "schism".into(),
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
    fn scancodes_cover_the_tracker_keyboard() {
        assert_eq!(scancode_for("KeyA"), Some(4));
        assert_eq!(scancode_for("Space"), Some(44));
        assert_eq!(scancode_for("F5"), Some(62));
        assert_eq!(scancode_for("Numpad5"), Some(93));
        assert_eq!(scancode_for("ControlLeft"), Some(224));
        assert_eq!(scancode_for("F13"), None);
    }

    #[test]
    fn keysyms_match_sdl2_control_and_navigation_values() {
        assert_eq!(keysym_for("Escape", "Escape"), 27);
        assert_eq!(keysym_for("Enter", "Enter"), 13);
        assert_eq!(keysym_for("Delete", "Delete"), 0x7f);
        assert_eq!(keysym_for("ArrowUp", "ArrowUp"), 0x4000_0052);
        assert_eq!(keysym_for("F10", "F10"), 0x4000_0043);
        assert_eq!(keysym_for("KeyA", "a"), 'a' as u32);
        assert_eq!(keysym_for("Digit1", "!"), '!' as u32);
        assert_eq!(keysym_for("Numpad1", "1"), 0x4000_0059);
    }

    #[test]
    fn text_input_skips_named_keys() {
        assert_eq!(text_for("a"), Some("a"));
        assert_eq!(text_for(" "), Some(" "));
        assert_eq!(text_for("Enter"), None);
        assert_eq!(text_for("ArrowUp"), None);
        assert_eq!(text_for("\u{0}"), None);
    }

    #[test]
    fn pointer_coordinates_scale_from_css_to_the_native_screen() {
        assert_eq!(pointer_position(0.0, 0.0, 640.0, 400.0), (0, 0));
        assert_eq!(pointer_position(320.0, 200.0, 640.0, 400.0), (320, 200));
        assert_eq!(pointer_position(1000.0, 1000.0, 320.0, 200.0), (639, 399));
        assert_eq!(pointer_position(-10.0, f64::NAN, 640.0, 400.0), (0, 0));
    }

    #[test]
    fn modifier_state_is_reported_to_the_engine() {
        let mut keys = Keys::default();
        keys.mods = KEYMOD_LSHIFT | KEYMOD_LCTRL;
        keys.caps = true;
        let mods = keys.modstate();
        assert_eq!(mods & KEYMOD_LSHIFT, KEYMOD_LSHIFT);
        assert_eq!(mods & KEYMOD_LCTRL, KEYMOD_LCTRL);
        assert_eq!(mods & KEYMOD_CAPS, KEYMOD_CAPS);
        assert_eq!(mods & KEYMOD_RALT, 0);
    }

    #[test]
    fn unmodified_f10_stands_in_for_the_host_reserved_escape() {
        assert!(is_menu_key("F10", 0));
        assert!(!is_menu_key("F10", KEYMOD_LSHIFT));
        assert!(!is_menu_key("F10", KEYMOD_LCTRL | KEYMOD_LSHIFT));
        assert!(!is_menu_key("F9", 0));
    }

    #[test]
    fn indexed_presentation_fits_and_stays_under_the_draw_limit() {
        for (width, height) in [(640, 400), (1280, 800), (2048, 2048), (320, 200)] {
            let [dx, dy, dw, dh] = destination(width, height);
            assert!(dw >= 1.0 && dh >= 1.0);
            assert!(dx >= 0.0 && dy >= 0.0);
            assert!(dx + dw <= width as f32 + 0.5);
            assert!(dy + dh <= height as f32 + 0.5);
        }
        assert_eq!(destination(640, 400), [0.0, 0.0, 640.0, 400.0]);
        assert!(640 * 400 + 256 * 4 + 64 < 4 * 1024 * 1024);
    }

    #[test]
    fn rgba_fallback_expands_indexed_colours() {
        let rgba = rgba_frame(&[0, 1, 255], &[0x1122_33ff, 0xaabb_ccff]);
        assert_eq!(&rgba[0..4], &[0x11, 0x22, 0x33, 0xff]);
        assert_eq!(&rgba[4..8], &[0xaa, 0xbb, 0xcc, 0xff]);
        assert_eq!(&rgba[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn start_reports_missing_capabilities() {
        let _guard = test_lock();
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec!["framebuffer.indexed8"])).unwrap();
        GAME.with(|g| {
            assert!(g
                .borrow()
                .error
                .as_deref()
                .unwrap()
                .contains("files.client"));
        });
    }

    #[test]
    fn start_falls_back_to_rgba_when_indexed_is_not_granted() {
        let _guard = test_lock();
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec![
            "files.client",
            "audio.pcm",
            "framebuffer.rgba8",
        ]))
        .unwrap();
        GAME.with(|g| {
            let game = g.borrow();
            assert_eq!(game.presentation, Presentation::Rgba);
            assert!(game.error.is_none());
        });
    }

    #[test]
    fn start_reports_missing_audio() {
        let _guard = test_lock();
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec!["files.client", "framebuffer.indexed8"])).unwrap();
        GAME.with(|g| {
            assert!(g.borrow().error.as_deref().unwrap().contains("audio.pcm"));
        });
    }

    #[test]
    fn start_accepts_indexed_when_the_palette_is_large_enough() {
        let _guard = test_lock();
        GAME.with(|g| *g.borrow_mut() = Game::default());
        start(start_input(vec![
            "files.client",
            "audio.pcm",
            "framebuffer.indexed8",
        ]))
        .unwrap();
        GAME.with(|g| {
            let game = g.borrow();
            assert_eq!(game.presentation, Presentation::Indexed);
            assert!(game.error.is_none());
        });
    }

    #[test]
    fn module_progress_stays_below_complete_until_eof() {
        let load = ModuleLoad {
            id: "f".into(),
            name: "song.it".into(),
            size: 200,
            bytes: vec![0; 100],
            offset: 100,
        };
        assert_eq!(load.progress_percent(), 50);
        let full = ModuleLoad {
            bytes: vec![0; 200],
            ..load
        };
        assert_eq!(full.progress_percent(), 99);
    }

    fn start_input(capabilities: Vec<&str>) -> CanvasStartInput {
        CanvasStartInput {
            width: 640,
            height: 400,
            dpr: 1.0,
            prefers_reduced_motion: false,
            state: None,
            fullscreen: false,
            capabilities: capabilities.into_iter().map(str::to_string).collect(),
            limits: CanvasLimits {
                max_file_bytes: 16 * 1024 * 1024,
                max_file_bytes_per_canvas: 32 * 1024 * 1024,
                max_file_handles: 4,
                max_file_read_bytes: 64 * 1024,
                max_framebuffer_pixels: 640 * 400,
                max_palette_entries: 256,
                max_pickers: 8,
                max_relative_pointer_delta: 2000,
                max_pcm_chunk_bytes: 64 * 1024,
                pcm_queue_ms: 2000,
            },
        }
    }
}
