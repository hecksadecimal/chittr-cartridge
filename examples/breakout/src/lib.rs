//! Breakout's Server Guest. It keeps the top five daily and all-time scores (the daily board rolls at UTC midnight on a
//! server-scheduled timer) and it keeps the level set in its KV store, so the poster can edit levels from the Canvas
//! Guest's built-in editor. The Canvas Guest sends one finished score, and it sends one level at a time to save.

#[allow(dead_code, non_camel_case_types)]
mod pdk;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const STATE_KEY: &str = "leaderboards";
const LEVELS_KEY: &str = "levels";
const DAY_TIMER: &str = "breakout-daily-rollover";
const TOP_SCORES: usize = 5;
const MAX_SCORE: u64 = 1_000_000;
const MAX_CAS_ATTEMPTS: usize = 8;
const SECONDS_PER_DAY: i64 = 86_400;

/// The board is this many square cells across and can hold this many rows. The Canvas Guest agrees.
const COLS: usize = 32;
const MAX_ROWS: usize = 32;
/// A level may use at most this many distinct brick styles (colour + strength). Indices are single characters.
const MAX_STYLES: usize = 16;
const MAX_LEVELS: usize = 6;
const MAX_NAME: usize = 24;
/// The characters that index a level's styles. `0` is the first style; `.` is an empty cell.
const INDEX_CHARS: &[u8] = b"0123456789ABCDEF";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Score {
    character_id: String,
    trolltag: String,
    score: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Leaderboards {
    day: String,
    daily: Vec<Score>,
    all_time: Vec<Score>,
}

impl Leaderboards {
    fn empty(day: String) -> Self {
        Self {
            day,
            daily: Vec::new(),
            all_time: Vec::new(),
        }
    }
}

/// One brick style: its health, whether it is indestructible, and its colour as `0xRRGGBB`.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Style {
    pub hp: u8,
    pub solid: bool,
    pub color: u32,
}

/// One level: a name, the styles it uses, and rows of cells. Each cell is `.` (empty) or a style index character.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Level {
    pub name: String,
    pub styles: Vec<Style>,
    pub rows: Vec<String>,
}

/// What the KV store holds under [`LEVELS_KEY`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LevelSet {
    pub v: u32,
    pub levels: Vec<Level>,
}

impl LevelSet {
    fn defaults() -> Self {
        Self {
            v: 2,
            levels: default_levels(),
        }
    }
}

fn index_char(index: usize) -> char {
    INDEX_CHARS.get(index).copied().unwrap_or(b'?') as char
}

/// The default palette characters the built-in levels are written with.
fn default_style(cell: char) -> (u8, bool, u32) {
    match cell {
        'r' => (1, false, 0xff5d73),
        'o' => (1, false, 0xff9f45),
        'y' => (1, false, 0xffd93d),
        'g' => (1, false, 0x59e08a),
        'c' => (1, false, 0x4fd6e6),
        'b' => (1, false, 0x6c8cff),
        'v' => (1, false, 0xb46cff),
        'R' => (2, false, 0xff5d73),
        'O' => (2, false, 0xff9f45),
        'Y' => (2, false, 0xffd93d),
        'G' => (2, false, 0x59e08a),
        'C' => (2, false, 0x4fd6e6),
        'B' => (2, false, 0x6c8cff),
        'V' => (2, false, 0xb46cff),
        '1' => (3, false, 0xff5d73),
        '2' => (3, false, 0xff9f45),
        '3' => (3, false, 0xffd93d),
        'X' => (1, true, 0x6b7793),
        _ => (1, false, 0x9ca3af),
    }
}

/// Turns a readable grid of palette characters into a [`Level`] with a compact style table.
fn level_from_rows(name: &str, rows: Vec<String>) -> Level {
    let mut styles: Vec<Style> = Vec::new();
    let mut encoded = Vec::with_capacity(rows.len());
    for row in rows {
        let mut out = String::with_capacity(row.chars().count());
        for cell in row.chars() {
            if cell == '.' {
                out.push('.');
                continue;
            }
            let (hp, solid, color) = default_style(cell);
            let index = match styles
                .iter()
                .position(|style| style.hp == hp && style.solid == solid && style.color == color)
            {
                Some(index) => index,
                None => {
                    styles.push(Style { hp, solid, color });
                    styles.len() - 1
                }
            };
            out.push(index_char(index));
        }
        encoded.push(out);
    }
    Level {
        name: name.to_string(),
        styles,
        rows: encoded,
    }
}

