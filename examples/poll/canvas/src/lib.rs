//! The poll Cartridge's Canvas Guest: a bar chart of the poll's results. The results come from the render's
//! `canvas` state (`{ bars: [{ label, votes }] }`), delivered on start and again on every change as an
//! `event` of type `state`. The bars grow over 600 ms, then the guest stops asking for frames.
//!
//! A pointer release over a bar sends its index to the Server Guest (`send`). The reply, that
//! option's tally and share, is written under the chart; an error is written there too.
//!
//! The chart shows the poll's pizza image (`load_image`) and sets its labels in the manifest's Mono font.
//! When a vote raises the total, the guest plays a short blip (`load_sound`, `play_sound`). The sound is silent
//! until the viewer turns the speaker on, and the canvas is `fullscreen`, so the host draws the expand button.
//!
//! The exports, host calls and events are the bindings `xtp plugin init` generates from
//! `docs/cartridges/v1/canvas.xtp.yaml` (see the README). Drawing goes through `chittr-cartridge`'s command-buffer
//! encoder (`Canvas2D`), the one part a scaffold can't provide.

mod pdk;

use std::cell::RefCell;

use chittr_cartridge::canvas::{Canvas2D, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Map, Value};

const BACKGROUND: u32 = 0x1018_20ff;
const BAR: u32 = 0xf59e_0bff;
const INK: u32 = 0xe5e7_ebff;
const GROW_MS: f64 = 600.0;
const BARS_MAX: usize = 16;

#[derive(Debug, Clone, Deserialize)]
struct Bar {
    label: String,
    votes: u32,
}

#[derive(Deserialize)]
struct Results {
    bars: Vec<Bar>,
}

#[derive(Default)]
struct State {
    bars: Vec<Bar>,
    width: f32,
    height: f32,
    /// The frame time at which the current growth began, once the first frame has run.
    grow_from: Option<f64>,
    /// True when something changed and the chart must be drawn again.
    dirty: bool,
    /// Each bar's column as it was last drawn, in backing pixels: x, y, width, height. A click inside one sends its index.
    columns: Vec<(f32, f32, f32, f32)>,
    /// The last reply or error, written under the chart.
    caption: Option<String>,
    /// The sequence number of the next message.
    seq: u32,
    /// The pizza image's handle from `load_image`, and once it has decoded, its pixel size.
    image_handle: Option<u32>,
    image: Option<(u32, u32, u32)>,
    /// The blip's handle from `load_sound`, and whether it has decoded.
    sound_handle: Option<u32>,
    sound_ready: bool,
    /// The total votes the chart last knew about: a rise in it is a vote, and plays the blip.
    total_votes: Option<u64>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// A failed host call or draw: the code and message the guest reports, as a `HookError`-shaped pair.
#[derive(Debug)]
struct Failed {
    code: String,
    message: String,
}

impl Failed {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }
}

impl From<chittr_cartridge::canvas::CanvasError> for Failed {
    fn from(error: chittr_cartridge::canvas::CanvasError) -> Self {
        Self::new(&error.code, error.message)
    }
}

/// The return of an export: `{}` on success, or the failure as its `error` field.
fn reply(result: Result<(), Failed>) -> CanvasReply {
    CanvasReply {
        error: result.err().map(|error| HookError { code: error.code, message: error.message }),
    }
}

/// The code of a generated error enum, as its wire string.
fn code_of<T: Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "host".to_string())
}

/// A host function's reply, as success or the host's error.
fn checked(error: Option<CanvasError>) -> Result<(), Failed> {
    match error {
        Some(error) => Err(Failed::new(&code_of(&error.code), error.message)),
        None => Ok(()),
    }
}

fn host_failed(error: Error) -> Failed {
    Failed::new("host", error.to_string())
}

/// Loads an asset and returns its handle at once; the host says when it has decoded.
fn load(path: &str, sound: bool) -> Option<u32> {
    let input = CanvasLoadInput { path: path.to_string() };
    let output = if sound { pdk::load_sound(input) } else { pdk::load_image(input) };
    let output = output.ok()?;
    if output.error.is_some() {
        return None;
    }
    output.handle.map(|handle| handle as u32)
}

fn bars_from(state: Option<&Map<String, Value>>) -> Vec<Bar> {
    state
        .and_then(|state| serde_json::from_value::<Results>(Value::Object(state.clone())).ok())
        .map(|results| results.bars.into_iter().take(BARS_MAX).collect())
        .unwrap_or_default()
}

