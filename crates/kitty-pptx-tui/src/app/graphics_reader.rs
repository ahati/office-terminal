//! Terminal graphics synchronization for Reader mode (PRD 14a §7).
//!
//! Staging and direct subrect placement (`Placement::CellsSubrect`) for
//! focusable document panes.

use kitty_pptx_core::{DisplayError, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{CellRect, ImageId, PixelRect, Placement, TerminalGraphicsDriver};

use crate::app::state::PresentationState;

pub const READER_SLOTS: usize = 12;

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
    last_prefetch: Option<(usize, RenderTargetSpec)>,
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
        // Trigger prefetching sized to viewport columns to avoid thrashing the cache budget (P1-1)
        if let Some(reader) = state.reader_state() {
            let center = reader.focused_page();
            let (pt_w, pt_h) = reader.page_size_pt(center);
            let ar = (pt_w / pt_h).max(0.1);
            let target_w = 2560u32;
            let target_h = ((target_w as f32 / ar).round() as u32).clamp(1024, 8192);
            let prefetch_target = RenderTargetSpec {
                width_px: target_w,
                height_px: target_h,
            };
            let prefetch_key = (center, prefetch_target);
            if self.last_prefetch != Some(prefetch_key) {
                let cols = reader.effective_columns();
                let page_bytes = (target_w as usize) * (target_h as usize) * 4;
                let cache_budget_bytes = 512 * 1024 * 1024;
                let max_cached_pages = (cache_budget_bytes / page_bytes.max(1)).clamp(4, 32);
                let behind = (cols + 1).min(max_cached_pages / 4);
                let visible_count = reader.visible_pages().len();
                let max_ahead = max_cached_pages.saturating_sub(visible_count + behind).max(1);
                let ahead = (cols * 2 + 1).min(max_ahead);
                let plan = PrefetchPlan {
                    center,
                    behind,
                    ahead,
                    stage: false,
                };
                pipeline.prefetch(&plan, &prefetch_target);
                self.last_prefetch = Some(prefetch_key);
            }
        }

        let placements = state.reader_placements();
        let mut active_slots = [false; READER_SLOTS];

        for placement in placements {
            let slot_idx = (placement.id.0.saturating_sub(1) as usize).min(READER_SLOTS - 1);
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
                        driver.stage(&bitmap, id)?;
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
