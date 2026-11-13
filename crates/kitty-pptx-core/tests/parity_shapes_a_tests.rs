//! Ports of the reference specs in batch `shapes-a`
//! (`/tmp/oooxml/packages/pptx/src`, read-only @ `1f5c6e1d`):
//! `custgeom-path-paint`, `frame-transform`, `renderer.gradient`,
//! `pattern-thumbnail` and `renderer-shape-image-fill`.
//!
//! Every case is either ported with the reference's own numbers against a Rust
//! API (EXACT), asserted through the compiled `VectorSlide` (BEHAVIOURAL), or
//! archived (not compiled) with the exact missing symbol when the production
//! API does not exist yet (BLOCKED). Canvas-only cases are listed in the
//! coverage report (`tests/parity/coverage/shapes-a.md`) as N/A.
//!
//! Layout: this target file holds the custGeom / frame / gradient / pattern
//! ports; `shapes_a_support/` holds the shared fixtures and the image-fill
//! cases, loaded as modules so the whole port stays one `--test` target while
//! every file stays under 600 lines.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_shapes_a_tests.rs.txt`; the
//! ledger keeps their rows.

use kitty_pptx_core::compiler::transform::compute_shape_transform;
use kitty_pptx_core::contracts::{
    AffineTransform, PathSegment, VectorCommand, VectorPaint, VectorStroke,
};
use kitty_pptx_core::svg::SlideSvgSerializer;
use serde_json::json;

mod shapes_a_support;

#[path = "shapes_a_support/image_fill_cases.rs"]
mod image_fill_cases;

#[path = "shapes_a_support/image_fill_paint_cases.rs"]
mod image_fill_paint_cases;

#[path = "shapes_a_support/gradient_cases.rs"]
mod gradient_cases;

use shapes_a_support::{compile_elements, compile_slide, contour, shape, square, EMU};

// ---------------------------------------------------------------------------
// custgeom-path-paint.test.ts
// ---------------------------------------------------------------------------

/// Case 1 — "fills and strokes the combined path once without per-path flags".
///
/// Three authored `<a:path>` entries with no `@fill`/`@stroke` attributes replay
/// into ONE canvas path, so the reference records exactly one `fill` and one
/// `stroke`, both on the same path index (`custgeom-path-paint.test.ts:84-89`).
/// The display list expresses that as a single `DrawPath` carrying both paints.
#[test]
fn custgeom_combined_path_fills_and_strokes_once() {
    let mut element = shape("custGeom");
    element["custGeom"] = json!([square(), square(), square()]);
    let commands = compile_elements(vec![element]);

    assert_eq!(commands.len(), 1, "one paint pass for the whole custGeom");
    let VectorCommand::DrawPath {
        path,
        fill,
        stroke,
        transform,
        ..
    } = &commands[0]
    else {
        panic!("expected a DrawPath, got {:?}", commands[0]);
    };

    // One path carries both paints, mirroring the single `beginPath` index.
    assert_eq!(fill, &Some(VectorPaint::Solid([0x33, 0x66, 0x99, 255])));
    let stroke: &VectorStroke = stroke.as_ref().expect("stroke must be emitted");
    assert_eq!(stroke.width_emu, EMU);
    assert_eq!(stroke.color, [0, 0, 0, 255]);

    // The three squares replay back-to-back (one `moveTo` per authored path).
    assert_eq!(
        path.segments
            .iter()
            .filter(|s| matches!(s, PathSegment::MoveTo { .. }))
            .count(),
        3
    );
    let mut expected = Vec::new();
    for _ in 0..3 {
        expected.extend(contour());
    }
    assert_eq!(path.segments, expected);

    // Local coordinates, placed at the shape origin (1in, 1in).
    assert_eq!(
        *transform,
        AffineTransform {
            sx: 1.0,
            ky: 0.0,
            kx: 0.0,
            sy: 1.0,
            tx: 914_400.0,
            ty: 914_400.0,
        }
    );
}

