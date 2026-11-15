use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use kitty_pptx_core::{DisplayError, PipelineEntry, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{
    CellRect, ImageId, Placement, TerminalGraphicsDriver, MAX_THUMBNAIL_IMAGES,
    THUMBNAIL_IMAGE_ID_BASE,
};

use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// Identity of the bitmap currently staged on the terminal for a slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct StagedImage {
    id: ImageId,
    slide: usize,
    width: u32,
    height: u32,
    /// Monotonic staging epoch; a new epoch means the image data changed even
    /// when the image id and placement rect stayed the same (e.g. slide N and
    /// slide N+8 share an id).
    epoch: u64,
}

/// Identity of the image currently placed on screen for a slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PlacedImage {
    id: ImageId,
    rect: CellRect,
    epoch: u64,
}

/// A slide-sorter card currently staged on the terminal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ThumbSlot {
    id: ImageId,
    slide: usize,
    rect: CellRect,
    /// Pixel target the resident bitmap was rendered for. A card resize that
    /// changes the target needs a re-transmission, not just a reposition.
    target: RenderTargetSpec,
}

/// Upper bound on newly transmitted thumbnails per frame. Every `stage` encodes
/// and writes a PNG, so filling a large grid in a single frame would stall the
/// event loop; the remaining cards arrive over the next few frames.
const MAX_THUMBNAIL_STAGES_PER_FRAME: usize = 4;

/// Keeps terminal graphics in sync with the rects recorded by the last frame.
///
/// The syncer stages the visible bitmap(s) on demand and places/repositions the
/// image only when the slide or its screen rect changed, so the render loop
/// never spams the terminal with duplicate image commands. Exactly one view
/// owns the screen at a time: the full-screen current slide (optionally with a
/// presenter preview) or the slide-sorter thumbnail grid.
#[derive(Default)]
pub(crate) struct GraphicsSyncer {
    current_staged: Option<StagedImage>,
    preview_staged: Option<StagedImage>,
    current_placed: Option<PlacedImage>,
    preview_placed: Option<PlacedImage>,
    /// Visible sorter cards, keyed by slide index for stable id reuse.
    thumbs: BTreeMap<usize, ThumbSlot>,
    suppressed: bool,
    epoch: u64,
}

