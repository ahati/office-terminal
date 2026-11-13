//! PRD 11 §5.3 boundary invariant tests for the extended preset library.
//!
//! Every promoted preset is exercised through the public dispatcher to make
//! sure it (a) resolves before the Tier-3 fallback, (b) produces a valid path,
//! (c) keeps its path points inside the element bounds and (d) is finite for
//! degenerate boxes. Callout presets are exempt from the bounds check because
//! their pointer targets legitimately live outside the element.

use crossbeam_channel::unbounded;
use kitty_pptx_core::compiler::shapes::{
    self, presets_arrows, presets_arrows_curved, presets_buttons, presets_callouts, presets_rects,
    presets_symbols, presets_symbols_extra,
};
use kitty_pptx_core::contracts::{FillRule, PathSegment};

const RECT_PRESETS: &[&str] = &[
    "snip1Rect",
    "snip2SameRect",
    "snip2DiagRect",
    "snipRoundRect",
    "round1Rect",
    "round2SameRect",
    "round2DiagRect",
    "halfFrame",
    "corner",
    "cornerTabs",
    "diagStripe",
    "plaque",
    "plaqueTabs",
    "squareTabs",
    "frame",
    "bevel",
    // ECMA-376 `foldedCorner` (the old hand-written library also accepted the
    // misspelling `folderCorner`, which the dispatcher now treats as unknown).
    "foldedCorner",
    "plus",
    "mathMultiply",
    "mathDivide",
    "mathEqual",
    "mathNotEqual",
];

const ARROW_PRESETS: &[&str] = &[
    "notchedRightArrow",
    "stripedRightArrow",
    "quadArrow",
    "leftRightUpArrow",
    "leftUpArrow",
    "upDownArrow",
    "bentArrow",
    "bentUpArrow",
    "uturnArrow",
    "circularArrow",
    "leftCircularArrow",
    "leftRightCircularArrow",
];

const CURVED_ARROW_PRESETS: &[&str] = &[
    "curvedRightArrow",
    "curvedLeftArrow",
    "curvedUpArrow",
    "curvedDownArrow",
];

const CALLOUT_PRESETS: &[&str] = &[
    "wedgeRectCallout",
    "wedgeRoundRectCallout",
    "cloudCallout",
    "wedgeEllipseCallout",
    "borderCallout1",
    "borderCallout2",
    "borderCallout3",
    "callout1",
    "callout2",
    "callout3",
    "accentCallout1",
    "accentCallout2",
    "accentCallout3",
    "accentBorderCallout1",
    "accentBorderCallout2",
    "accentBorderCallout3",
    "leftArrowCallout",
    "rightArrowCallout",
    "upArrowCallout",
    "downArrowCallout",
    "leftRightArrowCallout",
    "upDownArrowCallout",
    "quadArrowCallout",
];

const SYMBOL_PRESETS: &[&str] = &[
    "star4",
    "star7",
    "star10",
    "star12",
    "star16",
    "star24",
    "star32",
    "gear6",
    "gear9",
    "teardrop",
    "funnel",
    "leftBrace",
    "rightBrace",
    "bracePair",
    "leftBracket",
    "rightBracket",
    "bracketPair",
    "heptagon",
    "decagon",
    "dodecagon",
    "rtTriangle",
    "blockArc",
    "donut",
    "noSmoking",
    "pie",
    "pieWedge",
    "chord",
    "arc",
];

const SYMBOL_EXTRA_PRESETS: &[&str] = &[
    "sun",
    "moon",
    "smileyFace",
    "wave",
    "doubleWave",
    "irregularSeal1",
    "irregularSeal2",
    "ribbon",
    "ribbon2",
    "ellipseRibbon",
    "ellipseRibbon2",
    "leftRightRibbon",
    "verticalScroll",
    "horizontalScroll",
];

const BUTTON_PRESETS: &[&str] = &[
    "actionButtonBackPrevious",
    "actionButtonForwardNext",
    "actionButtonBeginning",
    "actionButtonEnd",
    "actionButtonHome",
    "actionButtonHelp",
    "actionButtonInformation",
    "actionButtonMovie",
    "actionButtonSound",
    "actionButtonBlank",
    "actionButtonDocument",
    "actionButtonReturn",
];

fn all_presets() -> Vec<&'static str> {
    let mut names = Vec::new();
    names.extend_from_slice(RECT_PRESETS);
    names.extend_from_slice(ARROW_PRESETS);
    names.extend_from_slice(CURVED_ARROW_PRESETS);
    names.extend_from_slice(CALLOUT_PRESETS);
    names.extend_from_slice(SYMBOL_PRESETS);
    names.extend_from_slice(SYMBOL_EXTRA_PRESETS);
    names.extend_from_slice(BUTTON_PRESETS);
    names
}

