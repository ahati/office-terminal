use office_terminal_core::{DisplayError, PipelineEntry, RenderPipeline, RenderTargetSpec};
use office_terminal_display::{
    ImageId, Placement, TerminalGraphicsDriver, CURRENT_IMAGE_ID_BASE, CURRENT_IMAGE_ID_SLOTS, PREVIEW_IMAGE_ID_BASE,
    PREVIEW_IMAGE_ID_SLOTS,
};

use crate::app::bindings::{FrameBindings, SlidePlacement};
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

impl SlideSlot {
    /// Image-id pool the display layer reserves for this slot.
    ///
    /// The pools are disjoint, so a swap inside one slot can never collide
    /// with the other slot's resident image.
    fn id_pool(self) -> (u32, u32) {
        match self {
            SlideSlot::Current => (CURRENT_IMAGE_ID_BASE, CURRENT_IMAGE_ID_SLOTS),
            SlideSlot::Preview => (PREVIEW_IMAGE_ID_BASE, PREVIEW_IMAGE_ID_SLOTS),
        }
    }
}

/// What resolving a slot against the pipeline produced for this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StageLookup {
    /// The slot's pixels are on the terminal under this id.
    Ready(ImageId),
    /// The bitmap is still being rendered; the screen must stay as it is.
    Rendering,
    /// Nothing to show and nothing pending (render failure, unsupported).
    Unavailable,
}

/// Keeps terminal graphics in sync with the geometry the last frame drew.
///
/// The syncer stages the visible bitmap(s) on demand and places/repositions
/// the image only when the slide or its screen rect changed, so the render
/// loop never spams the terminal with duplicate image commands. Exactly one
/// view owns the screen at a time: the full-screen current slide (optionally
/// with a presenter preview) or the slide-sorter thumbnail grid.
#[derive(Default)]
pub struct GraphicsSyncer {
    current: SlotState,
    preview: SlotState,
    thumbnails: ThumbnailSyncer,
    reader: ReaderGraphicsSyncer,
    suppressed: bool,
    /// One staging-epoch discipline shared by all three syncers.
    epochs: SlotEpochs,
}