fn repeat(cell: char) -> String {
    cell.to_string().repeat(COLS)
}

fn default_levels() -> Vec<Level> {
    let bands = level_from_rows(
        "FIRST LIGHT",
        vec![
            repeat('r'),
            repeat('r'),
            repeat('o'),
            repeat('o'),
            repeat('y'),
            repeat('y'),
            repeat('g'),
            repeat('g'),
            repeat('c'),
            repeat('c'),
        ],
    );
    let checker = level_from_rows(
        "CHECKMATE",
        (0..12)
            .map(|row| {
                (0..COLS)
                    .map(|col| if (row + col) % 2 == 0 { 'c' } else { 'b' })
                    .collect()
            })
            .collect(),
    );
    let bulwark = level_from_rows(
        "BULWARK",
        (0..12)
            .map(|row| {
                (0..COLS)
                    .map(|col| {
                        if col % 4 == 0 {
                            'X'
                        } else if row % 4 == 3 {
                            'Y'
                        } else {
                            'B'
                        }
                    })
                    .collect()
            })
            .collect(),
    );
    let diamond = level_from_rows(
        "DIAMOND",
        (0..15)
            .map(|row| {
                let reach = if row < 8 { row } else { 14 - row };
                (0..COLS)
                    .map(|col| {
                        let distance = (col as i32 - (COLS as i32) / 2).unsigned_abs() as usize;
                        if distance > reach {
                            '.'
                        } else if distance + 4 <= reach {
                            '1'
                        } else if distance + 2 <= reach {
                            'G'
                        } else {
                            'g'
                        }
                    })
                    .collect()
            })
            .collect(),
    );
    // The classic invader, doubled to the wider board.
    const INVADER: [&str; 7] = [
        "....2......2....",
        ".....2....2.....",
        "....33333333....",
        "...33.3333.33...",
        "..333333333333..",
        ".33..3....3..33.",
        "..33........33..",
    ];
    let invader = level_from_rows(
        "INVADER",
        INVADER
            .iter()
            .map(|row| row.chars().flat_map(|cell| [cell, cell]).collect())
            .collect(),
    );
    vec![bands, checker, bulwark, diamond, invader]
}

pub(crate) fn init(input: InitInput) -> Result<InitOutput, Error> {
    let day = input.chitt.created_at.format("%Y-%m-%d").to_string();
    let result = initialize_state(&day)
        .and_then(|()| ensure_levels())
        .and_then(|()| {
            schedule_day_rollover(seconds_until_midnight(input.chitt.created_at.timestamp()))
        });
    Ok(InitOutput {
        content: "Breakout — clear the bricks and chase today's high score. The poster can edit levels from the canvas."
            .to_string(),
        error: result.err(),
        ..Default::default()
    })
}

pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    let fallback_day = input.chitt.created_at.format("%Y-%m-%d").to_string();
    let admin = input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.character_id == input.chitt.poster_id);
    let levels = read_levels().unwrap_or_else(|_| LevelSet::defaults());
    let names: Vec<&str> = levels
        .levels
        .iter()
        .map(|level| level.name.as_str())
        .collect();
    let mut canvas = Map::new();
    canvas.insert("admin".to_string(), Value::Bool(admin));
    canvas.insert(
        "levels".to_string(),
        serde_json::to_value(&levels.levels).unwrap_or(Value::Array(Vec::new())),
    );
    Ok(match read_state(&fallback_day) {
        Ok(state) => RenderOutput {
            template: "card".to_string(),
            data: object(json!({
                "day": state.day,
                "daily_scores": state.daily,
                "all_time_scores": state.all_time,
                "admin": admin,
                "level_count": names.len(),
                "level_names": names,
            })),
            canvas: Some(canvas),
            error: None,
        },
        Err(error) => RenderOutput {
            template: String::new(),
            data: Map::new(),
            canvas: None,
            error: Some(error),
        },
    })
}

