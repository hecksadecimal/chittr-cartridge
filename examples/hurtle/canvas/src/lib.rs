//! Wordle-style Canvas Guest for Hurtle. The server owns guesses and feedback; this guest only
//! handles keyboard/pointer input and draws the latest server state, with short animations for
//! typing, reveals, mistakes, wins, losses and fresh blood.

#![allow(non_camel_case_types)]

mod pdk;

use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use chittr_cartridge::canvas::{
    request_frame, send, set_fps, Canvas2D, FillRule, TextAlign, TextBaseline,
};
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const WIDTH: f32 = 320.0;
const HEIGHT: f32 = 400.0;
const TILE: f32 = 32.0;
const TILE_GAP: f32 = 4.0;
const BOARD_Y: f32 = 22.0;
// Long words narrow their tiles to this width so the side gutters stay clear for blood.
const BOARD_MAX_WIDTH: f32 = 236.0;
const MAX_LETTERS: usize = 12;
// The length the server's state names when it has no game to show; new words have twelve letters.
const DEFAULT_LETTERS: usize = 12;
const STATUS_Y: f32 = BOARD_Y + 6.0 * TILE + 5.0 * TILE_GAP + 10.0;
const STATUS_HEIGHT: f32 = 40.0;
const KEYBOARD_Y: f32 = STATUS_Y + STATUS_HEIGHT + 8.0;
const KEY_HEIGHT: f32 = 30.0;
const KEY_STEP: f32 = 34.0;
const INK: u32 = 0xeee3cfff;
const GOLD: u32 = 0xc5a16aff;
const KEY_ROWS: [&str; 3] = ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"];
const MAX_STAINS: usize = 64;
const MAX_HEALTH: usize = 6;

// Effect lengths in milliseconds.
const POP_MS: f32 = 120.0;
const SHAKE_MS: f32 = 420.0;
const FLIP_MS: f32 = 360.0;
// Tiles start flipping this far apart, or closer for long words so the reveal stays brisk.
const FLIP_STAGGER_MS: f32 = 200.0;
const FLIP_SPREAD_MS: f32 = 1000.0;
const BOUNCE_MS: f32 = 480.0;
const WIN_MS: f32 = 1900.0;
const LOSS_MS: f32 = 700.0;
const SPLAT_MS: f32 = 1800.0;
const KEY_MS: f32 = 140.0;
// The `seq` of the message that reports a death shown; guesses count up from 1 and never reach it.
const SEEN_SEQ: u32 = i32::MAX as u32;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Guess {
    word: String,
    marks: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct GameState {
    round: Option<i64>,
    started: bool,
    guesses: Vec<Guess>,
    complete: bool,
    won: bool,
    answer: Option<String>,
    penalty_pending: bool,
    penalty_applied: bool,
    penalty_error: Option<String>,
    // The viewer's Character is suspended: the board stays visible, but the server refuses its input.
    suspended: bool,
    // The player's own death has not been shown by any canvas yet. `own_stain` supports old saves;
    // new deaths have three seeds, all of which must land when a fresh canvas replays the death.
    fresh_loss: bool,
    own_stain: Option<u32>,
    own_stains: Vec<u32>,
    health: Option<usize>,
    max_health: Option<usize>,
    // Letters in the word being played: a game keeps its own answer's length.
    length: Option<usize>,
    blood: Blood,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Blood {
    total: u64,
    stains: Vec<Bloodstain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bloodstain {
    color: u32,
    seed: u32,
}

/// When an effect began on the frame clock. Events carry no time, and the last frame's time can be
/// long stale when an event arrives, so an effect an event starts is queued and begins on the next frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Anim {
    #[default]
    Idle,
    Queued(f64),
    Running(f64),
}

impl Anim {
    fn after(delay: f32) -> Self {
        Anim::Queued(delay as f64)
    }

    fn resolve(&mut self, now: f64) {
        if let Anim::Queued(delay) = *self {
            *self = Anim::Running(now + delay);
        }
    }

    /// Milliseconds since the effect began: negative while it waits, infinite when it never ran.
    fn elapsed(self, now: f64) -> f32 {
        match self {
            Anim::Idle => f32::INFINITY,
            Anim::Queued(delay) => -delay as f32,
            Anim::Running(start) => (now - start) as f32,
        }
    }

    fn active(self, now: f64, duration: f32) -> bool {
        self.elapsed(now) < duration
    }
}

#[derive(Debug, Default)]
struct Effects {
    pops: [Anim; MAX_LETTERS],
    shake: Anim,
    reveal: Anim,
    reveal_row: usize,
    reveal_ms: f32,
    win: Anim,
    loss: Anim,
    splat: Anim,
    fresh: std::ops::Range<usize>,
    key: Anim,
    key_label: String,
}

impl Effects {
    fn start_reveal(&mut self, row: usize, letters: usize) {
        self.reveal_row = row;
        self.reveal_ms = reveal_duration(letters);
        self.reveal = Anim::after(0.0);
    }

    fn revealing(&self, now: f64) -> bool {
        self.reveal.active(now, self.reveal_ms)
    }

    fn timeline(&mut self) -> impl Iterator<Item = (&mut Anim, f32)> + '_ {
        self.pops.iter_mut().map(|anim| (anim, POP_MS)).chain([
            (&mut self.shake, SHAKE_MS),
            (&mut self.reveal, self.reveal_ms),
            (&mut self.win, WIN_MS),
            (&mut self.loss, LOSS_MS),
            (&mut self.splat, SPLAT_MS),
            (&mut self.key, KEY_MS),
        ])
    }

    fn resolve(&mut self, now: f64) {
        for (anim, _) in self.timeline() {
            anim.resolve(now);
        }
    }

    fn active(&mut self, now: f64) -> bool {
        self.timeline()
            .any(|(anim, duration)| anim.active(now, duration))
    }
}

struct Game {
    width: f32,
    height: f32,
    state: GameState,
    current: String,
    pending: bool,
    seq: u32,
    notice: Option<String>,
    hovered_key: Option<char>,
    // The guess in flight, restored to the board if the server refuses it.
    sent: Option<String>,
    // The latest frame's time, in milliseconds.
    clock: f64,
    // Reduced motion: no flips, shakes, bounces, sparks or growing stains.
    still: bool,
    fx: Effects,
    // Tell the server once this canvas has finished showing the player's death.
    acknowledge: bool,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            width: WIDTH,
            height: HEIGHT,
            state: GameState::default(),
            current: String::new(),
            pending: false,
            seq: 0,
            notice: None,
            hovered_key: None,
            sent: None,
            clock: 0.0,
            still: false,
            fx: Effects::default(),
            acknowledge: false,
        }
    }
}

thread_local! {
    static GAME: RefCell<Game> = RefCell::new(Game::default());
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    let state = match decode_state(input.state) {
        Ok(state) => state,
        Err(error) => return Ok(reply(Err(error))),
    };
    let animating = GAME.with(|game| {
        let mut game = game.borrow_mut();
        game.width = input.width as f32;
        game.height = input.height as f32;
        game.state = state;
        game.current.clear();
        game.pending = false;
        game.notice = None;
        game.hovered_key = None;
        game.sent = None;
        game.still = input.prefers_reduced_motion;
        game.fx = Effects::default();
        game.acknowledge = false;
        replay_unseen_loss(&mut game);
        game.fx.active(0.0)
    });
    let result = set_fps(60).map_err(Failed::from).and_then(|()| draw());
    Ok(reply(result.and_then(|()| {
        if animating {
            request_frame().map_err(Failed::from)
        } else {
            Ok(())
        }
    })))
}

// The host may start a new canvas in the middle of a death (the suspension that follows a loss
// draws the card again), so a death no canvas has finished showing plays again from its last row.
fn replay_unseen_loss(game: &mut Game) {
    let state = &game.state;
    if !(state.fresh_loss && state.complete && !state.won) {
        return;
    }
    let settle = if game.still || state.guesses.is_empty() {
        0.0
    } else {
        game.fx
            .start_reveal(state.guesses.len() - 1, letters(state));
        game.fx.reveal_ms
    };
    game.fx.loss = Anim::after(settle);
    let own: Vec<usize> = state
        .own_stains
        .iter()
        .chain(state.own_stain.iter())
        .filter_map(|seed| {
            state
                .blood
                .stains
                .iter()
                .rposition(|stain| stain.seed == *seed)
        })
        .collect();
    // Death splatters are appended atomically as one consecutive group; some may already have
    // rolled off the bounded wall. Never hide unrelated stains before or after that group.
    if !game.still {
        if let (Some(first), Some(last)) = (own.iter().min(), own.iter().max()) {
            game.fx.fresh = *first..*last + 1;
            game.fx.splat = Anim::after(settle);
        }
    }
    game.acknowledge = true;
}

// Frames run only while an effect plays; a still board costs nothing between events.
pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let (animating, shown) = GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        game.clock = input.t;
        game.fx.resolve(input.t);
        let shown = game.acknowledge
            && !game.fx.loss.active(input.t, LOSS_MS)
            && !game.fx.splat.active(input.t, SPLAT_MS);
        if shown {
            game.acknowledge = false;
        }
        (game.fx.active(input.t), shown)
    });
    let result = draw()
        .and_then(|()| {
            if shown {
                send(json!({ "type": "seen" }), Some(SEEN_SEQ)).map_err(Failed::from)
            } else {
                Ok(())
            }
        })
        .and_then(|()| {
            if animating {
                request_frame().map_err(Failed::from)
            } else {
                Ok(())
            }
        });
    Ok(reply(result))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        match input.r#type {
            CanvasEventType::State => {
                update_state(&mut game, decode_state(input.state.clone())?, false);
            }
            CanvasEventType::Key => handle_key(&mut game, &input)?,
            CanvasEventType::Pointer => handle_pointer(&mut game, &input)?,
            // The answer to a death reported shown is only fresh state, not the end of a guess.
            CanvasEventType::Reply => {
                let is_guess = input.seq != Some(SEEN_SEQ as i32);
                update_state(&mut game, decode_state(input.reply.clone())?, is_guess);
            }
            // If the report fails, the next canvas plays the death again; nothing to show here.
            CanvasEventType::Error if input.seq == Some(SEEN_SEQ as i32) => {}
            CanvasEventType::Error => refuse(&mut game, input.message),
            _ => {}
        }
        Ok::<(), Failed>(())
    });
    let result = result.and_then(|()| request_frame().map_err(Failed::from));
    Ok(reply(result))
}

