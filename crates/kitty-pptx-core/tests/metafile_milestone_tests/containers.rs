//! PRD 13 milestone 5: WMF placeable playback and EMZ/WMZ inflated containers.

use kitty_pptx_core::contracts::{VectorCommand, VectorSlide, WarningEvent};
use kitty_pptx_core::metafile;

use crate::helpers::{expand, text_runs, EMF, WMF};
use crate::metafile_support as support;

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
