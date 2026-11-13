//! Ports of the reference specs in batch `shapes-a`
//! (`/tmp/oooxml/packages/pptx/src`, read-only @ `1f5c6e1d`):
//! `custgeom-path-paint`, `frame-transform`, `renderer.gradient`,
//! `pattern-thumbnail` and `renderer-shape-image-fill`.
//!
//! Every case is either ported with the reference's own numbers against a Rust
//! API (EXACT), asserted through the compiled `VectorSlide` (BEHAVIOURAL), or
//! marked `#[ignore]` with the exact missing symbol when the production API does
//! not exist yet (BLOCKED). Canvas-only cases are listed in the coverage report
//! (`tests/parity/coverage/shapes-a.md`) as N/A.
//!
//! Layout: this target file holds the custGeom / frame / gradient / pattern
//! ports; `shapes_a_support/` holds the shared fixtures and the image-fill
//! cases, loaded as modules so the whole port stays one `--test` target while
//! every file stays under 600 lines.

use kitty_pptx_core::compiler::style::linear_gradient_axis;
use kitty_pptx_core::compiler::transform::compute_shape_transform;
use kitty_pptx_core::compiler::tables::compile_table_element;
use kitty_pptx_core::contracts::{
    AffineTransform, PathSegment, VectorCommand, VectorPaint, VectorStroke,
};
use pptx_model::TableElement;
use serde_json::json;

mod shapes_a_support;

#[path = "shapes_a_support/image_fill_cases.rs"]
mod image_fill_cases;

use shapes_a_support::{compile_elements, contour, shape, square, EMU};

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

/// Case 2 — "honours unfilled, unstroked and shaded paths individually".
///
/// BLOCKED: `pptx_model::ShapeElement::cust_geom_paint`
/// (`pptx_model::PathPaint`) is parsed but never read —
/// `compiler::shapes::build_shape_paths` hard-codes `PathFill::Norm` /
/// `stroke: true` for `custGeom`, so every path is filled and stroked once.
/// The same contract *is* threaded for presets; see
/// `preset_path_flags_shade_and_stroke_per_path` below.
#[test]
#[ignore = "BLOCKED: ShapeElement::cust_geom_paint (pptx_model::PathPaint) is not \
            consumed by compiler::shapes::build_shape_paths"]