pub(crate) fn on_canvas_message(input: CanvasMessageInput) -> Result<CanvasMessageOutput, Error> {
    Ok(match handle_message(&input) {
        Ok((reply, rerender)) => CanvasMessageOutput {
            reply: Some(reply),
            rerender: Some(rerender),
            error: None,
        },
        Err(error) => CanvasMessageOutput {
            reply: None,
            rerender: None,
            error: Some(error),
        },
    })
}

pub(crate) fn on_timer(input: TimerInput) -> Result<TimerOutput, Error> {
    if input.name != DAY_TIMER {
        return Ok(TimerOutput::default());
    }
    let day = input.scheduled_for.format("%Y-%m-%d").to_string();
    let result = roll_daily_board(&day).and_then(|changed| {
        // Keep the rollover chain alive even if notifying viewers temporarily fails.
        schedule_day_rollover(SECONDS_PER_DAY as i32)?;
        if changed {
            push_render_all()?;
        }
        Ok(())
    });
    Ok(TimerOutput {
        error: result.err(),
    })
}

/// Handles one canvas message. Returns the reply and whether the card should be drawn again.
fn handle_message(input: &CanvasMessageInput) -> Result<(Map<String, Value>, bool), HookError> {
    let message = &input.message;
    match message.get("type").and_then(Value::as_str) {
        Some("score") => {
            let changed = record_score(input, message)?;
            Ok((object(json!({ "saved": changed })), changed))
        }
        Some("save_level") => save_level(input, message),
        Some("reset_levels") => {
            require_admin(input)?;
            write_levels(&LevelSet::defaults())?;
            push_render_all()?;
            Ok((
                object(json!({ "saved": true, "levels": true, "reset": true })),
                true,
            ))
        }
        _ => Err(fail("bad_input", "unknown game message")),
    }
}

fn record_score(
    input: &CanvasMessageInput,
    message: &Map<String, Value>,
) -> Result<bool, HookError> {
    let score = message
        .get("score")
        .and_then(Value::as_u64)
        .filter(|score| (1..=MAX_SCORE).contains(score))
        .ok_or_else(|| fail("bad_score", "score must be between 1 and 1000000"))?;
    let observer = input
        .observer
        .as_ref()
        .ok_or_else(|| fail("logged_out", "log in to save a score"))?;
    let trolltag = Some(observer.trolltag.as_str())
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| fail("bad_observer", "your Character has no trolltag"))?;
    let score = Score {
        character_id: observer.character_id.clone(),
        trolltag: trolltag.to_string(),
        score,
    };
    let fallback_day = input.chitt.created_at.format("%Y-%m-%d").to_string();
    let changed = mutate_state(&fallback_day, |state| {
        let daily_changed = upsert(&mut state.daily, score.clone());
        let all_time_changed = upsert(&mut state.all_time, score.clone());
        daily_changed || all_time_changed
    })?;
    if changed {
        push_render_all()?;
    }
    Ok(changed)
}

/// Saves one edited level at `index`, replacing it or appending a new one.
fn save_level(
    input: &CanvasMessageInput,
    message: &Map<String, Value>,
) -> Result<(Map<String, Value>, bool), HookError> {
    require_admin(input)?;
    let index = message
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| fail("bad_levels", "a level index is required"))? as usize;
    let level: Level = serde_json::from_value(message.get("level").cloned().unwrap_or(Value::Null))
        .map_err(|error| fail("bad_levels", format!("the level is malformed: {error}")))?;
    validate_level(&level, index + 1)?;

    let mut set = read_levels()?;
    if index < set.levels.len() {
        set.levels[index] = level;
    } else if index == set.levels.len() && set.levels.len() < MAX_LEVELS {
        set.levels.push(level);
    } else {
        return Err(fail("bad_levels", "that level index is out of range"));
    }
    write_levels(&set)?;
    push_render_all()?;
    Ok((
        object(json!({ "saved": true, "levels": true, "index": index })),
        true,
    ))
}

