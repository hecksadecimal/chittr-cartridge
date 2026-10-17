//! A small keyboard- and touch-playable Snake game. Its exports, host calls and events are the bindings `xtp plugin
//! init` generates from `docs/cartridges/v1/canvas.xtp.yaml` (see the README); drawing goes through `chittr-cartridge`'s
//! command-buffer encoder, `Canvas2D`.

mod pdk;

use std::cell::RefCell;

use chittr_cartridge::canvas::{Canvas2D, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde_json::{json, Map, Value};

const COLS: i32 = 20;
const ROWS: i32 = 14;
const TICK_MS: f64 = 145.0;
const BACKGROUND: u32 = 0x101820ff;
const BOARD: u32 = 0x1b2833ff;
const GRID: u32 = 0x283744ff;
const SNAKE: u32 = 0x5ee6a8ff;
const HEAD: u32 = 0xa3ffcbff;
const FOOD: u32 = 0xff6b6bff;
const TEXT: u32 = 0xe5e7ebff;
const MUTED: u32 = 0x9ca3afff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    fn delta(self) -> Point {
        match self {
            Self::Up => Point { x: 0, y: -1 },
            Self::Right => Point { x: 1, y: 0 },
            Self::Down => Point { x: 0, y: 1 },
            Self::Left => Point { x: -1, y: 0 },
        }
    }

    fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Right => Self::Left,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
        }
    }
}

struct Game {
    width: f32,
    height: f32,
    snake: Vec<Point>,
    direction: Direction,
    next_direction: Direction,
    food: Point,
    score: u64,
    last_tick: Option<f64>,
    running: bool,
    dirty: bool,
    rng: u32,
    seq: u32,
    score_sent: bool,
    notice: Option<String>,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            width: 320.0,
            height: 240.0,
            snake: Vec::new(),
            direction: Direction::Right,
            next_direction: Direction::Right,
            food: Point { x: 14, y: 6 },
            score: 0,
            last_tick: None,
            running: true,
            dirty: true,
            rng: 0x5eed_1234,
            seq: 0,
            score_sent: false,
            notice: None,
        }
    }
}

impl Game {
    fn reset(&mut self) {
        self.snake = vec![
            Point { x: 8, y: 7 },
            Point { x: 7, y: 7 },
            Point { x: 6, y: 7 },
        ];
        self.direction = Direction::Right;
        self.next_direction = Direction::Right;
        self.score = 0;
        self.last_tick = None;
        self.running = true;
        self.dirty = true;
        self.score_sent = false;
        self.notice = None;
        self.food = Point { x: 14, y: 6 };
    }

    fn turn(&mut self, direction: Direction) {
        if direction != self.direction.opposite() {
            self.next_direction = direction;
        }
    }

    fn step(&mut self) -> bool {
        self.direction = self.next_direction;
        let delta = self.direction.delta();
        let head = self.snake[0];
        let next = Point {
            x: head.x + delta.x,
            y: head.y + delta.y,
        };
        let eating = next == self.food;
        if next.x < 0 || next.x >= COLS || next.y < 0 || next.y >= ROWS {
            self.running = false;
            return true;
        }
        let body_end = if eating {
            self.snake.len()
        } else {
            self.snake.len().saturating_sub(1)
        };
        if self.snake[..body_end].contains(&next) {
            self.running = false;
            return true;
        }
        self.snake.insert(0, next);
        if eating {
            self.score += 10;
            self.place_food();
            if !self.running {
                return true;
            }
        } else {
            self.snake.pop();
        }
        self.dirty = true;
        false
    }

    fn place_food(&mut self) {
        for _ in 0..(COLS * ROWS) {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            let point = Point {
                x: (self.rng % COLS as u32) as i32,
                y: ((self.rng >> 8) % ROWS as u32) as i32,
            };
            if !self.snake.contains(&point) {
                self.food = point;
                return;
            }
        }
        self.running = false;
    }
}

