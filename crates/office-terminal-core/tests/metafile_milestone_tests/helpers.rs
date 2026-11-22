//! Shared fixtures for the milestone playback tests: EMF/WMF MIME constants,
//! picture expansion, and text-run extraction.

use office_terminal_core::contracts::{VectorCommand, VectorSlide, WarningEvent};
use office_terminal_core::metafile;

use crate::metafile_support as support;

pub const EMF: &str = "image/x-emf";
pub const WMF: &str = "image/x-wmf";

pub fn expand(
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

pub fn text_runs(scene: &VectorSlide) -> Vec<office_terminal_core::contracts::PositionedGlyphRun> {
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
