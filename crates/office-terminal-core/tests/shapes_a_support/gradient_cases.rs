//! Ports of `renderer.gradient.test.ts` (batch `shapes-a`): the
//! `rotWithShape="0"` counter-rotation and the gradient/pattern line paint.
//!
//! Loaded as a module of the `parity_shapes_a_tests` target; see the target file
//! for the shared fixtures.

use office_terminal_core::compiler::style::{gradient, linear_gradient_axis};
use office_terminal_core::compiler::tables::compile_table_element;
use office_terminal_core::contracts::VectorPaint;
use office_terminal_core::svg::SlideSvgSerializer;
use pptx_model::TableElement;
use serde_json::json;

use crate::shapes_a_support::*;

/// One numeric attribute of an SVG tag (used for the paint-frame assertions).
fn numeric_attr(tag: &str, name: &str) -> f32 {
    let key = format!("{name}=\"");
    let start = tag.find(&key).unwrap_or_else(|| panic!("missing {name} in {tag}")) + key.len();
    let rest = &tag[start..];
    let end = rest.find('"').unwrap_or_else(|| panic!("unterminated {name}"));
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a number: {}", &rest[..end]))
}

/// Case 1 — "counter-rotates rotWithShape=false fills using the authored shape
/// rotation" (`renderer.gradient.test.ts:37-50`).
///
/// The reference projects the authored 90° gradient onto the horizontal axis
/// `(0,50) → (100,50)` because a `rotWithShape="0"` fill must not inherit the
/// shape's 90° rotation. The IR keeps the authored angle and the frame that
/// projects it (`GradientFrame::counter_rotation_deg`, the reference's
/// `counterRotatedFillTransform` argument, ref paint.ts:307-315), so both
/// backends compute the same axis from the same numbers.
#[test]
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

    let slide = compile_slide(vec![element.clone()]);
    let fills = shape_fills(&slide.commands);
    let Some(Some(VectorPaint::LinearGradient(gradient))) = fills.first() else {
        panic!("expected a linear gradient fill, got {fills:?}");
    };
    // The authored angle survives; the frame carries the counter-rotation.
    assert_eq!(gradient.angle_deg, 90.0);
    assert!(!gradient.frame.rot_with_shape);
    assert_eq!(gradient.frame.rotation_deg, 90.0);
    // No reflection: the frame turns back by the authored rotation, which is
    // the reference's `counterRotatedFillTransform` negation.
    assert!((gradient.frame.counter_rotation_deg() + 90.0).abs() < 1e-6);

    // Painted axis: `[0, 50, 100, 50]` as the reference's
    // `createLinearGradient` arguments (its `linear[0]`).
    let path = first_path(&slide.commands);
    let (start, end) = gradient::linear_axis_for_path(gradient, &path).expect("path bounds");
    assert!(
        (start.0 - 0.0).abs() < 1e-4 && (start.1 - 50.0).abs() < 1e-4,
        "{start:?}"
    );
    assert!((end.0 - 100.0).abs() < 1e-4 && (end.1 - 50.0).abs() < 1e-4, "{end:?}");

    // The same fixture with the schema default (`rotWithShape` omitted) keeps
    // the authored 90° axis: the shape rotation is inherited.
    element["fill"]["rotWithShape"] = json!(true);
    let rotating = compile_slide(vec![element]);
    let fills = shape_fills(&rotating.commands);
    let Some(Some(VectorPaint::LinearGradient(gradient))) = fills.first() else {
        panic!("expected a linear gradient fill, got {fills:?}");
    };
    assert!(gradient.frame.rot_with_shape);
    assert_eq!(gradient.frame.counter_rotation_deg(), 0.0);
    let path = first_path(&rotating.commands);
    let (start, end) = gradient::linear_axis_for_path(gradient, &path).expect("path bounds");
    assert!(
        (start.0 - 50.0).abs() < 1e-4 && (start.1 - 0.0).abs() < 1e-4,
        "{start:?}"
    );
    assert!((end.0 - 50.0).abs() < 1e-4 && (end.1 - 100.0).abs() < 1e-4, "{end:?}");
}