fn is_callout(name: &str) -> bool {
    name.to_ascii_lowercase().contains("callout")
}

fn is_open_leader(name: &str) -> bool {
    name.starts_with("lineCallout")
        || name.starts_with("borderCallout")
        || name.starts_with("accentCallout")
        || name.starts_with("accentBorderCallout")
        || matches!(name, "callout1" | "callout2" | "callout3")
}

fn is_open_path(name: &str) -> bool {
    is_open_leader(name) || name == "arc"
}

/// End points of every path segment (control points excluded).
fn endpoints(path: &kitty_pptx_core::contracts::VectorPath) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => {
                out.push((*x, *y));
            }
            PathSegment::QuadTo { x, y, .. } | PathSegment::CubicTo { x, y, .. } => {
                out.push((*x, *y));
            }
            PathSegment::Close => {}
        }
    }
    out
}

fn control_points(path: &kitty_pptx_core::contracts::VectorPath) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for seg in &path.segments {
        match seg {
            PathSegment::QuadTo { x1, y1, .. } => out.push((*x1, *y1)),
            PathSegment::CubicTo { x1, y1, x2, y2, .. } => {
                out.push((*x1, *y1));
                out.push((*x2, *y2));
            }
            _ => {}
        }
    }
    out
}

#[test]
fn promoted_preset_count_matches_catalog() {
    let names = all_presets();
    // `rectCallout` / `roundRectCallout` / `ellipseCallout` (Office-2003
    // spellings) and `lineCallout1..4` are absent from `presets.json`, which is
    // a verbatim `presetShapeDefinitions.xml`; the ECMA names for those are
    // `wedge*Callout` and `callout1..3`. They are covered by the unknown-name
    // fallback test instead.
    assert_eq!(names.len(), 115, "unexpected promotion count");
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "duplicate preset names");
}

#[test]
fn every_module_builder_accepts_its_catalog() {
    for name in RECT_PRESETS {
        assert!(
            presets_rects::build_rect_preset(name, 200.0, 120.0, &[]).is_some()
                || shapes::presets_t2::build_t2_preset(name, 200.0, 120.0, &[]).is_some(),
            "rect builder missed {name}"
        );
    }
    for name in ARROW_PRESETS {
        assert!(
            presets_arrows::build_arrow_preset(name, 200.0, 120.0, &[]).is_some(),
            "arrow builder missed {name}"
        );
    }
    for name in CURVED_ARROW_PRESETS {
        assert!(
            presets_arrows_curved::build_curved_arrow_preset(name, 200.0, 120.0, &[]).is_some(),
            "curved arrow builder missed {name}"
        );
    }
    for name in CALLOUT_PRESETS {
        assert!(
            presets_callouts::build_callout_preset(name, 200.0, 120.0, &[]).is_some()
                || shapes::presets_arrow_callouts::build_arrow_callout_preset(
                    name,
                    200.0,
                    120.0,
                    &[]
                )
                .is_some(),
            "callout builder missed {name}"
        );
    }
    for name in SYMBOL_PRESETS {
        assert!(
            presets_symbols::build_symbol_preset(name, 200.0, 120.0, &[]).is_some(),
            "symbol builder missed {name}"
        );
    }
    for name in SYMBOL_EXTRA_PRESETS {
        assert!(
            presets_symbols_extra::build_symbol_extra_preset(name, 200.0, 120.0, &[]).is_some(),
            "symbol extra builder missed {name}"
        );
    }
    for name in BUTTON_PRESETS {
        assert!(
            presets_buttons::build_button_preset(name, 200.0, 120.0, &[]).is_some(),
            "button builder missed {name}"
        );
    }
}

#[test]
fn extended_presets_dispatch_without_warning() {
    for name in all_presets() {
        let (tx, rx) = unbounded();
        let subpaths = shapes::build_shape_paths(name, 200.0, 120.0, &[], None, &tx);
        assert!(
            rx.try_recv().is_err(),
            "{name} must not emit UnsupportedPreset"
        );
        assert!(!subpaths.is_empty(), "{name} produced no subpath");
        let segments: usize = subpaths.iter().map(|s| s.path.segments.len()).sum();
        let min_segments = if is_open_leader(name) { 2 } else { 3 };
        assert!(
            segments >= min_segments,
            "{name} produced only {segments} segments"
        );
        assert!(
            matches!(subpaths[0].path.segments[0], PathSegment::MoveTo { .. }),
            "{name} must start with MoveTo"
        );
        // Multi-contour presets (smileyFace's eyes, sun's rays) close contour by
        // contour, so require at least one `Close` rather than a trailing one.
        if !is_open_path(name) {
            assert!(
                subpaths
                    .iter()
                    .any(|s| matches!(s.path.segments.last(), Some(PathSegment::Close))),
                "{name} must be closed"
            );
        }
    }
}

