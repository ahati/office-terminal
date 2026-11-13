//! Preset-geometry dispatch tests (ECMA-376 §19.5.31.3 + ST_PathFillMode).
//!
//! These pin the behaviour of `shapes::build_shape_paths`, the entry point the
//! element compilers use. Expectations are derived by hand from the shipped
//! `presets.json` definitions and the reference evaluator
//! (`packages/core/src/shape/preset-geometry/{geometry,evaluator}.ts`), so they
//! do not trust `preset_exec` itself.

use crossbeam_channel::unbounded;
use kitty_pptx_core::compiler::shapes::preset_data::preset_names;
use kitty_pptx_core::compiler::shapes::preset_exec::{PathFill, PresetSubpath};
use kitty_pptx_core::compiler::shapes::{apply_path_fill_mode, build_shape_path, build_shape_paths};
use kitty_pptx_core::contracts::{
    FillRule, PathSegment, VectorGradient, VectorPaint, VectorPath, WarningEvent,
};
use pptx_model::PathCmd;

/// On-curve points of a path in draw order; control points are ignored.
fn endpoints(path: &VectorPath) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    for segment in &path.segments {
        match segment {
            PathSegment::MoveTo { x, y }
            | PathSegment::LineTo { x, y }
            | PathSegment::QuadTo { x, y, .. }
            | PathSegment::CubicTo { x, y, .. } => points.push((*x, *y)),
            PathSegment::Close => {}
        }
    }
    points
}

/// Dispatch one preset with the given raw adjust values.
fn dispatch(name: &str, w: f32, h: f32, adj: &[Option<f64>]) -> (Vec<PresetSubpath>, bool) {
    let (tx, rx) = unbounded();
    let subpaths = build_shape_paths(name, w, h, adj, None, &tx);
    let warned = rx.try_recv().is_ok();
    (subpaths, warned)
}

fn assert_close(actual: (f32, f32), expected: (f32, f32)) {
    assert!(
        (actual.0 - expected.0).abs() < 1e-2 && (actual.1 - expected.1).abs() < 1e-2,
        "expected ({}, {}), got ({}, {})",
        expected.0,
        expected.1,
        actual.0,
        actual.1
    );
}

// ---------------------------------------------------------------------------
// Formula checks against the ECMA definitions
// ---------------------------------------------------------------------------

#[test]
fn hexagon_slant_is_min_dimension_times_adjust() {
    // presets.json `hexagon`: `x1 = ss * a / 100000` with `ss = min(w, h)`;
    // the second vertex sits at (x1, vc - dy1). At 300×150, ss = 150, so the
    // default adj = 25000 gives a 37.5 slant.
    let (subpaths, warned) = dispatch("hexagon", 300.0, 150.0, &[]);
    assert!(!warned);
    let points = endpoints(&subpaths[0].path);
    assert_eq!(points[0], (0.0, 75.0), "hexagon starts at the left vertex");
    assert_close(points[1], (37.5, 0.0));

    // Doubling the handle doubles the slant, and the bound is the small side:
    // a 150×300 box keeps the same 37.5 slant.
    let (subpaths, _) = dispatch("hexagon", 300.0, 150.0, &[Some(50_000.0)]);
    assert_close(endpoints(&subpaths[0].path)[1], (75.0, 0.0));

    let (subpaths, _) = dispatch("hexagon", 150.0, 300.0, &[]);
    assert_close(endpoints(&subpaths[0].path)[1], (37.5, 0.0));
}

#[test]
fn chevron_notch_is_half_the_min_dimension() {
    // presets.json `chevron`: `x1 = ss * a / 100000`, and the notch tip is the
    // last vertex `(x1, vc)`. Default adj = 50000 gives a notch of ss/2 = 75.
    let (subpaths, warned) = dispatch("chevron", 300.0, 150.0, &[]);
    assert!(!warned);
    let points = endpoints(&subpaths[0].path);
    assert_close(points[1], (225.0, 0.0));
    assert_close(points[5], (75.0, 75.0));
}

