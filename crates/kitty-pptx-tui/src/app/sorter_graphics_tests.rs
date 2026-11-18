//! Unit tests for slide-sorter thumbnail graphics.
//!
//! The sorter renders one bitmap per visible card instead of the full-screen
//! slide pair, so these tests cover staging/placement/release across grid
//! scrolls, resizes, overlays and blanking.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use kitty_pptx_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use kitty_pptx_display::{
    CellRect, DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver,
    MAX_THUMBNAIL_IMAGES, THUMBNAIL_IMAGE_ID_BASE,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::graphics::GraphicsSyncer;
use crate::app::graphics_thumbnails::max_sorter_cards_for_target_and_quota;
use crate::app::mode::{AppMode, BlankMode};
use crate::app::state::PresentationState;
use crate::app::ui::render_app;

/// Target the full-screen canvas renders at. Sorter cards deliberately do NOT
/// resolve against this target: they render at the dedicated aspect-derived
/// sorter target (1280 px wide) instead.
pub(crate) const CANVAS: RenderTargetSpec = RenderTargetSpec {
    width_px: 640,
    height_px: 360,
};

pub(crate) const WIDE: (u16, u16) = (100, 30);
pub(crate) const SHORT: (u16, u16) = (100, 20);

fn card_bitmap() -> RgbaSlide {
    RgbaSlide {
        width: 4,
        height: 2,
        pixels: vec![96u8; 4 * 2 * 4].into(),
    }
}

/// Pipeline that serves card-sized rasters and records the requested targets.
pub(crate) struct GridPipeline {
    slides: usize,
    /// When false, `entry_at` reports `NotLoaded`: the raster is neither
    /// cached nor scheduled (simulates budget eviction or cancellation).
    ready: AtomicBool,
    targets: Mutex<Vec<RenderTargetSpec>>,
    prefetched: Mutex<Vec<(usize, RenderTargetSpec)>>,
}

impl GridPipeline {
    pub(crate) fn new(slides: usize) -> Self {
        Self {
            slides,
            ready: AtomicBool::new(true),
            targets: Mutex::new(Vec::new()),
            prefetched: Mutex::new(Vec::new()),
        }
    }

    /// Simulates the rasters vanishing from the pipeline cache.
    pub(crate) fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }

    pub(crate) fn prefetch_count(&self) -> usize {
        self.prefetched.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Recorded `entry_at` targets, in call order.
    pub(crate) fn requested_targets(&self) -> Vec<RenderTargetSpec> {
        self.targets.lock().map(|t| t.clone()).unwrap_or_default()
    }

    /// Recorded `(slide, target)` prefetch pairs, in call order.
    pub(crate) fn prefetch_log(&self) -> Vec<(usize, RenderTargetSpec)> {
        self.prefetched
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default()
    }
}

impl RenderPipeline for GridPipeline {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec) {
        if let Ok(mut seen) = self.prefetched.lock() {
            for offset in 0..=plan.ahead {
                seen.push((plan.center + offset, *target));
            }
        }
    }

    fn entry(&self, index: usize) -> PipelineEntry {
        if self.ready.load(Ordering::SeqCst) && index < self.slides {
            PipelineEntry::Ready {
                bitmap: card_bitmap(),
                staged: false,
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }

    fn entry_at(&self, index: usize, target: &RenderTargetSpec) -> PipelineEntry {
        if let Ok(mut targets) = self.targets.lock() {
            targets.push(*target);
        }
        if self.ready.load(Ordering::SeqCst) && index < self.slides {
            PipelineEntry::Ready {
                bitmap: card_bitmap(),
                staged: false,
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }

    fn invalidate(&self, _cause: InvalidateCause) {}
}

#[derive(Default, Debug)]
pub(crate) struct DriverLog {
    pub(crate) staged: Vec<u32>,
    pub(crate) presented: Vec<(u32, CellRect)>,
    pub(crate) repositioned: Vec<(u32, CellRect)>,
    pub(crate) released: Vec<u32>,
    pub(crate) released_all: usize,
}

pub(crate) struct RecordingDriver {
    log: Arc<Mutex<DriverLog>>,
}

impl RecordingDriver {
    pub(crate) fn new() -> (Self, Arc<Mutex<DriverLog>>) {
        let log = Arc::new(Mutex::new(DriverLog::default()));
        (
            Self {
                log: Arc::clone(&log),
            },
            log,
        )
    }
}

impl TerminalGraphicsDriver for RecordingDriver {
    fn name(&self) -> &'static str {
        "recording"
    }
    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: true,
            resident_ids: MAX_THUMBNAIL_IMAGES + 16,
        }
    }
    fn placement_mode(&self) -> PlacementMode {
        PlacementMode::Interactive
    }
    fn stage(&mut self, _bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        self.log.lock().unwrap().staged.push(id.0);
        Ok(())
    }
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        if let Placement::Cells(rect) = p {
            self.log.lock().unwrap().presented.push((id.0, rect));
        }
        Ok(())
    }
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        if let Placement::Cells(rect) = p {
            self.log.lock().unwrap().repositioned.push((id.0, rect));
        }
        Ok(())
    }
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        self.log.lock().unwrap().released.push(id.0);
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), DisplayError> {
        self.log.lock().unwrap().released_all += 1;
        Ok(())
    }
}

