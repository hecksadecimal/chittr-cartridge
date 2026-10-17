//! The command buffer encoder (ABI `v: 1`, docs/cartridges/v1/abi.md; see docs/cartridges/v1/design.md). Pure: no
//! host calls, so it is tested natively against the golden bytes in tests/fixtures/canvas/command-buffer-v1.bin.
//!
//! Layout: each op is a `u8` opcode then its arguments, little-endian. `f32` for numbers, `u32` for RGBA
//! colours (0xRRGGBBAA) and ids, and `u32` byte length then UTF-8 for strings. The opcode values are the
//! table in abi.md; keep the two in step.

/// Largest buffer one `submit` may carry, in bytes. The host refuses more (abi.md, limits).
pub const SUBMIT_BYTES_MAX: usize = 4 * 1024 * 1024;

/// Composite operations for `set_composite`. The discriminant is the wire value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Composite {
    SourceOver = 0,
    SourceAtop = 1,
    DestinationOver = 2,
    DestinationOut = 3,
    Lighter = 4,
    Multiply = 5,
    Screen = 6,
    Overlay = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FillRule {
    NonZero = 0,
    EvenOdd = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TextAlign {
    Start = 0,
    End = 1,
    Left = 2,
    Right = 3,
    Center = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TextBaseline {
    Top = 0,
    Hanging = 1,
    Middle = 2,
    Alphabetic = 3,
    Ideographic = 4,
    Bottom = 5,
}

/// The opcode values. abi.md lists the same numbers.
pub mod op {
    pub const SAVE: u8 = 0x01;
    pub const RESTORE: u8 = 0x02;
    pub const SET_TRANSFORM: u8 = 0x03;
    pub const TRANSFORM: u8 = 0x04;
    pub const TRANSLATE: u8 = 0x05;
    pub const ROTATE: u8 = 0x06;
    pub const SCALE: u8 = 0x07;
    pub const SET_ALPHA: u8 = 0x08;
    pub const SET_COMPOSITE: u8 = 0x09;
    pub const SET_IMAGE_SMOOTHING: u8 = 0x0a;
    pub const BEGIN_PATH: u8 = 0x10;
    pub const MOVE_TO: u8 = 0x11;
    pub const LINE_TO: u8 = 0x12;
    pub const QUAD_TO: u8 = 0x13;
    pub const BEZIER_TO: u8 = 0x14;
    pub const ARC: u8 = 0x15;
    pub const ARC_TO: u8 = 0x16;
    pub const ELLIPSE: u8 = 0x17;
    pub const RECT: u8 = 0x18;
    pub const ROUND_RECT: u8 = 0x19;
    pub const CLOSE_PATH: u8 = 0x1a;
    pub const FILL: u8 = 0x1b;
    pub const STROKE: u8 = 0x1c;
    pub const CLIP: u8 = 0x1d;
    pub const FILL_RECT: u8 = 0x20;
    pub const STROKE_RECT: u8 = 0x21;
    pub const CLEAR_RECT: u8 = 0x22;
    pub const FILL_COLOR: u8 = 0x30;
    pub const STROKE_COLOR: u8 = 0x31;
    pub const LINE_WIDTH: u8 = 0x32;
    pub const LINE_CAP: u8 = 0x33;
    pub const LINE_JOIN: u8 = 0x34;
    pub const MITER_LIMIT: u8 = 0x35;
    pub const LINE_DASH: u8 = 0x36;
    pub const FILL_GRADIENT: u8 = 0x37;
    pub const STROKE_GRADIENT: u8 = 0x38;
    pub const DEF_LINEAR: u8 = 0x39;
    pub const DEF_RADIAL: u8 = 0x3a;
    pub const FONT: u8 = 0x40;
    pub const TEXT_ALIGN: u8 = 0x41;
    pub const TEXT_BASELINE: u8 = 0x42;
    pub const FILL_TEXT: u8 = 0x43;
    pub const STROKE_TEXT: u8 = 0x44;
    pub const DRAW_IMAGE: u8 = 0x50;
    pub const BLIT_RGBA: u8 = 0x51;
}

/// A colour as the wire carries it: 0xRRGGBBAA.
pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32
}

/// One frame's ops, encoded. `bytes()` is what `submit` sends.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CommandBuffer {
    bytes: Vec<u8>,
}

