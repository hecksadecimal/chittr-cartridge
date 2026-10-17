//! Collaborative pixel painting. Strokes are optimistic locally and sent in small batches
//! to the Server Guest, which stores and broadcasts the shared board.

use std::cell::RefCell;
use std::collections::VecDeque;

// The exports, host calls and events are the bindings `xtp plugin init` generates from `canvas.xtp.yaml` (see the
// README); drawing goes through `chittr-cartridge`'s command-buffer encoder, `Canvas2D`.

mod pdk;

use chittr_cartridge::canvas::{rgba, Canvas2D, Composite, FillRule, LineCap, LineJoin, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde::Serialize;
use serde_json::{json, Map, Value};

const LOGICAL_WIDTH: f32 = 960.0;
const LOGICAL_HEIGHT: f32 = 480.0;
const BOARD_WIDTH: usize = 512;
const BOARD_HEIGHT: usize = 512;
const TOOLBAR_WIDTH: f32 = 160.0;
const CELL: f32 = 1.0;
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 16.0;
const DEFAULT_ZOOM: f32 = 0.87890625;
const KEY_PAN_STEP: f32 = 48.0;
const PALETTE_COLS: usize = 16;
const PALETTE_ROWS: usize = 6;
const VISIBLE_SHADES: usize = 4;
const SWATCH_COLUMNS: usize = 8;
const SWATCH_WIDTH: f32 = 14.0;
const SWATCH_HEIGHT: f32 = 12.0;
const SWATCH_GAP: f32 = 2.0;
const PALETTE_LEFT: f32 = 16.0;
const PALETTE_TOP: f32 = 138.0;
const EMPTY: u8 = u8::MAX;
const UNPAINTED_INDEX: u32 = u32::MAX;
const MESSAGE_POINTS: usize = 64;
const MAX_BRUSH_RADIUS: u8 = 4;
const RUN_RENDER_THRESHOLD: usize = 2048;

const BASE_COLORS: [[u8; 3]; PALETTE_COLS] = [
    [239, 68, 68],
    [249, 115, 22],
    [245, 158, 11],
    [234, 179, 8],
    [132, 204, 22],
    [34, 197, 94],
    [16, 185, 129],
    [20, 184, 166],
    [6, 182, 212],
    [14, 165, 233],
    [59, 130, 246],
    [99, 102, 241],
    [139, 92, 246],
    [168, 85, 247],
    [217, 70, 239],
    [244, 63, 94],
];

#[derive(Default)]
struct State {
    pixels: Vec<u8>,
    painted_pixels: Vec<u32>,
    painted_slots: Vec<u32>,
    can_paint: bool,
    selected: u8,
    brush_radius: u8,
    pan_tool: bool,
    info_tool: bool,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    width: f32,
    height: f32,
    active: bool,
    panning: bool,
    last_x: f32,
    last_y: f32,
    last_point: Option<u32>,
    preview_center: Option<u32>,
    info_message: Option<String>,
    info_sequence: Option<u32>,
    stroke: Vec<u32>,
    stroke_originals: Vec<(u32, u8)>,
    stroke_color: Option<u8>,
    pending: VecDeque<Vec<u32>>,
    in_flight: Option<u32>,
    acknowledged: VecDeque<(u64, Vec<u32>)>,
    state_revision: u64,
    sequence: u32,
    status: Option<String>,
    dirty: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    let state_value = input.state.clone().map(Value::Object).unwrap_or(Value::Null);
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.width = input.width as f32;
        state.height = input.height as f32;
        state.selected = 63;
        state.brush_radius = 1;
        state.zoom = DEFAULT_ZOOM;
        load_shared_state(&mut state, &state_value);
    });
    let result = STATE.with(|cell| draw(&mut cell.borrow_mut()));
    Ok(reply(result))
}

pub(crate) fn frame(_input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = STATE.with(|cell| draw(&mut cell.borrow_mut()));
    Ok(reply(result))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let fields = fields_of(&input);
    let kind = code_of(&input.r#type);
    let result = match kind.as_str() {
        "state" => {
            STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                state.state_revision = state.state_revision.wrapping_add(1);
                let revision = state.state_revision;
                state
                    .acknowledged
                    .retain(|(sent_revision, _)| *sent_revision >= revision);
                load_shared_state(&mut state, fields.get("state").unwrap_or(&Value::Null));
            });
            request_frame()
        }
        "resize" => {
            STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                state.width = number(&fields, "width");
                state.height = number(&fields, "height");
                state.dirty = true;
            });
            request_frame()
        }
        "pointer" => pointer(&fields),
        "key" => key(&fields),
        "reply" => {
            let info = fields.get("reply").and_then(pixel_info_caption);
            STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                let sequence = fields
                    .get("seq")
                    .and_then(Value::as_u64)
                    .map(|seq| seq as u32);
                if let Some(sequence) = sequence {
                    acknowledge_in_flight(&mut state, sequence);
                }
                if let Some(sequence) = sequence {
                    if state.info_sequence == Some(sequence as u32) {
                        state.info_sequence = None;
                        if let Some(info) = info {
                            state.info_message = Some(info);
                            state.dirty = true;
                        }
                    }
                }
                state.status = None;
            });
            pump_messages();
            request_frame()
        }
        "error" => {
            let code = fields
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("error");
            STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                let sequence = fields.get("seq").and_then(Value::as_u64);
                if sequence.is_some_and(|sequence| state.info_sequence == Some(sequence as u32)) {
                    state.info_sequence = None;
                    state.info_message = Some("Pixel history could not be loaded".to_string());
                } else if sequence.is_some_and(|sequence| state.in_flight == Some(sequence as u32))
                {
                    reject_in_flight(&mut state, sequence.unwrap() as u32);
                    state.status = Some(if code == "logged_out" {
                        "Log in to paint on the shared canvas".to_string()
                    } else {
                        format!("Paint could not be saved ({code})")
                    });
                } else {
                    state.status = Some(if code == "logged_out" {
                        "Log in to paint on the shared canvas".to_string()
                    } else {
                        format!("Paint could not be saved ({code})")
                    });
                }
                state.dirty = true;
            });
            pump_messages();
            request_frame()
        }
        _ => Ok(()),
    };
    Ok(reply(result))
}

fn pixel_info_caption(reply: &Value) -> Option<String> {
    if reply.get("type").and_then(Value::as_str) != Some("pixel_info") {
        return None;
    }
    match (
        reply.get("name").and_then(Value::as_str),
        reply.get("trolltag").and_then(Value::as_str),
    ) {
        (Some(name), Some(trolltag)) => Some(format!("Last touched by {name} (@{trolltag})")),
        _ if reply.get("touched").and_then(Value::as_bool) == Some(false) => {
            Some("No recorded pixel history".to_string())
        }
        _ => None,
    }
}