thread_local! {
    static GAME: RefCell<Game> = RefCell::new(Game::default());
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    GAME.with(|game| {
        let mut game = game.borrow_mut();
        game.width = input.width as f32;
        game.height = input.height as f32;
        game.reset();
    });
    Ok(reply(set_fps(12).and_then(|()| request_frame())))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|game| draw(&mut game.borrow_mut(), input.t));
    Ok(reply(result))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let result = match input.r#type {
        CanvasEventType::Key => handle_key(&input),
        CanvasEventType::Pointer => handle_pointer(&input),
        CanvasEventType::Resize => {
            GAME.with(|game| {
                let mut game = game.borrow_mut();
                game.width = number(input.width, game.width);
                game.height = number(input.height, game.height);
                game.dirty = true;
            });
            request_frame()
        }
        CanvasEventType::Visibility | CanvasEventType::State => {
            GAME.with(|game| game.borrow_mut().dirty = true);
            request_frame()
        }
        CanvasEventType::Reply => {
            let saved = input
                .reply
                .as_ref()
                .and_then(|reply| reply.get("saved"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if saved {
                GAME.with(|game| game.borrow_mut().dirty = true);
                request_frame()
            } else {
                Ok(())
            }
        }
        CanvasEventType::Error => {
            GAME.with(|game| game.borrow_mut().dirty = true);
            request_frame()
        }
        _ => Ok(()),
    };
    Ok(reply(result))
}

/// A `number` field of an event, or the fallback when the event doesn't carry it.
fn number(value: Option<f64>, fallback: f32) -> f32 {
    value.map(|value| value as f32).unwrap_or(fallback)
}

fn handle_key(input: &CanvasEventInput) -> Result<(), Failed> {
    if input.action.as_deref() != Some("down") {
        return Ok(());
    }
    GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        let key = input.key.as_deref().unwrap_or("");
        let direction = match key {
            "ArrowUp" | "w" | "W" => Some(Direction::Up),
            "ArrowRight" | "d" | "D" => Some(Direction::Right),
            "ArrowDown" | "s" | "S" => Some(Direction::Down),
            "ArrowLeft" | "a" | "A" => Some(Direction::Left),
            " " | "Space" => {
                if !game.running {
                    game.reset();
                }
                None
            }
            _ => None,
        };
        if let Some(direction) = direction {
            if !game.running {
                game.reset();
            }
            game.turn(direction);
        }
        game.dirty = true;
    });
    request_frame()
}

fn handle_pointer(input: &CanvasEventInput) -> Result<(), Failed> {
    if input.action.as_deref() != Some("up") {
        return Ok(());
    }
    GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        if !game.running {
            game.reset();
            return;
        }
        let x = number(input.x, 0.0);
        let width = number(input.width, game.width);
        let scale = if width > 0.0 { game.width / width } else { 1.0 };
        let x = x * scale;
        let quarter = game.width / 4.0;
        let slot = ((x / quarter).floor() as usize).min(3);
        let direction = [
            Direction::Up,
            Direction::Left,
            Direction::Down,
            Direction::Right,
        ][slot];
        game.turn(direction);
        game.dirty = true;
    });
    request_frame()
}

