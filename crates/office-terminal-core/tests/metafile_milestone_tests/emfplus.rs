//! PRD 13 milestone 4: EMF+ playback, paints and object-table commits.

use office_terminal_core::contracts::{VectorCommand, VectorPaint, WarningEvent};
use office_terminal_core::metafile::{mapping, MetafilePaint, MetafileTextRun, TextAlignH, TextAlignV};
use office_terminal_core::SlideRasterizer;

use crate::helpers::{expand, text_runs, EMF};
use crate::metafile_support as support;

// ── M4: EMF+ / paints ──────────────────────────────────────────────────────

#[test]
fn emfplus_dual_getdc_fallback_is_reported() {
    let records = [
        support::rectangle([10, 10, 120, 80]),
        support::emf_plus_comment(true, &[support::emf_plus_record(0x4004, 0, &[])]),
    ];
    let (scene, summary, warnings) = expand("ppt/media/image1.emf", EMF, support::emf(&records));

    assert_eq!(summary.expanded, 1, "classic records must still play");
    assert_eq!(summary.placeholders, 0);
    assert!(scene
        .commands
        .iter()
        .any(|command| matches!(command, VectorCommand::DrawPath { .. })));
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            WarningEvent::FidelityDegraded {
                feature: "metafile_emfplus_fallback",
                ..
            }
        )),
        "dual GetDC fallback must emit its diagnostic: {warnings:?}"
    );
}

#[test]
fn hatched_brush_keeps_ink_and_reports_degradation() {
    let mut brush = Vec::new();
    brush.extend_from_slice(&2u32.to_le_bytes()); // handle
    brush.extend_from_slice(&2u32.to_le_bytes()); // BS_HATCHED
    brush.extend_from_slice(&0x0000_00FFu32.to_le_bytes()); // red
    brush.extend_from_slice(&0u32.to_le_bytes()); // HS_HORIZONTAL
    let records = [
        support::record(39, &brush),
        support::select_object(2),
        support::rectangle([10, 10, 120, 80]),
    ];
    let (scene, summary, warnings) = expand("ppt/media/image1.emf", EMF, support::emf(&records));

    assert_eq!(summary.expanded, 1);
    let pattern = scene.commands.iter().find_map(|command| match command {
        VectorCommand::DrawPath {
            fill: Some(VectorPaint::Pattern(pattern)),
            ..
        } => Some(pattern.clone()),
        _ => None,
    });
    let pattern = pattern.expect("hatched brush must paint a pattern fill");
    assert_eq!(pattern.preset, "horz");
    assert_eq!(pattern.fg, [255, 0, 0, 255]);
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            WarningEvent::FidelityDegraded {
                feature: "metafile_hatch",
                ..
            }
        )),
        "hatch approximation must be reported: {warnings:?}"
    );
}

#[test]
fn linear_gradient_paint_maps_to_the_display_list() {
    let paint = MetafilePaint::LinearGradient {
        from: (0.0, 0.0),
        to: (10.0, 0.0),
        stops: vec![(0.0, [255, 0, 0, 255]), (1.0, [0, 0, 255, 255])],
    };
    match mapping::paint::map_paint(&paint, &office_terminal_core::contracts::AffineTransform::identity()) {
        Some(VectorPaint::LinearGradient(gradient)) => {
            assert_eq!(gradient.stops.len(), 2);
            assert!(gradient.angle_deg.abs() < 1.0e-3, "horizontal axis");
        }
        other => panic!("expected a linear gradient, got {other:?}"),
    }

    let vertical = MetafilePaint::LinearGradient {
        from: (0.0, 0.0),
        to: (0.0, 10.0),
        stops: vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])],
    };
    match mapping::paint::map_paint(&vertical, &office_terminal_core::contracts::AffineTransform::identity()) {
        Some(VectorPaint::LinearGradient(gradient)) => {
            assert!((gradient.angle_deg - 90.0).abs() < 1.0e-3);
        }
        other => panic!("expected a linear gradient, got {other:?}"),
    }
}