fn number(fields: &serde_json::Map<String, Value>, name: &str) -> f32 {
    fields.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn load_shared_state(state: &mut State, value: &Value) {
    let decoded = value
        .get("pixels")
        .and_then(Value::as_str)
        .and_then(decode_pixels);
    if let Some(pixels) = decoded {
        state.pixels = pixels;
        state.painted_pixels.clear();
        state.painted_slots = vec![UNPAINTED_INDEX; state.pixels.len()];
        for (index, pixel) in state.pixels.iter().enumerate() {
            if *pixel != EMPTY {
                state.painted_slots[index] = state.painted_pixels.len() as u32;
                state.painted_pixels.push(index as u32);
            }
        }
        replay_optimistic_pixels(state);
        state.can_paint = value
            .get("canPaint")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        state.status = None;
    } else {
        state.pixels = vec![EMPTY; BOARD_WIDTH * BOARD_HEIGHT];
        state.painted_pixels.clear();
        state.painted_slots = vec![UNPAINTED_INDEX; state.pixels.len()];
        state.can_paint = false;
        state.status = Some("Shared canvas data is unavailable".to_string());
    }
    state.dirty = true;
}

fn replay_optimistic_pixels(state: &mut State) {
    let batches: Vec<Vec<u32>> = state
        .acknowledged
        .iter()
        .map(|(_, batch)| batch.clone())
        .chain(state.pending.iter().cloned())
        .collect();
    for batch in &batches {
        replay_batch(state, batch);
    }
    let stroke_color = state.stroke_color.unwrap_or_else(|| paint_color(state));
    let stroke = state.stroke.clone();
    for point in stroke {
        set_pixel(state, point as usize, stroke_color);
    }
}

fn replay_batch(state: &mut State, batch: &[u32]) {
    if let Some((&color, points)) = batch.split_first() {
        for point in points {
            set_pixel(state, *point as usize, color as u8);
        }
    }
}

fn set_pixel(state: &mut State, index: usize, color: u8) -> bool {
    let Some(pixel) = state.pixels.get(index).copied() else {
        return false;
    };
    if pixel == color {
        return false;
    }
    match (pixel == EMPTY, color == EMPTY) {
        (true, false) => {
            state.painted_slots[index] = state.painted_pixels.len() as u32;
            state.painted_pixels.push(index as u32);
        }
        (false, true) => {
            let slot = state.painted_slots[index] as usize;
            let moved = state.painted_pixels.pop().unwrap();
            if slot < state.painted_pixels.len() {
                state.painted_pixels[slot] = moved;
                state.painted_slots[moved as usize] = slot as u32;
            }
            state.painted_slots[index] = UNPAINTED_INDEX;
        }
        _ => {}
    }
    state.pixels[index] = color;
    true
}

fn next_painted_run(
    pixels: &[u8],
    row_start: usize,
    x_start: usize,
    x_end: usize,
) -> Option<(u8, usize, usize)> {
    if x_start >= x_end {
        return None;
    }
    let row = pixels.get(row_start..row_start.checked_add(x_end)?)?;
    let mut x = x_start;
    while x < x_end && row[x] == EMPTY {
        x += 1;
    }
    if x == x_end {
        return None;
    }
    let run_start = x;
    let color = row[x];
    x += 1;
    while x < x_end && row[x] == color {
        x += 1;
    }
    Some((color, run_start, x))
}

fn decode_pixels(encoded: &str) -> Option<Vec<u8>> {
    let packed = decode_base64(encoded)?;
    let mut pixels = vec![EMPTY; BOARD_WIDTH * BOARD_HEIGHT];
    let (mut cursor, mut previous_end) = (0, 0usize);
    while cursor < packed.len() {
        let gap = read_varint(&packed, &mut cursor)? as usize;
        let run = read_varint(&packed, &mut cursor)? as usize;
        let color = *packed.get(cursor)?;
        cursor += 1;
        let start = previous_end.checked_add(gap)?;
        let end = start.checked_add(run)?;
        if run == 0 || end > pixels.len() || color >= (PALETTE_COLS * PALETTE_ROWS) as u8 {
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

fn board_origin(state: &State) -> (f32, f32) {
    let scale = CELL * state.zoom;
    (
        TOOLBAR_WIDTH
            + (LOGICAL_WIDTH - TOOLBAR_WIDTH - BOARD_WIDTH as f32 * scale) / 2.0
            + state.pan_x,
        (LOGICAL_HEIGHT - BOARD_HEIGHT as f32 * scale) / 2.0 + state.pan_y,
    )
}

fn visible_board_bounds(state: &State) -> (usize, usize, usize, usize) {
    let (left, top) = board_origin(state);
    let scale = CELL * state.zoom;
    let x_start = ((TOOLBAR_WIDTH - left) / scale)
        .floor()
        .clamp(0.0, BOARD_WIDTH as f32) as usize;
    let x_end = ((LOGICAL_WIDTH - left) / scale)
        .ceil()
        .clamp(0.0, BOARD_WIDTH as f32) as usize;
    let y_start = ((0.0 - top) / scale)
        .floor()
        .clamp(0.0, BOARD_HEIGHT as f32) as usize;
    let y_end = ((LOGICAL_HEIGHT - top) / scale)
        .ceil()
        .clamp(0.0, BOARD_HEIGHT as f32) as usize;
    (x_start, x_end, y_start, y_end)
}

fn board_index(state: &State, x: f32, y: f32) -> Option<usize> {
    let (left, top) = board_origin(state);
    let scale = CELL * state.zoom;
    let board_x = ((x - left) / scale).floor() as isize;
    let board_y = ((y - top) / scale).floor() as isize;
    if board_x < 0
        || board_y < 0
        || board_x >= BOARD_WIDTH as isize
        || board_y >= BOARD_HEIGHT as isize
    {
        return None;
    }
    Some(board_y as usize * BOARD_WIDTH + board_x as usize)
}

fn zoom_button(x: f32, y: f32) -> Option<ZoomAction> {
    if !(8.0..36.0).contains(&y) {
        return None;
    }
    if (LOGICAL_WIDTH - 108.0..LOGICAL_WIDTH - 80.0).contains(&x) {
        Some(ZoomAction::In)
    } else if (LOGICAL_WIDTH - 76.0..LOGICAL_WIDTH - 48.0).contains(&x) {
        Some(ZoomAction::Out)
    } else if (LOGICAL_WIDTH - 44.0..LOGICAL_WIDTH - 8.0).contains(&x) {
        Some(ZoomAction::Reset)
    } else {
        None
    }
}

enum ZoomAction {
    In,
    Out,
    Reset,
}

fn apply_zoom(state: &mut State, action: ZoomAction) {
    match action {
        ZoomAction::In => state.zoom = (state.zoom * 1.25).min(MAX_ZOOM),
        ZoomAction::Out => state.zoom = (state.zoom / 1.25).max(MIN_ZOOM),
        ZoomAction::Reset => {
            state.zoom = DEFAULT_ZOOM;
            state.pan_x = 0.0;
            state.pan_y = 0.0;
        }
    }
    state.dirty = true;
}

fn key(fields: &serde_json::Map<String, Value>) -> Result<(), Failed> {
    if fields.get("action").and_then(Value::as_str) != Some("down") {
        return Ok(());
    }
    let (pan_x, pan_y) = match fields.get("key").and_then(Value::as_str) {
        Some("ArrowLeft" | "a" | "A") => (KEY_PAN_STEP, 0.0),
        Some("ArrowRight" | "d" | "D") => (-KEY_PAN_STEP, 0.0),
        Some("ArrowUp" | "w" | "W") => (0.0, KEY_PAN_STEP),
        Some("ArrowDown" | "s" | "S") => (0.0, -KEY_PAN_STEP),
        _ => return Ok(()),
    };
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.pan_x += pan_x;
        state.pan_y += pan_y;
        state.dirty = true;
    });
    request_frame()
}

fn view_coordinates(state: &State) -> (i32, i32) {
    let (left, top) = board_origin(state);
    let scale = CELL * state.zoom;
    (
        ((TOOLBAR_WIDTH - left) / scale).floor() as i32,
        ((0.0 - top) / scale).floor() as i32,
    )
}

fn pointer(fields: &serde_json::Map<String, Value>) -> Result<(), Failed> {
    let action = fields.get("action").and_then(Value::as_str).unwrap_or("");
    let (x, y, css_width, css_height) = (
        number(fields, "x"),
        number(fields, "y"),
        number(fields, "width"),
        number(fields, "height"),
    );
    if css_width <= 0.0 || css_height <= 0.0 {
        return Ok(());
    }
    let (x, y) = (
        x * LOGICAL_WIDTH / css_width,
        y * LOGICAL_HEIGHT / css_height,
    );
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let mut info_request = None;
        match action {
            "down" => {
                state.active = false;
                state.panning = false;
                state.last_point = None;
                state.stroke.clear();
                state.stroke_originals.clear();
                state.stroke_color = None;
                if x < TOOLBAR_WIDTH {
                    state.preview_center = None;
                    if (32.0..62.0).contains(&y) {
                        if x < 53.0 {
                            state.pan_tool = false;
                            state.info_tool = false;
                        } else if x < 103.0 {
                            state.pan_tool = true;
                            state.info_tool = false;
                        } else {
                            activate_info_tool(&mut state);
                        }
                        state.dirty = true;
                    } else if let Some(radius) = radius_hit(x, y) {
                        select_brush_radius(&mut state, radius);
                        state.dirty = true;
                    } else if let Some(color) = palette_hit(x, y) {
                        state.selected = color;
                        state.pan_tool = false;
                        state.info_tool = false;
                        state.dirty = true;
                    } else if let Some(color) = special_color_hit(x, y) {
                        state.selected = color;
                        state.pan_tool = false;
                        state.info_tool = false;
                        state.dirty = true;
                    }
                } else if let Some(action) = zoom_button(x, y) {
                    state.preview_center = None;
                    apply_zoom(&mut state, action);
                } else if state.pan_tool {
                    state.preview_center = None;
                    state.panning = true;
                    state.last_x = x;
                    state.last_y = y;
                } else {
                    state.preview_center = board_index(&state, x, y).map(|index| index as u32);
                    if state.info_tool {
                        if let Some(index) = state.preview_center {
                            let seq = state.sequence;
                            state.sequence = state.sequence.wrapping_add(1);
                            state.info_message = Some("Looking up pixel history...".to_string());
                            info_request = Some((
                                index as usize % BOARD_WIDTH,
                                index as usize / BOARD_WIDTH,
                                seq,
                            ));
                        }
                        state.dirty = true;
                    } else if state.can_paint && state.preview_center.is_some() {
                        state.active = true;
                        state.stroke_color = Some(paint_color(&state));
                        add_point(&mut state, x, y);
                    } else if !state.can_paint {
                        state.status = Some("Log in to paint on the shared canvas".to_string());
                        state.dirty = true;
                    }
                }
            }
            "move" if fields.get("buttons").and_then(Value::as_u64).unwrap_or(0) > 0 => {
                if state.panning {
                    state.pan_x += x - state.last_x;
                    state.pan_y += y - state.last_y;
                    state.last_x = x;
                    state.last_y = y;
                    state.dirty = true;
                } else if state.active {
                    state.preview_center = board_index(&state, x, y).map(|index| index as u32);
                    add_point(&mut state, x, y);
                }
            }
            "move" => {
                if !state.active && !state.panning {
                    let preview = if x >= TOOLBAR_WIDTH && !state.pan_tool {
                        board_index(&state, x, y).map(|index| index as u32)
                    } else {
                        None
                    };
                    if state.preview_center != preview {
                        state.preview_center = preview;
                        state.dirty = true;
                    }
                }
            }
            "up" if state.panning => {
                state.panning = false;
            }
            "up" if state.active => {
                state.preview_center = board_index(&state, x, y).map(|index| index as u32);
                add_point(&mut state, x, y);
                state.active = false;
                if !state.stroke.is_empty() {
                    queue_stroke(&mut state);
                }
                state.stroke_originals.clear();
                state.stroke_color = None;
            }
            "up" => {
                if !state.pan_tool {
                    state.preview_center = board_index(&state, x, y).map(|index| index as u32);
                    state.dirty = true;
                }
            }
            "cancel" => {
                state.active = false;
                state.panning = false;
                state.last_point = None;
                state.preview_center = None;
                for (index, color) in std::mem::take(&mut state.stroke_originals) {
                    set_pixel(&mut state, index as usize, color);
                }
                state.stroke.clear();
                state.stroke_color = None;
                state.dirty = true;
            }
            _ => {}
        }
        if let Some((pixel_x, pixel_y, seq)) = info_request {
            match send(
                json!({ "type": "info", "x": pixel_x, "y": pixel_y }),
                Some(seq),
            ) {
                Ok(()) => state.info_sequence = Some(seq),
                Err(error) => {
                    state.info_message =
                        Some(format!("Pixel lookup could not start ({})", error.code));
                    state.dirty = true;
                    state.sequence = seq;
                }
            }
        }
    });
    pump_messages();
    request_frame()
}