// A shared blood update must not erase another player's half-typed guess or pending request.
fn update_state(game: &mut Game, state: GameState, is_reply: bool) {
    let same_attempt = game.state.round == state.round
        && game.state.started == state.started
        && game.state.complete == state.complete
        && game.state.guesses == state.guesses;
    if game.state.round == state.round {
        queue_effects(game, &state);
    } else {
        game.fx = Effects::default();
        game.acknowledge = false;
    }
    game.state = state;
    if is_reply || !same_attempt {
        game.current.clear();
        game.pending = false;
        game.notice = None;
        game.hovered_key = None;
        game.sent = None;
    }
}

// A guess's result arrives as the reply and again as a render push; only the first change starts
// anything. A win, a loss and fresh blood wait for the reveal so the flip is not spoiled.
fn queue_effects(game: &mut Game, new: &GameState) {
    let old = &game.state;
    let fx = &mut game.fx;
    if !game.still && new.guesses.len() > old.guesses.len() && new.guesses.starts_with(&old.guesses)
    {
        fx.start_reveal(new.guesses.len() - 1, letters(new));
    }
    let settle = (fx.reveal_ms - fx.reveal.elapsed(game.clock)).max(0.0);
    if !game.still && new.won && !old.won {
        fx.win = Anim::after(settle);
    }
    if new.complete && !new.won && !old.complete {
        fx.loss = Anim::after(settle);
        game.acknowledge |= new.fresh_loss;
    }
    if !game.still && new.blood.total > old.blood.total && !new.blood.stains.is_empty() {
        let fresh = ((new.blood.total - old.blood.total) as usize).min(new.blood.stains.len());
        fx.fresh = new.blood.stains.len() - fresh..new.blood.stains.len();
        fx.splat = Anim::after(settle);
    }
}

fn handle_key(game: &mut Game, input: &CanvasEventInput) -> Result<(), Failed> {
    if input.action.as_deref() != Some("down") {
        return Ok(());
    }
    let key = input.key.as_deref().unwrap_or("");
    if key == "Enter" {
        activate_key(game, "ENTER")?;
    } else if key == "Backspace" {
        activate_key(game, "DEL")?;
    } else if key.len() == 1 && key.as_bytes()[0].is_ascii_alphabetic() {
        activate_key(game, &key.to_ascii_uppercase())?;
    }
    Ok(())
}

fn handle_pointer(game: &mut Game, input: &CanvasEventInput) -> Result<(), Failed> {
    let action = input.action.as_deref();
    if action != Some("up") && action != Some("move") && action != Some("cancel") {
        return Ok(());
    }
    if action == Some("cancel") {
        game.hovered_key = None;
        return Ok(());
    }
    let css_width = input.width.unwrap_or(0.0) as f32;
    let css_height = input.height.unwrap_or(0.0) as f32;
    if css_width <= 0.0 || css_height <= 0.0 || game.width <= 0.0 || game.height <= 0.0 {
        return Ok(());
    }
    let scale = (game.width / WIDTH).min(game.height / HEIGHT);
    let left = (game.width - WIDTH * scale) / 2.0;
    let top = (game.height - HEIGHT * scale) / 2.0;
    let x = (input.x.unwrap_or(-1.0) as f32 * game.width / css_width - left) / scale;
    let y = (input.y.unwrap_or(-1.0) as f32 * game.height / css_height - top) / scale;
    if action == Some("move") {
        game.hovered_key = match input.pointer_type.as_deref() {
            Some("mouse" | "pen") => selectable_letter_at(game, x, y),
            _ => None,
        };
        return Ok(());
    }
    if let Some((key, _, _)) = keyboard_key_at(x, y) {
        activate_key(game, &key)?;
    }
    Ok(())
}

// Input waits for the reveal to finish, as the keyboard colours do.
fn accepting(game: &Game) -> bool {
    game.state.started
        && !game.state.suspended
        && !game.state.complete
        && !game.pending
        && !game.fx.revealing(game.clock)
}

fn revealing(game: &Game) -> Option<usize> {
    game.fx.revealing(game.clock).then_some(game.fx.reveal_row)
}

fn selectable_letter_at(game: &Game, x: f32, y: f32) -> Option<char> {
    if !accepting(game) || game.current.len() >= letters(&game.state) {
        return None;
    }
    keyboard_key_at(x, y).and_then(|(key, _, _)| {
        let letter = key_letter(&key)?;
        (!is_proven_invalid(letter, &game.state.guesses)).then_some(letter)
    })
}

fn activate_key(game: &mut Game, key: &str) -> Result<(), Failed> {
    game.fx.key_label = key.to_string();
    game.fx.key = Anim::after(0.0);
    if key == "ENTER" {
        submit(game)
    } else if key == "DEL" {
        if accepting(game) {
            game.current.pop();
        }
        Ok(())
    } else {
        add_letter(game, key.as_bytes()[0] as char);
        Ok(())
    }
}

fn add_letter(game: &mut Game, letter: char) {
    let letter = letter.to_ascii_uppercase();
    if accepting(game)
        && game.current.len() < letters(&game.state)
        && !is_proven_invalid(letter, &game.state.guesses)
    {
        game.current.push(letter);
        game.notice = None;
        if !game.still {
            game.fx.pops[game.current.len() - 1] = Anim::after(0.0);
        }
    }
}

// A refused guess goes back on the board so the player can fix it rather than retype it.
fn refuse(game: &mut Game, message: Option<String>) {
    game.pending = false;
    if let Some(word) = game.sent.take() {
        if game.current.is_empty() {
            game.current = word;
        }
    }
    game.notice = Some(
        message
            .map(|message| sentence(&message))
            .unwrap_or_else(|| "The guess could not be sent.".to_string()),
    );
    shake(game);
}

// The server words its refusals as lowercase phrases; the status line shows them as sentences.
fn sentence(message: &str) -> String {
    let mut chars = message.trim().chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut text: String = first.to_uppercase().chain(chars).collect();
    if !text.ends_with(['.', '!', '?']) {
        text.push('.');
    }
    text
}

fn shake(game: &mut Game) {
    if !game.still {
        game.fx.shake = Anim::after(0.0);
    }
}

// Whether the draft can be sent; a short draft shakes instead.
fn ready_to_send(game: &mut Game) -> bool {
    if !accepting(game) {
        return false;
    }
    if game.current.len() != letters(&game.state) {
        game.notice = Some("Not enough letters.".to_string());
        shake(game);
        return false;
    }
    true
}

fn submit(game: &mut Game) -> Result<(), Failed> {
    if !ready_to_send(game) {
        return Ok(());
    }
    game.seq = game.seq.wrapping_add(1).max(1);
    send(
        json!({ "type": "guess", "word": game.current.to_ascii_lowercase() }),
        Some(game.seq),
    )
    .map_err(Failed::from)?;
    game.sent = Some(std::mem::take(&mut game.current));
    game.pending = true;
    game.notice = None;
    Ok(())
}

fn decode_state(state: Option<Map<String, Value>>) -> Result<GameState, Failed> {
    match state {
        Some(state) => serde_json::from_value(Value::Object(state))
            .map_err(|error| Failed::new("invalid_state", error.to_string())),
        None => Ok(GameState::default()),
    }
}

fn draw() -> Result<(), Failed> {
    GAME.with(|cell| {
        let game = cell.borrow();
        let mut canvas = Canvas2D::new();
        let scale = (game.width / WIDTH).min(game.height / HEIGHT).max(0.1);
        let left = (game.width - WIDTH * scale) / 2.0;
        let top = (game.height - HEIGHT * scale) / 2.0;
        canvas
            .fill_color(0x101116ff)
            .fill_rect(0.0, 0.0, game.width, game.height);
        canvas.save().translate(left, top).scale(scale, scale);
        draw_scene(&mut canvas, &game);
        canvas.restore();
        canvas.submit().map_err(Failed::from)
    })
}

fn draw_scene(canvas: &mut Canvas2D, game: &Game) {
    canvas
        .save()
        .begin_path()
        .rect(0.0, 0.0, WIDTH, HEIGHT)
        .clip(FillRule::NonZero);
    let lost = game.state.complete && !game.state.won;
    let loss_ms = game.fx.loss.elapsed(game.clock);
    if lost && !game.still && (0.0..LOSS_MS).contains(&loss_ms) {
        let amplitude = 7.0 * (1.0 - loss_ms / LOSS_MS).powi(2);
        canvas.translate(
            (loss_ms * 0.11).sin() * amplitude,
            (loss_ms * 0.17).cos() * amplitude * 0.5,
        );
    }
    // Oversized so the room's edge never shows while it shakes.
    canvas
        .def_linear(
            1,
            0.0,
            0.0,
            WIDTH,
            HEIGHT,
            &[(0.0, 0x25252bff), (0.5, 0x17191fff), (1.0, 0x101116ff)],
        )
        .fill_gradient(1)
        .fill_rect(-12.0, -12.0, WIDTH + 24.0, HEIGHT + 24.0);
    // Stone courses, fine grain and brass frame, all deliberately still (no flashing effects).
    for row in 0..(HEIGHT / 34.0).ceil() as usize {
        let y = row as f32 * 34.0;
        canvas.fill_color(0x00000024).fill_rect(0.0, y, WIDTH, 1.0);
        for column in 0..5 {
            let x = column as f32 * 76.0 - if row % 2 == 0 { 0.0 } else { 38.0 };
            canvas.fill_rect(x, y, 1.0, 34.0);
        }
    }
    let mut grain = Noise(717);
    for _ in 0..300 {
        let x = grain.unit() * WIDTH;
        let y = grain.unit() * HEIGHT;
        canvas.fill_color(0xffffff07).fill_rect(x, y, 1.0, 1.0);
    }
    canvas.stroke_color(0xc5a16a42).line_width(1.0).stroke_rect(
        7.0,
        7.0,
        WIDTH - 14.0,
        HEIGHT - 14.0,
    );
    for &(x, y) in &[
        (11.0, 11.0),
        (WIDTH - 11.0, 11.0),
        (11.0, HEIGHT - 11.0),
        (WIDTH - 11.0, HEIGHT - 11.0),
    ] {
        disc(canvas, x, y, 1.5, GOLD);
    }
    draw_blood(
        canvas,
        &game.state.blood,
        Some((game.fx.fresh.clone(), game.fx.splat.elapsed(game.clock))),
    );
    if lost {
        // The room darkens to red at the edges and stays that way: a slow fade, never a flash.
        let alpha = (ease_out(progress(loss_ms, LOSS_MS)) * 120.0) as u32;
        canvas
            .def_radial(
                2,
                WIDTH / 2.0,
                HEIGHT / 2.0,
                90.0,
                WIDTH / 2.0,
                HEIGHT / 2.0,
                260.0,
                &[(0.0, 0x5a080c00), (1.0, 0x5a080c00 | alpha)],
            )
            .fill_gradient(2)
            .fill_rect(-12.0, -12.0, WIDTH + 24.0, HEIGHT + 24.0);
    }
    draw_health(canvas, game);
    draw_board(canvas, game);
    draw_status(canvas, game);
    draw_keyboard(canvas, game);
    draw_sparks(canvas, game);
    canvas.restore();
}

