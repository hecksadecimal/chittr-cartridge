//! Place: a shared pixel board in the style of r/place. A signed-in Observer places one pixel, then waits out a
//! cooldown. The board is kept in 32-by-32 tiles, each stamped with when it last changed, so a placement rewrites one
//! tile and a render sends the most recently changed tiles first. The canvas fetches any tile the render had no room
//! for.

mod pdk;

use chittr_cartridge::kv;
use chittr_cartridge::push::{push_render, PushTarget};
use extism_pdk::Error;
use pdk::types::*;
use serde_json::{json, Map, Value};

const WIDTH: usize = 512;
const HEIGHT: usize = 512;
const TILE_SIDE: usize = 32;
const TILES_PER_ROW: usize = WIDTH / TILE_SIDE;
const TILE_COUNT: usize = TILES_PER_ROW * (HEIGHT / TILE_SIDE);
const TILE_PIXELS: usize = TILE_SIDE * TILE_SIDE;
/// The r/place 2022 palette's size. The canvas holds the colors themselves.
const COLORS: u64 = 32;
/// An unplaced pixel. The canvas shows it as white, as r/place starts out.
const EMPTY: u8 = u8::MAX;
/// Seconds between one Character's placements.
const COOLDOWN_SECONDS: u64 = 15;
/// Tiles one fetch may ask for.
const MAX_FETCH: usize = 32;
const CAS_ATTEMPTS: usize = 16;
/// The counter every placement draws its stamp from.
const STAMP_KEY: &str = "place:stamp";
/// Inline tile data a render may carry. The canvas state is capped at 16 KiB of JSON, and the stamps and the
/// host's own fields need the rest.
const STATE_TILE_BUDGET: usize = 13 * 1024;
/// Tile data one fetch reply may carry, under the 4 KiB reply cap.
const REPLY_TILE_BUDGET: usize = 3 * 1024;
/// The base64url alphabet the canvas decodes, unpadded.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: usize,
    y: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PixelTouch {
    name: String,
    trolltag: String,
}

/// A tile as stored and sent: its stamp and its wire data (see `encode_tile`). `None` is a tile nobody has placed on.
type StoredTile = Option<(u32, Vec<u8>)>;

/// One stored tile: the stamp of its last change, and its pixels in row order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tile {
    stamp: u32,
    pixels: Vec<u8>,
}

impl Tile {
    fn empty() -> Self {
        Self { stamp: 0, pixels: vec![EMPTY; TILE_PIXELS] }
    }
}

pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput {
        content: "A shared pixel board. Place a pixel every 15 seconds, and make something together.".to_string(),
        ..Default::default()
    })
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
    let tiles = read_stored_tiles()?;
    let mut state = canvas_state(&tiles);
    match &input.observer {
        Some(observer) => {
            state.insert("canPlace".to_string(), json!(!observer.character_id.is_empty()));
            state.insert("cooldown".to_string(), json!(cooldown_left(&observer.character_id)?));
            // Only admins are told they are, so only their canvas draws the cooldown switch.
            if observer.is_admin {
                state.insert("admin".to_string(), json!(true));
            }
        }
        None => {
            state.insert("canPlace".to_string(), json!(false));
        }
    }
    Ok(RenderOutput {
        template: "card".to_string(),
        data: object(json!({ "signedIn": input.observer.is_some() })),
        canvas: Some(state),
        error: None,
    })
}

