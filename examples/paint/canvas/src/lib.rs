//! Place: a shared pixel board in the style of r/place. Tap a pixel, pick a color, place it, and wait out the
//! cooldown. The board arrives as 32-by-32 tiles: the render carries the most recently changed ones and the guest
//! fetches the rest one request at a time, paced and retried, so a busy board loads steadily instead of tripping the
//! host's rate limits. Each frame redraws only what changed, on device-pixel edges, as raw pixels or as runs,
//! whichever makes the smaller command buffer. The card can go fullscreen; the layout keeps its aspect ratio and
//! only gets sharper.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::OnceLock;

// The exports and events are the bindings `xtp plugin init` generates from `canvas.xtp.yaml` (see the README); host
// calls and drawing go through `chittr-cartridge`'s canvas module.

mod pdk;

use chittr_cartridge::canvas::{rgba, Canvas2D, FillRule, LineCap, LineJoin, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde::Serialize;
use serde_json::{json, Map, Value};

const LOGICAL_WIDTH: f32 = 960.0;
const LOGICAL_HEIGHT: f32 = 640.0;
const BOARD_WIDTH: usize = 512;
const BOARD_HEIGHT: usize = 512;
const TILE_SIDE: usize = 32;
const TILES_PER_ROW: usize = BOARD_WIDTH / TILE_SIDE;
const TILE_COUNT: usize = TILES_PER_ROW * (BOARD_HEIGHT / TILE_SIDE);
const TILE_PIXELS: usize = TILE_SIDE * TILE_SIDE;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 40.0;
const ZOOM_STEP: f32 = 1.5;
const KEY_PAN_STEP: f32 = 64.0;
/// How much of the board stays in view however far it is panned, in logical pixels.
const PAN_MARGIN: f32 = 64.0;
/// How far a press may wander, in logical pixels, and still count as a tap rather than a drag.
const TAP_SLOP: f32 = 6.0;
/// An unplaced pixel, shown white as r/place starts out.
const EMPTY: u8 = u8::MAX;
/// A pixel of a tile that has not loaded yet, shown as a soft gray.
const LOADING: u8 = u8::MAX - 1;
const COOLDOWN_SECONDS: f64 = 15.0;
/// Tiles one fetch asks for. The server answers with as many as fit in a reply.
const FETCH_BATCH: usize = 32;
/// The gap between fetches, about six a second: well inside the host's 20 messages a second, with room left for
/// placements and lookups.
const FETCH_INTERVAL_MS: f64 = 160.0;
/// The first wait after a failed fetch. It doubles with each failure in a row, up to the max.
const FETCH_RETRY_MS: f64 = 500.0;
const FETCH_RETRY_MAX_MS: f64 = 8000.0;
/// The most a frame's command buffer may hold. The host's cap is 4 MiB; the rest is headroom for the overlays.
const FRAME_BUDGET: usize = 3 * 1024 * 1024 + 512 * 1024;
/// Device pixels per cell from which the pixel grid shows.
const GRID_MIN_CELL: f32 = 10.0;
/// More dirty tiles than this are redrawn as one rectangle per row of tiles.
const DIRTY_TILE_RECTS: usize = 16;
const REMOTE_FLASH_MS: f64 = 900.0;
const LOCAL_FLASH_MS: f64 = 500.0;
const TOAST_MS: f64 = 2600.0;
/// Cells one flash lights individually; a bigger change only gets its ring.
const FLASH_CELLS: usize = 1500;
/// Frame rates: smooth while something animates, and just enough to tick the cooldown and pace loading otherwise.
const ANIMATION_FPS: u32 = 60;
const IDLE_FPS: u32 = 10;

/// The r/place 2022 palette, in its order.
const PALETTE: [u32; 32] = [
    0x6d001a, 0xbe0039, 0xff4500, 0xffa800, 0xffd635, 0xfff8b8, 0x00a368, 0x00cc78, 0x7eed56, 0x00756f, 0x009eaa,
    0x00ccc0, 0x2450a4, 0x3690ea, 0x51e9f4, 0x493ac1, 0x6a5cff, 0x94b3ff, 0x811e9f, 0xb44ac0, 0xe4abff, 0xde107f,
    0xff3881, 0xff99aa, 0x6d482f, 0x9c6926, 0xffb470, 0x000000, 0x515252, 0x898d90, 0xd4d7d9, 0xffffff,
];

// The interface's own colors: a dark surround and dark, slightly see-through chrome, so the board is the only bright
// thing on the card in either theme.
const SURROUND: u32 = 0x1c1c20ff;
const CHROME: u32 = 0x18181be8;
const CHROME_LINE: u32 = 0xffffff1f;
const INK: u32 = 0xf4f4f5ff;
const INK_MUTED: u32 = 0xa1a1aaff;
const ACCENT: u32 = 0xff4500ff;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Gesture {
    #[default]
    Idle,
    /// A press on the board: a tap selects a pixel, a drag pans.
    Press { start_x: f32, start_y: f32, last_x: f32, last_y: f32, dragged: bool },
    Pinch(Pinch),
}

/// A two-finger zoom: the fingers' starting spread and zoom, and the board point under their midpoint, which stays
/// under it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pinch {
    distance: f32,
    zoom: f32,
    board_x: f32,
    board_y: f32,
}

/// One placement, as a board index and a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    index: u32,
    color: u8,
}

/// A placement the server stored, kept shown until its tile arrives with at least this stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Acknowledged {
    placement: Placement,
    tile: usize,
    stamp: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlashKind {
    Local,
    Remote,
}

/// A short highlight over changed cells. It starts on the first frame that draws it.
#[derive(Debug, Clone, PartialEq)]
struct Flash {
    kind: FlashKind,
    cells: Vec<u32>,
    bounds: CellRect,
    start: Option<f64>,
}

/// Board cells, half-open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellRect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl CellRect {
    fn cell(index: usize) -> Self {
        let (x, y) = (index % BOARD_WIDTH, index / BOARD_WIDTH);
        Self { x0: x, y0: y, x1: x + 1, y1: y + 1 }
    }

    fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// Backing-store pixels, half-open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DevRect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl DevRect {
    fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    fn area(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            (self.x1 - self.x0) as usize * (self.y1 - self.y0) as usize
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    fn touches(&self, other: &Self) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }

    fn width(&self) -> f32 {
        (self.x1 - self.x0) as f32
    }

    fn height(&self) -> f32 {
        (self.y1 - self.y0) as f32
    }
}

/// What the last frame drew over the board, so the next frame redraws only what changed.
#[derive(Debug, Default, Clone, PartialEq)]
struct Drawn {
    coordinates: Option<String>,
    /// The admin's cooldown switch: `None` for everyone else, who never sees it.
    switch: Option<bool>,
    loading: Option<String>,
    panel: String,
    toast: Option<String>,
    marks: String,
    mark_rects: Vec<DevRect>,
    flashes: Vec<DevRect>,
}

#[derive(Default)]
struct State {
    pixels: Vec<u8>,
    /// The stamp of the server content each tile holds; `None` until it has some, or after a refused placement.
    tile_stamps: Vec<Option<u32>>,
    /// Each tile's stamp in the latest render.
    wanted: Vec<u32>,
    fetching: Option<(u32, Vec<usize>)>,
    next_fetch_at: f64,
    fetch_failures: u32,
    loaded: bool,
    unavailable: bool,
    can_place: bool,
    fullscreen: bool,
    reduced_motion: bool,
    /// Whether the render said this viewer is an admin, and whether they have switched the cooldown off.
    admin: bool,
    skip_cooldown: bool,
    color: Option<u8>,
    selection: Option<u32>,
    /// Who placed the selected pixel, once the server has said.
    selection_info: Option<String>,
    info_request: Option<u32>,
    info_sequence: Option<(u32, u32)>,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    width: f32,
    height: f32,
    /// Pointers that are down: id, position, and whether it is a touch.
    pointers: Vec<(i32, f32, f32, bool)>,
    gesture: Gesture,
    hover: Option<u32>,
    pending: VecDeque<Placement>,
    in_flight: Option<u32>,
    acknowledged: Vec<Acknowledged>,
    /// When the cooldown ends, in frame time; and a cooldown heard of before the first frame, in seconds.
    cooldown_until: f64,
    cooldown_pending: Option<f64>,
    toast: Option<(String, Option<f64>)>,
    sequence: u32,
    full_redraw: bool,
    dirty_tiles: Vec<Option<CellRect>>,
    truncated: bool,
    flashes: Vec<Flash>,
    drawn: Drawn,
    now: f64,
    fps: u32,
}

impl State {
    fn new(width: f32, height: f32) -> Self {
        Self {
            pixels: vec![EMPTY; BOARD_WIDTH * BOARD_HEIGHT],
            tile_stamps: vec![None; TILE_COUNT],
            wanted: vec![0; TILE_COUNT],
            dirty_tiles: vec![None; TILE_COUNT],
            zoom: fit_zoom(),
            width,
            height,
            full_redraw: true,
            ..Self::default()
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new(0.0, 0.0));
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    let state_value = input.state.clone().map(Value::Object).unwrap_or(Value::Null);
    let result = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        *state = State::new(input.width as f32, input.height as f32);
        state.reduced_motion = input.prefers_reduced_motion;
        state.fullscreen = input.fullscreen;
        load_shared_state(&mut state, &state_value);
        flush(&mut state);
        draw(&mut state)?;
        schedule(&mut state)
    });
    Ok(reply(result))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        tick(&mut state, input.t);
        flush(&mut state);
        draw(&mut state)?;
        schedule(&mut state)
    });
    Ok(reply(result))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let fields = fields_of(&input);
    let kind = code_of(&input.r#type);
    let result = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !handle_event(&mut state, &kind, &fields) {
            return Ok(());
        }
        flush(&mut state);
        request_frame()?;
        schedule(&mut state)
    });
    Ok(reply(result))
}

/// Moves the clock to frame time `t`: starts a cooldown heard of before the clock was known, and times out the toast.
fn tick(state: &mut State, t: f64) {
    state.now = t;
    if let Some(seconds) = state.cooldown_pending.take() {
        state.cooldown_until = t + seconds * 1000.0;
    }
    if let Some((_, start)) = &mut state.toast {
        let start = *start.get_or_insert(t);
        if t - start >= TOAST_MS {
            state.toast = None;
        }
    }
}

/// Whether anything needs frames to come without an event: a flash, the cooldown counting down, a toast, or tiles
/// still to fetch.
fn wants_frames(state: &State) -> bool {
    !state.flashes.is_empty()
        || cooling(state)
        || state.toast.is_some()
        || (state.loaded && tiles_left(state) > 0)
}

/// Asks for the next frame when one is needed, at the rate the moment calls for.
fn schedule(state: &mut State) -> Result<(), Failed> {
    if !wants_frames(state) {
        return Ok(());
    }
    let fps = if state.flashes.is_empty() { IDLE_FPS } else { ANIMATION_FPS };
    if state.fps != fps {
        chittr_cartridge::canvas::set_fps(fps).map_err(Failed::from)?;
        state.fps = fps;
    }
    request_frame()
}

/// Applies one event to the state. Returns false for an event the guest ignores. It makes no host calls; `flush`
/// sends what it queued.
fn handle_event(state: &mut State, kind: &str, fields: &Map<String, Value>) -> bool {
    match kind {
        "state" => load_shared_state(state, fields.get("state").unwrap_or(&Value::Null)),
        "resize" => {
            // Fullscreen keeps the aspect ratio, so the layout stays put and the view keeps its place; only the
            // backing store grows.
            state.width = number(fields, "width");
            state.height = number(fields, "height");
            state.fullscreen = fields.get("fullscreen").and_then(Value::as_bool).unwrap_or(state.fullscreen);
            state.full_redraw = true;
        }
        "pointer" => pointer(state, fields),
        "key" => key(state, fields),
        "blur" => state.hover = None,
        "reply" => {
            let sequence = fields.get("seq").and_then(Value::as_u64).map(|seq| seq as u32);
            if let Some(sequence) = sequence {
                handle_reply(state, sequence, fields.get("reply").unwrap_or(&Value::Null));
            }
        }
        "error" => {
            let code = fields.get("code").and_then(Value::as_str).unwrap_or("error");
            let sequence = fields.get("seq").and_then(Value::as_u64).map(|seq| seq as u32);
            handle_error(state, sequence, code);
        }
        _ => return false,
    }
    true
}