// Stable seeded geometry means rerenders never move old blood.
struct Noise(u32);
impl Noise {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / 16777216.0
    }
}

fn progress(ms: f32, duration: f32) -> f32 {
    (ms / duration).clamp(0.0, 1.0)
}

fn ease_out(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
}

fn ease_back(p: f32) -> f32 {
    let c = 1.70158;
    1.0 + (c + 1.0) * (p - 1.0).powi(3) + c * (p - 1.0).powi(2)
}

fn disc(canvas: &mut Canvas2D, x: f32, y: f32, radius: f32, color: u32) {
    canvas
        .fill_color(color)
        .begin_path()
        .arc(x, y, radius, 0.0, TAU, false)
        .fill(FillRule::NonZero);
}

/// Adds a circle to the current path, so many specks fill in one call.
fn dot(canvas: &mut Canvas2D, x: f32, y: f32, radius: f32) {
    canvas
        .move_to(x + radius, y)
        .arc(x, y, radius, 0.0, TAU, false);
}

fn shade(rgb: u32, factor: f32) -> u32 {
    let channel =
        |shift: u32| ((((rgb >> shift) & 0xff) as f32 * factor).min(255.0) as u32) << shift;
    channel(24) | channel(16) | channel(8)
}

fn tint(rgb: u32, alpha: f32) -> u32 {
    (rgb & 0xffffff00) | (alpha.clamp(0.0, 1.0) * 255.0) as u32
}

// Each stain's seed picks its kind: a cast-off trail, an impact, or a swing followed by the hit.
fn kind(seed: u32) -> u32 {
    ((seed ^ 0x9e3779b9).wrapping_mul(0x85ebca6b) >> 16) % 100
}

// Only the newest stains are drawn. Older ones have dried darker; the newest keeps the player's colour.
fn draw_blood(canvas: &mut Canvas2D, blood: &Blood, fresh: Option<(std::ops::Range<usize>, f32)>) {
    let skip = blood.stains.len().saturating_sub(MAX_STAINS);
    let last = (blood.stains.len() - skip).saturating_sub(1).max(1) as f32;
    for (index, stain) in blood.stains.iter().enumerate().skip(skip) {
        let swept = kind(stain.seed) < 35;
        let (grow, run) = match fresh {
            Some((ref range, ms)) if range.contains(&index) && ms < SPLAT_MS => (
                if swept {
                    ease_out(progress(ms, 320.0))
                } else {
                    ease_back(progress(ms, 240.0))
                },
                ease_out(progress(ms - 160.0, SPLAT_MS - 160.0)),
            ),
            _ => (1.0, 1.0),
        };
        if grow <= 0.01 {
            continue;
        }
        let age = (last - (index - skip) as f32) / last;
        let rgb = shade(stain.color, 1.0 - 0.28 * age);
        match kind(stain.seed) {
            0..=34 => draw_cast_off(canvas, stain.seed, rgb, grow, run),
            35..=84 => draw_impact(canvas, stain.seed, rgb, grow, run),
            _ => {
                draw_cast_off(canvas, stain.seed ^ 0x5bd1e995, rgb, grow, run);
                draw_impact(canvas, stain.seed, rgb, grow, run);
            }
        }
    }
}

// Stains sit in the gutters beside the board, behind every playing surface.
fn place(noise: &mut Noise) -> (bool, f32, f32) {
    let right = noise.unit() > 0.5;
    let x = if right {
        268.0 + noise.unit() * 38.0
    } else {
        14.0 + noise.unit() * 38.0
    };
    let y = BOARD_Y + 6.0 + noise.unit() * (HEIGHT - BOARD_Y - 90.0);
    (right, x, y)
}

// A deposit lobe: its offset along and across the line of travel, and its outline.
type Lobe = (f32, f32, Vec<(f32, f32)>);

#[derive(Debug, Clone, Copy)]
struct Drop {
    x: f32,
    y: f32,
    angle: f32,
    size: f32,
    elong: f32,
    tail: f32,
}

/// A drop stretched along its flight, with a thin spike ahead of it when it was moving fast.
fn streak_drop(canvas: &mut Canvas2D, drop: Drop) {
    let (c, s) = (drop.angle.cos(), drop.angle.sin());
    let rx = drop.size * drop.elong;
    canvas
        .move_to(drop.x + c * rx, drop.y + s * rx)
        .ellipse(drop.x, drop.y, rx, drop.size, drop.angle, 0.0, TAU, false);
    if drop.tail > 0.2 {
        let (bx, by, w) = (
            drop.x + c * rx * 0.55,
            drop.y + s * rx * 0.55,
            drop.size * 0.32,
        );
        canvas
            .move_to(bx + s * w, by - c * w)
            .line_to(drop.x + c * (rx + drop.tail), drop.y + s * (rx + drop.tail))
            .line_to(bx - s * w, by + c * w)
            .close_path();
    }
}

fn fill_drops(canvas: &mut Canvas2D, drops: &[Drop], color: u32) {
    canvas.fill_color(color).begin_path();
    for drop in drops {
        streak_drop(canvas, *drop);
    }
    canvas.fill(FillRule::NonZero);
}

/// A gravity run: wide where it leaves the stain, a thin wavering thread, a slightly heavier tip.
#[allow(clippy::too_many_arguments)]
fn thread(
    canvas: &mut Canvas2D,
    noise: &mut Noise,
    x: f32,
    top: f32,
    length: f32,
    width: f32,
    rgb: u32,
) {
    let wobble = (noise.unit() - 0.5) * 2.4;
    let neck = 0.45 + noise.unit() * 0.25;
    let tip = 1.05 + noise.unit() * 0.5;
    if length < 0.5 {
        return;
    }
    let bottom = top + length;
    let shoulder = top + (length * 0.3).min(6.0);
    for (color, k) in [(tint(rgb, 0.2), 1.7), (tint(shade(rgb, 0.62), 0.82), 1.0)] {
        let w = width * k;
        let nw = w * neck;
        let cx = x + wobble * 0.5;
        let rx = nw * tip;
        let ey = bottom - rx * 1.6;
        canvas
            .fill_color(color)
            .begin_path()
            .move_to(x + w, top)
            .bezier_to(
                x + nw + wobble * 0.3,
                shoulder,
                cx + nw + wobble * 0.5,
                top + length * 0.55,
                cx + rx,
                ey,
            )
            .ellipse(cx, ey, rx, rx * 1.6, 0.0, 0.0, PI, false)
            .bezier_to(
                cx - nw + wobble * 0.5,
                top + length * 0.55,
                x - nw + wobble * 0.3,
                shoulder,
                x - w,
                top,
            )
            .close_path()
            .fill(FillRule::NonZero);
    }
}

fn ceiling_thread(canvas: &mut Canvas2D, noise: &mut Noise, rgb: u32, run: f32) {
    let x = 16.0 + noise.unit() * 288.0;
    let length = (5.0 + noise.unit() * 30.0) * run;
    let width = 0.6 + noise.unit() * 0.7;
    thread(canvas, noise, x, 0.0, length, width, rgb);
}

/// An edge with a few broad lobes and a little fine jitter, like blood spread over rough stone.
/// `stretch` pushes the leading (+x) side out.
fn rough_blob(noise: &mut Noise, radius: f32, stretch: f32, count: usize) -> Vec<(f32, f32)> {
    let knots: Vec<f32> = (0..7).map(|_| 0.78 + noise.unit() * 0.4).collect();
    let lobes: Vec<(f32, f32, f32)> = (0..3)
        .map(|_| {
            (
                noise.unit() * TAU,
                0.18 + noise.unit() * 0.4,
                0.25 + noise.unit() * 0.35,
            )
        })
        .collect();
    (0..count)
        .map(|i| {
            let angle = i as f32 / count as f32 * TAU;
            let f = i as f32 / count as f32 * 7.0;
            let k = f.floor() as usize;
            let u = f - k as f32;
            let s = u * u * (3.0 - 2.0 * u);
            let mut r = knots[k % 7] * (1.0 - s) + knots[(k + 1) % 7] * s;
            for &(lobe, height, width) in &lobes {
                let distance = ((angle - lobe + PI * 3.0) % TAU - PI).abs();
                r += height * (1.0 - distance / width).max(0.0);
            }
            let r = radius * (r + (noise.unit() - 0.5) * 0.1);
            let push = if angle.cos() > 0.0 {
                1.0 + angle.cos() * (stretch - 1.0)
            } else {
                1.0
            };
            (angle.cos() * r * push, angle.sin() * r)
        })
        .collect()
}

/// Adds a blob to the current path as a smooth curve through its points' midpoints.
fn trace_blob(canvas: &mut Canvas2D, points: &[(f32, f32)], ox: f32, oy: f32, scale: f32) {
    let n = points.len();
    let mid = |a: (f32, f32), b: (f32, f32)| {
        (
            ox + (a.0 + b.0) / 2.0 * scale,
            oy + (a.1 + b.1) / 2.0 * scale,
        )
    };
    let start = mid(points[n - 1], points[0]);
    canvas.move_to(start.0, start.1);
    for i in 0..n {
        let point = points[i];
        let next = mid(point, points[(i + 1) % n]);
        canvas.quad_to(ox + point.0 * scale, oy + point.1 * scale, next.0, next.1);
    }
    canvas.close_path();
}