fn draw(game: &mut Game, t: f64) -> Result<(), Failed> {
    if game.running {
        let last = *game.last_tick.get_or_insert(t);
        if t - last >= TICK_MS {
            game.last_tick = Some(t);
            if game.step() {
                save_final_score(game)?;
            }
        }
    }
    if !game.dirty && game.running {
        let last = game.last_tick.unwrap_or(t);
        if t - last < TICK_MS {
            request_frame()?;
            return Ok(());
        }
    }

    game.dirty = false;
    let mut canvas = Canvas2D::new();
    let (width, height) = (game.width, game.height);
    canvas
        .fill_color(BACKGROUND)
        .fill_rect(0.0, 0.0, width, height);
    canvas
        .font("sans-serif", 26.0, 700.0)
        .fill_color(TEXT)
        .text_align(TextAlign::Left)
        .text_baseline(TextBaseline::Top);
    canvas.fill_text(&format!("SCORE  {}", game.score), 10.0, 6.0);
    canvas
        .font("sans-serif", 16.0, 400.0)
        .fill_color(MUTED)
        .text_align(TextAlign::Right)
        .text_baseline(TextBaseline::Middle)
        .fill_text("Arrows / WASD", width - 10.0, 19.0);
    let cell = ((width - 20.0) / COLS as f32)
        .min((height - 80.0) / ROWS as f32)
        .max(1.0);
    let board_w = cell * COLS as f32;
    let board_h = cell * ROWS as f32;
    let left = (width - board_w) / 2.0;
    let top = 40.0;
    canvas
        .fill_color(BOARD)
        .fill_rect(left, top, board_w, board_h);
    canvas.stroke_color(GRID).line_width(0.5);
    for x in 0..=COLS {
        let px = left + x as f32 * cell;
        canvas
            .begin_path()
            .move_to(px, top)
            .line_to(px, top + board_h)
            .stroke();
    }
    for y in 0..=ROWS {
        let py = top + y as f32 * cell;
        canvas
            .begin_path()
            .move_to(left, py)
            .line_to(left + board_w, py)
            .stroke();
    }
    draw_cell(&mut canvas, game.food, left, top, cell, FOOD);
    for (index, segment) in game.snake.iter().enumerate() {
        draw_cell(
            &mut canvas,
            *segment,
            left,
            top,
            cell,
            if index == 0 { HEAD } else { SNAKE },
        );
    }
    canvas
        .font("sans-serif", 32.0, 700.0)
        .text_align(TextAlign::Center)
        .text_baseline(TextBaseline::Middle);
    for (index, glyph) in ["↑", "←", "↓", "→"].iter().enumerate() {
        let center = width * (index as f32 + 0.5) / 4.0;
        canvas
            .fill_color(BOARD)
            .fill_rect(center - 21.0, height - 38.0, 42.0, 34.0);
        canvas
            .fill_color(MUTED)
            .fill_text(glyph, center, height - 21.0);
    }
    if !game.running {
        canvas
            .fill_color(0x000000b0)
            .fill_rect(left, top + board_h / 2.0 - 42.0, board_w, 84.0);
        canvas
            .fill_color(TEXT)
            .font("sans-serif", 32.0, 700.0)
            .fill_text("GAME OVER", width / 2.0, top + board_h / 2.0 - 14.0);
        let hint = game
            .notice
            .as_deref()
            .unwrap_or("Tap or press a direction to restart");
        canvas
            .font("sans-serif", 18.0, 400.0)
            .fill_color(MUTED)
            .fill_text(hint, width / 2.0, top + board_h / 2.0 + 23.0);
    }
    canvas.submit()?;
    if game.running {
        request_frame()?;
    }
    Ok(())
}

fn draw_cell(canvas: &mut Canvas2D, point: Point, left: f32, top: f32, cell: f32, color: u32) {
    canvas.fill_color(color).fill_rect(
        left + point.x as f32 * cell + 1.0,
        top + point.y as f32 * cell + 1.0,
        (cell - 2.0).max(1.0),
        (cell - 2.0).max(1.0),
    );
}

fn save_final_score(game: &mut Game) -> Result<(), Failed> {
    if game.score > 0 && !game.score_sent {
        game.seq = game.seq.wrapping_add(1);
        let message = match json!({ "type": "score", "score": game.score }) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        if let Err(error) = send_message(message, game.seq) {
            game.notice = Some(format!("Score not saved: {}", error.message));
        }
        game.score_sent = true;
    }
    Ok(())
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
fn code_of<T: serde::Serialize>(code: &T) -> String {
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

fn set_fps(fps: i32) -> Result<(), Failed> {
    checked(pdk::set_fps(SetFpsInput { fps }).map_err(host_failed)?.error)
}

fn request_frame() -> Result<(), Failed> {
    checked(pdk::request_frame().map_err(host_failed)?.error)
}

fn send_message(message: Map<String, Value>, seq: u32) -> Result<(), Failed> {
    checked(pdk::send(CanvasSendInput { message, seq: Some(seq as i32) }).map_err(host_failed)?.error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_moves_and_eating_increases_score() {
        let mut game = Game::default();
        game.reset();
        game.food = Point { x: 9, y: 7 };
        game.step();
        assert_eq!(game.score, 10);
        assert_eq!(game.snake[0], Point { x: 9, y: 7 });
        assert_eq!(game.snake.len(), 4);
    }

    #[test]
    fn reverse_direction_is_ignored() {
        let mut game = Game::default();
        game.reset();
        game.turn(Direction::Left);
        game.step();
        assert_eq!(game.snake[0], Point { x: 9, y: 7 });
    }
}