fn add_point(state: &mut State, x: f32, y: f32) {
    let Some(target) = board_index(state, x, y).map(|index| index as u32) else {
        return;
    };
    let start = state.last_point.unwrap_or(target);
    state.last_point = Some(target);
    let color = state.stroke_color.unwrap_or_else(|| paint_color(state));
    for center in line_points(start, target) {
        for point in brush_points(center, state.brush_radius) {
            let index = point as usize;
            let previous = state.pixels.get(index).copied();
            if set_pixel(state, index, color) {
                state.dirty = true;
                if state.stroke.last() != Some(&point) {
                    if let Some(previous) = previous {
                        state.stroke_originals.push((point, previous));
                    }
                    state.stroke.push(point);
                }
            }
        }
    }
}

fn brush_points(center: u32, radius: u8) -> Vec<u32> {
    let center_x = (center % BOARD_WIDTH as u32) as i32;
    let center_y = (center / BOARD_WIDTH as u32) as i32;
    let radius = i32::from(radius.min(MAX_BRUSH_RADIUS));
    let radius_squared = radius * radius;
    let mut points = Vec::new();
    for y in center_y - radius..=center_y + radius {
        for x in center_x - radius..=center_x + radius {
            if x >= 0
                && y >= 0
                && x < BOARD_WIDTH as i32
                && y < BOARD_HEIGHT as i32
                && (x - center_x).pow(2) + (y - center_y).pow(2) <= radius_squared
            {
                points.push((y * BOARD_WIDTH as i32 + x) as u32);
            }
        }
    }
    points
}

fn line_points(start: u32, end: u32) -> Vec<u32> {
    let (mut x, mut y) = (
        (start % BOARD_WIDTH as u32) as i32,
        (start / BOARD_WIDTH as u32) as i32,
    );
    let (end_x, end_y) = (
        (end % BOARD_WIDTH as u32) as i32,
        (end / BOARD_WIDTH as u32) as i32,
    );
    let dx = (end_x - x).abs();
    let sx = if x < end_x { 1 } else { -1 };
    let dy = -(end_y - y).abs();
    let sy = if y < end_y { 1 } else { -1 };
    let mut error = dx + dy;
    let mut points = Vec::with_capacity(dx.max(-dy) as usize + 1);
    loop {
        points.push((y * BOARD_WIDTH as i32 + x) as u32);
        if x == end_x && y == end_y {
            break;
        }
        let doubled_error = 2 * error;
        if doubled_error >= dy {
            error += dy;
            x += sx;
        }
        if doubled_error <= dx {
            error += dx;
            y += sy;
        }
    }
    points
}

