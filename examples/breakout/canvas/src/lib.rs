//! Breakout, with bite: five hand-built levels, multiball, an expanding paddle, a laser, slow-motion, falling
//! power-ups, particle effects and a neon arcade look. The Canvas Guest owns the whole game; the Server Guest owns
//! score validation and the daily and all-time leaderboards.
//!
//! Everything here draws through `chittr-cartridge`'s command-buffer encoder (`Canvas2D`), so one `submit` carries a
//! frame. Input is arrow keys / A-D / pointer, Space or a tap launches and fires, and a tap after a loss restarts.

#[allow(dead_code, non_camel_case_types)]
mod pdk;

use std::cell::RefCell;
use std::f32::consts::TAU;

use chittr_cartridge::canvas::{Canvas2D, Composite, FillRule, LineCap, TextAlign, TextBaseline};
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------------------------------------------

/// The playfield's top edge, under the HUD.
const TOP: f32 = 40.0;
/// The fixed logical playfield. Everything is drawn in these units and scaled onto the backing store, so windowed and
/// fullscreen keep the same layout and aspect ratio.
const LOGICAL_W: f32 = 368.0;
const LOGICAL_H: f32 = 540.0;
/// The gap between the playfield and the canvas edge.
const MARGIN: f32 = 8.0;
/// The board is this many square cells across. The Server Guest agrees.
const COLS: usize = 32;
/// A level may hold at most this many rows. The Server Guest agrees.
const MAX_ROWS: usize = 32;
/// A level may use at most this many distinct brick styles (colour + strength). The Server Guest agrees.
const MAX_STYLES: usize = 16;
/// The most levels the poster can keep. The Server Guest agrees.
const MAX_LEVELS: usize = 6;
/// A level name is at most this many characters. The Server Guest agrees.
const MAX_NAME: usize = 24;
const GAP: f32 = 1.0;
/// The characters that index a level's styles; `0` is the first style, `.` is empty.
const INDEX_CHARS: &[u8] = b"0123456789ABCDEF";
/// The most balls a multiball chain can put on screen at once.
const MAX_BALLS: usize = 8;
const MAX_PARTICLES: usize = 360;
const MAX_LIVES: u8 = 9;
const PADDLE_Y_FROM_BOTTOM: f32 = 22.0;

const BG_TOP: u32 = 0x070b16ff;
const BG_BOTTOM: u32 = 0x151b30ff;
const WHITE: u32 = 0xffffffff;
const TEXT: u32 = 0xeaf0ffff;
const MUTED: u32 = 0x8fa2c4ff;
const PADDLE_BASE: u32 = 0x4ad2dfff;
const PADDLE_WIDE: u32 = 0x63e08cff;
const BALL_CORE: u32 = 0xfff4c2ff;
const SOLID_BRICK: u32 = 0x6b7793ff;

/// Row colours; a brick's tier picks one, and its health shades it.
const PALETTE: [u32; 7] = [
    0xff5d73ff, // red
    0xff9f45ff, // orange
    0xffd93dff, // yellow
    0x59e08aff, // green
    0x4fd6e6ff, // cyan
    0x6c8cffff, // indigo
    0xb46cffff, // violet
];

/// The characters that index a level's styles; `0` is the first style, `.` is empty.
fn index_char(index: usize) -> char {
    INDEX_CHARS.get(index).copied().unwrap_or(b'?') as char
}

fn index_of(cell: char) -> Option<usize> {
    INDEX_CHARS
        .iter()
        .position(|candidate| *candidate as char == cell)
}

/// The colours the built-in levels are written with, keyed by letter.
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

fn style_rgba(style: &Style) -> u32 {
    (style.color << 8) | 0xff
}

fn repeat(cell: char) -> String {
    cell.to_string().repeat(COLS)
}

/// The levels a fresh Cartridge starts with. The Server Guest seeds the same set.
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

// ---------------------------------------------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------------------------------------------

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffff_ff00) | alpha as u32
}

fn shade(color: u32, factor: f32) -> u32 {
    let channel =
        |shift: u32| ((((color >> shift) & 0xff) as f32 * factor).clamp(0.0, 255.0)) as u32;
    (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | (color & 0xff)
}

// ---------------------------------------------------------------------------------------------------------------
// Game state
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Playing,
    Clearing(f32),
    Over,
    Won,
}

#[derive(Clone)]
struct Ball {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: f32,
    stuck: bool,
    trail: Vec<(f32, f32)>,
}

struct Brick {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hp: u8,
    max_hp: u8,
    color: u32,
    solid: bool,
    alive: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum PowerKind {
    MultiBall,
    Expand,
    Laser,
    Slow,
    Life,
}

impl PowerKind {
    fn letter(self) -> &'static str {
        match self {
            PowerKind::MultiBall => "M",
            PowerKind::Expand => "E",
            PowerKind::Laser => "L",
            PowerKind::Slow => "S",
            PowerKind::Life => "+",
        }
    }

    fn color(self) -> u32 {
        match self {
            PowerKind::MultiBall => PALETTE[5],
            PowerKind::Expand => PALETTE[3],
            PowerKind::Laser => PALETTE[0],
            PowerKind::Slow => PALETTE[4],
            PowerKind::Life => PALETTE[2],
        }
    }
}

struct PowerUp {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kind: PowerKind,
    spin: f32,
}

struct Bullet {
    x: f32,
    y: f32,
}

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max_life: f32,
    size: f32,
    color: u32,
    gravity: f32,
    spark: bool,
}

struct Star {
    x: f32,
    y: f32,
    size: f32,
    phase: f32,
    speed: f32,
}

/// One brick style: its health, whether it is indestructible, and its colour as `0xRRGGBB`.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
struct Style {
    hp: u8,
    solid: bool,
    color: u32,
}

/// One level: a name, the styles it uses, and up to [`MAX_ROWS`] rows of [`COLS`] cells. A cell is `.` (empty) or a
/// single character indexing [`Level::styles`]. Levels arrive from the Server Guest (stored in its KV store) and can be
/// edited here.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct Level {
    name: String,
    styles: Vec<Style>,
    rows: Vec<String>,
}

/// Which screen the guest is showing.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Game,
    Editor,
}

struct Game {
    width: f32,
    height: f32,
    view_w: f32,
    view_h: f32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    fullscreen: bool,
    reduced: bool,
    phase: Phase,
    level: usize,
    score: u64,
    lives: u8,
    combo: u32,
    combo_timer: f32,
    time: f32,
    paddle_x: f32,
    paddle_v: f32,
    paddle_w: f32,
    paddle_target_w: f32,
    left_pressed: bool,
    right_pressed: bool,
    fire_held: bool,
    fire_cooldown: f32,
    expand_timer: f32,
    shots: u32,
    slow_timer: f32,
    /// The ball waits on the paddle until the player launches it.
    awaiting_launch: bool,
    /// False until the first tap or click on the canvas; nothing moves before then.
    engaged: bool,
    banner: f32,
    toast: f32,
    shake: f32,
    flash: f32,
    bricks: Vec<Brick>,
    balls: Vec<Ball>,
    powerups: Vec<PowerUp>,
    bullets: Vec<Bullet>,
    particles: Vec<Particle>,
    stars: Vec<Star>,
    rng: u32,
    seq: i32,
    score_sent: bool,
    notice: Option<String>,
    // Level data and the poster's editor.
    levels: Vec<Level>,
    admin: bool,
    mode: Mode,
    brush: Style,
    erasing: bool,
    renaming: bool,
    name_buffer: String,
    cursor: (i32, i32),
    edit_index: usize,
    status: Option<String>,
    status_timer: f32,
    playtest: bool,
    levels_override: Option<Vec<Level>>,
    return_to_editor: f32,
    pointer_down: bool,
    flash_color: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            width: LOGICAL_W,
            height: LOGICAL_H,
            view_w: LOGICAL_W,
            view_h: LOGICAL_H,
            scale: 1.0,
            off_x: 0.0,
            off_y: 0.0,
            fullscreen: false,
            reduced: false,
            phase: Phase::Playing,
            level: 0,
            score: 0,
            lives: 3,
            combo: 0,
            combo_timer: 0.0,
            time: 0.0,
            paddle_x: 210.0,
            paddle_v: 0.0,
            paddle_w: 80.0,
            paddle_target_w: 80.0,
            left_pressed: false,
            right_pressed: false,
            fire_held: false,
            fire_cooldown: 0.0,
            expand_timer: 0.0,
            shots: 0,
            slow_timer: 0.0,
            awaiting_launch: false,
            engaged: false,
            banner: 0.0,
            toast: 0.0,
            shake: 0.0,
            flash: 0.0,
            bricks: Vec::new(),
            balls: Vec::new(),
            powerups: Vec::new(),
            bullets: Vec::new(),
            particles: Vec::new(),
            stars: Vec::new(),
            rng: 0x9e37_79b9,
            seq: 0,
            score_sent: false,
            notice: None,
            levels: default_levels(),
            admin: false,
            mode: Mode::Game,
            brush: Style {
                hp: 1,
                solid: false,
                color: 0xffd93d,
            },
            erasing: false,
            renaming: false,
            name_buffer: String::new(),
            cursor: (0, 0),
            edit_index: 0,
            status: None,
            status_timer: 0.0,
            playtest: false,
            levels_override: None,
            return_to_editor: 0.0,
            pointer_down: false,
            flash_color: PALETTE[0],
        }
    }
}

impl Game {
    fn reset(&mut self) {
        let width = self.width;
        let height = self.height;
        let view_w = self.view_w;
        let view_h = self.view_h;
        let scale = self.scale;
        let off_x = self.off_x;
        let off_y = self.off_y;
        let fullscreen = self.fullscreen;
        let reduced = self.reduced;
        let stars = std::mem::take(&mut self.stars);
        let seq = self.seq;
        let levels = std::mem::take(&mut self.levels);
        let admin = self.admin;
        *self = Self {
            width,
            height,
            view_w,
            view_h,
            scale,
            off_x,
            off_y,
            fullscreen,
            reduced,
            stars,
            seq,
            levels,
            admin,
            ..Self::default()
        };
        self.rng = 0x9e37_79b9 ^ (width as u32).rotate_left(7) ^ (height as u32).rotate_left(13);
        self.seed_stars();
        self.load_level(0);
    }

    /// Recomputes the letterboxed viewport that maps the fixed logical playfield onto the canvas's backing store.
    fn refresh_view(&mut self) {
        let view_w = self.view_w.max(1.0);
        let view_h = self.view_h.max(1.0);
        self.scale = (view_w / self.width).min(view_h / self.height);
        self.off_x = (view_w - self.width * self.scale) / 2.0;
        self.off_y = (view_h - self.height * self.scale) / 2.0;
    }

    /// A pointer's CSS position, mapped into the logical playfield.
    fn to_logical(&self, event: &CanvasEventInput) -> (f32, f32) {
        let css_w = event.width.unwrap_or(self.view_w as f64) as f32;
        let css_h = event.height.unwrap_or(self.view_h as f64) as f32;
        let per_css_x = if css_w > 0.0 {
            self.view_w / css_w
        } else {
            1.0
        };
        let per_css_y = if css_h > 0.0 {
            self.view_h / css_h
        } else {
            1.0
        };
        let x = event.x.unwrap_or(0.0) as f32 * per_css_x;
        let y = event.y.unwrap_or(0.0) as f32 * per_css_y;
        (
            (x - self.off_x) / self.scale.max(0.0001),
            (y - self.off_y) / self.scale.max(0.0001),
        )
    }

    /// The levels the game plays: the poster's edited set, or (during a playtest) just the level being tested.
    fn active_levels(&self) -> &[Level] {
        self.levels_override.as_deref().unwrap_or(&self.levels)
    }

    /// Applies the Server Guest's `canvas` state: the admin flag and the level set.
    fn apply_state(&mut self, state: Option<&Map<String, Value>>) {
        let Some(state) = state else {
            return;
        };
        if let Some(admin) = state.get("admin").and_then(Value::as_bool) {
            self.admin = admin;
        }
        if let Some(value) = state.get("levels") {
            if let Ok(parsed) = serde_json::from_value::<Vec<Level>>(value.clone()) {
                let cleaned: Vec<Level> = parsed.into_iter().filter(level_is_playable).collect();
                // Never clobber unsaved edits in the editor.
                if !cleaned.is_empty() && !matches!(self.mode, Mode::Editor) {
                    self.levels = cleaned;
                    if self.level >= self.levels.len() {
                        self.load_level(0);
                    }
                }
            }
        }
    }

    fn paddle_y(&self) -> f32 {
        self.height - PADDLE_Y_FROM_BOTTOM
    }

    fn base_paddle_width(&self) -> f32 {
        (self.width * 0.18).clamp(58.0, 92.0)
    }

    fn ball_speed(&self) -> f32 {
        (196.0 + 15.0 * self.level as f32 + (self.time * 4.0).min(70.0)).min(410.0)
    }

    fn speed_scale(&self) -> f32 {
        if self.slow_timer > 0.0 {
            0.62
        } else {
            1.0
        }
    }

    fn combo_multiplier(&self) -> u64 {
        1 + (self.combo as u64 / 4).min(4)
    }

    fn seed_stars(&mut self) {
        self.stars.clear();
        let count = if self.reduced { 18 } else { 46 };
        for _ in 0..count {
            let x = self.rand() as f32 / u32::MAX as f32 * self.width;
            let y = self.rand() as f32 / u32::MAX as f32 * self.height;
            let size = 0.7 + self.rand() as f32 / u32::MAX as f32 * 1.8;
            let phase = self.rand() as f32 / u32::MAX as f32 * TAU;
            let speed = 0.6 + self.rand() as f32 / u32::MAX as f32 * 2.2;
            self.stars.push(Star {
                x,
                y,
                size,
                phase,
                speed,
            });
        }
    }

    fn rand(&mut self) -> u32 {
        // xorshift32; good enough for sparkle and drops, and no host call.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn unit(&mut self) -> f32 {
        self.rand() as f32 / u32::MAX as f32
    }

