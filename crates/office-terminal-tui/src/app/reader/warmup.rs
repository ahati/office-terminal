//! First-window raster warm-up for reader mode.
//!
//! The one-shot first-frame paint renders the anchor page synchronously on
//! the UI thread. Without warm-up the worker pool sits idle until that
//! render finishes, so the next page's raster only *starts* afterwards —
//! an immediate scroller reaches the first page boundary while that
//! raster is still in flight, and the next page's peek pane stays blank
//! until the anchor flips.
//!
//! The window's raster targets MUST use the renderer's own column plan
//! ([`ReaderState::planned_columns`]), not the stored effective column
//! count: on tall viewports the renderer plans one column while the stored
//! count still says two, and a target computed from the stale plan never
//! matches the placement lookup — the peek pane would stay `NotLoaded`
//! (blank) forever.

use office_terminal_core::{PrefetchPlan, RenderPipeline};

use crate::app::reader_types::reader_viewport_metrics;
use crate::app::state::PresentationState;

/// Schedules the rasters of the pages following `center` on the worker
/// pool so they overlap the synchronous first-frame paint instead of
/// starting after it.
pub(crate) fn prefetch_first_window(
    state: &PresentationState,
    pipeline: &dyn RenderPipeline,
    center: usize,
) {
    let Some(reader) = state.reader_state() else {
        return;
    };
    let (grid_cols, _rows) = state.layout_size();
    let (viewport_w_px, viewport_h_px, cell_w_px, _cell_h_px) =
        reader_viewport_metrics(grid_cols, _rows);
    let cols = reader.planned_columns(grid_cols, viewport_h_px, cell_w_px);

    let total = reader.total_pages();
    // Mirror the syncer's window breadth (its `cols * 2 + 1` pages ahead).
    let ahead = (cols * 2 + 1).min(total.saturating_sub(center + 1));
    if ahead == 0 {
        return;
    }

    let mut targets = Vec::with_capacity(ahead);
    for dist in 1..=ahead {
        let page = center + dist;
        if page >= total {
            break;
        }
        let pane_w = reader.pane_width_px_for(page, viewport_w_px, viewport_h_px, cell_w_px, cols);
        targets.push((page, reader.page_render_target(page, pane_w)));
    }
    if targets.is_empty() {
        return;
    }

    pipeline.prefetch_window(
        &PrefetchPlan {
            center,
            behind: 0,
            ahead,
            stage: true,
        },
        &targets,
    );
}
