//! The encoder against the golden bytes in tests/fixtures/canvas/command-buffer-v1.bin, which any decoder of the
//! ABI can be checked against. Every op in the table appears once, in this order. The values are exact in f32,
//! so a decoder reads back the same numbers written here.
//!
//! Regenerate the fixture after an intentional ABI change with:
//!   cargo test --features canvas --test command_buffer -- --ignored write_golden

#![cfg(feature = "canvas")]

use chittr_cartridge::canvas::{
    rgba, CommandBuffer, Composite, FillRule, LineCap, LineJoin, TextAlign, TextBaseline,
};

const GOLDEN: &[u8] = include_bytes!("fixtures/canvas/command-buffer-v1.bin");
const GOLDEN_V2: &[u8] = include_bytes!("fixtures/canvas/command-buffer-v2.bin");

fn sequence() -> CommandBuffer {
    let mut c = CommandBuffer::new();
    c.save()
        .set_transform([1.0, 0.0, 0.0, 1.0, 10.0, 20.0])
        .transform([0.5, 0.0, 0.0, 0.5, 0.0, 0.0])
        .translate(1.5, -2.25)
        .rotate(0.5)
        .scale(2.0, 3.0)
        .set_alpha(0.75)
        .set_composite(Composite::Multiply)
        .set_image_smoothing(false)
        .begin_path()
        .move_to(1.0, 2.0)
        .line_to(3.0, 4.0)
        .quad_to(5.0, 6.0, 7.0, 8.0)
        .bezier_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
        .arc(10.0, 20.0, 5.0, 0.0, 3.25, true)
        .arc_to(1.0, 1.0, 2.0, 2.0, 3.0)
        .ellipse(4.0, 5.0, 6.0, 7.0, 0.25, 0.0, 1.5, false)
        .rect(0.0, 0.0, 10.0, 10.0)
        .round_rect(1.0, 2.0, 3.0, 4.0, 5.0)
        .close_path()
        .fill(FillRule::EvenOdd)
        .stroke()
        .clip(FillRule::NonZero)
        .fill_rect(0.0, 0.0, 320.0, 180.0)
        .stroke_rect(1.0, 1.0, 2.0, 2.0)
        .clear_rect(3.0, 3.0, 4.0, 4.0)
        .fill_color(rgba(255, 48, 48, 255))
        .stroke_color(rgba(0, 0, 0, 128))
        .line_width(2.5)
        .line_cap(LineCap::Round)
        .line_join(LineJoin::Bevel)
        .miter_limit(10.0)
        .line_dash(&[4.0, 2.0])
        .def_linear(
            7,
            0.0,
            0.0,
            100.0,
            0.0,
            &[(0.0, rgba(0, 0, 0, 255)), (1.0, rgba(255, 255, 255, 255))],
        )
        .def_radial(8, 50.0, 50.0, 0.0, 50.0, 50.0, 40.0, &[(0.5, 0x1122_33ff)])
        .fill_gradient(7)
        .stroke_gradient(8)
        .font("Mono", 12.0, 700.0)
        .text_align(TextAlign::Center)
        .text_baseline(TextBaseline::Middle)
        .fill_text("h\u{e9}llo", 10.0, 20.0)
        .stroke_text("x", 1.0, 2.0)
        .draw_image(3, [0.0, 0.0, 16.0, 16.0], [10.0, 10.0, 32.0, 32.0])
        .blit_rgba(0.0, 0.0, 2, 1, &[1, 2, 3, 4, 5, 6, 7, 8])
        .restore();
    c
}

#[test]
fn encoder_matches_the_golden_bytes() {
    assert_eq!(sequence().bytes(), GOLDEN);
}

/// The v2 ops (0x52, 0x53), in table order. The v1 sequence above stays the 0x01–0x51 fixture.
fn v2_sequence() -> CommandBuffer {
    let mut c = CommandBuffer::new();
    c.blit_rgba_scaled(
        [10.0, 20.0, 320.0, 200.0],
        2,
        1,
        false,
        &[1, 2, 3, 4, 5, 6, 7, 8],
    )
    .blit_indexed_scaled(
        [0.0, 0.0, 64.0, 32.0],
        2,
        1,
        true,
        &[
            rgba(1, 2, 3, 4),
            rgba(5, 6, 7, 8),
            rgba(9, 10, 11, 12),
        ],
        &[0, 2],
    );
    c
}

#[test]
fn encoder_matches_the_v2_golden_bytes() {
    assert_eq!(v2_sequence().bytes(), GOLDEN_V2);
}

#[test]
#[ignore = "writes the golden fixture; run on purpose after an ABI change"]
fn write_golden() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/canvas/command-buffer-v1.bin"
    );
    std::fs::write(path, sequence().bytes()).expect("write the golden fixture");
}

#[test]
#[ignore = "writes the v2 golden fixture; run on purpose after an ABI change"]
fn write_golden_v2() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/canvas/command-buffer-v2.bin"
    );
    std::fs::write(path, v2_sequence().bytes()).expect("write the v2 golden fixture");
}
