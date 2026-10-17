//! A shared pixel-art canvas. Canvas messages are accepted only from signed-in Observers;
//! the canvas contents are stored per Chitt and pushed to every viewer after a change.

mod pdk;

use extism_pdk::Error;
use pdk::types::*;
use push::PushTarget;
use serde_json::{json, Map, Value};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

const BOARD_KEY: &str = "painting";
const WIDTH: usize = 512;
const HEIGHT: usize = 512;
const FORMER_WIDTH: usize = 1024;
const FORMER_HEIGHT: usize = 1024;
const TOUCH_KEY_WIDTH: usize = FORMER_WIDTH;
const PREVIOUS_WIDTH: usize = 128;
const PREVIOUS_HEIGHT: usize = 72;
const LEGACY_WIDTH: usize = 64;
const LEGACY_HEIGHT: usize = 36;
const PIXELS: usize = WIDTH * HEIGHT;
const COLORS: u64 = 16 * 6;
const MAX_POINTS: usize = 64;
const CAS_ATTEMPTS: usize = 16;
const TOUCH_TILE_SIDE: usize = 16;
const TOUCH_TILES_PER_ROW: usize = WIDTH / TOUCH_TILE_SIDE;
const EMPTY: u8 = u8::MAX;
const BOARD_STORAGE_MAGIC: [u8; 3] = [0xfe, b'P', b'B'];
const BOARD_STORAGE_RLE: u8 = 1;
const BOARD_STORAGE_RAW: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: usize,
    y: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PixelTouch {
    name: String,
    trolltag: String,
}

thread_local! {
    static BOARD_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput { content: "A shared canvas for everyone.".to_string(), ..Default::default() })
}

pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    Ok(match render_card(&input) {
        Ok(output) => output,
        Err(error) => RenderOutput {
            template: String::new(),
            data: Map::new(),
            canvas: None,
            error: Some(error),
        },
    })
}

fn render_card(input: &RenderInput) -> Result<RenderOutput, HookError> {
    let board = read_board()?;
    Ok(RenderOutput {
        template: "card".to_string(),
        data: object(json!({ "signedIn": input.observer.is_some() })),
        canvas: Some(object(json!({
            "pixels": encode_board(&board),
            "canPaint": input.observer.is_some(),
        }))),
        error: None,
    })
}

pub(crate) fn on_canvas_message(input: CanvasMessageInput) -> Result<CanvasMessageOutput, Error> {
    Ok(match handle_message(input) {
        Ok(reply) => CanvasMessageOutput {
            reply: Some(reply),
            rerender: Some(false),
            error: None,
        },
        Err(error) => CanvasMessageOutput {
            reply: None,
            rerender: None,
            error: Some(error),
        },
    })
}

/// Serves a message: a pixel lookup (`info`), or a stroke from a signed-in Observer. Returns the reply.
fn handle_message(input: CanvasMessageInput) -> Result<Map<String, Value>, HookError> {
    let message = Value::Object(input.message.clone());
    if message.get("type").and_then(Value::as_str) == Some("info") {
        let point = parse_point(&message)?;
        let touch = read_touch(point)?;
        return Ok(object(match touch {
            Some(touch) => json!({
                "type": "pixel_info",
                "name": touch.name,
                "trolltag": touch.trolltag,
            }),
            None => json!({ "type": "pixel_info", "touched": false }),
        }));
    }

    let observer = require_observer(input.observer)?;
    let (color, points) = parse_paint(&message)?;
    let trolltag = Some(observer.trolltag.as_str())
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| HookError::new("bad_observer", "your Character has no trolltag"))?
        .to_string();
    let touch = PixelTouch {
        name: observer.name,
        trolltag,
    };
    let changed = paint(color, &points)?;
    record_touches(&points, &touch)?;
    if changed {
        push::push_render(PushTarget::All).map_err(kv_failure)?;
    }
    Ok(object(json!({ "painted": points.len() })))
}

fn parse_point(message: &Value) -> Result<Point, HookError> {
    let x = message
        .get("x")
        .and_then(Value::as_u64)
        .filter(|x| *x < WIDTH as u64)
        .ok_or_else(|| HookError::new("bad_point", "pixel is outside the canvas"))?
        as usize;
    let y = message
        .get("y")
        .and_then(Value::as_u64)
        .filter(|y| *y < HEIGHT as u64)
        .ok_or_else(|| HookError::new("bad_point", "pixel is outside the canvas"))?
        as usize;
    Ok(Point { x, y })
}

