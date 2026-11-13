//! The decode-plan ports of `renderer-adaptive-image-budget.test.ts` and the
//! device-pixel derivation of `renderer-picture-raster-target.test.ts`.
//!
//! Loaded as a module of the `parity_elements_tests` target so each file stays
//! under the 600-line limit.

use kitty_pptx_core::contracts::{
    AffineTransform, NormalizedRect, RectF32, VectorCommand,
};
use kitty_pptx_core::loader::SizeHint;



// ───────────────────────────────────────────────────────────────────────────
// renderer-adaptive-image-budget.test.ts — the decode plan.
//
// The reference inspects `decode` call options (`resizeWidth`/`resizeHeight`)
// and the typed quota rejection. The Rust twin inspects
// `rasterizer::decode_plan`, the policy seam the reference's
// `imageResources` option bag configures
// (ref core/src/image/adaptive-image-budget.ts:130-330).
// ───────────────────────────────────────────────────────────────────────────

use kitty_pptx_core::rasterizer::decode_plan::{
    plan_decoded_image_targets, scene_decode_demands, BudgetStrategy, DecodeDemand,
    DecodeResolution, ImageResourceOptions, MAX_DECODED_IMAGE_BYTES,
};


use crate::{picture_slide, target};

/// "requests both planned raster axes so the retained surface matches its pixel
/// limit" (`renderer-adaptive-image-budget.test.ts:109-140`).
///
/// A 304800×209550 EMU frame at 960 CSS px is 32×22 device px; the 2816-byte
/// budget is exactly `32 · 22 · 4`, so the display plan keeps that grid.
#[test]
fn display_plan_requests_both_planned_raster_axes() {
    let slide = picture_slide(
        "ppt/media/photo.png",
        (0.0, 0.0, 304_800.0, 209_550.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    let demands = scene_decode_demands(&slide, &target(960, 720));
    assert_eq!(demands.len(), 1);
    assert!((demands[0].target_width_px - 32.0).abs() < 1e-3, "{:?}", demands[0]);
    assert!((demands[0].target_height_px - 22.0).abs() < 1e-3);

    let plan = plan_decoded_image_targets(
        &demands,
        &ImageResourceOptions {
            decoded_byte_budget: 2_816,
            strategy: BudgetStrategy::Adaptive,
            resolution: DecodeResolution::Display,
        },
    )
    .expect("plan");
    let entry = plan.targets.get("ppt/media/photo.png").expect("target");
    assert_eq!((entry.width, entry.height), (32, 22));
    assert_eq!(plan.planned_bytes, 2_816);
    assert!(!plan.degraded);
    // The plan's grid is the decoder request the reference asserts on.
    assert_eq!(
        plan.size_hint("ppt/media/photo.png"),
        SizeHint::Native,
        "an undegraded plan keeps the source grid"
    );
}

/// "allocates one bounded quality level across every raster on the slide"
/// (`renderer-adaptive-image-budget.test.ts:142-171`).
///
/// Two 4000×4000 sources displayed at 960×1440 device px each, against a
/// 1 MiB budget: one uniform scale, identical targets, total ≤ budget.
#[test]
fn one_bounded_quality_level_covers_every_raster() {
    let mut slide = picture_slide(
        "ppt/media/left.png",
        (0.0, 0.0, 4_572_000.0, 6_858_000.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    slide.commands.push(VectorCommand::draw_image(
        "ppt/media/right.png".to_string(),
        NormalizedRect::default(),
        RectF32::new(4_572_000.0, 0.0, 4_572_000.0, 6_858_000.0),
        1.0,
        AffineTransform::identity(),
    ));
    let budget = 1_048_576u64;
    let plan = plan_decoded_image_targets(
        &scene_decode_demands(&slide, &target(1_920, 1_440)),
        &ImageResourceOptions {
            decoded_byte_budget: budget,
            strategy: BudgetStrategy::Adaptive,
            resolution: DecodeResolution::NativeIfFit,
        },
    )
    .expect("plan");

    let left = plan.targets.get("ppt/media/left.png").expect("left");
    let right = plan.targets.get("ppt/media/right.png").expect("right");
    assert_eq!((left.width, left.height), (right.width, right.height));
    assert!(plan.degraded, "the budget forces a reduction");
    let total = u64::from(left.width) * u64::from(left.height) * 4 * 2;
    assert!(total <= budget, "{total} > {budget}");
}

/// "renders the reported 109,571,670-pixel picture at a bounded supersampled
/// target" (`renderer-adaptive-image-budget.test.ts:206-232`).
///
/// A 12 090×9 063 source over a half-slide frame at 960 CSS px / DPR 2 is
/// 960×1440 device px; the native grid (109 MP) crosses the 32 MP per-surface
/// ceiling, so the plan requests the 2× supersampled display grid instead.
#[test]
fn huge_picture_gets_a_bounded_supersampled_target() {
    let slide = picture_slide(
        "ppt/media/large-photo.png",
        (0.0, 0.0, 4_572_000.0, 6_858_000.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    let plan = plan_decoded_image_targets(
        &scene_decode_demands(&slide, &target(1_920, 1_440)),
        &ImageResourceOptions::default(),
    )
    .expect("plan");
    let entry = plan.targets.get("ppt/media/large-photo.png").expect("target");
    assert_eq!((entry.width, entry.height), (1_920, 2_880));
    assert!(
        entry.retained_pixels <= kitty_pptx_core::rasterizer::decode_plan::MAX_RASTER_PIXELS,
        "the retained surface is bounded by MAX_RASTER_PIXELS"
    );
    assert_eq!(MAX_DECODED_IMAGE_BYTES, 128 * 1024 * 1024);
}

/// "keeps a strict mode that surfaces the typed aggregate quota crossing"
/// (`renderer-adaptive-image-budget.test.ts:284-306`).
#[test]
fn strict_mode_surfaces_the_typed_quota_crossing() {
    let mut slide = picture_slide(
        "ppt/media/left.png",
        (0.0, 0.0, 4_572_000.0, 6_858_000.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    slide.commands.push(VectorCommand::draw_image(
        "ppt/media/right.png".to_string(),
        NormalizedRect::default(),
        RectF32::new(4_572_000.0, 0.0, 4_572_000.0, 6_858_000.0),
        1.0,
        AffineTransform::identity(),
    ));
    let error = plan_decoded_image_targets(
        &scene_decode_demands(&slide, &target(1_920, 1_440)),
        &ImageResourceOptions {
            decoded_byte_budget: 1_048_576,
            strategy: BudgetStrategy::Strict,
            resolution: DecodeResolution::NativeIfFit,
        },
    )
    .expect_err("strict mode must not degrade");
    match error {
        kitty_pptx_core::error::RenderError::DecodedImageLimit {
            metric,
            limit,
            observed,
        } => {
            assert_eq!(metric, "active-decoded-bytes");
            assert_eq!(limit, 1_048_576);
            assert!(observed > limit, "{observed}");
        }
        other => panic!("expected the typed quota crossing, got {other:?}"),
    }
}

/// "does not apply the display-sized pixel cap to a TIFF codec result"
/// (`renderer-adaptive-image-budget.test.ts:176-198`).
///
/// The optional TIFF codec cannot be decoder-resized, so a TIFF blip keeps its
/// authored grid: it contributes no decode demand (and therefore no cap) next
/// to an identical PNG frame that does.
#[test]
fn tiff_sources_stay_outside_the_raster_plan() {
    let mut slide = picture_slide(
        "ppt/media/photo.tiff",
        (0.0, 0.0, 4_572_000.0, 6_858_000.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    let demands = scene_decode_demands(&slide, &target(960, 720));
    assert!(demands.is_empty(), "a TIFF source is never capped");

    slide.commands.clear();
    slide.commands.push(VectorCommand::draw_image(
        "ppt/media/photo.png".to_string(),
        NormalizedRect::default(),
        RectF32::new(0.0, 0.0, 4_572_000.0, 6_858_000.0),
        1.0,
        AffineTransform::identity(),
    ));
    let demands: Vec<DecodeDemand> = scene_decode_demands(&slide, &target(960, 720));
    assert_eq!(demands.len(), 1, "the PNG twin is planned");
}

// ───────────────────────────────────────────────────────────────────────────
// renderer-picture-raster-target.test.ts — "derives full-source device pixels
// from the frame, effective DPR, and srcRect crop"
// (`renderer-picture-raster-target.test.ts:237-274`).
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn full_source_device_pixels_follow_frame_dpr_and_crop() {
    // 4572000×3429000 EMU at 960 CSS px / DPR 2 = 960×720 device px; the 50%
    // visible source width doubles the full-source demand to 1920×720, and the
    // native-admission share retains it at 2× that grid while the budget has
    // room (the reference's 3840×1440).
    let slide = picture_slide(
        "ppt/media/108mp-poster.png",
        (0.0, 0.0, 4_572_000.0, 3_429_000.0),
        (0.25, 0.0, 0.5, 1.0),
    );
    let demands = scene_decode_demands(&slide, &target(1_920, 1_440));
    assert_eq!(demands.len(), 1);
    assert!((demands[0].target_width_px - 1_920.0).abs() < 1e-2);
    assert!((demands[0].target_height_px - 720.0).abs() < 1e-2);

    let plan = plan_decoded_image_targets(&demands, &ImageResourceOptions::default())
        .expect("plan");
    let entry = plan.targets.get("ppt/media/108mp-poster.png").expect("target");
    assert_eq!((entry.width, entry.height), (3_840, 1_440));
}