/// Only the Chitt's poster may edit levels.
fn require_admin(input: &CanvasMessageInput) -> Result<(), HookError> {
    let observer = input
        .observer
        .as_ref()
        .ok_or_else(|| fail("logged_out", "log in as the poster to edit levels"))?;
    if observer.character_id != input.chitt.poster_id {
        return Err(fail("forbidden", "only the poster can edit levels"));
    }
    Ok(())
}

fn validate_level(level: &Level, number: usize) -> Result<(), HookError> {
    let name = level.name.trim();
    if name.is_empty() {
        return Err(fail("bad_levels", format!("level {number} needs a name")));
    }
    if name.chars().count() > MAX_NAME {
        return Err(fail(
            "bad_levels",
            format!("level {number}'s name is longer than {MAX_NAME} characters"),
        ));
    }
    if level.styles.is_empty() || level.styles.len() > MAX_STYLES {
        return Err(fail(
            "bad_levels",
            format!("level {number} must use 1 to {MAX_STYLES} brick styles"),
        ));
    }
    for style in &level.styles {
        if !style.solid && !(1..=3).contains(&style.hp) {
            return Err(fail(
                "bad_levels",
                format!("level {number} has a brick with {0} health", style.hp),
            ));
        }
        if style.color > 0x00ff_ffff {
            return Err(fail(
                "bad_levels",
                format!("level {number} has a colour outside 24 bits"),
            ));
        }
    }
    if level.rows.is_empty() || level.rows.len() > MAX_ROWS {
        return Err(fail(
            "bad_levels",
            format!("level {number} must have 1 to {MAX_ROWS} rows"),
        ));
    }
    let mut breakable = false;
    for (row_index, row) in level.rows.iter().enumerate() {
        if row.chars().count() != COLS {
            return Err(fail(
                "bad_levels",
                format!(
                    "level {number}, row {} must be {COLS} cells wide",
                    row_index + 1
                ),
            ));
        }
        for cell in row.chars() {
            if cell == '.' {
                continue;
            }
            let Some(index) = INDEX_CHARS.iter().position(|c| *c as char == cell) else {
                return Err(fail(
                    "bad_levels",
                    format!("level {number} has an unknown cell '{cell}'"),
                ));
            };
            if index >= level.styles.len() {
                return Err(fail(
                    "bad_levels",
                    format!("level {number} uses a style index it does not define"),
                ));
            }
            if !level.styles[index].solid {
                breakable = true;
            }
        }
    }
    if !breakable {
        return Err(fail(
            "bad_levels",
            format!("level {number} needs at least one breakable brick"),
        ));
    }
    Ok(())
}

fn initialize_state(day: &str) -> Result<(), HookError> {
    for _ in 0..MAX_CAS_ATTEMPTS {
        let previous = kv_read(STATE_KEY)?;
        if previous.is_some() {
            return Ok(());
        }
        let value = serde_json::to_vec(&Leaderboards::empty(day.to_string()))
            .map_err(|error| fail("invalid_state", error.to_string()))?;
        if kv_compare_and_swap(STATE_KEY, None, &value)? {
            return Ok(());
        }
    }
    Err(fail(
        "storage_busy",
        "could not initialize leaderboards; try again",
    ))
}

fn ensure_levels() -> Result<(), HookError> {
    for _ in 0..MAX_CAS_ATTEMPTS {
        let previous = kv_read(LEVELS_KEY)?;
        if previous.is_some() {
            return Ok(());
        }
        let value = serde_json::to_vec(&LevelSet::defaults())
            .map_err(|error| fail("invalid_levels", error.to_string()))?;
        if kv_compare_and_swap(LEVELS_KEY, None, &value)? {
            return Ok(());
        }
    }
    Err(fail(
        "storage_busy",
        "could not initialize levels; try again",
    ))
}

fn read_levels() -> Result<LevelSet, HookError> {
    match kv_read(LEVELS_KEY)? {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| fail("invalid_levels", error.to_string())),
        None => Ok(LevelSet::defaults()),
    }
}

fn write_levels(set: &LevelSet) -> Result<(), HookError> {
    let value =
        serde_json::to_vec(set).map_err(|error| fail("invalid_levels", error.to_string()))?;
    let reply = pdk::kv_set(KvSetInput {
        key: LEVELS_KEY.to_string(),
        ttl_seconds: None,
        value: STANDARD.encode(&value),
    })
    .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => Ok(()),
    }
}