fn paint_color(state: &State) -> u8 {
    state.selected
}

fn palette_hit(x: f32, y: f32) -> Option<u8> {
    let pitch_x = SWATCH_WIDTH + SWATCH_GAP;
    let pitch_y = SWATCH_HEIGHT + SWATCH_GAP;
    if x < PALETTE_LEFT || y < PALETTE_TOP {
        return None;
    }
    let column = ((x - PALETTE_LEFT) / pitch_x).floor() as usize;
    let row = ((y - PALETTE_TOP) / pitch_y).floor() as usize;
    if column >= SWATCH_COLUMNS || row >= VISIBLE_SHADES * 2 {
        return None;
    }
    let family = (row / VISIBLE_SHADES) * SWATCH_COLUMNS + column;
    let shade = row % VISIBLE_SHADES + 1;
    Some((family * PALETTE_ROWS + shade) as u8)
}

fn special_color_hit(x: f32, y: f32) -> Option<u8> {
    let pitch_x = SWATCH_WIDTH + SWATCH_GAP;
    let pitch_y = SWATCH_HEIGHT + SWATCH_GAP;
    let special_row = VISIBLE_SHADES * 2;
    let row_top = PALETTE_TOP + special_row as f32 * pitch_y;
    if !(row_top..row_top + SWATCH_HEIGHT).contains(&y) || x < PALETTE_LEFT {
        return None;
    }
    let column = ((x - PALETTE_LEFT) / pitch_x).floor() as usize;
    if column >= 3 {
        return None;
    }
    [0, 5, EMPTY].get(column).copied()
}

fn radius_hit(x: f32, y: f32) -> Option<u8> {
    if !(88.0..112.0).contains(&y) || !(8.0..152.0).contains(&x) {
        return None;
    }
    let button = ((x - 8.0) / 29.0).floor() as u8;
    let left = 8.0 + f32::from(button) * 29.0;
    (button <= MAX_BRUSH_RADIUS && x < left + 26.0).then_some(button)
}

fn activate_info_tool(state: &mut State) {
    state.info_tool = true;
    state.brush_radius = 0;
    state.pan_tool = false;
    state.dirty = true;
}

fn select_brush_radius(state: &mut State, radius: u8) {
    state.brush_radius = radius;
    state.info_tool = false;
    state.pan_tool = false;
}

fn encode_points(color: u8, points: Vec<u32>) -> Vec<u32> {
    let mut batch = Vec::with_capacity(points.len() + 1);
    batch.push(u32::from(color));
    batch.extend(points);
    batch
}

fn queue_stroke(state: &mut State) {
    let Some(color) = state.stroke_color else {
        return;
    };
    let stroke = std::mem::take(&mut state.stroke);
    for chunk in stroke.chunks(MESSAGE_POINTS) {
        state
            .pending
            .push_back(encode_points(color, chunk.to_vec()));
    }
}

fn acknowledge_in_flight(state: &mut State, sequence: u32) -> bool {
    if state.in_flight != Some(sequence) {
        return false;
    }
    state.in_flight = None;
    if let Some(batch) = state.pending.pop_front() {
        state.acknowledged.push_back((state.state_revision, batch));
    }
    true
}

fn reject_in_flight(state: &mut State, sequence: u32) -> bool {
    if state.in_flight != Some(sequence) {
        return false;
    }
    state.in_flight = None;
    state.pending.pop_front();
    true
}

fn pump_messages() {
    if STATE.with(|cell| cell.borrow().in_flight.is_some()) {
        return;
    }
    loop {
        let next = STATE.with(|cell| cell.borrow().pending.front().cloned());
        let Some(batch) = next else {
            return;
        };
        let (color, points) = (batch[0], &batch[1..]);
        let mut coordinates = Vec::with_capacity(points.len() * 2);
        for point in points {
            let point = *point as usize;
            coordinates.push(point % BOARD_WIDTH);
            coordinates.push(point / BOARD_WIDTH);
        }
        let seq = STATE.with(|cell| cell.borrow().sequence);
        match send(
            json!({ "type": "paint", "color": color, "points": coordinates }),
            Some(seq),
        ) {
            Ok(()) => STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                state.in_flight = Some(seq);
                state.sequence = state.sequence.wrapping_add(1);
            }),
            Err(error) if error.code == "busy" => return,
            Err(error) => {
                STATE.with(|cell| {
                    let mut state = cell.borrow_mut();
                    state.status = Some(format!("Paint could not be sent ({})", error.code));
                    state.dirty = true;
                    state.pending.pop_front();
                });
            }
        }
    }
}

fn draw(state: &mut State) -> Result<(), Failed> {
    if !state.dirty || state.width <= 0.0 || state.height <= 0.0 {
        return Ok(());
    }
    state.dirty = false;
    let mut canvas = Canvas2D::new();
    begin_frame(
        &mut canvas,
        state.width / LOGICAL_WIDTH,
        state.height / LOGICAL_HEIGHT,
    );
    draw_toolbar(&mut canvas, state);

    canvas
        .save()
        .begin_path()
        .rect(
            TOOLBAR_WIDTH,
            0.0,
            LOGICAL_WIDTH - TOOLBAR_WIDTH,
            LOGICAL_HEIGHT,
        )
        .clip(FillRule::NonZero);
    let (left, top) = board_origin(state);
    let scale = CELL * state.zoom;
    canvas
        .save()
        .translate(left, top)
        .scale(scale, scale)
        .stroke_color(0x6b7280ff)
        .line_width(1.0)
        .stroke_rect(0.0, 0.0, BOARD_WIDTH as f32, BOARD_HEIGHT as f32);
    let (x_start, x_end, y_start, y_end) = visible_board_bounds(state);
    if state.painted_pixels.len() < RUN_RENDER_THRESHOLD {
        for index in &state.painted_pixels {
            let index = *index as usize;
            let x = index % BOARD_WIDTH;
            let y = index / BOARD_WIDTH;
            if x >= x_start && x < x_end && y >= y_start && y < y_end {
                canvas
                    .fill_color(palette_color(state.pixels[index]))
                    .fill_rect(x as f32, y as f32, 1.0, 1.0);
            }
        }
    } else {
        for y in y_start..y_end {
            let mut x = x_start;
            while let Some((color, run_start, end_x)) =
                next_painted_run(&state.pixels, y * BOARD_WIDTH, x, x_end)
            {
                canvas.fill_color(palette_color(color)).fill_rect(
                    run_start as f32,
                    y as f32,
                    (end_x - run_start) as f32,
                    1.0,
                );
                x = end_x;
            }
        }
    }
    draw_brush_preview(&mut canvas, state);
    canvas.restore();
    draw_view_coordinates(&mut canvas, state);
    draw_view_controls(&mut canvas, state);
    let hint = state
        .status
        .as_deref()
        .or_else(|| {
            state.info_tool.then_some(
                state
                    .info_message
                    .as_deref()
                    .unwrap_or("Click a pixel to inspect its history"),
            )
        })
        .or_else(|| (!state.can_paint).then_some("Log in to paint"));
    if let Some(hint) = hint {
        canvas
            .fill_color(0x374151ff)
            .font("sans-serif", 15.0, 500.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_text(
                hint,
                (LOGICAL_WIDTH + TOOLBAR_WIDTH) / 2.0,
                LOGICAL_HEIGHT - 20.0,
            );
    }
    canvas.restore();
    canvas.restore();
    canvas.submit().map_err(Failed::from)
}

fn draw_view_coordinates(canvas: &mut Canvas2D, state: &State) {
    let (x, y) = view_coordinates(state);
    canvas
        .fill_color(0x111827cc)
        .fill_rect(TOOLBAR_WIDTH + 10.0, 8.0, 112.0, 28.0);
    canvas
        .fill_color(0xffffffff)
        .font("sans-serif", 12.0, 600.0)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Middle)
        .fill_text(&format!("X {x}  Y {y}"), TOOLBAR_WIDTH + 17.0, 22.0);
}

