// The bindings in pdk.rs are generated, and their error enums keep the schema's names.
#![allow(non_camel_case_types)]

mod pdk;

use chittr_cartridge::canvas::{rgba, Canvas2D};
use extism_pdk::Error;
use pdk::types::*;

// Runs once when the canvas starts. Draws the first frame, in backing-store pixels.
pub(crate) fn start(input: CanvasStartInput) -> Result<CanvasReply, Error> {
    Ok(draw(input.width as f32, input.height as f32))
}

// Runs once per frame, and only after the guest called request_frame. The starter has nothing to animate.
pub(crate) fn frame(_input: CanvasFrameInput) -> Result<CanvasReply, Error> {
    Ok(CanvasReply { error: None })
}

// Runs for each input or state event (an optional export). The starter ignores them.
pub(crate) fn event(_input: CanvasEventInput) -> Result<CanvasReply, Error> {
    Ok(CanvasReply { error: None })
}

fn draw(width: f32, height: f32) -> CanvasReply {
    let mut canvas = Canvas2D::new();
    canvas.fill_color(rgba(16, 24, 32, 255)).fill_rect(0.0, 0.0, width, height);
    canvas.fill_color(rgba(245, 158, 11, 255)).fill_rect(width / 4.0, height / 4.0, width / 2.0, height / 2.0);
    match canvas.submit() {
        Ok(()) => CanvasReply { error: None },
        Err(error) => CanvasReply { error: Some(HookError { code: error.code, message: error.message }) },
    }
}