fn read_state(fallback_day: &str) -> Result<Leaderboards, HookError> {
    match kv_read(STATE_KEY)? {
        Some(bytes) => {
            serde_json::from_slice(&bytes).map_err(|error| fail("invalid_state", error.to_string()))
        }
        None => Ok(Leaderboards::empty(fallback_day.to_string())),
    }
}

/// Applies one update to the shared state without losing a concurrent score or daily rollover.
fn mutate_state(
    fallback_day: &str,
    mut update: impl FnMut(&mut Leaderboards) -> bool,
) -> Result<bool, HookError> {
    for _ in 0..MAX_CAS_ATTEMPTS {
        let previous = kv_read(STATE_KEY)?;
        let mut state = match &previous {
            Some(bytes) => serde_json::from_slice(bytes)
                .map_err(|error| fail("invalid_state", error.to_string()))?,
            None => Leaderboards::empty(fallback_day.to_string()),
        };
        let changed = update(&mut state);
        if !changed && previous.is_some() {
            return Ok(false);
        }
        let value =
            serde_json::to_vec(&state).map_err(|error| fail("invalid_state", error.to_string()))?;
        if kv_compare_and_swap(STATE_KEY, previous.as_deref(), &value)? {
            return Ok(changed);
        }
    }
    Err(fail("storage_busy", "could not save the score; try again"))
}

fn roll_daily_board(day: &str) -> Result<bool, HookError> {
    mutate_state(day, |state| roll_state_day(state, day))
}

fn roll_state_day(state: &mut Leaderboards, day: &str) -> bool {
    if day > state.day.as_str() {
        state.day = day.to_string();
        state.daily.clear();
        true
    } else {
        false
    }
}

fn upsert(entries: &mut Vec<Score>, score: Score) -> bool {
    let before = entries.clone();
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.character_id == score.character_id)
    {
        let best_score = existing.score.max(score.score);
        if best_score != existing.score || existing.trolltag != score.trolltag {
            existing.score = best_score;
            existing.trolltag = score.trolltag;
        }
    } else {
        entries.push(score);
    }
    entries.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.trolltag.cmp(&b.trolltag))
            .then_with(|| a.character_id.cmp(&b.character_id))
    });
    entries.truncate(TOP_SCORES);
    *entries != before
}

fn seconds_until_midnight(timestamp: i64) -> i32 {
    let remaining = SECONDS_PER_DAY - timestamp.rem_euclid(SECONDS_PER_DAY);
    remaining.max(10) as i32
}

fn schedule_day_rollover(delay_seconds: i32) -> Result<(), HookError> {
    let reply = pdk::schedule_timer(ScheduleTimerInput {
        name: DAY_TIMER.to_string(),
        delay_seconds,
        payload: None,
    })
    .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => Ok(()),
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn fail(code: &str, message: impl Into<String>) -> HookError {
    HookError {
        code: code.to_string(),
        message: message.into(),
    }
}

fn host_failed(error: Error) -> HookError {
    fail("unavailable", error.to_string())
}

fn malformed() -> HookError {
    fail("invalid", "the host's reply is missing a field")
}

fn code_of<T: Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn kv_error(error: KvError) -> HookError {
    fail(&code_of(&error.code), error.message)
}

fn kv_read(key: &str) -> Result<Option<Vec<u8>>, HookError> {
    let reply = pdk::kv_get(KvKeyInput {
        key: key.to_string(),
    })
    .map_err(host_failed)?;
    if let Some(error) = reply.error {
        return Err(kv_error(error));
    }
    reply.value.map(|value| decode(&value)).transpose()
}

fn kv_compare_and_swap(
    key: &str,
    expected: Option<&[u8]>,
    value: &[u8],
) -> Result<bool, HookError> {
    let reply = pdk::kv_compare_and_swap(KvCompareAndSwapInput {
        key: key.to_string(),
        expected: expected.map(|bytes| STANDARD.encode(bytes)),
        value: STANDARD.encode(value),
    })
    .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => reply.swapped.ok_or_else(malformed),
    }
}

