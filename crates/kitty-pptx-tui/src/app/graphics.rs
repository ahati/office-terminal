use kitty_pptx_core::{DisplayError, PipelineEntry, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{CellRect, ImageId, Placement, TerminalGraphicsDriver};

use crate::app::bindings::FrameBindings;
use crate::app::graphics_reader::ReaderGraphicsSyncer;
use crate::app::graphics_slots::{PlacedKey, PlacedRecord, SlotBitmap, SlotEpochs, SlotState};
use crate::app::graphics_thumbnails::ThumbnailSyncer;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// The two full-screen slide slots the syncer owns: the current slide and
/// the presenter next-slide preview. Slot bookkeeping lives in the shared
/// engine; this key parameterizes the policy methods over it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SlideSlot {
    Current,
    Preview,
}

/// Keeps terminal graphics in sync with the geometry the last frame drew.
///
/// The syncer stages the visible bitmap(s) on demand and places/repositions
/// the image only when the slide or its screen rect changed, so the render
/// loop never spams the terminal with duplicate image commands. Exactly one
/// view owns the screen at a time: the full-screen current slide (optionally
/// with a presenter preview) or the slide-sorter thumbnail grid.
#[derive(Default)]
pub(crate) struct GraphicsSyncer {
    current: SlotState,
    preview: SlotState,
    thumbnails: ThumbnailSyncer,
    reader: ReaderGraphicsSyncer,
    suppressed: bool,
    /// One staging-epoch discipline shared by all three syncers.
    epochs: SlotEpochs,
}