fn handle_reply(state: &mut State, sequence: u32, reply: &Value) {
    if state.info_sequence.is_some_and(|(seq, _)| seq == sequence) {
        let (_, index) = state.info_sequence.take().unwrap();
        if state.selection == Some(index) {
            state.selection_info = pixel_info_caption(reply);
        }
    } else if state.in_flight == Some(sequence) {
        state.in_flight = None;
        let wait = reply.get("wait").and_then(Value::as_f64);
        match reply.get("type").and_then(Value::as_str) {
            Some("placed") => {
                let stamp = reply
                    .get("stamps")
                    .and_then(|stamps| stamps.get(0))
                    .and_then(|pair| Some((pair.get(0)?.as_u64()? as usize, pair.get(1)?.as_u64()? as u32)))
                    .filter(|(tile, _)| *tile < TILE_COUNT);
                if let Some(placement) = state.pending.pop_front() {
                    if let Some((tile, stamp)) = stamp {
                        state.acknowledged.push(Acknowledged { placement, tile, stamp });
                        prune_acknowledged(state);
                    }
                    if state.selection == Some(placement.index) {
                        state.info_request = Some(placement.index);
                    }
                }
                start_cooldown(state, wait.unwrap_or(COOLDOWN_SECONDS));
            }
            Some("cooldown") => {
                take_back_placement(state);
                let wait = wait.unwrap_or(COOLDOWN_SECONDS);
                start_cooldown(state, wait);
                show_toast(state, &format!("You can place again in {}", clock_text(wait)));
            }
            _ => {
                // Unchanged: the pixel already had that color, and the server charged no cooldown.
                state.pending.pop_front();
                state.cooldown_until = 0.0;
                state.cooldown_pending = None;
                show_toast(state, "That pixel is already that color");
            }
        }
    } else if state.fetching.as_ref().is_some_and(|(seq, _)| *seq == sequence) {
        state.fetching = None;
        state.fetch_failures = 0;
        let tiles = reply
            .get("tiles")
            .and_then(Value::as_array)
            .map(|tiles| tiles.iter().filter_map(parse_fetched_tile).collect())
            .unwrap_or_default();
        apply_tiles(state, tiles, false);
    }
}

fn handle_error(state: &mut State, sequence: Option<u32>, code: &str) {
    match sequence {
        Some(sequence) if state.info_sequence.is_some_and(|(seq, _)| seq == sequence) => {
            state.info_sequence = None;
        }
        Some(sequence) if state.in_flight == Some(sequence) => {
            state.in_flight = None;
            take_back_placement(state);
            state.cooldown_until = 0.0;
            state.cooldown_pending = None;
            let message = match code {
                "logged_out" => "Log in to place pixels".to_string(),
                "slow_down" | "busy" => "The board is busy. Try again in a moment".to_string(),
                _ => format!("That pixel couldn't be placed ({code})"),
            };
            show_toast(state, &message);
        }
        Some(sequence) if state.fetching.as_ref().is_some_and(|(seq, _)| *seq == sequence) => {
            state.fetching = None;
            back_off(state);
        }
        _ => {}
    }
}

/// Waits longer after each failed fetch in a row, then tries again; loading never stops for good.
fn back_off(state: &mut State) {
    state.fetch_failures += 1;
    let wait = FETCH_RETRY_MS * 2f64.powi(state.fetch_failures.min(8) as i32 - 1);
    state.next_fetch_at = state.now + wait.min(FETCH_RETRY_MAX_MS);
}

fn pixel_info_caption(reply: &Value) -> Option<String> {
    if reply.get("type").and_then(Value::as_str) != Some("pixel_info") {
        return None;
    }
    match reply.get("trolltag").and_then(Value::as_str) {
        Some(trolltag) => Some(format!("@{trolltag}")),
        None if reply.get("touched").and_then(Value::as_bool) == Some(false) => Some("never placed".to_string()),
        None => None,
    }
}