impl GraphicsSyncer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets all staging/placement bookkeeping (e.g. after a teardown).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Synchronizes staged images and placements with the current frame.
    /// A failed background render is recorded on the state (canvas note +
    /// warning entry) instead of silently leaving the slide blank.
    ///
    /// Returns how many sorter cards the per-frame staging throttle deferred:
    /// their rasters are already resolvable, so only another drawn frame can
    /// place them.
    ///
    /// `bindings` carries the geometry the just-drawn frame produced;
    /// `canvas_target` is the resolution the full-screen view is rendered
    /// at: the sorter shares those bitmaps and lets the terminal scale them
    /// into the cards, so both views show the exact same pixels.
    pub fn sync(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        canvas_target: &RenderTargetSpec,
        bindings: &FrameBindings,
    ) -> Result<usize, DisplayError> {
        if state.total_slides() == 0 {
            return self.clear_everything(driver).map(|_| 0);
        }
        if state.is_blank() {
            return self.suppress(driver).map(|_| 0);
        }
        self.suppressed = false;

        // When an overlay modal is active (search, help, find, etc.), underlying
        // graphics stay resident and must not do expensive staging, downscaling,
        // or transmission on the UI thread while the user interacts with the modal.
        if state.mode().is_overlay() {
            return Ok(0);
        }

        // Resolve the base view exactly like the renderer does, so an overlay
        // modal on top of the sorter does not tear the grid images down.
        if state.base_mode() == AppMode::Reader {
            self.release_slot(SlideSlot::Current, driver)?;
            self.release_slot(SlideSlot::Preview, driver)?;
            self.reader
                .sync(state, pipeline, driver, canvas_target, &mut self.epochs, bindings)?;
            // The sorter's cards are torn down only once the reader actually
            // presents its panes — the same present-then-release discipline
            // as the slide path below. Releasing them first would blank the
            // screen for the whole raster window whenever the returning
            // page's reader raster was evicted during the sorter visit:
            // the reader would come back dark, as if the jump never landed.
            if self.reader.has_placements() {
                self.release_thumbnails(driver)?;
            }
            return Ok(0);
        }

        self.reader.release_all(driver)?;
        if state.base_mode() == AppMode::Sorter {
            return self.sync_thumbnails(state, pipeline, driver, bindings);
        }

        // The sorter grid is only torn down once the slide view actually owns
        // the screen: releasing the cards while the canvas bitmap is still
        // being rendered would blank the whole screen for the length of that
        // render. `sync_slide` presents the slide (and every swap is
        // present-then-release), so the grid never leaves a hole behind.
        let slide_pending = self.sync_slide(state, pipeline, driver, canvas_target, bindings)?;
        if !slide_pending {
            self.release_thumbnails(driver)?;
        }
        Ok(0)
    }

    /// Syncs the full-screen slide and the presenter next-slide preview.
    ///
    /// Returns whether the current slot is still waiting for its first bitmap:
    /// the grid then stays resident instead of being released.
    fn sync_slide(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        canvas_target: &RenderTargetSpec,
        bindings: &FrameBindings,
    ) -> Result<bool, DisplayError> {
        let current = bindings.current;
        let preview = bindings.preview;
        let mut pending = false;

        match current {
            Some(placement) => {
                let slide = state.current_slide();
                match self.ensure_staged(
                    slide,
                    state.current_image_id(),
                    SlideSlot::Current,
                    pipeline,
                    driver,
                    state,
                    canvas_target,
                )? {
                    StageLookup::Ready(id) => {
                        state.clear_render_failure_for(slide);
                        self.place_slot(SlideSlot::Current, id, &placement, driver)?;
                    }
                    // First bitmap still in flight and nothing to keep on
                    // screen: hold whatever the previous view left up.
                    StageLookup::Rendering => pending = self.current.is_clear(),
                    StageLookup::Unavailable => {}
                }
            }
            None => self.release_slot(SlideSlot::Current, driver)?,
        }

        let preview = if state.mode() == AppMode::PresenterConsole {
            match (state.next_slide_index(), state.next_image_id(), preview) {
                (Some(slide), Some(id), Some(placement)) => Some((slide, id, placement)),
                _ => None,
            }
        } else {
            None
        };

        match preview {
            Some((slide, id, placement)) => {
                if let StageLookup::Ready(id) =
                    self.ensure_staged(slide, id, SlideSlot::Preview, pipeline, driver, state, canvas_target)?
                {
                    self.place_slot(SlideSlot::Preview, id, &placement, driver)?;
                }
            }
            None => self.release_slot(SlideSlot::Preview, driver)?,
        }

        Ok(pending)
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
    ) -> Result<usize, DisplayError> {
        self.release_slot(SlideSlot::Current, driver)?;
        self.release_slot(SlideSlot::Preview, driver)?;
        self.thumbnails
            .sync_thumbnails(state, pipeline, driver, &mut self.epochs, &bindings.thumbnails)
    }

    /// Releases all staged thumbnails (leaving the sorter). The downscaled-card
    /// cache is kept: the deck is unchanged, so re-entering the sorter restages
    /// from cache instead of re-rendering every visible slide.
    pub(crate) fn release_thumbnails(&mut self, driver: &mut dyn TerminalGraphicsDriver) -> Result<(), DisplayError> {
        self.thumbnails.release_all_keep_cache(driver)
    }

    /// Stages `slide` into its slot when the slot does not already hold those
    /// pixels. A failed render is noted on the state instead of blocking
    /// forever silently.
    #[allow(clippy::too_many_arguments)]
    fn ensure_staged(
        &mut self,
        slide: usize,
        preferred: ImageId,
        slot_key: SlideSlot,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        state: &mut PresentationState,
        canvas_target: &RenderTargetSpec,
    ) -> Result<StageLookup, DisplayError> {
        match pipeline.entry(slide) {
            PipelineEntry::Ready { bitmap, .. } => {
                let want = SlotBitmap {
                    id: preferred,
                    page: slide,
                    width: bitmap.width,
                    height: bitmap.height,
                };
                // The same pixels may already be resident under the spare id
                // of a completed swap: keep them instead of re-sending.
                if let Some(staged_id) = self.staged_equivalent(slot_key, &want) {
                    return Ok(StageLookup::Ready(staged_id));
                }

                let id = self.stage_id(slot_key, preferred);
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
                self.slot_mut(slot_key).record_staged(SlotBitmap { id, ..want }, epoch);
                Ok(StageLookup::Ready(id))
            }
            PipelineEntry::Failed { error } => {
                state.note_render_failure(slide, error.to_string());
                // Keep presenting an already staged bitmap if we have one.
                Ok(self.keep_staged(slot_key, slide))
            }
            // Still rendering: keep presenting whatever the slot holds for
            // this slide, and tell the caller the first bitmap has not
            // arrived when it holds nothing at all.
            PipelineEntry::Rendering => Ok(self
                .staged_for(slot_key, slide)
                .map_or(StageLookup::Rendering, StageLookup::Ready)),
            PipelineEntry::NotLoaded => Ok(self.keep_staged(slot_key, slide)),
        }
    }

    /// The id of a slot bitmap that already holds exactly these pixels.
    fn staged_equivalent(&self, slot_key: SlideSlot, want: &SlotBitmap) -> Option<ImageId> {
        self.slot(slot_key).staged().and_then(|staged| {
            (staged.page == want.page && staged.width == want.width && staged.height == want.height)
                .then_some(staged.id)
        })
    }

    /// Keep presenting the bitmap the slot already stages for `slide` (across
    /// render hiccups) so the view never flashes blank.
    fn keep_staged(&self, slot_key: SlideSlot, slide: usize) -> StageLookup {
        self.staged_for(slot_key, slide)
            .map_or(StageLookup::Unavailable, StageLookup::Ready)
    }

    /// Id of the bitmap the slot already holds for `slide`, if any.
    fn staged_for(&self, slot_key: SlideSlot, slide: usize) -> Option<ImageId> {
        self.slot(slot_key)
            .staged()
            .filter(|staged| staged.page == slide)
            .map(|staged| staged.id)
    }

    /// Id to transmit fresh pixels under.
    ///
    /// Kitty deletes an image *and every placement of it* the moment new data
    /// is transmitted for its id: "when re-transmitting image data for a
    /// specific id, the existing image and all its placements must be deleted
    /// ... the new data ... is not actually displayed until a placement for it
    /// is created". Re-staging over a displayed slot therefore blanks the
    /// slide for the whole transmit + decode window — the flicker seen when
    /// the canvas upgrades from the sorter's card raster (or a pre-resize
    /// frame) to the canvas raster. While the preferred id is on screen the
    /// slot stages into a spare id from its own pool instead; [`Self::place_slot`]
    /// then presents the new bitmap before releasing the old one, so the slot
    /// is never empty.
    fn stage_id(&self, slot_key: SlideSlot, preferred: ImageId) -> ImageId {
        if self.slot(slot_key).placed_id() != Some(preferred) {
            return preferred;
        }
        let (base, slots) = slot_key.id_pool();
        let offset = preferred.0.saturating_sub(base);
        ImageId(base + (offset + 1) % slots.max(1))
    }

    /// Places (or repositions) one slide slot's image for `placement`.
    ///
    /// Zoomed placements carry a crop window: the staged raster's visible
    /// region goes to the cells as a `CellsSubrect` placement (the same
    /// mechanism the reader panes use), computed against the staged bitmap's
    /// own dimensions so it can never reference pixels outside the raster.
    ///
    /// Same id: re-place for the new geometry — kitty replaces the placement
    /// of that (image id, placement id) pair without flicker, which is what
    /// makes zoom a pure placement change. Different id: present the
    /// incoming bitmap first and release the outgoing one afterwards, so
    /// the slot never holds an empty screen between the two.
    fn place_slot(
        &mut self,
        slot_key: SlideSlot,
        id: ImageId,
        placement: &SlidePlacement,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        let key = match placement.crop {
            None => PlacedKey::Cells(placement.rect),
            Some(crop) => {
                // The staged bitmap's dimensions define the raster the crop
                // indexes into; a placement only happens with a stage.
                let (width, height) = self
                    .slot(slot_key)
                    .staged()
                    .map(|staged| (staged.width, staged.height))
                    .unwrap_or((1, 1));
                PlacedKey::Subrect {
                    rect: placement.rect,
                    src: crop.to_pixel_rect(width, height),
                }
            }
        };
        if !self.slot(slot_key).needs_place(id, key, true) {
            return Ok(());
        }

        let driver_placement = match key {
            PlacedKey::Cells(rect) => Placement::Cells(rect),
            PlacedKey::Subrect { rect, src } => Placement::CellsSubrect { rect, src },
        };

        match self.slot_mut(slot_key).take_placed() {
            Some(previous) if previous.id == id => {
                driver.reposition(id, driver_placement)?;
            }
            Some(previous) => {
                driver.present(id, driver_placement)?;
                self.drop_placed(slot_key, previous, driver)?;
            }
            None => driver.present(id, driver_placement)?,
        }
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
    fn clear_everything(&mut self, driver: &mut dyn TerminalGraphicsDriver) -> Result<(), DisplayError> {
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