/// An impact thrown away from the board. The deposit is several merged lobes that taper along the
/// line of travel, flat in tone with a crisp darker drying edge, mottling and pores of stone; fingers
/// break up into chains of drops; a cone of spray thins and stretches with distance. About a third
/// of hits are spray only.
fn draw_impact(canvas: &mut Canvas2D, seed: u32, rgb: u32, grow: f32, run: f32) {
    let mut noise = Noise(seed);
    let (right, x, y) = place(&mut noise);
    let heavy = noise.unit() < 0.66;
    let r = if heavy {
        7.0 + noise.unit() * 8.0
    } else {
        3.0 + noise.unit() * 3.0
    };
    let heading = if right { 0.0 } else { PI } + (noise.unit() - 0.5) * 1.8;
    let stretch = 1.1 + noise.unit() * 0.4;
    let runs = if heavy {
        1 + (noise.unit() * 3.0) as usize
    } else {
        (noise.unit() * 1.5) as usize
    };
    // Runs first, so the deposit covers where they leave it.
    for _ in 0..runs {
        let along = (noise.unit() - 0.5) * r * 1.1;
        let length = (6.0 + noise.unit().powf(1.6) * 64.0) * run;
        let width = 0.6 + noise.unit() * if heavy { 1.1 } else { 0.5 };
        thread(
            canvas,
            &mut noise,
            x + along,
            y + r * 0.3,
            length,
            width,
            rgb,
        );
    }
    ceiling_thread(canvas, &mut noise, rgb, run);

    canvas
        .save()
        .translate(x, y)
        .scale(grow, grow)
        .rotate(heading);
    canvas.fill_color(tint(rgb, 0.28)).begin_path();
    for _ in 0..46 {
        let angle = (noise.unit() - 0.5) * 1.5 * noise.unit();
        let distance = r * (1.6 + noise.unit() * 5.2);
        let size = 0.18 + noise.unit() * 0.3;
        dot(canvas, angle.cos() * distance, angle.sin() * distance, size);
    }
    canvas.fill(FillRule::NonZero);
    // Spray is denser and larger at the cone's centre, smaller, sparser and longer at its edge.
    let mut small = Vec::new();
    let mut large = Vec::new();
    for _ in 0..34 {
        let offset = (noise.unit() - 0.5) * (noise.unit() + 0.35);
        let angle = offset * if noise.unit() > 0.9 { 4.5 } else { 1.8 };
        let reach = noise.unit();
        let distance = r * (1.1 + reach * reach * 4.0);
        let centre = 1.0 - (angle.abs() / 1.2).min(1.0);
        let size =
            (0.25 + noise.unit().powf(2.2) * 2.2) * (0.5 + centre * 0.8) * (1.15 - reach * 0.65);
        let elong = 1.1 + reach * 2.0 + noise.unit() * 0.5;
        let tail = if reach > 0.3 {
            size * elong * reach * 2.4
        } else {
            0.0
        };
        let drop = Drop {
            x: angle.cos() * distance,
            y: angle.sin() * distance,
            angle,
            size,
            elong,
            tail,
        };
        if size > 0.95 {
            large.push(drop);
        } else {
            small.push(drop);
        }
    }
    fill_drops(canvas, &small, tint(rgb, 0.5));
    fill_drops(canvas, &large, tint(shade(rgb, 0.62), 0.88));

    if heavy {
        // Fingers leave the leading edge and break into a chain of shrinking drops.
        canvas.fill_color(tint(shade(rgb, 0.62), 0.86)).begin_path();
        let fingers = 4 + (noise.unit() * 4.0) as usize;
        for _ in 0..fingers {
            let angle = (noise.unit() - 0.5) * 1.7;
            let start = r * 0.8;
            let length = r * (0.4 + noise.unit() * 1.1);
            let width = r * (0.07 + noise.unit() * 0.08);
            let bend = (noise.unit() - 0.5) * length * 0.3;
            let (co, si) = (angle.cos(), angle.sin());
            let (px, py) = (-si, co);
            let tip = start + length;
            let half = start + length * 0.5;
            canvas
                .move_to(co * start - px * width, si * start - py * width)
                .quad_to(
                    co * half + px * bend,
                    si * half + py * bend,
                    co * tip,
                    si * tip,
                )
                .quad_to(
                    co * half + px * bend,
                    si * half + py * bend,
                    co * start + px * width,
                    si * start + py * width,
                )
                .close_path();
            let mut along = tip + width * 2.5;
            let mut size = width * (0.9 + noise.unit() * 0.5);
            let chain = 1 + (noise.unit() * 3.0) as usize;
            for link in 0..chain {
                streak_drop(
                    canvas,
                    Drop {
                        x: co * along,
                        y: si * along,
                        angle,
                        size,
                        elong: 1.6 + link as f32 * 0.5,
                        tail: 0.0,
                    },
                );
                along += size * (3.0 + noise.unit() * 3.0);
                size *= 0.68;
            }
        }
        canvas.fill(FillRule::NonZero);

        // The deposit: three merged lobes, each smaller and further along the line of travel.
        let second_y = (noise.unit() - 0.5) * r * 0.5;
        let third_y = (noise.unit() - 0.5) * r * 0.6;
        let lobes: Vec<Lobe> = [
            (0.0, 0.0, 1.0),
            (r * 0.55, second_y, 0.68),
            (r, third_y, 0.42),
        ]
        .into_iter()
        .map(|(ox, oy, k)| (ox, oy, rough_blob(&mut noise, r * k, stretch, 28)))
        .collect();
        for (scale, color) in [
            (1.12, tint(rgb, 0.12)),
            (1.0, tint(shade(rgb, 0.42), 0.92)),
            (0.9, tint(shade(rgb, 0.62), 0.9)),
        ] {
            canvas.fill_color(color).begin_path();
            for (ox, oy, points) in &lobes {
                trace_blob(canvas, points, *ox, *oy, scale);
            }
            canvas.fill(FillRule::NonZero);
        }
        // Mottling: darker where it pooled, lighter where it thinned.
        for i in 0..5 {
            let (ox, oy, _) = lobes[i % 3];
            let angle = noise.unit() * TAU;
            let distance = noise.unit() * r * 0.4;
            let size = r * (0.18 + noise.unit() * 0.22);
            let tone = if i % 2 == 1 {
                tint(shade(rgb, 0.45), 0.35)
            } else {
                tint(shade(rgb, 0.85), 0.18)
            };
            let points = rough_blob(&mut noise, size, 1.2, 14);
            canvas.fill_color(tone).begin_path();
            trace_blob(
                canvas,
                &points,
                ox + angle.cos() * distance,
                oy + angle.sin() * distance,
                1.0,
            );
            canvas.fill(FillRule::NonZero);
        }
        canvas.fill_color(0x18191f42).begin_path();
        for _ in 0..22 {
            let angle = noise.unit() * TAU;
            let distance = noise.unit().sqrt() * r * 0.85;
            let size = 0.22 + noise.unit() * 0.45;
            dot(
                canvas,
                angle.cos() * distance,
                angle.sin() * distance * 0.9,
                size,
            );
        }
        canvas.fill(FillRule::NonZero);
    }
    canvas.restore();
}

/// A swung blade's cast-off: an uneven curved trail of stretched drops with darker cores and gaps,
/// flecks thrown ahead of the larger drops, and a few short runs. `grow` sweeps the trail in.
fn draw_cast_off(canvas: &mut Canvas2D, seed: u32, rgb: u32, grow: f32, run: f32) {
    let mut noise = Noise(seed);
    let (right, x, y) = place(&mut noise);
    let dir = if right { -1.0 } else { 1.0 };
    let p0 = (x, y - 20.0 - noise.unit() * 30.0);
    let p2x = x + dir * (noise.unit() * 30.0 - 8.0);
    let p2 = (p2x, p0.1 + 90.0 + noise.unit() * 90.0);
    let p1x = x + dir * (26.0 + noise.unit() * 30.0);
    let p1 = (p1x, (p0.1 + p2.1) / 2.0 + (noise.unit() - 0.5) * 30.0);
    let at = |t: f32| {
        let u = 1.0 - t;
        (
            u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
            u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
        )
    };
    let sweep = grow.min(1.0);
    let count = 14 + (noise.unit() * 8.0) as usize;
    let mut drops = Vec::new();
    let mut cores = Vec::new();
    let mut flecks = Vec::new();
    let mut runs = Vec::new();
    for i in 0..count {
        let t = ((i as f32 + (noise.unit() - 0.5) * 0.6) / (count - 1) as f32).clamp(0.0, 1.0);
        let gap = noise.unit() < 0.12;
        let p = at(t);
        let q = at((t + 0.01).min(1.0));
        let angle = (q.1 - p.1).atan2(q.0 - p.0);
        let jitter = (noise.unit() - 0.5) * 2.2;
        let size = (1.7 - t * 1.0) * (0.8 + noise.unit() * 0.4);
        let elong = 1.5 + t * 2.6 + noise.unit() * 0.5;
        let tail = size * elong * (0.6 + t * 1.6);
        let has_run = size > 1.5 && noise.unit() > 0.5;
        let length = (4.0 + noise.unit() * 18.0) * run;
        let fleck = noise.unit() > 0.6;
        let fleck_distance = size * elong * 2.2 + noise.unit() * 4.0;
        let fleck_size = 0.25 + noise.unit() * 0.4;
        if gap || t > sweep {
            continue;
        }
        let px = p.0 - angle.sin() * jitter;
        let py = p.1 + angle.cos() * jitter;
        drops.push(Drop {
            x: px,
            y: py,
            angle,
            size,
            elong,
            tail,
        });
        cores.push(Drop {
            x: px - angle.cos() * size * 0.4,
            y: py - angle.sin() * size * 0.4,
            angle,
            size: size * 0.55,
            elong: elong * 0.8,
            tail: 0.0,
        });
        if fleck {
            flecks.push((
                px + angle.cos() * fleck_distance,
                py + angle.sin() * fleck_distance,
                fleck_size,
            ));
        }
        if has_run {
            runs.push((px, py + size * 0.5, length, size * 0.38));
        }
    }
    for (rx, ry, length, width) in runs {
        thread(canvas, &mut noise, rx, ry, length, width, rgb);
    }
    fill_drops(canvas, &drops, tint(rgb, 0.74));
    fill_drops(canvas, &cores, tint(shade(rgb, 0.48), 0.6));
    canvas.fill_color(tint(rgb, 0.45)).begin_path();
    for (fx, fy, size) in flecks {
        dot(canvas, fx, fy, size);
    }
    canvas.fill(FillRule::NonZero);
    ceiling_thread(canvas, &mut noise, rgb, run);
}

fn maximum_health(state: &GameState) -> usize {
    state.max_health.unwrap_or(MAX_HEALTH).clamp(1, MAX_HEALTH)
}