fn number(fields: &Map<String, Value>, name: &str) -> f32 {
    fields.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn show_toast(state: &mut State, text: &str) {
    state.toast = Some((text.to_string(), None));
}

fn start_cooldown(state: &mut State, seconds: f64) {
    if seconds <= 0.0 {
        state.cooldown_until = 0.0;
        state.cooldown_pending = None;
    } else if state.now > 0.0 {
        state.cooldown_until = state.now + seconds * 1000.0;
    } else {
        // No frame has given the clock yet; the first one starts it.
        state.cooldown_pending = Some(seconds);
    }
}

fn cooling(state: &State) -> bool {
    state.cooldown_pending.is_some() || state.cooldown_until > state.now
}

fn cooldown_seconds(state: &State) -> f64 {
    state
        .cooldown_pending
        .unwrap_or(((state.cooldown_until - state.now) / 1000.0).max(0.0))
}

/// `m:ss`, rounded up so it never shows 0:00 while still waiting.
fn clock_text(seconds: f64) -> String {
    let seconds = seconds.ceil() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

// Shared state and tiles.

/// Takes a render's state: every tile's stamp, the data of the tiles it had room for, and this Observer's cooldown.
/// Tiles that changed but came without data are fetched by `flush`.
fn load_shared_state(state: &mut State, value: &Value) {
    let stamps = value
        .get("stamps")
        .and_then(Value::as_str)
        .and_then(decode_base64)
        .filter(|bytes| bytes.len() == TILE_COUNT * 4);
    let Some(stamps) = stamps else {
        state.can_place = false;
        state.unavailable = true;
        return;
    };
    state.unavailable = false;
    state.wanted = stamps
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    state.can_place = value.get("canPlace").and_then(Value::as_bool).unwrap_or(false);
    state.admin = value.get("admin").and_then(Value::as_bool) == Some(true);
    state.skip_cooldown &= state.admin;
    if state.in_flight.is_none() {
        if let Some(seconds) = value.get("cooldown").and_then(Value::as_f64) {
            start_cooldown(state, seconds);
        }
    }
    // A fresh render is a fresh chance: retry at once whatever failed before.
    state.fetch_failures = 0;
    state.next_fetch_at = 0.0;
    let mut incoming: Vec<(usize, u32, Vec<u8>)> = Vec::new();
    for entry in value.get("tiles").and_then(Value::as_array).into_iter().flatten() {
        let tile = entry
            .get(0)
            .and_then(Value::as_u64)
            .filter(|tile| *tile < TILE_COUNT as u64)
            .map(|tile| tile as usize);
        let pixels = entry
            .get(1)
            .and_then(Value::as_str)
            .and_then(decode_base64)
            .and_then(|data| decode_tile(&data));
        if let (Some(tile), Some(pixels)) = (tile, pixels) {
            incoming.push((tile, state.wanted[tile], pixels));
        }
    }
    let first_load = !state.loaded;
    state.loaded = true;
    if first_load {
        // Nothing is on screen yet, so none of this is a change worth flashing. Tiles still to come show as loading.
        for tile in 0..TILE_COUNT {
            if state.wanted[tile] == 0 {
                state.tile_stamps[tile] = Some(0);
            } else {
                write_tile(&mut state.pixels, tile, &[LOADING; TILE_PIXELS]);
            }
        }
        state.full_redraw = true;
    } else {
        // A tile the render reports unplaced is empty, whatever the guest last held for it.
        let cleared: Vec<(usize, u32, Vec<u8>)> = (0..TILE_COUNT)
            .filter(|tile| state.wanted[*tile] == 0 && state.tile_stamps[*tile] != Some(0))
            .map(|tile| (tile, 0, vec![EMPTY; TILE_PIXELS]))
            .collect();
        apply_tiles(state, cleared, true);
    }
    apply_tiles(state, incoming, false);
}

fn parse_fetched_tile(entry: &Value) -> Option<(usize, u32, Vec<u8>)> {
    let tile = entry.get(0)?.as_u64().filter(|tile| *tile < TILE_COUNT as u64)? as usize;
    let stamp = entry.get(1)?.as_u64()? as u32;
    let pixels = decode_tile(&decode_base64(entry.get(2)?.as_str()?)?)?;
    Some((tile, stamp, pixels))
}

/// Whether server content with `stamp` should replace what tile `tile` holds: it is newer than that, or at least as
/// new as the latest render reported.
fn accepts(state: &State, tile: usize, stamp: u32) -> bool {
    let held = state.tile_stamps[tile];
    held != Some(stamp) && (held.is_none_or(|held| stamp > held) || stamp >= state.wanted[tile])
}

fn tile_needs_fetch(state: &State, tile: usize) -> bool {
    state.wanted[tile] != 0 && state.tile_stamps[tile].is_none_or(|held| held < state.wanted[tile])
}

fn tiles_left(state: &State) -> usize {
    (0..TILE_COUNT).filter(|tile| tile_needs_fetch(state, *tile)).count()
}

/// Puts server tiles on the board, shows the guest's unconfirmed placement over them, and flashes whatever that
/// changed on a tile the guest had already shown. `force` takes the tiles whatever their stamps.
fn apply_tiles(state: &mut State, tiles: Vec<(usize, u32, Vec<u8>)>, force: bool) {
    let mut affected = vec![false; TILE_COUNT];
    let mut before = Vec::new();
    for (tile, stamp, pixels) in tiles {
        if !force && !accepts(state, tile, stamp) {
            continue;
        }
        let shown = state.tile_stamps[tile].is_some();
        before.push((tile, read_tile(&state.pixels, tile), shown));
        write_tile(&mut state.pixels, tile, &pixels);
        state.tile_stamps[tile] = Some(stamp);
        affected[tile] = true;
    }
    if before.is_empty() {
        return;
    }
    prune_acknowledged(state);
    let unconfirmed: Vec<Placement> = state
        .acknowledged
        .iter()
        .map(|acknowledged| acknowledged.placement)
        .chain(state.pending.iter().copied())
        .collect();
    for placement in unconfirmed {
        if affected[tile_of_index(placement.index as usize)] {
            state.pixels[placement.index as usize] = placement.color;
        }
    }
    for (tile, old, shown) in before {
        let now = read_tile(&state.pixels, tile);
        let changed: Vec<u32> = (0..TILE_PIXELS)
            .filter(|offset| old[*offset] != now[*offset])
            .map(|offset| tile_index(tile, offset) as u32)
            .collect();
        for index in &changed {
            mark_dirty(state, *index as usize);
        }
        if let Some(selection) = state.selection.filter(|selection| changed.contains(selection)) {
            state.info_request = Some(selection);
        }
        if shown && !changed.is_empty() {
            add_flash(state, FlashKind::Remote, &changed);
        }
    }
}

fn tile_index(tile: usize, offset: usize) -> usize {
    let (tile_x, tile_y) = (tile % TILES_PER_ROW, tile / TILES_PER_ROW);
    (tile_y * TILE_SIDE + offset / TILE_SIDE) * BOARD_WIDTH + tile_x * TILE_SIDE + offset % TILE_SIDE
}

fn tile_of_index(index: usize) -> usize {
    (index / BOARD_WIDTH / TILE_SIDE) * TILES_PER_ROW + (index % BOARD_WIDTH) / TILE_SIDE
}

fn read_tile(pixels: &[u8], tile: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(TILE_PIXELS);
    for row in 0..TILE_SIDE {
        let start = tile_index(tile, row * TILE_SIDE);
        out.extend_from_slice(&pixels[start..start + TILE_SIDE]);
    }
    out
}

fn write_tile(pixels: &mut [u8], tile: usize, data: &[u8]) {
    for row in 0..TILE_SIDE {
        let start = tile_index(tile, row * TILE_SIDE);
        pixels[start..start + TILE_SIDE].copy_from_slice(&data[row * TILE_SIDE..(row + 1) * TILE_SIDE]);
    }
}

/// Drops the stored placements whose tiles have arrived with the stamps the server wrote them at.
fn prune_acknowledged(state: &mut State) {
    let stamps = &state.tile_stamps;
    state
        .acknowledged
        .retain(|acknowledged| !stamps[acknowledged.tile].is_some_and(|held| held >= acknowledged.stamp));
}

/// The tiles to fetch next, nearest the middle of the view first.
fn next_fetch(state: &State) -> Vec<usize> {
    let (left, top) = board_origin(state);
    let (center_x, center_y) = view_center();
    let center_x = (center_x - left) / state.zoom;
    let center_y = (center_y - top) / state.zoom;
    let mut tiles: Vec<usize> = (0..TILE_COUNT).filter(|tile| tile_needs_fetch(state, *tile)).collect();
    let distance = |tile: &usize| {
        let x = ((tile % TILES_PER_ROW) * TILE_SIDE + TILE_SIDE / 2) as f32 - center_x;
        let y = ((tile / TILES_PER_ROW) * TILE_SIDE + TILE_SIDE / 2) as f32 - center_y;
        (x * x + y * y) as u32
    };
    tiles.sort_by_key(distance);
    tiles.truncate(FETCH_BATCH);
    tiles
}

/// Sends what the state has queued: a pixel lookup, the placement, and the next tile fetch once its time has come.
/// Only one fetch is ever in flight, and fetches are spaced out, so loading a busy board never floods the host.
fn flush(state: &mut State) {
    if let Some(index) = state.info_request.take() {
        let (x, y) = (index as usize % BOARD_WIDTH, index as usize / BOARD_WIDTH);
        let seq = next_sequence(state);
        if send(json!({ "type": "info", "x": x, "y": y }), seq).is_ok() {
            state.info_sequence = Some((seq, index));
        }
    }
    if state.in_flight.is_none() {
        if let Some(placement) = state.pending.front().copied() {
            let index = placement.index as usize;
            let mut message = json!({
                "type": "place", "x": index % BOARD_WIDTH, "y": index / BOARD_WIDTH, "color": placement.color,
            });
            if skipping_cooldown(state) {
                message["skipCooldown"] = json!(true);
            }
            let seq = next_sequence(state);
            match send(message, seq) {
                Ok(()) => state.in_flight = Some(seq),
                Err(error) if error.code == "busy" => {}
                Err(error) => {
                    take_back_placement(state);
                    state.cooldown_until = 0.0;
                    state.cooldown_pending = None;
                    show_toast(state, &format!("That pixel couldn't be placed ({})", error.code));
                }
            }
        }
    }
    if state.fetching.is_none() && state.now >= state.next_fetch_at {
        let ids = next_fetch(state);
        if !ids.is_empty() {
            let seq = next_sequence(state);
            match send(json!({ "type": "tiles", "ids": ids }), seq) {
                Ok(()) => {
                    state.fetching = Some((seq, ids));
                    state.next_fetch_at = state.now + FETCH_INTERVAL_MS;
                }
                Err(_) => back_off(state),
            }
        }
    }
}

fn next_sequence(state: &mut State) -> u32 {
    let seq = state.sequence;
    state.sequence = state.sequence.wrapping_add(1);
    seq
}

/// Takes back the placement at the front of the queue: its tile is fetched again, or emptied when the server has
/// nothing there.
fn take_back_placement(state: &mut State) {
    let Some(placement) = state.pending.pop_front() else { return };
    let tile = tile_of_index(placement.index as usize);
    if state.wanted[tile] == 0 {
        apply_tiles(state, vec![(tile, 0, vec![EMPTY; TILE_PIXELS])], true);
    } else {
        state.tile_stamps[tile] = None;
        state.next_fetch_at = 0.0;
    }
}

// Encodings shared with the Server Guest.

/// A tile's wire data: the 1,024 raw pixels, or runs of `(gap, length, color)` with varint gaps and lengths that skip
/// unplaced pixels. Runs are always shorter than 1,024 bytes, so the length tells the two apart.
fn decode_tile(data: &[u8]) -> Option<Vec<u8>> {
    let colors = PALETTE.len() as u8;
    if data.len() == TILE_PIXELS {
        return data
            .iter()
            .all(|pixel| *pixel == EMPTY || *pixel < colors)
            .then(|| data.to_vec());
    }
    let mut pixels = vec![EMPTY; TILE_PIXELS];
    let (mut cursor, mut previous_end) = (0, 0usize);
    while cursor < data.len() {
        let gap = read_varint(data, &mut cursor)? as usize;
        let run = read_varint(data, &mut cursor)? as usize;
        let color = *data.get(cursor)?;
        cursor += 1;
        let start = previous_end.checked_add(gap)?;
        let end = start.checked_add(run)?;
        if run == 0 || end > TILE_PIXELS || color >= colors {
            return None;
        }
        pixels[start..end].fill(color);
        previous_end = end;
    }
    Some(pixels)
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let mut value = 0u32;
    for shift in (0..=28).step_by(7) {
        let byte = *bytes.get(*cursor)?;
        *cursor += 1;
        if shift == 28 && byte & 0x70 != 0 {
            return None;
        }
        value |= u32::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn decode_base64(encoded: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(encoded.len() * 3 / 4);
    let (mut bits, mut bit_count) = (0u32, 0);
    for byte in encoded.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            bytes.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }
    if bit_count >= 6 || bits != 0 {
        return None;
    }
    Some(bytes)
}

// The view.

/// The bottom of the area the board fits and centers in: above the palette bar.
const VIEW_BOTTOM: f32 = PANEL_TOP - 8.0;

/// The zoom that fits the whole board above the palette bar, with a little room around it.
fn fit_zoom() -> f32 {
    ((LOGICAL_WIDTH - 48.0) / BOARD_WIDTH as f32).min((VIEW_BOTTOM - 40.0) / BOARD_HEIGHT as f32)
}

fn view_center() -> (f32, f32) {
    (LOGICAL_WIDTH / 2.0, VIEW_BOTTOM / 2.0)
}

/// Where the board's top-left corner sits with no pan, in logical pixels: centered above the palette bar.
fn centered_origin(zoom: f32) -> (f32, f32) {
    let (center_x, center_y) = view_center();
    (center_x - BOARD_WIDTH as f32 * zoom / 2.0, center_y - BOARD_HEIGHT as f32 * zoom / 2.0)
}

fn board_origin(state: &State) -> (f32, f32) {
    let (left, top) = centered_origin(state.zoom);
    (left + state.pan_x, top + state.pan_y)
}

fn board_index(state: &State, x: f32, y: f32) -> Option<usize> {
    let (left, top) = board_origin(state);
    let board_x = ((x - left) / state.zoom).floor();
    let board_y = ((y - top) / state.zoom).floor();
    if board_x < 0.0 || board_y < 0.0 || board_x >= BOARD_WIDTH as f32 || board_y >= BOARD_HEIGHT as f32 {
        return None;
    }
    Some(board_y as usize * BOARD_WIDTH + board_x as usize)
}

/// Keeps at least a margin of the board in view.
fn clamp_pan(state: &mut State) {
    let (left, top) = centered_origin(state.zoom);
    let size_x = BOARD_WIDTH as f32 * state.zoom;
    let size_y = BOARD_HEIGHT as f32 * state.zoom;
    let min_x = PAN_MARGIN - size_x - left;
    let max_x = LOGICAL_WIDTH - PAN_MARGIN - left;
    let min_y = PAN_MARGIN - size_y - top;
    let max_y = LOGICAL_HEIGHT - PAN_MARGIN - top;
    state.pan_x = state.pan_x.clamp(min_x.min(max_x), max_x.max(min_x));
    state.pan_y = state.pan_y.clamp(min_y.min(max_y), max_y.max(min_y));
}

/// Zooms so the board point under (`x`, `y`) stays put.
fn zoom_at(state: &mut State, zoom: f32, x: f32, y: f32) {
    let (left, top) = board_origin(state);
    let board_x = (x - left) / state.zoom;
    let board_y = (y - top) / state.zoom;
    state.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    place_board_point(state, board_x, board_y, x, y);
}

/// Pans so board point (`board_x`, `board_y`) is at (`x`, `y`).
fn place_board_point(state: &mut State, board_x: f32, board_y: f32, x: f32, y: f32) {
    let (left, top) = centered_origin(state.zoom);
    state.pan_x = x - board_x * state.zoom - left;
    state.pan_y = y - board_y * state.zoom - top;
    clamp_pan(state);
    state.full_redraw = true;
}

/// The point zoom keeps still: the selected pixel, the hovered one, or the middle of the view.
fn zoom_focus(state: &State) -> (f32, f32) {
    match state.selection.or(state.hover) {
        Some(index) => {
            let (left, top) = board_origin(state);
            let x = (index as usize % BOARD_WIDTH) as f32 + 0.5;
            let y = (index as usize / BOARD_WIDTH) as f32 + 0.5;
            (left + x * state.zoom, top + y * state.zoom)
        }
        None => view_center(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoomAction {
    In,
    Out,
    Fit,
}

fn apply_zoom(state: &mut State, action: ZoomAction) {
    let (x, y) = zoom_focus(state);
    match action {
        ZoomAction::In => zoom_at(state, state.zoom * ZOOM_STEP, x, y),
        ZoomAction::Out => zoom_at(state, state.zoom / ZOOM_STEP, x, y),
        ZoomAction::Fit => {
            state.zoom = fit_zoom();
            state.pan_x = 0.0;
            state.pan_y = 0.0;
            state.full_redraw = true;
        }
    }
}

// The chrome's layout, in logical pixels.

const MARGIN: f32 = 14.0;
const PILL_HEIGHT: f32 = 32.0;
const ZOOM_BUTTON: f32 = 40.0;
/// Bottom right, level with the palette bar: the host draws its fullscreen button in the top right corner.
const ZOOM_RECT: (f32, f32, f32, f32) = (
    LOGICAL_WIDTH - MARGIN - ZOOM_BUTTON * 3.0,
    PANEL_TOP + (PANEL_HEIGHT - PILL_HEIGHT) / 2.0,
    ZOOM_BUTTON * 3.0,
    PILL_HEIGHT,
);
/// Bottom left, level with the palette bar, and only ever drawn for admins.
const SWITCH_RECT: (f32, f32, f32, f32) =
    (MARGIN, PANEL_TOP + (PANEL_HEIGHT - PILL_HEIGHT) / 2.0, 150.0, PILL_HEIGHT);
const COORDINATES_RECT: (f32, f32, f32, f32) = (MARGIN, MARGIN, 300.0, PILL_HEIGHT);
const LOADING_RECT: (f32, f32, f32, f32) = (LOGICAL_WIDTH / 2.0 - 110.0, MARGIN, 220.0, PILL_HEIGHT);
const SWATCH: f32 = 22.0;
const SWATCH_PITCH: f32 = 26.0;
const SWATCH_COLUMNS: usize = 16;
const PANEL_PADDING: f32 = 12.0;
const PLACE_WIDTH: f32 = 96.0;
const PANEL_WIDTH: f32 =
    PANEL_PADDING * 3.0 + SWATCH_PITCH * SWATCH_COLUMNS as f32 - (SWATCH_PITCH - SWATCH) + PLACE_WIDTH;
const PANEL_HEIGHT: f32 = PANEL_PADDING * 2.0 + SWATCH_PITCH + SWATCH;
const PANEL_LEFT: f32 = (LOGICAL_WIDTH - PANEL_WIDTH) / 2.0;
const PANEL_TOP: f32 = LOGICAL_HEIGHT - MARGIN - PANEL_HEIGHT;
const PLACE_RECT: (f32, f32, f32, f32) = (
    PANEL_LEFT + PANEL_WIDTH - PANEL_PADDING - PLACE_WIDTH,
    PANEL_TOP + PANEL_PADDING,
    PLACE_WIDTH,
    PANEL_HEIGHT - PANEL_PADDING * 2.0,
);
const TOAST_RECT: (f32, f32, f32, f32) = (LOGICAL_WIDTH / 2.0 - 220.0, PANEL_TOP - 12.0 - PILL_HEIGHT, 440.0, PILL_HEIGHT);

fn inside((left, top, width, height): (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    (left..left + width).contains(&x) && (top..top + height).contains(&y)
}

fn swatch_origin(color: usize) -> (f32, f32) {
    (
        PANEL_LEFT + PANEL_PADDING + (color % SWATCH_COLUMNS) as f32 * SWATCH_PITCH,
        PANEL_TOP + PANEL_PADDING + (color / SWATCH_COLUMNS) as f32 * SWATCH_PITCH,
    )
}

fn swatch_hit(x: f32, y: f32) -> Option<u8> {
    let column = ((x - PANEL_LEFT - PANEL_PADDING) / SWATCH_PITCH).floor();
    let row = ((y - PANEL_TOP - PANEL_PADDING) / SWATCH_PITCH).floor();
    if column < 0.0 || row < 0.0 || column >= SWATCH_COLUMNS as f32 || row >= 2.0 {
        return None;
    }
    let color = row as usize * SWATCH_COLUMNS + column as usize;
    let (left, top) = swatch_origin(color);
    (x < left + SWATCH && y < top + SWATCH).then_some(color as u8)
}

fn zoom_hit(x: f32, y: f32) -> Option<ZoomAction> {
    if !inside(ZOOM_RECT, x, y) {
        return None;
    }
    Some(match ((x - ZOOM_RECT.0) / ZOOM_BUTTON) as usize {
        0 => ZoomAction::Out,
        1 => ZoomAction::In,
        _ => ZoomAction::Fit,
    })
}

fn panel_rect() -> (f32, f32, f32, f32) {
    (PANEL_LEFT, PANEL_TOP, PANEL_WIDTH, PANEL_HEIGHT)
}

// Input.

fn key(state: &mut State, fields: &Map<String, Value>) {
    if fields.get("action").and_then(Value::as_str) != Some("down") {
        return;
    }
    let key = fields.get("key").and_then(Value::as_str).unwrap_or("");
    let nudge = match key {
        "ArrowLeft" => Some((-1, 0)),
        "ArrowRight" => Some((1, 0)),
        "ArrowUp" => Some((0, -1)),
        "ArrowDown" => Some((0, 1)),
        _ => None,
    };
    if let Some((dx, dy)) = nudge {
        // Arrows move the selection a pixel at a time; with nothing selected they pan, as W/A/S/D always do.
        match state.selection {
            Some(index) => {
                let x = (index as usize % BOARD_WIDTH) as i32 + dx;
                let y = (index as usize / BOARD_WIDTH) as i32 + dy;
                if (0..BOARD_WIDTH as i32).contains(&x) && (0..BOARD_HEIGHT as i32).contains(&y) {
                    select(state, Some(y as usize * BOARD_WIDTH + x as usize));
                }
            }
            None => pan_by(state, -dx as f32 * KEY_PAN_STEP, -dy as f32 * KEY_PAN_STEP),
        }
        return;
    }
    match key {
        "a" | "A" => pan_by(state, KEY_PAN_STEP, 0.0),
        "d" | "D" => pan_by(state, -KEY_PAN_STEP, 0.0),
        "w" | "W" => pan_by(state, 0.0, KEY_PAN_STEP),
        "s" | "S" => pan_by(state, 0.0, -KEY_PAN_STEP),
        "+" | "=" => apply_zoom(state, ZoomAction::In),
        "-" | "_" => apply_zoom(state, ZoomAction::Out),
        "0" => apply_zoom(state, ZoomAction::Fit),
        "Enter" | " " => place(state),
        "q" | "Q" => cycle_color(state, PALETTE.len() - 1),
        "e" | "E" => cycle_color(state, 1),
        "Backspace" | "Delete" => select(state, None),
        _ => {}
    }
}

fn pan_by(state: &mut State, x: f32, y: f32) {
    state.pan_x += x;
    state.pan_y += y;
    clamp_pan(state);
    state.full_redraw = true;
}

fn cycle_color(state: &mut State, step: usize) {
    let next = state.color.map_or(0, |color| (color as usize + step) % PALETTE.len());
    state.color = Some(next as u8);
}

fn pointer(state: &mut State, fields: &Map<String, Value>) {
    let action = fields.get("action").and_then(Value::as_str).unwrap_or("");
    let (css_width, css_height) = (number(fields, "width"), number(fields, "height"));
    if css_width <= 0.0 || css_height <= 0.0 {
        return;
    }
    let x = number(fields, "x") * LOGICAL_WIDTH / css_width;
    let y = number(fields, "y") * LOGICAL_HEIGHT / css_height;
    let id = fields.get("pointerId").and_then(Value::as_i64).unwrap_or(0) as i32;
    let touch = fields.get("pointerType").and_then(Value::as_str) == Some("touch");
    match action {
        "down" => {
            // Only touches pinch. Anything else starts afresh, so a release the guest never heard of can't leave a
            // phantom second finger behind.
            state.pointers.retain(|pointer| touch && pointer.3 && pointer.0 != id);
            state.pointers.push((id, x, y, touch));
            if state.pointers.len() >= 2 {
                start_pinch(state);
            } else {
                pointer_down(state, x, y);
            }
        }
        "move" => {
            if let Some(pointer) = state.pointers.iter_mut().find(|pointer| pointer.0 == id) {
                (pointer.1, pointer.2) = (x, y);
            }
            match state.gesture {
                Gesture::Pinch(pinch) => update_pinch(state, pinch),
                Gesture::Press { start_x, start_y, last_x, last_y, dragged } => {
                    // The position only moves on once it is a drag, so the drag's first step covers the slop too.
                    if dragged || (x - start_x).hypot(y - start_y) > TAP_SLOP {
                        pan_by(state, x - last_x, y - last_y);
                        state.hover = None;
                        state.gesture = Gesture::Press { start_x, start_y, last_x: x, last_y: y, dragged: true };
                    }
                }
                Gesture::Idle => {
                    state.hover = if touch || over_chrome(state, x, y) {
                        None
                    } else {
                        board_index(state, x, y).map(|index| index as u32)
                    };
                }
            }
        }
        "up" | "cancel" => {
            state.pointers.retain(|pointer| pointer.0 != id);
            match state.gesture {
                Gesture::Press { dragged: false, .. } if action == "up" => {
                    // A tap off the board clears the selection.
                    select(state, board_index(state, x, y));
                }
                Gesture::Pinch(_) if !state.pointers.is_empty() => return,
                _ => {}
            }
            state.gesture = Gesture::Idle;
        }
        _ => {}
    }
}

/// Whether (`x`, `y`) is over the chrome, which takes the press instead of the board.
fn over_chrome(state: &State, x: f32, y: f32) -> bool {
    inside(ZOOM_RECT, x, y)
        || state.admin && inside(SWITCH_RECT, x, y)
        || inside(panel_rect(), x, y) && (state.can_place || state.loaded)
}

fn pointer_down(state: &mut State, x: f32, y: f32) {
    state.gesture = Gesture::Idle;
    if let Some(action) = zoom_hit(x, y) {
        apply_zoom(state, action);
        return;
    }
    if state.admin && inside(SWITCH_RECT, x, y) {
        toggle_cooldown(state);
        return;
    }
    if inside(panel_rect(), x, y) {
        if let Some(color) = swatch_hit(x, y).filter(|_| state.can_place) {
            state.color = Some(color);
        } else if inside(PLACE_RECT, x, y) {
            place(state);
        }
        return;
    }
    state.gesture = Gesture::Press { start_x: x, start_y: y, last_x: x, last_y: y, dragged: false };
}

fn select(state: &mut State, index: Option<usize>) {
    let index = index.map(|index| index as u32);
    if state.selection == index {
        return;
    }
    state.selection = index;
    state.selection_info = None;
    state.info_request = index;
}

fn start_pinch(state: &mut State) {
    let ((_, ax, ay, _), (_, bx, by, _)) = (state.pointers[0], state.pointers[1]);
    let (mid_x, mid_y) = ((ax + bx) / 2.0, (ay + by) / 2.0);
    let (left, top) = board_origin(state);
    state.gesture = Gesture::Pinch(Pinch {
        distance: (ax - bx).hypot(ay - by).max(1.0),
        zoom: state.zoom,
        board_x: (mid_x - left) / state.zoom,
        board_y: (mid_y - top) / state.zoom,
    });
    state.hover = None;
}

fn update_pinch(state: &mut State, pinch: Pinch) {
    if state.pointers.len() < 2 {
        return;
    }
    let ((_, ax, ay, _), (_, bx, by, _)) = (state.pointers[0], state.pointers[1]);
    let distance = (ax - bx).hypot(ay - by).max(1.0);
    state.zoom = (pinch.zoom * distance / pinch.distance).clamp(MIN_ZOOM, MAX_ZOOM);
    place_board_point(state, pinch.board_x, pinch.board_y, (ax + bx) / 2.0, (ay + by) / 2.0);
}

/// Why Place can't go ahead right now, or `None` when it can.
fn place_blocker(state: &State) -> Option<&'static str> {
    if !state.can_place {
        Some("Log in to place pixels")
    } else if !skipping_cooldown(state) && (cooling(state) || !state.pending.is_empty()) {
        Some("Wait for your cooldown")
    } else if state.selection.is_none() {
        Some("Tap a pixel first")
    } else if state.color.is_none() {
        Some("Pick a color first")
    } else {
        None
    }
}

/// Whether this viewer is an admin with the cooldown switched off. The server checks for itself that they are an admin.
fn skipping_cooldown(state: &State) -> bool {
    state.admin && state.skip_cooldown
}

fn toggle_cooldown(state: &mut State) {
    state.skip_cooldown = !state.skip_cooldown;
    show_toast(state, if state.skip_cooldown { "Cooldown off for you (admin)" } else { "Cooldown back on" });
}

/// Places the chosen color on the selected pixel: shown at once, sent, and taken back if the server says no.
fn place(state: &mut State) {
    if let Some(reason) = place_blocker(state) {
        show_toast(state, reason);
        return;
    }
    let (Some(index), Some(color)) = (state.selection, state.color) else { return };
    if state.pixels[index as usize] == color {
        show_toast(state, "That pixel is already that color");
        return;
    }
    set_pixel(state, index as usize, color);
    state.pending.push_back(Placement { index, color });
    if !skipping_cooldown(state) {
        start_cooldown(state, COOLDOWN_SECONDS);
    }
    add_flash(state, FlashKind::Local, &[index]);
}

fn set_pixel(state: &mut State, index: usize, color: u8) -> bool {
    match state.pixels.get(index) {
        Some(pixel) if *pixel != color => {
            state.pixels[index] = color;
            mark_dirty(state, index);
            true
        }
        _ => false,
    }
}

fn mark_dirty(state: &mut State, index: usize) {
    let tile = tile_of_index(index);
    let cell = CellRect::cell(index);
    state.dirty_tiles[tile] = Some(state.dirty_tiles[tile].map_or(cell, |rect| rect.union(cell)));
}

fn add_flash(state: &mut State, kind: FlashKind, cells: &[u32]) {
    if state.reduced_motion || cells.is_empty() {
        return;
    }
    let bounds = cells
        .iter()
        .map(|cell| CellRect::cell(*cell as usize))
        .reduce(CellRect::union)
        .unwrap();
    // Cells changed before the next frame join the flash that frame starts.
    if let Some(flash) = state
        .flashes
        .last_mut()
        .filter(|flash| flash.kind == kind && flash.start.is_none())
    {
        flash.bounds = flash.bounds.union(bounds);
        flash.cells.extend_from_slice(cells);
        return;
    }
    state.flashes.push(Flash { kind, cells: cells.to_vec(), bounds, start: None });
}

// Drawing.

/// How logical pixels and board cells land on the backing store. Cell edges are rounded to whole device pixels, so
/// neighbouring cells meet exactly, whether drawn as raw pixels or as rectangles.
#[derive(Debug, Clone, Copy)]
struct View {
    scale_x: f32,
    scale_y: f32,
    left: f32,
    top: f32,
    zoom: f32,
    width: i32,
    height: i32,
}

impl View {
    fn of(state: &State) -> Self {
        let (left, top) = board_origin(state);
        Self {
            scale_x: state.width / LOGICAL_WIDTH,
            scale_y: state.height / LOGICAL_HEIGHT,
            left,
            top,
            zoom: state.zoom,
            width: state.width.round() as i32,
            height: state.height.round() as i32,
        }
    }

    /// The device column where board column `x` starts.
    fn edge_x(&self, x: usize) -> i32 {
        ((self.left + x as f32 * self.zoom) * self.scale_x).round() as i32
    }

    fn edge_y(&self, y: usize) -> i32 {
        ((self.top + y as f32 * self.zoom) * self.scale_y).round() as i32
    }

    /// The board column device column `px` falls in, or `None` off the board.
    fn cell_x(&self, px: i32) -> Option<usize> {
        Self::cell_at(px, |x| self.edge_x(x), ((px as f32 + 0.5) / self.scale_x - self.left) / self.zoom, BOARD_WIDTH)
    }

    fn cell_y(&self, py: i32) -> Option<usize> {
        Self::cell_at(py, |y| self.edge_y(y), ((py as f32 + 0.5) / self.scale_y - self.top) / self.zoom, BOARD_HEIGHT)
    }

    fn cell_at(p: i32, edge: impl Fn(usize) -> i32, guess: f32, cells: usize) -> Option<usize> {
        if p < edge(0) || p >= edge(cells) {
            return None;
        }
        let mut cell = (guess.floor().max(0.0) as usize).min(cells - 1);
        while cell > 0 && edge(cell) > p {
            cell -= 1;
        }
        while cell + 1 < cells && edge(cell + 1) <= p {
            cell += 1;
        }
        Some(cell)
    }

    fn cell_device_size(&self) -> f32 {
        self.zoom * self.scale_x.min(self.scale_y)
    }

    fn canvas(&self) -> DevRect {
        DevRect { x0: 0, y0: 0, x1: self.width, y1: self.height }
    }

    fn board(&self) -> DevRect {
        DevRect {
            x0: self.edge_x(0),
            y0: self.edge_y(0),
            x1: self.edge_x(BOARD_WIDTH),
            y1: self.edge_y(BOARD_HEIGHT),
        }
    }

    /// A logical rectangle on the backing store, grown by a couple of pixels for antialiased edges.
    fn logical(&self, (x, y, width, height): (f32, f32, f32, f32)) -> DevRect {
        DevRect {
            x0: (x * self.scale_x).floor() as i32 - 2,
            y0: (y * self.scale_y).floor() as i32 - 2,
            x1: ((x + width) * self.scale_x).ceil() as i32 + 2,
            y1: ((y + height) * self.scale_y).ceil() as i32 + 2,
        }
    }

    /// Board cells on the backing store, grown by `margin` device pixels.
    fn cells(&self, rect: CellRect, margin: i32) -> DevRect {
        DevRect {
            x0: self.edge_x(rect.x0) - margin,
            y0: self.edge_y(rect.y0) - margin,
            x1: self.edge_x(rect.x1) + margin,
            y1: self.edge_y(rect.y1) + margin,
        }
        .intersect(self.canvas())
    }

    /// The cell at `index` on the backing store, at least `min` device pixels a side, centered on the cell.
    fn cell_box(&self, index: u32, min: f32) -> (f32, f32, f32, f32) {
        let (x, y) = (index as usize % BOARD_WIDTH, index as usize / BOARD_WIDTH);
        let (left, top) = (self.edge_x(x) as f32, self.edge_y(y) as f32);
        let (right, bottom) = (self.edge_x(x + 1) as f32, self.edge_y(y + 1) as f32);
        let (center_x, center_y) = ((left + right) / 2.0, (top + bottom) / 2.0);
        let half_x = ((right - left).max(min)) / 2.0;
        let half_y = ((bottom - top).max(min)) / 2.0;
        (center_x - half_x, center_y - half_y, half_x * 2.0, half_y * 2.0)
    }
}

fn draw(state: &mut State) -> Result<(), Failed> {
    if state.width <= 0.0 || state.height <= 0.0 {
        return Ok(());
    }
    let mut canvas = Canvas2D::new();
    if !compose_frame(state, &mut canvas) {
        return Ok(());
    }
    canvas.submit().map_err(Failed::from)
}

/// Encodes the frame: works out what changed since the last one, clips to it, and redraws everything there from
/// scratch. Returns false when nothing changed.
fn compose_frame(state: &mut State, canvas: &mut Canvas2D) -> bool {
    let view = View::of(state);
    let now = state.now;
    for flash in &mut state.flashes {
        flash.start.get_or_insert(now);
    }
    state.flashes.retain(|flash| now - flash.start.unwrap_or(now) < flash_duration(flash.kind));

    let (marks, mark_rects) = marks(state, &view);
    let drawn = Drawn {
        coordinates: coordinates_text(state),
        loading: loading_text(state),
        switch: state.admin.then_some(state.skip_cooldown),
        panel: panel_signature(state),
        toast: state.toast.as_ref().map(|(text, _)| text.clone()),
        marks,
        mark_rects,
        flashes: state.flashes.iter().map(|flash| flash_rect(&view, flash)).collect(),
    };
    let mut rects = Vec::new();
    if state.full_redraw {
        rects.push(view.canvas());
    } else {
        let previous = &state.drawn;
        if drawn.coordinates != previous.coordinates {
            rects.push(view.logical(COORDINATES_RECT));
        }
        if drawn.loading != previous.loading {
            rects.push(view.logical(LOADING_RECT));
        }
        if drawn.panel != previous.panel {
            rects.push(view.logical(panel_rect()));
        }
        if drawn.switch != previous.switch {
            rects.push(view.logical(SWITCH_RECT));
        }
        if drawn.toast != previous.toast {
            rects.push(view.logical(TOAST_RECT));
        }
        if drawn.marks != previous.marks {
            rects.extend(previous.mark_rects.iter().copied());
            rects.extend(drawn.mark_rects.iter().copied());
        }
        rects.extend(previous.flashes.iter().copied());
        rects.extend(drawn.flashes.iter().copied());
        rects.extend(dirty_tile_rects(state, &view));
    }
    state.dirty_tiles.fill(None);
    // A partial frame redraws only some of the board, so only a full one can tell that everything fit.
    if state.full_redraw {
        state.truncated = false;
    }
    state.full_redraw = false;
    let rects = merge_rects(rects, view.canvas());
    state.drawn = drawn;
    if rects.is_empty() {
        return false;
    }

    canvas.save().set_transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).begin_path();
    for rect in &rects {
        canvas.rect(rect.x0 as f32, rect.y0 as f32, rect.width(), rect.height());
    }
    canvas.clip(FillRule::NonZero);
    canvas.fill_color(SURROUND);
    for rect in &rects {
        canvas.fill_rect(rect.x0 as f32, rect.y0 as f32, rect.width(), rect.height());
    }
    for rect in &rects {
        let area = rect.intersect(view.board());
        if !area.is_empty() {
            draw_board_region(canvas, state, &view, area);
        }
    }
    draw_flashes(canvas, state, &view);
    draw_marks(canvas, state, &view);
    canvas.save().scale(view.scale_x, view.scale_y);
    draw_chrome(canvas, state, &|rect| rects.iter().any(|dirty| dirty.touches(&view.logical(rect))));
    canvas.restore();
    canvas.restore();
    true
}

/// The rectangles of the board cells that changed: one per dirty tile, or one per row of tiles when there are many.
fn dirty_tile_rects(state: &State, view: &View) -> Vec<DevRect> {
    let dirty: Vec<(usize, CellRect)> = state
        .dirty_tiles
        .iter()
        .enumerate()
        .filter_map(|(tile, rect)| rect.map(|rect| (tile, rect)))
        .collect();
    let rects: Vec<CellRect> = if dirty.len() <= DIRTY_TILE_RECTS {
        dirty.into_iter().map(|(_, rect)| rect).collect()
    } else {
        let mut rows: Vec<Option<CellRect>> = vec![None; BOARD_HEIGHT / TILE_SIDE];
        for (tile, rect) in dirty {
            let row = &mut rows[tile / TILES_PER_ROW];
            *row = Some(row.map_or(rect, |row| row.union(rect)));
        }
        rows.into_iter().flatten().collect()
    };
    rects.into_iter().map(|rect| view.cells(rect, 1)).collect()
}

/// Clips rectangles to `bounds` and merges the ones that touch, so no area is drawn twice.
fn merge_rects(rects: Vec<DevRect>, bounds: DevRect) -> Vec<DevRect> {
    let mut merged: Vec<DevRect> = Vec::new();
    for rect in rects.into_iter().map(|rect| rect.intersect(bounds)).filter(|rect| !rect.is_empty()) {
        let mut rect = rect;
        while let Some(position) = merged.iter().position(|other| other.touches(&rect)) {
            rect = rect.union(merged.swap_remove(position));
        }
        merged.push(rect);
    }
    merged
}

/// Draws the board cells inside `area` (which lies on the board): as raw pixels or as same-colored rectangles over
/// white, whichever is smaller, and as many rectangles as the frame has room for when neither fits.
fn draw_board_region(canvas: &mut Canvas2D, state: &mut State, view: &View, area: DevRect) {
    let columns: Vec<Option<usize>> = (area.x0..area.x1).map(|px| view.cell_x(px)).collect();
    let rows: Vec<Option<usize>> = (area.y0..area.y1).map(|py| view.cell_y(py)).collect();
    let (Some(cx0), Some(cy0)) = (columns.iter().flatten().next(), rows.iter().flatten().next()) else {
        return;
    };
    let cx1 = columns.iter().flatten().last().unwrap() + 1;
    let cy1 = rows.iter().flatten().last().unwrap() + 1;
    let rects = board_rects(&state.pixels, *cx0, cx1, *cy0, cy1);
    let mut colors = 0;
    let mut last = None;
    for rect in &rects {
        if last != Some(rect.4) {
            colors += 1;
            last = Some(rect.4);
        }
    }
    let rect_bytes = rects.len() * 17 + colors * 5 + 22;
    let blit_bytes = area.area() * 4 + 21;
    let room = FRAME_BUDGET.saturating_sub(canvas.len());
    if blit_bytes < rect_bytes && blit_bytes <= room {
        blit_board(canvas, &state.pixels, area, &columns, &rows);
    } else {
        state.truncated |= rect_bytes > room;
        let lut = palette_lut();
        canvas.fill_color(0xffffffff).fill_rect(area.x0 as f32, area.y0 as f32, area.width(), area.height());
        let mut color = None;
        for (x0, y0, x1, y1, pixel) in rects {
            if canvas.len() + 22 > FRAME_BUDGET {
                break;
            }
            let (left, top) = (view.edge_x(x0), view.edge_y(y0));
            let (right, bottom) = (view.edge_x(x1), view.edge_y(y1));
            if right <= left || bottom <= top {
                continue;
            }
            if color != Some(pixel) {
                let [r, g, b, a] = lut[pixel as usize];
                canvas.fill_color(rgba(r, g, b, a));
                color = Some(pixel);
            }
            canvas.fill_rect(left as f32, top as f32, (right - left) as f32, (bottom - top) as f32);
        }
    }
    if view.cell_device_size() >= GRID_MIN_CELL {
        draw_grid(canvas, view, area, (*cx0, cx1), (*cy0, cy1));
    }
}

/// The placed cells in a block of the board as rectangles `(x0, y0, x1, y1, color)`, sorted by color. A run that
/// repeats on the next row grows down instead of starting a new rectangle. Unplaced cells are left out: they are the
/// white the rectangles are drawn over.
fn board_rects(pixels: &[u8], x0: usize, x1: usize, y0: usize, y1: usize) -> Vec<(usize, usize, usize, usize, u8)> {
    let mut done = Vec::new();
    let mut open: Vec<(usize, usize, usize, usize, u8)> = Vec::new();
    for y in y0..y1 {
        let row = &pixels[y * BOARD_WIDTH..(y + 1) * BOARD_WIDTH];
        let mut next_open = Vec::with_capacity(open.len());
        let mut previous = open.into_iter().peekable();
        let mut x = x0;
        while x < x1 {
            if row[x] == EMPTY {
                x += 1;
                continue;
            }
            let (start, color) = (x, row[x]);
            while x < x1 && row[x] == color {
                x += 1;
            }
            while previous.peek().is_some_and(|rect| rect.0 < start) {
                done.push(previous.next().unwrap());
            }
            match previous.peek() {
                Some(rect) if rect.0 == start && rect.2 == x && rect.4 == color => {
                    let mut rect = previous.next().unwrap();
                    rect.3 = y + 1;
                    next_open.push(rect);
                }
                _ => next_open.push((start, y, x, y + 1, color)),
            }
        }
        done.extend(previous);
        open = next_open;
    }
    done.extend(open);
    done.sort_by_key(|rect| rect.4);
    done
}

fn blit_board(canvas: &mut Canvas2D, pixels: &[u8], area: DevRect, columns: &[Option<usize>], rows: &[Option<usize>]) {
    let lut = palette_lut();
    let width = columns.len();
    let mut buffer = vec![0u8; width * rows.len() * 4];
    for (out, y) in buffer.chunks_exact_mut(width * 4).zip(rows) {
        let Some(y) = y else { continue };
        let row = &pixels[y * BOARD_WIDTH..(y + 1) * BOARD_WIDTH];
        for (pixel, x) in out.chunks_exact_mut(4).zip(columns) {
            if let Some(x) = x {
                pixel.copy_from_slice(&lut[row[*x] as usize]);
            }
        }
    }
    canvas.blit_rgba(area.x0 as f32, area.y0 as f32, width as u32, rows.len() as u32, &buffer);
}

/// One-pixel lines along the cell edges in `area`, once cells are big enough to tell apart.
fn draw_grid(canvas: &mut Canvas2D, view: &View, area: DevRect, (x0, x1): (usize, usize), (y0, y1): (usize, usize)) {
    canvas.fill_color(0x0000001a);
    for x in x0..=x1 {
        canvas.fill_rect(view.edge_x(x) as f32, area.y0 as f32, 1.0, area.height());
    }
    for y in y0..=y1 {
        canvas.fill_rect(area.x0 as f32, view.edge_y(y) as f32, area.width(), 1.0);
    }
}

/// The marks drawn on the board, as a signature of what they show and the rectangles they cover: the reticle on the
/// selected pixel, with the chosen color previewed inside, and an outline on the hovered one.
fn marks(state: &State, view: &View) -> (String, Vec<DevRect>) {
    let mut rects = Vec::new();
    let preview = preview_color(state);
    if let Some(index) = state.selection {
        let (x, y, width, height) = view.cell_box(index, 14.0);
        let grow = 4.0 + width.min(height) * 0.12;
        rects.push(DevRect {
            x0: (x - grow).floor() as i32 - 3,
            y0: (y - grow).floor() as i32 - 3,
            x1: (x + width + grow).ceil() as i32 + 3,
            y1: (y + height + grow).ceil() as i32 + 3,
        });
    }
    let hover = state.hover.filter(|hover| Some(*hover) != state.selection && view.cell_device_size() >= 6.0);
    if let Some(index) = hover {
        rects.push(view.cells(CellRect::cell(index as usize), 2));
    }
    (format!("{:?} {:?} {:?}", state.selection, preview, hover), rects)
}

/// The color to preview in the reticle: the chosen one, when it could be placed now.
fn preview_color(state: &State) -> Option<u8> {
    state
        .color
        .filter(|_| state.can_place && (skipping_cooldown(state) || !cooling(state)) && state.selection.is_some())
}

fn draw_marks(canvas: &mut Canvas2D, state: &State, view: &View) {
    if let Some(index) = state.hover.filter(|hover| Some(*hover) != state.selection && view.cell_device_size() >= 6.0) {
        let (x, y, width, height) = view.cell_box(index, 0.0);
        canvas.stroke_color(0x00000080).line_width(1.0).stroke_rect(x + 0.5, y + 0.5, width - 1.0, height - 1.0);
    }
    let Some(index) = state.selection else { return };
    let (x, y, width, height) = view.cell_box(index, 14.0);
    if let Some(color) = preview_color(state) {
        let [r, g, b, a] = palette_lut()[color as usize];
        let (cx, cy, cw, ch) = view.cell_box(index, 0.0);
        canvas.fill_color(rgba(r, g, b, a)).fill_rect(cx, cy, cw.max(1.0), ch.max(1.0));
    }
    // Four corner brackets, r/place style, dark under light so they read on any color.
    let grow = 4.0 + width.min(height) * 0.12;
    let (left, top, right, bottom) = (x - grow, y - grow, x + width + grow, y + height + grow);
    let arm = ((right - left) * 0.3).max(5.0);
    canvas.begin_path();
    for (corner_x, corner_y, dx, dy) in [(left, top, 1.0, 1.0), (right, top, -1.0, 1.0), (left, bottom, 1.0, -1.0), (right, bottom, -1.0, -1.0)] {
        canvas
            .move_to(corner_x + dx * arm, corner_y)
            .line_to(corner_x, corner_y)
            .line_to(corner_x, corner_y + dy * arm);
    }
    canvas.line_cap(LineCap::Round).line_join(LineJoin::Round);
    canvas.stroke_color(0x000000b3).line_width(4.0).stroke();
    canvas.stroke_color(0xffffffff).line_width(2.0).stroke();
}

fn flash_duration(kind: FlashKind) -> f64 {
    match kind {
        FlashKind::Local => LOCAL_FLASH_MS,
        FlashKind::Remote => REMOTE_FLASH_MS,
    }
}

fn flash_rect(view: &View, flash: &Flash) -> DevRect {
    // Room for the ring each flash draws around its cells, at its widest.
    view.cells(flash.bounds, 12 + (view.cell_device_size() * 2.0).ceil() as i32)
}

fn draw_flashes(canvas: &mut Canvas2D, state: &State, view: &View) {
    for flash in &state.flashes {
        let progress = ((state.now - flash.start.unwrap_or(state.now)) / flash_duration(flash.kind)).clamp(0.0, 1.0) as f32;
        let fade = (1.0 - progress) * (1.0 - progress);
        if flash.kind == FlashKind::Remote && flash.cells.len() <= FLASH_CELLS {
            canvas.fill_color(rgba(255, 255, 255, (0.6 * fade * 255.0) as u8));
            for cell in &flash.cells {
                let (x, y, width, height) = view.cell_box(*cell, 1.0);
                canvas.fill_rect(x, y, width, height);
            }
        }
        // A ring that eases outwards as it fades: amber for someone else, the accent for your own pixel.
        let grow = 2.0 + progress * (8.0 + view.cell_device_size() * 2.0);
        let bounds = flash.bounds;
        let (left, top) = (view.edge_x(bounds.x0) as f32 - grow, view.edge_y(bounds.y0) as f32 - grow);
        let (right, bottom) = (view.edge_x(bounds.x1) as f32 + grow, view.edge_y(bounds.y1) as f32 + grow);
        let color = match flash.kind {
            FlashKind::Remote => rgba(251, 191, 36, (fade * 230.0) as u8),
            FlashKind::Local => rgba(255, 69, 0, (fade * 255.0) as u8),
        };
        canvas.stroke_color(color).line_width(2.0).stroke_rect(left, top, right - left, bottom - top);
    }
}

fn coordinates_text(state: &State) -> Option<String> {
    let index = state.selection.or(state.hover)? as usize;
    let place = format!("({}, {})", index % BOARD_WIDTH, index / BOARD_WIDTH);
    if state.selection.is_none() {
        return Some(place);
    }
    Some(match &state.selection_info {
        Some(info) => format!("{place}  ·  {info}"),
        None => format!("{place}  ·  …"),
    })
}

fn loading_text(state: &State) -> Option<String> {
    if state.unavailable {
        return Some("Board unavailable".to_string());
    }
    if !state.loaded {
        return None;
    }
    let left = tiles_left(state);
    if left == 0 {
        return state.truncated.then(|| "Zoom in to see every pixel".to_string());
    }
    if state.fetch_failures >= 3 {
        return Some("Reconnecting…".to_string());
    }
    let total = state.wanted.iter().filter(|stamp| **stamp != 0).count().max(1);
    Some(format!("Loading  {}%", (total - left.min(total)) * 100 / total))
}

/// Everything the palette bar shows, so it is redrawn only when one of those changes.
fn panel_signature(state: &State) -> String {
    format!(
        "{} {:?} {} {}",
        state.can_place,
        state.color,
        place_label(state),
        place_blocker(state).is_none()
    )
}

fn place_label(state: &State) -> String {
    if skipping_cooldown(state) {
        "Place".to_string()
    } else if cooling(state) {
        clock_text(cooldown_seconds(state))
    } else if !state.pending.is_empty() {
        "…".to_string()
    } else {
        "Place".to_string()
    }
}

/// A rounded, slightly see-through pill.
fn pill(canvas: &mut Canvas2D, x: f32, y: f32, width: f32, height: f32) {
    canvas
        .fill_color(CHROME)
        .begin_path()
        .round_rect(x, y, width, height, height / 2.0)
        .fill(FillRule::NonZero);
    canvas
        .stroke_color(CHROME_LINE)
        .line_width(1.0)
        .begin_path()
        .round_rect(x + 0.5, y + 0.5, width - 1.0, height - 1.0, (height - 1.0) / 2.0)
        .stroke();
}

/// About how wide `text` is in the chrome's 13 px font. There is no text measuring without a host call.
fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * 7.2
}

fn label(canvas: &mut Canvas2D, text: &str, x: f32, y: f32, color: u32, align: TextAlign, weight: f32) {
    canvas
        .fill_color(color)
        .font("sans-serif", 13.0, weight)
        .text_align(align)
        .text_baseline(TextBaseline::Middle)
        .fill_text(text, x, y);
}

/// Draws each piece of chrome whose rectangle `redrawn` says is part of this frame; the rest is clipped away anyway.
fn draw_chrome(canvas: &mut Canvas2D, state: &State, redrawn: &dyn Fn((f32, f32, f32, f32)) -> bool) {
    if let Some(text) = coordinates_text(state).filter(|_| redrawn(COORDINATES_RECT)) {
        let (x, y, width, height) = COORDINATES_RECT;
        let width = (text_width(&text) + 28.0).min(width);
        pill(canvas, x, y, width, height);
        label(canvas, &text, x + 14.0, y + height / 2.0, INK, TextAlign::Left, 500.0);
    }

    if redrawn(ZOOM_RECT) {
        draw_zoom_buttons(canvas);
    }
    if let Some(text) = loading_text(state).filter(|_| redrawn(LOADING_RECT)) {
        draw_loading(canvas, state, &text);
    }
    if state.admin && redrawn(SWITCH_RECT) {
        draw_cooldown_switch(canvas, state.skip_cooldown);
    }
    if redrawn(panel_rect()) {
        draw_panel(canvas, state);
    }
    if let Some((text, _)) = state.toast.as_ref().filter(|_| redrawn(TOAST_RECT)) {
        let (x, y, width, height) = TOAST_RECT;
        let pill_width = (text_width(text) + 36.0).min(width);
        pill(canvas, x + (width - pill_width) / 2.0, y, pill_width, height);
        label(canvas, text, x + width / 2.0, y + height / 2.0, INK, TextAlign::Center, 500.0);
    }
}

/// The admin's switch: a pill with a label and a little track, lit when the cooldown is off.
fn draw_cooldown_switch(canvas: &mut Canvas2D, on: bool) {
    let (x, y, width, height) = SWITCH_RECT;
    pill(canvas, x, y, width, height);
    label(canvas, "No cooldown", x + 14.0, y + height / 2.0, if on { INK } else { INK_MUTED }, TextAlign::Left, 500.0);
    let (track_width, track_height) = (30.0, 18.0);
    let track_x = x + width - 12.0 - track_width;
    let track_y = y + (height - track_height) / 2.0;
    canvas
        .fill_color(if on { ACCENT } else { 0xffffff26 })
        .begin_path()
        .round_rect(track_x, track_y, track_width, track_height, track_height / 2.0)
        .fill(FillRule::NonZero);
    let knob_x = if on { track_x + track_width - track_height / 2.0 } else { track_x + track_height / 2.0 };
    canvas
        .fill_color(INK)
        .begin_path()
        .arc(knob_x, track_y + track_height / 2.0, track_height / 2.0 - 3.0, 0.0, std::f32::consts::TAU, false)
        .fill(FillRule::NonZero);
}

fn draw_zoom_buttons(canvas: &mut Canvas2D) {
    let (x, y, width, height) = ZOOM_RECT;
    pill(canvas, x, y, width, height);
    canvas.fill_color(CHROME_LINE);
    for divider in 1..3 {
        canvas.fill_rect(x + ZOOM_BUTTON * divider as f32, y + 8.0, 1.0, height - 16.0);
    }
    for (slot, text) in ["−", "+", "Fit"].iter().enumerate() {
        let size = if slot < 2 { 18.0 } else { 12.0 };
        canvas
            .fill_color(INK)
            .font("sans-serif", size, 500.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_text(text, x + ZOOM_BUTTON * (slot as f32 + 0.5), y + height / 2.0 + 0.5);
    }
}

fn draw_loading(canvas: &mut Canvas2D, state: &State, text: &str) {
    let (x, y, width, height) = LOADING_RECT;
    pill(canvas, x, y, width, height);
    label(canvas, text, x + width / 2.0, y + height / 2.0 - 2.0, INK_MUTED, TextAlign::Center, 500.0);
    // A hairline progress bar along the pill's bottom.
    let total = state.wanted.iter().filter(|stamp| **stamp != 0).count().max(1) as f32;
    let done = 1.0 - tiles_left(state) as f32 / total;
    canvas.fill_color(0xffffff1a).fill_rect(x + 24.0, y + height - 7.0, width - 48.0, 2.0);
    canvas.fill_color(ACCENT).fill_rect(x + 24.0, y + height - 7.0, (width - 48.0) * done, 2.0);
}

fn draw_panel(canvas: &mut Canvas2D, state: &State) {
    let (x, y, width, height) = panel_rect();
    if !state.can_place {
        // Logged out: a single quiet line where the palette would be.
        let text = "Log in to place pixels";
        let pill_width = text_width(text) + 40.0;
        let top = y + (height - 36.0) / 2.0;
        pill(canvas, x + (width - pill_width) / 2.0, top, pill_width, 36.0);
        label(canvas, text, x + width / 2.0, top + 18.0, INK_MUTED, TextAlign::Center, 500.0);
        return;
    }
    canvas
        .fill_color(CHROME)
        .begin_path()
        .round_rect(x, y, width, height, 18.0)
        .fill(FillRule::NonZero);
    canvas
        .stroke_color(CHROME_LINE)
        .line_width(1.0)
        .begin_path()
        .round_rect(x + 0.5, y + 0.5, width - 1.0, height - 1.0, 17.5)
        .stroke();
    for (color, [r, g, b, a]) in palette_lut().iter().take(PALETTE.len()).copied().enumerate() {
        let (left, top) = swatch_origin(color);
        canvas
            .fill_color(rgba(r, g, b, a))
            .begin_path()
            .round_rect(left, top, SWATCH, SWATCH, 6.0)
            .fill(FillRule::NonZero);
        canvas
            .stroke_color(0xffffff26)
            .line_width(1.0)
            .begin_path()
            .round_rect(left + 0.5, top + 0.5, SWATCH - 1.0, SWATCH - 1.0, 5.5)
            .stroke();
        if state.color == Some(color as u8) {
            canvas
                .stroke_color(0xffffffff)
                .line_width(2.0)
                .begin_path()
                .round_rect(left - 3.0, top - 3.0, SWATCH + 6.0, SWATCH + 6.0, 8.0)
                .stroke();
        }
    }
    let (left, top, button_width, button_height) = PLACE_RECT;
    let ready = place_blocker(state).is_none();
    let (fill, ink) = if ready { (0xfafafaff, 0x18181bff) } else { (0xffffff14, INK_MUTED) };
    canvas
        .fill_color(fill)
        .begin_path()
        .round_rect(left, top, button_width, button_height, 12.0)
        .fill(FillRule::NonZero);
    canvas
        .fill_color(ink)
        .font("sans-serif", 15.0, 650.0)
        .text_align(TextAlign::Center)
        .text_baseline(TextBaseline::Middle)
        .fill_text(&place_label(state), left + button_width / 2.0, top + button_height / 2.0 + 0.5);
}

/// Each palette index as RGBA bytes: the r/place colors, white for an unplaced pixel and gray for a loading one.
fn palette_lut() -> &'static [[u8; 4]; 256] {
    static LUT: OnceLock<[[u8; 4]; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [[0u8; 4]; 256];
        for (entry, color) in lut.iter_mut().zip(PALETTE) {
            *entry = [(color >> 16) as u8, (color >> 8) as u8, color as u8, 255];
        }
        lut[EMPTY as usize] = [255, 255, 255, 255];
        lut[LOADING as usize] = [228, 228, 231, 255];
        lut
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_state() -> State {
        let mut state = State::new(960.0, 640.0);
        state.zoom = 1.0;
        state.now = 1000.0;
        let stamps = encode_stamps(&[0; TILE_COUNT]);
        load_shared_state(&mut state, &json!({ "stamps": stamps, "tiles": [], "canPlace": true, "cooldown": 0 }));
        state
    }

    fn encode_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut encoded = String::new();
        for chunk in bytes.chunks(3) {
            let value = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for (position, shift) in [18, 12, 6, 0].into_iter().enumerate() {
                if position <= chunk.len() {
                    encoded.push(ALPHABET[((value >> shift) & 63) as usize] as char);
                }
            }
        }
        encoded
    }

    fn encode_stamps(stamps: &[u32]) -> String {
        encode_base64(&stamps.iter().flat_map(|stamp| stamp.to_le_bytes()).collect::<Vec<_>>())
    }

    fn pointer_fields(action: &str, x: f32, y: f32, id: i32, kind: &str) -> Map<String, Value> {
        json!({
            "action": action, "x": x, "y": y, "width": LOGICAL_WIDTH, "height": LOGICAL_HEIGHT,
            "pointerId": id, "buttons": 1, "pointerType": kind,
        })
        .as_object()
        .unwrap()
        .clone()
    }

    fn key_fields(key: &str) -> Map<String, Value> {
        json!({ "action": "down", "key": key }).as_object().unwrap().clone()
    }

    /// The logical point at the center of board cell (`x`, `y`).
    fn cell_point(state: &State, x: usize, y: usize) -> (f32, f32) {
        let (left, top) = board_origin(state);
        (left + (x as f32 + 0.5) * state.zoom, top + (y as f32 + 0.5) * state.zoom)
    }

    fn tap(state: &mut State, x: f32, y: f32) {
        pointer(state, &pointer_fields("down", x, y, 1, "mouse"));
        pointer(state, &pointer_fields("up", x, y, 1, "mouse"));
    }

    fn select_and_color(state: &mut State, x: usize, y: usize, color: u8) {
        let (px, py) = cell_point(state, x, y);
        tap(state, px, py);
        let (sx, sy) = swatch_origin(color as usize);
        tap(state, sx + 2.0, sy + 2.0);
    }

    #[test]
    fn the_palette_is_r_place_2022_with_white_for_unplaced_and_gray_for_loading() {
        assert_eq!(PALETTE.len(), 32);
        assert_eq!(palette_lut()[2], [255, 69, 0, 255]);
        assert_eq!(palette_lut()[31], [255, 255, 255, 255]);
        assert_eq!(palette_lut()[EMPTY as usize], [255, 255, 255, 255]);
        assert_eq!(palette_lut()[LOADING as usize], [228, 228, 231, 255]);
        for color in 0..PALETTE.len() {
            let (x, y) = swatch_origin(color);
            assert_eq!(swatch_hit(x + 1.0, y + 1.0), Some(color as u8));
            assert_eq!(swatch_hit(x + SWATCH + 1.0, y + 1.0), None);
        }
        assert!(PANEL_LEFT > 0.0 && PANEL_LEFT + PANEL_WIDTH < LOGICAL_WIDTH);
        assert!(inside(panel_rect(), PLACE_RECT.0 + 1.0, PLACE_RECT.1 + 1.0));
        // The zoom pill sits bottom right, clear of the palette bar and of the host's fullscreen button up top.
        assert!(ZOOM_RECT.0 > PANEL_LEFT + PANEL_WIDTH + MARGIN);
        assert!(ZOOM_RECT.1 > LOGICAL_HEIGHT / 2.0 && ZOOM_RECT.1 + ZOOM_RECT.3 <= LOGICAL_HEIGHT - MARGIN);
        assert_eq!(zoom_hit(ZOOM_RECT.0 + 1.0, ZOOM_RECT.1 + 1.0), Some(ZoomAction::Out));
    }

    #[test]
    fn the_whole_board_fits_above_the_palette_bar() {
        let state = State::new(960.0, 640.0);
        let (left, top) = board_origin(&state);
        assert!(left >= 0.0 && left + BOARD_WIDTH as f32 * state.zoom <= LOGICAL_WIDTH);
        assert!(top >= 0.0 && top + BOARD_HEIGHT as f32 * state.zoom <= PANEL_TOP);
    }

    #[test]
    fn a_tap_selects_a_pixel_and_asks_who_placed_it_and_a_drag_pans_instead() {
        let mut state = ready_state();
        let (x, y) = cell_point(&state, 10, 20);
        tap(&mut state, x, y);
        assert_eq!(state.selection, Some((20 * BOARD_WIDTH + 10) as u32));
        assert_eq!(state.info_request, state.selection);
        assert_eq!(coordinates_text(&state).unwrap(), "(10, 20)  ·  …");
        state.info_sequence = Some((4, state.selection.unwrap()));
        state.info_request = None;
        handle_reply(&mut state, 4, &json!({ "type": "pixel_info", "name": "Ada", "trolltag": "ada" }));
        assert_eq!(coordinates_text(&state).unwrap(), "(10, 20)  ·  @ada");

        pointer(&mut state, &pointer_fields("down", x, y, 1, "mouse"));
        pointer(&mut state, &pointer_fields("move", x + 3.0, y, 1, "mouse"));
        assert_eq!(state.pan_x, 0.0);
        pointer(&mut state, &pointer_fields("move", x + 30.0, y + 10.0, 1, "mouse"));
        pointer(&mut state, &pointer_fields("up", x + 30.0, y + 10.0, 1, "mouse"));
        assert_eq!((state.pan_x, state.pan_y), (30.0, 10.0));
        assert_eq!(state.selection, Some((20 * BOARD_WIDTH + 10) as u32));

        // A tap off the board clears the selection.
        tap(&mut state, 2.0, 300.0);
        assert_eq!(state.selection, None);
    }

    #[test]
    fn placing_shows_the_pixel_at_once_and_starts_the_cooldown() {
        let mut state = ready_state();
        select_and_color(&mut state, 5, 5, 2);
        assert_eq!(state.color, Some(2));
        assert_eq!(preview_color(&state), Some(2));
        key(&mut state, &key_fields("Enter"));
        let index = 5 * BOARD_WIDTH + 5;
        assert_eq!(state.pixels[index], 2);
        assert_eq!(state.pending, [Placement { index: index as u32, color: 2 }]);
        assert!(cooling(&state));
        assert_eq!(place_label(&state), "0:15");
        assert_eq!(preview_color(&state), None);

        state.in_flight = Some(9);
        handle_reply(&mut state, 9, &json!({ "type": "placed", "stamps": [[0, 3]], "wait": 15 }));
        assert!(state.pending.is_empty());
        assert_eq!(state.acknowledged.len(), 1);
        state.now += 14_500.0;
        assert_eq!(place_label(&state), "0:01");
        state.now += 600.0;
        assert!(!cooling(&state));
        assert_eq!(place_label(&state), "Place");

        // Placing again during a cooldown only explains why not.
        start_cooldown(&mut state, 30.0);
        key(&mut state, &key_fields(" "));
        assert_eq!(state.toast.as_ref().unwrap().0, "Wait for your cooldown");
        assert!(state.pending.is_empty());
    }

    #[test]
    fn only_admins_get_the_cooldown_switch_and_it_lets_them_place_back_to_back() {
        let mut state = ready_state();
        let (x, y) = (SWITCH_RECT.0 + 10.0, SWITCH_RECT.1 + 10.0);
        assert!(SWITCH_RECT.0 + SWITCH_RECT.2 + MARGIN <= PANEL_LEFT);
        // Not an admin: nothing is drawn there, and a tap there is just a tap on the board.
        tap(&mut state, x, y);
        assert!(!state.skip_cooldown);
        compose_frame(&mut state, &mut Canvas2D::new());
        assert_eq!(state.drawn.switch, None);

        let admin_state = json!({ "stamps": encode_stamps(&[0; TILE_COUNT]), "canPlace": true, "admin": true });
        load_shared_state(&mut state, &admin_state);
        compose_frame(&mut state, &mut Canvas2D::new());
        assert_eq!(state.drawn.switch, Some(false));
        tap(&mut state, x, y);
        assert!(skipping_cooldown(&state));
        assert_eq!(state.toast.as_ref().unwrap().0, "Cooldown off for you (admin)");

        select_and_color(&mut state, 5, 5, 2);
        place(&mut state);
        assert!(!cooling(&state));
        assert_eq!(place_label(&state), "Place");
        select_and_color(&mut state, 6, 5, 3);
        place(&mut state);
        assert_eq!(state.pending.len(), 2);

        // Losing admin (a later render without the flag) turns the switch off and hides it.
        load_shared_state(&mut state, &json!({ "stamps": encode_stamps(&[0; TILE_COUNT]), "canPlace": true }));
        assert!(!state.admin && !state.skip_cooldown);
    }

    #[test]
    fn a_placement_the_server_refuses_is_taken_back() {
        let mut state = ready_state();
        select_and_color(&mut state, 5, 5, 2);
        place(&mut state);
        state.in_flight = Some(1);
        handle_reply(&mut state, 1, &json!({ "type": "cooldown", "wait": 12 }));
        assert_eq!(state.pixels[5 * BOARD_WIDTH + 5], EMPTY);
        assert_eq!(state.toast.as_ref().unwrap().0, "You can place again in 0:12");
        assert_eq!(place_label(&state), "0:12");

        state.cooldown_until = 0.0;
        place(&mut state);
        state.in_flight = Some(2);
        handle_error(&mut state, Some(2), "storage_busy");
        assert_eq!(state.pixels[5 * BOARD_WIDTH + 5], EMPTY);
        assert!(!cooling(&state));
    }

    #[test]
    fn a_cooldown_from_the_first_render_waits_for_the_clock() {
        let mut state = State::new(960.0, 640.0);
        load_shared_state(&mut state, &json!({ "stamps": encode_stamps(&[0; TILE_COUNT]), "canPlace": true, "cooldown": 9 }));
        assert!(cooling(&state));
        assert_eq!(place_label(&state), "0:09");
        tick(&mut state, 50_000.0);
        assert_eq!(state.cooldown_until, 59_000.0);
        assert_eq!(place_label(&state), "0:09");
    }

    #[test]
    fn logged_out_viewers_see_the_board_but_no_palette() {
        let mut state = State::new(960.0, 640.0);
        load_shared_state(&mut state, &json!({ "stamps": encode_stamps(&[0; TILE_COUNT]), "canPlace": false }));
        assert_eq!(place_blocker(&state), Some("Log in to place pixels"));
        let (sx, sy) = swatch_origin(3);
        tap(&mut state, sx + 2.0, sy + 2.0);
        assert_eq!(state.color, None);
    }

    #[test]
    fn unloaded_tiles_show_as_loading_and_fetches_go_one_at_a_time_nearest_first() {
        let mut state = State::new(960.0, 640.0);
        let mut stamps = [0u32; TILE_COUNT];
        stamps[0] = 3;
        stamps[TILE_COUNT - 1] = 2;
        stamps[8 * TILES_PER_ROW + 8] = 1;
        load_shared_state(
            &mut state,
            &json!({ "stamps": encode_stamps(&stamps), "tiles": [[0, encode_base64(&[1, 1, 7])]], "canPlace": true }),
        );
        assert_eq!(state.pixels[1], 7);
        assert_eq!(state.pixels[(BOARD_HEIGHT - 1) * BOARD_WIDTH + BOARD_WIDTH - 1], LOADING);
        assert_eq!(state.pixels[(BOARD_HEIGHT - 1) * BOARD_WIDTH], EMPTY);
        assert_eq!(tiles_left(&state), 2);
        assert_eq!(next_fetch(&state), [8 * TILES_PER_ROW + 8, TILE_COUNT - 1]);
        assert_eq!(loading_text(&state).unwrap(), "Loading  33%");
        assert!(state.flashes.is_empty());

        // A fetched tile replaces the loading gray without flashing.
        let noisy = vec![9u8; TILE_PIXELS];
        state.fetching = Some((5, vec![TILE_COUNT - 1]));
        handle_reply(&mut state, 5, &json!({ "type": "tiles", "tiles": [[TILE_COUNT - 1, 2, encode_base64(&noisy)]] }));
        assert_eq!(state.pixels[(BOARD_HEIGHT - 1) * BOARD_WIDTH + BOARD_WIDTH - 1], 9);
        assert_eq!(tiles_left(&state), 1);
        assert!(state.fetching.is_none());
        assert!(state.flashes.is_empty());
        assert!(wants_frames(&state));
    }

    #[test]
    fn failed_fetches_back_off_and_retry_instead_of_stopping() {
        let mut state = ready_state();
        state.now = 10_000.0;
        for (failures, wait) in [(1, 500.0), (2, 1000.0), (3, 2000.0), (4, 4000.0), (5, 8000.0), (9, 8000.0)] {
            state.fetching = Some((1, vec![0]));
            state.fetch_failures = failures - 1;
            handle_error(&mut state, Some(1), "slow_down");
            assert!(state.fetching.is_none());
            assert_eq!(state.next_fetch_at, 10_000.0 + wait);
        }
        state.wanted[0] = 1;
        state.tile_stamps[0] = None;
        assert_eq!(loading_text(&state).unwrap(), "Reconnecting…");
        // A new render clears the backoff.
        let mut stamps = [0u32; TILE_COUNT];
        stamps[0] = 1;
        load_shared_state(&mut state, &json!({ "stamps": encode_stamps(&stamps), "canPlace": true }));
        assert_eq!((state.fetch_failures, state.next_fetch_at), (0, 0.0));
    }

    #[test]
    fn someone_elses_pixel_flashes_and_refreshes_the_selection_info() {
        let mut state = ready_state();
        let (x, y) = cell_point(&state, 1, 1);
        tap(&mut state, x, y);
        state.info_request = None;
        let mut tile = vec![EMPTY; TILE_PIXELS];
        tile[33] = 4;
        let mut stamps = [0u32; TILE_COUNT];
        stamps[0] = 1;
        load_shared_state(&mut state, &json!({ "stamps": encode_stamps(&stamps), "tiles": [[0, encode_base64(&tile)]], "canPlace": true }));
        assert_eq!(state.pixels[BOARD_WIDTH + 1], 4);
        assert_eq!(state.flashes.len(), 1);
        assert_eq!(state.flashes[0].kind, FlashKind::Remote);
        assert_eq!(state.info_request, Some((BOARD_WIDTH + 1) as u32));
        assert_eq!(state.dirty_tiles[0], Some(CellRect { x0: 1, y0: 1, x1: 2, y1: 2 }));
    }

    #[test]
    fn keys_nudge_the_selection_pan_zoom_and_cycle_colors() {
        let mut state = ready_state();
        key(&mut state, &key_fields("ArrowLeft"));
        assert_eq!(state.pan_x, KEY_PAN_STEP);
        key(&mut state, &key_fields("d"));
        assert_eq!(state.pan_x, 0.0);
        let (x, y) = cell_point(&state, 0, 0);
        tap(&mut state, x, y);
        key(&mut state, &key_fields("ArrowLeft"));
        assert_eq!(state.selection, Some(0));
        key(&mut state, &key_fields("ArrowRight"));
        key(&mut state, &key_fields("ArrowDown"));
        assert_eq!(state.selection, Some((BOARD_WIDTH + 1) as u32));
        key(&mut state, &key_fields("e"));
        assert_eq!(state.color, Some(0));
        key(&mut state, &key_fields("q"));
        assert_eq!(state.color, Some(31));
        key(&mut state, &key_fields("Backspace"));
        assert_eq!(state.selection, None);

        key(&mut state, &key_fields("+"));
        assert_eq!(state.zoom, 1.5);
        for _ in 0..20 {
            key(&mut state, &key_fields("+"));
        }
        assert_eq!(state.zoom, MAX_ZOOM);
        key(&mut state, &key_fields("0"));
        assert_eq!((state.zoom, state.pan_x, state.pan_y), (fit_zoom(), 0.0, 0.0));
    }

    #[test]
    fn zooming_keeps_the_selected_pixel_under_the_reticle() {
        let mut state = ready_state();
        let (x, y) = cell_point(&state, 300, 100);
        tap(&mut state, x, y);
        for _ in 0..4 {
            apply_zoom(&mut state, ZoomAction::In);
        }
        assert_eq!(board_index(&state, x, y), Some(100 * BOARD_WIDTH + 300));
    }

    #[test]
    fn two_fingers_pinch_around_their_midpoint_without_selecting() {
        let mut state = ready_state();
        let (x, y) = cell_point(&state, 50, 50);
        pointer(&mut state, &pointer_fields("down", x, y, 1, "touch"));
        pointer(&mut state, &pointer_fields("down", x + 40.0, y, 2, "touch"));
        assert!(matches!(state.gesture, Gesture::Pinch(_)));
        let (mid_x, mid_y) = (x + 20.0, y);
        let before = board_index(&state, mid_x, mid_y);
        pointer(&mut state, &pointer_fields("move", x - 20.0, y, 1, "touch"));
        pointer(&mut state, &pointer_fields("move", x + 60.0, y, 2, "touch"));
        assert_eq!(state.zoom, 2.0);
        assert_eq!(board_index(&state, mid_x, mid_y), before);
        pointer(&mut state, &pointer_fields("up", x - 20.0, y, 1, "touch"));
        pointer(&mut state, &pointer_fields("up", x + 60.0, y, 2, "touch"));
        assert_eq!(state.gesture, Gesture::Idle);
        assert_eq!(state.selection, None);

        // Two mouse presses are never a pinch.
        pointer(&mut state, &pointer_fields("down", x, y, 1, "mouse"));
        pointer(&mut state, &pointer_fields("down", x, y, 2, "mouse"));
        assert_eq!(state.pointers.len(), 1);
    }

    #[test]
    fn going_fullscreen_redraws_sharper_and_keeps_the_view() {
        let mut state = ready_state();
        state.pan_x = 40.0;
        compose_frame(&mut state, &mut Canvas2D::new());
        let fields = json!({ "width": 1920.0, "height": 1280.0, "fullscreen": true }).as_object().unwrap().clone();
        assert!(handle_event(&mut state, "resize", &fields));
        assert!(state.fullscreen && state.full_redraw);
        assert_eq!((state.pan_x, state.zoom), (40.0, 1.0));
        assert_eq!(View::of(&state).cell_device_size(), 2.0);
        let mut canvas = Canvas2D::new();
        assert!(compose_frame(&mut state, &mut canvas));
        assert!(canvas.len() <= 4 * 1024 * 1024);
    }

    #[test]
    fn cell_edges_land_on_whole_device_pixels_and_map_back_to_their_cells() {
        let mut state = State::new(1333.0, 888.7);
        state.zoom = 2.3;
        let view = View::of(&state);
        for x in [0, 1, 100, 511] {
            let (left, right) = (view.edge_x(x), view.edge_x(x + 1));
            assert!(right > left);
            for px in left..right {
                assert_eq!(view.cell_x(px), Some(x));
            }
        }
        assert_eq!(view.cell_x(view.edge_x(0) - 1), None);
        assert_eq!(view.cell_x(view.edge_x(BOARD_WIDTH)), None);
    }

    #[test]
    fn board_rects_merge_runs_across_rows_and_leave_unplaced_cells_white() {
        let mut pixels = vec![EMPTY; BOARD_WIDTH * BOARD_HEIGHT];
        for y in 0..3 {
            pixels[y * BOARD_WIDTH + 1] = 3;
            pixels[y * BOARD_WIDTH + 2] = 3;
        }
        pixels[5] = 5;
        pixels[BOARD_WIDTH + 6] = 5;
        let rects = board_rects(&pixels, 0, 8, 0, 4);
        assert_eq!(rects, [(1, 0, 3, 3, 3), (5, 0, 6, 1, 5), (6, 1, 7, 2, 5)]);
    }

    #[test]
    fn a_frame_redraws_only_what_changed() {
        let mut state = ready_state();
        let mut canvas = Canvas2D::new();
        assert!(compose_frame(&mut state, &mut canvas));
        let full = canvas.len();
        assert!(!compose_frame(&mut state, &mut Canvas2D::new()));

        set_pixel(&mut state, 100 * BOARD_WIDTH + 100, 10);
        let mut canvas = Canvas2D::new();
        assert!(compose_frame(&mut state, &mut canvas));
        assert!(canvas.len() < full);

        state.hover = Some((100 * BOARD_WIDTH + 5) as u32);
        let mut canvas = Canvas2D::new();
        assert!(compose_frame(&mut state, &mut canvas));
        assert_eq!(state.drawn.coordinates.as_deref(), Some("(5, 100)"));
        // At this zoom a cell is one device pixel, too small for a hover outline.
        assert!(state.drawn.mark_rects.is_empty());
    }

    #[test]
    fn a_noisy_board_never_overflows_the_frame_buffer() {
        let mut state = State::new(2048.0, 1365.0);
        state.zoom = 2.5;
        state.loaded = true;
        state.pixels = (0..BOARD_WIDTH * BOARD_HEIGHT).map(|index| (index % 2) as u8 * 7).collect();
        let mut canvas = Canvas2D::new();
        assert!(compose_frame(&mut state, &mut canvas));
        assert!(canvas.len() <= 4 * 1024 * 1024);
        assert_eq!(loading_text(&state).as_deref(), state.truncated.then_some("Zoom in to see every pixel"));

        let mut state = State::new(1200.0, 800.0);
        state.pixels = (0..BOARD_WIDTH * BOARD_HEIGHT).map(|index| (index % 3) as u8).collect();
        let mut canvas = Canvas2D::new();
        assert!(compose_frame(&mut state, &mut canvas));
        assert!(!state.truncated);
    }

    #[test]
    fn flashes_fade_out_and_are_skipped_under_reduced_motion() {
        let mut state = ready_state();
        add_flash(&mut state, FlashKind::Remote, &[0, 1]);
        add_flash(&mut state, FlashKind::Remote, &[2]);
        assert_eq!(state.flashes.len(), 1);
        state.now = 5000.0;
        compose_frame(&mut state, &mut Canvas2D::new());
        assert_eq!(state.flashes[0].start, Some(5000.0));
        state.now = 5000.0 + REMOTE_FLASH_MS;
        compose_frame(&mut state, &mut Canvas2D::new());
        assert!(state.flashes.is_empty());

        state.reduced_motion = true;
        add_flash(&mut state, FlashKind::Local, &[0]);
        assert!(state.flashes.is_empty());
    }

    #[test]
    fn toasts_time_out() {
        let mut state = ready_state();
        show_toast(&mut state, "Hello");
        tick(&mut state, 2000.0);
        tick(&mut state, 2000.0 + TOAST_MS - 1.0);
        assert!(state.toast.is_some());
        tick(&mut state, 2000.0 + TOAST_MS);
        assert!(state.toast.is_none());
    }

    #[test]
    fn tile_codecs_reject_bad_data() {
        assert_eq!(decode_tile(&[]).unwrap(), vec![EMPTY; TILE_PIXELS]);
        assert!(decode_tile(&[0, 1, 32]).is_none());
        assert!(decode_tile(&[0, 0, 1]).is_none());
        assert!(decode_tile(&vec![200; TILE_PIXELS]).is_none());
        assert!(decode_base64("invalid!").is_none());
        assert_eq!(tile_of_index(tile_index(37, 999)), 37);
        assert_eq!(clock_text(60.0), "1:00");
        assert_eq!(clock_text(0.2), "0:01");
    }
}

// Host calls, and the failure type the exports return. A failure is the reply's `error` field, never a trap.

/// A failed host call or draw: the code and message the guest reports.
#[derive(Debug)]
struct Failed {
    code: String,
    message: String,
}

impl From<chittr_cartridge::canvas::CanvasError> for Failed {
    fn from(error: chittr_cartridge::canvas::CanvasError) -> Self {
        Self { code: error.code, message: error.message }
    }
}

/// The return of an export: `{}` on success, or the failure as its `error` field.
fn reply(result: Result<(), Failed>) -> CanvasReply {
    CanvasReply {
        error: result.err().map(|error| HookError { code: error.code, message: error.message }),
    }
}

/// The code of a generated enum, as its wire string.
fn code_of<T: Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "host".to_string())
}

/// The fields of a canvas event as one JSON object, so the handlers read them by name.
fn fields_of(input: &CanvasEventInput) -> Map<String, Value> {
    match serde_json::to_value(input) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn request_frame() -> Result<(), Failed> {
    chittr_cartridge::canvas::request_frame().map_err(Failed::from)
}

/// Sends one message to the Server Guest's `on_canvas_message`, with its sequence number.
fn send(message: Value, seq: u32) -> Result<(), Failed> {
    chittr_cartridge::canvas::send(message, Some(seq)).map_err(Failed::from)
}