impl GraphicsSyncer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Forgets all staging/placement bookkeeping (e.g. after a resize).
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Synchronizes staged images and placements with the current frame.
    /// A failed background render is recorded on the state (canvas note +
    /// warning entry) instead of silently leaving the slide blank.
    pub(crate) fn sync(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        if state.total_slides() == 0 {
            return self.clear_everything(driver);
        }
        if state.is_blank() {
            return self.suppress(driver);
        }
        self.suppressed = false;

        // Resolve the base view exactly like the renderer does, so an overlay
        // modal on top of the sorter does not tear the grid images down.
        if state.base_mode() == AppMode::Sorter {
            return self.sync_thumbnails(state, pipeline, driver);
        }

        self.release_thumbnails(driver)?;
        self.sync_slide(state, pipeline, driver)
    }

    /// Syncs the full-screen slide and the presenter next-slide preview.
    fn sync_slide(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let (current_rect, preview_rect) = state.placement_rects();

        match current_rect {
            Some(rect) => {
                let id = state.current_image_id();
                let slide = state.current_slide();
                if self.ensure_staged(slide, id, pipeline, driver, false, state)? {
                    state.clear_render_failure_for(slide);
                    self.place_current(id, rect, driver)?;
                }
            }
            None => self.release_current(driver)?,
        }

        let preview = if state.mode() == AppMode::PresenterConsole {
            match (
                state.next_slide_index(),
                state.next_image_id(),
                preview_rect,
            ) {
                (Some(slide), Some(id), Some(rect)) => Some((slide, id, rect)),
                _ => None,
            }
        } else {
            None
        };

        match preview {
            Some((slide, id, rect)) => {
                if self.ensure_staged(slide, id, pipeline, driver, true, state)? {
                    self.place_preview(id, rect, driver)?;
                }
            }
            None => self.release_preview(driver)?,
        }

        Ok(())
    }

    /// Syncs the slide-sorter grid: one bitmap per visible card.
    ///
    /// Only visible cards are staged; cards that scroll out are released so the
    /// resident-id pool stays bounded. Thumbnails render from a card-sized
    /// target instead of the full-canvas raster.
    fn sync_thumbnails(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        // The sorter replaces the full-screen canvas: no slide image may stay
        // on screen behind the grid.
        self.release_current(driver)?;
        self.release_preview(driver)?;

        let mut visible = state.thumbnail_placement_rects();
        visible.truncate(MAX_THUMBNAIL_IMAGES as usize);
        let Some((_, first_rect)) = visible.first() else {
            return self.release_thumbnails(driver);
        };

        let target = thumbnail_target(state.aspect_ratio(), *first_rect);

        // Queue the visible window before staging: the pipeline renders on its
        // workers at the card target, so the next frames can stage immediately.
        let slides: Vec<usize> = visible.iter().map(|(slide, _)| *slide).collect();
        pipeline.prefetch_thumbnails(&slides, &target);

        // Cards that scrolled out of view free their resident ids.
        let keep: HashSet<usize> = visible.iter().map(|(slide, _)| *slide).collect();
        self.release_thumbnails_except(&keep, driver)?;

        let mut stages_left = MAX_THUMBNAIL_STAGES_PER_FRAME;
        for (slide, rect) in visible {
            match pipeline.entry_at(slide, &target) {
                PipelineEntry::Ready { bitmap, .. } => {
                    state.clear_render_failure_for(slide);

                    let staged = self
                        .thumbs
                        .get(&slide)
                        .map(|slot| (slot.id, slot.rect, slot.target));
                    match staged {
                        // Already on screen at the right size and position.
                        Some((_, prev_rect, prev_target))
                            if prev_rect == rect && prev_target == target =>
                        {
                            continue
                        }
                        // Same bitmap, new card geometry: move it, no re-transmit.
                        Some((id, _, prev_target)) if prev_target == target => {
                            driver.reposition(id, Placement::Cells(rect))?;
                            if let Some(slot) = self.thumbs.get_mut(&slide) {
                                slot.rect = rect;
                            }
                            continue;
                        }
                        // New card, or the card was resized to a new target.
                        _ => {}
                    }

                    if stages_left == 0 {
                        continue;
                    }
                    stages_left -= 1;

                    let id = self.thumbnail_id(slide);
                    driver.stage(&bitmap, id)?;
                    driver.present(id, Placement::Cells(rect))?;
                    self.epoch = self.epoch.saturating_add(1);
                    self.thumbs.insert(
                        slide,
                        ThumbSlot {
                            id,
                            slide,
                            rect,
                            target,
                        },
                    );
                }
                // Only the card under the cursor is reported: a broken deck
                // would otherwise file one drawer entry per visible card.
                PipelineEntry::Failed { error } if slide == state.selected_sorter_index() => {
                    state.note_render_failure(slide, error.to_string());
                }
                PipelineEntry::Failed { .. } => {}
                // Rendering / NotLoaded: the card fills in on a later frame.
                _ => {}
            }
        }

        Ok(())
    }

    /// Lowest free thumbnail id, stable for as long as the card stays visible.
    ///
    /// Re-assigning by grid position instead would re-transmit every card on
    /// every scroll step.
    fn thumbnail_id(&self, slide: usize) -> ImageId {
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

    /// Releases every thumbnail whose slide is no longer visible.
    fn release_thumbnails_except(
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
                driver.release(slot.id)?;
            }
        }
        Ok(())
    }

    /// Releases all staged thumbnails (leaving the sorter).
    fn release_thumbnails(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        if self.thumbs.is_empty() {
            return Ok(());
        }
        let ids: Vec<ImageId> = self.thumbs.values().map(|slot| slot.id).collect();
        self.thumbs.clear();
        for id in ids {
            driver.release(id)?;
        }
        Ok(())
    }

    /// Stages `slide` under `id` when the slot does not already hold it.
    ///
    /// Returns `true` when the bitmap is ready on the terminal. A failed
    /// render is noted on the state instead of blocking forever silently.
    fn ensure_staged(
        &mut self,
        slide: usize,
        id: ImageId,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        preview: bool,
        state: &mut PresentationState,
    ) -> Result<bool, DisplayError> {
        let staged = if preview {
            self.preview_staged
        } else {
            self.current_staged
        };
        match pipeline.entry(slide) {
            PipelineEntry::Ready { bitmap, .. } => {
                if staged.is_some_and(|s| {
                    s.id == id
                        && s.slide == slide
                        && s.width == bitmap.width
                        && s.height == bitmap.height
                }) {
                    return Ok(true);
                }

                driver.stage(&bitmap, id)?;

                self.epoch = self.epoch.saturating_add(1);
                let key = StagedImage {
                    id,
                    slide,
                    width: bitmap.width,
                    height: bitmap.height,
                    epoch: self.epoch,
                };
                if preview {
                    self.preview_staged = Some(key);
                } else {
                    self.current_staged = Some(key);
                }
                Ok(true)
            }
            PipelineEntry::Failed { error } => {
                state.note_render_failure(slide, error.to_string());
                // Keep presenting an already staged bitmap if we have one.
                Ok(staged.is_some_and(|s| s.id == id && s.slide == slide))
            }
            // Rendering / NotLoaded: keep presenting an already staged bitmap.
            _ => Ok(staged.is_some_and(|s| s.id == id && s.slide == slide)),
        }
    }

    fn place_current(
        &mut self,
        id: ImageId,
        rect: CellRect,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let epoch = self.current_staged.map(|s| s.epoch).unwrap_or(0);
        if let Some(previous) = self.current_placed {
            if previous.id == id {
                if previous.rect == rect && previous.epoch == epoch {
                    return Ok(());
                }
                // Same id: re-place for new data/rect; kitty wipes the old
                // placement when the same image id is placed again.
                self.present_current(id, rect, driver, true)?;
                self.current_placed = Some(PlacedImage { id, rect, epoch });
                return Ok(());
            }
            self.release_current(driver)?;
        }

        self.present_current(id, rect, driver, false)?;
        self.current_placed = Some(PlacedImage { id, rect, epoch });
        Ok(())
    }

    fn place_preview(
        &mut self,
        id: ImageId,
        rect: CellRect,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let epoch = self.preview_staged.map(|s| s.epoch).unwrap_or(0);
        if let Some(previous) = self.preview_placed {
            if previous.id == id {
                if previous.rect == rect && previous.epoch == epoch {
                    return Ok(());
                }
                driver.reposition(id, Placement::Cells(rect))?;
                self.preview_placed = Some(PlacedImage { id, rect, epoch });
                return Ok(());
            }
            self.release_preview(driver)?;
        }

        driver.present(id, Placement::Cells(rect))?;
        self.preview_placed = Some(PlacedImage { id, rect, epoch });
        Ok(())
    }

    fn present_current(
        &mut self,
        id: ImageId,
        rect: CellRect,
        driver: &mut dyn TerminalGraphicsDriver,
        reposition: bool,
    ) -> Result<(), DisplayError> {
        let placement = Placement::Cells(rect);
        if reposition {
            driver.reposition(id, placement)
        } else {
            driver.present(id, placement)
        }
    }

    /// Releases the current image and invalidates its staged key.
    fn release_current(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let Some(previous) = self.current_placed.take() else {
            return Ok(());
        };
        driver.release(previous.id)?;
        if self.current_staged.is_some_and(|s| s.id == previous.id) {
            self.current_staged = None;
        }
        Ok(())
    }

    /// Releases the preview image and invalidates its staged key.
    fn release_preview(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let Some(previous) = self.preview_placed.take() else {
            return Ok(());
        };
        driver.release(previous.id)?;
        if self.preview_staged.is_some_and(|s| s.id == previous.id) {
            self.preview_staged = None;
        }
        Ok(())
    }

    /// Empty deck: nothing may remain on screen.
    fn clear_everything(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        if self.current_placed.is_none() && self.preview_placed.is_none() && self.thumbs.is_empty()
        {
            return Ok(());
        }
        driver.release_all()?;
        self.reset();
        Ok(())
    }

    /// Blank screen: hide all graphics until the slide is shown again.
    fn suppress(&mut self, driver: &mut dyn TerminalGraphicsDriver) -> Result<(), DisplayError> {
        if self.suppressed {
            return Ok(());
        }
        driver.release_all()?;
        self.reset();
        self.suppressed = true;
        Ok(())
    }
}