// Keep health in step with the tile reveal, not the early server reply. Old states without health
// still draw correctly from their saved feedback. A correct guess never costs health.
fn visible_health(game: &Game) -> Option<usize> {
    if !game.state.started {
        return None;
    }
    let max = maximum_health(&game.state);
    let wrong = |guess: &Guess| guess.marks.is_empty() || guess.marks.iter().any(|mark| *mark != 2);
    let health = game
        .state
        .health
        .unwrap_or_else(|| {
            max.saturating_sub(
                game.state
                    .guesses
                    .iter()
                    .filter(|guess| wrong(guess))
                    .count(),
            )
        })
        .min(max);
    let hidden_hit = revealing(game)
        .and_then(|row| game.state.guesses.get(row))
        .is_some_and(wrong);
    Some((health + usize::from(hidden_hit)).min(max))
}

fn draw_health(canvas: &mut Canvas2D, game: &Game) {
    let max = maximum_health(&game.state);
    let health = visible_health(game);
    let color = match health {
        Some(4..) => 0x83b37fff,
        Some(2..=3) => GOLD,
        Some(_) => 0xe2a59aff,
        None => 0x726f6cff,
    };
    canvas
        .fill_color(0x101116f5)
        .fill_rect(17.0, 8.0, 286.0, 12.0);
    let label = health.map_or_else(|| "HEALTH —".to_string(), |hp| format!("HEALTH {hp}/{max}"));
    draw_text(canvas, &label, 53.0, 13.0, 8.0, 700.0, color);
    let width = (206.0 - (max - 1) as f32 * 3.0) / max as f32;
    for segment in 0..max {
        let x = 94.0 + segment as f32 * (width + 3.0);
        canvas.fill_color(0x38252cff).fill_rect(x, 9.0, width, 8.0);
        if health.is_some_and(|hp| segment < hp) {
            canvas
                .fill_color(color)
                .fill_rect(x + 1.0, 10.0, width - 2.0, 6.0);
        }
        canvas
            .stroke_color(0xc5a16a66)
            .line_width(0.5)
            .stroke_rect(x, 9.0, width, 8.0);
    }
}

fn draw_board(canvas: &mut Canvas2D, game: &Game) {
    let length = letters(&game.state);
    let board = board_for(length);
    let left = board.left;
    let tile = board.tile;
    let radius = (tile / 5.0).min(4.0);
    let stagger = flip_stagger(length);
    let active_row = game.state.guesses.len();
    let reveal_row = revealing(game);
    let reveal_ms = game.fx.reveal.elapsed(game.clock);
    let win_ms = game.fx.win.elapsed(game.clock);
    let shake_ms = game.fx.shake.elapsed(game.clock);
    for row in 0..6 {
        let saved = game.state.guesses.get(row);
        let shake_x = if row == active_row && (0.0..SHAKE_MS).contains(&shake_ms) {
            (shake_ms * 0.075).sin() * 6.0 * (1.0 - shake_ms / SHAKE_MS)
        } else {
            0.0
        };
        for column in 0..length {
            let x = left + column as f32 * (tile + board.gap);
            let y = BOARD_Y + row as f32 * (TILE + TILE_GAP);
            let letter = saved
                .and_then(|guess| guess.word.chars().nth(column))
                .or_else(|| {
                    (row == active_row)
                        .then(|| game.current.chars().nth(column))
                        .flatten()
                });
            // A revealing tile shows its feedback colour only once it has turned edge-on.
            let (flip, hidden) = if reveal_row == Some(row) {
                let p = progress(reveal_ms - column as f32 * stagger, FLIP_MS);
                ((p * PI).cos().abs().max(0.04), p < 0.5)
            } else {
                (1.0, false)
            };
            let lift = if game.state.won && row + 1 == game.state.guesses.len() {
                let p = progress(win_ms - column as f32 * (450.0 / length as f32), BOUNCE_MS);
                if p < 1.0 {
                    -(p * PI).sin() * 11.0
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let pop = if row == active_row {
                let p = progress(game.fx.pops[column].elapsed(game.clock), POP_MS);
                if p < 1.0 {
                    1.0 + (p * PI).sin() * 0.13
                } else {
                    1.0
                }
            } else {
                1.0
            };
            let mark = saved
                .filter(|_| !hidden)
                .and_then(|guess| guess.marks.get(column).copied())
                .unwrap_or(0);
            let color = match mark {
                2 => 0x538d4eff,
                1 => 0xb59f3bff,
                _ if saved.is_some_and(|guess| !guess.marks.is_empty()) && !hidden => 0x3a3a3cff,
                _ => 0x202229ff,
            };
            let active = game.state.started && !game.state.complete && row == active_row;
            let (cx, cy) = (x + tile / 2.0, y + TILE / 2.0);
            canvas
                .save()
                .translate(cx + shake_x, cy + lift)
                .scale(pop, pop * flip)
                .translate(-cx, -cy);
            canvas
                .fill_color(0x00000050)
                .begin_path()
                .round_rect(x, y + 2.0, tile, TILE, radius)
                .fill(FillRule::NonZero);
            canvas
                .fill_color(color)
                .begin_path()
                .round_rect(x, y, tile, TILE, radius)
                .fill(FillRule::NonZero)
                .stroke_color(if hidden {
                    0x8a7350ff
                } else if active && letter.is_some() {
                    GOLD
                } else {
                    0x5b5965aa
                })
                .line_width(1.0)
                .stroke();
            canvas
                .fill_color(0xffffff0c)
                .fill_rect(x + tile * 0.125, y + 1.0, tile * 0.75, 1.0);
            if active && column == game.current.len() && !game.pending && reveal_row.is_none() {
                canvas.fill_color(GOLD).fill_rect(
                    x + tile * 0.34,
                    y + TILE - 7.0,
                    tile * 0.375,
                    2.0,
                );
            }
            if let Some(letter) = letter {
                draw_text(
                    canvas,
                    &letter.to_ascii_uppercase().to_string(),
                    cx,
                    cy,
                    (tile * 0.75).min(19.0),
                    700.0,
                    INK,
                );
            }
            canvas.restore();
        }
        let used = row < game.state.guesses.len();
        draw_text(
            canvas,
            &format!("{:02}", row + 1),
            left - 16.0,
            BOARD_Y + row as f32 * (TILE + TILE_GAP) + TILE / 2.0,
            8.0,
            700.0,
            if used { GOLD } else { 0x726f6cff },
        );
        if used && reveal_row != Some(row) {
            let (result, result_color) = if saved.is_some_and(|guess| guess.marks.is_empty()) {
                ("!", GOLD)
            } else if saved.is_some_and(|guess| guess.marks.iter().all(|mark| *mark == 2)) {
                (
                    "+",
                    if game.state.won && row + 1 == game.state.guesses.len() {
                        0x83b37fff
                    } else {
                        GOLD
                    },
                )
            } else {
                ("×", 0xb58a80ff)
            };
            draw_text(
                canvas,
                result,
                left + board.width + 16.0,
                BOARD_Y + row as f32 * (TILE + TILE_GAP) + TILE / 2.0,
                13.0,
                700.0,
                result_color,
            );
        }
    }
}

fn letters(state: &GameState) -> usize {
    state
        .length
        .unwrap_or(DEFAULT_LETTERS)
        .clamp(1, MAX_LETTERS)
}

fn flip_stagger(letters: usize) -> f32 {
    (FLIP_SPREAD_MS / letters.saturating_sub(1).max(1) as f32).min(FLIP_STAGGER_MS)
}

fn reveal_duration(letters: usize) -> f32 {
    flip_stagger(letters) * letters.saturating_sub(1) as f32 + FLIP_MS
}

struct Board {
    left: f32,
    tile: f32,
    gap: f32,
    width: f32,
}

// Tiles are full size until the word outgrows BOARD_MAX_WIDTH; then they narrow but keep their
// height, so the rows, status and keyboard never move.
fn board_for(letters: usize) -> Board {
    let n = letters as f32;
    let gap = if letters > 6 { 2.0 } else { TILE_GAP };
    let tile = ((BOARD_MAX_WIDTH - gap * (n - 1.0)) / n).min(TILE);
    let width = tile * n + gap * (n - 1.0);
    Board {
        left: (WIDTH - width) / 2.0,
        tile,
        gap,
        width,
    }
}

// Confetti-like sparks thrown up from the winning row; each spark's path is a pure function of time.
fn draw_sparks(canvas: &mut Canvas2D, game: &Game) {
    let ms = game.fx.win.elapsed(game.clock);
    if !(0.0..WIN_MS).contains(&ms) || game.state.guesses.is_empty() {
        return;
    }
    let board = board_for(letters(&game.state));
    let row_y = BOARD_Y + (game.state.guesses.len() - 1) as f32 * (TILE + TILE_GAP) + TILE / 2.0;
    let mut noise = Noise(0x5eed);
    let colors = [GOLD, 0x83b37fff, INK];
    for index in 0..48 {
        let x0 = board.left + noise.unit() * board.width;
        let angle = -FRAC_PI_2 + (noise.unit() - 0.5) * 2.6;
        let speed = 90.0 + noise.unit() * 190.0;
        let delay = noise.unit() * 300.0;
        let life = 0.8 + noise.unit() * 0.8;
        let size = 1.2 + noise.unit() * 1.8;
        let spin = (noise.unit() - 0.5) * 14.0;
        let t = (ms - delay) / 1000.0;
        if !(0.0..=life).contains(&t) {
            continue;
        }
        canvas
            .save()
            .translate(
                x0 + angle.cos() * speed * t,
                row_y + angle.sin() * speed * t + 210.0 * t * t,
            )
            .rotate(spin * t)
            .set_alpha(1.0 - t / life)
            .fill_color(colors[index % 3])
            .fill_rect(-size / 2.0, -size, size, size * 2.0)
            .restore();
    }
}

fn draw_status(canvas: &mut Canvas2D, game: &Game) {
    canvas
        .fill_color(0x101116ec)
        .begin_path()
        .round_rect(19.0, STATUS_Y, 282.0, STATUS_HEIGHT, 5.0)
        .fill(FillRule::NonZero)
        .stroke_color(0xc5a16a30)
        .stroke();
    let revealing = revealing(game).is_some();
    let status = if game.state.suspended && !game.state.complete {
        "You are suspended and cannot play Hurtle."
    } else if !game.state.started {
        "Accept your sentence above to begin."
    } else if revealing {
        "The executioner reads your word…"
    } else if game.state.won {
        "REPRIEVE GRANTED. You keep your head."
    } else if game.state.complete && game.state.penalty_applied {
        "Health depleted. Suspended for one hour."
    } else if game.state.complete && game.state.penalty_pending {
        "Health depleted. Applying one-hour suspension."
    } else if game.state.penalty_pending || game.pending {
        "Checking guess..."
    } else if game
        .state
        .guesses
        .last()
        .is_some_and(|guess| guess.marks.is_empty())
    {
        "No clues revealed. One health lost."
    } else if let Some(error) = game
        .state
        .penalty_error
        .as_deref()
        .or(game.notice.as_deref())
    {
        error
    } else if game.state.complete {
        "Game over."
    } else {
        "The blade waits. Make your guess."
    };
    draw_wrapped_status(canvas, status);

    if game.state.won && !revealing {
        if let Some(answer) = game.state.answer.as_deref() {
            draw_text(
                canvas,
                &format!("Word: {}", answer.to_ascii_uppercase()),
                WIDTH / 2.0,
                STATUS_Y + 32.0,
                10.0,
                700.0,
                INK,
            );
        }
    } else {
        let fresh = (0.0..900.0).contains(&game.fx.splat.elapsed(game.clock));
        draw_text(
            canvas,
            &format!(
                "{} SPLATTERS TODAY · THE WALLS REMEMBER",
                game.state.blood.total
            ),
            WIDTH / 2.0,
            STATUS_Y + 32.0,
            7.0,
            700.0,
            if fresh { 0xe2a59aff } else { GOLD },
        );
    }
}

fn draw_wrapped_status(canvas: &mut Canvas2D, text: &str) {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let current = lines.last().expect("status always has a line");
        let extra = usize::from(!current.is_empty());
        if current.chars().count() + extra + word.chars().count() > 42 && lines.len() < 2 {
            lines.push(word.to_string());
        } else {
            let line = lines.last_mut().expect("status always has a line");
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    for (index, line) in lines.iter().take(2).enumerate() {
        draw_text(
            canvas,
            &ellipsize(line, 46),
            WIDTH / 2.0,
            STATUS_Y + 10.0 + index as f32 * 11.0,
            10.0,
            400.0,
            INK,
        );
    }
}

fn ellipsize(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        text.chars()
            .take(limit.saturating_sub(1))
            .chain(['…'])
            .collect()
    }
}

fn draw_keyboard(canvas: &mut Canvas2D, game: &Game) {
    let current_index =
        (game.current.chars().count() < letters(&game.state)).then(|| game.current.chars().count());
    // A guess's letters colour the keys only after its tiles have turned.
    let shown = match revealing(game) {
        Some(row) => &game.state.guesses[..row.min(game.state.guesses.len())],
        None => &game.state.guesses[..],
    };
    let palette = letter_palette(shown, current_index);
    let pressed = game
        .fx
        .key
        .active(game.clock, KEY_MS)
        .then_some(game.fx.key_label.as_str());
    for row in 0..3 {
        let y = KEYBOARD_Y + row as f32 * KEY_STEP;
        for (label, x, width) in keyboard_row(row) {
            let letter = key_letter(&label);
            let color = letter
                .and_then(|letter| palette.get(&letter).copied())
                .unwrap_or(if label == "ENTER" {
                    0x796143ff
                } else {
                    0x4b4b55ff
                });
            let enabled = game.state.started
                && !game.state.suspended
                && !game.state.complete
                && !game.pending;
            let hovered = enabled && letter.is_some() && game.hovered_key == letter;
            let press = if pressed == Some(label.as_str()) {
                2.0
            } else {
                0.0
            };
            canvas.save().set_alpha(if enabled { 1.0 } else { 0.55 });
            canvas
                .fill_color(0x00000066)
                .begin_path()
                .round_rect(x, y + 2.0, width, KEY_HEIGHT, 4.0)
                .fill(FillRule::NonZero);
            canvas
                .fill_color(color)
                .begin_path()
                .round_rect(x, y + press, width, KEY_HEIGHT, 4.0)
                .fill(FillRule::NonZero);
            if press > 0.0 {
                canvas.fill_color(0xffffff29).fill(FillRule::NonZero);
            }
            canvas
                .stroke_color(if hovered { INK } else { 0xffffff19 })
                .line_width(1.0)
                .stroke();
            draw_text(
                canvas,
                &label,
                x + width / 2.0,
                y + KEY_HEIGHT / 2.0 + press,
                if width > 30.0 { 9.0 } else { 12.0 },
                700.0,
                INK,
            );
            canvas.restore();
        }
    }
}

// The letter a key types; ENTER and DEL type none, so they never share a letter's colour or hover.
fn key_letter(label: &str) -> Option<char> {
    let mut chars = label.chars();
    chars.next().filter(|_| chars.next().is_none())
}

fn letter_palette(
    guesses: &[Guess],
    current_index: Option<usize>,
) -> std::collections::HashMap<char, u32> {
    let mut colors = std::collections::HashMap::new();
    for guess in guesses {
        for (index, (letter, mark)) in guess
            .word
            .chars()
            .zip(guess.marks.iter().copied())
            .enumerate()
        {
            let letter = letter.to_ascii_uppercase();
            let color = match mark {
                2 if current_index == Some(index) => 0x538d4eff,
                1 | 2 => 0xb59f3bff,
                _ => 0x3a3a3cff,
            };
            let previous = colors.get(&letter).copied().unwrap_or(0x00000000);
            let priority = |color| match color {
                0x538d4eff => 3,
                0xb59f3bff => 2,
                0x3a3a3cff => 1,
                _ => 0,
            };
            if priority(color) > priority(previous) {
                colors.insert(letter, color);
            }
        }
    }
    colors
}

fn is_proven_invalid(letter: char, guesses: &[Guess]) -> bool {
    let letter = letter.to_ascii_uppercase();
    let mut seen = false;
    let mut found = false;
    for guess in guesses {
        for (guessed_letter, mark) in guess.word.chars().zip(guess.marks.iter().copied()) {
            if guessed_letter.to_ascii_uppercase() == letter {
                seen = true;
                found |= mark > 0;
            }
        }
    }
    seen && !found
}

fn keyboard_row(row: usize) -> Vec<(String, f32, f32)> {
    let (labels, widths): (Vec<String>, Vec<f32>) = match row {
        0 => (
            KEY_ROWS[0]
                .chars()
                .map(|letter| letter.to_string())
                .collect(),
            vec![25.0; 10],
        ),
        1 => (
            KEY_ROWS[1]
                .chars()
                .map(|letter| letter.to_string())
                .collect(),
            vec![26.0; 9],
        ),
        _ => {
            let mut labels = vec!["ENTER".to_string()];
            labels.extend(KEY_ROWS[2].chars().map(|letter| letter.to_string()));
            labels.push("DEL".to_string());
            let mut widths = vec![42.0];
            widths.extend(vec![25.0; 7]);
            widths.push(42.0);
            (labels, widths)
        }
    };
    let gap = 3.0;
    let total = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1) as f32);
    let mut x = (WIDTH - total) / 2.0;
    labels
        .into_iter()
        .zip(widths)
        .map(|(label, width)| {
            let key = (label, x, width);
            x += width + gap;
            key
        })
        .collect()
}