fn decode(text: &str) -> Result<Vec<u8>, HookError> {
    STANDARD
        .decode(text)
        .map_err(|_| fail("invalid", "a stored value is not base64"))
}

fn push_render_all() -> Result<(), HookError> {
    let reply = pdk::push_render(PushRenderInput { character_id: None }).map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(fail(&code_of(&error.code), error.message)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(id: &str, tag: &str, points: u64) -> Score {
        Score {
            character_id: id.into(),
            trolltag: tag.into(),
            score: points,
        }
    }

    fn level(name: &str, rows: &[&str]) -> Level {
        level_from_rows(name, rows.iter().map(|row| (*row).to_string()).collect())
    }

    #[test]
    fn leaderboard_keeps_personal_bests_and_orders_ties_deterministically() {
        let mut entries = vec![score("b", "zed", 10), score("a", "ada", 10)];
        assert!(upsert(&mut entries, score("c", "cat", 20)));
        assert!(!upsert(&mut entries, score("a", "ada", 8)));
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.trolltag.as_str())
                .collect::<Vec<_>>(),
            ["cat", "ada", "zed"]
        );
    }

    #[test]
    fn rollover_clears_daily_scores_without_touching_all_time() {
        let mut boards = Leaderboards {
            day: "2024-05-01".into(),
            daily: vec![score("a", "ada", 50)],
            all_time: vec![score("a", "ada", 100)],
        };
        assert!(roll_state_day(&mut boards, "2024-05-02"));
        assert!(boards.daily.is_empty());
        assert_eq!(boards.all_time[0].score, 100);
        assert!(!roll_state_day(&mut boards, "2024-05-01"));
    }

    #[test]
    fn a_full_level_set_fits_the_canvas_state_budget() {
        let mut set = LevelSet::defaults();
        while set.levels.len() < MAX_LEVELS {
            set.levels.push(set.levels[0].clone());
        }
        let state = serde_json::to_string(&set.levels).unwrap();
        assert!(
            state.len() < 16 * 1024,
            "the canvas state is {} bytes, over the 16 KiB budget",
            state.len()
        );
        let one = serde_json::to_string(&set.levels[0]).unwrap();
        assert!(
            one.len() < 4 * 1024 - 64,
            "one level message is {} bytes, over the 4 KiB send budget",
            one.len()
        );
    }

    #[test]
    fn default_levels_validate_and_are_thirty_two_wide() {
        for (index, level) in default_levels().iter().enumerate() {
            validate_level(level, index + 1)
                .unwrap_or_else(|error| panic!("{} is invalid: {error:?}", level.name));
            for row in &level.rows {
                assert_eq!(row.chars().count(), COLS);
            }
            assert!(!level.styles.is_empty());
        }
    }

    #[test]
    fn validation_rejects_wrong_widths_and_empty_levels() {
        let mut narrow = level("NARROW", &["111"]);
        assert!(validate_level(&narrow, 1).is_err());

        let empty = level("EMPTY", &[&".".repeat(COLS)]);
        assert!(validate_level(&empty, 1).is_err());

        let tall = level(
            "TALL",
            &vec!["1".repeat(COLS); MAX_ROWS + 1]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        assert!(validate_level(&tall, 1).is_err());

        // A style index that no style defines is rejected.
        narrow.rows = vec!["0".repeat(COLS)];
        narrow.styles.clear();
        assert!(validate_level(&narrow, 1).is_err());
    }

    #[test]
    fn validation_accepts_arbitrary_colours_and_indestructible_bricks() {
        let mut custom = level("CUSTOM", &[&"r".repeat(COLS), &"X".repeat(COLS)]);
        custom.styles[0].color = 0x123456;
        validate_level(&custom, 1).expect("any 24-bit colour is allowed");

        let mut too_bright = custom.clone();
        too_bright.styles[0].color = 0xff00_0000;
        assert!(validate_level(&too_bright, 1).is_err());

        let long_name = level(&"A".repeat(MAX_NAME + 1), &[&"r".repeat(COLS)]);
        assert!(validate_level(&long_name, 1).is_err());
    }
}
