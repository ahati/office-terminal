//! Process-wide metafile playback override (PRD 13 §10.3 / §10.4).
//!
//! Kept in its own test binary: one test function flips the global mode so the
//! process-global atomic cannot race another test.

#![cfg(feature = "metafile")]


mod metafile_support;

use kitty_pptx_core::contracts::{VectorCommand, WarningEvent};
use kitty_pptx_core::metafile::{self, MetafileMode};
use metafile_support as support;

const EMF: &str = "image/x-emf";

fn expand_letter(bytes: Vec<u8>) -> (usize, usize, usize, Vec<WarningEvent>) {
    metafile::clear_global_cache();
    let (compile_tx, _compile_rx) = crossbeam_channel::unbounded();
    let mut scene = support::compile_slide(
        &support::picture_slide("ppt/media/image1.emf", EMF),
        &compile_tx,
    );
    let store = support::FixtureStore::new("ppt/media/image1.emf", bytes);
    let (tx, rx) = crossbeam_channel::unbounded();
    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);
    let markers = scene
        .commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawMetafile { .. }))
        .count();
    (
        summary.expanded,
        summary.placeholders,
        markers,
        rx.try_iter().collect(),
    )
}

fn text_emf() -> Vec<u8> {
    support::emf(&[
        support::create_font(1, 20, 700, 0, false, false, "Carlito"),
        support::select_object(1),
        support::set_text_align(0x0018),
        support::ext_text_out((20, 40), "Hi", 0, None, None),
    ])
}

#[test]
fn placeholder_mode_suppresses_playback_and_diagnostics() {
    metafile::set_playback_mode(MetafileMode::Placeholder);
    let (expanded, placeholders, markers, warnings) = expand_letter(text_emf());
    assert_eq!((expanded, placeholders, markers), (0, 1, 0));
    assert!(
        warnings.is_empty(),
        "the debug override is documented to stay silent: {warnings:?}"
    );

    metafile::set_playback_mode(MetafileMode::Playback);
    let (expanded, placeholders, markers, _) = expand_letter(text_emf());
    assert_eq!((expanded, placeholders, markers), (1, 0, 0));
}