/// The canvas state: every tile's stamp, and the data of as many tiles as fit, most recently changed first. A tile
/// whose data is absent is either unchanged since the canvas last saw it or one the canvas fetches.
fn canvas_state(tiles: &[StoredTile]) -> Map<String, Value> {
    let mut stamps = Vec::with_capacity(TILE_COUNT * 4);
    for tile in tiles {
        stamps.extend_from_slice(&tile.as_ref().map_or(0, |(stamp, _)| *stamp).to_le_bytes());
    }
    let mut order: Vec<usize> = (0..tiles.len()).filter(|index| tiles[*index].is_some()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(tiles[*index].as_ref().map_or(0, |(stamp, _)| *stamp)));
    let mut inline = Vec::new();
    let mut budget = STATE_TILE_BUDGET;
    for index in order {
        let Some((_, data)) = &tiles[index] else { continue };
        let cost = encoded_len(data.len()) + 10;
        if cost > budget {
            continue;
        }
        budget -= cost;
        inline.push(json!([index, encode_base64(data)]));
    }
    object(json!({
        "stamps": encode_base64(&stamps),
        "tiles": inline,
    }))
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

/// Serves a message: a pixel lookup (`info`), a tile fetch (`tiles`), or a placement from a signed-in Observer
/// (`place`).
fn handle_message(input: CanvasMessageInput) -> Result<Map<String, Value>, HookError> {
    let message = Value::Object(input.message);
    match message.get("type").and_then(Value::as_str) {
        Some("info") => {
            let point = parse_point(&message)?;
            Ok(object(match read_touch(point)? {
                Some(touch) => json!({
                    "type": "pixel_info",
                    "name": touch.name,
                    "trolltag": touch.trolltag,
                }),
                None => json!({ "type": "pixel_info", "touched": false }),
            }))
        }
        Some("tiles") => fetch_tiles(&message),
        Some("place") => {
            let observer = require_observer(input.observer)?;
            let point = parse_point(&message)?;
            let color = parse_color(&message)?;
            let trolltag = Some(observer.trolltag.as_str())
                .filter(|tag| !tag.is_empty())
                .ok_or_else(|| HookError::new("bad_observer", "your Character has no trolltag"))?
                .to_string();
            // Admins may skip the cooldown with the canvas's switch. The flag means nothing from anyone else.
            let skip_cooldown =
                observer.is_admin && message.get("skipCooldown").and_then(Value::as_bool) == Some(true);
            let touch = PixelTouch { name: observer.name, trolltag };
            place_pixel(&observer.character_id, point, color, &touch, skip_cooldown)
        }
        _ => Err(HookError::new("bad_input", "unknown message")),
    }
}

/// Places one pixel if the Character's cooldown has run out, or at once when `skip_cooldown` (an admin's choice). The
/// cooldown is claimed first, so two placements at once can't both get through, and handed back when nothing ends up
/// placed. A skipped cooldown is neither checked nor started, and any cooldown already running is left as it is.
fn place_pixel(
    character_id: &str,
    point: Point,
    color: u8,
    touch: &PixelTouch,
    skip_cooldown: bool,
) -> Result<Map<String, Value>, HookError> {
    let key = cooldown_key(character_id);
    if !skip_cooldown && !kv::set_if_absent(&key, b"1", Some(COOLDOWN_SECONDS)).map_err(kv_failure)? {
        return Ok(object(json!({ "type": "cooldown", "wait": cooldown_left(character_id)? })));
    }
    let hand_back = || if skip_cooldown { Ok(()) } else { kv::delete(&key).map(|_| ()) };
    let written = match paint(color, point, touch) {
        Ok(written) => written,
        Err(error) => {
            let _ = hand_back();
            return Err(error);
        }
    };
    let Some((tile, stamp)) = written else {
        hand_back().map_err(kv_failure)?;
        return Ok(object(json!({ "type": "unchanged" })));
    };
    push_render(PushTarget::All).map_err(kv_failure)?;
    let wait = if skip_cooldown { 0 } else { COOLDOWN_SECONDS };
    Ok(object(json!({ "type": "placed", "stamps": [[tile, stamp]], "wait": wait })))
}

fn cooldown_key(character_id: &str) -> String {
    format!("place:cooldown:{character_id}")
}

/// Whole seconds left on a Character's cooldown; 0 when it may place.
fn cooldown_left(character_id: &str) -> Result<u64, HookError> {
    Ok(kv::get(&cooldown_key(character_id))
        .map_err(kv_failure)?
        .map_or(0, |entry| entry.ttl_seconds.unwrap_or(COOLDOWN_SECONDS)))
}

fn parse_point(message: &Value) -> Result<Point, HookError> {
    let coordinate = |name: &str, limit: usize| {
        message
            .get(name)
            .and_then(Value::as_u64)
            .filter(|value| *value < limit as u64)
            .map(|value| value as usize)
            .ok_or_else(|| HookError::new("bad_point", "pixel is outside the board"))
    };
    Ok(Point { x: coordinate("x", WIDTH)?, y: coordinate("y", HEIGHT)? })
}

fn parse_color(message: &Value) -> Result<u8, HookError> {
    message
        .get("color")
        .and_then(Value::as_u64)
        .filter(|color| *color < COLORS)
        .map(|color| color as u8)
        .ok_or_else(|| HookError::new("bad_color", "choose a palette color"))
}

/// Answers a fetch with the asked-for tiles that fit in one reply, as `[id, stamp, data]`. The canvas asks again
/// for any it did not get.
fn fetch_tiles(message: &Value) -> Result<Map<String, Value>, HookError> {
    let mut ids = Vec::new();
    for id in message
        .get("ids")
        .and_then(Value::as_array)
        .filter(|ids| !ids.is_empty() && ids.len() <= MAX_FETCH)
        .ok_or_else(|| HookError::new("bad_tiles", "ask for 1 to 32 tiles"))?
    {
        let id = id
            .as_u64()
            .filter(|id| *id < TILE_COUNT as u64)
            .ok_or_else(|| HookError::new("bad_tiles", "a tile id is outside the board"))?
            as usize;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let keys: Vec<String> = ids.iter().map(|id| tile_key(*id)).collect();
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    let values = kv::get_many(&keys).map_err(kv_failure)?;
    let mut tiles = Vec::new();
    let mut budget = REPLY_TILE_BUDGET;
    for (id, value) in ids.iter().zip(values) {
        let (stamp, data) = match value {
            Some(bytes) => split_stored(&bytes)?,
            None => (0, Vec::new()),
        };
        let cost = encoded_len(data.len()) + 20;
        if cost > budget && !tiles.is_empty() {
            break;
        }
        budget = budget.saturating_sub(cost);
        tiles.push(json!([id, stamp, encode_base64(&data)]));
    }
    Ok(object(json!({ "type": "tiles", "tiles": tiles })))
}

fn require_observer(observer: Option<Observer>) -> Result<Observer, HookError> {
    let observer = observer
        .ok_or_else(|| HookError::new("logged_out", "log in to place pixels"))?;
    if observer.character_id.is_empty() {
        return Err(HookError::new(
            "bad_observer",
            "a Character is required to place pixels",
        ));
    }
    Ok(observer)
}

/// The tile a point is in, and its offset inside that tile.
fn tile_of(point: Point) -> (usize, usize) {
    let tile = (point.y / TILE_SIDE) * TILES_PER_ROW + point.x / TILE_SIDE;
    (tile, (point.y % TILE_SIDE) * TILE_SIDE + point.x % TILE_SIDE)
}

fn tile_key(tile: usize) -> String {
    format!("place:tile:{tile}")
}

fn who_key(tile: usize) -> String {
    format!("place:who:{tile}")
}

/// Sets one pixel and records who placed it. Returns its tile and new stamp, or `None` when the pixel already had
/// that color.
fn paint(color: u8, point: Point, touch: &PixelTouch) -> Result<Option<(usize, u32)>, HookError> {
    let (tile, offset) = tile_of(point);
    let key = tile_key(tile);
    let mut stamp = None;
    for _ in 0..CAS_ATTEMPTS {
        let previous = kv::get(&key).map_err(kv_failure)?.map(|entry| entry.value);
        let mut stored = match &previous {
            Some(bytes) => decode_stored_tile(bytes)?,
            None => Tile::empty(),
        };
        if stored.pixels[offset] == color {
            return Ok(None);
        }
        stored.pixels[offset] = color;
        // The stamp is drawn once, when something changes. A tile's stamp never goes backwards, even when a later
        // placement wrote it first.
        let placement = match stamp {
            Some(stamp) => stamp,
            None => *stamp.insert(kv::incr(STAMP_KEY, 1, None).map_err(kv_failure)? as u32),
        };
        stored.stamp = placement.max(stored.stamp.wrapping_add(1));
        if kv::compare_and_swap(&key, previous.as_deref(), encode_stored_tile(&stored)).map_err(kv_failure)? {
            record_touch(tile, offset, touch)?;
            return Ok(Some((tile, stored.stamp)));
        }
    }
    Err(HookError::new("storage_busy", "the board is busy; try again"))
}

/// Pixel history for one tile: a 1-based placer index per pixel (0 for none) as little-endian u16s, then the placers
/// as a JSON list of `[name, trolltag]`. Placers no pixel points at any more are dropped on each write.
fn record_touch(tile: usize, offset: usize, touch: &PixelTouch) -> Result<(), HookError> {
    let key = who_key(tile);
    for _ in 0..CAS_ATTEMPTS {
        let previous = kv::get(&key).map_err(kv_failure)?.map(|entry| entry.value);
        let (mut indices, mut placers) = match &previous {
            Some(bytes) => decode_touches(bytes)?,
            None => (vec![0; TILE_PIXELS], Vec::new()),
        };
        let placer = match placers.iter().position(|placer| placer == touch) {
            Some(index) => index,
            None => {
                placers.push(touch.clone());
                placers.len() - 1
            }
        };
        indices[offset] = placer as u16 + 1;
        let value = encode_touches(&indices, &placers);
        if kv::compare_and_swap(&key, previous.as_deref(), value).map_err(kv_failure)? {
            return Ok(());
        }
    }
    Err(HookError::new("storage_busy", "pixel history is busy; try again"))
}

fn encode_touches(indices: &[u16], placers: &[PixelTouch]) -> Vec<u8> {
    let mut remap = vec![0u16; placers.len() + 1];
    let mut kept = Vec::new();
    for index in indices {
        let index = *index as usize;
        if index != 0 && remap[index] == 0 {
            kept.push(json!([placers[index - 1].name, placers[index - 1].trolltag]));
            remap[index] = kept.len() as u16;
        }
    }
    let mut value = Vec::with_capacity(TILE_PIXELS * 2 + kept.len() * 24);
    for index in indices {
        value.extend_from_slice(&remap[*index as usize].to_le_bytes());
    }
    value.extend_from_slice(Value::Array(kept).to_string().as_bytes());
    value
}

fn decode_touches(bytes: &[u8]) -> Result<(Vec<u16>, Vec<PixelTouch>), HookError> {
    let invalid = || HookError::new("invalid_touch", "saved pixel history is invalid");
    if bytes.len() < TILE_PIXELS * 2 {
        return Err(invalid());
    }
    let (indices, placers) = bytes.split_at(TILE_PIXELS * 2);
    let placers: Vec<(String, String)> = serde_json::from_slice(placers).map_err(|_| invalid())?;
    let indices: Vec<u16> = indices
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    if indices.iter().any(|index| *index as usize > placers.len()) {
        return Err(invalid());
    }
    let placers = placers
        .into_iter()
        .map(|(name, trolltag)| PixelTouch { name, trolltag })
        .collect();
    Ok((indices, placers))
}

fn read_touch(point: Point) -> Result<Option<PixelTouch>, HookError> {
    let (tile, offset) = tile_of(point);
    let Some(entry) = kv::get(&who_key(tile)).map_err(kv_failure)? else {
        return Ok(None);
    };
    let (indices, placers) = decode_touches(&entry.value)?;
    Ok(match indices[offset] {
        0 => None,
        index => Some(placers[index as usize - 1].clone()),
    })
}

/// Every tile, in one read.
fn read_stored_tiles() -> Result<Vec<StoredTile>, HookError> {
    let keys: Vec<String> = (0..TILE_COUNT).map(tile_key).collect();
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    kv::get_many(&keys)
        .map_err(kv_failure)?
        .into_iter()
        .map(|value| value.map(|bytes| split_stored(&bytes)).transpose())
        .collect()
}

/// A stored tile is its stamp as a little-endian u32, then its wire data (see `encode_tile`).
fn split_stored(bytes: &[u8]) -> Result<(u32, Vec<u8>), HookError> {
    if bytes.len() < 4 {
        return Err(invalid_board_data());
    }
    let stamp = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Ok((stamp, bytes[4..].to_vec()))
}

fn decode_stored_tile(bytes: &[u8]) -> Result<Tile, HookError> {
    let (stamp, data) = split_stored(bytes)?;
    Ok(Tile { stamp, pixels: decode_tile(&data)? })
}

fn encode_stored_tile(tile: &Tile) -> Vec<u8> {
    let mut stored = tile.stamp.to_le_bytes().to_vec();
    stored.extend_from_slice(&encode_tile(&tile.pixels));
    stored
}

/// A tile's wire data: runs of `(gap, length, color)`, the gap and length as varints, skipping unplaced pixels; or,
/// when that would not be shorter, the 1,024 raw pixels. Every encoding but the raw one is shorter than 1,024 bytes,
/// so the length tells them apart.
fn encode_tile(pixels: &[u8]) -> Vec<u8> {
    let mut packed = Vec::new();
    let (mut index, mut previous_end) = (0, 0);
    while index < pixels.len() {
        if pixels[index] == EMPTY {
            index += 1;
            continue;
        }
        let color = pixels[index];
        let start = index;
        while index < pixels.len() && pixels[index] == color {
            index += 1;
        }
        write_varint(&mut packed, (start - previous_end) as u32);
        write_varint(&mut packed, (index - start) as u32);
        packed.push(color);
        previous_end = index;
        if packed.len() >= TILE_PIXELS {
            return pixels.to_vec();
        }
    }
    packed
}

fn decode_tile(data: &[u8]) -> Result<Vec<u8>, HookError> {
    if data.len() == TILE_PIXELS {
        if data.iter().any(|pixel| *pixel != EMPTY && u64::from(*pixel) >= COLORS) {
            return Err(invalid_board_data());
        }
        return Ok(data.to_vec());
    }
    let mut pixels = vec![EMPTY; TILE_PIXELS];
    let (mut cursor, mut previous_end) = (0, 0usize);
    while cursor < data.len() {
        let gap = read_varint(data, &mut cursor).ok_or_else(invalid_board_data)? as usize;
        let run = read_varint(data, &mut cursor).ok_or_else(invalid_board_data)? as usize;
        let color = *data.get(cursor).ok_or_else(invalid_board_data)?;
        cursor += 1;
        let start = previous_end.checked_add(gap).ok_or_else(invalid_board_data)?;
        let end = start.checked_add(run).ok_or_else(invalid_board_data)?;
        if run == 0 || end > TILE_PIXELS || u64::from(color) >= COLORS {
            return Err(invalid_board_data());
        }
        pixels[start..end].fill(color);
        previous_end = end;
    }
    Ok(pixels)
}

fn invalid_board_data() -> HookError {
    HookError::new("invalid_board", "the saved board data is invalid")
}

fn write_varint(output: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
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

fn encoded_len(bytes: usize) -> usize {
    (bytes * 4).div_ceil(3)
}

fn encode_base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(encoded_len(bytes.len()));
    for chunk in bytes.chunks(3) {
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

fn kv_failure(error: kv::KvError) -> HookError {
    HookError::new(error.code, error.message)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use chittr_cartridge::testing::{self, HookClass, TestHost};
    use serde::de::DeserializeOwned;
    use serde::Serialize;

    fn wire<T: DeserializeOwned>(input: impl Serialize) -> T {
        serde_json::from_value(serde_json::to_value(input).expect("input serializes"))
            .expect("input matches the generated ABI type")
    }

    fn placer(id: &str) -> chittr_cartridge::envelope::Observer {
        let mut observer = testing::observer(id);
        observer.profile.insert("trolltag".to_string(), json!(id));
        observer
    }

    fn send(chitt: &testing::ChittRef, id: Option<&str>, message: Value) -> CanvasMessageOutput {
        let input = testing::canvas_message(chitt, id.map(placer), message, Some(1));
        on_canvas_message(wire(input)).expect("the Hook returns")
    }

    fn place(chitt: &testing::ChittRef, id: &str, x: usize, y: usize, color: u8) -> Map<String, Value> {
        let output = send(chitt, Some(id), json!({ "type": "place", "x": x, "y": y, "color": color }));
        assert!(output.error.is_none(), "{:?}", output.error);
        output.reply.unwrap()
    }

    fn render_state(chitt: &testing::ChittRef, id: Option<&str>) -> Map<String, Value> {
        let output = render(wire(testing::render(chitt, id.map(placer)))).expect("the Hook returns");
        assert!(output.error.is_none(), "{:?}", output.error);
        output.canvas.expect("render has canvas state")
    }

    fn decode_base64(text: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        let (mut bits, mut count) = (0u32, 0);
        for byte in text.bytes() {
            let value = BASE64.iter().position(|c| *c == byte).unwrap() as u32;
            bits = (bits << 6) | value;
            count += 6;
            if count >= 8 {
                count -= 8;
                bytes.push((bits >> count) as u8);
                bits &= (1 << count) - 1;
            }
        }
        bytes
    }

    fn stamps(state: &Map<String, Value>) -> Vec<u32> {
        decode_base64(state["stamps"].as_str().unwrap())
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn placements_need_a_palette_color_and_a_point_on_the_board() {
        let _host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        for (message, code) in [
            (json!({ "type": "place", "x": 0, "y": 0, "color": 32 }), "bad_color"),
            (json!({ "type": "place", "x": 0, "y": 0, "color": 255 }), "bad_color"),
            (json!({ "type": "place", "x": 512, "y": 0, "color": 0 }), "bad_point"),
            (json!({ "type": "paint", "color": 0, "points": [0, 0] }), "bad_input"),
        ] {
            assert_eq!(send(&chitt, Some("ada"), message).error.unwrap().code, code);
        }
    }

    #[test]
    fn tiles_cover_the_board_in_row_order() {
        assert_eq!(tile_of(Point { x: 0, y: 0 }), (0, 0));
        assert_eq!(tile_of(Point { x: 33, y: 1 }), (1, 33));
        assert_eq!(tile_of(Point { x: 511, y: 511 }), (TILE_COUNT - 1, TILE_PIXELS - 1));
    }

    #[test]
    fn tile_encoding_is_compact_for_sparse_tiles_and_raw_for_noisy_ones() {
        let mut sparse = vec![EMPTY; TILE_PIXELS];
        sparse[1] = 0;
        sparse[2] = 31;
        let data = encode_tile(&sparse);
        assert_eq!(data, [1, 1, 0, 0, 1, 31]);
        assert_eq!(decode_tile(&data).unwrap(), sparse);
        assert!(encode_tile(&vec![EMPTY; TILE_PIXELS]).is_empty());

        let noisy: Vec<u8> = (0..TILE_PIXELS).map(|index| (index % 2) as u8).collect();
        let data = encode_tile(&noisy);
        assert_eq!(data.len(), TILE_PIXELS);
        assert_eq!(decode_tile(&data).unwrap(), noisy);
        assert!(decode_tile(&[0, 0, 0]).is_err());
        assert!(decode_tile(&[0, 1, 32]).is_err());
    }

    #[test]
    fn logged_out_viewers_can_look_but_not_place() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        let output = send(&chitt, None, json!({ "type": "place", "x": 0, "y": 0, "color": 1 }));
        assert_eq!(output.error.unwrap().code, "logged_out");
        let output = send(&chitt, None, json!({ "type": "info", "x": 0, "y": 0 }));
        assert_eq!(output.reply.unwrap()["touched"], json!(false));
        host.begin_hook(HookClass::Render);
        let state = render_state(&chitt, None);
        assert_eq!(state["canPlace"], json!(false));
        assert!(state.get("cooldown").is_none());
    }

    #[test]
    fn a_placement_starts_a_fifteen_second_cooldown() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        let reply = place(&chitt, "ada", 1, 1, 7);
        assert_eq!(reply["type"], json!("placed"));
        assert_eq!(reply["stamps"], json!([[0, 1]]));
        assert_eq!(reply["wait"], json!(15));
        assert_eq!(host.pushes().len(), 1);

        host.advance(5);
        let reply = place(&chitt, "ada", 2, 2, 7);
        assert_eq!(reply, object(json!({ "type": "cooldown", "wait": 10 })));
        // Someone else's cooldown is their own.
        assert_eq!(place(&chitt, "bob", 2, 2, 8)["type"], json!("placed"));

        host.begin_hook(HookClass::Render);
        let state = render_state(&chitt, Some("ada"));
        assert_eq!((state["canPlace"].clone(), state["cooldown"].clone()), (json!(true), json!(10)));

        host.advance(10);
        host.begin_hook(HookClass::Background);
        assert_eq!(place(&chitt, "ada", 3, 3, 7)["type"], json!("placed"));
    }

    #[test]
    fn only_admins_can_skip_the_cooldown_and_only_they_are_told_they_can() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        let admin = || placer("root").admin();
        let place_as = |observer: chittr_cartridge::envelope::Observer, x: usize, skip: bool| {
            let message = json!({ "type": "place", "x": x, "y": 0, "color": 3, "skipCooldown": skip });
            let input = testing::canvas_message(&chitt, Some(observer), message, Some(1));
            on_canvas_message(wire(input)).expect("the Hook returns").reply.unwrap()
        };
        for x in 0..3 {
            let reply = place_as(admin(), x, true);
            assert_eq!((reply["type"].clone(), reply["wait"].clone()), (json!("placed"), json!(0)));
        }
        assert!(host.kv_value(&cooldown_key("root")).is_none());
        // Switched off, an admin waits like everyone else.
        assert_eq!(place_as(admin(), 3, false)["wait"], json!(15));
        assert_eq!(place_as(admin(), 4, false)["type"], json!("cooldown"));

        // Anyone else asking to skip is ignored.
        assert_eq!(place_as(placer("ada"), 5, true)["wait"], json!(15));
        assert_eq!(place_as(placer("ada"), 6, true)["type"], json!("cooldown"));

        host.begin_hook(HookClass::Render);
        let output = render(wire(testing::render(&chitt, Some(admin())))).unwrap();
        assert_eq!(output.canvas.unwrap()["admin"], json!(true));
        assert!(render_state(&chitt, Some("ada")).get("admin").is_none());
        assert!(render_state(&chitt, None).get("admin").is_none());
    }

    #[test]
    fn placing_a_pixel_its_own_color_changes_nothing_and_costs_no_cooldown() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        place(&chitt, "ada", 1, 1, 7);
        assert_eq!(place(&chitt, "bob", 1, 1, 7)["type"], json!("unchanged"));
        assert_eq!(host.pushes().len(), 1);
        assert_eq!(place(&chitt, "bob", 2, 1, 7)["type"], json!("placed"));
    }

    #[test]
    fn renders_send_the_newest_tiles_first() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        place(&chitt, "ada", 40, 1, 7);
        place(&chitt, "bob", 2, 2, 8);
        let mut keys = host.kv_keys();
        keys.sort();
        assert_eq!(
            keys,
            [
                "place:cooldown:ada",
                "place:cooldown:bob",
                "place:stamp",
                "place:tile:0",
                "place:tile:1",
                "place:who:0",
                "place:who:1",
            ]
        );
        host.begin_hook(HookClass::Render);
        let state = render_state(&chitt, None);
        assert_eq!(&stamps(&state)[..3], &[2, 1, 0]);
        assert_eq!(state["tiles"][0][0], json!(0));
        let pixels = decode_tile(&decode_base64(state["tiles"][0][1].as_str().unwrap())).unwrap();
        assert_eq!(pixels[66], 8);
    }

    #[test]
    fn a_tile_stamp_never_goes_backwards() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        host.put_kv(&tile_key(0), encode_stored_tile(&Tile { stamp: 9, pixels: vec![EMPTY; TILE_PIXELS] }), None)
            .unwrap();
        assert_eq!(place(&chitt, "ada", 0, 0, 1)["stamps"], json!([[0, 10]]));
    }

    #[test]
    fn pixel_history_names_the_last_placer_and_forgets_placed_over_ones() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        place(&chitt, "ada", 1, 0, 1);
        place(&chitt, "bob", 1, 0, 2);
        let reply = send(&chitt, None, json!({ "type": "info", "x": 1, "y": 0 })).reply.unwrap();
        assert_eq!((reply["name"].as_str(), reply["trolltag"].as_str()), (Some("bob"), Some("bob")));
        let (_, placers) = decode_touches(&host.kv_value(&who_key(0)).unwrap()).unwrap();
        assert_eq!(placers.len(), 1);
    }

    #[test]
    fn a_busy_board_still_renders_and_the_rest_arrives_by_fetch() {
        let host = TestHost::new();
        let chitt = testing::chitt("chitt-1", "poster-1");
        let noisy: Vec<u8> = (0..TILE_PIXELS).map(|index| (index % 3) as u8).collect();
        for tile in 0..TILE_COUNT {
            let stored = Tile { stamp: tile as u32 + 1, pixels: noisy.clone() };
            host.put_kv(&tile_key(tile), encode_stored_tile(&stored), None).unwrap();
        }
        host.begin_hook(HookClass::Render);
        let state = render_state(&chitt, Some("ada"));
        assert!(serde_json::to_vec(&state).unwrap().len() < 15 * 1024);
        let inline = state["tiles"].as_array().unwrap();
        assert!(!inline.is_empty() && inline.len() < TILE_COUNT);
        assert_eq!(inline[0][0], json!(TILE_COUNT - 1));

        host.begin_hook(HookClass::Background);
        let ids: Vec<usize> = (0..MAX_FETCH).collect();
        let reply = send(&chitt, None, json!({ "type": "tiles", "ids": ids })).reply.unwrap();
        assert!(serde_json::to_vec(&reply).unwrap().len() < 4 * 1024);
        let tiles = reply["tiles"].as_array().unwrap();
        assert!(!tiles.is_empty());
        assert_eq!(tiles[0][1], json!(1));
        assert_eq!(decode_tile(&decode_base64(tiles[0][2].as_str().unwrap())).unwrap(), noisy);

        let output = send(&chitt, None, json!({ "type": "tiles", "ids": [256] }));
        assert_eq!(output.error.unwrap().code, "bad_tiles");
    }
}
