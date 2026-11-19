use kitty_pptx_core::{DisplayError, PipelineEntry, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::{CellRect, ImageId, Placement, TerminalGraphicsDriver};

use crate::app::bindings::FrameBindings;
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

/// Keeps terminal graphics in sync with the geometry the last frame drew.
///
/// The syncer stages the visible bitmap(s) on demand and places/repositions
/// the image only when the slide or its screen rect changed, so the render
/// loop never spams the terminal with duplicate image commands. Exactly one
/// view owns the screen at a time: the full-screen current slide (optionally
/// with a presenter preview) or the slide-sorter thumbnail grid.
#[derive(Default)]
pub(crate) struct GraphicsSyncer {
    current_staged: Option<StagedImage>,
    preview_staged: Option<StagedImage>,
    current_placed: Option<PlacedImage>,
    preview_placed: Option<PlacedImage>,
    thumbnails: crate::app::graphics_thumbnails::ThumbnailSyncer,
    reader: crate::app::graphics_reader::ReaderGraphicsSyncer,
    suppressed: bool,
    epoch: u64,
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
            self.release_current(driver)?;
            self.release_preview(driver)?;
            return self
                .reader
                .sync(state, pipeline, driver, canvas_target, &mut self.epoch, bindings);
        }

        self.reader.release_all(driver)?;
        if state.base_mode() == AppMode::Sorter {
            return self.sync_thumbnails(state, pipeline, driver, bindings);
        }

        self.release_thumbnails(driver)?;
        self.sync_slide(state, pipeline, driver, bindings)
    }

    /// Syncs the full-screen slide and the presenter next-slide preview.
    fn sync_slide(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        bindings: &FrameBindings,
    ) -> Result<(), DisplayError> {
        let current_rect = bindings.current;
        let preview_rect = bindings.preview;

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
        self.release_current(driver)?;
        self.release_preview(driver)?;
        self.thumbnails
            .sync_thumbnails(state, pipeline, driver, &mut self.epoch, &bindings.thumbnails)
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
        if self.current_placed.is_none()
            && self.preview_placed.is_none()
            && self.thumbnails.thumbs.is_empty()
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