impl GraphicsSyncer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Forgets all staging/placement bookkeeping (e.g. after a teardown).
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Forgets staging/placement bookkeeping after a resize but keeps the
    /// sorter's downscaled-card cache: the sorter target is aspect-derived, so
    /// a resize never invalidates the cached cards.
    pub(crate) fn reset_preserving_thumbnail_cache(&mut self) {
        let cache = std::mem::take(&mut self.thumbnails.downscale_cache);
        *self = Self::default();
        self.thumbnails.downscale_cache = cache;
    }

    /// Synchronizes staged images and placements with the current frame.
    /// A failed background render is recorded on the state (canvas note +
    /// warning entry) instead of silently leaving the slide blank.
    ///
    /// `bindings` carries the geometry the just-drawn frame produced;
    /// `canvas_target` is the resolution the full-screen view is rendered
    /// at: the sorter shares those bitmaps and lets the terminal scale them
    /// into the cards, so both views show the exact same pixels.
    pub(crate) fn sync(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        canvas_target: &RenderTargetSpec,
        bindings: &FrameBindings,
    ) -> Result<(), DisplayError> {
        if state.total_slides() == 0 {
            return self.clear_everything(driver);
        }
        if state.is_blank() {
            return self.suppress(driver);
        }
        self.suppressed = false;

        // When an overlay modal is active (search, help, find, etc.), underlying
        // graphics stay resident and must not do expensive staging, downscaling,
        // or transmission on the UI thread while the user interacts with the modal.
        if state.mode().is_overlay() {
            return Ok(());
        }

        // Resolve the base view exactly like the renderer does, so an overlay
        // modal on top of the sorter does not tear the grid images down.
        if state.base_mode() == AppMode::Reader {
            self.release_thumbnails(driver)?;
            self.release_slot(SlideSlot::Current, driver)?;
            self.release_slot(SlideSlot::Preview, driver)?;
            return self.reader.sync(
                state,
                pipeline,
                driver,
                canvas_target,
                &mut self.epochs,
                bindings,
            );
        }

        self.reader.release_all(driver)?;
        if state.base_mode() == AppMode::Sorter {
            return self.sync_thumbnails(state, pipeline, driver, bindings);
        }

        self.release_thumbnails(driver)?;
        self.sync_slide(state, pipeline, driver, canvas_target, bindings)
    }

    /// Syncs the full-screen slide and the presenter next-slide preview.
    fn sync_slide(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        canvas_target: &RenderTargetSpec,
        bindings: &FrameBindings,
    ) -> Result<(), DisplayError> {
        let current_rect = bindings.current;
        let preview_rect = bindings.preview;

        match current_rect {
            Some(rect) => {
                let id = state.current_image_id();
                let slide = state.current_slide();
                if self.ensure_staged(
                    slide,
                    id,
                    SlideSlot::Current,
                    pipeline,
                    driver,
                    state,
                    canvas_target,
                )? {
                    state.clear_render_failure_for(slide);
                    self.place_slot(SlideSlot::Current, id, rect, driver)?;
                }
            }
            None => self.release_slot(SlideSlot::Current, driver)?,
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
                if self.ensure_staged(
                    slide,
                    id,
                    SlideSlot::Preview,
                    pipeline,
                    driver,
                    state,
                    canvas_target,
                )? {
                    self.place_slot(SlideSlot::Preview, id, rect, driver)?;
                }
            }
            None => self.release_slot(SlideSlot::Preview, driver)?,
        }

        Ok(())
    }

    /// Syncs the slide-sorter grid: one bitmap per visible card.
    ///
    /// Cards render at a dedicated aspect-derived sorter target (see
    /// [`crate::app::graphics_thumbnails::thumbnail_target_for_aspect`]) —
    /// not the canvas raster. 1280 px keeps text runs above the rasterizer's
    /// 6 px font floor while cutting rasterization cost ~5x; PDF pages even
    /// gain sharpness because their 2560 px canonical source is finally
    /// downsampled instead of upsampled past. The dedicated target is also
    /// resize- and zoom-invariant, so sorter rasters survive layout changes.
    ///
    /// Only visible cards are staged; cards that scroll out are released so the
    /// resident-id pool stays bounded.
    fn sync_thumbnails(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        bindings: &FrameBindings,
    ) -> Result<(), DisplayError> {
        self.release_slot(SlideSlot::Current, driver)?;
        self.release_slot(SlideSlot::Preview, driver)?;
        self.thumbnails.sync_thumbnails(
            state,
            pipeline,
            driver,
            &mut self.epochs,
            &bindings.thumbnails,
        )
    }

    /// Releases all staged thumbnails (leaving the sorter). The downscaled-card
    /// cache is kept: the deck is unchanged, so re-entering the sorter restages
    /// from cache instead of re-rendering every visible slide.
    fn release_thumbnails(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        self.thumbnails.release_all_keep_cache(driver)
    }

    /// Stages `slide` under `id` when the slot does not already hold it.
    ///
    /// Returns `true` when the bitmap is ready on the terminal. A failed
    /// render is noted on the state instead of blocking forever silently.
    #[allow(clippy::too_many_arguments)]
    fn ensure_staged(
        &mut self,
        slide: usize,
        id: ImageId,
        slot_key: SlideSlot,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        state: &mut PresentationState,
        canvas_target: &RenderTargetSpec,
    ) -> Result<bool, DisplayError> {
        match pipeline.entry(slide) {
            PipelineEntry::Ready { bitmap, .. } => {
                let want =
                    SlotBitmap { id, page: slide, width: bitmap.width, height: bitmap.height };
                if !self.slot(slot_key).needs_stage(&want) {
                    return Ok(true);
                }

                // Prefer the worker's pre-encoded payload: one batched write,
                // no on-thread encode. Fall back to encoding here when the
                // worker has not produced one (sync renders, payload-cache
                // eviction) or the driver cannot transmit prebuilt bytes.
                match pipeline.staged_payload(slide, canvas_target) {
                    Some(payload) => driver
                        .transmit_prebuilt(id, bitmap.width, bitmap.height, &payload)
                        .or_else(|_| driver.stage(&bitmap, id))?,
                    None => driver.stage(&bitmap, id)?,
                }
                let epoch = self.epochs.bump();
                self.slot_mut(slot_key).record_staged(want, epoch);
                Ok(true)
            }
            PipelineEntry::Failed { error } => {
                state.note_render_failure(slide, error.to_string());
                // Keep presenting an already staged bitmap if we have one.
                Ok(self.stages(slot_key, id, slide))
            }
            // Rendering / NotLoaded: keep presenting an already staged bitmap.
            _ => Ok(self.stages(slot_key, id, slide)),
        }
    }

    /// Whether a slot still stages `id` for `slide` — kept across render
    /// hiccups so the view never flashes blank.
    fn stages(&self, slot_key: SlideSlot, id: ImageId, slide: usize) -> bool {
        self.slot(slot_key)
            .staged()
            .is_some_and(|staged| staged.id == id && staged.page == slide)
    }

    /// Places (or repositions) one full-screen slot's image at `rect`.
    ///
    /// Same id: re-place for new data/rect; kitty wipes the old placement
    /// when the same image id is placed again. Different id: release the
    /// old placement first, then present.
    fn place_slot(
        &mut self,
        slot_key: SlideSlot,
        id: ImageId,
        rect: CellRect,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let key = PlacedKey::Cells(rect);
        if !self.slot(slot_key).needs_place(id, key, true) {
            return Ok(());
        }

        if let Some(previous) = self.slot_mut(slot_key).take_placed() {
            if previous.id == id {
                driver.reposition(id, Placement::Cells(rect))?;
                self.record_placement(slot_key, id, key);
                return Ok(());
            }
            self.drop_placed(slot_key, previous, driver)?;
        }

        driver.present(id, Placement::Cells(rect))?;
        self.record_placement(slot_key, id, key);
        Ok(())
    }

    /// Records a placement against the slot's current staged epoch.
    fn record_placement(&mut self, slot_key: SlideSlot, id: ImageId, key: PlacedKey) {
        let epoch = self.slot(slot_key).staged_epoch();
        self.slot_mut(slot_key).record_placed(id, key, epoch);
    }

    /// Releases a placement the slot no longer shows and forgets the
    /// staged key when it matches the released id (the driver dropped that
    /// bitmap; a different staged bitmap stays valid).
    fn drop_placed(
        &mut self,
        slot_key: SlideSlot,
        previous: PlacedRecord,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        driver.release(previous.id)?;
        if self.slot(slot_key).staged_holds(previous.id) {
            self.slot_mut(slot_key).take_staged();
        }
        Ok(())
    }

    /// Releases a slot's image and invalidates its staged key.
    fn release_slot(
        &mut self,
        slot_key: SlideSlot,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let Some(previous) = self.slot_mut(slot_key).take_placed() else {
            return Ok(());
        };
        self.drop_placed(slot_key, previous, driver)
    }

    fn slot(&self, slot_key: SlideSlot) -> &SlotState {
        match slot_key {
            SlideSlot::Current => &self.current,
            SlideSlot::Preview => &self.preview,
        }
    }

    fn slot_mut(&mut self, slot_key: SlideSlot) -> &mut SlotState {
        match slot_key {
            SlideSlot::Current => &mut self.current,
            SlideSlot::Preview => &mut self.preview,
        }
    }

    /// Empty deck: nothing may remain on screen.
    fn clear_everything(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        if self.current.is_clear() && self.preview.is_clear() && self.thumbnails.thumbs.is_empty() {
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