#[test]
fn emfplus_text_anchor_follows_the_world_transform() {
    // EMF+ positions DrawString in page coordinates and delivers the world /
    // container transform separately. The same anchor expressed either through
    // the transform or in the layout rect must land at the same place.
    let records = [
        support::emf_plus_font(1, 12.0, 3, 0, "Carlito"),
        support::emf_plus_solid_brush(2, 0xff00_0000),
        support::emf_plus_world_transform([1.0, 0.0, 0.0, 1.0, 100.0, 50.0]),
        support::emf_plus_draw_string(1, 2, u32::MAX, "A", [0.0, 0.0, 0.0, 0.0]),
        support::emf_plus_world_transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        support::emf_plus_draw_string(1, 2, u32::MAX, "A", [100.0, 50.0, 0.0, 0.0]),
    ];
    let (scene, summary, warnings) = expand(
        "ppt/media/image1.emf",
        EMF,
        support::emf_plus_only_emf(&records),
    );

    assert_eq!(summary.expanded, 1, "EMF+ text must play: {warnings:?}");
    let runs = text_runs(&scene);
    assert_eq!(runs.len(), 2, "both DrawString records must emit runs");
    let transformed = runs[0].glyphs.first().copied().expect("transformed glyph");
    let authored = runs[1].glyphs.first().copied().expect("authored glyph");
    assert!(
        (transformed.x - authored.x).abs() < 1.0 && (transformed.y - authored.y).abs() < 1.0,
        "world-transformed anchor {transformed:?} must equal authored anchor {authored:?}"
    );
}

#[test]
fn emfplus_rotated_text_rotates_about_the_anchor() {
    let run = MetafileTextRun {
        text: "A".into(),
        glyph_indices: None,
        family: "Carlito".into(),
        size: 12.0,
        color: [0, 0, 0, 255],
        bold: false,
        italic: false,
        underline: false,
        strike_out: false,
        x: 10.0,
        y: 20.0,
        escapement_deg: 0.0,
        dx: Vec::new(),
        horizontal_align: TextAlignH::Left,
        vertical_align: TextAlignV::Top,
        background: None,
        background_rect: None,
        // Quarter turn: local +x maps to output +y.
        transform: office_terminal_core::contracts::AffineTransform {
            sx: 0.0,
            ky: 1.0,
            kx: -1.0,
            sy: 0.0,
            tx: 100.0,
            ty: 50.0,
        },
        right_to_left: false,
    };
    let scale = 9525.0;
    let mapped = mapping::text::map_text_run(
        &run,
        &office_terminal_core::contracts::AffineTransform::scale(scale, scale),
    )
    .expect("mapped");
    assert!(
        mapped.transform.kx < -0.99 && mapped.transform.ky > 0.99,
        "expected a quarter-turn transform, got {:?}",
        mapped.transform
    );
    // Anchor: T(10,20) = (80,60), scaled to (762000,571500). Glyphs stay in
    // unrotated local space; the command transform rotates them about the
    // anchor, so the baseline offset remains on +y before rendering.
    let (anchor_x, anchor_y) = (80.0 * scale, 60.0 * scale);
    let glyph = mapped.runs[0].glyphs.first().copied().expect("glyph");
    assert!(
        (glyph.x - anchor_x).abs() < 1.0,
        "left-aligned glyph must sit on the anchor x: {glyph:?}"
    );
    assert!(
        glyph.y > anchor_y + 1.0,
        "baseline offset must stay on +y before the transform: {glyph:?}"
    );
}