#[test]
fn right_arrow_head_uses_adj2_and_shaft_uses_adj1() {
    // presets.json `rightArrow`: `dx1 = ss * a2 / 100000` is the head length
    // (adj2, not adj1) and `dy1 = h * a1 / 200000` is the half shaft height.
    let (subpaths, _) = dispatch("rightArrow", 300.0, 150.0, &[]);
    let points = endpoints(&subpaths[0].path);
    assert_close(points[1], (225.0, 37.5));

    // adj1 = 20000 shrinks the shaft (dy1 = 15) but must not move the head.
    let (subpaths, _) = dispatch("rightArrow", 300.0, 150.0, &[Some(20_000.0)]);
    let points = endpoints(&subpaths[0].path);
    assert_close(points[1], (225.0, 60.0));
    assert_close(points[2], (225.0, 0.0));

    // adj2 = 20000 shortens the head to 30 and leaves the shaft alone.
    let (subpaths, _) = dispatch("rightArrow", 300.0, 150.0, &[None, Some(20_000.0)]);
    let points = endpoints(&subpaths[0].path);
    assert_close(points[1], (270.0, 37.5));
}

#[test]
fn star8_inner_radius_is_wd2_times_adjust() {
    // presets.json `star8`: `iwd2 = wd2 * a / 50000`; the first inner vertex is
    // `(hc - iwd2 * 92388/100000, vc - ihd2 * 38268/100000)`. On a square box
    // the ellipse collapses to a circle, so its distance from the centre is the
    // inner radius itself.
    let radius = |w: f32, adj: f64| -> f32 { (w / 2.0) * (adj / 50_000.0) as f32 };

    for adj_value in [25_000.0_f64, 37_500.0] {
        let (subpaths, _) = dispatch("star8", 300.0, 300.0, &[Some(adj_value)]);
        let point = endpoints(&subpaths[0].path)[1];
        let measured = ((point.0 - 150.0).powi(2) + (point.1 - 150.0).powi(2)).sqrt();
        assert!(
            (measured - radius(300.0, adj_value)).abs() < 1e-2,
            "adj {adj_value}: expected inner radius {}, got {measured}",
            radius(300.0, adj_value)
        );
    }

    // Non-square boxes scale the axes independently but still follow wd2.
    let (subpaths, _) = dispatch("star8", 300.0, 150.0, &[Some(37_500.0)]);
    let point = endpoints(&subpaths[0].path)[1];
    let expected_x = 150.0 - radius(300.0, 37_500.0) * 0.92388;
    assert!((point.0 - expected_x).abs() < 1e-2, "got {}", point.0);
}

// ---------------------------------------------------------------------------
// Catalogue coverage
// ---------------------------------------------------------------------------