fn keyboard_key_at(x: f32, y: f32) -> Option<(String, f32, f32)> {
    if !(KEYBOARD_Y..=KEYBOARD_Y + 2.0 * KEY_STEP + KEY_HEIGHT).contains(&y) {
        return None;
    }
    let row = ((y - KEYBOARD_Y) / KEY_STEP).floor() as usize;
    if y > KEYBOARD_Y + row as f32 * KEY_STEP + KEY_HEIGHT {
        return None;
    }
    keyboard_row(row.min(2))
        .into_iter()
        .find(|(_, left, width)| x >= *left && x <= *left + *width)
}

fn draw_text(
    canvas: &mut Canvas2D,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    weight: f32,
    color: u32,
) {
    canvas
        .font("sans-serif", size, weight)
        .fill_color(color)
        .text_align(TextAlign::Center)
        .text_baseline(TextBaseline::Middle)
        .fill_text(text, x, y);
}

fn reply(result: Result<(), Failed>) -> CanvasReply {
    CanvasReply {
        error: result.err().map(|error| HookError {
            code: error.code,
            message: error.message,
        }),
    }
}

#[derive(Debug)]
struct Failed {
    code: String,
    message: String,
}

impl Failed {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl From<chittr_cartridge::canvas::CanvasError> for Failed {
    fn from(error: chittr_cartridge::canvas::CanvasError) -> Self {
        Self::new(&error.code, error.message)
    }
}

impl From<Error> for Failed {
    fn from(error: Error) -> Self {
        Self::new("host", error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod health;

    // The fixtures play five-letter words, as games saved before the switch to twelve do.
    const REVEAL_MS: f32 = FLIP_STAGGER_MS * 4.0 + FLIP_MS;

    fn started_game() -> Game {
        let mut game = Game::default();
        game.state.started = true;
        game.state.round = Some(12);
        game.state.length = Some(5);
        game
    }

    fn crane() -> Guess {
        Guess {
            word: "crane".into(),
            marks: vec![2, 1, 0, 0, 1],
        }
    }

    // Runs the frame clock forward the way the host would, resolving queued effects.
    fn tick(game: &mut Game, now: f64) {
        game.clock = now;
        game.fx.resolve(now);
    }

    fn stains(count: u64) -> Blood {
        Blood {
            total: count,
            stains: (0..count)
                .map(|index| Bloodstain {
                    color: [0x9b2638ff, 0x0088ccff, 0x9b65c9ff, 0xd59020ff, 0x378a62ff]
                        [index as usize % 5],
                    seed: (index as u32).wrapping_mul(2654435761).wrapping_add(42),
                })
                .collect(),
        }
    }

    #[test]
    fn keyboard_colors_reflect_feedback_at_the_current_guess_index() {
        let guesses = [Guess {
            word: "crane".to_string(),
            marks: vec![2, 1, 0, 0, 0],
        }];

        let palette = letter_palette(&guesses, Some(0));
        assert_eq!(palette[&'C'], 0x538d4eff);
        assert_eq!(palette[&'R'], 0xb59f3bff);
        assert_eq!(palette[&'A'], 0x3a3a3cff);

        let palette = letter_palette(&guesses, Some(1));
        assert_eq!(palette[&'C'], 0xb59f3bff);
        assert_eq!(palette[&'R'], 0xb59f3bff);
    }

    #[test]
    fn keyboard_keeps_the_strongest_feedback_for_repeated_letters() {
        let guesses = [
            Guess {
                word: "CRANE".to_string(),
                marks: vec![0, 0, 1, 0, 0],
            },
            Guess {
                word: "ALARM".to_string(),
                marks: vec![2, 0, 0, 0, 0],
            },
        ];

        let palette = letter_palette(&guesses, Some(0));
        assert_eq!(palette[&'A'], 0x538d4eff);
    }

    #[test]
    fn selectable_letter_requires_an_active_game_and_a_letter_key() {
        let mut game = Game::default();
        assert_eq!(selectable_letter_at(&game, 34.0, KEYBOARD_Y + 15.0), None);

        game.state.started = true;
        assert_eq!(
            selectable_letter_at(&game, 34.0, KEYBOARD_Y + 15.0),
            Some('Q')
        );
        assert_eq!(
            selectable_letter_at(&game, 34.0, KEYBOARD_Y + 2.0 * KEY_STEP + 15.0),
            None
        );

        game.state.guesses.push(Guess {
            word: "quick".to_string(),
            marks: vec![0, 1, 0, 0, 0],
        });
        assert_eq!(selectable_letter_at(&game, 34.0, KEYBOARD_Y + 15.0), None);
        assert_eq!(
            selectable_letter_at(&game, 205.0, KEYBOARD_Y + 15.0),
            Some('U')
        );

        add_letter(&mut game, 'Q');
        assert!(game.current.is_empty());

        game.pending = true;
        assert_eq!(selectable_letter_at(&game, 34.0, KEYBOARD_Y + 15.0), None);
    }

    #[test]
    fn shared_stains_preserve_the_draft_but_a_new_word_or_reply_resets_it() {
        let mut game = started_game();
        game.current = "CR".into();
        let mut state = game.state.clone();
        state.blood.total = 1;
        update_state(&mut game, state.clone(), false);
        assert_eq!(game.current, "CR");
        assert_eq!(game.state.blood.total, 1);
        game.pending = true;
        update_state(&mut game, state.clone(), false);
        assert!(game.pending);
        update_state(&mut game, state.clone(), true);
        assert!(!game.pending);
        assert!(game.current.is_empty());
        game.current = "CR".into();
        state.round = Some(13);
        update_state(&mut game, state, false);
        assert!(game.current.is_empty());
    }

    #[test]
    fn server_state_decodes_profile_blood_and_penalty_fields() {
        let state = decode_state(Some(
            serde_json::from_value(json!({
                "started": true, "penaltyApplied": true, "penaltyPending": false,
                "penaltyError": "permission denied",
                "blood": { "total": 3, "stains": [{ "color": 0x0088ccffu32, "seed": 42 }] }
            }))
            .unwrap(),
        ))
        .unwrap();
        assert!(state.penalty_applied);
        assert_eq!(state.penalty_error.as_deref(), Some("permission denied"));
        assert_eq!(state.blood.total, 3);
        assert_eq!(state.blood.stains[0].color, 0x0088ccff);
        assert_eq!(decode_state(None).unwrap().blood.total, 0);
    }

    #[test]
    fn blood_is_deterministic_progressive_and_bounded() {
        let stain = || Bloodstain {
            color: 0x0088ccff,
            seed: 42,
        };
        let draw = |blood: &Blood| {
            let mut canvas = Canvas2D::new();
            draw_blood(&mut canvas, blood, None);
            canvas
        };
        assert!(draw(&Blood::default()).is_empty());
        let one = draw(&Blood {
            total: 1,
            stains: vec![stain()],
        });
        let repeat = draw(&Blood {
            total: 1,
            stains: vec![stain()],
        });
        assert_eq!(one.bytes(), repeat.bytes());
        let many = draw(&Blood {
            total: 64,
            stains: (0..64).map(|_| stain()).collect(),
        });
        assert!(many.len() > one.len());
        let excess = draw(&Blood {
            total: 100,
            stains: (0..100).map(|_| stain()).collect(),
        });
        assert_eq!(many.bytes(), excess.bytes());
    }

    #[test]
    fn the_mix_draws_cast_offs_impacts_and_both() {
        let kinds: Vec<u32> = (0..400u32)
            .map(|index| kind(index.wrapping_mul(2654435761).wrapping_add(42)))
            .collect();
        let share =
            |range: std::ops::Range<u32>| kinds.iter().filter(|kind| range.contains(kind)).count();
        assert!((100..180).contains(&share(0..35)), "cast-offs");
        assert!((160..240).contains(&share(35..85)), "impacts");
        assert!((30..90).contains(&share(85..100)), "both");
    }

    #[test]
    fn older_stains_dry_darker_and_the_newest_keeps_its_colour() {
        assert_eq!(shade(0x9b2638ff, 1.0), 0x9b263800);
        assert_eq!(shade(0xffffffff, 0.5), 0x7f7f7f00);
        assert_eq!(tint(0x9b263800, 1.0), 0x9b2638ff);
    }

    #[test]
    fn a_fresh_stain_grows_in_and_is_hidden_while_it_waits() {
        let blood = stains(3);
        let settled = {
            let mut canvas = Canvas2D::new();
            draw_blood(&mut canvas, &blood, Some((2..3, f32::INFINITY)));
            canvas
        };
        let mut waiting = Canvas2D::new();
        draw_blood(&mut waiting, &blood, Some((2..3, -500.0)));
        let mut growing = Canvas2D::new();
        draw_blood(&mut growing, &blood, Some((2..3, 100.0)));
        assert!(waiting.len() < settled.len());
        assert_ne!(growing.bytes(), settled.bytes());
    }

    #[test]
    fn compact_layout_keeps_board_status_and_full_size_keys_separate() {
        assert_eq!(HEIGHT, 400.0);
        let board_bottom = BOARD_Y + 6.0 * TILE + 5.0 * TILE_GAP;
        assert!(board_bottom < STATUS_Y);
        assert!(STATUS_Y + STATUS_HEIGHT < KEYBOARD_Y);
        assert_eq!(KEY_HEIGHT, 30.0);
        assert!(KEYBOARD_Y + 2.0 * KEY_STEP + KEY_HEIGHT + 2.0 <= HEIGHT - 7.0);
        assert!(include_str!("../../cartridge/templates/card.liquid")
            .contains("<lc-canvas width=\"320\" height=\"400\""));
        assert_eq!(
            ellipsize("You have a very long message", 12),
            "You have a …"
        );
        assert_eq!(ellipsize("You win!", 12), "You win!");
    }

    // Every frame of an effect redraws the whole wall, so a full wall must stay cheap to encode.
    #[test]
    fn complete_scenes_fit_the_frame_budget() {
        for count in [0, 1, 8, 64] {
            let mut game = started_game();
            game.state.guesses.push(crane());
            game.current = "RI".into();
            game.state.blood = stains(count);
            let mut canvas = Canvas2D::new();
            draw_scene(&mut canvas, &game);
            assert!(
                canvas.len() < 600_000,
                "scene with {count} stains is {} bytes",
                canvas.len()
            );
            // Optional actual command buffers for inspecting the native drawing in a browser.
            if let Some(dir) = std::env::var_os("HURTLE_PREVIEW_DIR") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(format!("scene-{count}.bin")), canvas.bytes()).unwrap();
            }
        }
    }

