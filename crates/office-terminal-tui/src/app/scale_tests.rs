//! Tests for the zoom → view policy (`app/scale.rs`): resolution follows
//! placement, and a zoom step is never destructive.
//!
//! Pins the three contract points of the fix for the slide-zoom defect
//! (GitHub issue #38):
//!
//! 1. Zoom rungs inside the canvas supersample headroom keep the base
//!    target — a zoom step is a pure re-placement (no re-render, no
//!    pipeline churn, no driver teardown).
//! 2. Zoom out never shrinks the raster (a smaller bitmap would upscale
//!    into the same cells and pixelate).
//! 3. Zoom in past the headroom bumps the target, capped, and prefetches it
//!    exactly once — with no cache invalidation at all (old rasters age
//!    out by budget, the swap discipline replaces the bitmap on screen).

use std::sync::{Arc, Mutex};

use office_terminal_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
};
use office_terminal_display::{DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver};

use super::PresentationApp;

/// Pipeline that counts the calls a zoom step must (not) make.
#[derive(Default)]
struct CountingPipeline {
    prefetches: Mutex<Vec<RenderTargetSpec>>,
    invalidations: Mutex<Vec<InvalidateCause>>,
}

impl RenderPipeline for CountingPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, target: &RenderTargetSpec) {
        self.prefetches.lock().expect("prefetch log").push(*target);
    }
    fn entry(&self, _index: usize) -> PipelineEntry {
        PipelineEntry::NotLoaded
    }
    fn invalidate(&self, cause: InvalidateCause) {
        self.invalidations.lock().expect("invalidate log").push(cause);
    }
}

struct NullDriver;

impl TerminalGraphicsDriver for NullDriver {
    fn name(&self) -> &'static str {
        "null"
    }
    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: false,
            resident_ids: 8,
        }
    }
    fn placement_mode(&self) -> PlacementMode {
        PlacementMode::Interactive
    }
    fn stage(&mut self, _bmp: &office_terminal_core::RgbaSlide, _id: ImageId) -> Result<(), DisplayError> {
        Ok(())
    }
    fn present(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
        Ok(())
    }
    fn reposition(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
        Ok(())
    }
    fn release(&mut self, _id: ImageId) -> Result<(), DisplayError> {
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), DisplayError> {
        Ok(())
    }
}

fn zoom_app(pipeline: &Arc<CountingPipeline>) -> PresentationApp {
    let dyn_pipeline: Arc<dyn RenderPipeline> = Arc::clone(pipeline) as Arc<dyn RenderPipeline>;
    PresentationApp::new(dyn_pipeline, Box::new(NullDriver), 3, vec![]).with_slide_dimensions(10_000_000, 7_500_000)
    // 4:3
}

/// `handle_resize` legitimately invalidates with `Resize` (caches stay); a
/// zoom step must add nothing beyond that baseline count.
fn resize_baseline_invalidations(pipeline: &CountingPipeline) -> usize {
    let invalidations = pipeline.invalidations.lock().expect("invalidate log");
    assert!(
        invalidations
            .iter()
            .all(|cause| matches!(cause, InvalidateCause::Resize)),
        "the resize baseline may only carry Resize invalidations"
    );
    invalidations.len()
}

fn zoom_to(app: &mut PresentationApp, scale: f32) {
    assert!(app.state_mut().set_scale(scale), "factor must move");
    app.apply_scale();
}

#[test]
fn zoom_rungs_inside_the_supersample_headroom_keep_the_base_target() {
    let pipeline = Arc::new(CountingPipeline::default());
    let mut app = zoom_app(&pipeline);
    app.handle_resize(120, 40);
    let base = app.current_target;
    let baseline_invalidations = resize_baseline_invalidations(&pipeline);

    // Every rung up to the 2x supersample factor is a pure re-placement.
    for scale in [1.1, 1.25, 1.5, 1.75, 2.0] {
        zoom_to(&mut app, scale);
        assert_eq!(
            app.current_target, base,
            "zoom {scale} needs no new raster (inside the headroom)"
        );
    }

    let prefetches = pipeline.prefetches.lock().expect("prefetch log");
    assert_eq!(
        prefetches.as_slice(),
        &[base],
        "only the resize prefetch exists; zoom steps added nothing"
    );
    drop(prefetches);
    assert_eq!(
        pipeline.invalidations.lock().expect("invalidate log").len(),
        baseline_invalidations,
        "a zoom step must never invalidate caches"
    );
}

#[test]
fn zoom_out_never_shrinks_the_raster() {
    let pipeline = Arc::new(CountingPipeline::default());
    let mut app = zoom_app(&pipeline);
    app.handle_resize(120, 40);
    let base = app.current_target;

    for scale in [0.9, 0.5, 0.25, 0.1] {
        zoom_to(&mut app, scale);
        assert_eq!(
            app.current_target, base,
            "zoom {scale} keeps the supersampled base (client-side downscale)"
        );
    }
}

#[test]
fn zoom_past_the_headroom_bumps_the_target_once_capped() {
    let pipeline = Arc::new(CountingPipeline::default());
    let mut app = zoom_app(&pipeline);
    app.handle_resize(120, 40);
    let base = app.current_target;
    let baseline_invalidations = resize_baseline_invalidations(&pipeline);

    // Top ladder rung: 4.0 / 2.0 supersample = 2x the base raster.
    zoom_to(&mut app, 4.0);
    let bumped = app.current_target;
    assert_eq!(bumped.width_px, (base.width_px * 2).min(8192));
    assert!(bumped.width_px > base.width_px, "the raster must grow");

    // Caps hold: bounded width and a bounded pixel budget.
    assert!(bumped.width_px <= 8192);
    assert!(
        u64::from(bumped.width_px) * u64::from(bumped.height_px) <= 64_000_000,
        "the pixmap allocation must stay bounded"
    );

    let prefetches = pipeline.prefetches.lock().expect("prefetch log");
    assert_eq!(
        prefetches.as_slice(),
        &[base, bumped],
        "the resize prefetch plus exactly one at the bumped target"
    );
    drop(prefetches);
    assert_eq!(
        pipeline.invalidations.lock().expect("invalidate log").len(),
        baseline_invalidations,
        "even a target bump must not wipe caches: the swap discipline replaces the bitmap"
    );
}

#[test]
fn repeat_zoom_steps_at_a_steady_factor_are_inert() {
    let pipeline = Arc::new(CountingPipeline::default());
    let mut app = zoom_app(&pipeline);
    app.handle_resize(120, 40);
    let baseline_invalidations = resize_baseline_invalidations(&pipeline);

    app.state_mut().set_scale(1.5);
    app.apply_scale();
    let after_step = pipeline.prefetches.lock().expect("prefetch log").len();

    // The user leans on `+` at the top rung (set_scale refuses, the action
    // still lands): the policy must stay completely inert.
    app.apply_scale();
    app.apply_scale();

    let prefetches = pipeline.prefetches.lock().expect("prefetch log");
    assert_eq!(prefetches.len(), after_step, "no target change, no pipeline work");
    drop(prefetches);
    assert_eq!(
        pipeline.invalidations.lock().expect("invalidate log").len(),
        baseline_invalidations,
        "repeat zoom steps stay completely inert"
    );
}