fn total_votes(bars: &[Bar]) -> u64 {
    bars.iter().map(|bar| u64::from(bar.votes)).sum()
}

fn reset_growth(state: &mut State, bars: Vec<Bar>) {
    state.bars = bars;
    state.grow_from = None;
    state.dirty = true;
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    // The image and the blip are the Cartridge's assets. The host decodes them and says so with `image_loaded` and
    // `sound_loaded`; until then the chart draws without them.
    let image_handle = load("pizza.webp", false);
    let sound_handle = load("blip.wav", true);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.width = input.width as f32;
        s.height = input.height as f32;
        let bars = bars_from(input.state.as_ref());
        s.total_votes = Some(total_votes(&bars));
        reset_growth(&mut s, bars);
        s.image_handle = image_handle;
        s.sound_handle = sound_handle;
    });
    Ok(reply(set_fps(30).and_then(|()| request_frame())))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = STATE.with(|s| draw(&mut s.borrow_mut(), input.t));
    Ok(reply(result))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let result = match input.r#type {
        CanvasEventType::State => {
            let bars = bars_from(input.state.as_ref());
            let total = total_votes(&bars);
            // A rise in the total is a vote: play the blip, if the sound has decoded.
            let blip = STATE.with(|s| {
                let mut s = s.borrow_mut();
                let rose = s.total_votes.is_some_and(|before| total > before);
                s.total_votes = Some(total);
                reset_growth(&mut s, bars);
                if rose && s.sound_ready { s.sound_handle } else { None }
            });
            if let Some(handle) = blip {
                // Dropped while the speaker is off, so this is silent until the viewer turns it on.
                let _ = pdk::play_sound(CanvasPlaySoundInput {
                    handle: handle as i32,
                    r#loop: Some(false),
                    volume: Some(0.6),
                });
            }
            request_frame()
        }
        CanvasEventType::Image_loaded => {
            let handle = input.handle.unwrap_or(0) as u32;
            let width = input.width.unwrap_or(0.0) as u32;
            let height = input.height.unwrap_or(0.0) as u32;
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                if s.image_handle == Some(handle) {
                    s.image = Some((handle, width, height));
                    s.dirty = true;
                }
            });
            request_frame()
        }
        CanvasEventType::Sound_loaded => {
            let handle = input.handle.unwrap_or(0) as u32;
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                if s.sound_handle == Some(handle) {
                    s.sound_ready = true;
                }
            });
            Ok(())
        }
        // A failed load names its handle. The chart goes on without that asset.
        CanvasEventType::Error if input.handle.is_some() => Ok(()),
        CanvasEventType::Resize => {
            let width = input.width.unwrap_or(0.0) as f32;
            let height = input.height.unwrap_or(0.0) as f32;
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.width = width;
                s.height = height;
                s.dirty = true;
            });
            request_frame()
        }
        CanvasEventType::Visibility => {
            STATE.with(|s| s.borrow_mut().dirty = true);
            request_frame()
        }
        CanvasEventType::Pointer => pointer(&input),
        CanvasEventType::Reply => {
            let caption = reply_caption(input.reply.as_ref());
            set_caption(caption);
            request_frame()
        }
        CanvasEventType::Error => {
            let code = input.code.as_deref().unwrap_or("error");
            set_caption(format!("That didn't work ({code})"));
            request_frame()
        }
        // Unknown events are ignored, as the ABI asks.
        _ => Ok(()),
    };
    Ok(reply(result))
}

fn set_fps(fps: i32) -> Result<(), Failed> {
    checked(pdk::set_fps(SetFpsInput { fps }).map_err(host_failed)?.error)
}

fn request_frame() -> Result<(), Failed> {
    checked(pdk::request_frame().map_err(host_failed)?.error)
}

fn set_caption(caption: String) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.caption = Some(caption);
        s.dirty = true;
    });
}

/// The caption for a bar's reply: its label, tally and share.
fn reply_caption(reply: Option<&Map<String, Value>>) -> String {
    let field = |name: &str| reply.and_then(|r| r.get(name));
    let label = field("label").and_then(Value::as_str).unwrap_or("That option");
    let votes = field("votes").and_then(Value::as_i64).unwrap_or(0);
    let percent = field("percent").and_then(Value::as_i64).unwrap_or(0);
    format!("{label}: {votes} votes ({percent}%)")
}