#[test]
fn extended_presets_stay_within_bounds() {
    let boxes = [
        (200.0_f32, 120.0_f32),
        (40.0, 200.0),
        (120.0, 40.0),
        (64.0, 64.0),
    ];
    for name in all_presets() {
        // Callout leaders target a point that is deliberately outside the
        // element box (ECMA-376 §20.1.10.31 callout adj1/adj2).
        if is_callout(name) {
            continue;
        }
        // The ECMA `math*` definitions draw the operator as thick strokes whose
        // contours derive from the box; for extreme aspect ratios (40x200) the
        // `mathNotEqual` slash legitimately leaves the box. PowerPoint clips at
        // paint time and the reference renderer does not clip either, so this
        // is reference behaviour rather than a regression.
        if name.to_ascii_lowercase().starts_with("math") {
            continue;
        }
        for &(w, h) in &boxes {
            let (tx, _rx) = unbounded();
            let path = shapes::build_shape_path(name, w, h, &[], None, &tx);
            // The ECMA guide tables carry 5-decimal cosines, so a computed
            // vertex can sit a hair outside the box (heptagon/star7 at 40x200
            // overshoot by 1.05e-3 px on the bottom edge).
            let eps = 1e-2;
            for (x, y) in endpoints(&path) {
                assert!(
                    x.is_finite() && y.is_finite(),
                    "{name} has non-finite point"
                );
                assert!(
                    x >= -eps && x <= w + eps,
                    "{name} x={x} outside [0,{w}] for box {w}x{h}"
                );
                assert!(
                    y >= -eps && y <= h + eps,
                    "{name} y={y} outside [0,{h}] for box {w}x{h}"
                );
            }
            // `wave`/`doubleWave` place their control points at y1 ± 10/3·y1
            // (ECMA `dy2`), which is far outside the box by construction even
            // though the traced curve stays inside; skip the control check.
            if matches!(name, "wave" | "doubleWave") {
                continue;
            }
            // Bézier control points may sit marginally outside an inscribed
            // ellipse (the 45° sub-arc control radius is ~1.03R); allow 5%.
            for (x, y) in control_points(&path) {
                assert!(x.is_finite() && y.is_finite(), "{name} control non-finite");
                assert!(
                    x >= -0.05 * w - 1e-3 && x <= 1.05 * w + 1e-3,
                    "{name} control x={x} far outside box {w}x{h}"
                );
                assert!(
                    y >= -0.05 * h - 1e-3 && y <= 1.05 * h + 1e-3,
                    "{name} control y={y} far outside box {w}x{h}"
                );
            }
        }
    }
}

#[test]
fn degenerate_boxes_are_finite() {
    for name in all_presets() {
        for &(w, h) in &[(0.0_f32, 0.0_f32), (0.0, 50.0), (50.0, 0.0), (1.0, 1.0)] {
            let (tx, _rx) = unbounded();
            let path = shapes::build_shape_path(name, w, h, &[Some(f64::NAN)], None, &tx);
            for (x, y) in endpoints(&path).into_iter().chain(control_points(&path)) {
                assert!(
                    x.is_finite() && y.is_finite(),
                    "{name} produced non-finite geometry for {w}x{h}"
                );
            }
        }
    }
}

#[test]
fn extreme_adjustments_are_clamped() {
    let extremes = [
        Some(-1_000_000.0_f64),
        Some(1_000_000.0),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ];
    for name in all_presets() {
        for value in extremes {
            let (tx, _rx) = unbounded();
            let path = shapes::build_shape_path(
                name,
                200.0,
                120.0,
                &[value, value, value, value, value, value, value, value],
                None,
                &tx,
            );
            for (x, y) in endpoints(&path).into_iter().chain(control_points(&path)) {
                assert!(x.is_finite() && y.is_finite(), "{name} NaN for {value:?}");
            }
        }
    }
}