/// Draws one sorter frame at the given terminal size, clearing stale rects.
pub(crate) fn draw_sorter(
    state: &PresentationState,
    (width, height): (u16, u16),
) -> Vec<(usize, CellRect)> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    state.clear_placement_rects();
    terminal.draw(|f| render_app(state, f)).unwrap();
    state.thumbnail_placement_rects()
}

pub(crate) fn sorter_state(slides: usize) -> PresentationState {
    let mut state = PresentationState::new(slides, vec![]);
    state.set_mode(AppMode::Sorter);
    state
}

/// Synchronizes repeatedly until the per-frame staging throttle has drained.
pub(crate) fn sync_until_settled(
    syncer: &mut GraphicsSyncer,
    state: &mut PresentationState,
    pipeline: &GridPipeline,
    driver: &mut RecordingDriver,
    log: &Arc<Mutex<DriverLog>>,
    expected: usize,
) {
    for _ in 0..16 {
        syncer.sync(state, pipeline, driver, &CANVAS).unwrap();
        if log.lock().unwrap().staged.len() >= expected {
            return;
        }
    }
}

#[test]
fn sorter_stages_one_image_per_visible_card() {
    let pipeline = GridPipeline::new(12);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(12);

    let cards = draw_sorter(&state, WIDE);
    assert_eq!(cards.len(), 12, "the whole deck fits one grid page");

    sync_until_settled(&mut syncer, &mut state, &pipeline, &mut driver, &log, 12);

    let log = log.lock().unwrap();
    assert_eq!(log.staged.len(), 12, "one bitmap per card");
    assert_eq!(log.presented.len(), 12);

    let mut ids: Vec<u32> = log.staged.clone();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 12, "every card owns a distinct image id");
    assert!(
        log.staged.iter().all(|id| *id >= THUMBNAIL_IMAGE_ID_BASE),
        "thumbnail ids come from the dedicated range: {:?}",
        log.staged
    );
    assert!(
        log.staged
            .iter()
            .all(|id| *id < THUMBNAIL_IMAGE_ID_BASE + MAX_THUMBNAIL_IMAGES),
        "thumbnail ids stay inside the resident pool"
    );

    for (_, rect) in &log.presented {
        assert!(
            cards.iter().any(|(_, card)| card == rect),
            "every placement uses a rect the layout recorded: {rect:?}"
        );
    }
}

#[test]
fn sorter_does_not_retransmit_an_unchanged_grid() {
    let pipeline = GridPipeline::new(6);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(6);

    let cards = draw_sorter(&state, WIDE);
    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        cards.len(),
    );

    let before = {
        let log = log.lock().unwrap();
        (
            log.staged.len(),
            log.presented.len(),
            log.repositioned.len(),
            log.released.len(),
        )
    };

    for _ in 0..3 {
        syncer
            .sync(&mut state, &pipeline, &mut driver, &CANVAS)
            .unwrap();
    }

    let after = log.lock().unwrap();
    assert_eq!(after.staged.len(), before.0);
    assert_eq!(after.presented.len(), before.1);
    assert_eq!(after.repositioned.len(), before.2);
    assert_eq!(after.released.len(), before.3);
}

