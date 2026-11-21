//! Anchor-flip continuity for reader scrolling.
//!
//! The anchor wraps when the scroll TARGET crosses the row stride; the wrap
//! shifts the smoothed display by the same stride, so under continuous
//! scrolling the display is still NEGATIVE under the new anchor (it lags by
//! the remaining glide distance). The renderer must then draw the previous
//! row with the lag added back — `(anchor − step, displayed + stride)` is
//! geometrically identical to the pre-wrap frame — instead of clamping the
//! new row to the viewport top, which made the whole page jump up by the lag
//! at every boundary (the reported flicker).
//!
//! Each pre/post pair is a true wrap image: `render_reader` advances the
//! smoothed display toward its target before placing panes, so the compared
//! states carry the same target/display gap (and therefore the same advance)
//! and differ only by the wrap.

use kitty_pptx_display::{CellRect, PixelRect};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::reader_state::ReaderState;
use crate::app::reader_types::reader_viewport_metrics;
use crate::app::ui::reader::render_reader;
use crate::app::state::PresentationState;

/// Letter pages, spread layout (2 columns at this width).
const WIDTH: u16 = 120;
const HEIGHT: u16 = 45;

fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![None; pages]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0)]);
    state
}

/// Pane geometry (page, cell rect, source crop) of one drawn reader frame.
/// Publishes the frame's bindings onto the state exactly like the run loop,
/// so the column count the event path sees matches the drawn layout.
fn pane_geometry(state: &mut PresentationState) -> Vec<(usize, CellRect, PixelRect)> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, WIDTH, HEIGHT), &mut bindings))
        .unwrap();
    let snapshot = bindings.reader.clone();
    state.publish_frame_bindings(bindings);
    snapshot
        .into_iter()
        .map(|p| (p.page, p.cell_rect, p.pixel_src))
        .collect()
}

/// Row stride exactly as the event path computes it (after one published
/// frame, so the stored column count matches the renderer's layout).
fn event_stride(state: &mut PresentationState) -> f32 {
    let reader = state.reader_state_mut().unwrap();
    let (_, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(WIDTH, HEIGHT);
    let slot_w = reader.slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px);
    reader.row_stride_px(viewport_h_px, slot_w, cell_w_px, cell_h_px)
}

/// Sets an exact (target, smoothed display) pair: snap onto the target, then
/// shift by the lag.
fn set_scroll(reader: &mut ReaderState, target: i64, displayed: f32) {
    reader.scroll_px = target;
    reader.snap_display();
    reader.shift_displayed_scroll_px(displayed - target as f32);
}

#[test]
fn a_lagging_wrap_renders_the_pre_wrap_geometry() {
    let mut sync = reader_state(30);
    pane_geometry(&mut sync); // publish the column count like the run loop
    let stride = event_stride(&mut sync);
    let lag = (stride / 5.0).round(); // a mid-glide lag under fast scrolling
    assert!(lag > 1.0, "stride {stride} must be worth several cells");

    // Pre-wrap: anchor 2, target one stride ahead of the display.
    let mut pre = reader_state(30);
    {
        let reader = pre.reader_state_mut().unwrap();
        reader.anchor = 2;
        set_scroll(reader, stride as i64, stride - lag);
    }

    // Post-wrap: anchor 4 (the target crossed the boundary and wrapped),
    // display lagging at `-lag`. This frame must look exactly like pre-wrap.
    let mut post = reader_state(30);
    {
        let reader = post.reader_state_mut().unwrap();
        reader.anchor = 4;
        set_scroll(reader, 0, -lag);
    }

    let pre_geom = pane_geometry(&mut pre);
    let post_geom = pane_geometry(&mut post);
    assert!(!pre_geom.is_empty(), "the pre-wrap frame places panes");
    assert!(
        pre_geom.iter().all(|&(page, _, _)| (2..4).contains(&page)),
        "the pre-wrap frame shows the old row: {pre_geom:?}"
    );
    assert_eq!(
        pre_geom, post_geom,
        "a lagging anchor flip must not move any pane"
    );
}

#[test]
fn a_converged_wrap_renders_the_new_row() {
    // Once the display catches up (non-negative), the new anchor renders
    // normally — the compensation must not stick.
    let mut state = reader_state(30);
    {
        let reader = state.reader_state_mut().unwrap();
        reader.anchor = 4;
        set_scroll(reader, 0, 0.0);
    }
    let geom = pane_geometry(&mut state);
    assert!(
        geom.iter().all(|&(page, _, _)| (4..6).contains(&page)),
        "a converged anchor renders its own row: {geom:?}"
    );
}

#[test]
fn a_multi_row_lag_backs_up_repeatedly() {
    // A very fast fling can wrap the target a second row ahead of the
    // display; the compensation must back up twice and stay continuous.
    let mut sync = reader_state(40);
    pane_geometry(&mut sync); // publish the column count like the run loop
    let stride = event_stride(&mut sync);
    let lag = stride + (stride * 0.8).round(); // deep enough to back up twice

    // The true pre-image of a double wrap is one wrap back, with the same
    // target/display gap: (anchor 4, target = stride, displayed = -m).
    let mut pre = reader_state(40);
    {
        let reader = pre.reader_state_mut().unwrap();
        reader.anchor = 4;
        set_scroll(reader, stride as i64, -(lag - stride));
    }
    let mut post = reader_state(40);
    {
        let reader = post.reader_state_mut().unwrap();
        reader.anchor = 6;
        set_scroll(reader, 0, -lag);
    }

    let pre_geom = pane_geometry(&mut pre);
    let post_geom = pane_geometry(&mut post);
    assert_eq!(pre_geom, post_geom, "a double wrap stays continuous too");
}

#[test]
fn page_targets_snap_to_device_pixel_classes() {
    let mut state = reader_state(10);
    let reader = state.reader_state_mut().unwrap();

    // A small pane (600 device px) renders at the 1280 class — ~4x fewer
    // pixels than the old fixed 2560 — instead of paying full price.
    let small = reader.page_render_target(0, 600.0);
    assert_eq!(small.width_px, 1280);
    assert_eq!(small.height_px, 1656, "aspect-true height for letter pages");

    // Zoomed / large panes step up the ladder; beyond the top they cap.
    assert_eq!(reader.page_render_target(0, 1100.0).width_px, 1920);
    assert_eq!(reader.page_render_target(0, 1600.0).width_px, 2560);
    assert_eq!(reader.page_render_target(0, 4000.0).width_px, 3840);

    // Degenerate panes still produce a sane target.
    let fallback = reader.page_render_target(0, 0.0);
    assert_eq!(fallback.width_px, 1280);
}