#[test]
fn all_preset_names_resolve_to_a_non_empty_path() {
    let names = preset_names();
    assert_eq!(names.len(), 186, "presets.json definition count");

    for name in names {
        let (subpaths, warned) = dispatch(name, 200.0, 120.0, &[]);
        assert!(!warned, "{name} must not warn");
        assert!(!subpaths.is_empty(), "{name} produced no subpath");
        for subpath in &subpaths {
            assert!(
                !subpath.path.segments.is_empty(),
                "{name} produced an empty subpath"
            );
            assert!(
                matches!(subpath.path.segments[0], PathSegment::MoveTo { .. }),
                "{name} subpath must start with MoveTo"
            );
            for (x, y) in endpoints(&subpath.path) {
                assert!(x.is_finite() && y.is_finite(), "{name} has a NaN point");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Connectors: line-like, never a filled outline
// ---------------------------------------------------------------------------

#[test]
fn connectors_are_open_unfilled_polylines() {
    // presets.json `straightConnector1` / `bentConnector3` declare
    // `<a:path fill="none">`; painting them with the shape fill would turn a
    // connector into a slab, so the dispatcher must preserve the mode.
    let (subpaths, warned) = dispatch("straightConnector1", 200.0, 120.0, &[]);
    assert!(!warned);
    assert_eq!(subpaths.len(), 1);
    assert_eq!(subpaths[0].fill, PathFill::None);
    assert!(subpaths[0].stroke);
    assert_eq!(
        endpoints(&subpaths[0].path),
        vec![(0.0, 0.0), (200.0, 120.0)],
        "straightConnector1 is one diagonal line"
    );
    assert!(
        !subpaths[0]
            .path
            .segments
            .iter()
            .any(|segment| matches!(segment, PathSegment::Close)),
        "a connector must stay open"
    );

    // bentConnector3 with the default adj1 = 50000 bends at x = w/2.
    let (subpaths, _) = dispatch("bentConnector3", 200.0, 120.0, &[]);
    assert_eq!(subpaths[0].fill, PathFill::None);
    assert_eq!(
        endpoints(&subpaths[0].path),
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 120.0), (200.0, 120.0)]
    );

    // The single-path wrapper keeps the line instead of collapsing to nothing.
    let (tx, _rx) = unbounded();
    let merged = build_shape_path("bentConnector3", 200.0, 120.0, &[], None, &tx);
    assert_eq!(merged.segments.len(), 4, "the polyline survives the merge");
}

// ---------------------------------------------------------------------------
// Unknown names and custom geometry
// ---------------------------------------------------------------------------

#[test]
fn unknown_preset_warns_and_returns_the_bounding_rect() {
    let (tx, rx) = unbounded();
    let subpaths = build_shape_paths("futureFuturisticShape999", 80.0, 40.0, &[], None, &tx);
    assert_eq!(subpaths.len(), 1);
    assert_eq!(subpaths[0].fill, PathFill::Norm);
    assert!(subpaths[0].stroke);
    assert_eq!(
        endpoints(&subpaths[0].path),
        vec![(0.0, 0.0), (80.0, 0.0), (80.0, 40.0), (0.0, 40.0)]
    );
    match rx.try_recv() {
        Ok(WarningEvent::UnsupportedPreset { name }) => {
            assert_eq!(name, "futureFuturisticShape999");
        }
        other => panic!("expected UnsupportedPreset, got {other:?}"),
    }
}

#[test]
fn cust_geom_bypasses_the_preset_engine_unchanged() {
    // `custGeom` still replays the authored commands with unit-normalized
    // coordinates scaled by the element box (unchanged behaviour).
    let paths = vec![vec![
        PathCmd::MoveTo { x: 0.0, y: 0.0 },
        PathCmd::LineTo { x: 0.5, y: 1.0 },
        PathCmd::LineTo { x: 1.0, y: 0.0 },
        PathCmd::Close,
    ]];
    let (tx, rx) = unbounded();
    let subpaths = build_shape_paths("custGeom", 100.0, 40.0, &[], Some(&paths), &tx);
    assert!(rx.try_recv().is_err(), "custGeom must not warn");
    assert_eq!(subpaths.len(), 1);
    assert_eq!(subpaths[0].fill, PathFill::Norm);
    assert!(subpaths[0].stroke);
    assert_eq!(
        subpaths[0].path.segments,
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 40.0 },
            PathSegment::LineTo { x: 100.0, y: 0.0 },
            PathSegment::Close,
        ]
    );
    assert_eq!(subpaths[0].path.fill_rule, FillRule::NonZero);
}

// ---------------------------------------------------------------------------
// Robustness and fill-modifier tinting
// ---------------------------------------------------------------------------

#[test]
fn non_finite_adjust_degrades_to_the_preset_default() {
    // A NaN handle must not poison the guides; the preset default is used.
    let (subpaths, warned) = dispatch("hexagon", 300.0, 150.0, &[Some(f64::NAN)]);
    assert!(!warned);
    assert_close(endpoints(&subpaths[0].path)[1], (37.5, 0.0));
}

#[test]
fn non_finite_geometry_degrades_to_the_placeholder() {
    // halfframe computes `h * x1 / w` (0/0) in a 0×0 box; the reference draws
    // nothing, so the dispatcher emits the placeholder instead of NaN.
    let (tx, rx) = unbounded();
    let subpaths = build_shape_paths("halfFrame", 0.0, 0.0, &[], None, &tx);
    assert_eq!(subpaths[0].path.segments.len(), 5);
    for (x, y) in endpoints(&subpaths[0].path) {
        assert!(x.is_finite() && y.is_finite());
    }
    match rx.try_recv() {
        Ok(WarningEvent::FidelityDegraded { feature, .. }) => {
            assert_eq!(feature, "preset_geometry_nonfinite");
        }
        other => panic!("expected FidelityDegraded, got {other:?}"),
    }
}