/// Supplementary (not a reference case): the same per-path paint contract,
/// driven through the preset engine, which does thread `PathFill` / `stroke`
/// (`presets.json` flags; ref `preset-geometry/index.ts:44-99`).
///
/// `actionbuttonbackprevious` authors exactly the reference fixture's flag
/// pattern: fills `[norm, darken, none, none]`, strokes `[no, no, yes, yes]`.
/// The reference paints a `darken` path as a translucent black overlay on top of
/// its base fill; the IR folds that pair into one opaque tinted paint
/// (`v * 153/255`, `compiler/shapes/mod.rs:171-215`).
#[test]
fn preset_path_flags_shade_and_stroke_per_path() {
    let commands = compile_elements(vec![shape("actionbuttonbackprevious")]);

    assert_eq!(commands.len(), 4, "one DrawPath per authored <a:path>");
    let fills = shapes_a_support::shape_fills(&commands);
    let strokes = shapes_a_support::shape_strokes(&commands);

    assert_eq!(fills[0], Some(VectorPaint::Solid([0x33, 0x66, 0x99, 255])));
    assert_eq!(fills[1], Some(VectorPaint::Solid([31, 61, 92, 255])));
    assert_eq!(fills[2], None, "fill=none path is stroke-only");
    assert_eq!(fills[3], None, "fill=none path is stroke-only");

    assert_eq!(strokes[0], None, "stroke=false");
    assert_eq!(strokes[1], None, "stroke=false");
    let stroked = strokes[2].as_ref().expect("stroke=true");
    assert_eq!(stroked.width_emu, EMU);
    assert_eq!(stroked.color, [0, 0, 0, 255]);
    assert!(strokes[3].is_some(), "stroke=true");
}

// ---------------------------------------------------------------------------
// frame-transform.test.ts
// ---------------------------------------------------------------------------

/// `it.each` table/chart/media — "applies rotation and both flips to a $type
/// frame" (`frame-transform.test.ts:31-53`).
///
/// The reference records `translate(60,50)`, `translate(-60,-50)`,
/// `rotate(π/2)`, `scale(-1,1)`, `scale(1,-1)` for frame
/// `{x:10,y:20,w:40,h:10,rotation:90,flipH:true,flipV:true}` at scale 2. The
/// element type is only a TypeScript parameter, so one Rust assertion covers all
/// three rows. The composed matrix is the canvas analogue of that call sequence
/// (ref `renderer.ts:7574-7589`): the page centre `(x + w/2, y + h/2)` is a
/// fixed point and both flips fold into `cos`/`sin` entries.
#[test]
fn frame_transform_applies_rotation_and_both_flips() {
    let m = compute_shape_transform(10, 20, 40, 10, 90.0, true, true);

    // R(π/2)·S(-1,-1) = [[0, 1], [-1, 0]]: 90° clockwise, both axes mirrored.
    assert!(m.sx.abs() < 1e-6, "a = cos·sx = 0, got {}", m.sx);
    assert!((m.kx - 1.0).abs() < 1e-6, "b = -sin·sy = 1, got {}", m.kx);
    assert!((m.ky + 1.0).abs() < 1e-6, "c = sin·sx = -1, got {}", m.ky);
    assert!(m.sy.abs() < 1e-6, "d = cos·sy = 0, got {}", m.sy);

    // translate(center) … translate(-center) leaves the centre fixed: local
    // centre (20, 5) maps to (x + w/2, y + h/2) = (30, 25).
    let center = (
        m.sx * 20.0 + m.kx * 5.0 + m.tx,
        m.ky * 20.0 + m.sy * 5.0 + m.ty,
    );
    assert!((center.0 - 30.0).abs() < 1e-3, "centre x: {center:?}");
    assert!((center.1 - 25.0).abs() < 1e-3, "centre y: {center:?}");
    assert!((m.tx - 25.0).abs() < 1e-3, "tx: {}", m.tx);
    assert!((m.ty - 45.0).abs() < 1e-3, "ty: {}", m.ty);

    // The 40 × 10 frame's corners land rotated about that centre.
    for (x, y, ex, ey) in [
        (0.0f32, 0.0f32, 25.0f32, 45.0f32),
        (40.0, 0.0, 25.0, 5.0),
        (40.0, 10.0, 35.0, 5.0),
        (0.0, 10.0, 35.0, 45.0),
    ] {
        let ax = m.sx * x + m.kx * y + m.tx;
        let ay = m.ky * x + m.sy * y + m.ty;
        assert!(
            (ax - ex).abs() < 1e-4 && (ay - ey).abs() < 1e-4,
            "({x},{y}) → ({ax},{ay}), expected ({ex},{ey})"
        );
    }

    // ref `renderer.ts:7579`: an identity frame returns without touching state.
    let identity = compute_shape_transform(10, 20, 40, 10, 0.0, false, false);
    assert_eq!((identity.tx, identity.ty), (10.0, 20.0));
    assert_eq!(
        (identity.sx, identity.sy, identity.kx, identity.ky),
        (1.0, 1.0, 0.0, 0.0)
    );
}