fn touch_key(point: Point) -> String {
    format!("touch:{}", point.y * TOUCH_KEY_WIDTH + point.x)
}

fn touch_tile(point: Point) -> (usize, usize) {
    let tile_x = point.x / TOUCH_TILE_SIDE;
    let tile_y = point.y / TOUCH_TILE_SIDE;
    let offset = (point.y % TOUCH_TILE_SIDE) * TOUCH_TILE_SIDE + point.x % TOUCH_TILE_SIDE;
    (tile_y * TOUCH_TILES_PER_ROW + tile_x, offset)
}

fn touch_tile_key(tile: usize) -> String {
    format!("touches:{tile}")
}

fn record_touches(points: &[Point], touch: &PixelTouch) -> Result<(), HookError> {
    let mut tiles: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut recorded = HashSet::new();
    for point in points {
        let pixel_index = point.y * WIDTH + point.x;
        if recorded.insert(pixel_index) {
            let (tile, offset) = touch_tile(*point);
            tiles.entry(tile).or_default().push(offset);
        }
    }
    for (tile, offsets) in tiles {
        let key = touch_tile_key(tile);
        let mut written = false;
        for _ in 0..CAS_ATTEMPTS {
            let previous = kv::get(&key).map_err(kv_failure)?;
            let mut touches = match &previous {
                Some(entry) => serde_json::from_slice::<Vec<Option<PixelTouch>>>(&entry.value)
                    .map_err(|error| HookError::new("invalid_touch", error.to_string()))?,
                None => vec![None; TOUCH_TILE_SIDE * TOUCH_TILE_SIDE],
            };
            if touches.len() != TOUCH_TILE_SIDE * TOUCH_TILE_SIDE {
                return Err(HookError::new(
                    "invalid_touch",
                    "saved pixel history has an invalid size",
                ));
            }
            for offset in &offsets {
                touches[*offset] = Some(touch.clone());
            }
            let value = serde_json::to_vec(&touches)
                .map_err(|error| HookError::new("invalid_touch", error.to_string()))?;
            let expected = previous.as_ref().map(|entry| entry.value.as_slice());
            if kv::compare_and_swap(&key, expected, value).map_err(kv_failure)? {
                written = true;
                break;
            }
        }
        if !written {
            return Err(HookError::new(
                "storage_busy",
                "pixel history is busy; try painting again",
            ));
        }
    }
    Ok(())
}

fn read_touch(point: Point) -> Result<Option<PixelTouch>, HookError> {
    let (tile, offset) = touch_tile(point);
    if let Some(entry) = kv::get(&touch_tile_key(tile)).map_err(kv_failure)? {
        let touches: Vec<Option<PixelTouch>> = serde_json::from_slice(&entry.value)
            .map_err(|error| HookError::new("invalid_touch", error.to_string()))?;
        if touches.len() != TOUCH_TILE_SIDE * TOUCH_TILE_SIDE {
            return Err(HookError::new(
                "invalid_touch",
                "saved pixel history has an invalid size",
            ));
        }
        if let Some(touch) = touches[offset].clone() {
            return Ok(Some(touch));
        }
    }
    let Some(entry) = kv::get(&touch_key(point)).map_err(kv_failure)? else {
        return Ok(None);
    };
    serde_json::from_slice(&entry.value)
        .map(Some)
        .map_err(|error| HookError::new("invalid_touch", error.to_string()))
}

fn require_observer(observer: Option<Observer>) -> Result<Observer, HookError> {
    let observer = observer
        .ok_or_else(|| HookError::new("logged_out", "log in to paint on the shared canvas"))?;
    if observer.character_id.is_empty() {
        return Err(HookError::new(
            "bad_observer",
            "a Character is required to paint",
        ));
    }
    Ok(observer)
}