/// Pixel target for a sorter card.
///
/// Thumbnails must not transmit full-canvas rasters: the card is a few hundred
/// pixels wide, so the raster is derived from its cell footprint using the same
/// ~10x20 px-per-cell heuristic as the main canvas target.
fn thumbnail_target(aspect_ratio: f32, rect: CellRect) -> RenderTargetSpec {
    let width_px = (u32::from(rect.cols.max(1)) * 10).clamp(160, 640);
    let aspect = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio
    } else {
        16.0 / 9.0
    };
    let height_px = ((f64::from(width_px) / f64::from(aspect)).round() as u32).max(1);
    RenderTargetSpec {
        width_px,
        height_px,
    }
}

/// Terminal-graphics placement state accessors.
impl PresentationState {
    // ─── Terminal graphics placement ───────────────────────────────

    /// Shared sink where the current slide view records its screen rect.
    pub fn placement_sink(&self) -> Arc<Mutex<Option<CellRect>>> {
        Arc::clone(&self.placement_rect)
    }

    /// Shared sink where the presenter next-slide preview records its rect.
    pub fn preview_placement_sink(&self) -> Arc<Mutex<Option<CellRect>>> {
        Arc::clone(&self.preview_placement_rect)
    }

    /// Shared sink where the sorter grid records one rect per visible card.
    pub fn thumbnail_placement_sink(&self) -> crate::widget::RectListSink {
        Arc::clone(&self.thumbnail_placement_rects)
    }