#[test]
fn fill_modes_tint_solid_paints_like_the_reference() {
    // ref `geometry.ts:349-399` `pathFillModeOverlay`: black-over at 102/255 for
    // darken, white-over at 102/255 for lighten, 50/255 for the "less" modes.
    let solid = |rgb: [u8; 3]| VectorPaint::Solid([rgb[0], rgb[1], rgb[2], 255]);
    let tint = |rgb: [u8; 3], mode: PathFill| match apply_path_fill_mode(Some(&solid(rgb)), mode) {
        Some(VectorPaint::Solid(color)) => color,
        other => panic!("expected a solid tint, got {other:?}"),
    };

    // 808080 darken -> 4D4D4D, FFFFFF darken -> 999999.
    assert_eq!(tint([0x80, 0x80, 0x80], PathFill::Darken), [0x4D, 0x4D, 0x4D, 255]);
    assert_eq!(tint([0xFF, 0xFF, 0xFF], PathFill::Darken), [0x99, 0x99, 0x99, 255]);
    // 808080 darkenLess -> 676767, FFFFFF darkenLess -> CDCDCD.
    assert_eq!(
        tint([0x80, 0x80, 0x80], PathFill::DarkenLess),
        [0x67, 0x67, 0x67, 255]
    );
    assert_eq!(
        tint([0xFF, 0xFF, 0xFF], PathFill::DarkenLess),
        [0xCD, 0xCD, 0xCD, 255]
    );
    // 000000 lighten -> 666666, 000000 lightenLess -> 323232.
    assert_eq!(tint([0, 0, 0], PathFill::Lighten), [0x66, 0x66, 0x66, 255]);
    assert_eq!(tint([0, 0, 0], PathFill::LightenLess), [0x32, 0x32, 0x32, 255]);
    // Norm is a pass-through, and alpha is never touched.
    assert_eq!(
        tint([0x11, 0x22, 0x33], PathFill::Norm),
        [0x11, 0x22, 0x33, 255]
    );
    // A translucent base fill takes the overlay's alpha too: the folded colour
    // is the two-pass composite (white at 102/255 over [1,2,3] at 128/255).
    assert_eq!(
        apply_path_fill_mode(Some(&VectorPaint::Solid([1, 2, 3, 128])), PathFill::Lighten),
        Some(VectorPaint::Solid([146, 146, 147, 179]))
    );
    // `<a:path fill="none">` drops the fill entirely.
    assert_eq!(apply_path_fill_mode(Some(&solid([1, 2, 3])), PathFill::None), None);
    assert_eq!(apply_path_fill_mode(None, PathFill::Norm), None);

    // Gradients cannot carry a tinted stop list here, so they stay untinted.
    let gradient = VectorPaint::LinearGradient(VectorGradient {
        angle_deg: 90.0,
        stops: vec![(0.0, [1, 2, 3, 255]), (1.0, [4, 5, 6, 255])],
    });
    assert_eq!(
        apply_path_fill_mode(Some(&gradient), PathFill::Darken),
        Some(gradient)
    );
}

// ---------------------------------------------------------------------------
// Display-list emission (compiler shape arm)
// ---------------------------------------------------------------------------

/// `DrawPath` payloads of a compiled slide, in emission order.
fn draw_paths(slide: &kitty_pptx_core::contracts::VectorSlide) -> Vec<&kitty_pptx_core::contracts::VectorCommand> {
    slide
        .commands
        .iter()
        .filter(|command| {
            matches!(command, kitty_pptx_core::contracts::VectorCommand::DrawPath { .. })
        })
        .collect()
}