fn parse_paint(message: &Value) -> Result<(u8, Vec<Point>), HookError> {
    if message.get("type").and_then(Value::as_str) != Some("paint") {
        return Err(HookError::new("bad_input", "expected a paint message"));
    }
    let color = message
        .get("color")
        .and_then(Value::as_u64)
        .filter(|color| *color < COLORS || *color == u64::from(EMPTY))
        .ok_or_else(|| HookError::new("bad_color", "choose a palette color or transparent"))?
        as u8;
    let coordinates = message
        .get("points")
        .and_then(Value::as_array)
        .filter(|points| {
            !points.is_empty() && points.len() <= MAX_POINTS * 2 && points.len() % 2 == 0
        })
        .ok_or_else(|| {
            HookError::new("bad_points", "a paint stroke must contain 1 to 64 points")
        })?;
    let mut points = Vec::with_capacity(coordinates.len() / 2);
    for pair in coordinates.chunks_exact(2) {
        let x = pair[0]
            .as_u64()
            .filter(|x| *x < WIDTH as u64)
            .ok_or_else(|| HookError::new("bad_point", "paint point is outside the canvas"))?
            as usize;
        let y = pair[1]
            .as_u64()
            .filter(|y| *y < HEIGHT as u64)
            .ok_or_else(|| HookError::new("bad_point", "paint point is outside the canvas"))?
            as usize;
        points.push(Point { x, y });
    }
    Ok((color, points))
}

fn paint(color: u8, points: &[Point]) -> Result<bool, HookError> {
    for _ in 0..CAS_ATTEMPTS {
        let previous = kv::get(BOARD_KEY).map_err(kv_failure)?;
        let (changed, swapped) = BOARD_BUFFER.with(|buffer| {
            let mut board = buffer.borrow_mut();
            match &previous {
                Some(entry) => decode_board_into(&entry.value, &mut board)?,
                None => {
                    board.clear();
                    board.resize(PIXELS, EMPTY);
                }
            }
            let mut changed = false;
            for point in points {
                let pixel = &mut board[point.y * WIDTH + point.x];
                if *pixel != color {
                    *pixel = color;
                    changed = true;
                }
            }
            if !changed {
                return Ok::<_, HookError>((false, false));
            }
            let expected = previous.as_ref().map(|entry| entry.value.as_slice());
            let encoded = encode_board_storage(&board);
            let swapped = kv::compare_and_swap(BOARD_KEY, expected, encoded).map_err(kv_failure)?;
            Ok((true, swapped))
        })?;
        if !changed {
            return Ok(false);
        }
        if swapped {
            return Ok(true);
        }
    }
    Err(HookError::new(
        "storage_busy",
        "the canvas is busy; try painting again",
    ))
}

fn read_board() -> Result<Vec<u8>, HookError> {
    match kv::get(BOARD_KEY).map_err(kv_failure)? {
        Some(entry) => decode_board(&entry.value),
        None => Ok(vec![EMPTY; PIXELS]),
    }
}

fn decode_board(bytes: &[u8]) -> Result<Vec<u8>, HookError> {
    let mut board = Vec::with_capacity(PIXELS);
    decode_board_into(bytes, &mut board)?;
    Ok(board)
}

fn decode_board_into(bytes: &[u8], board: &mut Vec<u8>) -> Result<(), HookError> {
    board.clear();
    if bytes.starts_with(&BOARD_STORAGE_MAGIC) {
        return match bytes.get(3).copied() {
            Some(BOARD_STORAGE_RLE) => decode_packed_board(&bytes[4..], board),
            Some(BOARD_STORAGE_RAW) if bytes.len() == PIXELS + 4 => {
                let pixels = &bytes[4..];
                validate_board_bytes(pixels)?;
                board.extend_from_slice(pixels);
                Ok(())
            }
            _ => Err(HookError::new(
                "invalid_canvas",
                "the saved canvas data is invalid",
            )),
        };
    }
    validate_board_bytes(bytes)?;
    if bytes.len() == PIXELS {
        board.extend_from_slice(bytes);
        return Ok(());
    }
    if bytes.len() == FORMER_WIDTH * FORMER_HEIGHT {
        board.resize(PIXELS, EMPTY);
        for y in 0..HEIGHT {
            let old_row = &bytes[y * FORMER_WIDTH..(y + 1) * FORMER_WIDTH];
            let new_row = &mut board[y * WIDTH..(y + 1) * WIDTH];
            new_row.copy_from_slice(&old_row[..WIDTH]);
        }
        return Ok(());
    }
    if bytes.len() == PREVIOUS_WIDTH * PREVIOUS_HEIGHT {
        board.resize(PIXELS, EMPTY);
        for y in 0..PREVIOUS_HEIGHT {
            let old_row = &bytes[y * PREVIOUS_WIDTH..(y + 1) * PREVIOUS_WIDTH];
            let new_row = &mut board[y * WIDTH..y * WIDTH + PREVIOUS_WIDTH];
            new_row.copy_from_slice(old_row);
        }
        return Ok(());
    }
    if bytes.len() == LEGACY_WIDTH * LEGACY_HEIGHT {
        board.resize(PIXELS, EMPTY);
        for y in 0..LEGACY_HEIGHT {
            let old_row = &bytes[y * LEGACY_WIDTH..(y + 1) * LEGACY_WIDTH];
            let new_row = &mut board[y * WIDTH..y * WIDTH + LEGACY_WIDTH];
            new_row.copy_from_slice(old_row);
        }
        return Ok(());
    }
    Err(HookError::new(
        "invalid_canvas",
        "the saved canvas data is invalid",
    ))
}

