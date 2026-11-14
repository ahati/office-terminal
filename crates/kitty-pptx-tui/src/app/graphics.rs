use std::sync::{Arc, Mutex};

use kitty_pptx_core::{DisplayError, PipelineEntry, RenderPipeline};
use kitty_pptx_display::{CellRect, ImageId, Placement, TerminalGraphicsDriver};

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

/// Keeps terminal graphics in sync with the rects recorded by the last frame.
///
/// The syncer stages the current (and presenter-preview) bitmap on demand and
/// places/repositions the image only when the slide or its screen rect changed,
/// so the render loop never spams the terminal with duplicate image commands.
#[derive(Default)]
pub(crate) struct GraphicsSyncer {
    current_staged: Option<StagedImage>,
    preview_staged: Option<StagedImage>,
    current_placed: Option<PlacedImage>,
    preview_placed: Option<PlacedImage>,
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
                    s.id == id && s.slide == slide && s.width == bitmap.width && s.height == bitmap.height
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
        if self.current_placed.is_none() && self.preview_placed.is_none() {
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