fn draw_brush_preview(canvas: &mut Canvas2D, state: &State) {
    if state.pan_tool || state.info_tool {
        return;
    }
    let Some(center) = state.preview_center else {
        return;
    };
    canvas.begin_path();
    for (x1, y1, x2, y2) in brush_outline(center, state.brush_radius) {
        canvas.move_to(x1, y1).line_to(x2, y2);
    }
    canvas.stroke_color(0x000000ff).line_width(0.8).stroke();
    canvas.stroke_color(0xffffffff).line_width(0.35).stroke();
}

fn brush_outline(center: u32, radius: u8) -> Vec<(f32, f32, f32, f32)> {
    let points = brush_points(center, radius);
    let center_x = (center % BOARD_WIDTH as u32) as i32;
    let center_y = (center / BOARD_WIDTH as u32) as i32;
    let radius = i32::from(radius.min(MAX_BRUSH_RADIUS));
    let radius_squared = radius * radius;
    let mut edges = Vec::with_capacity(points.len() * 2);
    for point in points {
        let x = (point % BOARD_WIDTH as u32) as i32;
        let y = (point / BOARD_WIDTH as u32) as i32;
        let neighbors = [
            (x, y - 1, x as f32, y as f32, (x + 1) as f32, y as f32),
            (
                x + 1,
                y,
                (x + 1) as f32,
                y as f32,
                (x + 1) as f32,
                (y + 1) as f32,
            ),
            (
                x,
                y + 1,
                (x + 1) as f32,
                (y + 1) as f32,
                x as f32,
                (y + 1) as f32,
            ),
            (x - 1, y, x as f32, (y + 1) as f32, x as f32, y as f32),
        ];
        for (neighbor_x, neighbor_y, x1, y1, x2, y2) in neighbors {
            let neighbor_filled = neighbor_x >= 0
                && neighbor_y >= 0
                && neighbor_x < BOARD_WIDTH as i32
                && neighbor_y < BOARD_HEIGHT as i32
                && (neighbor_x - center_x).pow(2) + (neighbor_y - center_y).pow(2)
                    <= radius_squared;
            if !neighbor_filled {
                edges.push((x1, y1, x2, y2));
            }
        }
    }
    edges
}

fn begin_frame(canvas: &mut Canvas2D, scale_x: f32, scale_y: f32) {
    canvas.save().scale(scale_x, scale_y);
    canvas.clear_rect(0.0, 0.0, LOGICAL_WIDTH, LOGICAL_HEIGHT);
}

fn draw_toolbar(canvas: &mut Canvas2D, state: &State) {
    canvas
        .fill_color(0xe5e7ebff)
        .fill_rect(0.0, 0.0, TOOLBAR_WIDTH, LOGICAL_HEIGHT);
    canvas
        .stroke_color(0x9ca3afff)
        .line_width(1.0)
        .begin_path()
        .move_to(TOOLBAR_WIDTH, 0.0)
        .line_to(TOOLBAR_WIDTH, LOGICAL_HEIGHT)
        .stroke();

    canvas
        .fill_color(0x374151ff)
        .font("sans-serif", 13.0, 700.0)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Middle)
        .fill_text("TOOLS", 12.0, 17.0);

    for (x, width, label, selected) in [
        (5.0, 46.0, "Brush", !state.pan_tool && !state.info_tool),
        (55.0, 48.0, "Pan", state.pan_tool),
        (107.0, 48.0, "Info", state.info_tool),
    ] {
        canvas
            .fill_color(if selected { 0xbfdbfeff } else { 0xf9fafbff })
            .fill_rect(x, 32.0, width, 30.0);
        canvas
            .stroke_color(if selected { 0x2563ebff } else { 0x9ca3afff })
            .line_width(if selected { 2.0 } else { 1.0 })
            .stroke_rect(x + 0.5, 32.5, width - 1.0, 29.0);
        canvas
            .fill_color(0x111827ff)
            .font("sans-serif", 11.0, 600.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_text(label, x + width / 2.0, 47.0);
    }

    canvas
        .fill_color(0x374151ff)
        .font("sans-serif", 13.0, 700.0)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Middle)
        .fill_text("RADIUS", 12.0, 77.0);

    for radius in 0..=MAX_BRUSH_RADIUS {
        let x = 8.0 + f32::from(radius) * 29.0;
        let selected = state.brush_radius == radius;
        canvas
            .fill_color(if selected { 0xbfdbfeff } else { 0xf9fafbff })
            .fill_rect(x, 88.0, 26.0, 24.0);
        canvas
            .stroke_color(if selected { 0x2563ebff } else { 0x9ca3afff })
            .line_width(if selected { 2.0 } else { 1.0 })
            .stroke_rect(x + 0.5, 88.5, 25.0, 23.0);
        canvas
            .fill_color(0x111827ff)
            .font("sans-serif", 11.0, 600.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_text(&radius.to_string(), x + 13.0, 100.0);
    }

    canvas
        .fill_color(0x374151ff)
        .font("sans-serif", 13.0, 700.0)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Middle)
        .fill_text("COLORS", 12.0, 121.0);

    for family in 0..PALETTE_COLS {
        let family_column = family % SWATCH_COLUMNS;
        let family_row = family / SWATCH_COLUMNS;
        for shade in 1..=VISIBLE_SHADES {
            let index = (family * PALETTE_ROWS + shade) as u8;
            let x = PALETTE_LEFT + family_column as f32 * (SWATCH_WIDTH + SWATCH_GAP);
            let y = PALETTE_TOP
                + (family_row * VISIBLE_SHADES + shade - 1) as f32 * (SWATCH_HEIGHT + SWATCH_GAP);
            canvas
                .fill_color(palette_color(index))
                .fill_rect(x, y, SWATCH_WIDTH, SWATCH_HEIGHT);
            if index == state.selected {
                canvas.stroke_color(0x111827ff).line_width(2.0).stroke_rect(
                    x + 1.0,
                    y + 1.0,
                    SWATCH_WIDTH - 2.0,
                    SWATCH_HEIGHT - 2.0,
                );
            }
        }
    }

    let special_row = VISIBLE_SHADES * 2;
    let special_y = PALETTE_TOP + special_row as f32 * (SWATCH_HEIGHT + SWATCH_GAP);
    for (column, color) in [0, 5, EMPTY].into_iter().enumerate() {
        let x = PALETTE_LEFT + column as f32 * (SWATCH_WIDTH + SWATCH_GAP);
        if color == EMPTY {
            draw_checkerboard(canvas, x, special_y, SWATCH_WIDTH, SWATCH_HEIGHT, 4.0);
        } else {
            canvas.fill_color(palette_color(color)).fill_rect(
                x,
                special_y,
                SWATCH_WIDTH,
                SWATCH_HEIGHT,
            );
        }
        if color == state.selected {
            canvas.stroke_color(0x111827ff).line_width(2.0).stroke_rect(
                x + 1.0,
                special_y + 1.0,
                SWATCH_WIDTH - 2.0,
                SWATCH_HEIGHT - 2.0,
            );
        }
    }

    canvas
        .fill_color(0x9ca3afff)
        .fill_rect(10.0, 326.0, TOOLBAR_WIDTH - 20.0, 1.0);
    canvas
        .fill_color(0x374151ff)
        .font("sans-serif", 11.0, 600.0)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Middle)
        .fill_text("Selected color", 12.0, 344.0);
    if state.selected == EMPTY {
        draw_checkerboard(canvas, 12.0, 358.0, 26.0, 22.0, 7.0);
    } else {
        canvas
            .fill_color(palette_color(state.selected))
            .fill_rect(12.0, 358.0, 26.0, 22.0);
    }
    canvas
        .stroke_color(0x6b7280ff)
        .line_width(1.0)
        .stroke_rect(12.5, 358.5, 25.0, 21.0);
    canvas
        .fill_color(0x111827ff)
        .font("sans-serif", 11.0, 500.0)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Middle)
        .fill_text(
            if state.selected == EMPTY {
                "Transparent"
            } else {
                "Color"
            },
            46.0,
            369.0,
        );
}

