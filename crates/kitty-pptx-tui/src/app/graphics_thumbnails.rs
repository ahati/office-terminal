//! Slide sorter thumbnail graphics synchronization and quota accounting.

use kitty_pptx_core::{
    DisplayError, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec, RgbaSlide,
};
use kitty_pptx_display::{
    terminal_image_quota_bytes, CellRect, ImageId, Placement, TerminalGraphicsDriver,
    MAX_THUMBNAIL_IMAGES, THUMBNAIL_IMAGE_ID_BASE,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::app::graphics_slots::{PlacedKey, SlotBitmap, SlotEpochs, SlotState};
use crate::app::state::PresentationState;

/// Upper bound on newly transmitted thumbnails per frame.
pub const MAX_THUMBNAIL_STAGES_PER_FRAME: usize = 4;

/// Sorter cards render at a dedicated raster width instead of reusing the
/// full-screen canvas target.
///
/// 1280 px keeps every realistic text run safely above the rasterizer's 6 px
/// font floor: the floor equals ~4.5 pt at 1280 px wide (96 px/in), while real
/// decks bottom out around 8 pt. A card therefore reads like the full-screen
/// slide while rasterization drops ~5x in cost and ~5x in memory compared to a
/// ~2844 px canvas raster. Because the target is derived from the slide aspect
/// alone, terminal resizes and zoom changes never invalidate sorter rasters.
/// PDF pages benefit twice: their canonical source bitmap is 2560 px wide, so
/// a card downsamples from it instead of upsampling past it like the canvas
/// view does (sharper, not just cheaper).
pub const THUMBNAIL_TARGET_WIDTH: u32 = 1280;

/// Clamp for the aspect-derived target's secondary axis.
const THUMBNAIL_TARGET_AXIS_MIN: u32 = 512;
const THUMBNAIL_TARGET_AXIS_MAX: u32 = 8192;

/// Downscaled card memory bound; the terminal scales this into the card cells.
const THUMBNAIL_CARD_MAX_WIDTH: u32 = 480;
const THUMBNAIL_CARD_MAX_HEIGHT: u32 = 360;

/// Downscale cache capacity. Matches the resident sorter card pool; a window
/// larger than this wholesale-clears (see PRD 14a §11).
const DOWNSCALE_CACHE_CAP: usize = 64;

/// Deterministic sorter raster target for a slide aspect ratio (w/h).
///
/// Landscape slides fit 1280 px wide; portrait slides fit 1280 px tall.
pub fn thumbnail_target_for_aspect(aspect: f32) -> RenderTargetSpec {
    if !aspect.is_finite() || aspect <= 0.0 {
        return thumbnail_target_for_aspect(16.0 / 9.0);
    }
    if aspect >= 1.0 {
        let height = ((THUMBNAIL_TARGET_WIDTH as f32 / aspect).round() as u32)
            .clamp(THUMBNAIL_TARGET_AXIS_MIN, THUMBNAIL_TARGET_AXIS_MAX);
        RenderTargetSpec {
            width_px: THUMBNAIL_TARGET_WIDTH,
            height_px: height,
        }
    } else {
        let width = ((THUMBNAIL_TARGET_WIDTH as f32 * aspect).round() as u32)
            .clamp(THUMBNAIL_TARGET_AXIS_MIN, THUMBNAIL_TARGET_AXIS_MAX);
        RenderTargetSpec {
            width_px: width,
            height_px: THUMBNAIL_TARGET_WIDTH,
        }
    }
}

/// Calculate the maximum number of visible sorter cards given a target spec.
///
/// Cards are downscaled to thumbnail dimensions (~480x360), so per-card memory
/// is computed against the downscaled bounds to avoid prematurely capping
/// portrait or high-resolution canvas targets.
pub fn max_sorter_cards_for_target_and_quota(
    target: &RenderTargetSpec,
    quota: Option<usize>,
) -> usize {
    let w = (target.width_px as usize).min(THUMBNAIL_CARD_MAX_WIDTH as usize);
    let h = (target.height_px as usize).min(THUMBNAIL_CARD_MAX_HEIGHT as usize);
    let per_card = (w * h * 4).max(1);
    let q = quota.unwrap_or_else(terminal_image_quota_bytes);
    q.checked_div(per_card)
        .map(|c| c.clamp(1, MAX_THUMBNAIL_IMAGES as usize))
        .unwrap_or(MAX_THUMBNAIL_IMAGES as usize)
}

pub fn max_sorter_cards_for_target(target: &RenderTargetSpec) -> usize {
    max_sorter_cards_for_target_and_quota(target, None)
}

/// A sorter card slot: engine bookkeeping (staged identity plus the
/// on-screen placement, including the terminal id) next to the policy field
/// the quota accountant needs. Staging order lives on the syncer because
/// eviction is global; a resident card always carries a placement
/// ([`ThumbnailSyncer::stage_card`] records stage + present together).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThumbSlot {
    pub state: SlotState,
    pub bytes: usize,
}

