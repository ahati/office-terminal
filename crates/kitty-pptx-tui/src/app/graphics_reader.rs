//! Terminal graphics synchronization for Reader mode (PRD 14a §7).
//!
//! Staging and direct subrect placement (`Placement::CellsSubrect`) for
//! focusable document panes.

use kitty_pptx_core::{DisplayError, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{CellRect, ImageId, PixelRect, Placement, TerminalGraphicsDriver};

use crate::app::state::PresentationState;

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
    staged: [Option<ReaderStaged>; 2],
    placed: [Option<ReaderPlaced>; 2],
}

impl ReaderGraphicsSyncer {
    pub(crate) fn release_all(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        for slot in 0..2 {
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
        let mut active_slots = [false; 2];

        for (slot_idx, placement) in placements.into_iter().take(2).enumerate() {
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
                    }
                }
                PipelineEntry::Failed { error } => {
                    state.note_render_failure(page, error.to_string());
                }
                _ => {}
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

        // Trigger prefetching for focused page
        if let Some(reader) = state.reader_state() {
            let center = reader.focused_page();
            let plan = PrefetchPlan {
                center,
                behind: 2,
                ahead: 4,
                stage: false,
            };
            let (pt_w, pt_h) = reader.page_size_pt(center);
            let ar = (pt_w / pt_h).max(0.1);
            let target_w = 2560u32;
            let target_h = ((target_w as f32 / ar).round() as u32).clamp(1024, 8192);
            let prefetch_target = RenderTargetSpec {
                width_px: target_w,
                height_px: target_h,
            };
            pipeline.prefetch(&plan, &prefetch_target);
        }

        Ok(())
    }
}