#[test]
fn emfplus_split_object_commits_at_the_declared_total() {
    // MS-EMFPLUS makes TotalObjectSize authoritative, but some producers keep
    // the continuation bit set on the final fragment. The object must enter the
    // table as soon as the declared bytes are read, or the next draw record
    // sees an empty object slot.
    let path = support::emf_plus_path_data(&[(10, 10), (110, 10), (110, 90), (10, 90)], true);
    let total = path.len();
    let split = 16;
    assert!(path.len() > split && split.is_multiple_of(4));
    let records = [
        support::emf_plus_pen(3, 0xff26_2626, 1.0, 0, 0),
        support::emf_plus_split_object(4, 3, total, &path[..split], true),
        support::emf_plus_split_object(4, 3, total, &path[split..], true),
        support::emf_plus_draw_path(4, 3),
    ];
    let (scene, summary, warnings) = expand(
        "ppt/media/image1.emf",
        EMF,
        support::emf_plus_only_emf(&records),
    );

    assert_eq!(summary.expanded, 1, "split object must play: {warnings:?}");
    assert_eq!(summary.placeholders, 0);
    let stroked = scene.commands.iter().any(|command| {
        matches!(
            command,
            VectorCommand::DrawPath {
                stroke: Some(_),
                fill: None,
                ..
            }
        )
    });
    assert!(stroked, "draw record after the split object must stroke");
}

#[test]
fn path_gradient_brush_is_approximated_not_dropped() {
    // metafile-emfplus has no radial/path-gradient paint; the brush must still
    // paint (linear approximation) instead of replacing the whole picture with
    // the placeholder (patch 0007).
    let records = [
        support::emf_plus_path_gradient_brush(
            2,
            0xffff_0000,
            &[0xff00_00ff, 0xff00_ff00],
            None,
        ),
        support::emf_plus_fill_rects(2, [10.0, 10.0, 110.0, 90.0]),
    ];
    let (scene, summary, warnings) = expand(
        "ppt/media/image1.emf",
        EMF,
        support::emf_plus_only_emf(&records),
    );

    assert_eq!(summary.expanded, 1, "brush must not abort playback: {warnings:?}");
    assert_eq!(summary.placeholders, 0);
    let gradient = scene.commands.iter().find_map(|command| match command {
        VectorCommand::DrawPath {
            fill: Some(VectorPaint::LinearGradient(gradient)),
            ..
        } => Some(gradient.clone()),
        _ => None,
    });
    let gradient = gradient.expect("path-gradient fill must become a linear gradient");
    assert_eq!(gradient.stops.len(), 3, "two surrounding colours + centre");
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            WarningEvent::FidelityDegraded {
                feature: "metafile_gradient",
                ..
            }
        )),
        "approximation must be reported: {warnings:?}"
    );

    // Composition proof: the approximated fill must paint, not vanish.
    let store = support::FixtureStore::new(
        "ppt/media/image1.emf",
        support::emf_plus_only_emf(&records),
    );
    let target = office_terminal_core::contracts::RenderTargetSpec {
        width_px: 320,
        height_px: 160,
    };
    let bitmap = office_terminal_core::TinySkiaRasterizer::new()
        .rasterize(&scene, &target, &store)
        .expect("rasterize");
    let ink = bitmap
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel != [255, 255, 255, 255])
        .count();
    assert!(ink > 0, "path-gradient approximation must paint pixels");
}

#[test]
fn asymmetric_pen_caps_keep_the_frame_stroke() {
    // Office/GDI+ chart frames use round start caps with flat end caps. The
    // renderer-neutral stroke carries one cap; rejecting the pen would drop
    // the frame and every tick mark.
    let records = [
        support::emf_plus_pen(3, 0xff26_2626, 1.0, 1, 0),
        support::emf_plus_path(4, &[(10, 10), (110, 10), (110, 90), (10, 90)], true),
        support::emf_plus_draw_path(4, 3),
    ];
    let (scene, summary, warnings) = expand(
        "ppt/media/image1.emf",
        EMF,
        support::emf_plus_only_emf(&records),
    );

    assert_eq!(summary.expanded, 1, "frame path must play: {warnings:?}");
    assert_eq!(summary.placeholders, 0);
    let framed = scene.commands.iter().any(|command| {
        matches!(
            command,
            VectorCommand::DrawPath {
                stroke: Some(_),
                fill: None,
                ..
            }
        )
    });
    assert!(
        framed,
        "asymmetric-cap pen must still stroke the frame: {warnings:?}"
    );
}