// ---------------------------------------------------------------------------
// pattern-thumbnail.test.ts
// ---------------------------------------------------------------------------

/// "uses the same point cells in an actual chart and shape render"
/// (`pattern-thumbnail.test.ts:26-63`).
///
/// The reference registers one CanvasPattern per painted body and asserts both
/// matrices carry the same point cell: `(canvasWidth / slideWidth) · 12700/8`,
/// i.e. one bitmap pixel is 1/8 pt (ref core/src/shape/paint.ts:459-479).
///
/// Rust twin: `Fill::Pattern` compiles to `VectorPaint::Pattern` (both the shape
/// body and a chart's cell/plot brush go through `compile_fill`), the cell step
/// is the shared `PATTERN_CELL_EMU` constant, and the SVG twin emits the
/// `<pattern>` cell at 8 pt = 12700 EMU. The reference's `chartFill` (chart-area
/// background) has no `pptx-model` field, so only the shape body is exercised;
/// the chart half is N/A.
#[test]
fn pattern_cells_use_the_shared_point_grid() {
    let mut element = shape("rect");
    element["x"] = json!(0);
    element["y"] = json!(0);
    element["width"] = json!(2_000_000);
    element["height"] = json!(1_000_000);
    element["fill"] = json!({
        "fillType": "pattern", "preset": "pct50",
        "fg": "D21D54", "bg": "12CED4",
    });
    let slide = compile_slide(vec![element]);

    let fills = shapes_a_support::shape_fills(&slide.commands);
    assert_eq!(
        fills[0],
        Some(VectorPaint::Pattern(shapes_a_support::pattern_fill(
            "pct50",
            [0xd2, 0x1d, 0x54, 255],
            [0x12, 0xce, 0xd4, 255],
        ))),
        "the preset, not its foreground fallback"
    );

    // ref `matrices[0].a ≈ (240 / 9_144_000) * 12_700 / 8`.
    let canvas_px = 240.0f64;
    let reference = (canvas_px / 9_144_000.0) * 12_700.0 / 8.0;
    let scale = (canvas_px / 9_144_000.0) as f32;
    let rust_step = (kitty_pptx_core::contracts::PATTERN_CELL_EMU * scale) as f64;
    // `toBeCloseTo` precision (2 decimals on the reference's own scale).
    assert!(
        (rust_step - reference).abs() < 1e-6,
        "cell step {rust_step} vs reference {reference}"
    );
    assert_eq!(kitty_pptx_core::contracts::PATTERN_CELL_EMU, 12_700.0 / 8.0);

    // The serialized twin keeps the same point grid: an 8 pt = 12700 EMU cell.
    let store = shapes_a_support::RecordingStore::new(vec![]);
    let svg = kitty_pptx_core::svg::DefaultSlideSvgSerializer::new()
        .serialize_svg(
            &slide,
            &store,
            kitty_pptx_core::contracts::SvgTextMode::SelectableText,
        )
        .expect("serialize svg");
    assert!(
        svg.contains(r#"width="12700" height="12700""#),
        "8 pt pattern cell:\n{svg}"
    );
    assert!(svg.contains(r#"width="1587.5" height="1587.5""#), "1/8 pt pixel");
    assert!(svg.contains("fill=\"url(#pattfill_1)\""), "the brush is referenced");
}