fn decode_packed_board(bytes: &[u8], board: &mut Vec<u8>) -> Result<(), HookError> {
    board.resize(PIXELS, EMPTY);
    let (mut cursor, mut previous_end) = (0, 0usize);
    while cursor < bytes.len() {
        let gap = read_varint(bytes, &mut cursor).ok_or_else(invalid_canvas_data)? as usize;
        let run = read_varint(bytes, &mut cursor).ok_or_else(invalid_canvas_data)? as usize;
        let color = *bytes.get(cursor).ok_or_else(invalid_canvas_data)?;
        cursor += 1;
        let start = previous_end
            .checked_add(gap)
            .ok_or_else(invalid_canvas_data)?;
        let end = start.checked_add(run).ok_or_else(invalid_canvas_data)?;
        if run == 0 || end > PIXELS || u64::from(color) >= COLORS {
            return Err(invalid_canvas_data());
        }
        board[start..end].fill(color);
        previous_end = end;
    }
    Ok(())
}

fn invalid_canvas_data() -> HookError {
    HookError::new("invalid_canvas", "the saved canvas data is invalid")
}

fn validate_board_bytes(bytes: &[u8]) -> Result<(), HookError> {
    if bytes
        .iter()
        .any(|pixel| *pixel != EMPTY && u64::from(*pixel) >= COLORS)
    {
        return Err(HookError::new(
            "invalid_canvas",
            "the saved canvas data is invalid",
        ));
    }
    Ok(())
}