/// Where a card's pixels come from this frame.
enum CardSource {
    /// Fresh pipeline raster at the sorter target; downscaled on first use.
    Raster(RgbaSlide),
    /// Downscale-cache hit (raster evicted or never scheduled).
    Cached(RgbaSlide),
}

/// Manages thumbnail staging, caching, and terminal quota eviction.
#[derive(Default)]
pub struct ThumbnailSyncer {
    pub thumbs: BTreeMap<usize, ThumbSlot>,
    pub staging_order: VecDeque<usize>,
    pub total_staged_bytes: usize,
    pub thumb_prefetch: Option<(usize, usize, RenderTargetSpec)>,
    /// Downscaled card per slide, tagged with the target it was built from.
    /// Survives resizes (the sorter target is aspect-derived) and sorter
    /// re-entry, so scroll-back and re-entry never re-render.
    pub downscale_cache: HashMap<usize, (RenderTargetSpec, RgbaSlide)>,
}

impl ThumbnailSyncer {
    /// Terminal ids of the cards currently resident on screen.
    fn resident_ids(&self) -> Vec<ImageId> {
        self.thumbs
            .values()
            .filter_map(|slot| slot.state.placed_id())
            .collect()
    }

    /// Drops every staged card's bookkeeping (slot map, staging order,
    /// quota bytes) and returns the terminal ids to release.
    fn take_resident_ids(&mut self) -> Vec<ImageId> {
        let ids = self.resident_ids();
        self.thumbs.clear();
        self.staging_order.clear();
        self.total_staged_bytes = 0;
        ids
    }

    /// Releases every staged card and drops the downscale cache (teardown:
    /// blank screen, empty deck).
    pub fn release_all(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        self.thumb_prefetch = None;
        self.downscale_cache.clear();
        if self.thumbs.is_empty() {
            return Ok(());
        }
        for id in self.take_resident_ids() {
            driver.release(id)?;
        }
        Ok(())
    }

