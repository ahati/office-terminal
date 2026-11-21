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
    state.set_layout_dimensions(WIDTH, HEIGHT);
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

/// A reader state over `sizes` with one frame published, exactly like the
/// run loop: the stored column count matches the renderer's plan.
fn published_state(sizes: &[(f32, f32)]) -> PresentationState {
    let total = sizes.len();
    let mut state = PresentationState::new(total, vec![None; total]);
    state.enable_reader_mode(total, sizes.to_vec());
    state.set_layout_dimensions(WIDTH, HEIGHT);
    pane_geometry(&mut state);
    state
}

/// Letter pages with a short landscape row (two consecutive landscape
/// pages) between tall portrait rows: row strides differ, which is where
/// stride re-bases can flicker.
fn mixed_height_sizes() -> Vec<(f32, f32)> {
    let p = (612.0, 792.0); // portrait: 880 px tall at this size
    // A page wider than the viewport's aspect (1200/880 ~ 1.36) so the
    // one-column slot contains it to 600 px — a genuinely short row.
    let w = (1200.0, 600.0);
    let mut sizes = vec![p; 30];
    sizes[10] = w;
    sizes[11] = w;
    sizes
}

#[test]
fn every_lag_renders_the_pre_wrap_geometry() {
    // A held key sweeps the whole lag range, not just one pinned value: for
    // every lag the post-wrap frame (anchor advanced by the published step,
    // display shifted by the stride) must render exactly the pre-wrap
    // geometry.
    let sizes = vec![(612.0_f32, 792.0_f32); 30];
    let mut sync = published_state(&sizes);
    let stride = event_stride(&mut sync);
    let step = sync.reader_state().unwrap().effective_columns();
    let cell_h = reader_viewport_metrics(WIDTH, HEIGHT).3;
    let anchor = 6;

    let mut failures = Vec::new();
    let mut lag_px = 0.0f32;
    while lag_px < stride {
        let lag = lag_px.round() as i64;

        let mut pre = published_state(&sizes);
        {
            let reader = pre.reader_state_mut().unwrap();
            reader.anchor = anchor;
            set_scroll(reader, stride as i64, stride - lag as f32);
        }
        let mut post = published_state(&sizes);
        {
            let reader = post.reader_state_mut().unwrap();
            reader.anchor = anchor + step;
            set_scroll(reader, 0, -(lag as f32));
        }

        if pane_geometry(&mut pre) != pane_geometry(&mut post) {
            failures.push(lag);
        }
        lag_px += cell_h;
    }
    assert!(
        failures.is_empty(),
        "anchor flips broke continuity at lags {failures:?} of stride {stride}"
    );
}

#[test]
fn every_lag_renders_the_pre_wrap_geometry_on_retreat() {
    // The mirror sweep for scrolling up: the anchor retreats by the
    // published step and target/display shift by +stride; every lag must
    // stay continuous.
    let sizes = vec![(612.0_f32, 792.0_f32); 30];
    let mut sync = published_state(&sizes);
    let stride = event_stride(&mut sync);
    let step = sync.reader_state().unwrap().effective_columns();
    let cell_h = reader_viewport_metrics(WIDTH, HEIGHT).3;
    let anchor = 8;

    let mut failures = Vec::new();
    let mut lag_px = 0.0f32;
    while lag_px < stride {
        let lag = lag_px.round() as i64;

        // Pre-retreat: anchor 8, display one pixel above the target.
        let mut pre = published_state(&sizes);
        {
            let reader = pre.reader_state_mut().unwrap();
            reader.anchor = anchor;
            set_scroll(reader, lag, lag as f32 + 1.0);
        }
        // Post-retreat: the anchor walked back one step; both target and
        // display shifted by +stride.
        let mut post = published_state(&sizes);
        {
            let reader = post.reader_state_mut().unwrap();
            reader.anchor = anchor - step;
            set_scroll(reader, stride as i64 + lag, stride + lag as f32 + 1.0);
        }

        if pane_geometry(&mut pre) != pane_geometry(&mut post) {
            failures.push(lag);
        }
        lag_px += cell_h;
    }
    assert!(
        failures.is_empty(),
        "anchor retreats broke continuity at lags {failures:?} of stride {stride}"
    );
}