    #[test]
    fn keyboard_hit_targets_match_every_drawn_key_and_reject_gutters() {
        for row in 0..3 {
            let y = KEYBOARD_Y + row as f32 * KEY_STEP;
            for (label, x, width) in keyboard_row(row) {
                assert_eq!(keyboard_key_at(x + width / 2.0, y + 15.0).unwrap().0, label);
            }
            assert!(keyboard_key_at(160.0, y + 33.0).is_none());
        }
        assert!(keyboard_key_at(160.0, STATUS_Y + STATUS_HEIGHT / 2.0).is_none());
    }

    #[test]
    fn a_letter_is_not_invalid_if_any_guess_proved_it_present() {
        let guesses = [
            Guess {
                word: "crane".to_string(),
                marks: vec![0, 0, 0, 0, 0],
            },
            Guess {
                word: "civic".to_string(),
                marks: vec![1, 0, 0, 0, 0],
            },
        ];

        assert!(!is_proven_invalid('C', &guesses));
        assert!(is_proven_invalid('R', &guesses));
        assert!(!is_proven_invalid('Z', &guesses));
    }

    #[test]
    fn queued_effects_start_on_the_next_frame_not_the_stale_clock() {
        let mut game = started_game();
        tick(&mut game, 1_000.0);
        add_letter(&mut game, 'C');
        assert_eq!(game.fx.pops[0], Anim::Queued(0.0));
        // Ten idle seconds later the key arrives; the pop must still play in full.
        tick(&mut game, 11_000.0);
        assert_eq!(game.fx.pops[0], Anim::Running(11_000.0));
        assert!(game.fx.active(11_050.0));
        assert!(!game.fx.active(11_000.0 + POP_MS as f64));
    }

    #[test]
    fn a_new_guess_flips_in_and_holds_its_colours_and_input_until_turned() {
        let mut game = started_game();
        let mut state = game.state.clone();
        state.guesses.push(crane());
        update_state(&mut game, state.clone(), true);
        tick(&mut game, 0.0);
        assert_eq!(revealing(&game), Some(0));
        assert!(!accepting(&game));
        add_letter(&mut game, 'S');
        assert!(game.current.is_empty());

        // The keyboard shows nothing for the revealing row yet.
        let mut early = Canvas2D::new();
        draw_keyboard(&mut early, &game);
        // The render push of the same state does not restart the reveal.
        tick(&mut game, 500.0);
        update_state(&mut game, state, false);
        assert_eq!(game.fx.reveal, Anim::Running(0.0));

        tick(&mut game, REVEAL_MS as f64);
        assert_eq!(revealing(&game), None);
        assert!(accepting(&game));
        let mut late = Canvas2D::new();
        draw_keyboard(&mut late, &game);
        assert_ne!(early.bytes(), late.bytes());
    }

    #[test]
    fn a_loss_waits_for_the_reveal_then_shakes_and_splats() {
        let mut game = started_game();
        game.state.guesses = (0..5).map(|_| crane()).collect();
        tick(&mut game, 0.0);
        let mut state = game.state.clone();
        state.guesses.push(crane());
        state.complete = true;
        state.blood = stains(1);
        update_state(&mut game, state, true);
        assert_eq!(game.fx.loss, Anim::Queued(REVEAL_MS as f64));
        assert_eq!(game.fx.splat, Anim::Queued(REVEAL_MS as f64));
        assert_eq!(game.fx.fresh, 0..1);
        tick(&mut game, 16.0);
        assert!(game.fx.loss.elapsed(16.0) < 0.0);
        assert!(game
            .fx
            .active(16.0 + REVEAL_MS as f64 + LOSS_MS as f64 / 2.0));
    }