fn custgeom_per_path_fill_and_stroke_flags() {
    let mut element = shape("custGeom");
    element["custGeom"] = json!([square(), square(), square()]);
    element["custGeomPaint"] = json!([
        {"fill": null, "stroke": false},
        {"fill": "none", "stroke": true},
        {"fill": "darken", "stroke": true},
    ]);
    let commands = compile_elements(vec![element]);

    // ref: path 1 paints the base fill, path 2 only its stroke, path 3 the base
    // fill plus a darkening overlay and then its stroke.
    let fills = shapes_a_support::shape_fills(&commands);
    let strokes = shapes_a_support::shape_strokes(&commands);
    assert_eq!(strokes.iter().filter(|s| s.is_some()).count(), 2);
    assert_eq!(fills.iter().filter(|f| f.is_some()).count(), 3);
    assert_eq!(fills[2], Some(VectorPaint::Solid([31, 61, 92, 255])));
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
// renderer.gradient.test.ts
// ---------------------------------------------------------------------------

/// Case 1 — "counter-rotates rotWithShape=false fills using the authored shape
/// rotation" (`renderer.gradient.test.ts:37-50`).
///
/// The reference projects the authored 90° gradient onto the horizontal axis
/// `(0,50) → (100,50)` because a `rotWithShape="0"` fill must not inherit the
/// shape's 90° rotation. BLOCKED: `Fill::Gradient::rot_with_shape` is dropped by
/// `compiler::style::compile_fill` and `VectorGradient` has no rotation slot, so
/// the gradient always rotates with the shape.
#[test]
#[ignore = "BLOCKED: VectorGradient has no rot_with_shape / counter-rotation; \
            compiler::style::compile_fill drops Fill::Gradient.rot_with_shape"]
fn rot_with_shape_false_counter_rotates_the_gradient() {
    let mut element = shape("rect");
    element["width"] = json!(100);
    element["height"] = json!(100);
    element["rotation"] = json!(90.0);
    element["fill"] = json!({
        "fillType": "gradient",
        "stops": [
            {"position": 0.0, "color": "000000"},
            {"position": 1.0, "color": "FFFFFF"}
        ],
        "angle": 90.0,
        "gradType": "linear",
        "rotWithShape": false,
    });

    let commands = compile_elements(vec![element]);
    let fills = shapes_a_support::shape_fills(&commands);
    let Some(Some(VectorPaint::LinearGradient(gradient))) = fills.first() else {
        panic!("expected a linear gradient fill, got {fills:?}");
    };
    // Counter-rotated by the authored 90° shape rotation.
    assert!(
        gradient.angle_deg.abs() < 1e-6,
        "angle: {}",
        gradient.angle_deg
    );
    let (start, end) = linear_gradient_axis(gradient.angle_deg, 0.0, 0.0, 100.0, 100.0);
    assert!(
        (start.0 - 0.0).abs() < 1e-4 && (start.1 - 50.0).abs() < 1e-4,
        "{start:?}"
    );
    assert!(
        (end.0 - 100.0).abs() < 1e-4 && (end.1 - 50.0).abs() < 1e-4,
        "{end:?}"
    );
}

// Case 2 — "uses the complete gradient line paint instead of its flat fallback
// color" (`renderer.gradient.test.ts:52-68`). BLOCKED: `VectorStroke` carries one
// flat `[u8; 4]` colour and `compiler::style::compile_stroke` ignores
// `pptx_model::Stroke::fill`, so a gradient line keeps its flat fallback — the
// exact behaviour the reference case forbids. No Rust symbol can express the
// gradient stroke yet, so the case cannot even be encoded as a failing test.
// (Pinned by `image_fill_cases::support_checks::stroke_gradient_fill_is_dropped_by_compile_stroke`.)

/// Case 3 — "uses the complete gradient paint for table cells"
/// (`renderer.gradient.test.ts:70-110`).
///
/// The reference asserts exactly one `createLinearGradient` call for the cell
/// fill and that `fillRect` paints that gradient object rather than the flat
/// fallback. The display-list equivalent: one `DrawPath` whose fill is the
/// authored `LinearGradient` (never a `Solid`), framed on the cell box.
#[test]
fn table_cell_uses_the_authored_gradient_paint() {
    let table_value = json!({
        "x": 0, "y": 0, "width": 100, "height": 50,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "background": null,
        "cols": [100],
        "rows": [{
            "height": 50,
            "cells": [{
                "textBody": null,
                "fill": {
                    "fillType": "gradient",
                    "stops": [
                        {"position": 0.0, "color": "000000"},
                        {"position": 1.0, "color": "FFFFFF"}
                    ],
                    "angle": 0.0,
                    "gradType": "linear",
                },
                "borderL": null, "borderR": null, "borderT": null, "borderB": null,
                "gridSpan": 1, "rowSpan": 1, "hMerge": false, "vMerge": false,
            }],
        }],
        "rtl": false,
    });
    let table: TableElement = serde_json::from_value(table_value).expect("table JSON");
    let (tx, _rx) = crossbeam_channel::unbounded();
    let commands = compile_table_element(&table, &tx);

    // ref `expect(linear).toHaveLength(1)`: exactly one gradient, no flat fill.
    let paints = shapes_a_support::shape_fills(&commands);
    assert_eq!(paints.len(), 1, "one cell fill, got {paints:?}");
    let Some(VectorPaint::LinearGradient(gradient)) = &paints[0] else {
        panic!("cell fill must stay a gradient, got {:?}", paints[0]);
    };
    assert_eq!(gradient.angle_deg, 0.0);
    assert_eq!(
        gradient.stops,
        vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])]
    );

    // The gradient spans the cell box `(0,0)-(100,50)` on its horizontal axis.
    let (start, end) = linear_gradient_axis(gradient.angle_deg, 0.0, 0.0, 100.0, 50.0);
    assert_eq!(start, (0.0, 25.0));
    assert_eq!(end, (100.0, 25.0));

    assert!(
        !paints
            .iter()
            .any(|p| matches!(p, Some(VectorPaint::Solid(_)))),
        "the flat fallback colour must not be painted"
    );
    // No border was authored, so the cell fill is the only command.
    assert_eq!(commands.len(), 1);
}

// ---------------------------------------------------------------------------
// pattern-thumbnail.test.ts
// ---------------------------------------------------------------------------

// "uses the same point cells in an actual chart and shape render"
// (`pattern-thumbnail.test.ts:35-63`).
//
// BLOCKED: the display list has no pattern paint at all —
// `compiler::style::compile_fill` maps `Fill::Pattern` onto
// `VectorPaint::Solid(fg)`, and neither shape nor chart compilation emits the
// `12700 / 8` EMU pattern cell scale (`(240/9144000) * 12700/8`) the reference
// asserts for both fills. Symbols needed: a `VectorPaint::Pattern`
// (preset + fg + bg) plus a pattern-cell scale in the shape and chart
// compilers. The case also reads canvas matrices through
// `createPattern().setTransform`, so it has no non-IR assertion target either.