#[test]
fn snip1_rect_actually_cuts_the_corner() {
    let (tx, _rx) = unbounded();
    let path = shapes::build_shape_path("snip1Rect", 200.0, 120.0, &[], None, &tx);
    let pts = endpoints(&path);
    assert!(
        !pts.iter()
            .any(|&(x, y)| (x - 200.0).abs() < 1e-3 && y.abs() < 1e-3),
        "top-right corner must be snipped away"
    );
    // Default cut: dx1 = ss * adj / 100000 = 120 * 16667 / 100000 = 20.0004.
    let cut = 120.0 * 16667.0 / 100_000.0;
    assert!(pts
        .iter()
        .any(|&(x, y)| (x - (200.0 - cut)).abs() < 1e-2 && y.abs() < 1e-2));
    assert!(pts
        .iter()
        .any(|&(x, y)| (x - 200.0).abs() < 1e-2 && (y - cut).abs() < 1e-2));
}

#[test]
fn adjustment_sensitivity_changes_geometry() {
    let cases: &[(&str, f64, f64)] = &[
        ("snip1Rect", 5_000.0, 90_000.0),
        ("round2DiagRect", 5_000.0, 90_000.0),
        ("star4", 20_000.0, 80_000.0),
        ("star5", 20_000.0, 80_000.0),
        ("wedgeRectCallout", 20_000.0, 80_000.0),
        ("star32", 30_000.0, 70_000.0),
        ("gear6", 10_000.0, 50_000.0),
        ("notchedRightArrow", 10_000.0, 80_000.0),
        ("leftBrace", 10_000.0, 60_000.0),
        ("plaque", 5_000.0, 80_000.0),
        ("curvedRightArrow", 20_000.0, 60_000.0),
    ];
    for &(name, low, high) in cases {
        let (tx, _rx) = unbounded();
        let a = shapes::build_shape_path(name, 200.0, 120.0, &[Some(low)], None, &tx);
        let b = shapes::build_shape_path(name, 200.0, 120.0, &[Some(high)], None, &tx);
        assert_ne!(a.segments, b.segments, "{name} ignored its adjustment");
    }
}

#[test]
fn preset_paths_use_the_nonzero_fill_rule() {
    // The reference paints every preset `<a:path>` with `ctx.fill()`'s default
    // nonzero rule (`preset-geometry/index.ts:64-70`); holes such as `donut`'s
    // inner ring are authored with the opposite winding instead, so no even-odd
    // rule is needed (see `preset_dispatch_tests::donut_rings_wind_oppositely`).
    for name in [
        "stripedRightArrow",
        "gear6",
        "gear9",
        "frame",
        "bevel",
        "donut",
    ] {
        let (tx, _rx) = unbounded();
        let path = shapes::build_shape_path(name, 200.0, 120.0, &[], None, &tx);
        assert_eq!(path.fill_rule, FillRule::NonZero, "{name}");
    }
}

#[test]
fn brace_pair_composes_fill_and_outline_paths() {
    // presets.json `bracePair` / `bracketPair` each ship two `<a:path>`s: one
    // closed fill contour plus a stroke-only outline that draws both halves.
    for name in ["bracePair", "bracketPair"] {
        let (tx, _rx) = unbounded();
        let subpaths = shapes::build_shape_paths(name, 200.0, 120.0, &[], None, &tx);
        assert_eq!(subpaths.len(), 2, "{name} fill + outline paths");
        assert!(subpaths[0].fill.is_filled(), "{name} first path carries the fill");
        assert!(!subpaths[0].stroke, "{name} fill path is not stroked");
        assert_eq!(
            subpaths[1].fill,
            shapes::PathFill::None,
            "{name} outline path is stroke-only"
        );
        assert!(subpaths[1].stroke);
        let moves = subpaths[1]
            .path
            .segments
            .iter()
            .filter(|segment| matches!(segment, PathSegment::MoveTo { .. }))
            .count();
        assert_eq!(moves, 2, "{name} outline composes two halves");
    }
}

#[test]
fn unknown_presets_still_fall_back() {
    // `folderCorner` is a misspelling of the ECMA `foldedCorner`; presets.json
    // only ships the standard name, so it degrades like any unknown geometry.
    for name in [
        "futureFuturisticShape999",
        "folderCorner",
        "rectCallout",
        "roundRectCallout",
        "ellipseCallout",
        "lineCallout1",
        "lineCallout4",
    ] {
        let (tx, rx) = unbounded();
        let path = shapes::build_shape_path(name, 200.0, 120.0, &[], None, &tx);
        assert_eq!(path.segments.len(), 5, "{name} fallback rect");
        match rx.try_recv() {
            Ok(kitty_pptx_core::contracts::WarningEvent::UnsupportedPreset { name: got }) => {
                assert_eq!(got, name);
            }
            other => panic!("{name} expected UnsupportedPreset, got {other:?}"),
        }
    }
}