    #[test]
    fn a_win_bounces_after_the_reveal_and_others_deaths_splat_at_once() {
        let mut game = started_game();
        let mut state = game.state.clone();
        state.guesses.push(Guess {
            word: "blade".into(),
            marks: vec![2; 5],
        });
        state.won = true;
        state.complete = true;
        update_state(&mut game, state.clone(), true);
        assert_eq!(game.fx.win, Anim::Queued(REVEAL_MS as f64));
        assert_eq!(game.fx.loss, Anim::Idle);

        tick(&mut game, 0.0);
        tick(&mut game, 5_000.0);
        state.blood = stains(2);
        update_state(&mut game, state, false);
        assert_eq!(game.fx.splat, Anim::Queued(0.0));
        assert_eq!(game.fx.fresh, 0..2);
    }

    #[test]
    fn a_refused_guess_comes_back_to_the_board_and_shakes() {
        let mut game = started_game();
        game.current = "CRA".into();
        assert!(!ready_to_send(&mut game));
        assert_eq!(game.notice.as_deref(), Some("Not enough letters."));
        assert_eq!(game.fx.shake, Anim::Queued(0.0));
        assert_eq!(game.current, "CRA");

        game.fx.shake = Anim::Idle;
        game.sent = Some("CRANE".into());
        game.current.clear();
        game.pending = true;
        refuse(&mut game, Some("you already tried that word".into()));
        assert!(!game.pending);
        assert_eq!(game.current, "CRANE");
        assert_eq!(game.notice.as_deref(), Some("You already tried that word."));
        refuse(&mut game, Some("that's not in the word list".into()));
        assert_eq!(game.notice.as_deref(), Some("That's not in the word list."));
        assert_eq!(sentence("Done!"), "Done!");
        assert_eq!(sentence("  "), "");
        assert_eq!(game.fx.shake, Anim::Queued(0.0));
    }

    #[test]
    fn reduced_motion_skips_flips_shakes_bounces_and_growing_stains() {
        let mut game = started_game();
        game.still = true;
        add_letter(&mut game, 'C');
        assert_eq!(game.fx.pops[0], Anim::Idle);
        assert!(!ready_to_send(&mut game));
        assert_eq!(game.fx.shake, Anim::Idle);
        let mut state = game.state.clone();
        state.guesses = (0..6).map(|_| crane()).collect();
        state.complete = true;
        state.blood = stains(1);
        update_state(&mut game, state, true);
        assert_eq!(game.fx.reveal, Anim::Idle);
        assert_eq!(game.fx.splat, Anim::Idle);
        // The loss still fades the room's edge, which is a change of colour, not motion.
        assert_eq!(game.fx.loss, Anim::Queued(0.0));
    }

    #[test]
    fn a_suspension_blocks_input_but_lets_the_loss_play_out() {
        let mut game = started_game();
        game.state.guesses = (0..5).map(|_| crane()).collect();
        tick(&mut game, 0.0);
        let mut lost = game.state.clone();
        lost.guesses.push(crane());
        lost.complete = true;
        lost.penalty_applied = true;
        lost.blood = stains(1);
        update_state(&mut game, lost.clone(), true);
        tick(&mut game, 16.0);
        // The suspension's render push arrives mid-reveal and must not cut the death short.
        lost.suspended = true;
        update_state(&mut game, lost, false);
        assert_eq!(game.fx.reveal, Anim::Running(16.0));
        assert_eq!(game.fx.splat, Anim::Running(16.0 + REVEAL_MS as f64));
        assert!(game.fx.active(16.0 + REVEAL_MS as f64 + 100.0));

        let mut fresh = started_game();
        fresh.state.suspended = true;
        add_letter(&mut fresh, 'C');
        assert!(fresh.current.is_empty());
        assert!(!ready_to_send(&mut fresh));
        assert_eq!(selectable_letter_at(&fresh, 34.0, KEYBOARD_Y + 15.0), None);
    }

    fn lost_state(fresh_loss: bool) -> GameState {
        let mut state = started_game().state;
        state.guesses = (0..6).map(|_| crane()).collect();
        state.complete = true;
        state.penalty_applied = true;
        state.suspended = true;
        state.fresh_loss = fresh_loss;
        state.own_stain = Some(42);
        state.blood = stains(3);
        state.blood.stains[1].seed = 42;
        state
    }

    #[test]
    fn a_canvas_started_mid_death_replays_it_and_only_the_players_stain_lands() {
        let mut game = Game::default();
        game.state = lost_state(true);
        replay_unseen_loss(&mut game);
        assert_eq!(game.fx.reveal, Anim::Queued(0.0));
        assert_eq!(game.fx.reveal_row, 5);
        assert_eq!(game.fx.loss, Anim::Queued(REVEAL_MS as f64));
        assert_eq!(game.fx.splat, Anim::Queued(REVEAL_MS as f64));
        assert_eq!(game.fx.fresh, 1..2);
        assert!(game.acknowledge);

        // Report the death shown only once the stain has landed and run.
        tick(&mut game, 0.0);
        let done = (REVEAL_MS + SPLAT_MS) as f64;
        assert!(game.fx.splat.active(done - 1.0, SPLAT_MS));
        assert!(!game.fx.splat.active(done, SPLAT_MS) && !game.fx.loss.active(done, LOSS_MS));
    }

    #[test]
    fn a_death_already_shown_or_a_win_is_drawn_settled() {
        let mut game = Game::default();
        game.state = lost_state(false);
        replay_unseen_loss(&mut game);
        assert_eq!(game.fx.loss, Anim::Idle);
        assert!(!game.acknowledge);

        let mut game = Game::default();
        game.state = lost_state(true);
        game.state.won = true;
        replay_unseen_loss(&mut game);
        assert_eq!(game.fx.reveal, Anim::Idle);

        let mut game = Game::default();
        game.still = true;
        game.state = lost_state(true);
        replay_unseen_loss(&mut game);
        assert_eq!(game.fx.reveal, Anim::Idle);
        assert_eq!(game.fx.splat, Anim::Idle);
        assert_eq!(game.fx.loss, Anim::Queued(0.0));
        assert!(game.acknowledge);
    }

    #[test]
    fn a_running_canvas_that_sees_the_loss_also_reports_it() {
        let mut game = started_game();
        game.state.guesses = (0..5).map(|_| crane()).collect();
        let mut state = lost_state(true);
        state.suspended = false;
        update_state(&mut game, state, true);
        assert!(game.acknowledge);
    }

    #[test]
    fn only_letter_keys_take_a_letters_hover_and_colour() {
        assert_eq!(key_letter("E"), Some('E'));
        assert_eq!(key_letter("ENTER"), None);
        assert_eq!(key_letter("DEL"), None);

        // Hovering or colouring E and D must leave ENTER and DEL as they were.
        let draw = |hover: Option<char>, guesses: Vec<Guess>| {
            let mut game = started_game();
            game.hovered_key = hover;
            game.state.guesses = guesses;
            let mut canvas = Canvas2D::new();
            draw_keyboard(&mut canvas, &game);
            canvas
        };
        let plain = draw(None, vec![]);
        let ed = vec![Guess {
            word: "ended".into(),
            marks: vec![0; 5],
        }];
        for (hover, guesses) in [(Some('E'), vec![]), (Some('D'), vec![]), (None, ed)] {
            let changed = draw(hover, guesses);
            let enter_and_del = |canvas: &Canvas2D| {
                let bytes = canvas.bytes();
                let key = |label: &str| {
                    let needle =
                        [&(label.len() as u32).to_le_bytes()[..], label.as_bytes()].concat();
                    let at = bytes
                        .windows(needle.len())
                        .position(|w| w == needle)
                        .unwrap();
                    // The key's fill and outline come shortly before its label.
                    bytes[at.saturating_sub(120)..at].to_vec()
                };
                (key("ENTER"), key("DEL"))
            };
            assert_eq!(enter_and_del(&changed), enter_and_del(&plain));
        }
    }

    #[test]
    fn a_new_word_clears_every_effect() {
        let mut game = started_game();
        game.fx.shake = Anim::Running(5.0);
        let mut state = game.state.clone();
        state.round = Some(13);
        update_state(&mut game, state, false);
        assert_eq!(game.fx.shake, Anim::Idle);
    }

    #[test]
    fn five_letter_boards_are_unchanged_and_twelve_fit_inside_the_frame() {
        let five = board_for(5);
        assert_eq!(
            (five.tile, five.gap, five.width, five.left),
            (32.0, 4.0, 176.0, 72.0)
        );
        assert_eq!(reveal_duration(5), REVEAL_MS);

        let twelve = board_for(12);
        assert!(twelve.width <= BOARD_MAX_WIDTH + 0.01);
        assert!(twelve.tile > 17.0);
        // Row numbers and the result marks stay inside the brass frame.
        assert!(twelve.left - 16.0 - 6.0 > 7.0);
        assert!(twelve.left + twelve.width + 16.0 + 6.0 < WIDTH - 7.0);
        assert!(reveal_duration(12) < 1_400.0);
    }

    #[test]
    fn a_twelve_letter_game_takes_twelve_letters_and_draws_twelve_tiles() {
        let mut game = started_game();
        game.state.length = Some(12);
        for letter in "DECAPITATIONS".chars() {
            add_letter(&mut game, letter);
        }
        assert_eq!(game.current, "DECAPITATION");
        assert!(ready_to_send(&mut game));
        game.current.pop();
        assert!(!ready_to_send(&mut game));

        // An empty board with no game falls back to the twelve-letter default.
        assert_eq!(letters(&GameState::default()), 12);
        let draw = |length: usize| {
            let mut game = started_game();
            game.state.length = Some(length);
            let mut canvas = Canvas2D::new();
            draw_board(&mut canvas, &game);
            canvas.len()
        };
        assert!(draw(12) > draw(5));
    }

    #[test]
    fn a_twelve_letter_reveal_waits_its_own_length() {
        let mut game = started_game();
        game.state.length = Some(12);
        let mut state = game.state.clone();
        state.guesses.push(Guess {
            word: "deceptionist".into(),
            marks: vec![2, 2, 2, 0, 2, 1, 1, 1, 1, 2, 0, 1],
        });
        update_state(&mut game, state, true);
        tick(&mut game, 0.0);
        assert_eq!(game.fx.reveal_ms, reveal_duration(12));
        assert!(!accepting(&game));
        tick(&mut game, reveal_duration(12) as f64);
        assert!(accepting(&game));
    }
}
