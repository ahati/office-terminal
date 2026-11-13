//! Ports of `renderer-shape-image-fill.test.ts` (batch `shapes-a`), plus the
//! degradation pins the BLOCKED rows rely on.
//!
//! Loaded as a module of the `parity_shapes_a_tests` target so the port stays a
//! single `--test` target while every file stays under the 600-line limit.

use kitty_pptx_core::compiler::shapes::{apply_path_fill_mode, build_shape_paths, PathFill};
use kitty_pptx_core::compiler::style::{compile_fill, compile_stroke};
use kitty_pptx_core::contracts::{VectorCommand, VectorPaint, VectorPath};
use serde_json::{json, Value};

use crate::shapes_a_support::*;

fn image_fill(mode: Value) -> Value {
    let mut fill = json!({
        "fillType": "image",
        "imagePath": "ppt/media/mode.png",
        "mimeType": "image/png",
        "dpi": 96,
        "stretch": false,
    });
    for (key, value) in mode.as_object().expect("mode object") {
        fill[key] = value.clone();
    }
    fill
}

/// `it.each` "fails closed when the fill mode is %s" — `omitted`
/// (`renderer-shape-image-fill.test.ts:126-145`).
///
/// §20.1.8.14 EG_FillModeProperties: an omitted mode has no schema default, so
/// nothing is decoded or painted — but the shape's own outline still is.
/// Asserted through the compiled display list: zero `DrawImage`, one stroked
/// `DrawPath` with the fixture's 1 pt black line and no invented fill.
#[test]
fn image_fill_fails_closed_when_the_fill_mode_is_omitted() {
    let mut element = shape("ellipse");
    element["fill"] = image_fill(json!({}));
    let commands = compile_elements(vec![element]);

    assert_eq!(draw_image_count(&commands), 0, "no blip may be painted");
    assert_eq!(commands.len(), 1, "the stroked silhouette survives");
    let strokes = shape_strokes(&commands);
    assert_eq!(strokes.len(), 1);
    let stroke = strokes[0].as_ref().expect("stroke retained");
    assert_eq!((stroke.width_emu, stroke.color), (EMU, [0, 0, 0, 255]));
    assert_eq!(shape_fills(&commands), vec![None]);
}

/// `it.each` "fails closed when the fill mode is %s" — `conflicting`
/// (`renderer-shape-image-fill.test.ts:126-145`).
///
/// A `tile` + `stretch` pair is schema-invalid, so it paints (and decodes)
/// nothing; the stroke is still emitted.
#[test]
fn image_fill_fails_closed_when_tile_and_stretch_conflict() {
    let mut element = shape("ellipse");
    element["fill"] = image_fill(json!({
        "stretch": true,
        "tile": {"algn": "tl", "tx": 0, "ty": 0, "sx": 1.0, "sy": 1.0, "flip": "none"},
    }));
    let commands = compile_elements(vec![element]);

    assert_eq!(draw_image_count(&commands), 0, "no blip may be painted");
    assert_eq!(shape_fills(&commands), vec![None]);
    let strokes = shape_strokes(&commands);
    assert_eq!(strokes.len(), 1);
    assert_eq!(strokes[0].as_ref().map(|s| s.width_emu), Some(EMU));
}

