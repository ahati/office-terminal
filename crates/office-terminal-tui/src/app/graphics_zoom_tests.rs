//! Zoom placement contract for the full-screen slide slots (issue #38).
//!
//! Split from `graphics_tests.rs` to respect the 600-line ceiling; shares
//! that module's driver/pipeline mocks (`pub(super)` surface).

use office_terminal_display::Placement;
use ratatui::layout::Rect;

use super::graphics_tests::{canvas, MockDriver, MockPipeline};
use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::state::PresentationState;

#[test]
fn test_zoomed_slide_replaces_the_placement_without_retransmitting() {
    // The zoom fix's core contract (issue #38): zoom is a placement change.
    // One staged bitmap serves the plain fit, the zoomed crop, and pans;
    // only the placement sequence changes - never the pixels.
    let pipeline = MockPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(3, vec![]);
    let mut syncer = GraphicsSyncer::new();

    // 1. Plain fit: one stage, whole-canvas placement.
    let mut bindings = FrameBindings::default();
    bindings.record_current(Rect::new(0, 0, 80, 24), 16.0 / 9.0, 1.0, (0.0, 0.0));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged, vec![1]);
        assert!(matches!(log.presented_forms[0], (1, Placement::Cells(_))));
    }

    // 2. Zoom to 2x: the same bitmap is placed as a crop window.
    state.set_scale(2.0);
    let mut bindings = FrameBindings::default();
    bindings.record_current(Rect::new(0, 0, 80, 24), 16.0 / 9.0, 2.0, (0.0, 0.0));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged.len(), 1, "zoom must not re-transmit pixels");
        assert_eq!(log.presented_forms.len(), 2, "the placement is replaced");
        assert!(matches!(
            log.presented_forms[1],
            (1, Placement::CellsSubrect { .. })
        ));
    }

    // 3. Pan: same id, re-placed crop window, still no re-transmit.
    let mut bindings = FrameBindings::default();
    bindings.record_current(Rect::new(0, 0, 80, 24), 16.0 / 9.0, 2.0, (40.0, 0.0));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged.len(), 1, "pan must not re-transmit pixels");
        assert_eq!(log.presented_forms.len(), 3);
        assert!(matches!(
            log.presented_forms[2],
            (1, Placement::CellsSubrect { .. })
        ));
    }

    // 4. Back to the plain fit: whole-canvas placement again, still no stage.
    state.set_scale(1.0);
    let mut bindings = FrameBindings::default();
    bindings.record_current(Rect::new(0, 0, 80, 24), 16.0 / 9.0, 1.0, (0.0, 0.0));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged.len(), 1, "reset must not re-transmit pixels");
        assert_eq!(log.presented_forms.len(), 4);
        assert!(matches!(log.presented_forms[3], (1, Placement::Cells(_))));
        assert!(log.released.is_empty(), "no swap, no release: one bitmap throughout");
    }
}
