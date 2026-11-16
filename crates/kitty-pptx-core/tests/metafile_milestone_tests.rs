//! Milestone playback tests for PRD 13: text (M3), EMF+ (M4) and WMF/EMZ (M5).

#![cfg(feature = "metafile")]

mod metafile_support;

use kitty_pptx_core::contracts::{VectorCommand, VectorPaint, VectorSlide, WarningEvent};
use kitty_pptx_core::metafile::{
    self, mapping, MetafilePaint, MetafileTextRun, TextAlignH, TextAlignV,
};
use kitty_pptx_core::{SlideRasterizer, SlideSvgSerializer};
use metafile_support as support;

const EMF: &str = "image/x-emf";
const WMF: &str = "image/x-wmf";

fn expand(
    rel_id: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> (
    VectorSlide,
    metafile::MetafileExpansionSummary,
    Vec<WarningEvent>,
) {
    metafile::clear_global_cache();
    let (compile_tx, _compile_rx) = crossbeam_channel::unbounded();
    let mut scene = support::compile_slide(&support::picture_slide(rel_id, mime), &compile_tx);
    let store = support::FixtureStore::new(rel_id, bytes);
    let (tx, rx) = crossbeam_channel::unbounded();
    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);
    (scene, summary, rx.try_iter().collect())
}

fn text_runs(scene: &VectorSlide) -> Vec<kitty_pptx_core::contracts::PositionedGlyphRun> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn first_glyph_x(scene: &VectorSlide, text: &str) -> Option<f32> {
    text_runs(scene)
        .into_iter()
        .find(|run| run.text == text)
        .and_then(|run| run.glyphs.first().map(|glyph| glyph.x))
}

fn has_font_warning(warnings: &[WarningEvent], requested: &str, substitute: &str) -> bool {
    warnings.iter().any(|warning| {
        matches!(
            warning,
            WarningEvent::FontSubstituted { requested: r, substitute: s }
                if r == requested && s == substitute
        )
    })
}

// ── M3: text ───────────────────────────────────────────────────────────────

fn text_emf(align: u32, escapement_tenths: i32, dx: Option<&[i32]>) -> Vec<u8> {
    support::emf(&[
        support::create_font(1, 20, 700, escapement_tenths, false, true, "Arial"),
        support::select_object(1),
        support::set_text_color(0x0000_00FF),
        support::set_text_align(align),
        support::ext_text_out((100, 60), "Hi", 0, None, dx),
    ])
}

#[test]
fn emf_text_plays_with_font_substitution_and_decorations() {
    let (scene, summary, warnings) = expand("ppt/media/image1.emf", EMF, text_emf(0x0018, 0, None));

    assert_eq!(summary.expanded, 1, "text records must play");
    assert_eq!(summary.placeholders, 0);
    assert_eq!(support::markers(&scene), 0);
    let runs = text_runs(&scene);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "Hi");
    assert_eq!(runs[0].font_family, "Liberation Sans");
    assert!(runs[0].bold);
    assert!(runs[0].decorations.underline.is_some());
    assert_eq!(runs[0].color, [255, 0, 0, 255]);
    assert!(
        has_font_warning(&warnings, "Arial", "Liberation Sans"),
        "substitution must surface as FontSubstituted: {warnings:?}"
    );

    // The run must paint, not merely exist: an end-to-end raster check catches
    // baseline/transform mistakes that command assertions miss.
    let store = support::FixtureStore::new("ppt/media/image1.emf", text_emf(0x0018, 0, None));
    let target = kitty_pptx_core::contracts::RenderTargetSpec {
        width_px: 320,
        height_px: 240,
    };
    let bitmap = kitty_pptx_core::TinySkiaRasterizer::new()
        .rasterize(&scene, &target, &store)
        .expect("rasterize");
    let ink = bitmap
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel != [255, 255, 255, 255])
        .count();
    assert!(ink > 0, "shaped metafile text must paint pixels");

    // SVG must carry the same shaped run as selectable text.
    let svg = kitty_pptx_core::DefaultSlideSvgSerializer::new()
        .serialize_svg(&scene, &store, kitty_pptx_core::SvgTextMode::SelectableText)
        .expect("serialize svg");
    assert!(
        svg.contains("Hi") || (svg.contains('H') && svg.contains('i')),
        "metafile text must survive SVG export"
    );
}