    /// Releases every staged card but keeps the downscale cache (leaving the
    /// sorter, or a resize): the deck content is unchanged, so re-entry and
    /// scroll-back restage from cache instead of re-rendering.
    pub fn release_all_keep_cache(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        self.thumb_prefetch = None;
        if self.thumbs.is_empty() {
            return Ok(());
        }
        for id in self.take_resident_ids() {
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
                if let Some(id) = slot.state.placed_id() {
                    driver.release(id)?;
                }
            }
        }
        Ok(())
    }

    pub fn thumbnail_id(&self, slide: usize) -> ImageId {
        if let Some(id) = self.thumbs.get(&slide).and_then(|slot| slot.state.placed_id()) {
            return id;
        }
        let used: HashSet<u32> = self.resident_ids().into_iter().map(|ImageId(id)| id).collect();
        let mut id = THUMBNAIL_IMAGE_ID_BASE;
        while used.contains(&id) {
            id += 1;
        }
        ImageId(id)
    }

    /// Cached downscaled card for `slide`, valid only for `target`.
    fn cached_card(&self, slide: usize, target: &RenderTargetSpec) -> Option<RgbaSlide> {
        match self.downscale_cache.get(&slide) {
            Some((cached_target, cached)) if cached_target == target => Some(cached.clone()),
            _ => None,
        }
    }

    /// Downscaled card for a freshly resolved raster, reusing the cache when
    /// the target matches (PRD 14a §11: no repeated resizing on the UI thread).
    fn card_bitmap(
        &mut self,
        slide: usize,
        bitmap: &RgbaSlide,
        target: &RenderTargetSpec,
    ) -> RgbaSlide {
        if let Some(cached) = self.cached_card(slide, target) {
            return cached;
        }
        let ds = bitmap.downscale_to(THUMBNAIL_CARD_MAX_WIDTH, THUMBNAIL_CARD_MAX_HEIGHT);
        if self.downscale_cache.len() >= DOWNSCALE_CACHE_CAP {
            self.downscale_cache.clear();
        }
        self.downscale_cache.insert(slide, (*target, ds.clone()));
        ds
    }

    /// Stages `downscaled` under a fresh id, evicting oldest cards when the
    /// terminal image quota is exceeded, and records the slot bookkeeping.
    fn stage_card(
        &mut self,
        slide: usize,
        downscaled: &RgbaSlide,
        rect: CellRect,
        quota: usize,
        driver: &mut dyn TerminalGraphicsDriver,
        epochs: &mut SlotEpochs,
    ) -> Result<(), DisplayError> {
        let card_bytes = (downscaled.width as usize) * (downscaled.height as usize) * 4;

        // Enforce terminal quota by evicting oldest cards if needed
        while self.total_staged_bytes + card_bytes > quota && !self.staging_order.is_empty() {
            if let Some(oldest_slide) = self.staging_order.pop_front() {
                if let Some(old_slot) = self.thumbs.remove(&oldest_slide) {
                    self.total_staged_bytes =
                        self.total_staged_bytes.saturating_sub(old_slot.bytes);
                    if let Some(id) = old_slot.state.placed_id() {
                        let _ = driver.release(id);
                    }
                }
            }
        }

        let id = self.thumbnail_id(slide);
        driver.stage(downscaled, id)?;
        driver.present(id, Placement::Cells(rect))?;
        let epoch = epochs.bump();

        let mut state = SlotState::default();
        state.record_staged(
            SlotBitmap { id, page: slide, width: downscaled.width, height: downscaled.height },
            epoch,
        );
        state.record_placed(id, PlacedKey::Cells(rect), epoch);

        self.thumbs.insert(slide, ThumbSlot { state, bytes: card_bytes });
        self.staging_order.push_back(slide);
        self.total_staged_bytes += card_bytes;
        Ok(())
    }

    pub fn sync_thumbnails(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        epochs: &mut SlotEpochs,
        visible_cards: &[(usize, CellRect)],
    ) -> Result<(), DisplayError> {
        // Aspect-derived sorter target: render-invariant across resizes and
        // zoom, so cached rasters and cards stay valid when the layout moves.
        let thumb_target = thumbnail_target_for_aspect(state.aspect_ratio());
        let mut visible = visible_cards.to_vec();
        let max_cards = max_sorter_cards_for_target(&thumb_target);
        visible.truncate(max_cards);

        let Some((first_slide, _)) = visible.first().copied() else {
            return self.release_all(driver);
        };

        let prefetch_key = (first_slide, visible.len(), thumb_target);
        if self.thumb_prefetch != Some(prefetch_key) {
            let plan = PrefetchPlan {
                center: first_slide,
                behind: 0,
                ahead: visible.len().saturating_sub(1),
                stage: true,
            };
            pipeline.prefetch(&plan, &thumb_target);
            self.thumb_prefetch = Some(prefetch_key);
        }

        let keep: HashSet<usize> = visible.iter().map(|(slide, _)| *slide).collect();
        self.release_except(&keep, driver)?;

        let quota = terminal_image_quota_bytes();
        let mut stages_left = MAX_THUMBNAIL_STAGES_PER_FRAME;
        let mut missed_not_loaded = false;

        for &(slide, rect) in &visible {
            // Resolve the card against the pipeline first so render failures
            // are recorded for the selected card even when it is staged.
            let source = match pipeline.entry_at(slide, &thumb_target) {
                PipelineEntry::Ready { bitmap, .. } => {
                    state.clear_render_failure_for(slide);
                    Some(CardSource::Raster(bitmap))
                }
                PipelineEntry::Failed { error } if slide == state.selected_sorter_index() => {
                    state.note_render_failure(slide, error.to_string());
                    continue;
                }
                PipelineEntry::Failed { .. } | PipelineEntry::Rendering => continue,
                // Raster evicted from the pipeline budget or its task was
                // cancelled: restage from the cached downscaled card so a
                // scroll-back never re-renders at full resolution. Without a
                // cached card, flag the miss so the window prefetch is
                // re-issued below.
                PipelineEntry::NotLoaded => match self.cached_card(slide, &thumb_target) {
                    Some(card) => Some(CardSource::Cached(card)),
                    None => {
                        missed_not_loaded = true;
                        continue;
                    }
                },
            };

            // A staged card only ever moves: reposition for the new rect
            // without retransmitting its pixels. A content change at the
            // same rect is deliberately skipped — sorter targets are
            // aspect-derived, so content changes imply a DeckReload (which
            // resets the grid) — hence the disabled epoch compare.
            let key = PlacedKey::Cells(rect);
            if let Some(thumb) = self.thumbs.get_mut(&slide) {
                let Some(id) = thumb.state.placed_id() else {
                    continue;
                };
                if !thumb.state.needs_place(id, key, false) {
                    continue;
                }
                driver.reposition(id, Placement::Cells(rect))?;
                let placed_epoch = thumb.state.staged_epoch();
                thumb.state.record_placed(id, key, placed_epoch);
                continue;
            }

            let Some(source) = source else { continue };

            if stages_left == 0 {
                continue;
            }
            stages_left -= 1;

            let downscaled = match source {
                CardSource::Cached(card) => card,
                CardSource::Raster(bitmap) => self.card_bitmap(slide, &bitmap, &thumb_target),
            };

            self.stage_card(slide, &downscaled, rect, quota, driver, epochs)?;
        }

        // Self-healing: re-issue the idempotent window prefetch while any
        // visible card stays unresolvable, so an evicted or cancelled raster
        // can never leave a card blank until the window scrolls.
        if missed_not_loaded {
            let plan = PrefetchPlan {
                center: first_slide,
                behind: 0,
                ahead: visible.len().saturating_sub(1),
                stage: true,
            };
            pipeline.prefetch(&plan, &thumb_target);
        }

        Ok(())
    }
}
