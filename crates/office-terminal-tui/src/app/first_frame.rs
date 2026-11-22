//! Bounded first-frame paint and pipeline readiness check.

use office_terminal_core::{PipelineEntry, RenderPipeline, RenderTargetSpec};

use crate::app::mode::AppMode;
use crate::app::reader;
use crate::app::state::PresentationState;

/// Whether the bounded first-frame paint needs a synchronous render.
///
/// Only `Ready` skips it — a cached raster (or a worker result that won the
/// race against the first paint) can be staged directly, so the deck opens
/// without paying any pause. `Rendering` still renders: the deck must not
/// open blank while a worker is mid-flight, and the duplicate result is
/// idempotent. `Failed` still renders: the retry either clears the failure
/// latch or re-latches it, and the syncer's failure-note path surfaces the
/// outcome on the first frame either way.
pub(crate) fn first_frame_needs_sync(entry: &PipelineEntry) -> bool {
    !matches!(entry, PipelineEntry::Ready { .. })
}

/// One-shot synchronous first paint, issued once from `run()` right
/// after `handle_resize` computed the target and scheduled the prefetch.
///
/// The readiness check resolves the initial slide against the exact
/// `current_target` (`entry_at`, not `entry`): reader mode hands scheduling
/// to `ReaderSyncer` without latching the pipeline's last target, so a
/// plain `entry` could consult a stale latch here. `recompute_target`'s
/// reader branch has just set `current_target` to the canonical
/// 2560-px-wide target the reader panes render at, and `current_slide`
/// mirrors the focused page, so this one code path covers reader decks
/// too. A worker that already won the race answers `Ready` and the sync
/// render is skipped.
///
/// `trigger_prefetch` scheduled the same slide at top priority during
/// `handle_resize`; when the sync render wins instead, the worker's
/// later result hits the raster-cache fast path (idempotent, cheap) —
/// acceptable duplicate work for exactly one slide. A render failure is
/// deliberately dropped: the syncer's failure-note path reports it on the
/// first frame exactly like a failed worker render would.
pub(crate) fn paint_first_frame(
    state: &PresentationState,
    pipeline: &dyn RenderPipeline,
    current_target: &RenderTargetSpec,
) {
    let initial = state.current_slide();
    // Reader: warm the next pages on the workers BEFORE the synchronous
    // anchor paint — otherwise they idle through the anchor's whole
    // raster and the next page's peek scrolls into view blank (see
    // `reader::warmup`).
    if state.base_mode() == AppMode::Reader {
        reader::warmup::prefetch_first_window(state, pipeline, initial);
    }
    let entry = pipeline.entry_at(initial, current_target);
    if first_frame_needs_sync(&entry) {
        let _ = pipeline.render(initial, current_target);
    }
}