#[test]
fn emf_text_alignment_offsets_the_baseline_origin() {
    let (left, ..) = expand("ppt/media/image1.emf", EMF, text_emf(0x0000, 0, None));
    let (center, ..) = expand("ppt/media/image1.emf", EMF, text_emf(0x0006, 0, None));
    let left_x = first_glyph_x(&left, "Hi").expect("left run");
    let center_x = first_glyph_x(&center, "Hi").expect("center run");
    assert!(
        center_x < left_x,
        "TA_CENTER must shift the run left of the reference point ({center_x} !< {left_x})"
    );
}

#[test]
fn emf_text_escapement_rotates_the_run() {
    let (scene, summary, _) = expand("ppt/media/image1.emf", EMF, text_emf(0x0018, 900, None));
    assert_eq!(summary.expanded, 1);
    let transform = scene
        .commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawText { transform, .. } => Some(*transform),
            _ => None,
        })
        .expect("rotated text command");
    // 90 degrees CCW in screen coordinates: x-axis maps onto -y.
    assert!(
        transform.kx.abs() > 0.5,
        "expected rotation, got {transform:?}"
    );
    assert!(
        transform.ky.abs() > 0.5,
        "expected rotation, got {transform:?}"
    );
}

#[test]
fn emf_text_dx_positions_each_character() {
    let (scene, summary, _) = expand(
        "ppt/media/image1.emf",
        EMF,
        text_emf(0x0018, 0, Some(&[10, 30])),
    );
    assert_eq!(summary.expanded, 1);
    let runs = text_runs(&scene);
    assert_eq!(runs.len(), 2, "dx overrides emit one run per character");
    assert_eq!(runs[0].text, "H");
    assert_eq!(runs[1].text, "i");
    let first_x = runs[0].glyphs.first().map(|glyph| glyph.x).unwrap_or(0.0);
    let second_x = runs[1].glyphs.first().map(|glyph| glyph.x).unwrap_or(0.0);
    assert!(
        second_x - first_x > 0.0,
        "the second character must advance by its dx ({first_x} → {second_x})"
    );
}

#[test]
fn emf_opaque_text_emits_a_background_fill() {
    let records = [
        support::create_font(1, 20, 400, 0, false, false, "Carlito"),
        support::select_object(1),
        support::set_bk_mode(2),
        support::set_bk_color(0x0000_FF00),
        support::set_text_align(0x0018),
        support::ext_text_out((20, 40), "Bg", 2, Some([10, 20, 180, 80]), None),
    ];
    let (scene, summary, _) = expand("ppt/media/image1.emf", EMF, support::emf(&records));
    assert_eq!(summary.expanded, 1);

    let background_index = scene.commands.iter().position(|command| {
        matches!(
            command,
            VectorCommand::DrawPath {
                fill: Some(VectorPaint::Solid([0, 255, 0, 255])),
                ..
            }
        )
    });
    let text_index = scene
        .commands
        .iter()
        .position(|command| matches!(command, VectorCommand::DrawText { .. }));
    assert!(background_index.is_some(), "expected the ETO_OPAQUE fill");
    assert!(text_index.is_some());
    assert!(
        background_index < text_index,
        "the opaque background must paint before the text"
    );
}