fn encode_board(board: &[u8]) -> String {
    const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let packed = pack_board(board);
    let mut encoded = String::with_capacity(packed.len().div_ceil(3) * 4);
    for chunk in packed.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(BASE64[((value >> 18) & 63) as usize] as char);
        encoded.push(BASE64[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(BASE64[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(BASE64[(value & 63) as usize] as char);
        }
    }
    encoded
}

fn encode_board_storage(board: &[u8]) -> Vec<u8> {
    let header_len = BOARD_STORAGE_MAGIC.len() + 1;
    let packed = pack_board_limited(board, board.len().saturating_sub(header_len + 1));
    let (mode, payload) = match packed {
        Some(packed) => (BOARD_STORAGE_RLE, packed),
        None => (BOARD_STORAGE_RAW, board.to_vec()),
    };
    let mut stored = Vec::with_capacity(BOARD_STORAGE_MAGIC.len() + 1 + payload.len());
    stored.extend_from_slice(&BOARD_STORAGE_MAGIC);
    stored.push(mode);
    stored.extend_from_slice(&payload);
    stored
}

fn pack_board(board: &[u8]) -> Vec<u8> {
    pack_board_limited(board, usize::MAX).expect("unbounded board encoding cannot be limited")
}

fn pack_board_limited(board: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut packed = Vec::new();
    let mut index = 0;
    let mut previous_end = 0;
    while index < board.len() {
        if board[index] == EMPTY {
            index += 1;
            continue;
        }
        let color = board[index];
        let start = index;
        while index < board.len() && board[index] == color {
            index += 1;
        }
        if !write_varint_limited(&mut packed, (start - previous_end) as u32, limit)
            || !write_varint_limited(&mut packed, (index - start) as u32, limit)
            || packed.len() == limit
        {
            return None;
        }
        packed.push(color);
        previous_end = index;
    }
    Some(packed)
}

fn write_varint_limited(output: &mut Vec<u8>, mut value: u32, limit: usize) -> bool {
    while value >= 0x80 {
        if output.len() == limit {
            return false;
        }
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    if output.len() == limit {
        return false;
    }
    output.push(value as u8);
    true
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

fn kv_failure(error: kv::KvError) -> HookError {
    HookError::new(error.code, error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_valid_stroke_and_bounds_color_and_coordinates() {
        let message = json!({ "type": "paint", "color": 95, "points": [0, 0, 511, 511] });
        let (color, points) = parse_paint(&message).unwrap();
        assert_eq!(color, 95);
        assert_eq!(points, [Point { x: 0, y: 0 }, Point { x: 511, y: 511 }]);

        assert_eq!(
            parse_paint(&json!({ "type": "paint", "color": 96, "points": [0, 0] }))
                .unwrap_err()
                .code,
            "bad_color"
        );
        assert_eq!(
            parse_paint(&json!({ "type": "paint", "color": 255, "points": [0, 0] }))
                .unwrap()
                .0,
            EMPTY
        );
        assert_eq!(
            parse_paint(&json!({ "type": "paint", "color": 0, "points": [512, 0] }))
                .unwrap_err()
                .code,
            "bad_point"
        );
        assert_eq!(
            parse_paint(&json!({ "type": "paint", "color": 0, "points": [] }))
                .unwrap_err()
                .code,
            "bad_points"
        );
    }

    #[test]
    fn info_requests_validate_coordinates_and_build_stable_pixel_keys() {
        assert_eq!(
            parse_point(&json!({ "type": "info", "x": 511, "y": 511 })).unwrap(),
            Point { x: 511, y: 511 }
        );
        assert_eq!(
            parse_point(&json!({ "type": "info", "x": 512, "y": 0 }))
                .unwrap_err()
                .code,
            "bad_point"
        );
        assert_eq!(touch_key(Point { x: 1, y: 1 }), "touch:1025");
        assert_eq!(touch_tile(Point { x: 1, y: 1 }), (0, 17));
        assert_eq!(touch_tile(Point { x: 511, y: 511 }), (1023, 255));
        assert_eq!(touch_tile_key(1023), "touches:1023");
    }

    #[test]
    fn rejects_logged_out_paint_messages_before_storage_access() {
        assert_eq!(require_observer(None).unwrap_err().code, "logged_out");
    }

    #[test]
    fn sparse_board_encoding_keeps_white_black_palette_indices_and_fits_the_canvas_state() {
        let mut board = vec![EMPTY; PIXELS];
        board[1] = 0;
        board[2] = 95;
        let encoded = encode_board(&board);
        assert_eq!(encoded, "AQEAAAFf");
        assert!(
            serde_json::to_vec(&json!({ "pixels": encoded }))
                .unwrap()
                .len()
                < 16 * 1024
        );
        assert_eq!(encode_board(&vec![EMPTY; PIXELS]), "");
        assert_eq!(decode_board(&board).unwrap(), board);
        assert_eq!(decode_board(&[0; PIXELS]).unwrap().len(), PIXELS);
        assert_eq!(
            decode_board(&[0; PIXELS - 1]).unwrap_err().code,
            "invalid_canvas"
        );
    }

    #[test]
    fn expands_previous_boards_without_shifting_existing_pixels() {
        let mut previous = vec![EMPTY; PREVIOUS_WIDTH * PREVIOUS_HEIGHT];
        previous[PREVIOUS_WIDTH + 1] = 41;
        let expanded = decode_board(&previous).unwrap();
        assert_eq!(expanded.len(), PIXELS);
        assert_eq!(expanded[WIDTH + 1], 41);
        assert_eq!(expanded[WIDTH], EMPTY);
        assert_eq!(expanded[PREVIOUS_WIDTH + 1], EMPTY);

        let mut legacy = vec![EMPTY; LEGACY_WIDTH * LEGACY_HEIGHT];
        legacy[LEGACY_WIDTH + 1] = 42;
        let expanded = decode_board(&legacy).unwrap();
        assert_eq!(expanded.len(), PIXELS);
        assert_eq!(expanded[WIDTH + 1], 42);
        assert_eq!(expanded[WIDTH], EMPTY);
        assert_eq!(expanded[LEGACY_WIDTH + 1], EMPTY);
    }

    #[test]
    fn reduces_the_former_1024_board_to_512_without_shifting_retained_pixels() {
        let mut former = vec![EMPTY; FORMER_WIDTH * FORMER_HEIGHT];
        former[FORMER_WIDTH + 1] = 41;
        former[511 * FORMER_WIDTH + 511] = 42;
        former[511 * FORMER_WIDTH + 512] = 43;
        former[512 * FORMER_WIDTH] = 44;
        let reduced = decode_board(&former).unwrap();
        assert_eq!(reduced.len(), PIXELS);
        assert_eq!(reduced[WIDTH + 1], 41);
        assert_eq!(reduced[511 * WIDTH + 511], 42);
        assert_eq!(reduced[0], EMPTY);
    }

    #[test]
    fn stored_boards_use_compact_runs_and_preserve_legacy_and_dense_formats() {
        let mut sparse = vec![EMPTY; PIXELS];
        sparse[1] = 0;
        sparse[2] = 95;
        let stored = encode_board_storage(&sparse);
        assert_eq!(&stored[..3], &BOARD_STORAGE_MAGIC);
        assert_eq!(stored[3], BOARD_STORAGE_RLE);
        assert!(stored.len() < 16);
        assert_eq!(decode_board(&stored).unwrap(), sparse);

        let dense = (0..PIXELS)
            .map(|index| (index % 2) as u8)
            .collect::<Vec<_>>();
        let stored = encode_board_storage(&dense);
        assert_eq!(stored[3], BOARD_STORAGE_RAW);
        assert_eq!(decode_board(&stored).unwrap(), dense);

        let legacy = vec![95; PIXELS];
        assert_eq!(decode_board(&legacy).unwrap(), legacy);
    }
}

// Host calls. The board code reads the KV store and pushes renders through the two modules below, which wrap the
// generated bindings in the shapes the code was written against. A failure is the output's `error` field, never a trap.

impl HookError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// The code of a generated error enum, as its wire string.
fn code_of<T: serde::Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unavailable".to_string())
}

/// The KV store: a read and a compare-and-swap over the generated bindings.
mod kv {
    use super::pdk;
    use super::pdk::types::{KvCompareAndSwapInput, KvKeyInput};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use extism_pdk::Error;

    /// A KV failure: the host's code and message, or `unavailable` when the call itself failed.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct KvError {
        pub code: String,
        pub message: String,
    }

    /// A stored value.
    pub struct Entry {
        pub value: Vec<u8>,
    }

    fn failed(error: Error) -> KvError {
        KvError { code: "unavailable".to_string(), message: error.to_string() }
    }

    fn wire(error: pdk::types::KvError) -> KvError {
        KvError { code: super::code_of(&error.code), message: error.message }
    }

    fn invalid(message: &str) -> KvError {
        KvError { code: "invalid".to_string(), message: message.to_string() }
    }

    pub fn get(key: &str) -> Result<Option<Entry>, KvError> {
        let reply = pdk::kv_get(KvKeyInput { key: key.to_string() }).map_err(failed)?;
        if let Some(error) = reply.error {
            return Err(wire(error));
        }
        match reply.value {
            Some(value) => STANDARD
                .decode(value)
                .map(|value| Some(Entry { value }))
                .map_err(|_| invalid("a stored value is not base64")),
            None => Ok(None),
        }
    }

    pub fn compare_and_swap(key: &str, expected: Option<&[u8]>, value: impl AsRef<[u8]>) -> Result<bool, KvError> {
        let reply = pdk::kv_compare_and_swap(KvCompareAndSwapInput {
            key: key.to_string(),
            expected: expected.map(|bytes| STANDARD.encode(bytes)),
            value: STANDARD.encode(value),
        })
        .map_err(failed)?;
        if let Some(error) = reply.error {
            return Err(wire(error));
        }
        reply.swapped.ok_or_else(|| invalid("the host's reply is missing a field"))
    }
}

/// Render Pushes: the Observers of this Chitt are told that its card changed.
mod push {
    use super::kv::KvError;
    use super::pdk;
    use super::pdk::types::PushRenderInput;

    pub enum PushTarget {
        All,
    }

    pub fn push_render(_target: PushTarget) -> Result<(), KvError> {
        let reply = pdk::push_render(PushRenderInput { character_id: None }).map_err(|error| KvError {
            code: "unavailable".to_string(),
            message: error.to_string(),
        })?;
        match reply.error {
            Some(error) => Err(KvError { code: super::code_of(&error.code), message: error.message }),
            None => Ok(()),
        }
    }
}
