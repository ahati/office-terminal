//! Terminal graphics synchronization for Reader mode (PRD 14a §7).
//!
//! Staging and direct subrect placement (`Placement::CellsSubrect`) for
//! focusable document panes.

use kitty_pptx_core::{DisplayError, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{
    CellRect, ImageId, PixelRect, Placement, TerminalGraphicsDriver, READER_IMAGE_ID_BASE,
    READER_IMAGE_ID_SLOTS,
};

use crate::app::state::PresentationState;

/// Number of resident reader image ids (mirrors the display-layer pool).
pub const READER_SLOTS: usize = READER_IMAGE_ID_SLOTS as usize;

/// Reader raster cache budget: pages stay resident while scrolling so row
/// transitions and small jumps do not re-rasterize.
const READER_CACHE_BUDGET_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ReaderStaged {
    id: ImageId,
    page: usize,
    width: u32,
    height: u32,
    epoch: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ReaderPlaced {
    id: ImageId,
    cell_rect: CellRect,
    pixel_src: PixelRect,
    epoch: u64,
}

/// Graphics syncer state for document reader viewports.
#[derive(Default)]
pub(crate) struct ReaderGraphicsSyncer {
    staged: [Option<ReaderStaged>; READER_SLOTS],
    placed: [Option<ReaderPlaced>; READER_SLOTS],
    /// Prefetch guard keyed on the exact visible target set: re-issues when
    /// the visible window changes, never per frame.
    last_prefetch: Option<Vec<(usize, RenderTargetSpec)>>,
}

impl ReaderGraphicsSyncer {
    pub(crate) fn release_all(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        for slot in 0..READER_SLOTS {
            if let Some(placed) = self.placed[slot].take() {
                let _ = driver.release(placed.id);
            }
            self.staged[slot] = None;
        }
        Ok(())
    }

    pub(crate) fn sync(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        _canvas_target: &RenderTargetSpec,
        epoch: &mut u64,
    ) -> Result<(), DisplayError> {
        let placements = state.reader_placements();

        // Per-page prefetch targets (P1-1): every visible pane stages at its
        // own aspect-correct target — mixed-size documents included — and the
        // next row is pre-staged so panes scrolling into view never flash
        // blank.
        if let Some(reader) = state.reader_state() {
            let center = reader.focused_page();
            let mut targets: Vec<(usize, RenderTargetSpec)> = Vec::new();
            for placement in &placements {
                if !targets.iter().any(|&(page, _)| page == placement.page) {
                    targets.push((placement.page, placement.page_target));
                }
            }
            let cols = reader.num_columns().max(1);
            let next_anchor = if reader.cover_alone && reader.anchor == 0 {
                1
            } else {
                reader.anchor + cols
            };
            for i in 0..cols {
                let page = next_anchor + i;
                if page < reader.total_pages() && !targets.iter().any(|&(p, _)| p == page) {
                    targets.push((page, reader.page_render_target(page)));
                }
            }

            if self.last_prefetch.as_ref() != Some(&targets) {
                let center_target = reader.page_render_target(center);
                let page_bytes =
                    (center_target.width_px as usize) * (center_target.height_px as usize) * 4;
                let max_cached_pages =
                    (READER_CACHE_BUDGET_BYTES / page_bytes.max(1)).clamp(4, 32);
                let behind = (cols + 1).min(max_cached_pages / 4);
                let visible_count = reader.visible_pages().len();
                let max_ahead = max_cached_pages.saturating_sub(visible_count + behind).max(1);
                let ahead = (cols * 2 + 1).min(max_ahead);
                let plan = PrefetchPlan {
                    center,
                    behind,
                    ahead,
                    // Reader pages are pre-encoded on the workers so the syncer
                    // can later write prebuilt bytes instead of encoding.
                    stage: true,
                };
                pipeline.prefetch_window(&plan, &targets);
                self.last_prefetch = Some(targets);
            }
        }

        let mut active_slots = [false; READER_SLOTS];

        for placement in placements {
            // Stable per-page ids (reader image pool base 81): slot index is
            // derived from the id, not from a row-parity scheme.
            let slot_idx = (placement.id.0.saturating_sub(READER_IMAGE_ID_BASE) as usize)
                .min(READER_SLOTS - 1);
            active_slots[slot_idx] = true;
            let page = placement.page;
            let id = placement.id;

            match pipeline.entry_at(page, &placement.page_target) {
                PipelineEntry::Ready { bitmap, .. } => {
                    let needs_stage = match self.staged[slot_idx] {
                        Some(s) => {
                            !(s.id == id
                                && s.page == page
                                && s.width == bitmap.width
                                && s.height == bitmap.height)
                        }
                        None => true,
                    };

                    if needs_stage {
                        // Pre-encoded payloads from the raster workers keep
                        // PNG encoding off the UI thread; fall back to staging
                        // the bitmap directly when no payload is cached (or
                        // the driver cannot take prebuilt bytes).
                        let prebuilt_sent =
                            match pipeline.staged_payload(page, &placement.page_target) {
                                Some(payload) => driver
                                    .transmit_prebuilt(id, bitmap.width, bitmap.height, &payload)
                                    .is_ok(),
                                None => false,
                            };
                        if !prebuilt_sent {
                            driver.stage(&bitmap, id)?;
                        }
                        *epoch = epoch.saturating_add(1);
                        self.staged[slot_idx] = Some(ReaderStaged {
                            id,
                            page,
                            width: bitmap.width,
                            height: bitmap.height,
                            epoch: *epoch,
                        });
                    }

                    let cur_epoch = self.staged[slot_idx].map(|s| s.epoch).unwrap_or(0);
                    let needs_place = match self.placed[slot_idx] {
                        Some(p) => {
                            !(p.id == id
                                && p.cell_rect == placement.cell_rect
                                && p.pixel_src == placement.pixel_src
                                && p.epoch == cur_epoch)
                        }
                        None => true,
                    };

                    if needs_place {
                        let prev_placed = self.placed[slot_idx];
                        let direct = Placement::CellsSubrect {
                            rect: placement.cell_rect,
                            src: placement.pixel_src,
                        };
                        driver.present(id, direct)?;
                        self.placed[slot_idx] = Some(ReaderPlaced {
                            id,
                            cell_rect: placement.cell_rect,
                            pixel_src: placement.pixel_src,
                            epoch: cur_epoch,
                        });
                        if let Some(prev) = prev_placed {
                            if prev.id != id {
                                let _ = driver.release(prev.id);
                            }
                        }
                    }

                    // A page that renders cleanly clears any stale failure
                    // note, mirroring the slide syncer.
                    state.clear_render_failure_for(page);
                }
                PipelineEntry::Rendering | PipelineEntry::NotLoaded => {
                    // If this slot was showing a different page before, release it so
                    // stale pages are never shown during page turns (P1-2)
                    if let Some(staged) = self.staged[slot_idx] {
                        if staged.page != page {
                            if let Some(placed) = self.placed[slot_idx].take() {
                                let _ = driver.release(placed.id);
                            }
                        }
                    }
                }
                PipelineEntry::Failed { error } => {
                    state.note_render_failure(page, error.to_string());
                }
            }
        }

        // Release any slots no longer active (e.g. going from Spread to Single)
        for (slot, &active) in active_slots.iter().enumerate() {
            if !active {
                if let Some(placed) = self.placed[slot].take() {
                    let _ = driver.release(placed.id);
                }
                self.staged[slot] = None;
            }
        }

        Ok(())
    }
}