#[test]
fn text_mapper_tags_size_clamping_and_dx_mismatch() {
    let run = MetafileTextRun {
        text: "ab".into(),
        glyph_indices: None,
        family: "Carlito".into(),
        size: 1.0e9,
        color: [0, 0, 0, 255],
        bold: false,
        italic: false,
        underline: false,
        strike_out: false,
        x: 0.0,
        y: 0.0,
        escapement_deg: 0.0,
        dx: vec![1.0],
        horizontal_align: TextAlignH::Left,
        vertical_align: TextAlignV::Baseline,
        background: None,
        background_rect: None,
        transform: kitty_pptx_core::contracts::AffineTransform::identity(),
        right_to_left: false,
    };
    let mapped = mapping::text::map_text_run(
        &run,
        &kitty_pptx_core::contracts::AffineTransform::scale(9525.0, 9525.0),
    )
    .expect("mapped");
    let tags: Vec<&str> = mapped
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.tag)
        .collect();
    assert!(tags.contains(&"metafile_text_size"), "{tags:?}");
    assert!(tags.contains(&"metafile_text_dx"), "{tags:?}");
    let pt = mapped.runs[0].font_size_pt;
    assert!((0.5..=400.0).contains(&pt), "clamped size {pt}");
}

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
    match mapping::paint::map_paint(&paint, &kitty_pptx_core::contracts::AffineTransform::identity()) {
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
    match mapping::paint::map_paint(&vertical, &kitty_pptx_core::contracts::AffineTransform::identity()) {
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
        transform: kitty_pptx_core::contracts::AffineTransform {
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
        &kitty_pptx_core::contracts::AffineTransform::scale(scale, scale),
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
    let target = kitty_pptx_core::contracts::RenderTargetSpec {
        width_px: 320,
        height_px: 160,
    };
    let bitmap = kitty_pptx_core::TinySkiaRasterizer::new()
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

// ── M5: WMF / EMZ / WMZ ────────────────────────────────────────────────────

#[test]
fn placeable_wmf_plays_with_window_mapping() {
    let records = [
        support::wmf_rect([10, 10, 120, 80]),
        support::wmf_ext_text([20, 60], b"Hi", 0, &[]),
    ];
    let (scene, summary, _) = expand(
        "ppt/media/image1.wmf",
        WMF,
        support::wmf_placeable(&records),
    );

    assert_eq!(summary.expanded, 1, "placeable WMF must play");
    assert_eq!(summary.placeholders, 0);
    assert!(scene
        .commands
        .iter()
        .any(|command| matches!(command, VectorCommand::DrawPath { .. })));
    let texts: Vec<String> = text_runs(&scene).into_iter().map(|run| run.text).collect();
    assert!(texts.iter().any(|text| text == "Hi"), "got {texts:?}");
}

#[test]
fn emz_inflates_a_real_emf() {
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/metafile/windows-emf-paths.emf"
    ))
    .expect("emf fixture");
    let packed = gzip(&fixture);
    let (scene, summary, warnings) = expand("ppt/media/image1.emz", EMF, packed);

    assert_eq!(summary.expanded, 1, "EMZ must inflate then play");
    assert_eq!(summary.placeholders, 0, "warnings: {warnings:?}");
    assert_eq!(support::markers(&scene), 0);
}

#[test]
fn wmz_inflates_a_real_wmf() {
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/metafile/windows-gdi-vector.wmf"
    ))
    .expect("wmf fixture");
    let (_, summary, warnings) = expand("ppt/media/image1.wmz", WMF, gzip(&fixture));
    assert_eq!(
        summary.expanded, 1,
        "WMZ must inflate then play: {warnings:?}"
    );
    assert_eq!(summary.placeholders, 0);
}

#[test]
fn mislabelled_emz_is_classified_by_content() {
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/metafile/windows-emf-paths.emf"
    ))
    .expect("emf fixture");
    let (_, summary, _) = expand("ppt/media/image1.emz", EMF, fixture);
    assert_eq!(summary.expanded, 1);
}

#[test]
fn emz_bomb_degrades_to_a_placeholder() {
    let limits = metafile::MetafileLimits::default();
    let bomb = gzip(&vec![0u8; limits.max_input_bytes + 1]);
    let (scene, summary, warnings) = expand("ppt/media/image1.emz", EMF, bomb);

    assert_eq!(summary.expanded, 0);
    assert_eq!(summary.placeholders, 1);
    assert!(meets_placeholder(&scene));
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning, WarningEvent::ResourceLimitHit { .. })),
        "a bomb must surface as a resource limit: {warnings:?}"
    );
}

fn meets_placeholder(scene: &VectorSlide) -> bool {
    text_runs(scene)
        .iter()
        .any(|run| run.text.contains("METAFILE"))
}

fn gzip(data: &[u8]) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).expect("gzip write");
    encoder.finish().expect("gzip finish")
}