    fn load_level(&mut self, index: usize) {
        self.level = index;
        self.phase = Phase::Playing;
        self.time = 0.0;
        self.combo = 0;
        self.combo_timer = 0.0;
        self.expand_timer = 0.0;
        self.slow_timer = 0.0;
        self.fire_cooldown = 0.0;
        self.fire_held = false;
        self.bullets.clear();
        self.powerups.clear();
        self.bricks.clear();
        self.paddle_target_w = self.base_paddle_width();
        self.paddle_w = self.paddle_target_w;
        self.paddle_x = self.width / 2.0;

        let layout = self.active_levels().get(index).cloned().unwrap_or(Level {
            name: String::new(),
            styles: Vec::new(),
            rows: Vec::new(),
        });
        let cell = self.brick_w();
        let brick_top = TOP + 6.0;
        let step = cell + GAP;
        for (row, line) in layout.rows.iter().enumerate().take(MAX_ROWS) {
            for (col, ch) in line.chars().enumerate().take(COLS) {
                if ch == '.' {
                    continue;
                }
                let Some(style) = index_of(ch).and_then(|slot| layout.styles.get(slot)) else {
                    continue;
                };
                let hp = if style.solid { u8::MAX } else { style.hp };
                self.bricks.push(Brick {
                    x: MARGIN + col as f32 * step,
                    y: brick_top + row as f32 * step,
                    w: cell,
                    h: cell,
                    hp,
                    max_hp: hp,
                    color: style_rgba(style),
                    solid: style.solid,
                    alive: true,
                });
            }
        }

        self.balls = vec![self.fresh_ball()];
        self.awaiting_launch = true;
        self.banner = 2.4;
        self.shake = 0.0;
    }