#[test]
fn flips_stay_continuous_across_rows_of_different_heights() {
    // The user-visible case: a short landscape row between tall portrait
    // rows. Row strides differ, so every stride re-base — the advance
    // wrap, the retreat wrap, and the render's lag-compensated walk-back —
    // must use the stride of the row it steps INTO, not the row it leaves.
    let sizes = mixed_height_sizes();
    let cell_h = reader_viewport_metrics(WIDTH, HEIGHT).3;

    // Flips into, inside, and out of the short landscape row [10, 11].
    let mut failures = Vec::new();
    for &(pre_anchor, post_anchor) in &[(8usize, 9usize), (9, 10), (10, 11), (11, 12)] {
        let mut pre = published_state(&sizes);
        {
            let reader = pre.reader_state_mut().unwrap();
            reader.anchor = pre_anchor;
        }
        let pre_stride = event_stride(&mut pre);

        let mut post = published_state(&sizes);
        {
            let reader = post.reader_state_mut().unwrap();
            reader.anchor = post_anchor;
        }

        let mut lag_px = 0.0f32;
        while lag_px < pre_stride {
            let lag = lag_px.round() as i64;
            {
                let reader = pre.reader_state_mut().unwrap();
                set_scroll(reader, pre_stride as i64, pre_stride - lag as f32);
            }
            {
                let reader = post.reader_state_mut().unwrap();
                set_scroll(reader, 0, -(lag as f32));
            }
            if pane_geometry(&mut pre) != pane_geometry(&mut post) {
                if failures.is_empty() {
                    let pre_geom = pane_geometry(&mut pre);
                    let post_geom = pane_geometry(&mut post);
                    eprintln!("MIXED first-failing ({pre_anchor}->{post_anchor}) lag {lag}:");
                    eprintln!("  pre:  {pre_geom:?}");
                    eprintln!("  post: {post_geom:?}");
                }
                failures.push((pre_anchor, post_anchor, lag));
            }
            lag_px += cell_h;
        }
    }
    assert!(
        failures.is_empty(),
        "flips across rows of different heights broke continuity: {failures:?}"
    );
}

#[test]
fn a_retreat_consumes_the_stride_of_the_row_it_steps_into() {
    // Scrolling up out of a tall row into a short one: the re-based scroll
    // target must land inside the SHORT row's range (its own stride minus
    // the overshoot), not the tall row's — the same per-row rule the
    // render walks follow (PRD 14a §6.4).
    let sizes = mixed_height_sizes();
    let mut state = published_state(&sizes);
    {
        let reader = state.reader_state_mut().unwrap();
        reader.anchor = 12; // the portrait row after the short landscape rows
    }
    let short_stride = {
        let reader = state.reader_state().unwrap();
        let (_, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(WIDTH, HEIGHT);
        let slot_w = reader.slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px);
        reader.row_stride_for_anchor(11, viewport_h_px, slot_w, cell_w_px, cell_h_px)
    };
    let tall_stride = {
        let reader = state.reader_state().unwrap();
        let (_, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(WIDTH, HEIGHT);
        let slot_w = reader.slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px);
        reader.row_stride_for_anchor(12, viewport_h_px, slot_w, cell_w_px, cell_h_px)
    };
    assert!(short_stride < tall_stride, "the fixture must mix row heights");

    // 10 px above row 12's top: retreating into the short row 11.
    let (_, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(WIDTH, HEIGHT);
    let geo = crate::app::reader_scroll::RowGeometry {
        viewport_h_px,
        slot_w_px: state
            .reader_state()
            .unwrap()
            .slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px),
        cell_w_px,
        cell_h_px,
    };
    state.reader_state_mut().unwrap().scroll_px = -10;
    let _ = state
        .reader_state_mut()
        .unwrap()
        .scroll_column_detailed(0, tall_stride, tall_stride - 20.0, geo);
    {
        let reader = state.reader_state().unwrap();
        assert_eq!(reader.anchor, 11, "the anchor retreats into the short row");
        assert_eq!(
            reader.scroll_px,
            short_stride as i64 - 10,
            "the retreat consumes the short row's own stride"
        );
    }
}