impl CommandBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    fn op(&mut self, code: u8) -> &mut Self {
        self.bytes.push(code);
        self
    }

    fn u8(&mut self, v: u8) -> &mut Self {
        self.bytes.push(v);
        self
    }

    fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes.extend_from_slice(&v.to_le_bytes());
        self
    }

    fn f32(&mut self, v: f32) -> &mut Self {
        self.bytes.extend_from_slice(&v.to_le_bytes());
        self
    }

    fn f32s(&mut self, values: &[f32]) -> &mut Self {
        for v in values {
            self.f32(*v);
        }
        self
    }

    fn string(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.bytes.extend_from_slice(s.as_bytes());
        self
    }

    fn stops(&mut self, stops: &[(f32, u32)]) -> &mut Self {
        self.u32(stops.len() as u32);
        for (offset, colour) in stops {
            self.f32(*offset).u32(*colour);
        }
        self
    }

    pub fn save(&mut self) -> &mut Self {
        self.op(op::SAVE)
    }

    pub fn restore(&mut self) -> &mut Self {
        self.op(op::RESTORE)
    }

    pub fn set_transform(&mut self, m: [f32; 6]) -> &mut Self {
        self.op(op::SET_TRANSFORM).f32s(&m)
    }

    pub fn transform(&mut self, m: [f32; 6]) -> &mut Self {
        self.op(op::TRANSFORM).f32s(&m)
    }

    pub fn translate(&mut self, x: f32, y: f32) -> &mut Self {
        self.op(op::TRANSLATE).f32(x).f32(y)
    }

    pub fn rotate(&mut self, radians: f32) -> &mut Self {
        self.op(op::ROTATE).f32(radians)
    }

    pub fn scale(&mut self, x: f32, y: f32) -> &mut Self {
        self.op(op::SCALE).f32(x).f32(y)
    }

    pub fn set_alpha(&mut self, alpha: f32) -> &mut Self {
        self.op(op::SET_ALPHA).f32(alpha)
    }

    pub fn set_composite(&mut self, mode: Composite) -> &mut Self {
        self.op(op::SET_COMPOSITE).u8(mode as u8)
    }

    pub fn set_image_smoothing(&mut self, on: bool) -> &mut Self {
        self.op(op::SET_IMAGE_SMOOTHING).u8(on as u8)
    }

    pub fn begin_path(&mut self) -> &mut Self {
        self.op(op::BEGIN_PATH)
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.op(op::MOVE_TO).f32(x).f32(y)
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.op(op::LINE_TO).f32(x).f32(y)
    }

    pub fn quad_to(&mut self, cpx: f32, cpy: f32, x: f32, y: f32) -> &mut Self {
        self.op(op::QUAD_TO).f32s(&[cpx, cpy, x, y])
    }

    pub fn bezier_to(
        &mut self,
        c1x: f32,
        c1y: f32,
        c2x: f32,
        c2y: f32,
        x: f32,
        y: f32,
    ) -> &mut Self {
        self.op(op::BEZIER_TO).f32s(&[c1x, c1y, c2x, c2y, x, y])
    }

    pub fn arc(&mut self, x: f32, y: f32, r: f32, start: f32, end: f32, ccw: bool) -> &mut Self {
        self.op(op::ARC).f32s(&[x, y, r, start, end]).u8(ccw as u8)
    }

    pub fn arc_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, r: f32) -> &mut Self {
        self.op(op::ARC_TO).f32s(&[x1, y1, x2, y2, r])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        &mut self,
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        rotation: f32,
        start: f32,
        end: f32,
        ccw: bool,
    ) -> &mut Self {
        self.op(op::ELLIPSE)
            .f32s(&[x, y, rx, ry, rotation, start, end])
            .u8(ccw as u8)
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.op(op::RECT).f32s(&[x, y, w, h])
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) -> &mut Self {
        self.op(op::ROUND_RECT).f32s(&[x, y, w, h, r])
    }

    pub fn close_path(&mut self) -> &mut Self {
        self.op(op::CLOSE_PATH)
    }

    pub fn fill(&mut self, rule: FillRule) -> &mut Self {
        self.op(op::FILL).u8(rule as u8)
    }

    pub fn stroke(&mut self) -> &mut Self {
        self.op(op::STROKE)
    }

    pub fn clip(&mut self, rule: FillRule) -> &mut Self {
        self.op(op::CLIP).u8(rule as u8)
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.op(op::FILL_RECT).f32s(&[x, y, w, h])
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.op(op::STROKE_RECT).f32s(&[x, y, w, h])
    }

    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.op(op::CLEAR_RECT).f32s(&[x, y, w, h])
    }

    pub fn fill_color(&mut self, rgba: u32) -> &mut Self {
        self.op(op::FILL_COLOR).u32(rgba)
    }

    pub fn stroke_color(&mut self, rgba: u32) -> &mut Self {
        self.op(op::STROKE_COLOR).u32(rgba)
    }

    pub fn line_width(&mut self, width: f32) -> &mut Self {
        self.op(op::LINE_WIDTH).f32(width)
    }

    pub fn line_cap(&mut self, cap: LineCap) -> &mut Self {
        self.op(op::LINE_CAP).u8(cap as u8)
    }

    pub fn line_join(&mut self, join: LineJoin) -> &mut Self {
        self.op(op::LINE_JOIN).u8(join as u8)
    }

    pub fn miter_limit(&mut self, limit: f32) -> &mut Self {
        self.op(op::MITER_LIMIT).f32(limit)
    }

    pub fn line_dash(&mut self, segments: &[f32]) -> &mut Self {
        self.op(op::LINE_DASH)
            .u32(segments.len() as u32)
            .f32s(segments)
    }

    pub fn fill_gradient(&mut self, id: u32) -> &mut Self {
        self.op(op::FILL_GRADIENT).u32(id)
    }

    pub fn stroke_gradient(&mut self, id: u32) -> &mut Self {
        self.op(op::STROKE_GRADIENT).u32(id)
    }

    /// Defines a linear gradient for this submit. `stops` are `(offset, rgba)` pairs.
    pub fn def_linear(
        &mut self,
        id: u32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        stops: &[(f32, u32)],
    ) -> &mut Self {
        self.op(op::DEF_LINEAR)
            .u32(id)
            .f32s(&[x0, y0, x1, y1])
            .stops(stops)
    }

    /// Defines a radial gradient for this submit.
    #[allow(clippy::too_many_arguments)]
    pub fn def_radial(
        &mut self,
        id: u32,
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
        stops: &[(f32, u32)],
    ) -> &mut Self {
        self.op(op::DEF_RADIAL)
            .u32(id)
            .f32s(&[x0, y0, r0, x1, y1, r1])
            .stops(stops)
    }

    /// Sets the font. `family` is a manifest font name, or anything else for sans-serif.
    pub fn font(&mut self, family: &str, size: f32, weight: f32) -> &mut Self {
        self.op(op::FONT).string(family).f32(size).f32(weight)
    }

    pub fn text_align(&mut self, align: TextAlign) -> &mut Self {
        self.op(op::TEXT_ALIGN).u8(align as u8)
    }

    pub fn text_baseline(&mut self, baseline: TextBaseline) -> &mut Self {
        self.op(op::TEXT_BASELINE).u8(baseline as u8)
    }

    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) -> &mut Self {
        self.op(op::FILL_TEXT).f32(x).f32(y).string(text)
    }

    pub fn stroke_text(&mut self, text: &str, x: f32, y: f32) -> &mut Self {
        self.op(op::STROKE_TEXT).f32(x).f32(y).string(text)
    }

    /// Draws a loaded image's source rectangle into a destination rectangle. Images arrive in a later ticket.
    pub fn draw_image(&mut self, handle: u32, src: [f32; 4], dst: [f32; 4]) -> &mut Self {
        self.op(op::DRAW_IMAGE).u32(handle).f32s(&src).f32s(&dst)
    }

    /// Puts raw RGBA pixels at (x, y), ignoring the transform. `pixels` must be `w * h * 4` bytes.
    pub fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: &[u8]) -> &mut Self {
        self.op(op::BLIT_RGBA)
            .f32(x)
            .f32(y)
            .u32(w)
            .u32(h)
            .u32(pixels.len() as u32);
        self.bytes.extend_from_slice(pixels);
        self
    }
}