    fn fresh_ball(&self) -> Ball {
        Ball {
            x: self.paddle_x,
            y: self.paddle_y() - 8.0,
            vx: 0.0,
            vy: 0.0,
            r: 5.0,
            stuck: true,
            trail: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Update
    // -----------------------------------------------------------------------------------------------------------

    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.shake = (self.shake - dt * 30.0).max(0.0);
        // A gentle, slow fade: no hard luminance flashes.
        self.flash = (self.flash - dt * 1.4).max(0.0);
        self.banner = (self.banner - dt).max(0.0);
        self.toast = (self.toast - dt).max(0.0);
        self.status_timer = (self.status_timer - dt).max(0.0);
        if self.status_timer == 0.0 && matches!(self.mode, Mode::Game) {
            self.status = None;
        }
        self.update_particles(dt);

        if matches!(self.mode, Mode::Editor) {
            return;
        }

        if self.return_to_editor > 0.0 {
            self.return_to_editor = (self.return_to_editor - dt).max(0.0);
            if self.return_to_editor == 0.0 {
                self.exit_to_editor();
                return;
            }
        }

        // Nothing moves until the player first taps or clicks the board.
        if !self.engaged {
            return;
        }

        match self.phase {
            Phase::Playing => self.update_playing(dt),
            Phase::Clearing(timer) => {
                let next = timer - dt;
                if next <= 0.0 {
                    self.advance_level();
                } else {
                    self.phase = Phase::Clearing(next);
                }
            }
            Phase::Over | Phase::Won => {}
        }
    }

    fn update_playing(&mut self, dt: f32) {
        self.update_paddle(dt);
        self.update_timers(dt);
        self.update_shooting(dt);
        self.update_bullets(dt);
        self.update_balls(dt);
        self.update_powerups(dt);

        if self.combo_timer > 0.0 {
            self.combo_timer -= dt;
            if self.combo_timer <= 0.0 {
                self.combo = 0;
            }
        }

        if self.balls.is_empty() {
            self.lose_life();
            return;
        }

        let remaining = self.bricks.iter().any(|brick| brick.alive && !brick.solid);
        if !remaining {
            self.clear_level();
        }
    }

    fn update_paddle(&mut self, dt: f32) {
        let speed = 420.0;
        let previous = self.paddle_x;
        if self.left_pressed {
            self.paddle_x -= speed * dt;
        }
        if self.right_pressed {
            self.paddle_x += speed * dt;
        }
        self.paddle_w += (self.paddle_target_w - self.paddle_w) * (dt * 9.0).min(1.0);
        let half = self.paddle_w / 2.0;
        self.paddle_x = self
            .paddle_x
            .clamp(MARGIN + half, self.width - MARGIN - half);
        self.paddle_v = if dt > 0.0 {
            (self.paddle_x - previous) / dt
        } else {
            0.0
        };
    }

    fn update_timers(&mut self, dt: f32) {
        let wide = self.expand_timer > 0.0;
        self.expand_timer = (self.expand_timer - dt).max(0.0);
        self.slow_timer = (self.slow_timer - dt).max(0.0);
        self.paddle_target_w = if wide {
            self.base_paddle_width() * 1.55
        } else {
            self.base_paddle_width()
        };
    }

    fn update_shooting(&mut self, dt: f32) {
        self.fire_cooldown = (self.fire_cooldown - dt).max(0.0);
        if self.shots > 0 && self.fire_held && self.fire_cooldown == 0.0 {
            self.fire();
        }
    }

    fn fire(&mut self) {
        if self.shots == 0 {
            return;
        }
        self.shots -= 1;
        self.fire_cooldown = 0.2;
        let half = self.paddle_w / 2.0;
        for side in [-1.0f32, 1.0] {
            self.bullets.push(Bullet {
                x: self.paddle_x + side * (half - 6.0),
                y: self.paddle_y() - 8.0,
            });
        }
        self.spawn_shake(1.6);
        self.burst(
            self.paddle_x,
            self.paddle_y() - 10.0,
            PALETTE[0],
            4,
            70.0,
            false,
        );
    }

    fn update_bullets(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.bullets.len() {
            self.bullets[i].y -= 560.0 * dt;
            let (bx, by) = (self.bullets[i].x, self.bullets[i].y);
            if by < TOP {
                self.bullets.swap_remove(i);
                continue;
            }
            let hit = self
                .bricks
                .iter()
                .position(|brick| brick.alive && point_in(bx, by, brick));
            match hit {
                Some(index) => {
                    self.hit_brick(index, bx, by);
                    self.bullets.swap_remove(i);
                }
                None => i += 1,
            }
        }
    }

    fn update_balls(&mut self, dt: f32) {
        let scale = self.speed_scale();
        let radius = 5.0;
        let paddle_y = self.paddle_y();
        let paddle_h = 9.0;
        let paddle_left = self.paddle_x - self.paddle_w / 2.0;
        let paddle_right = self.paddle_x + self.paddle_w / 2.0;

        if self.awaiting_launch {
            let (px, py) = (self.paddle_x, paddle_y - radius - 2.0);
            for ball in &mut self.balls {
                if ball.stuck {
                    ball.x = px;
                    ball.y = py;
                }
            }
            return;
        }

        let mut lost = Vec::new();
        for index in 0..self.balls.len() {
            if self.balls[index].stuck {
                continue;
            }
            let mut ball = self.balls[index].clone();
            let speed = ball.vx.hypot(ball.vy) * scale;
            let steps = ((speed * dt) / 3.0).ceil().clamp(1.0, 12.0) as usize;
            let step = dt / steps as f32;
            let mut gone = false;

            for _ in 0..steps {
                ball.x += ball.vx * scale * step;
                ball.y += ball.vy * scale * step;

                if ball.x - ball.r < MARGIN {
                    ball.x = MARGIN + ball.r;
                    ball.vx = ball.vx.abs();
                    self.burst(ball.x, ball.y, PALETTE[4], 3, 60.0, true);
                } else if ball.x + ball.r > self.width - MARGIN {
                    ball.x = self.width - MARGIN - ball.r;
                    ball.vx = -ball.vx.abs();
                    self.burst(ball.x, ball.y, PALETTE[4], 3, 60.0, true);
                }
                if ball.y - ball.r < TOP {
                    ball.y = TOP + ball.r;
                    ball.vy = ball.vy.abs();
                    self.burst(ball.x, ball.y, PALETTE[5], 3, 60.0, true);
                }

                if ball.vy > 0.0
                    && ball.y + ball.r >= paddle_y
                    && ball.y - ball.r <= paddle_y + paddle_h
                    && ball.x + ball.r >= paddle_left
                    && ball.x - ball.r <= paddle_right
                {
                    let offset =
                        ((ball.x - self.paddle_x) / (self.paddle_w / 2.0)).clamp(-1.0, 1.0);
                    let english = (self.paddle_v * 0.0009).clamp(-0.35, 0.35);
                    let angle = (offset + english).clamp(-1.15, 1.15);
                    let speed = self.ball_speed();
                    ball.vx = speed * angle.sin();
                    ball.vy = -speed * angle.cos();
                    ball.y = paddle_y - ball.r - 0.5;
                    self.combo = 0;
                    self.burst(ball.x, paddle_y, PADDLE_BASE, 5, 90.0, true);
                }

                if let Some((brick, axis, push)) = self.brick_collision(&ball) {
                    match axis {
                        Axis::X => {
                            ball.vx = -ball.vx;
                            ball.x += push;
                        }
                        Axis::Y => {
                            ball.vy = -ball.vy;
                            ball.y += push;
                        }
                    }
                    self.hit_brick(brick, ball.x, ball.y);
                }

                if ball.y - ball.r > self.height {
                    gone = true;
                    break;
                }
            }

            if gone {
                lost.push(index);
            } else {
                ball.trail.push((ball.x, ball.y));
                let keep = if self.reduced { 4 } else { 7 };
                if ball.trail.len() > keep {
                    ball.trail.remove(0);
                }
                self.balls[index] = ball;
            }
        }

        for index in lost.into_iter().rev() {
            let ball = self.balls.remove(index);
            self.burst(ball.x, ball.y, PALETTE[0], 14, 170.0, false);
            self.flash = 0.55;
            self.spawn_shake(7.0);
        }
    }

    fn brick_collision(&self, ball: &Ball) -> Option<(usize, Axis, f32)> {
        for (index, brick) in self.bricks.iter().enumerate() {
            if !brick.alive {
                continue;
            }
            let cx = brick.x + brick.w / 2.0;
            let cy = brick.y + brick.h / 2.0;
            let overlap_x = ball.r + brick.w / 2.0 - (ball.x - cx).abs();
            let overlap_y = ball.r + brick.h / 2.0 - (ball.y - cy).abs();
            if overlap_x > 0.0 && overlap_y > 0.0 {
                return if overlap_x < overlap_y {
                    Some((
                        index,
                        Axis::X,
                        if ball.x < cx { -overlap_x } else { overlap_x },
                    ))
                } else {
                    Some((
                        index,
                        Axis::Y,
                        if ball.y < cy { -overlap_y } else { overlap_y },
                    ))
                };
            }
        }
        None
    }

    fn hit_brick(&mut self, index: usize, x: f32, y: f32) {
        let (solid, color, max_hp) = {
            let brick = &self.bricks[index];
            (brick.solid, brick.color, brick.max_hp)
        };
        if solid {
            self.burst(x, y, SOLID_BRICK, 5, 110.0, true);
            self.spawn_shake(2.0);
            return;
        }

        let destroyed;
        {
            let brick = &mut self.bricks[index];
            brick.hp = brick.hp.saturating_sub(1);
            destroyed = brick.hp == 0;
            if destroyed {
                brick.alive = false;
            }
        }

        if destroyed {
            self.combo = (self.combo + 1).min(99);
            self.combo_timer = 1.4;
            self.score = self
                .score
                .saturating_add(10 * max_hp as u64 * self.combo_multiplier());
            self.burst(x, y, color, 16, 190.0, false);
            self.burst(x, y, WHITE, 4, 240.0, true);
            self.spawn_shake(4.5);
            self.maybe_drop_powerup(x, y);
        } else {
            self.combo_timer = self.combo_timer.max(0.8);
            self.score = self.score.saturating_add(5);
            self.burst(x, y, shade(color, 0.8), 7, 120.0, true);
            self.spawn_shake(2.2);
        }
    }

    fn maybe_drop_powerup(&mut self, x: f32, y: f32) {
        let chance = 0.16 + 0.015 * self.level as f32;
        if self.unit() > chance {
            return;
        }
        let roll = self.unit();
        let kind = if roll < 0.30 {
            PowerKind::MultiBall
        } else if roll < 0.55 {
            PowerKind::Expand
        } else if roll < 0.80 {
            PowerKind::Laser
        } else if roll < 0.93 {
            PowerKind::Slow
        } else {
            PowerKind::Life
        };
        let spin = self.unit() * TAU;
        self.powerups.push(PowerUp {
            x: x.clamp(MARGIN + 12.0, self.width - MARGIN - 12.0),
            y,
            w: 22.0,
            h: 12.0,
            kind,
            spin,
        });
    }

    fn update_powerups(&mut self, dt: f32) {
        let paddle_y = self.paddle_y();
        let half = self.paddle_w / 2.0;
        let mut i = 0;
        while i < self.powerups.len() {
            self.powerups[i].y += 92.0 * dt;
            self.powerups[i].spin += dt * 3.0;
            let caught = self.powerups[i].y + self.powerups[i].h / 2.0 >= paddle_y
                && self.powerups[i].y - self.powerups[i].h / 2.0 <= paddle_y + 9.0
                && self.powerups[i].x + self.powerups[i].w / 2.0 >= self.paddle_x - half
                && self.powerups[i].x - self.powerups[i].w / 2.0 <= self.paddle_x + half;
            if caught {
                let powerup = self.powerups.swap_remove(i);
                self.apply_powerup(powerup.kind);
                continue;
            }
            if self.powerups[i].y - self.powerups[i].h > self.height {
                self.powerups.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn apply_powerup(&mut self, kind: PowerKind) {
        let color = kind.color();
        self.burst(
            self.paddle_x,
            self.paddle_y() - 6.0,
            color,
            18,
            190.0,
            false,
        );
        self.flash = 0.35;
        self.flash_color = color;
        match kind {
            PowerKind::MultiBall => {
                self.launch_stuck();
                let snapshot: Vec<Ball> = self.balls.clone();
                for ball in snapshot {
                    if self.balls.len() >= MAX_BALLS {
                        break;
                    }
                    for turn in [0.5f32, -0.5] {
                        if self.balls.len() >= MAX_BALLS {
                            break;
                        }
                        let (vx, vy) = rotate(ball.vx, ball.vy, turn);
                        let mut extra = ball.clone();
                        extra.stuck = false;
                        extra.vx = vx;
                        extra.vy = vy;
                        extra.trail.clear();
                        self.balls.push(extra);
                    }
                }
            }
            PowerKind::Expand => self.expand_timer = 16.0,
            PowerKind::Laser => self.shots = (self.shots + 15).min(99),
            PowerKind::Slow => self.slow_timer = 9.0,
            PowerKind::Life => self.lives = (self.lives + 2).min(MAX_LIVES),
        }
        self.notice = Some(format!("{} up!", powerup_name(kind)));
        self.toast = 1.6;
    }

    fn launch_stuck(&mut self) {
        let speed = self.ball_speed();
        let half = (self.paddle_w / 2.0).max(1.0);
        let jitter = (self.unit() - 0.5) * 0.12;
        for ball in &mut self.balls {
            if !ball.stuck {
                continue;
            }
            let offset = ((ball.x - self.paddle_x) / half).clamp(-1.0, 1.0);
            let angle = (offset * 0.9 + jitter).clamp(-1.1, 1.1);
            ball.vx = speed * angle.sin();
            ball.vy = -speed * angle.cos();
            ball.stuck = false;
        }
    }

    fn lose_life(&mut self) {
        self.lives = self.lives.saturating_sub(1);
        self.combo = 0;
        self.expand_timer = 0.0;
        self.shots = 0;
        self.slow_timer = 0.0;
        self.powerups.clear();
        self.bullets.clear();
        self.flash = 0.8;
        self.flash_color = PALETTE[0];
        self.spawn_shake(10.0);
        if self.lives == 0 {
            self.finish(false);
            return;
        }
        self.notice = None;
        self.balls = vec![self.fresh_ball()];
        self.awaiting_launch = true;
    }

    fn clear_level(&mut self) {
        self.score = self.score.saturating_add(250 * (self.level as u64 + 1));
        self.phase = Phase::Clearing(2.3);
        self.banner = 0.0;
        self.spawn_confetti();
        self.spawn_shake(8.0);
        self.notice = None;
    }

    fn advance_level(&mut self) {
        if self.playtest {
            // A playtest is a single level: show the panel, then return to the editor.
            self.finish(true);
        } else if self.level + 1 >= self.active_levels().len() {
            self.finish(true);
        } else {
            self.load_level(self.level + 1);
        }
    }

    fn finish(&mut self, won: bool) {
        self.phase = if won { Phase::Won } else { Phase::Over };
        self.bullets.clear();
        self.powerups.clear();
        self.notice = None;
        if self.playtest {
            self.return_to_editor = 1.6;
        } else {
            self.submit_score();
        }
    }

    fn submit_score(&mut self) {
        if self.score == 0 || self.score_sent {
            return;
        }
        self.score_sent = true;
        self.seq = self.seq.wrapping_add(1);
        let message = match json!({ "type": "score", "score": self.score }) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        if let Err(error) = send_message(message, self.seq) {
            self.notice = Some(format!("Score not saved: {}", error.message));
        }
    }

    fn restart(&mut self) {
        let width = self.width;
        let height = self.height;
        let view_w = self.view_w;
        let view_h = self.view_h;
        let scale = self.scale;
        let off_x = self.off_x;
        let off_y = self.off_y;
        let fullscreen = self.fullscreen;
        let reduced = self.reduced;
        let stars = std::mem::take(&mut self.stars);
        let seq = self.seq;
        let levels = std::mem::take(&mut self.levels);
        let admin = self.admin;
        let engaged = self.engaged;
        *self = Self {
            width,
            height,
            view_w,
            view_h,
            scale,
            off_x,
            off_y,
            fullscreen,
            reduced,
            stars,
            seq,
            levels,
            admin,
            engaged,
            ..Self::default()
        };
        self.seed_stars();
        self.load_level(0);
    }

    // -----------------------------------------------------------------------------------------------------------
    // Level editor
    // -----------------------------------------------------------------------------------------------------------

    /// Opens the editor, starting on the level that is being played.
    fn enter_editor(&mut self) {
        self.mode = Mode::Editor;
        self.edit_index = self.edit_index.min(self.levels.len().saturating_sub(1));
        self.cursor = (0, 0);
        self.levels_override = None;
        self.playtest = false;
        self.return_to_editor = 0.0;
        self.left_pressed = false;
        self.right_pressed = false;
        self.fire_held = false;
        self.pointer_down = false;
        self.renaming = false;
        self.status = Some("Pick a cell, then tap the grid to paint".to_string());
        self.status_timer = 4.0;
    }

    /// Leaves the editor and starts a fresh run on the edited levels.
    fn exit_to_editor(&mut self) {
        // Coming back from a playtest keeps the editor open.
        self.mode = Mode::Editor;
        self.levels_override = None;
        self.playtest = false;
        self.pointer_down = false;
        self.renaming = false;
        self.status = Some("Back in the editor".to_string());
        self.status_timer = 2.5;
    }

    /// Leaves the editor and plays the edited levels from the top.
    fn leave_editor_to_play(&mut self) {
        self.mode = Mode::Game;
        self.levels_override = None;
        self.playtest = false;
        self.pointer_down = false;
        self.status = None;
        self.restart_play();
    }

    fn restart_play(&mut self) {
        self.phase = Phase::Playing;
        self.score = 0;
        self.lives = 3;
        self.combo = 0;
        self.score_sent = false;
        self.notice = None;
        self.load_level(0);
    }

    /// Plays just the level being edited, so the poster can try it out.
    fn playtest(&mut self) {
        let Some(level) = self.levels.get(self.edit_index).cloned() else {
            return;
        };
        if !level_is_playable(&level) {
            self.set_status("Add at least one breakable brick before playing".to_string());
            return;
        }
        self.levels_override = Some(vec![level]);
        self.playtest = true;
        self.mode = Mode::Game;
        self.renaming = false;
        self.restart_play();
        self.status = Some("Playtest — ends back in the editor".to_string());
        self.status_timer = 3.0;
    }

    fn set_status(&mut self, text: String) {
        self.status = Some(text);
        self.status_timer = 3.5;
    }

    /// Adds a fresh, empty level after the current one and selects it.
    fn new_level(&mut self) {
        if self.levels.len() >= MAX_LEVELS {
            self.set_status(format!("At most {MAX_LEVELS} levels"));
            return;
        }
        let number = self.levels.len() + 1;
        self.levels.push(Level {
            name: format!("LEVEL {number}"),
            styles: Vec::new(),
            rows: vec![".".repeat(COLS); 4],
        });
        self.edit_index = self.levels.len() - 1;
        self.set_status("New level added".to_string());
    }

    fn select_level(&mut self, index: usize) {
        if self.levels.is_empty() {
            return;
        }
        self.edit_index = index.min(self.levels.len() - 1);
    }

    fn next_level(&mut self) {
        if !self.levels.is_empty() {
            self.edit_index = (self.edit_index + 1) % self.levels.len();
        }
    }

    fn previous_level(&mut self) {
        if !self.levels.is_empty() {
            self.edit_index = (self.edit_index + self.levels.len() - 1) % self.levels.len();
        }
    }

    fn paint(&mut self, col: usize, row: usize) {
        let erasing = self.erasing;
        let brush = self.brush;
        let max_styles = MAX_STYLES;
        let mut full = false;
        if let Some(level) = self.levels.get_mut(self.edit_index) {
            if row < level.rows.len() && col < COLS {
                let cell = if erasing {
                    '.'
                } else {
                    match level.styles.iter().position(|style| *style == brush) {
                        Some(index) => index_char(index),
                        None => {
                            if level.styles.len() >= max_styles {
                                full = true;
                                '.'
                            } else {
                                level.styles.push(brush);
                                index_char(level.styles.len() - 1)
                            }
                        }
                    }
                };
                if !full {
                    let mut cells: Vec<char> = level.rows[row].chars().collect();
                    if col < cells.len() {
                        cells[col] = cell;
                        level.rows[row] = cells.into_iter().collect();
                    }
                }
            }
        }
        if full {
            self.set_status(format!("This level already uses {max_styles} colours"));
        }
    }

    fn resize_level_rows(&mut self, rows: usize) {
        let rows = rows.clamp(1, MAX_ROWS);
        if let Some(level) = self.levels.get_mut(self.edit_index) {
            level.rows.resize(rows, ".".repeat(COLS));
        }
    }

    /// Sends the whole level set to the Server Guest to validate and store.
    /// Validates and saves the level being edited. The server replaces that index or appends it.
    fn save_levels(&mut self) {
        if !self.admin {
            self.set_status("Only the poster can save levels".to_string());
            return;
        }
        let Some(level) = self.levels.get(self.edit_index).cloned() else {
            return;
        };
        if !level_is_playable(&level) {
            self.set_status(format!(
                "Level {} needs a breakable brick and at most {MAX_STYLES} colours",
                self.edit_index + 1
            ));
            return;
        }
        self.seq = self.seq.wrapping_add(1);
        let index = self.edit_index;
        let message = match json!({ "type": "save_level", "index": index, "level": level }) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        match send_message(message, self.seq) {
            Ok(()) => self.set_status(format!("Saving level {}...", index + 1)),
            Err(error) => self.set_status(format!("Could not save: {}", error.message)),
        }
    }

    fn reset_levels(&mut self) {
        if !self.admin {
            return;
        }
        self.levels = default_levels();
        self.edit_index = 0;
        self.seq = self.seq.wrapping_add(1);
        let message = match json!({ "type": "reset_levels" }) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        match send_message(message, self.seq) {
            Ok(()) => self.set_status("Restored the default levels".to_string()),
            Err(error) => self.set_status(format!("Could not reset: {}", error.message)),
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Particles
    // -----------------------------------------------------------------------------------------------------------

    fn push_particle(&mut self, particle: Particle) {
        if self.particles.len() >= MAX_PARTICLES {
            self.particles.remove(0);
        }
        self.particles.push(particle);
    }

    fn burst(&mut self, x: f32, y: f32, color: u32, count: usize, speed: f32, spark: bool) {
        let count = if self.reduced { count / 2 } else { count };
        for _ in 0..count {
            let angle = self.unit() * TAU;
            let magnitude = speed * (0.35 + self.unit() * 0.9);
            let life = 0.35 + self.unit() * 0.55;
            let size = 1.4 + self.unit() * 2.4;
            let particle = Particle {
                x,
                y,
                vx: angle.cos() * magnitude,
                vy: angle.sin() * magnitude - 20.0,
                life,
                max_life: life,
                size,
                color,
                gravity: if spark { 30.0 } else { 210.0 },
                spark,
            };
            self.push_particle(particle);
        }
    }

    fn spawn_confetti(&mut self) {
        let width = self.width;
        let height = self.height;
        let count = if self.reduced { 50 } else { 150 };
        for i in 0..count {
            let x = self.unit() * width;
            let color = PALETTE[i % PALETTE.len()];
            let life = 1.2 + self.unit() * 1.4;
            let start_y = -self.unit() * height * 0.4;
            let vx = (self.unit() - 0.5) * 140.0;
            let vy = 60.0 + self.unit() * 160.0;
            let size = 2.0 + self.unit() * 3.0;
            let particle = Particle {
                x,
                y: start_y,
                vx,
                vy,
                life,
                max_life: life,
                size,
                color,
                gravity: 120.0,
                spark: false,
            };
            self.push_particle(particle);
        }
    }

    fn update_particles(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.life -= dt;
            if p.life <= 0.0 {
                self.particles.swap_remove(i);
                continue;
            }
            p.vy += p.gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            i += 1;
        }
    }

    fn spawn_shake(&mut self, amount: f32) {
        self.shake = self.shake.max(amount);
    }

    // -----------------------------------------------------------------------------------------------------------
    // Draw
    // -----------------------------------------------------------------------------------------------------------

    fn draw(&self, canvas: &mut Canvas2D) {
        // Letterbox: fill the whole backing store, then map the fixed logical playfield onto it, so windowed and
        // fullscreen share one coordinate system and keep their aspect ratio.
        canvas.fill_color(0x04060bff).fill_rect(
            0.0,
            0.0,
            self.view_w.max(1.0),
            self.view_h.max(1.0),
        );
        canvas
            .save()
            .translate(self.off_x, self.off_y)
            .scale(self.scale, self.scale);

        match self.mode {
            Mode::Editor => self.draw_editor(canvas),
            Mode::Game => {
                let width = self.width.max(1.0);
                let height = self.height.max(1.0);
                self.draw_background(canvas, width, height);

                let shake_x = if self.shake > 0.0 && !self.reduced {
                    (self.unit_time(53.0).sin()) * self.shake
                } else {
                    0.0
                };
                let shake_y = if self.shake > 0.0 && !self.reduced {
                    (self.unit_time(71.0).cos()) * self.shake
                } else {
                    0.0
                };

                canvas.save();
                canvas.translate(shake_x, shake_y);
                self.draw_playfield(canvas);
                self.draw_bricks(canvas);
                self.draw_powerups(canvas);
                self.draw_particles(canvas);
                self.draw_balls(canvas);
                self.draw_bullets(canvas);
                self.draw_paddle(canvas);
                canvas.restore();

                self.draw_hud(canvas, width);
                self.draw_overlays(canvas, width, height);
                self.draw_status(canvas, width, height);
            }
        }
        canvas.restore();
    }

    /// The transient status pill, used for playtest messages and editor feedback in game mode.
    fn draw_status(&self, canvas: &mut Canvas2D, width: f32, height: f32) {
        if self.status_timer <= 0.0 {
            return;
        }
        let Some(status) = self.status.as_deref() else {
            return;
        };
        let alpha = (self.status_timer / 1.5).clamp(0.0, 1.0);
        let y = height - PADDLE_Y_FROM_BOTTOM - 46.0;
        canvas.set_alpha(0.75 * alpha).fill_color(0x05070eff);
        rounded(canvas, MARGIN, y - 11.0, width - MARGIN * 2.0, 23.0, 11.0);
        canvas
            .set_alpha(alpha)
            .font("monospace", 10.0, 700.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_color(PALETTE[4])
            .fill_text(status, width / 2.0, y)
            .set_alpha(1.0);
    }

    fn unit_time(&self, seed: f32) -> f32 {
        self.time * 60.0 + seed
    }

    fn draw_background(&self, canvas: &mut Canvas2D, width: f32, height: f32) {
        canvas.def_linear(1, 0.0, 0.0, 0.0, height, &[(0.0, BG_TOP), (1.0, BG_BOTTOM)]);
        canvas.fill_gradient(1).fill_rect(0.0, 0.0, width, height);

        // A drifting nebula, then a vignette, for depth.
        let drift = (self.time * 0.18).sin() * width * 0.18;
        canvas.def_radial(
            2,
            width * 0.5 + drift,
            height * 0.34,
            6.0,
            width * 0.5 + drift,
            height * 0.36,
            width * 0.72,
            &[
                (0.0, with_alpha(PALETTE[5], 46)),
                (1.0, with_alpha(PALETTE[5], 0)),
            ],
        );
        canvas
            .set_composite(Composite::Lighter)
            .fill_gradient(2)
            .fill_rect(0.0, 0.0, width, height)
            .set_composite(Composite::SourceOver);
        canvas.def_radial(
            3,
            width * 0.5,
            height * 0.45,
            width * 0.15,
            width * 0.5,
            height * 0.5,
            width * 0.8,
            &[(0.0, 0x00000000), (1.0, 0x000000c0)],
        );
        canvas.fill_gradient(3).fill_rect(0.0, 0.0, width, height);

        canvas.fill_color(WHITE);
        for star in &self.stars {
            let twinkle = 0.5 + 0.5 * (self.time * star.speed + star.phase).sin();
            let alpha = (30.0 + twinkle * 150.0) as u8;
            canvas
                .set_alpha(alpha as f32 / 255.0)
                .fill_rect(star.x, star.y, star.size, star.size);
        }
        canvas.set_alpha(1.0);
    }

    fn draw_playfield(&self, canvas: &mut Canvas2D) {
        let width = self.width;
        let height = self.height;
        let left = MARGIN;
        let right = width - MARGIN;
        let bottom = height - 4.0;
        let glow = with_alpha(PALETTE[4], 40);
        canvas.line_width(3.0).stroke_color(glow).stroke_rect(
            left - 3.0,
            TOP - 3.0,
            right - left + 6.0,
            bottom - TOP + 6.0,
        );
        canvas
            .line_width(1.2)
            .stroke_color(with_alpha(PALETTE[4], 120))
            .stroke_rect(left, TOP, right - left, bottom - TOP);
        // A faint floor line so losing a ball reads instantly.
        canvas.fill_color(with_alpha(PALETTE[0], 55)).fill_rect(
            left,
            bottom - 1.0,
            right - left,
            1.5,
        );
    }

    fn brick_w(&self) -> f32 {
        (self.width - MARGIN * 2.0 - GAP * (COLS as f32 - 1.0)) / COLS as f32
    }

    fn draw_bricks(&self, canvas: &mut Canvas2D) {
        for brick in &self.bricks {
            if !brick.alive {
                continue;
            }
            draw_brick(
                canvas,
                brick.x,
                brick.y,
                brick.w,
                brick.h,
                brick.color,
                brick.hp,
                brick.max_hp,
                brick.solid,
            );
        }
    }

    fn draw_powerups(&self, canvas: &mut Canvas2D) {
        for powerup in &self.powerups {
            let color = powerup.kind.color();
            let pulse = 0.6 + 0.4 * (powerup.spin * 2.0).sin().abs();
            canvas
                .set_composite(Composite::Lighter)
                .set_alpha(0.35 * pulse)
                .fill_color(color);
            rounded(
                canvas,
                powerup.x - powerup.w / 2.0 - 3.0,
                powerup.y - powerup.h / 2.0 - 3.0,
                powerup.w + 6.0,
                powerup.h + 6.0,
                6.0,
            );
            canvas.set_composite(Composite::SourceOver).set_alpha(1.0);
            canvas.fill_color(shade(color, 0.5));
            rounded(
                canvas,
                powerup.x - powerup.w / 2.0,
                powerup.y - powerup.h / 2.0,
                powerup.w,
                powerup.h,
                4.0,
            );
            canvas.fill_color(color);
            rounded(
                canvas,
                powerup.x - powerup.w / 2.0 + 1.5,
                powerup.y - powerup.h / 2.0 + 1.5,
                powerup.w - 3.0,
                powerup.h - 5.0,
                3.0,
            );
            canvas
                .font("monospace", 8.0, 700.0)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .fill_color(0x10131bff)
                .fill_text(powerup.kind.letter(), powerup.x, powerup.y - 0.5);
        }
    }

    fn draw_particles(&self, canvas: &mut Canvas2D) {
        for p in &self.particles {
            let t = (p.life / p.max_life).clamp(0.0, 1.0);
            canvas.set_alpha(t * 235.0 / 255.0);
            if p.spark {
                canvas
                    .line_width(p.size.max(1.0))
                    .line_cap(LineCap::Round)
                    .stroke_color(p.color);
                canvas
                    .begin_path()
                    .move_to(p.x, p.y)
                    .line_to(p.x - p.vx * 0.02, p.y - p.vy * 0.02)
                    .stroke();
            } else {
                let size = p.size * (0.4 + t * 0.6);
                canvas.fill_color(p.color).fill_rect(
                    p.x - size / 2.0,
                    p.y - size / 2.0,
                    size,
                    size,
                );
            }
        }
        canvas.set_alpha(1.0);
    }

    fn draw_balls(&self, canvas: &mut Canvas2D) {
        for ball in &self.balls {
            // A fading tail.
            canvas.line_cap(LineCap::Round);
            let mut previous = (ball.x, ball.y);
            for (i, point) in ball.trail.iter().enumerate().rev() {
                let strength = (i + 1) as f32 / (ball.trail.len() as f32 + 1.0);
                canvas
                    .set_alpha(strength * 0.4)
                    .line_width(ball.r * 1.3 * strength)
                    .stroke_color(BALL_CORE);
                canvas
                    .begin_path()
                    .move_to(previous.0, previous.1)
                    .line_to(point.0, point.1)
                    .stroke();
                previous = *point;
            }
            canvas
                .set_alpha(1.0)
                .set_composite(Composite::Lighter)
                .set_alpha(0.55)
                .fill_color(PALETTE[2]);
            circle(canvas, ball.x, ball.y, ball.r * 2.1);
            canvas.set_composite(Composite::SourceOver).set_alpha(1.0);
            canvas.fill_color(BALL_CORE);
            circle(canvas, ball.x, ball.y, ball.r);
            canvas.fill_color(WHITE);
            circle(canvas, ball.x - 1.2, ball.y - 1.4, ball.r * 0.45);
        }
    }

    fn draw_bullets(&self, canvas: &mut Canvas2D) {
        for bullet in &self.bullets {
            canvas
                .set_composite(Composite::Lighter)
                .set_alpha(0.65)
                .fill_color(PALETTE[0]);
            canvas.fill_rect(bullet.x - 2.5, bullet.y - 9.0, 5.0, 12.0);
            canvas.set_composite(Composite::SourceOver).set_alpha(1.0);
            canvas.fill_color(WHITE);
            canvas.fill_rect(bullet.x - 1.0, bullet.y - 8.0, 2.0, 10.0);
        }
    }

    fn draw_paddle(&self, canvas: &mut Canvas2D) {
        let half = self.paddle_w / 2.0;
        let x = self.paddle_x - half;
        let y = self.paddle_y();
        let color = if self.expand_timer > 0.0 {
            PADDLE_WIDE
        } else {
            PADDLE_BASE
        };
        canvas
            .set_composite(Composite::Lighter)
            .set_alpha(0.3)
            .fill_color(color);
        rounded(canvas, x - 3.0, y - 4.0, self.paddle_w + 6.0, 16.0, 8.0);
        canvas.set_composite(Composite::SourceOver).set_alpha(1.0);

        canvas.fill_color(shade(color, 0.45));
        rounded(canvas, x, y, self.paddle_w, 9.0, 4.5);
        canvas.fill_color(color);
        rounded(canvas, x + 1.0, y + 1.0, self.paddle_w - 2.0, 6.0, 3.0);
        canvas
            .set_alpha(0.55)
            .fill_color(shade(color, 1.7))
            .fill_rect(x + 4.0, y + 1.5, self.paddle_w - 8.0, 1.6)
            .set_alpha(1.0);

        if self.shots > 0 {
            let pulse = 0.5 + 0.5 * (self.time * 12.0).sin();
            canvas.fill_color(with_alpha(PALETTE[0], (140.0 + pulse * 110.0) as u8));
            for side in [-1.0f32, 1.0] {
                let turret_x = self.paddle_x + side * (half - 6.0) - 2.0;
                canvas.fill_rect(turret_x, y - 5.0, 4.0, 5.0);
            }
        }
        if self.expand_timer > 0.0 {
            canvas
                .line_width(1.0)
                .stroke_color(with_alpha(WHITE, 150))
                .stroke_rect(x + 0.5, y + 0.5, self.paddle_w - 1.0, 8.0);
        }
    }

    fn draw_hud(&self, canvas: &mut Canvas2D, width: f32) {
        canvas
            .set_alpha(0.55)
            .fill_color(0x080b14ff)
            .fill_rect(0.0, 0.0, width, TOP - 6.0)
            .set_alpha(1.0);
        canvas
            .fill_color(with_alpha(PALETTE[4], 90))
            .fill_rect(0.0, TOP - 6.5, width, 1.2)
            .fill_rect(MARGIN, TOP - 26.0, 2.0, 13.0);

        // Score.
        canvas
            .font("monospace", 17.0, 700.0)
            .text_align(TextAlign::Left)
            .text_baseline(TextBaseline::Top)
            .fill_color(TEXT)
            .fill_text(
                &format!("{:06}", self.score.min(999_999)),
                MARGIN + 6.0,
                6.0,
            );
        canvas
            .font("sans-serif", 8.0, 600.0)
            .fill_color(MUTED)
            .fill_text("SCORE", MARGIN + 6.0, 25.0);

        // Level, centered.
        let level_name = self
            .active_levels()
            .get(self.level)
            .map(|level| level.name.as_str())
            .unwrap_or("");
        canvas
            .text_align(TextAlign::Center)
            .font("monospace", 10.0, 700.0)
            .fill_color(PALETTE[4])
            .fill_text(
                &format!("LEVEL {}  ·  {}", self.level + 1, clip(level_name, 12)),
                width / 2.0,
                9.0,
            );

        // Lives, kept clear of the host's top-right fullscreen button.
        let life_color = if self.lives <= 1 {
            PALETTE[0]
        } else {
            BALL_CORE
        };
        for i in 0..self.lives.min(6) {
            let x = width - MARGIN - 46.0 - i as f32 * 15.0;
            canvas.fill_color(life_color);
            circle(canvas, x, 18.0, 4.0);
            canvas.set_alpha(0.6).fill_color(shade(life_color, 1.6));
            circle(canvas, x - 1.0, 16.8, 1.5);
            canvas.set_alpha(1.0);
        }

        // The poster's entry to the level editor, bottom-left.
        if self.admin {
            let (bx, by, bw, bh) = edit_button_rect(width, self.height);
            canvas.fill_color(with_alpha(PALETTE[4], 45));
            rounded(canvas, bx, by, bw, bh, 4.0);
            canvas
                .line_width(1.0)
                .stroke_color(with_alpha(PALETTE[4], 170))
                .stroke_rect(bx + 0.5, by + 0.5, bw - 1.0, bh - 1.0);
            canvas
                .font("monospace", 8.0, 700.0)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .fill_color(PALETTE[4])
                .fill_text("EDIT", bx + bw / 2.0, by + bh / 2.0);
        }

        // Active power-up chips, along the lower right, clear of the score, the paddle and the host's fullscreen button.
        let mut active: Vec<(PowerKind, f32, u32)> = Vec::new();
        if self.expand_timer > 0.0 {
            active.push((PowerKind::Expand, self.expand_timer / 16.0, 0));
        }
        if self.shots > 0 {
            active.push((PowerKind::Laser, 1.0, self.shots));
        }
        if self.slow_timer > 0.0 {
            active.push((PowerKind::Slow, self.slow_timer / 9.0, 0));
        }
        let chip_w = 42.0;
        let chip_h = 12.0;
        let chip_y = self.height - chip_h - 30.0;
        let mut chip_x = width - MARGIN - chip_w;
        for (kind, fraction, count) in active {
            let color = kind.color();
            canvas.fill_color(with_alpha(color, 70));
            rounded(canvas, chip_x, chip_y, chip_w, chip_h, 4.0);
            canvas.fill_color(color);
            rounded(
                canvas,
                chip_x,
                chip_y,
                chip_w * fraction.clamp(0.12, 1.0),
                chip_h,
                4.0,
            );
            canvas
                .font("monospace", 8.0, 700.0)
                .text_align(TextAlign::Left)
                .text_baseline(TextBaseline::Middle)
                .fill_color(0x0b0e15ff)
                .fill_text(kind.letter(), chip_x + 6.0, chip_y + chip_h / 2.0);
            if count > 0 {
                canvas.text_align(TextAlign::Right).fill_text(
                    &format!("{count}"),
                    chip_x + chip_w - 6.0,
                    chip_y + chip_h / 2.0,
                );
            }
            chip_x -= chip_w + 5.0;
        }

        // Combo burst.
        if self.combo >= 4 && self.combo_timer > 0.0 {
            let t = (self.combo_timer / 1.4).clamp(0.0, 1.0);
            canvas
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .font("monospace", 16.0 + t * 6.0, 700.0)
                .fill_color(with_alpha(PALETTE[2], (t * 220.0) as u8));
            let x = self.balls.first().map(|b| b.x).unwrap_or(width / 2.0);
            canvas.fill_text(
                &format!("x{}", self.combo_multiplier()),
                x,
                self.paddle_y() - 34.0,
            );
        }
    }

    fn draw_overlays(&self, canvas: &mut Canvas2D, width: f32, height: f32) {
        let center_y = height / 2.0;

        // Before the first interaction the board is inert; say so plainly.
        if !self.engaged {
            self.draw_begin_prompt(canvas, width, center_y);
            return;
        }

        // A waiting ball: nudge the player to launch it.
        if self.awaiting_launch && matches!(self.phase, Phase::Playing) {
            canvas
                .font("sans-serif", 10.0, 600.0)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .fill_color(with_alpha(TEXT, 220))
                .fill_text(
                    "Press Space or tap to launch",
                    width / 2.0,
                    self.paddle_y() - 34.0,
                );
        }
        if self.banner > 0.0 && matches!(self.phase, Phase::Playing) {
            let t = (self.banner / 2.4).clamp(0.0, 1.0);
            let alpha = (t.min(0.35) / 0.35 * 220.0) as u8;
            canvas
                .set_composite(Composite::Lighter)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .font("monospace", 26.0, 700.0)
                .fill_color(with_alpha(PALETTE[4], alpha / 3))
                .fill_text(
                    &format!("LEVEL {}", self.level + 1),
                    width / 2.0,
                    center_y - 8.0,
                )
                .set_composite(Composite::SourceOver)
                .fill_color(with_alpha(TEXT, alpha))
                .fill_text(
                    &format!("LEVEL {}", self.level + 1),
                    width / 2.0,
                    center_y - 8.0,
                );
            canvas
                .font("sans-serif", 12.0, 600.0)
                .fill_color(with_alpha(MUTED, alpha))
                .fill_text(
                    self.active_levels()
                        .get(self.level)
                        .map(|level| level.name.as_str())
                        .unwrap_or(""),
                    width / 2.0,
                    center_y + 16.0,
                );
        }

        match self.phase {
            Phase::Clearing(timer) => {
                let alpha = (220.0 * (timer / 2.3).clamp(0.0, 1.0)).min(220.0) as u8;
                self.overlay_panel(canvas, width, height);
                canvas
                    .text_align(TextAlign::Center)
                    .text_baseline(TextBaseline::Middle)
                    .font("monospace", 24.0, 700.0)
                    .fill_color(with_alpha(PALETTE[3], alpha))
                    .fill_text("LEVEL CLEAR", width / 2.0, center_y - 4.0);
                canvas
                    .font("sans-serif", 12.0, 500.0)
                    .fill_color(with_alpha(TEXT, alpha))
                    .fill_text(
                        &format!("{} points", self.score),
                        width / 2.0,
                        center_y + 22.0,
                    );
            }
            Phase::Over | Phase::Won => {
                self.overlay_panel(canvas, width, height);
                let title = if matches!(self.phase, Phase::Won) {
                    "ALL LEVELS CLEARED"
                } else {
                    "GAME OVER"
                };
                let accent = if matches!(self.phase, Phase::Won) {
                    PALETTE[3]
                } else {
                    PALETTE[0]
                };
                canvas
                    .text_align(TextAlign::Center)
                    .text_baseline(TextBaseline::Middle)
                    .font("monospace", 21.0, 700.0)
                    .fill_color(accent)
                    .fill_text(title, width / 2.0, center_y - 18.0);
                canvas
                    .font("monospace", 15.0, 700.0)
                    .fill_color(TEXT)
                    .fill_text(
                        &format!("SCORE {:06}", self.score.min(999_999)),
                        width / 2.0,
                        center_y + 8.0,
                    );
                canvas
                    .font("sans-serif", 11.0, 500.0)
                    .fill_color(MUTED)
                    .fill_text(
                        self.notice
                            .as_deref()
                            .unwrap_or("Space or tap to play again"),
                        width / 2.0,
                        center_y + 32.0,
                    );
            }
            Phase::Playing => {
                if self.toast > 0.0 {
                    if let Some(notice) = self.notice.as_deref() {
                        let alpha = (self.toast / 1.6).clamp(0.0, 1.0);
                        let y = height - PADDLE_Y_FROM_BOTTOM - 46.0;
                        canvas.set_alpha(0.75 * alpha).fill_color(0x05070eff);
                        rounded(canvas, width / 2.0 - 62.0, y - 11.0, 124.0, 23.0, 11.0);
                        canvas
                            .set_alpha(alpha)
                            .font("monospace", 11.0, 700.0)
                            .text_align(TextAlign::Center)
                            .text_baseline(TextBaseline::Middle)
                            .fill_color(PALETTE[2])
                            .fill_text(notice, width / 2.0, y)
                            .set_alpha(1.0);
                    }
                }
            }
        }

        // A soft edge glow instead of a full-screen white flash: low contrast, slow, and never a luminance spike.
        if self.flash > 0.0 {
            let peak = (self.flash * 96.0).min(96.0) as u8;
            canvas.def_radial(
                9,
                width * 0.5,
                height * 0.5,
                width * 0.2,
                width * 0.5,
                height * 0.5,
                width * 0.78,
                &[
                    (0.0, with_alpha(self.flash_color, 0)),
                    (0.55, with_alpha(self.flash_color, 0)),
                    (1.0, with_alpha(self.flash_color, peak)),
                ],
            );
            canvas.fill_gradient(9).fill_rect(0.0, 0.0, width, height);
        }
    }

    fn draw_begin_prompt(&self, canvas: &mut Canvas2D, width: f32, center_y: f32) {
        canvas
            .set_alpha(0.72)
            .fill_color(0x05070eff)
            .fill_rect(0.0, center_y - 46.0, width, 92.0)
            .set_alpha(1.0);
        canvas
            .fill_color(with_alpha(PALETTE[4], 140))
            .fill_rect(0.0, center_y - 46.0, width, 1.2)
            .fill_rect(0.0, center_y + 45.0, width, 1.2);
        canvas
            .font("monospace", 16.0, 700.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_color(PALETTE[4])
            .fill_text("CLICK OR TAP TO BEGIN", width / 2.0, center_y - 11.0);
        canvas
            .font("sans-serif", 10.0, 500.0)
            .fill_color(TEXT)
            .fill_text(
                "Then press Space to launch the ball",
                width / 2.0,
                center_y + 14.0,
            );
    }

    fn overlay_panel(&self, canvas: &mut Canvas2D, width: f32, height: f32) {
        canvas
            .set_alpha(0.72)
            .fill_color(0x05070eff)
            .fill_rect(0.0, height / 2.0 - 52.0, width, 104.0)
            .set_alpha(1.0);
        canvas
            .fill_color(with_alpha(PALETTE[4], 120))
            .fill_rect(0.0, height / 2.0 - 52.0, width, 1.2)
            .fill_rect(0.0, height / 2.0 + 51.0, width, 1.2);
    }
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

/// Shortens `text` to `max` characters, with an ellipsis when it was longer.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn point_in(x: f32, y: f32, brick: &Brick) -> bool {
    x >= brick.x && x <= brick.x + brick.w && y >= brick.y && y <= brick.y + brick.h
}

/// Whether a level that arrived from the Server Guest can be played: a shape the game understands, with at least one
/// brick to clear.
fn level_is_playable(level: &Level) -> bool {
    if level.rows.is_empty() || level.rows.len() > MAX_ROWS {
        return false;
    }
    if level.styles.is_empty() || level.styles.len() > MAX_STYLES {
        return false;
    }
    if level
        .styles
        .iter()
        .any(|style| !style.solid && !(1..=3).contains(&style.hp))
    {
        return false;
    }
    let mut breakable = false;
    for row in &level.rows {
        if row.chars().count() != COLS {
            return false;
        }
        for cell in row.chars() {
            if cell == '.' {
                continue;
            }
            let Some(index) = index_of(cell) else {
                return false;
            };
            let Some(style) = level.styles.get(index) else {
                return false;
            };
            if !style.solid {
                breakable = true;
            }
        }
    }
    breakable
}

/// Draws one brick. Used both in play and in the editor, so the editor renders true to how a brick appears.
#[allow(clippy::too_many_arguments)]
fn draw_brick(
    canvas: &mut Canvas2D,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: u32,
    hp: u8,
    max_hp: u8,
    solid: bool,
) {
    let damaged = if solid { 0 } else { max_hp.saturating_sub(hp) };
    let base = shade(color, 1.0 - damaged as f32 * 0.22);
    let short = w.min(h);
    let radius = (short * 0.22).clamp(0.8, 3.0);
    canvas.fill_color(with_alpha(shade(base, 0.35), 150));
    rounded(canvas, x - 1.0, y - 1.0, w + 2.0, h + 2.0, radius + 1.0);
    canvas.fill_color(base);
    rounded(canvas, x, y, w, h, radius);
    // Glossy top and a shadowed base edge.
    canvas
        .set_alpha(0.42)
        .fill_color(shade(base, 1.55))
        .fill_rect(x + w * 0.14, y + h * 0.14, w * 0.72, h * 0.32)
        .set_alpha(0.5)
        .fill_color(shade(base, 0.35))
        .fill_rect(x + w * 0.14, y + h - h * 0.2, w * 0.72, h * 0.1)
        .set_alpha(1.0);
    canvas
        .line_width(1.0)
        .stroke_color(with_alpha(shade(base, 1.4), 150))
        .stroke_rect(x + 0.5, y + 0.5, w - 1.0, h - 1.0);

    // Reinforced bricks read as armoured at a glance: an inner plate, a second ring for three-hit bricks, and corner
    // rivets. Damage still shows as cracks on top.
    if !solid && max_hp >= 2 {
        let inset = short * 0.18;
        canvas
            .line_width(1.0)
            .stroke_color(with_alpha(shade(base, 2.0), 230))
            .stroke_rect(x + inset, y + inset, w - inset * 2.0, h - inset * 2.0);
        if max_hp >= 3 {
            let inner = inset + short * 0.12;
            canvas
                .line_width(1.0)
                .stroke_color(with_alpha(0x0b0e15ff, 170))
                .stroke_rect(x + inner, y + inner, w - inner * 2.0, h - inner * 2.0);
        }
        canvas.fill_color(with_alpha(0xffffffe0, 210));
        let r = (short * 0.06).max(0.55);
        let m = short * 0.2;
        for (dx, dy) in [(m, m), (w - m, m), (m, h - m), (w - m, h - m)] {
            canvas.fill_rect(x + dx - r, y + dy - r, r * 2.0, r * 2.0);
        }
    }

    if solid {
        canvas
            .line_width((short * 0.08).max(1.0))
            .stroke_color(with_alpha(0xd8e2ffff, 200));
        canvas
            .begin_path()
            .move_to(x + w * 0.22, y + h * 0.22)
            .line_to(x + w * 0.78, y + h * 0.78)
            .move_to(x + w * 0.78, y + h * 0.22)
            .line_to(x + w * 0.22, y + h * 0.78)
            .stroke();
    } else if hp < max_hp {
        let cracks = (max_hp - hp) as usize;
        canvas.line_width(1.0).stroke_color(0x00000099);
        for crack in 0..cracks {
            let cx = x + w * (0.3 + crack as f32 * 0.18);
            canvas
                .begin_path()
                .move_to(cx, y + h * 0.2)
                .line_to(cx + w * 0.16, y + h / 2.0)
                .line_to(cx - w * 0.1, y + h - h * 0.2)
                .stroke();
        }
    }
}

fn rotate(vx: f32, vy: f32, angle: f32) -> (f32, f32) {
    let (sin, cos) = angle.sin_cos();
    (vx * cos - vy * sin, vx * sin + vy * cos)
}

fn powerup_name(kind: PowerKind) -> &'static str {
    match kind {
        PowerKind::MultiBall => "Multiball",
        PowerKind::Expand => "Wide paddle",
        PowerKind::Laser => "Laser",
        PowerKind::Slow => "Slow-mo",
        PowerKind::Life => "Extra life",
    }
}

fn rounded(canvas: &mut Canvas2D, x: f32, y: f32, w: f32, h: f32, r: f32) {
    canvas
        .begin_path()
        .round_rect(x, y, w, h, r)
        .fill(FillRule::NonZero);
}

fn circle(canvas: &mut Canvas2D, x: f32, y: f32, r: f32) {
    canvas
        .begin_path()
        .arc(x, y, r, 0.0, TAU, false)
        .fill(FillRule::NonZero);
}

// ---------------------------------------------------------------------------------------------------------------
// Level editor layout
// ---------------------------------------------------------------------------------------------------------------

const EDIT_TOP: f32 = 46.0;
const COLOUR_COLS: usize = 24;
const COLOUR_ROWS: usize = 6;
/// Saturation and value for each row of the colour field, top to bottom.
const SHADES: [(f32, f32); COLOUR_ROWS] = [
    (1.0, 1.0),
    (1.0, 0.78),
    (1.0, 0.55),
    (0.78, 1.0),
    (0.5, 1.0),
    (1.0, 0.32),
];
/// The strength buttons: health 1-3, then steel.
const STRENGTHS: [(u8, bool, &str); 4] = [
    (1, false, "1"),
    (2, false, "2"),
    (3, false, "3"),
    (1, true, "STEEL"),
];

#[derive(Clone, Copy, PartialEq)]
enum EditorAction {
    Previous,
    Next,
    New,
    AddRow,
    RemoveRow,
    Defaults,
    Save,
    Playtest,
    Play,
    Rename,
}

const EDITOR_ROW_A: [(&str, EditorAction); 6] = [
    ("PREV", EditorAction::Previous),
    ("NEXT", EditorAction::Next),
    ("NEW", EditorAction::New),
    ("+ROW", EditorAction::AddRow),
    ("-ROW", EditorAction::RemoveRow),
    ("DEFAULTS", EditorAction::Defaults),
];
const EDITOR_ROW_B: [(&str, EditorAction); 4] = [
    ("SAVE", EditorAction::Save),
    ("PLAYTEST", EditorAction::Playtest),
    ("PLAY ALL", EditorAction::Play),
    ("RENAME", EditorAction::Rename),
];

/// The on-screen keypad used to rename a level on touch devices. `DEL`, `SP`, `OK` and `X` are special.
const RENAME_KEYS: [&str; 40] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "SP",
    "DEL", "OK", "X",
];

/// Cell size inside the editor grid, in logical pixels. The cells are square, exactly as in play.
fn editor_cell_size(width: f32) -> (f32, f32) {
    let cell = (width - MARGIN * 2.0 - GAP * (COLS as f32 - 1.0)) / COLS as f32;
    (cell, cell)
}

fn editor_grid_rect(width: f32) -> (f32, f32, f32, f32) {
    let (cell, _) = editor_cell_size(width);
    let total = cell * COLS as f32 + GAP * (COLS as f32 - 1.0);
    (MARGIN, EDIT_TOP, total, total)
}

/// The hue-and-shade colour field, under the grid.
fn editor_colour_rect(width: f32) -> (f32, f32, f32, f32) {
    let (_, _, grid_w, grid_h) = editor_grid_rect(width);
    (MARGIN, EDIT_TOP + grid_h + 5.0, grid_w, 36.0)
}

/// The colour a cell of the colour field picks.
fn colour_at(col: usize, row: usize) -> u32 {
    let hue = col as f32 / COLOUR_COLS as f32;
    let (saturation, value) = SHADES[row.min(SHADES.len() - 1)];
    hsv_to_rgb(hue, saturation, value)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let h = (hue.rem_euclid(1.0)) * 6.0;
    let sector = h.floor() as i32;
    let f = h - sector as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * f);
    let t = value * (1.0 - saturation * (1.0 - f));
    let (r, g, b) = match sector.rem_euclid(6) {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    let channel = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(r) << 16) | (channel(g) << 8) | channel(b)
}

/// The strength buttons, under the colour field.
fn editor_strength_rects(width: f32) -> [(f32, f32, f32, f32); 4] {
    let (_, colour_y, grid_w, colour_h) = editor_colour_rect(width);
    let y = colour_y + colour_h + 4.0;
    let gap = 4.0;
    let count = STRENGTHS.len() as f32;
    let w = (grid_w - gap * (count - 1.0)) / count;
    std::array::from_fn(|index| (MARGIN + index as f32 * (w + gap), y, w, 16.0))
}

/// The style swatches: the eraser, then each style the level uses. `count` includes the eraser.
fn editor_palette_rects(width: f32, count: usize) -> Vec<(f32, f32, f32, f32)> {
    let (_, colour_y, grid_w, colour_h) = editor_colour_rect(width);
    let y = colour_y + colour_h + 4.0 + 16.0 + 4.0;
    let count = count.max(1) as f32;
    let gap = 3.0;
    let w = (grid_w - gap * (count - 1.0)) / count;
    (0..count as usize)
        .map(|index| (MARGIN + index as f32 * (w + gap), y, w, 17.0))
        .collect()
}

/// The action buttons: six on the first row, four on the second.
fn editor_button_rects(width: f32) -> Vec<(&'static str, EditorAction, f32, f32, f32, f32)> {
    let (_, colour_y, _, colour_h) = editor_colour_rect(width);
    let row_a_y = colour_y + colour_h + 4.0 + 16.0 + 4.0 + 17.0 + 5.0;
    let row_b_y = row_a_y + 17.0 + 4.0;
    let mut out = Vec::new();
    let gap = 4.0;
    let count = EDITOR_ROW_A.len() as f32;
    let w = (width - MARGIN * 2.0 - gap * (count - 1.0)) / count;
    for (index, (label, action)) in EDITOR_ROW_A.iter().enumerate() {
        out.push((
            *label,
            *action,
            MARGIN + index as f32 * (w + gap),
            row_a_y,
            w,
            17.0,
        ));
    }
    let count = EDITOR_ROW_B.len() as f32;
    let w = (width - MARGIN * 2.0 - gap * (count - 1.0)) / count;
    for (index, (label, action)) in EDITOR_ROW_B.iter().enumerate() {
        out.push((
            *label,
            *action,
            MARGIN + index as f32 * (w + gap),
            row_b_y,
            w,
            20.0,
        ));
    }
    out
}

fn point_in_rect(x: f32, y: f32, rect: (f32, f32, f32, f32)) -> bool {
    x >= rect.0 && x <= rect.0 + rect.2 && y >= rect.1 && y <= rect.1 + rect.3
}

/// The rename panel, centred over the editor.
fn rename_panel_rect(width: f32) -> (f32, f32, f32, f32) {
    let panel_w = (width - 16.0).min(340.0);
    let panel_h = 158.0;
    ((width - panel_w) / 2.0, 191.0, panel_w, panel_h)
}

/// The keypad's key rects, in [`RENAME_KEYS`] order.
fn rename_key_rects(width: f32) -> Vec<(f32, f32, f32, f32)> {
    let (px, py, panel_w, _) = rename_panel_rect(width);
    let gap = 4.0;
    let key_w = (panel_w - 16.0 - gap * 9.0) / 10.0;
    let key_h = 22.0;
    let start = py + 36.0;
    (0..RENAME_KEYS.len())
        .map(|index| {
            let row = index / 10;
            let col = index % 10;
            (
                px + 8.0 + col as f32 * (key_w + gap),
                start + row as f32 * (key_h + gap),
                key_w,
                key_h,
            )
        })
        .collect()
}

/// The bottom-left admin button that opens the editor.
fn edit_button_rect(_width: f32, height: f32) -> (f32, f32, f32, f32) {
    (MARGIN, height - 22.0, 52.0, 16.0)
}

thread_local! {
    static GAME: RefCell<Game> = RefCell::new(Game::default());
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        game.view_w = input.width as f32;
        game.view_h = input.height as f32;
        game.fullscreen = input.fullscreen;
        game.reduced = input.prefers_reduced_motion;
        game.width = LOGICAL_W;
        game.height = LOGICAL_H;
        game.apply_state(input.state.as_ref());
        game.refresh_view();
        game.reset();
    });
    Ok(reply(
        set_fps(if input.prefers_reduced_motion { 30 } else { 60 }).and_then(|()| request_frame()),
    ))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    let result = GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        game.update((input.dt as f32 / 1000.0).clamp(0.0, 0.05));
        let animate = matches!(game.mode, Mode::Editor)
            || matches!(game.phase, Phase::Playing | Phase::Clearing(_))
            || !game.particles.is_empty()
            || game.return_to_editor > 0.0;
        let mut canvas = Canvas2D::new();
        game.draw(&mut canvas);
        canvas.submit()?;
        if animate {
            request_frame()?;
        }
        Ok::<(), Failed>(())
    });
    Ok(reply(result))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let result = match input.r#type {
        CanvasEventType::Key => handle_key(&input),
        CanvasEventType::Pointer => handle_pointer(&input),
        CanvasEventType::State => {
            GAME.with(|cell| {
                let mut game = cell.borrow_mut();
                game.apply_state(input.state.as_ref());
            });
            request_frame()
        }
        CanvasEventType::Resize => {
            GAME.with(|cell| {
                let mut game = cell.borrow_mut();
                if let Some(width) = input.width {
                    game.view_w = width as f32;
                }
                if let Some(height) = input.height {
                    game.view_h = height as f32;
                }
                if let Some(fullscreen) = input.fullscreen {
                    game.fullscreen = fullscreen;
                }
                game.paddle_x = game.paddle_x.clamp(
                    MARGIN + 20.0,
                    (game.width - MARGIN - 20.0).max(MARGIN + 20.0),
                );
                game.refresh_view();
            });
            request_frame()
        }
        CanvasEventType::Reply => {
            let reply = input.reply.as_ref();
            let is_levels = reply
                .and_then(|reply| reply.get("levels"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let saved = reply
                .and_then(|reply| reply.get("saved"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            GAME.with(|cell| {
                let mut game = cell.borrow_mut();
                if is_levels {
                    game.set_status(if saved {
                        "Levels saved to the Chitt".to_string()
                    } else {
                        "The server saved the levels but nothing changed".to_string()
                    });
                } else {
                    game.notice = Some(
                        if saved {
                            "Score saved — Space to replay"
                        } else {
                            "No new best — Space to replay"
                        }
                        .to_string(),
                    );
                }
            });
            request_frame()
        }
        CanvasEventType::Error => {
            GAME.with(|cell| {
                let mut game = cell.borrow_mut();
                if matches!(game.mode, Mode::Editor) {
                    game.set_status("The server refused that change".to_string());
                } else {
                    game.notice = Some("Could not save — Space to replay".to_string());
                }
            });
            request_frame()
        }
        CanvasEventType::Visibility => {
            GAME.with(|cell| {
                let mut game = cell.borrow_mut();
                game.left_pressed = false;
                game.right_pressed = false;
                game.fire_held = false;
                game.pointer_down = false;
            });
            Ok(())
        }
        _ => Ok(()),
    };
    Ok(reply(result))
}

fn handle_key(input: &CanvasEventInput) -> Result<(), Failed> {
    GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        if matches!(game.mode, Mode::Editor) {
            game.handle_editor_key(input);
        } else {
            let key = input.key.as_deref().unwrap_or("");
            let down = input.action.as_deref() == Some("down");
            match key {
                "ArrowLeft" | "a" | "A" => game.left_pressed = down,
                "ArrowRight" | "d" | "D" => game.right_pressed = down,
                " " | "Space" | "Enter" => {
                    if down {
                        game.primary_action();
                    } else {
                        game.fire_held = false;
                    }
                }
                "e" | "E" if down && game.admin => game.enter_editor(),
                _ => {}
            }
        }
    });
    request_frame()
}

fn handle_pointer(input: &CanvasEventInput) -> Result<(), Failed> {
    GAME.with(|cell| {
        let mut game = cell.borrow_mut();
        if matches!(game.mode, Mode::Editor) {
            game.handle_editor_pointer(input);
        } else {
            let (x, y) = game.to_logical(input);
            let action = input.action.as_deref().unwrap_or("");
            match action {
                "down" => {
                    if game.admin && point_in_rect(x, y, edit_button_rect(game.width, game.height))
                    {
                        game.enter_editor();
                    } else {
                        game.paddle_x = x.clamp(
                            MARGIN + 20.0,
                            (game.width - MARGIN - 20.0).max(MARGIN + 20.0),
                        );
                        if game.engaged {
                            game.primary_action();
                        } else {
                            // The first tap or click only wakes the board; Space (or a second tap)
                            // launches the ball.
                            game.engaged = true;
                        }
                    }
                }
                "move" => {
                    game.paddle_x = x.clamp(
                        MARGIN + 20.0,
                        (game.width - MARGIN - 20.0).max(MARGIN + 20.0),
                    );
                }
                "up" | "cancel" => game.fire_held = false,
                _ => {}
            }
        }
    });
    request_frame()
}

impl Game {
    /// Space or a tap: restart when the round is over, launch a waiting ball, or fire while the laser is up.
    fn primary_action(&mut self) {
        if !self.engaged {
            return;
        }
        match self.phase {
            Phase::Over | Phase::Won => {
                if self.playtest {
                    self.exit_to_editor();
                } else {
                    self.restart();
                }
            }
            Phase::Playing => {
                if self.awaiting_launch {
                    self.awaiting_launch = false;
                    self.launch_stuck();
                    return;
                }
                self.fire_held = true;
                if self.shots > 0 && self.fire_cooldown == 0.0 {
                    self.fire();
                }
            }
            Phase::Clearing(_) => {}
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Level editor
    // -----------------------------------------------------------------------------------------------------------

    fn draw_editor(&self, canvas: &mut Canvas2D) {
        let width = self.width;
        let height = self.height;
        canvas
            .fill_color(0x060911ff)
            .fill_rect(0.0, 0.0, width, height);

        canvas
            .font("monospace", 11.0, 700.0)
            .text_align(TextAlign::Left)
            .text_baseline(TextBaseline::Top)
            .fill_color(PALETTE[4])
            .fill_text("LEVEL EDITOR", MARGIN, 4.0);
        canvas
            .font("sans-serif", 7.0, 600.0)
            .fill_color(MUTED)
            .fill_text(
                if self.admin { "POSTER" } else { "READ ONLY" },
                MARGIN,
                19.0,
            );
        if let Some(level) = self.levels.get(self.edit_index) {
            canvas
                .font("monospace", 9.0, 700.0)
                .text_align(TextAlign::Right)
                .text_baseline(TextBaseline::Top)
                .fill_color(TEXT)
                .fill_text(
                    &format!(
                        "{}  ·  {}/{}",
                        clip(&level.name, 18),
                        self.edit_index + 1,
                        self.levels.len()
                    ),
                    width - MARGIN,
                    19.0,
                );
        }

        // Level tabs, clear of the host's top-right fullscreen button.
        let count = self.levels.len();
        if count > 0 {
            let count_f = count as f32;
            let gap = 3.0;
            let tab_w = (width - MARGIN * 2.0 - gap * (count_f - 1.0)) / count_f;
            for index in 0..count {
                let x = MARGIN + index as f32 * (tab_w + gap);
                let selected = index == self.edit_index;
                canvas.fill_color(if selected { PALETTE[4] } else { 0x24304cff });
                rounded(canvas, x, 28.0, tab_w, 14.0, 4.0);
                canvas
                    .font("monospace", 9.0, 700.0)
                    .text_align(TextAlign::Center)
                    .text_baseline(TextBaseline::Middle)
                    .fill_color(if selected { 0x0b0e15ff } else { TEXT })
                    .fill_text(&format!("{}", index + 1), x + tab_w / 2.0, 35.0);
            }
        }

        // The grid, drawn exactly as the bricks appear in play.
        let (gx, gy, _, _) = editor_grid_rect(width);
        let (cw, ch) = editor_cell_size(width);
        let level = self.levels.get(self.edit_index);
        for row in 0..MAX_ROWS {
            for col in 0..COLS {
                let x = gx + col as f32 * (cw + GAP);
                let y = gy + row as f32 * (ch + GAP);
                let has_row = level.map(|level| row < level.rows.len()).unwrap_or(false);
                let style = level
                    .and_then(|level| level.rows.get(row))
                    .and_then(|line| line.chars().nth(col))
                    .and_then(index_of)
                    .and_then(|index| level.and_then(|level| level.styles.get(index).copied()));
                if let Some(style) = style {
                    let hp = if style.solid { 1 } else { style.hp };
                    draw_brick(
                        canvas,
                        x,
                        y,
                        cw,
                        ch,
                        style_rgba(&style),
                        hp,
                        hp,
                        style.solid,
                    );
                } else if has_row {
                    canvas.set_alpha(0.35).fill_color(0x141c2eff);
                    rounded(canvas, x, y, cw, ch, 1.5);
                    canvas.set_alpha(1.0);
                }
            }
        }

        // The cursor.
        let (ccol, crow) = self.cursor;
        if ccol >= 0 && crow >= 0 && (ccol as usize) < COLS && (crow as usize) < MAX_ROWS {
            let x = gx + ccol as f32 * (cw + GAP);
            let y = gy + crow as f32 * (ch + GAP);
            canvas.line_width(1.6).stroke_color(WHITE).stroke_rect(
                x - 0.5,
                y - 0.5,
                cw + 1.0,
                ch + 1.0,
            );
        }

        // The colour field: hue across, shade down. Any cell here is a colour the editor can paint.
        let (fx, fy, fw, fh) = editor_colour_rect(width);
        let fcw = fw / COLOUR_COLS as f32;
        let fch = fh / COLOUR_ROWS as f32;
        for row in 0..COLOUR_ROWS {
            for col in 0..COLOUR_COLS {
                let color = colour_at(col, row);
                canvas.fill_color((color << 8) | 0xff);
                canvas.fill_rect(
                    fx + col as f32 * fcw,
                    fy + row as f32 * fch,
                    fcw + 0.5,
                    fch + 0.5,
                );
            }
        }
        canvas
            .line_width(1.0)
            .stroke_color(with_alpha(0x00000099, 140))
            .stroke_rect(fx + 0.5, fy + 0.5, fw - 1.0, fh - 1.0);
        for row in 0..COLOUR_ROWS {
            for col in 0..COLOUR_COLS {
                if colour_at(col, row) == self.brush.color {
                    canvas.line_width(2.0).stroke_color(WHITE).stroke_rect(
                        fx + col as f32 * fcw - 1.0,
                        fy + row as f32 * fch - 1.0,
                        fcw + 2.0,
                        fch + 2.0,
                    );
                    break;
                }
            }
        }

        // The strength buttons, using the current colour.
        for (index, rect) in editor_strength_rects(width).iter().enumerate() {
            let (hp, solid, label) = STRENGTHS[index];
            let color = if solid { 0x6b7793 } else { self.brush.color };
            canvas.fill_color((color << 8) | 0xff);
            rounded(canvas, rect.0, rect.1, rect.2, rect.3, 3.0);
            if !self.erasing && self.brush.hp == hp && self.brush.solid == solid {
                canvas.line_width(2.0).stroke_color(WHITE).stroke_rect(
                    rect.0 - 1.0,
                    rect.1 - 1.0,
                    rect.2 + 2.0,
                    rect.3 + 2.0,
                );
            }
            canvas
                .font("monospace", 7.0, 700.0)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .fill_color(0x0b0e15ff)
                .fill_text(label, rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0);
        }

        // The level's styles, plus the eraser first.
        let palette_count = self
            .levels
            .get(self.edit_index)
            .map(|level| level.styles.len() + 1)
            .unwrap_or(1);
        let palette = editor_palette_rects(width, palette_count);
        for (index, rect) in palette.iter().enumerate() {
            if index == 0 {
                canvas.fill_color(0x141c2eff);
                rounded(canvas, rect.0, rect.1, rect.2, rect.3, 3.0);
                if self.erasing {
                    canvas.line_width(2.0).stroke_color(WHITE).stroke_rect(
                        rect.0 - 1.0,
                        rect.1 - 1.0,
                        rect.2 + 2.0,
                        rect.3 + 2.0,
                    );
                }
                canvas
                    .font("monospace", 6.0, 700.0)
                    .text_align(TextAlign::Center)
                    .text_baseline(TextBaseline::Middle)
                    .fill_color(TEXT)
                    .fill_text("ERASE", rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0);
                continue;
            }
            let Some(style) = self
                .levels
                .get(self.edit_index)
                .and_then(|level| level.styles.get(index - 1))
            else {
                continue;
            };
            let hp = if style.solid { 1 } else { style.hp };
            draw_brick(
                canvas,
                rect.0,
                rect.1,
                rect.2,
                rect.3,
                style_rgba(style),
                hp,
                hp,
                style.solid,
            );
            if !self.erasing && self.brush == *style {
                canvas.line_width(2.0).stroke_color(WHITE).stroke_rect(
                    rect.0 - 1.0,
                    rect.1 - 1.0,
                    rect.2 + 2.0,
                    rect.3 + 2.0,
                );
            }
        }

        // The action buttons.
        for (label, _, bx, by, bw, bh) in editor_button_rects(width) {
            canvas.fill_color(0x1d2740ff);
            rounded(canvas, bx, by, bw, bh, 4.0);
            canvas
                .line_width(1.0)
                .stroke_color(with_alpha(PALETTE[4], 130))
                .stroke_rect(bx + 0.5, by + 0.5, bw - 1.0, bh - 1.0);
            canvas
                .font("monospace", 7.0, 700.0)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .fill_color(TEXT)
                .fill_text(label, bx + bw / 2.0, by + bh / 2.0);
        }

        let status = self
            .status
            .as_deref()
            .unwrap_or("Tap to paint · pick a colour · arrows move · Esc plays");
        canvas
            .font("sans-serif", 8.0, 500.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Bottom)
            .fill_color(MUTED)
            .fill_text(status, width / 2.0, height - 3.0);

        if self.renaming {
            self.draw_rename(canvas, width);
        }
    }

    fn draw_rename(&self, canvas: &mut Canvas2D, width: f32) {
        let (px, py, pw, ph) = rename_panel_rect(width);
        canvas
            .set_alpha(0.82)
            .fill_color(0x05070eff)
            .fill_rect(0.0, 0.0, width, self.height)
            .set_alpha(1.0);
        canvas.fill_color(0x121a2bff);
        rounded(canvas, px, py, pw, ph, 8.0);
        canvas
            .line_width(1.0)
            .stroke_color(with_alpha(PALETTE[4], 190))
            .stroke_rect(px + 0.5, py + 0.5, pw - 1.0, ph - 1.0);
        canvas
            .font("sans-serif", 8.0, 600.0)
            .text_align(TextAlign::Left)
            .text_baseline(TextBaseline::Top)
            .fill_color(MUTED)
            .fill_text("LEVEL NAME", px + 10.0, py + 8.0);
        let caret = if (self.time * 2.0) as i32 % 2 == 0 {
            "_"
        } else {
            " "
        };
        canvas
            .font("monospace", 13.0, 700.0)
            .fill_color(TEXT)
            .fill_text(
                &format!("{}{caret}", self.name_buffer),
                px + 10.0,
                py + 20.0,
            );

        for (index, rect) in rename_key_rects(width).iter().enumerate() {
            let token = RENAME_KEYS[index];
            let special = token.len() > 1;
            let color = if token == "OK" {
                PALETTE[3]
            } else if special {
                0x24304cff
            } else {
                0x1d2740ff
            };
            canvas.fill_color(color);
            rounded(canvas, rect.0, rect.1, rect.2, rect.3, 3.0);
            canvas
                .font("monospace", if special { 8.0 } else { 12.0 }, 700.0)
                .text_align(TextAlign::Center)
                .text_baseline(TextBaseline::Middle)
                .fill_color(if token == "OK" { 0x0b0e15ff } else { TEXT })
                .fill_text(
                    if index == RENAME_KEYS.len() - 1 {
                        "ESC"
                    } else {
                        token
                    },
                    rect.0 + rect.2 / 2.0,
                    rect.1 + rect.3 / 2.0,
                );
        }
        canvas
            .font("sans-serif", 8.0, 500.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Bottom)
            .fill_color(MUTED)
            .fill_text(
                "Type on your keyboard or tap the keys · Enter saves",
                px + pw / 2.0,
                py + ph - 5.0,
            );
    }

    fn run_editor_action(&mut self, action: EditorAction) {
        match action {
            EditorAction::Previous => self.previous_level(),
            EditorAction::Next => self.next_level(),
            EditorAction::New => self.new_level(),
            EditorAction::AddRow => {
                let rows = self
                    .levels
                    .get(self.edit_index)
                    .map(|level| level.rows.len() + 1)
                    .unwrap_or(1);
                self.resize_level_rows(rows);
            }
            EditorAction::RemoveRow => {
                let rows = self
                    .levels
                    .get(self.edit_index)
                    .map(|level| level.rows.len().saturating_sub(1))
                    .unwrap_or(1);
                self.resize_level_rows(rows);
            }
            EditorAction::Defaults => self.reset_levels(),
            EditorAction::Save => self.save_levels(),
            EditorAction::Playtest => self.playtest(),
            EditorAction::Play => self.leave_editor_to_play(),
            EditorAction::Rename => self.start_rename(),
        }
    }

    fn start_rename(&mut self) {
        self.name_buffer = self
            .levels
            .get(self.edit_index)
            .map(|level| level.name.clone())
            .unwrap_or_default();
        self.renaming = true;
        self.pointer_down = false;
    }

    fn commit_rename(&mut self) {
        let name = self.name_buffer.trim().to_string();
        if name.is_empty() {
            self.set_status("A level needs a name".to_string());
            return;
        }
        if let Some(level) = self.levels.get_mut(self.edit_index) {
            level.name = name;
        }
        self.renaming = false;
        self.set_status("Renamed — SAVE stores it".to_string());
    }

    fn cancel_rename(&mut self) {
        self.renaming = false;
        self.set_status("Rename cancelled".to_string());
    }

    fn rename_push(&mut self, text: &str) {
        if self.name_buffer.chars().count() + text.chars().count() <= MAX_NAME {
            self.name_buffer.push_str(text);
        }
    }

    fn handle_rename_key(&mut self, key: &str) {
        match key {
            "Enter" => self.commit_rename(),
            "Escape" => self.cancel_rename(),
            "Backspace" | "Delete" => {
                self.name_buffer.pop();
            }
            " " | "Space" => self.rename_push(" "),
            _ => {
                let mut chars = key.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if !c.is_control() {
                        let upper = c.to_ascii_uppercase().to_string();
                        self.rename_push(&upper);
                    }
                }
            }
        }
    }

    fn handle_rename_pointer(&mut self, x: f32, y: f32) {
        for (index, rect) in rename_key_rects(self.width).iter().enumerate() {
            if !point_in_rect(x, y, *rect) {
                continue;
            }
            if index == RENAME_KEYS.len() - 1 {
                self.cancel_rename();
                return;
            }
            match RENAME_KEYS[index] {
                "DEL" => {
                    self.name_buffer.pop();
                }
                "SP" => self.rename_push(" "),
                "OK" => self.commit_rename(),
                token => self.rename_push(token),
            }
            return;
        }
    }

    fn handle_editor_pointer(&mut self, event: &CanvasEventInput) {
        let (x, y) = self.to_logical(event);
        let action = event.action.as_deref().unwrap_or("");
        if self.renaming {
            if action == "down" {
                self.handle_rename_pointer(x, y);
            }
            self.pointer_down = false;
            return;
        }
        match action {
            "down" => self.pointer_down = true,
            "up" | "cancel" => {
                self.pointer_down = false;
                return;
            }
            "move" => {}
            _ => return,
        }

        let (gx, gy, gw, gh) = editor_grid_rect(self.width);
        if x >= gx && x <= gx + gw && y >= gy && y <= gy + gh {
            let (cw, ch) = editor_cell_size(self.width);
            let col = ((x - gx) / (cw + GAP)).floor() as i32;
            let row = ((y - gy) / (ch + GAP)).floor() as i32;
            if col >= 0 && row >= 0 && (col as usize) < COLS && (row as usize) < MAX_ROWS {
                self.cursor = (col, row);
                if self.pointer_down {
                    self.paint(col as usize, row as usize);
                }
            }
            return;
        }

        if !self.pointer_down {
            // Only act on a fresh press for the controls.
            return;
        }
        self.pointer_down = false;

        // The colour field.
        let (fx, fy, fw, fh) = editor_colour_rect(self.width);
        if point_in_rect(x, y, (fx, fy, fw, fh)) {
            let col = ((x - fx) / (fw / COLOUR_COLS as f32)).floor() as usize;
            let row = ((y - fy) / (fh / COLOUR_ROWS as f32)).floor() as usize;
            if col < COLOUR_COLS && row < COLOUR_ROWS {
                self.brush.color = colour_at(col, row);
                self.erasing = false;
            }
            return;
        }

        // The strength buttons.
        for (index, rect) in editor_strength_rects(self.width).iter().enumerate() {
            if point_in_rect(x, y, *rect) {
                let (hp, solid, _) = STRENGTHS[index];
                self.brush.hp = hp;
                self.brush.solid = solid;
                self.erasing = false;
                return;
            }
        }

        // The level's styles, with the eraser first.
        let palette_count = self
            .levels
            .get(self.edit_index)
            .map(|level| level.styles.len() + 1)
            .unwrap_or(1);
        for (index, rect) in editor_palette_rects(self.width, palette_count)
            .iter()
            .enumerate()
        {
            if point_in_rect(x, y, *rect) {
                if index == 0 {
                    self.erasing = true;
                } else if let Some(style) = self
                    .levels
                    .get(self.edit_index)
                    .and_then(|level| level.styles.get(index - 1))
                    .copied()
                {
                    self.brush = style;
                    self.erasing = false;
                }
                return;
            }
        }

        for (_, action, bx, by, bw, bh) in editor_button_rects(self.width) {
            if point_in_rect(x, y, (bx, by, bw, bh)) {
                self.run_editor_action(action);
                return;
            }
        }
        let count = self.levels.len();
        if count > 0 && (28.0..=42.0).contains(&y) {
            let count_f = count as f32;
            let gap = 3.0;
            let tab_w = (self.width - MARGIN * 2.0 - gap * (count_f - 1.0)) / count_f;
            for index in 0..count {
                let tx = MARGIN + index as f32 * (tab_w + gap);
                if x >= tx && x <= tx + tab_w {
                    self.select_level(index);
                    return;
                }
            }
        }
    }

    fn handle_editor_key(&mut self, event: &CanvasEventInput) {
        if event.action.as_deref() != Some("down") {
            return;
        }
        let key = event.key.as_deref().unwrap_or("");
        if self.renaming {
            self.handle_rename_key(key);
            return;
        }
        match key {
            "ArrowLeft" => self.cursor.0 = (self.cursor.0 - 1).max(0),
            "ArrowRight" => self.cursor.0 = (self.cursor.0 + 1).min(COLS as i32 - 1),
            "ArrowUp" => self.cursor.1 = (self.cursor.1 - 1).max(0),
            "ArrowDown" => self.cursor.1 = (self.cursor.1 + 1).min(MAX_ROWS as i32 - 1),
            "1" => {
                self.brush.hp = 1;
                self.brush.solid = false;
                self.erasing = false;
            }
            "2" => {
                self.brush.hp = 2;
                self.brush.solid = false;
                self.erasing = false;
            }
            "3" => {
                self.brush.hp = 3;
                self.brush.solid = false;
                self.erasing = false;
            }
            "x" | "X" => {
                self.brush.solid = true;
                self.erasing = false;
            }
            "0" | "." | "Delete" | "Backspace" => self.erasing = true,
            " " | "Space" | "Enter" => {
                let (col, row) = (self.cursor.0 as usize, self.cursor.1 as usize);
                self.paint(col, row);
            }
            "[" => self.previous_level(),
            "]" => self.next_level(),
            "s" | "S" => self.save_levels(),
            "p" | "P" => self.playtest(),
            "n" | "N" => self.new_level(),
            "Escape" | "e" | "E" => self.leave_editor_to_play(),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Host glue
// ---------------------------------------------------------------------------------------------------------------

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

fn reply(result: Result<(), Failed>) -> CanvasReply {
    CanvasReply {
        error: result.err().map(|error| HookError {
            code: error.code,
            message: error.message,
        }),
    }
}

#[cfg(target_arch = "wasm32")]
fn code_of<T: serde::Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "host".to_string())
}

#[cfg(target_arch = "wasm32")]
fn checked(error: Option<CanvasError>) -> Result<(), Failed> {
    match error {
        Some(error) => Err(Failed::new(&code_of(&error.code), error.message)),
        None => Ok(()),
    }
}

#[cfg(target_arch = "wasm32")]
fn host_failed(error: Error) -> Failed {
    Failed::new("host", error.to_string())
}

// The host calls only exist on wasm. Natively the same functions are no-ops, so the game's logic can be unit-tested
// on the host without the Extism imports.
#[cfg(target_arch = "wasm32")]
fn set_fps(fps: u32) -> Result<(), Failed> {
    chittr_cartridge::canvas::set_fps(fps).map_err(Failed::from)
}

#[cfg(not(target_arch = "wasm32"))]
fn set_fps(_fps: u32) -> Result<(), Failed> {
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn request_frame() -> Result<(), Failed> {
    // The SDK sends the required empty JSON envelope for this no-field host call.
    chittr_cartridge::canvas::request_frame().map_err(Failed::from)
}

#[cfg(not(target_arch = "wasm32"))]
fn request_frame() -> Result<(), Failed> {
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn send_message(message: Map<String, Value>, seq: i32) -> Result<(), Failed> {
    checked(
        pdk::send(CanvasSendInput {
            message,
            seq: Some(seq),
        })
        .map_err(host_failed)?
        .error,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn send_message(_message: Map<String, Value>, _seq: i32) -> Result<(), Failed> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_level_is_thirty_two_columns_of_known_cells() {
        for level in default_levels() {
            assert!(level_is_playable(&level), "{} is not playable", level.name);
            for row in &level.rows {
                assert_eq!(row.chars().count(), COLS, "row {row:?} is not {COLS} wide");
            }
        }
    }

    #[test]
    fn levels_build_the_expected_bricks_and_geometry() {
        let mut game = Game {
            width: LOGICAL_W,
            height: LOGICAL_H,
            ..Game::default()
        };
        game.load_level(0);
        assert_eq!(game.balls.len(), 1);
        assert!(game.bricks.iter().all(|brick| !brick.solid));
        assert!(
            game.bricks.len() >= 300,
            "a 32-wide board holds a lot of bricks"
        );
        assert!(game.brick_w() > 8.0, "cells stay big enough to hit");
        // The first level is all single-hit, but the diamond has tougher bricks.
        assert!(game.bricks.iter().all(|brick| brick.max_hp == 1));
        let diamond = default_levels()
            .into_iter()
            .find(|level| level.name == "DIAMOND")
            .expect("the diamond level exists");
        assert!(diamond
            .styles
            .iter()
            .any(|style| !style.solid && style.hp == 2));
    }

    #[test]
    fn solid_bricks_survive_hits_and_hide_behind_no_completion() {
        let mut game = Game::default();
        game.load_level(2);
        let solid = game
            .bricks
            .iter()
            .position(|brick| brick.solid)
            .expect("level 3 has solid bricks");
        game.hit_brick(solid, 0.0, 0.0);
        assert!(game.bricks[solid].alive);
        assert!(game.bricks.iter().any(|brick| brick.alive && !brick.solid));
    }

    #[test]
    fn powerups_fit_inside_the_playfield_after_a_wide_paddle_pickup() {
        let mut game = Game::default();
        game.load_level(0);
        game.apply_powerup(PowerKind::Expand);
        assert!(game.expand_timer > 0.0);
        game.update_paddle(1.0 / 60.0);
        let half = game.paddle_w / 2.0;
        assert!(game.paddle_x - half >= MARGIN - 0.01);
        assert!(game.paddle_x + half <= game.width - MARGIN + 0.01);
    }

    #[test]
    fn multiball_respects_the_cap() {
        let mut game = Game::default();
        game.load_level(0);
        for _ in 0..5 {
            game.apply_powerup(PowerKind::MultiBall);
        }
        assert!(game.balls.len() <= MAX_BALLS);
        assert!(game.balls.iter().all(|ball| !ball.stuck));
    }

    #[test]
    fn slow_motion_scales_the_ball_without_changing_its_speed() {
        let mut game = Game::default();
        game.load_level(0);
        game.slow_timer = 5.0;
        assert!(game.speed_scale() < 1.0);
        game.slow_timer = 0.0;
        assert_eq!(game.speed_scale(), 1.0);
    }

    #[test]
    fn laser_shots_stack_and_are_spent_one_volley_at_a_time() {
        let mut game = Game::default();
        game.load_level(0);
        game.apply_powerup(PowerKind::Laser);
        assert_eq!(game.shots, 15);
        game.apply_powerup(PowerKind::Laser);
        assert_eq!(game.shots, 30, "a second pickup must add to the ammo");

        game.fire();
        assert_eq!(game.shots, 29);
        assert_eq!(game.bullets.len(), 2);

        game.shots = 0;
        game.fire();
        assert_eq!(game.shots, 0);
        assert_eq!(game.bullets.len(), 2, "firing with no ammo does nothing");
    }

    #[test]
    fn clearing_a_level_advances_to_the_next_instead_of_ending_the_run() {
        let mut game = Game::default();
        game.load_level(0);
        game.engaged = true;
        for brick in &mut game.bricks {
            if !brick.solid {
                brick.alive = false;
            }
        }
        // Run through the clear-level pause and into the next level.
        for _ in 0..200 {
            game.update(0.05);
            if game.level == 1 && matches!(game.phase, Phase::Playing) {
                break;
            }
        }
        assert_eq!(game.level, 1, "the first clear should load level two");
        assert!(matches!(game.phase, Phase::Playing));
        assert!(!game.balls.is_empty());
        assert!(
            game.bricks.iter().any(|brick| brick.alive),
            "level two should have bricks"
        );
    }

    #[test]
    fn the_final_level_wins_the_run() {
        let mut game = Game::default();
        game.load_level(game.levels.len() - 1);
        game.engaged = true;
        for brick in &mut game.bricks {
            if !brick.solid {
                brick.alive = false;
            }
        }
        for _ in 0..200 {
            game.update(0.05);
            if matches!(game.phase, Phase::Won) {
                break;
            }
        }
        assert!(matches!(game.phase, Phase::Won));
    }

    #[test]
    fn the_board_waits_for_a_click_and_then_space() {
        let mut game = Game::default();
        game.load_level(0);
        assert!(!game.engaged);
        // Nothing happens, and the ball stays on the paddle, until the first click.
        for _ in 0..60 {
            game.update(0.05);
        }
        assert!(game.awaiting_launch);
        assert!(game.balls.iter().all(|ball| ball.stuck));
        // Space before the first click is ignored.
        game.primary_action();
        assert!(game.awaiting_launch);
        // After the click, Space launches.
        game.engaged = true;
        game.primary_action();
        assert!(!game.awaiting_launch);
        assert!(game.balls.iter().all(|ball| !ball.stuck));
    }

    #[test]
    fn level_is_playable_matches_the_editor_limits() {
        assert!(level_is_playable(&level_from_rows(
            "A",
            vec!["r".repeat(COLS)]
        )));
        assert!(!level_is_playable(&level_from_rows(
            "A",
            vec![".".repeat(COLS)]
        )));
        assert!(!level_is_playable(&level_from_rows(
            "A",
            vec!["X".repeat(COLS)]
        )));
        assert!(!level_is_playable(&level_from_rows(
            "A",
            vec!["r".repeat(COLS); MAX_ROWS + 1]
        )));
    }

    #[test]
    fn editor_paints_the_chosen_colour_and_can_erase() {
        let mut game = Game::default();
        game.enter_editor();
        game.erasing = false;
        game.brush = Style {
            hp: 3,
            solid: false,
            color: 0x123456,
        };
        game.paint(0, 0);
        let cell = game.levels[0].rows[0].chars().next().unwrap();
        let index = index_of(cell).expect("the painted cell indexes a style");
        assert_eq!(game.levels[0].styles[index].color, 0x123456);
        assert_eq!(game.levels[0].styles[index].hp, 3);

        game.erasing = true;
        game.paint(0, 0);
        assert_eq!(game.levels[0].rows[0].chars().next(), Some('.'));

        let before = game.levels[0].clone();
        game.paint(COLS + 4, 99);
        assert_eq!(game.levels[0], before, "out-of-range paints are ignored");
    }

    #[test]
    fn the_editor_can_add_and_resize_levels() {
        let mut game = Game::default();
        let before = game.levels.len();
        game.new_level();
        assert_eq!(game.levels.len(), before + 1);
        assert_eq!(game.edit_index, before);
        game.resize_level_rows(MAX_ROWS);
        assert_eq!(game.levels[before].rows.len(), MAX_ROWS);
        game.resize_level_rows(0);
        assert_eq!(game.levels[before].rows.len(), 1, "rows clamp to one");
    }

    #[test]
    fn the_editor_renames_a_level_from_the_keyboard_and_the_keypad() {
        let mut game = Game::default();
        game.enter_editor();
        game.start_rename();
        assert!(game.renaming);
        game.name_buffer.clear();
        for key in ["N", "E", "W"] {
            game.handle_rename_key(key);
        }
        game.handle_rename_key(" ");
        game.handle_rename_key("Z");
        game.commit_rename();
        assert!(!game.renaming);
        assert_eq!(game.levels[0].name, "NEW Z");

        // A long name is capped at the limit.
        game.start_rename();
        game.name_buffer.clear();
        for _ in 0..(MAX_NAME + 10) {
            game.handle_rename_key("A");
        }
        assert_eq!(game.name_buffer.chars().count(), MAX_NAME);

        // An empty name is refused, and cancelling keeps the old one.
        game.name_buffer = "   ".into();
        game.commit_rename();
        assert!(game.renaming);
        game.cancel_rename();
        assert!(!game.renaming);
        assert_eq!(game.levels[0].name, "NEW Z");

        // The on-screen keypad works too.
        game.start_rename();
        game.name_buffer.clear();
        let rects = rename_key_rects(game.width);
        let key = rects[0];
        game.handle_rename_pointer(key.0 + key.2 / 2.0, key.1 + key.3 / 2.0);
        assert_eq!(game.name_buffer, "A");
    }

    #[test]
    fn rename_keypad_and_buttons_are_laid_out_on_screen() {
        let keys = rename_key_rects(LOGICAL_W);
        assert_eq!(keys.len(), RENAME_KEYS.len());
        for rect in &keys {
            assert!(rect.0 >= 0.0 && rect.0 + rect.2 <= LOGICAL_W + 0.01);
            assert!(rect.1 >= 0.0 && rect.1 + rect.3 <= LOGICAL_H + 0.01);
        }
        assert!(editor_button_rects(LOGICAL_W)
            .iter()
            .any(|(label, _, _, _, _, _)| *label == "RENAME"));
    }

    #[test]
    fn playtest_uses_only_the_edited_level_and_returns_to_the_editor() {
        let mut game = Game::default();
        game.enter_editor();
        game.playtest();
        game.engaged = true;
        assert!(game.playtest);
        assert_eq!(game.active_levels().len(), 1);
        assert!(matches!(game.mode, Mode::Game));
        for brick in &mut game.bricks {
            if !brick.solid {
                brick.alive = false;
            }
        }
        for _ in 0..200 {
            game.update(0.05);
            if matches!(game.mode, Mode::Editor) {
                break;
            }
        }
        assert!(matches!(game.mode, Mode::Editor));
        assert!(!game.playtest);
    }

    #[test]
    fn apply_state_keeps_the_admin_flag_and_only_playable_levels() {
        let mut game = Game::default();
        let state = json!({
            "admin": true,
            "levels": [
                { "name": "OK", "styles": [{ "hp": 1, "solid": false, "color": 255 }], "rows": ["0".repeat(COLS)] },
                { "name": "BAD", "styles": [{ "hp": 1, "solid": false, "color": 255 }], "rows": ["0"] },
                { "name": "SOLID", "styles": [{ "hp": 1, "solid": true, "color": 255 }], "rows": ["0".repeat(COLS)] }
            ]
        });
        let map = state.as_object().unwrap().clone();
        game.apply_state(Some(&map));
        assert!(game.admin);
        assert_eq!(game.levels.len(), 1);
        assert_eq!(game.levels[0].name, "OK");
    }

    #[test]
    fn fullscreen_view_letterboxes_and_maps_pointer_coordinates() {
        let mut game = Game {
            view_w: 1680.0,
            view_h: 1050.0,
            ..Game::default()
        };
        game.refresh_view();
        // A 4:3 playfield in a 16:10 viewport fits by height, leaving bars left and right.
        assert!(game.scale > 0.0);
        assert!(game.off_x > 0.0);
        assert!(game.off_y.abs() < 0.01);
        let event = CanvasEventInput {
            r#type: CanvasEventType::Pointer,
            x: Some((game.view_w / 2.0) as f64),
            y: Some((game.view_h / 2.0) as f64),
            width: Some(game.view_w as f64),
            height: Some(game.view_h as f64),
            ..Default::default()
        };
        let (lx, ly) = game.to_logical(&event);
        assert!((lx - game.width / 2.0).abs() < 1.0);
        assert!((ly - game.height / 2.0).abs() < 1.0);
    }
}