/// "contains an ordinary decode failure while retaining shape stroke and text"
/// (`renderer-shape-image-fill.test.ts:216-235`).
///
/// The reference asserts the failure is contained: no `drawImage`, the stroke is
/// still painted, and the run still reaches the text callback. Rust has no
/// per-fill decode stage to fail (see the BLOCKED rows in the coverage report),
/// but the observable contract on the display list is the same: no `DrawImage`,
/// the stroked ellipse and the `"Picture fill text"` run are both emitted.
#[test]
fn image_fill_failure_retains_stroke_and_text() {
    let mut element = shape("ellipse");
    element["fill"] = image_fill(json!({"stretch": true, "alpha": 0.5}));
    element["textBody"] = picture_fill_text_body();
    let commands = compile_elements(vec![element]);

    assert_eq!(draw_image_count(&commands), 0, "no blip may be painted");
    let strokes = shape_strokes(&commands);
    assert_eq!(strokes.len(), 1, "the ellipse stroke is retained");
    assert_eq!(strokes[0].as_ref().map(|s| s.color), Some([0, 0, 0, 255]));
    assert_eq!(drawn_text(&commands), "Picture fill text");

    // ref: the shape is untouched by rendering (every run still 18 pt).
    let sizes: Vec<f32> = commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs),
            _ => None,
        })
        .flat_map(|runs| runs.iter().map(|run| run.font_size_pt))
        .collect();
    assert!(!sizes.is_empty(), "the run must be emitted");
    assert!(sizes.iter().all(|size| *size == 18.0), "sizes: {sizes:?}");
}

// ---------------------------------------------------------------------------
// Degradation pins for the BLOCKED rows (not reference cases)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod support_checks {
    use super::*;

    /// `renderer-shape-image-fill.test.ts` (all 18 BLOCKED rows) and
    /// `pattern-thumbnail.test.ts`: a `Fill::Image` or `Fill::Pattern` shape fill
    /// never reaches the display list. Pinned so a future image-fill/pattern
    /// implementation must come back here deliberately.
    #[test]
    fn compile_fill_projects_pattern_and_image_fills_as_documented() {
        let pattern: pptx_model::Fill = serde_json::from_value(json!({
            "fillType": "pattern", "preset": "pct50",
            "fg": "D21D54", "bg": "12CED4",
        }))
        .expect("pattern fill");
        assert_eq!(
            compile_fill(&pattern, 1.0),
            Some(VectorPaint::Solid([0xd2, 0x1d, 0x54, 255]))
        );

        let image: pptx_model::Fill = serde_json::from_value(json!({
            "fillType": "image", "imagePath": "ppt/media/a.png",
            "mimeType": "image/png", "stretch": true,
        }))
        .expect("image fill");
        assert_eq!(compile_fill(&image, 1.0), None);
    }

    /// `renderer.gradient.test.ts:52-68` wants the authored gradient line;
    /// `VectorStroke` has no paint slot, so the flat `color` wins.
    #[test]
    fn stroke_gradient_fill_is_dropped_by_compile_stroke() {
        let stroke: pptx_model::Stroke = serde_json::from_value(json!({
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
        }))
        .expect("stroke");
        let compiled = compile_stroke(&stroke).expect("stroke");
        assert_eq!(compiled.color, [255, 0, 0, 255]);
    }

    /// ref `geometry.ts:349-399`: `darken` composites black at 102/255, i.e. each
    /// channel becomes `v * 153/255`; `none` drops the paint entirely.
    #[test]
    fn darken_fold_matches_the_reference_overlay() {
        assert_eq!(
            apply_path_fill_mode(
                Some(&VectorPaint::Solid([0x33, 0x66, 0x99, 255])),
                PathFill::Darken
            ),
            Some(VectorPaint::Solid([31, 61, 92, 255]))
        );
        assert_eq!(
            apply_path_fill_mode(
                Some(&VectorPaint::Solid([0x33, 0x66, 0x99, 255])),
                PathFill::None
            ),
            None
        );
    }

    /// Documents the `custgeom-path-paint` BLOCKED symbol: `custGeom` dispatch
    /// always returns one `norm`-filled, stroked subpath.
    #[test]
    fn custgeom_dispatch_ignores_authored_path_paint() {
        let paths = vec![vec![pptx_model::PathCmd::MoveTo { x: 0.0, y: 0.0 }]];
        let (tx, _rx) = crossbeam_channel::unbounded();
        let subpaths = build_shape_paths("custGeom", PX as f32, PX as f32, &[], Some(&paths), &tx);
        assert_eq!(subpaths.len(), 1);
        assert_eq!(subpaths[0].fill, PathFill::Norm);
        assert!(subpaths[0].stroke);
        assert!(matches!(subpaths[0].path, VectorPath { .. }));
    }
}