#[test]
fn compiler_emits_one_draw_path_per_subpath_with_tinted_fills() {
    use kitty_pptx_core::contracts::{ShapeEffects, VectorCommand};
    use kitty_pptx_core::{DefaultSlideCompiler, SlideCompiler};

    // presets.json `cube` has four paths: norm, darkenLess, lightenLess, none.
    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "elements": [{
            "type": "shape",
            "x": 0,
            "y": 0,
            "width": 300000,
            "height": 200000,
            "rotation": 0.0,
            "flipH": false,
            "flipV": false,
            "geometry": "cube",
            "fill": { "fillType": "solid", "color": "4472C4" },
            "shadow": {
                "color": "000000",
                "alpha": 0.4,
                "blur": 38100,
                "dist": 25400,
                "dir": 45.0,
                "sx": 1.0,
                "sy": 1.0,
                "kx": 0.0,
                "ky": 0.0,
                "rotWithShape": true
            },
            "stroke": { "color": "000000", "width": 12700, "dashStyle": "solid",
                        "lineCap": "flat", "lineJoin": "miter" }
        }],
        "elementSources": [],
        "comments": []
    });
    let slide: pptx_model::Slide =
        serde_json::from_value(slide_json).expect("deserialize slide");
    let (tx, rx) = unbounded();
    let vector_slide = DefaultSlideCompiler::new()
        .compile(&slide, 9144000, 6858000, &tx)
        .expect("compile");
    assert!(rx.try_recv().is_err(), "cube must compile without warnings");

    let draws = draw_paths(&vector_slide);
    assert_eq!(draws.len(), 4, "one DrawPath per <a:path>");

    let parts: Vec<(Option<VectorPaint>, bool, bool)> = draws
        .iter()
        .map(|command| match command {
            VectorCommand::DrawPath {
                fill,
                stroke,
                effects,
                ..
            } => (fill.clone(), stroke.is_some(), **effects != ShapeEffects::default()),
            _ => unreachable!("filtered to DrawPath"),
        })
        .collect();

    // 0x44·205/255 = 55 (0x37), 0x72·205/255 = 92 (0x5C), 0xC4·205/255 = 158 (0x9E)
    // 255-(255-0x44)·205/255 = 105 (0x69), ... = 142 (0x8E), ... = 208 (0xD0)
    assert_eq!(
        parts[0].0,
        Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])),
        "norm keeps the shape fill"
    );
    assert_eq!(
        parts[1].0,
        Some(VectorPaint::Solid([0x37, 0x5C, 0x9E, 255])),
        "darkenLess"
    );
    assert_eq!(
        parts[2].0,
        Some(VectorPaint::Solid([0x69, 0x8E, 0xD0, 255])),
        "lightenLess"
    );
    assert_eq!(parts[3].0, None, "fill=\"none\" paints no fill");

    // Only the trailing `<a:path stroke="1">` outline takes the shape stroke.
    assert_eq!(
        parts.iter().map(|part| part.1).collect::<Vec<_>>(),
        vec![false, false, false, true]
    );
    // Effects ride on the first painted subpath only (ref index.ts:71-77).
    assert_eq!(
        parts.iter().map(|part| part.2).collect::<Vec<_>>(),
        vec![true, false, false, false]
    );
}

#[test]
fn donut_rings_wind_oppositely() {
    // presets.json `donut` ships both rings in one `<a:path>`: the outer sweep
    // is +cd4 per quarter, the inner one -5400000. Nonzero is therefore what
    // cuts the hole (the reference fills with `ctx.fill()`'s default rule).
    let (subpaths, _) = dispatch("donut", 200.0, 120.0, &[]);
    assert_eq!(subpaths.len(), 1);

    let mut contours: Vec<Vec<(f32, f32)>> = Vec::new();
    for segment in &subpaths[0].path.segments {
        match segment {
            PathSegment::MoveTo { x, y } => contours.push(vec![(*x, *y)]),
            PathSegment::LineTo { x, y }
            | PathSegment::QuadTo { x, y, .. }
            | PathSegment::CubicTo { x, y, .. } => {
                if let Some(last) = contours.last_mut() {
                    last.push((*x, *y));
                }
            }
            PathSegment::Close => {}
        }
    }
    assert_eq!(contours.len(), 2, "donut has an outer and an inner ring");

    let signed_area = |points: &[(f32, f32)]| -> f32 {
        let mut total = 0.0;
        for (index, (x0, y0)) in points.iter().enumerate() {
            let (x1, y1) = points[(index + 1) % points.len()];
            total += x0 * y1 - x1 * y0;
        }
        total / 2.0
    };
    let (outer, inner) = (signed_area(&contours[0]), signed_area(&contours[1]));
    assert!(
        outer * inner < 0.0,
        "rings must wind oppositely (outer {outer}, inner {inner})"
    );
}