    /// `(slide, rect)` pairs recorded by the last sorter frame.
    pub fn thumbnail_placement_rects(&self) -> Vec<(usize, CellRect)> {
        self.thumbnail_placement_rects
            .lock()
            .map(|rects| rects.clone())
            .unwrap_or_default()
    }

    /// Current `(slide, preview)` placement rects recorded by the last frame.
    pub fn placement_rects(&self) -> (Option<CellRect>, Option<CellRect>) {
        (
            read_placement(&self.placement_rect),
            read_placement(&self.preview_placement_rect),
        )
    }

    /// Clears recorded rects before a new frame is drawn.
    pub fn clear_placement_rects(&self) {
        clear_placement(&self.placement_rect);
        clear_placement(&self.preview_placement_rect);
        if let Ok(mut rects) = self.thumbnail_placement_rects.lock() {
            rects.clear();
        }
    }
}

/// Reads a placement sink, tolerating a poisoned mutex.
fn read_placement(sink: &Arc<Mutex<Option<CellRect>>>) -> Option<CellRect> {
    sink.lock().ok().and_then(|guard| *guard)
}

/// Resets a placement sink, tolerating a poisoned mutex.
fn clear_placement(sink: &Arc<Mutex<Option<CellRect>>>) {
    if let Ok(mut guard) = sink.lock() {
        *guard = None;
    }
}
