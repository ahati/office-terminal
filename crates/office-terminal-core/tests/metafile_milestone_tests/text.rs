//! PRD 13 milestone 3: EMF text playback — substitution, alignment,
//! escapement, per-character dx and opaque backgrounds.

use office_terminal_core::contracts::{VectorCommand, VectorPaint, VectorSlide, WarningEvent};
use office_terminal_core::metafile::{mapping, MetafileTextRun, TextAlignH, TextAlignV};
use office_terminal_core::{SlideRasterizer, SlideSvgSerializer};

use crate::helpers::{expand, text_runs, EMF};
use crate::metafile_support as support;

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
    let target = office_terminal_core::contracts::RenderTargetSpec {
        width_px: 320,
        height_px: 240,
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
    assert!(ink > 0, "shaped metafile text must paint pixels");

    // SVG must carry the same shaped run as selectable text.
    let svg = office_terminal_core::DefaultSlideSvgSerializer::new()
        .serialize_svg(&scene, &store, office_terminal_core::SvgTextMode::SelectableText)
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
        transform: office_terminal_core::contracts::AffineTransform::identity(),
        right_to_left: false,
    };
    let mapped = mapping::text::map_text_run(
        &run,
        &office_terminal_core::contracts::AffineTransform::scale(9525.0, 9525.0),
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