#[test]
fn sorter_releases_cards_that_scroll_out_of_view() {
    let pipeline = GridPipeline::new(30);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(30);

    let first_page = draw_sorter(&state, SHORT);
    assert_eq!(first_page.len(), 12);
    sync_until_settled(&mut syncer, &mut state, &pipeline, &mut driver, &log, 12);

    let first_ids = log.lock().unwrap().staged.clone();
    assert_eq!(first_ids.len(), 12);

    // Jump far enough down that no card of the first page stays visible.
    state.set_selected_sorter_index(24);
    let second_page = draw_sorter(&state, SHORT);
    assert_eq!(second_page.len(), 12);
    assert!(
        second_page
            .iter()
            .all(|(slide, _)| !first_page.iter().any(|(first, _)| first == slide)),
        "scrolling replaced the whole page"
    );

    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();

    let log = log.lock().unwrap();
    let released: Vec<u32> = log.released.clone();
    for id in &first_ids {
        assert!(
            released.contains(id),
            "off-screen card id {id} must be released"
        );
    }
    assert!(
        log.staged.len() > first_ids.len(),
        "the new page streams in after the release"
    );
}

#[test]
fn leaving_the_sorter_releases_thumbnails_and_restores_the_slide() {
    let pipeline = GridPipeline::new(4);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(4);

    let cards = draw_sorter(&state, WIDE);
    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        cards.len(),
    );
    let thumb_ids = log.lock().unwrap().staged.clone();
    assert_eq!(thumb_ids.len(), 4);

    state.set_mode(AppMode::Fullscreen);
    state.clear_placement_rects();
    let mut terminal = Terminal::new(TestBackend::new(WIDE.0, WIDE.1)).unwrap();
    terminal.draw(|f| render_app(&state, f)).unwrap();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();

    let log = log.lock().unwrap();
    for id in &thumb_ids {
        assert!(
            log.released.contains(id),
            "thumbnail id {id} must be released when the grid closes"
        );
    }
    assert!(
        log.staged.last() == Some(&state.current_image_id().0),
        "the full-screen slide is staged again: {:?}",
        log.staged
    );
    assert!(
        log.presented
            .iter()
            .any(|(id, _)| *id == state.current_image_id().0),
        "the restored slide is placed"
    );
}

#[test]
fn sorter_keeps_thumbnails_under_an_overlay_modal() {
    let pipeline = GridPipeline::new(6);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(6);

    let cards = draw_sorter(&state, WIDE);
    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        cards.len(),
    );
    let (staged, released) = {
        let log = log.lock().unwrap();
        (log.staged.len(), log.released.len())
    };

    state.enter_overlay(AppMode::HelpModal);
    assert_eq!(state.base_mode(), AppMode::Sorter);
    draw_sorter(&state, WIDE);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.released.len(), released, "an overlay is not a teardown");
    assert_eq!(log.staged.len(), staged, "cards stay staged");
}

#[test]
fn sorter_blank_screen_suppresses_and_resumes_thumbnails() {
    let pipeline = GridPipeline::new(6);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(6);

    let cards = draw_sorter(&state, WIDE);
    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        cards.len(),
    );

    state.toggle_blank(BlankMode::Black);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();
    assert_eq!(
        log.lock().unwrap().released_all,
        1,
        "blanking clears the grid once"
    );

    state.unblank();
    draw_sorter(&state, WIDE);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();
    assert!(
        log.lock().unwrap().staged.len() > cards.len(),
        "cards are re-staged after the blank screen"
    );
}

#[test]
fn resizing_cards_repositions_instead_of_retransmitting() {
    let pipeline = GridPipeline::new(12);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(12);

    draw_sorter(&state, (100, 30));
    sync_until_settled(&mut syncer, &mut state, &pipeline, &mut driver, &log, 12);
    let staged_before = log.lock().unwrap().staged.len();

    // Taller terminal: same column count and aspect, so the card target (and
    // therefore the bitmap) is unchanged while the rows move.
    draw_sorter(&state, (100, 40));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();

    let log = log.lock().unwrap();
    assert!(
        !log.repositioned.is_empty(),
        "existing cards move rather than re-upload"
    );
    assert_eq!(
        log.staged.len(),
        staged_before,
        "no card is re-transmitted for a pure move"
    );
}

#[test]
fn empty_deck_places_nothing() {
    let pipeline = GridPipeline::new(0);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(0);

    draw_sorter(&state, WIDE);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS)
        .unwrap();

    let log = log.lock().unwrap();
    assert!(log.staged.is_empty());
    assert!(log.presented.is_empty());
    assert_eq!(log.released_all, 0);
}

#[test]
fn sorter_caps_visible_cards_to_terminal_quota() {
    let card_bytes = 480 * 360 * 4;
    let target = RenderTargetSpec {
        width_px: 480,
        height_px: 360,
    };
    let max_cards = max_sorter_cards_for_target_and_quota(&target, Some(card_bytes * 3));
    assert_eq!(max_cards, 3, "Caps to exactly 3 cards under quota");
}