fn draw_checkerboard(canvas: &mut Canvas2D, x: f32, y: f32, width: f32, height: f32, size: f32) {
    let columns = (width / size).ceil() as usize;
    let rows = (height / size).ceil() as usize;
    for row in 0..rows {
        for column in 0..columns {
            canvas
                .fill_color(if (row + column) % 2 == 0 {
                    0xffffffff
                } else {
                    0x9ca3afff
                })
                .fill_rect(
                    x + column as f32 * size,
                    y + row as f32 * size,
                    size.min(width - column as f32 * size),
                    size.min(height - row as f32 * size),
                );
        }
    }
}

fn draw_view_controls(canvas: &mut Canvas2D, state: &State) {
    for (x, width, label) in [
        (LOGICAL_WIDTH - 108.0, 28.0, "+"),
        (LOGICAL_WIDTH - 76.0, 28.0, "-"),
        (LOGICAL_WIDTH - 44.0, 36.0, "Reset"),
    ] {
        canvas.fill_color(0xe5e7ebee).fill_rect(x, 8.0, width, 28.0);
        canvas.stroke_color(0x6b7280ff).line_width(1.0).stroke_rect(
            x + 0.5,
            8.5,
            width - 1.0,
            27.0,
        );
        canvas
            .fill_color(0x111827ff)
            .font("sans-serif", 14.0, 600.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_text(label, x + width / 2.0, 22.0);
    }
    canvas
        .fill_color(0x374151cc)
        .font("sans-serif", 12.0, 600.0)
        .text_align(TextAlign::Right)
        .text_baseline(TextBaseline::Middle)
        .fill_text(
            &format!("{:.0}%", state.zoom * 100.0),
            LOGICAL_WIDTH - 116.0,
            22.0,
        );
}

fn palette_color(index: u8) -> u32 {
    let family = usize::from(index) / PALETTE_ROWS;
    let shade = usize::from(index) % PALETTE_ROWS;
    let base = BASE_COLORS[family];
    let rgb = match shade {
        0 => [255, 255, 255],
        1 => mix(base, [255, 255, 255], 0.68),
        2 => mix(base, [255, 255, 255], 0.32),
        3 => base,
        4 => mix(base, [0, 0, 0], 0.38),
        _ => [0, 0, 0],
    };
    rgba(rgb[0], rgb[1], rgb[2], 255)
}

fn mix(color: [u8; 3], target: [u8; 3], amount: f32) -> [u8; 3] {
    std::array::from_fn(|channel| {
        (f32::from(color[channel]) * (1.0 - amount) + f32::from(target[channel]) * amount).round()
            as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_four_color_shades_for_each_family_and_special_white_black() {
        assert_eq!(PALETTE_COLS * PALETTE_ROWS, 96);
        assert_eq!(VISIBLE_SHADES, 4);
        for family in 0..PALETTE_COLS {
            for shade in 1..=VISIBLE_SHADES {
                assert_ne!(
                    palette_color((family * PALETTE_ROWS + shade) as u8),
                    0xffffffff
                );
                assert_ne!(
                    palette_color((family * PALETTE_ROWS + shade) as u8),
                    0x000000ff
                );
            }
        }
        assert_eq!(palette_color(0), 0xffffffff);
        assert_eq!(palette_color(5), 0x000000ff);
    }

    #[test]
    fn transparent_is_selectable_as_a_brush_color() {
        assert_eq!(EMPTY, u8::MAX);
        assert_eq!(PALETTE_COLS * PALETTE_ROWS, 96);
        assert_eq!((BOARD_WIDTH, BOARD_HEIGHT), (512, 512));
        assert_eq!(LOGICAL_WIDTH - TOOLBAR_WIDTH, 800.0);
        assert_eq!(
            paint_color(&State {
                selected: EMPTY,
                ..State::default()
            }),
            EMPTY
        );
    }

    #[test]
    fn toolbar_palette_hit_test_selects_every_color_swatch() {
        for family in 0..PALETTE_COLS {
            for shade in 0..VISIBLE_SHADES {
                let x = PALETTE_LEFT
                    + (family % SWATCH_COLUMNS) as f32 * (SWATCH_WIDTH + SWATCH_GAP)
                    + 1.0;
                let y = PALETTE_TOP
                    + (family / SWATCH_COLUMNS * VISIBLE_SHADES + shade) as f32
                        * (SWATCH_HEIGHT + SWATCH_GAP)
                    + 1.0;
                assert_eq!(
                    palette_hit(x, y),
                    Some((family * PALETTE_ROWS + shade + 1) as u8)
                );
            }
        }
        assert_eq!(palette_hit(1.0, PALETTE_TOP), None);
        let special_y =
            PALETTE_TOP + (VISIBLE_SHADES * 2) as f32 * (SWATCH_HEIGHT + SWATCH_GAP) + 1.0;
        assert_eq!(special_color_hit(PALETTE_LEFT + 1.0, special_y), Some(0));
        assert_eq!(special_color_hit(PALETTE_LEFT + 17.0, special_y), Some(5));
        assert_eq!(
            special_color_hit(PALETTE_LEFT + 33.0, special_y),
            Some(EMPTY)
        );
        assert_eq!(special_color_hit(PALETTE_LEFT + 49.0, special_y), None);
        assert_eq!(
            special_color_hit(PALETTE_LEFT + 1.0, special_y + SWATCH_HEIGHT),
            None
        );
    }

    #[test]
    fn painting_is_limited_to_the_drawing_surface() {
        let state = State {
            zoom: 1.0,
            ..State::default()
        };
        let (left, top) = board_origin(&state);
        assert_eq!(board_index(&state, left, top), Some(0));
        assert_eq!(
            board_index(
                &state,
                left + BOARD_WIDTH as f32 * CELL - 0.1,
                top + BOARD_HEIGHT as f32 * CELL - 0.1
            ),
            Some(BOARD_WIDTH * BOARD_HEIGHT - 1)
        );
        assert_eq!(board_index(&state, TOOLBAR_WIDTH - 1.0, top), None);
        assert_eq!(board_index(&state, left - 0.1, top), None);
    }

    #[test]
    fn visible_board_bounds_clip_to_the_drawing_viewport() {
        let state = State {
            zoom: 1.0,
            ..State::default()
        };
        assert_eq!(visible_board_bounds(&state), (0, 512, 16, 496));
        let zoomed = State {
            zoom: 2.0,
            ..State::default()
        };
        assert_eq!(visible_board_bounds(&zoomed), (56, 456, 136, 376));
    }

    #[test]
    fn tracks_visible_pixels_and_removes_erased_pixels_from_the_render_list() {
        let mut state = State {
            pixels: vec![EMPTY; 8],
            painted_slots: vec![UNPAINTED_INDEX; 8],
            ..State::default()
        };
        assert!(set_pixel(&mut state, 2, 3));
        assert!(set_pixel(&mut state, 5, 10));
        assert!(set_pixel(&mut state, 2, 4));
        assert_eq!(state.painted_pixels, [2, 5]);

        assert!(set_pixel(&mut state, 2, EMPTY));
        assert_eq!(state.painted_pixels, [5]);
        assert_eq!(state.painted_slots[5], 0);
        assert_eq!(state.painted_slots[2], UNPAINTED_INDEX);
        assert!(!set_pixel(&mut state, 2, EMPTY));
        assert!(set_pixel(&mut state, 5, EMPTY));
        assert!(state.painted_pixels.is_empty());
    }

    #[test]
    fn render_groups_adjacent_pixels_of_the_same_color_and_skips_transparent_pixels() {
        let pixels = [EMPTY, 3, 3, 5, EMPTY, 5, EMPTY];
        let mut x = 0;
        let mut runs = Vec::new();
        while let Some((color, run_start, end_x)) = next_painted_run(&pixels, 0, x, pixels.len()) {
            runs.push((color, run_start, end_x));
            x = end_x;
        }
        assert_eq!(runs, [(3, 1, 3), (5, 3, 4), (5, 5, 6)]);
        assert_eq!(next_painted_run(&pixels, 0, 6, pixels.len()), None);
        assert_eq!(
            next_painted_run(&pixels, pixels.len(), 0, pixels.len()),
            None
        );
    }

    #[test]
    fn a_stroke_is_split_into_server_sized_batches_only_when_queued() {
        let mut state = State {
            stroke_color: Some(EMPTY),
            stroke: (0..130).collect(),
            ..State::default()
        };
        assert!(state.pending.is_empty());
        queue_stroke(&mut state);
        assert_eq!(
            state
                .pending
                .iter()
                .map(|batch| batch.len() - 1)
                .collect::<Vec<_>>(),
            [64, 64, 2]
        );
        assert!(state
            .pending
            .iter()
            .all(|batch| batch[0] == u32::from(EMPTY)));
        assert!(state.stroke.is_empty());
    }

    #[test]
    fn incoming_shared_state_keeps_unconfirmed_batches_and_active_stroke_optimistic() {
        let mut state = State {
            pixels: vec![EMPTY; BOARD_WIDTH * BOARD_HEIGHT],
            painted_slots: vec![UNPAINTED_INDEX; BOARD_WIDTH * BOARD_HEIGHT],
            stroke: vec![2],
            stroke_color: Some(4),
            ..State::default()
        };
        state.pending.push_back(encode_points(0, vec![0, 1]));
        state.in_flight = Some(7);
        load_shared_state(&mut state, &json!({ "pixels": "", "canPaint": true }));
        assert_eq!(&state.pixels[..3], &[0, 0, 4]);
        assert_eq!(state.in_flight, Some(7));

        assert!(acknowledge_in_flight(&mut state, 7));
        assert_eq!(state.pending.len(), 0);
        assert_eq!(state.acknowledged.len(), 1);
        assert!(!acknowledge_in_flight(&mut state, 8));
        state.state_revision += 1;
        state
            .acknowledged
            .retain(|(revision, _)| *revision >= state.state_revision);
        load_shared_state(&mut state, &json!({ "pixels": "AAIA", "canPaint": true }));
        assert_eq!(state.pixels[1], 0);
    }

    #[test]
    fn acknowledged_batches_wait_for_the_next_shared_state_before_being_discarded() {
        let mut state = State::default();
        state.pending.push_back(encode_points(EMPTY, vec![3]));
        state.in_flight = Some(12);
        assert!(acknowledge_in_flight(&mut state, 12));
        assert_eq!(state.acknowledged.len(), 1);
        assert!(!reject_in_flight(&mut state, 12));

        state.state_revision += 1;
        state
            .acknowledged
            .retain(|(revision, _)| *revision >= state.state_revision);
        assert!(state.acknowledged.is_empty());
    }

    #[test]
    fn zoom_and_pan_change_the_visible_board_coordinates() {
        let mut state = State {
            zoom: 1.0,
            ..State::default()
        };
        let (left, top) = board_origin(&state);
        let center = (left + CELL / 2.0, top + CELL / 2.0);
        assert_eq!(board_index(&state, center.0, center.1), Some(0));
        state.zoom = 2.0;
        let zoomed_origin = board_origin(&state);
        assert_eq!(
            board_index(&state, zoomed_origin.0 + 1.0, zoomed_origin.1 + 1.0),
            Some(0)
        );
        state.pan_x += 20.0;
        assert_eq!(
            board_index(&state, zoomed_origin.0 + 20.0, zoomed_origin.1 + 1.0),
            Some(0)
        );
        apply_zoom(&mut state, ZoomAction::In);
        assert_eq!(state.zoom, 2.5);
        for _ in 0..20 {
            apply_zoom(&mut state, ZoomAction::In);
        }
        assert_eq!(state.zoom, MAX_ZOOM);
        for _ in 0..20 {
            apply_zoom(&mut state, ZoomAction::Out);
        }
        assert_eq!(state.zoom, MIN_ZOOM);
    }

    #[test]
    fn default_and_reset_zoom_match_three_zoom_in_clicks() {
        let mut expected: f32 = 0.45;
        for _ in 0..3 {
            expected *= 1.25;
        }
        assert_eq!(DEFAULT_ZOOM, expected);

        let mut state = State {
            zoom: 1.0,
            ..State::default()
        };
        apply_zoom(&mut state, ZoomAction::Reset);
        assert_eq!(state.zoom, expected);
    }

    #[test]
    fn arrow_and_wasd_keys_pan_without_changing_the_selected_tool() {
        for (key_name, expected_x, expected_y) in [
            ("ArrowLeft", KEY_PAN_STEP, 0.0),
            ("a", KEY_PAN_STEP, 0.0),
            ("ArrowRight", -KEY_PAN_STEP, 0.0),
            ("d", -KEY_PAN_STEP, 0.0),
            ("ArrowUp", 0.0, KEY_PAN_STEP),
            ("w", 0.0, KEY_PAN_STEP),
            ("ArrowDown", 0.0, -KEY_PAN_STEP),
            ("s", 0.0, -KEY_PAN_STEP),
        ] {
            let state_before = STATE.with(|cell| {
                let mut state = cell.borrow_mut();
                state.pan_x = 0.0;
                state.pan_y = 0.0;
                state.pan_tool = false;
                state.info_tool = false;
                state.dirty = false;
                (state.pan_x, state.pan_y, state.pan_tool)
            });
            assert_eq!(state_before, (0.0, 0.0, false));
            let fields = serde_json::from_value::<Value>(json!({
                "action": "down",
                "key": key_name,
            }))
            .unwrap();
            key(fields.as_object().unwrap()).unwrap();
            STATE.with(|cell| {
                let state = cell.borrow();
                assert_eq!((state.pan_x, state.pan_y), (expected_x, expected_y));
                assert!(!state.pan_tool);
            });
        }
        let state = State {
            zoom: DEFAULT_ZOOM,
            ..State::default()
        };
        assert_eq!(view_coordinates(&state), (-377, -22));
    }

    #[test]
    fn one_drag_interpolates_every_cell_between_pointer_samples() {
        let line = line_points(0, (5 * BOARD_WIDTH + 5) as u32);
        assert_eq!(
            line,
            (0..=5)
                .map(|coordinate| (coordinate * BOARD_WIDTH + coordinate) as u32)
                .collect::<Vec<_>>()
        );
        assert_eq!(line_points(7, 2), vec![7, 6, 5, 4, 3, 2]);
        assert_eq!(line_points(3, 3), vec![3]);
    }

    #[test]
    fn brush_radius_creates_a_circular_footprint_clipped_at_board_edges() {
        assert_eq!(brush_points(0, 0), vec![0]);
        assert_eq!(brush_points(5 * BOARD_WIDTH as u32 + 5, 1).len(), 5);
        assert_eq!(brush_points(0, 1), vec![0, 1, BOARD_WIDTH as u32]);
        assert_eq!(
            brush_points((BOARD_WIDTH * BOARD_HEIGHT - 1) as u32, 0),
            vec![(BOARD_WIDTH * BOARD_HEIGHT - 1) as u32]
        );
        assert!(brush_points(0, u8::MAX)
            .iter()
            .all(|point| (*point as usize) < BOARD_WIDTH * BOARD_HEIGHT));
        assert_eq!(
            brush_points(0, u8::MAX).len(),
            brush_points(0, MAX_BRUSH_RADIUS).len()
        );
    }

    #[test]
    fn brush_preview_outlines_only_the_impact_area() {
        assert_eq!(brush_outline(0, 0).len(), 4);
        assert_eq!(brush_outline((10 * BOARD_WIDTH + 10) as u32, 1).len(), 12);
        assert_eq!(brush_outline(0, 1).len(), 8);
    }

    #[test]
    fn radius_toolbar_selects_each_supported_radius_and_ignores_button_gaps() {
        for radius in 0..=MAX_BRUSH_RADIUS {
            let x = 8.0 + f32::from(radius) * 29.0 + 1.0;
            assert_eq!(radius_hit(x, 100.0), Some(radius));
        }
        assert_eq!(radius_hit(35.0, 100.0), None);
        assert_eq!(radius_hit(10.0, 87.0), None);
    }

    #[test]
    fn changing_brush_radius_keeps_the_selected_transparent_color() {
        let mut state = State {
            selected: EMPTY,
            brush_radius: 0,
            ..State::default()
        };
        select_brush_radius(&mut state, 3);
        assert_eq!(state.brush_radius, 3);
        assert_eq!(paint_color(&state), EMPTY);
    }

    #[test]
    fn info_tool_forces_radius_zero_and_formats_pixel_history_replies() {
        let mut state = State {
            brush_radius: MAX_BRUSH_RADIUS,
            pan_tool: true,
            ..State::default()
        };
        activate_info_tool(&mut state);
        assert!(state.info_tool);
        assert_eq!(state.brush_radius, 0);
        assert!(!state.pan_tool);
        assert_eq!(
            pixel_info_caption(&json!({
                "type": "pixel_info",
                "name": "Ada",
                "trolltag": "ada",
            }))
            .as_deref(),
            Some("Last touched by Ada (@ada)")
        );
        assert_eq!(
            pixel_info_caption(&json!({ "type": "pixel_info", "touched": false })).as_deref(),
            Some("No recorded pixel history")
        );
        assert_eq!(pixel_info_caption(&json!({ "painted": 3 })), None);
    }

    #[test]
    fn decodes_the_compact_shared_board_state() {
        let black = decode_pixels("AAEA").unwrap();
        assert_eq!(black.len(), BOARD_WIDTH * BOARD_HEIGHT);
        assert_eq!(black[0], 0);
        assert_eq!(black[1], EMPTY);
        let separated_runs = decode_pixels("AAEAAgFf").unwrap();
        assert_eq!(separated_runs[0], 0);
        assert_eq!(separated_runs[1], EMPTY);
        assert_eq!(separated_runs[2], EMPTY);
        assert_eq!(separated_runs[3], 95);
        let server_encoded = decode_pixels("AQEAAAFf").unwrap();
        assert_eq!(server_encoded[0], EMPTY);
        assert_eq!(server_encoded[1], 0);
        assert_eq!(server_encoded[2], 95);
        let transparent = decode_pixels("").unwrap();
        assert!(transparent.iter().all(|pixel| *pixel == EMPTY));
        assert!(decode_pixels("gAEBAA").is_none());
        assert!(decode_pixels("invalid").is_none());
        assert!(decode_pixels("_____w").is_none());
    }

    #[test]
    fn each_redraw_clears_the_full_scaled_canvas_before_repainting() {
        let mut canvas = Canvas2D::new();
        begin_frame(&mut canvas, 2.0, 2.0);
        let bytes = canvas.bytes();
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[1], 0x07);
        assert_eq!(f32::from_le_bytes(bytes[2..6].try_into().unwrap()), 2.0);
        assert_eq!(f32::from_le_bytes(bytes[6..10].try_into().unwrap()), 2.0);
        assert_eq!(bytes[10], 0x22);
        assert_eq!(bytes.len(), 27);
        assert_eq!(f32::from_le_bytes(bytes[11..15].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(bytes[15..19].try_into().unwrap()), 0.0);
        assert_eq!(
            f32::from_le_bytes(bytes[19..23].try_into().unwrap()),
            LOGICAL_WIDTH
        );
        assert_eq!(
            f32::from_le_bytes(bytes[23..27].try_into().unwrap()),
            LOGICAL_HEIGHT
        );
    }

    #[test]
    fn decodes_a_full_shared_board() {
        assert_eq!(decode_pixels("").unwrap().len(), BOARD_WIDTH * BOARD_HEIGHT);
    }
}

// Host calls, and the failure type the exports return. A failure is the reply's `error` field, never a trap.

/// A failed host call or draw: the code and message the guest reports.
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

/// The fields of a canvas event as one JSON object, so the handlers read them by name.
fn fields_of(input: &CanvasEventInput) -> Map<String, Value> {
    match serde_json::to_value(input) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn set_fps(fps: i32) -> Result<(), Failed> {
    checked(pdk::set_fps(SetFpsInput { fps }).map_err(host_failed)?.error)
}

fn request_frame() -> Result<(), Failed> {
    checked(pdk::request_frame().map_err(host_failed)?.error)
}

/// Sends one message to the Server Guest's `on_canvas_message`, with its sequence number.
fn send(message: Value, seq: Option<u32>) -> Result<(), Failed> {
    let message = match message {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    checked(pdk::send(CanvasSendInput { message, seq: seq.map(|seq| seq as i32) }).map_err(host_failed)?.error)
}
