//! Slide sorter thumbnail graphics synchronization and quota accounting.

use std::collections::{BTreeMap, HashSet, VecDeque};
use kitty_pptx_core::{DisplayError, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{
    terminal_image_quota_bytes, CellRect, ImageId, Placement, TerminalGraphicsDriver,
    MAX_THUMBNAIL_IMAGES, THUMBNAIL_IMAGE_ID_BASE,
};

use crate::app::state::PresentationState;

/// Upper bound on newly transmitted thumbnails per frame.
pub const MAX_THUMBNAIL_STAGES_PER_FRAME: usize = 4;

/// Calculate the maximum number of visible sorter cards given a target spec.
pub fn max_sorter_cards_for_target(target: &RenderTargetSpec) -> usize {
    let per_card = (target.width_px as usize) * (target.height_px as usize) * 4;
    let quota = terminal_image_quota_bytes();
    quota
        .checked_div(per_card)
        .map(|c| c.clamp(1, MAX_THUMBNAIL_IMAGES as usize))
        .unwrap_or(MAX_THUMBNAIL_IMAGES as usize)
}

/// A slide-sorter card currently staged on the terminal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThumbSlot {
    pub id: ImageId,
    pub slide: usize,
    pub rect: CellRect,
    pub bytes: usize,
}

/// Manages thumbnail staging, caching, and terminal quota eviction.
#[derive(Default)]
pub struct ThumbnailSyncer {
    pub thumbs: BTreeMap<usize, ThumbSlot>,
    pub staging_order: VecDeque<usize>,
    pub total_staged_bytes: usize,
    pub thumb_prefetch: Option<(usize, usize, RenderTargetSpec)>,
}

impl ThumbnailSyncer {
    pub fn release_all(&mut self, driver: &mut dyn TerminalGraphicsDriver) -> Result<(), DisplayError> {
        self.thumb_prefetch = None;
        if self.thumbs.is_empty() {
            return Ok(());
        }
        let ids: Vec<ImageId> = self.thumbs.values().map(|slot| slot.id).collect();
        self.thumbs.clear();
        self.staging_order.clear();
        self.total_staged_bytes = 0;
        for id in ids {
            driver.release(id)?;
        }
        Ok(())
    }

    pub fn release_except(
        &mut self,
        keep: &HashSet<usize>,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let stale: Vec<usize> = self
            .thumbs
            .keys()
            .copied()
            .filter(|slide| !keep.contains(slide))
            .collect();
        for slide in stale {
            if let Some(slot) = self.thumbs.remove(&slide) {
                self.total_staged_bytes = self.total_staged_bytes.saturating_sub(slot.bytes);
                self.staging_order.retain(|&s| s != slide);
                driver.release(slot.id)?;
            }
        }
        Ok(())
    }

    pub fn thumbnail_id(&self, slide: usize) -> ImageId {
        if let Some(slot) = self.thumbs.get(&slide) {
            return slot.id;
        }
        let used: HashSet<u32> = self.thumbs.values().map(|slot| slot.id.0).collect();
        let mut id = THUMBNAIL_IMAGE_ID_BASE;
        while used.contains(&id) {
            id += 1;
        }
        ImageId(id)
    }

    pub fn sync_thumbnails(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        canvas_target: &RenderTargetSpec,
        epoch: &mut u64,
    ) -> Result<(), DisplayError> {
        let mut visible = state.thumbnail_placement_rects();
        let max_cards = max_sorter_cards_for_target(canvas_target);
        visible.truncate(max_cards);

        let Some((first_slide, _)) = visible.first().copied() else {
            return self.release_all(driver);
        };

        let prefetch_key = (first_slide, visible.len(), *canvas_target);
        if self.thumb_prefetch != Some(prefetch_key) {
            let plan = PrefetchPlan {
                center: first_slide,
                behind: 0,
                ahead: visible.len().saturating_sub(1),
                stage: true,
            };
            pipeline.prefetch(&plan, canvas_target);
            self.thumb_prefetch = Some(prefetch_key);
        }

        let keep: HashSet<usize> = visible.iter().map(|(slide, _)| *slide).collect();
        self.release_except(&keep, driver)?;

        let quota = terminal_image_quota_bytes();
        let mut stages_left = MAX_THUMBNAIL_STAGES_PER_FRAME;

        for (slide, rect) in visible {
            let entry = match pipeline.entry_at(slide, canvas_target) {
                PipelineEntry::Ready { bitmap, staged } => PipelineEntry::Ready { bitmap, staged },
                _ => pipeline.entry(slide),
            };

            match entry {
                PipelineEntry::Ready { bitmap, .. } => {
                    state.clear_render_failure_for(slide);

                    let staged = self.thumbs.get(&slide).map(|slot| (slot.id, slot.rect));
                    match staged {
                        Some((_, prev_rect)) if prev_rect == rect => continue,
                        Some((id, _)) => {
                            driver.reposition(id, Placement::Cells(rect))?;
                            if let Some(slot) = self.thumbs.get_mut(&slide) {
                                slot.rect = rect;
                            }
                            continue;
                        }
                        None => {}
                    }

                    if stages_left == 0 {
                        continue;
                    }
                    stages_left -= 1;

                    let card_bytes = (bitmap.width as usize) * (bitmap.height as usize) * 4;

                    // Enforce terminal quota by evicting oldest cards if needed
                    while self.total_staged_bytes + card_bytes > quota && !self.staging_order.is_empty() {
                        if let Some(oldest_slide) = self.staging_order.pop_front() {
                            if let Some(old_slot) = self.thumbs.remove(&oldest_slide) {
                                self.total_staged_bytes = self.total_staged_bytes.saturating_sub(old_slot.bytes);
                                let _ = driver.release(old_slot.id);
                            }
                        }
                    }

                    let id = self.thumbnail_id(slide);
                    driver.stage(&bitmap, id)?;
                    driver.present(id, Placement::Cells(rect))?;
                    *epoch = epoch.saturating_add(1);

                    self.thumbs.insert(
                        slide,
                        ThumbSlot {
                            id,
                            slide,
                            rect,
                            bytes: card_bytes,
                        },
                    );
                    self.staging_order.push_back(slide);
                    self.total_staged_bytes += card_bytes;
                }
                PipelineEntry::Failed { error } if slide == state.selected_sorter_index() => {
                    state.note_render_failure(slide, error.to_string());
                }
                PipelineEntry::Failed { .. } => {}
                _ => {}
            }
        }

        Ok(())
    }
}