/// The painted twin of case 1: the counter-rotated brush must reach the raster
/// and the SVG on the *same* axis (100×100 local units, 1 in box).
#[test]
fn rot_with_shape_false_paints_the_counter_rotated_axis_in_both_backends() {
    let mut element = shape("rect");
    element["x"] = json!(914_400);
    element["y"] = json!(1_828_800);
    element["width"] = json!(914_400);
    element["height"] = json!(914_400);
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
    let slide = compile_slide(vec![element]);

    // The 1 in box starts at (96, 192) device px at the 960 px slide width and
    // spans 96 px. The reference builds the gradient in the shape's *rotated*
    // user space (`ctx.createLinearGradient` runs after `ctx.rotate`), so the
    // counter-rotated local axis runs along the slide's authored 90° direction:
    // the visible brush is vertical, first stop at the top.
    let store = RecordingStore::new(vec![]);
    let rendered = rasterize(&slide, &store);
    let top = pixel(&rendered, 144, 195);
    let bottom = pixel(&rendered, 144, 284);
    assert!(top[0] < 24, "top edge must be the dark stop, got {top:?}");
    assert!(bottom[0] > 231, "bottom edge must be the light stop, got {bottom:?}");

    // The SVG twin must frame the gradient on the same axis (local EMU).
    let svg = office_terminal_core::svg::DefaultSlideSvgSerializer::new()
        .serialize_svg(
            &slide,
            &store,
            office_terminal_core::contracts::SvgTextMode::SelectableText,
        )
        .expect("serialize svg");
    let tag = svg
        .lines()
        .find(|line| line.contains(r#"<linearGradient id="grad_1""#))
        .unwrap_or_else(|| panic!("gradient def missing:\n{svg}"));
    // f32 trig leaves a sub-EMU residue on the exact 90° rotation.
    assert!(numeric_attr(tag, "x1").abs() < 1.0, "x1: {tag}");
    assert!((numeric_attr(tag, "y1") - 457_200.0).abs() < 1.0, "y1: {tag}");
    assert!((numeric_attr(tag, "x2") - 914_400.0).abs() < 1.0, "x2: {tag}");
    assert!((numeric_attr(tag, "y2") - 457_200.0).abs() < 1.0, "y2: {tag}");
}

/// Case 2 — "uses the complete gradient line paint instead of its flat fallback
/// color" (`renderer.gradient.test.ts:52-68`).
///
/// The reference asserts exactly one `createLinearGradient` call for the stroke
/// and `ctx.strokeStyle === gradient`. The IR twin: the authored `a:ln` fill
/// reaches `VectorStroke::paint` as a `LinearGradient` (never the flat
/// `color`), framed on the stroked box, and both backends paint it.
#[test]
fn stroke_uses_its_authored_gradient_paint() {
    // The reference fixture: a 1 pt `FF0000` line whose fill is a black→white
    // 0° gradient, stroked over the box `{x: 10, y: 20, w: 100, h: 50}`.
    let mut element = shape("rect");
    element["x"] = json!(914_400);
    element["y"] = json!(914_400);
    element["width"] = json!(914_400);
    element["height"] = json!(914_400);
    element["fill"] = json!({ "fillType": "none" });
    element["stroke"] = json!({
        "color": "FF0000",
        "width": EMU,
        "fill": {
            "fillType": "gradient",
            "stops": [
                {"position": 0.0, "color": "000000"},
                {"position": 1.0, "color": "FFFFFF"}
            ],
            "angle": 0.0,
            "gradType": "linear",
        },
    });
    let slide = compile_slide(vec![element]);

    // Exactly one gradient stroke paint, and no flat fallback colour beside it.
    let strokes = shape_strokes(&slide.commands);
    let painted: Vec<_> = strokes.iter().flatten().collect();
    assert_eq!(painted.len(), 1, "one stroked path, got {strokes:?}");
    let stroke = painted[0];
    let Some(VectorPaint::LinearGradient(gradient)) = stroke.paint.as_deref() else {
        panic!("the line must keep its gradient paint, got {:?}", stroke.paint);
    };
    assert_eq!(gradient.angle_deg, 0.0);
    assert_eq!(gradient.stops, vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])]);
    // The flat authored colour is only the fallback and must not be painted.
    assert_eq!(stroke.color, [0xFF, 0, 0, 255]);

    // The paint frame is the stroked box: a horizontal axis across the path.
    let path = first_path(&slide.commands);
    let (start, end) = gradient::linear_axis_for_path(gradient, &path).expect("path bounds");
    assert!((start.0 - 0.0).abs() < 1e-4 && (start.1 - 457_200.0).abs() < 1e-4);
    assert!((end.0 - 914_400.0).abs() < 1e-4 && (end.1 - 457_200.0).abs() < 1e-4);

    // Raster: the top edge runs from the dark stop to the light one.
    let store = RecordingStore::new(vec![]);
    let rendered = rasterize(&slide, &store);
    // The 1 pt outline straddles the edge, so the sample is blended with the
    // white canvas: assert the dark→light direction, not an absolute value.
    let left = pixel(&rendered, 98, 96);
    let right = pixel(&rendered, 186, 96);
    assert!(
        left[0] + 80 < right[0],
        "stroke must run dark → light, got {left:?} → {right:?}"
    );
    assert!(right[0] > 200, "stroke end must be near white, got {right:?}");
    assert!(
        left[0] < 140,
        "a flat fallback would paint one colour along the whole edge, got {left:?}"
    );

    // SVG: the stroke references a paint server, never the flat colour.
    let svg = office_terminal_core::svg::DefaultSlideSvgSerializer::new()
        .serialize_svg(
            &slide,
            &store,
            office_terminal_core::contracts::SvgTextMode::SelectableText,
        )
        .expect("serialize svg");
    assert!(svg.contains(r#"stroke="url(#sgrad_1)""#), "paint server:\n{svg}");
    assert!(
        !svg.contains(r#"stroke="rgb(255,0,0)""#),
        "the flat fallback colour must not be painted:\n{svg}"
    );
    assert!(
        svg.contains(
            r#"<linearGradient id="sgrad_1" gradientUnits="userSpaceOnUse" x1="0" y1="457200" x2="914400" y2="457200">"#
        ),
        "stroke gradient frame:\n{svg}"
    );
}

/// Supplementary (not a reference case): the same `applyStroke` contract for a
/// `a:pattFill` line — the reference resolves any non-solid line fill through
/// `resolveShapeFill` (ref renderer.ts:7542-7565), so a pattern line paints the
/// preset bitmap rather than its flat colour.
#[test]
fn stroke_uses_its_authored_pattern_paint() {
    let mut element = shape("rect");
    element["fill"] = json!({ "fillType": "none" });
    element["stroke"] = json!({
        "color": "FF0000",
        "width": 95_250,
        "fill": { "fillType": "pattern", "preset": "pct50", "fg": "D21D54", "bg": "12CED4" },
    });
    let slide = compile_slide(vec![element]);

    let strokes = shape_strokes(&slide.commands);
    let stroke = strokes.iter().flatten().next().expect("the outline is stroked");
    assert_eq!(
        stroke.paint.as_deref(),
        Some(&VectorPaint::Pattern(pattern_fill(
            "pct50",
            [0xd2, 0x1d, 0x54, 255],
            [0x12, 0xce, 0xd4, 255],
        ))),
        "the pattern, not its foreground fallback"
    );

    let store = RecordingStore::new(vec![]);
    let svg = office_terminal_core::svg::DefaultSlideSvgSerializer::new()
        .serialize_svg(
            &slide,
            &store,
            office_terminal_core::contracts::SvgTextMode::SelectableText,
        )
        .expect("serialize svg");
    assert!(svg.contains(r#"stroke="url(#spattfill_1)""#), "paint server:\n{svg}");
    assert!(svg.contains(r#"<pattern id="spattfill_1""#), "pattern def:\n{svg}");

    // Both grid colours reach the painted outline.
    let rendered = rasterize(&slide, &store);
    let mut fg = 0usize;
    let mut bg = 0usize;
    for x in 96..192u32 {
        let px = pixel(&rendered, x, 96);
        if px[0] > 150 && px[2] < 140 {
            fg += 1;
        } else if px[1] > 150 && px[0] < 100 {
            bg += 1;
        }
    }
    assert!(fg > 0 && bg > 0, "both pattern cells must paint, fg {fg} bg {bg}");
}

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
    let paints = shape_fills(&commands);
    assert_eq!(paints.len(), 1, "one cell fill, got {paints:?}");
    let Some(VectorPaint::LinearGradient(gradient)) = &paints[0] else {
        panic!("cell fill must stay a gradient, got {:?}", paints[0]);
    };
    assert_eq!(gradient.angle_deg, 0.0);
    assert_eq!(gradient.stops, vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])]);

    // The gradient spans the cell box `(0,0)-(100,50)` on its horizontal axis.
    let (start, end) = linear_gradient_axis(gradient.angle_deg, 0.0, 0.0, 100.0, 50.0);
    assert_eq!(start, (0.0, 25.0));
    assert_eq!(end, (100.0, 25.0));

    assert!(
        !paints.iter().any(|p| matches!(p, Some(VectorPaint::Solid(_)))),
        "the flat fallback colour must not be painted"
    );
    // No border was authored, so the cell fill is the only command.
    assert_eq!(commands.len(), 1);
}

// ---------------------------------------------------------------------------
