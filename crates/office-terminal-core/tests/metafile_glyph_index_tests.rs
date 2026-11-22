//! Integration tests for EMF and EMF+ glyph-indexed text mapping (Issue #13).

#![cfg(feature = "metafile")]

use office_terminal_core::contracts::{
    PositionedGlyphRun, RenderTargetSpec, VectorCommand, VectorSlide, WarningEvent,
};
use office_terminal_core::metafile;
use office_terminal_core::text::cmap;
use office_terminal_core::{SlideRasterizer, SlideSvgSerializer};

mod metafile_support;
use metafile_support as support;

const EMF: &str = "image/x-emf";

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

fn text_runs(scene: &VectorSlide) -> Vec<PositionedGlyphRun> {
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

fn glyph_id_for(family: &str, bold: bool, italic: bool, ch: char) -> u16 {
    let map = cmap::reverse_cmap_for_face(family, bold, italic);
    map.iter()
        .find_map(|(gid, c)| if *c == ch { Some(*gid) } else { None })
        .unwrap_or_else(|| panic!("glyph ID not found for char '{ch}' in font {family}"))
}

fn ext_text_out_glyphs(
    reference: (i32, i32),
    glyphs: &[u16],
    options: u32,
    opaque_rect: Option<[i32; 4]>,
    dx: Option<&[i32]>,
) -> Vec<u8> {
    let utf16: Vec<u8> = glyphs.iter().flat_map(|g| g.to_le_bytes()).collect();
    let count = glyphs.len() as u32;
    let mut payload = vec![0u8; 68];
    payload[28..36].copy_from_slice(&support::i32s(&[reference.0, reference.1]));
    payload[36..40].copy_from_slice(&count.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload[44..48].copy_from_slice(&options.to_le_bytes());
    if let Some(rect) = opaque_rect {
        payload[48..64].copy_from_slice(&support::i32s(&rect));
    }
    payload.extend_from_slice(&utf16);
    while !payload.len().is_multiple_of(4) {
        payload.push(0);
    }
    if let Some(values) = dx {
        let dx_offset = (8 + payload.len()) as u32;
        payload[64..68].copy_from_slice(&dx_offset.to_le_bytes());
        payload.extend_from_slice(&support::i32s(values));
    }
    support::record(84, &payload)
}

fn emf_plus_draw_driver_string_raw(
    font_id: u8,
    brush_id: u8,
    glyphs: &[u16],
    positions: &[(f32, f32)],
    options: u32,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&u32::from(brush_id).to_le_bytes());
    data.extend_from_slice(&options.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // MatrixPresent = 0
    data.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());
    for g in glyphs {
        data.extend_from_slice(&g.to_le_bytes());
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    for (x, y) in positions {
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
    }
    support::emf_plus_record(0x4036, u16::from(font_id), &data)
}

#[test]
fn reverse_cmap_maps_carlito_glyphs() {
    let gid_h = glyph_id_for("Carlito", false, false, 'H');
    let gid_i = glyph_id_for("Carlito", false, false, 'i');
    assert!(gid_h > 0);
    assert!(gid_i > 0);

    let (text, mapped) = cmap::reverse_map_glyphs("Carlito", false, false, &[gid_h, gid_i], None);
    assert!(mapped);
    assert_eq!(text, "Hi");
}

#[test]
fn emf_glyph_index_text_is_mapped_and_rendered() {
    let gid_h = glyph_id_for("Carlito", false, false, 'H');
    let gid_i = glyph_id_for("Carlito", false, false, 'i');

    let emf_bytes = support::emf(&[
        support::create_font(1, 20, 400, 0, false, false, "Carlito"),
        support::select_object(1),
        support::set_text_color(0x0000_00FF),
        support::set_text_align(0x0018), // baseline
        ext_text_out_glyphs((100, 60), &[gid_h, gid_i], 0x0010, None, None), // ETO_GLYPH_INDEX
    ]);

    let (scene, summary, warnings) = expand("ppt/media/image1.emf", EMF, emf_bytes.clone());
    assert_eq!(summary.expanded, 1);
    assert_eq!(summary.placeholders, 0);

    let runs = text_runs(&scene);
    assert_eq!(runs.len(), 1, "DrawText must be emitted for glyph-indexed text");
    assert_eq!(runs[0].text, "Hi", "glyph IDs must be reverse-mapped to Unicode text");

    assert!(
        warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded { feature, .. } if *feature == "metafile_glyph_index_text"
        )),
        "metafile_glyph_index_text warning must be emitted: {warnings:?}"
    );

    // Verify rasterizer paints ink
    let store = support::FixtureStore::new("ppt/media/image1.emf", emf_bytes);
    let target = RenderTargetSpec {
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
    assert!(ink > 0, "shaped glyph-indexed text must paint pixels");

    // Verify SVG export
    let svg = office_terminal_core::DefaultSlideSvgSerializer::new()
        .serialize_svg(&scene, &store, office_terminal_core::SvgTextMode::SelectableText)
        .expect("serialize svg");
    assert!(svg.contains("Hi"), "reverse-mapped text must appear in SVG");
}

#[test]
fn emfplus_draw_driver_string_glyph_indices_mapped_and_rendered() {
    let gid_a = glyph_id_for("Carlito", false, false, 'A');
    let gid_b = glyph_id_for("Carlito", false, false, 'B');

    let records = [
        support::emf_plus_font(1, 14.0, 3, 0, "Carlito"),
        support::emf_plus_solid_brush(2, 0xff00_0000),
        emf_plus_draw_driver_string_raw(1, 2, &[gid_a, gid_b], &[(20.0, 40.0), (35.0, 40.0)], 0),
    ];
    let emf_bytes = support::emf_plus_only_emf(&records);

    let (scene, summary, warnings) = expand("ppt/media/image1.emf", EMF, emf_bytes);
    assert_eq!(summary.expanded, 1);
    assert_eq!(summary.placeholders, 0);

    let runs = text_runs(&scene);
    assert_eq!(runs.len(), 1, "DrawText must be emitted for EmfPlusDrawDriverString");
    assert_eq!(runs[0].text, "AB");

    assert!(
        warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded { feature, .. } if *feature == "metafile_glyph_index_text"
        )),
        "metafile_glyph_index_text warning must be emitted: {warnings:?}"
    );
}

#[test]
fn unmappable_glyph_id_produces_fallback_not_dropped() {
    let unmappable_gid = 65000u16;

    let emf_bytes = support::emf(&[
        support::create_font(1, 20, 400, 0, false, false, "Carlito"),
        support::select_object(1),
        support::set_text_color(0x0000_00FF),
        support::set_text_align(0x0018),
        ext_text_out_glyphs((100, 60), &[unmappable_gid], 0x0010, None, None),
    ]);

    let (scene, summary, warnings) = expand("ppt/media/image1.emf", EMF, emf_bytes);
    assert_eq!(summary.expanded, 1);

    let runs = text_runs(&scene);
    assert_eq!(runs.len(), 1, "unmappable glyph must still emit a text run");
    assert!(!runs[0].text.is_empty(), "text must not be empty");
    assert_eq!(runs[0].glyphs[0].glyph_id, unmappable_gid);

    assert!(warnings.iter().any(|w| matches!(
        w,
        WarningEvent::FidelityDegraded { feature, .. } if *feature == "metafile_glyph_index_text"
    )));
}
