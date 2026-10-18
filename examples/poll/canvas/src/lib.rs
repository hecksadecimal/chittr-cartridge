//! Canvas Guest for the poll's transparent, clickable answer buttons. The current question, choices and tallies come
//! from the Server Guest's `render` state. A click sends its option id to `on_canvas_message`, where the server
//! validates the answer and records the vote.

mod pdk;

use std::cell::RefCell;

use chittr_cartridge::canvas::{
    request_frame, send, set_fps, Canvas2D, CanvasError as SdkCanvasError, TextAlign, TextBaseline,
};
use extism_pdk::Error;
use pdk::types::*;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const TEXT: u32 = 0xe5e7_ebff;
const MUTED: u32 = 0x9ca3_afff;
const OUTLINE: u32 = 0x7b8794ff;
const SELECTED: u32 = 0xc5a16aff;
const BAR_COLOR: u32 = 0x7b8794ff;
const WINNER_COLOR: u32 = 0xf0bd55ff;
const BAR_ANIMATION_MS: f64 = 450.0;
const BUTTON_HEIGHT_CSS: f32 = 56.0;
const BUTTON_GAP_CSS: f32 = 8.0;
const BUTTON_MARGIN_X_CSS: f32 = 12.0;
const BUTTON_MARGIN_Y_CSS: f32 = 16.0;
const LABEL_FONT_CSS: f32 = 28.0;
const TALLY_FONT_CSS: f32 = 16.0;
const MAX_LABEL_CHARS: usize = 14;

#[derive(Debug, Clone, Deserialize, Default)]
struct PollOption {
    id: String,
    label: String,
    votes: i64,
    percent: i64,
    voted: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PollState {
    options: Vec<PollOption>,
    signed_in: bool,
    closed: bool,
}

#[derive(Default)]
struct Button {
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Default)]
struct State {
    width: f32,
    height: f32,
    dpr: f32,
    options: Vec<PollOption>,
    bar_from: Vec<f32>,
    animation_started: Option<f64>,
    animating: bool,
    signed_in: bool,
    closed: bool,
    buttons: Vec<Button>,
    seq: u32,
    dirty: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn reply(error: Option<CanvasError>) -> CanvasReply {
    CanvasReply {
        error: error.map(|error| HookError {
            code: serde_json::to_value(error.code)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "host".to_string()),
            message: error.message,
        }),
    }
}

fn guest_error(error: SdkCanvasError) -> CanvasError {
    CanvasError {
        code: serde_json::from_value(json!(error.code)).unwrap_or(CanvasErrorCode::Host),
        message: error.message,
    }
}

fn read_state(state: Option<&Map<String, Value>>) -> PollState {
    state
        .and_then(|state| serde_json::from_value(Value::Object(state.clone())).ok())
        .unwrap_or_default()
}

fn apply_state(state: &mut State, input: &PollState, animate: bool) {
    let mut changed = false;
    state.bar_from = input
        .options
        .iter()
        .map(
            |option| match state.options.iter().find(|old| old.id == option.id) {
                Some(old) => {
                    changed |= old.percent != option.percent;
                    old.percent as f32
                }
                None => option.percent as f32,
            },
        )
        .collect();
    state.options = input.options.clone();
    state.signed_in = input.signed_in;
    state.closed = input.closed;
    state.animation_started = None;
    state.animating = animate && changed;
    state.dirty = true;
}

pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.width = input.width as f32;
        state.height = input.height as f32;
        state.dpr = input.dpr as f32;
        apply_state(&mut state, &read_state(input.state.as_ref()), false);
    });
    let result = set_fps(30)
        .map_err(guest_error)
        .and_then(|()| STATE.with(|state| draw(&mut state.borrow_mut(), None)));
    Ok(reply(result.err()))
}

pub(crate) fn frame(input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    Ok(reply(draw_current(Some(input.t)).err()))
}

pub(crate) fn event(input: CanvasEventInput) -> Result<CanvasReply, Error> {
    let result = match input.r#type {
        CanvasEventType::State => {
            STATE.with(|state| {
                apply_state(
                    &mut state.borrow_mut(),
                    &read_state(input.state.as_ref()),
                    true,
                )
            });
            draw_current(None)
        }
        CanvasEventType::Resize => {
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.width = input.width.unwrap_or(state.width as f64) as f32;
                state.height = input.height.unwrap_or(state.height as f64) as f32;
                state.animating = false;
                state.animation_started = None;
                state.bar_from = state
                    .options
                    .iter()
                    .map(|option| option.percent as f32)
                    .collect();
                state.dirty = true;
            });
            draw_current(None)
        }
        CanvasEventType::Visibility => draw_current(None),
        CanvasEventType::Pointer => pointer(&input),
        CanvasEventType::Reply | CanvasEventType::Error => Ok(()),
        _ => Ok(()),
    };
    Ok(reply(result.err()))
}

fn draw_current(time: Option<f64>) -> Result<(), CanvasError> {
    let (result, animating) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let result = draw(&mut state, time);
        (result, state.animating)
    });
    result?;
    continue_animation(animating)
}

fn continue_animation(animating: bool) -> Result<(), CanvasError> {
    if !animating || request_frame().is_ok() {
        return Ok(());
    }
    // If the host declines another frame, settle at the latest percentages instead of trapping.
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.animating = false;
        state.animation_started = None;
        state.bar_from = state
            .options
            .iter()
            .map(|option| option.percent as f32)
            .collect();
        state.dirty = true;
        draw(&mut state, None)
    })
}