/// A pointer release over a bar sends that bar's index. The event's coordinates are CSS pixels of the canvas box, so
/// they are scaled to the backing store, where the columns were drawn.
fn pointer(input: &CanvasEventInput) -> Result<(), Failed> {
    if input.action.as_deref() != Some("up") {
        return Ok(());
    }
    let (css_width, css_height) = (input.width.unwrap_or(0.0) as f32, input.height.unwrap_or(0.0) as f32);
    let (x, y) = STATE.with(|s| {
        let s = s.borrow();
        let scale_x = if css_width > 0.0 { s.width / css_width } else { 1.0 };
        let scale_y = if css_height > 0.0 { s.height / css_height } else { 1.0 };
        (input.x.unwrap_or(0.0) as f32 * scale_x, input.y.unwrap_or(0.0) as f32 * scale_y)
    });
    let hit = STATE.with(|s| {
        s.borrow().columns.iter().position(|&(cx, cy, cw, ch)| x >= cx && x < cx + cw && y >= cy && y < cy + ch)
    });
    let Some(bar) = hit else {
        return Ok(());
    };
    let seq = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.seq += 1;
        s.seq
    });
    let message = match json!({ "bar": bar }) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    checked(pdk::send(CanvasSendInput { message, seq: Some(seq as i32) }).map_err(host_failed)?.error)
}

/// Draws one frame if the chart changed or is still growing. Asks for another frame while it grows.
fn draw(state: &mut State, t: f64) -> Result<(), Failed> {
    let from = *state.grow_from.get_or_insert(t);
    let progress = ((t - from) / GROW_MS).clamp(0.0, 1.0) as f32;
    if !state.dirty && progress >= 1.0 {
        return Ok(());
    }
    state.dirty = false;

    let (w, h) = (state.width, state.height);
    let mut c = Canvas2D::new();
    c.fill_color(BACKGROUND).fill_rect(0.0, 0.0, w, h);
    if state.bars.is_empty() {
        state.columns.clear();
        c.fill_color(INK)
            .font("Mono", 18.0, 400.0)
            .text_align(TextAlign::Center)
            .fill_text("No results yet", w / 2.0, h / 2.0);
    } else {
        state.columns = draw_bars(&mut c, &state.bars, w, h, progress);
    }
    if let Some((handle, image_w, image_h)) = state.image {
        // The pizza, in the top-left corner, over the caption's band.
        c.draw_image(
            handle,
            [0.0, 0.0, image_w as f32, image_h as f32],
            [6.0, 6.0, 24.0, 24.0],
        );
    }
    if let Some(caption) = &state.caption {
        c.fill_color(INK)
            .font("Mono", 16.0, 400.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Top)
            .fill_text(caption, w / 2.0, 4.0);
    }
    c.submit()?;
    if progress < 1.0 {
        request_frame()?;
    }
    Ok(())
}

/// Draws the bars and returns each bar's column (the whole height above it counts as the bar, for clicks).
fn draw_bars(
    c: &mut Canvas2D,
    bars: &[Bar],
    w: f32,
    h: f32,
    progress: f32,
) -> Vec<(f32, f32, f32, f32)> {
    let margin = 24.0;
    let label_band = 32.0;
    let area_h = (h - 2.0 * margin - label_band).max(1.0);
    let slot = ((w - 2.0 * margin) / bars.len() as f32).max(1.0);
    let bar_w = slot * 0.6;
    let top = bars.iter().map(|bar| bar.votes).max().unwrap_or(1).max(1) as f32;

    let label_size = (slot / 7.5).clamp(12.0, 18.0);
    c.font("Mono", label_size, 400.0)
        .text_align(TextAlign::Center)
        .text_baseline(TextBaseline::Alphabetic);
    let value_size = (label_size + 2.0).min(20.0);
    let mut columns = Vec::with_capacity(bars.len());
    for (i, bar) in bars.iter().enumerate() {
        let column_x = margin + i as f32 * slot;
        columns.push((column_x, 0.0, slot, h));
        let x = column_x + (slot - bar_w) / 2.0;
        let full = bar.votes as f32 / top * area_h;
        let height = full * progress;
        let y = h - margin - label_band - height;
        c.fill_color(BAR).fill_rect(x, y, bar_w, height);
        c.fill_color(INK)
            .font("Mono", value_size, 400.0)
            .fill_text(&bar.votes.to_string(), x + bar_w / 2.0, y - 4.0)
            .font("Mono", label_size, 400.0)
            .fill_text(&bar.label, x + bar_w / 2.0, h - margin);
    }
    columns
}