fn pointer(input: &CanvasEventInput) -> Result<(), CanvasError> {
    if input.action.as_deref() != Some("up") {
        return Ok(());
    }
    let hit = STATE.with(|state| {
        let state = state.borrow();
        if !state.signed_in || state.closed {
            return None;
        }
        let css_width = input.width.unwrap_or(state.width as f64) as f32;
        let css_height = input.height.unwrap_or(state.height as f64) as f32;
        let scale_x = if css_width > 0.0 {
            state.width / css_width
        } else {
            1.0
        };
        let scale_y = if css_height > 0.0 {
            state.height / css_height
        } else {
            1.0
        };
        let x = input.x.unwrap_or(0.0) as f32 * scale_x;
        let y = input.y.unwrap_or(0.0) as f32 * scale_y;
        state
            .buttons
            .iter()
            .find(|button| {
                x >= button.x
                    && x < button.x + button.width
                    && y >= button.y
                    && y < button.y + button.height
            })
            .map(|button| button.id.clone())
    });
    let Some(option_id) = hit else {
        return Ok(());
    };
    let (seq, message) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.seq = state.seq.wrapping_add(1).max(1);
        let mut message = Map::new();
        message.insert("optionId".to_string(), json!(option_id));
        (state.seq, message)
    });
    send(Value::Object(message), Some(seq)).map_err(guest_error)
}

fn draw(state: &mut State, time: Option<f64>) -> Result<(), CanvasError> {
    if !state.dirty && !state.animating {
        return Ok(());
    }
    state.dirty = false;
    let width = state.width.max(1.0);
    let height = state.height.max(1.0);
    let progress = if state.animating {
        if let Some(now) = time {
            let started = *state.animation_started.get_or_insert(now);
            let linear = ((now - started) / BAR_ANIMATION_MS).clamp(0.0, 1.0) as f32;
            1.0 - (1.0 - linear).powi(3)
        } else {
            0.0
        }
    } else {
        1.0
    };
    if progress >= 1.0 {
        state.animating = false;
    }

    let mut canvas = Canvas2D::new();
    // Clear to transparent; neither the canvas nor its answer buttons paint an opaque backdrop.
    canvas.clear_rect(0.0, 0.0, width, height);
    state.buttons.clear();

    if state.options.is_empty() {
        canvas
            .fill_color(MUTED)
            .font("Mono", LABEL_FONT_CSS * state.dpr.max(1.0), 400.0)
            .text_align(TextAlign::Center)
            .text_baseline(TextBaseline::Middle)
            .fill_text("No answers are configured", width / 2.0, height / 2.0);
    } else {
        let scale = state.dpr.max(1.0);
        let margin_x = BUTTON_MARGIN_X_CSS * scale;
        let margin_y = BUTTON_MARGIN_Y_CSS * scale;
        let gap = BUTTON_GAP_CSS * scale;
        let count = state.options.len() as f32;
        let available = (height - margin_y * 2.0 - gap * (count - 1.0)).max(1.0);
        let button_height = (BUTTON_HEIGHT_CSS * scale).min(available / count);
        let pitch = button_height + gap;
        let top = ((height - (button_height * count + gap * (count - 1.0))) / 2.0).max(0.0);
        let button_width = (width - margin_x * 2.0).max(1.0);
        let max_votes = state
            .options
            .iter()
            .map(|option| option.votes)
            .max()
            .unwrap_or(0);
        let bar_height = 6.0 * scale;
        for (index, option) in state.options.iter().enumerate() {
            let x = margin_x;
            let y = top + index as f32 * pitch;
            let color = if option.voted { SELECTED } else { OUTLINE };
            canvas
                .stroke_color(color)
                .line_width(if option.voted {
                    2.0 * scale
                } else {
                    1.5 * scale
                })
                .stroke_rect(x, y, button_width, button_height);

            let from = state
                .bar_from
                .get(index)
                .copied()
                .unwrap_or(option.percent as f32);
            let percent = from + (option.percent as f32 - from) * progress;
            let bar_width = (button_width - 2.0 * scale) * percent.clamp(0.0, 100.0) / 100.0;
            if bar_width > 0.0 {
                let bar_color = if max_votes > 0 && option.votes == max_votes {
                    WINNER_COLOR
                } else {
                    BAR_COLOR
                };
                canvas.fill_color(bar_color).fill_rect(
                    x + scale,
                    y + button_height - bar_height - scale,
                    bar_width,
                    bar_height,
                );
            }

            let label = truncate(&option.label, MAX_LABEL_CHARS);
            let text_y = y + (button_height - bar_height) / 2.0;
            canvas
                .fill_color(TEXT)
                .font("Mono", LABEL_FONT_CSS * scale, 500.0)
                .text_align(TextAlign::Left)
                .text_baseline(TextBaseline::Middle)
                .fill_text(&label, x + 14.0 * scale, text_y);
            let votes = format!("{} · {}%", option.votes, option.percent);
            canvas
                .fill_color(MUTED)
                .font("Mono", TALLY_FONT_CSS * scale, 400.0)
                .text_align(TextAlign::Right)
                .text_baseline(TextBaseline::Middle)
                .fill_text(&votes, x + button_width - 14.0 * scale, text_y);
            state.buttons.push(Button {
                id: option.id.clone(),
                x,
                y,
                width: button_width,
                height: button_height,
            });
        }
    }
    canvas.submit().map_err(|error| CanvasError {
        code: CanvasErrorCode::Invalid,
        message: error.to_string(),
    })
}

fn truncate(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    let mut out: String = label.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}
