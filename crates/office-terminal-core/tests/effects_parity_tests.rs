//! Raster-pixel parity tests for the 2-D effect pipeline.
//!
//! Every expectation is derived from the reference compositor
//! (ref packages/pptx/src/renderer.ts:3765-3860 `paintWithRasterEffects`,
//! ref packages/core/src/shape/effects.ts) and asserted as a concrete alpha
//! over the opaque white slide background:
//!
//! * outer shadow / inner shadow / reflection use `filter='blur(r)'`
//!   ⇒ sigma = r device px,
//! * glow uses `shadowBlur = rad` ⇒ sigma = rad / 2 and no intensity curve,
//! * softEdge feathers with sigma = rad / 3 and bleeds `rad` outward,
//! * pass order is shadow → reflection → glow → body → inner shadow.

use office_terminal_core::*;

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

const W: u32 = 200;
const H: u32 = 100;
/// Body fill; also the colour the glow reuses so alphas compare directly.
const FILL: [u8; 4] = [0x44, 0x72, 0xC4, 255];

/// Slide geometry: 2 000 000 × 1 000 000 EMU mapped 1:1 onto 200 × 100 px, so
/// 1 px == 10 000 EMU. The body is the rect x 10..90, y 10..50 px.
fn slide_with(effects: ShapeEffects, transform: AffineTransform, fill: [u8; 4]) -> VectorSlide {
    let mut slide = VectorSlide::new(2_000_000, 1_000_000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));
    slide.commands.push(VectorCommand::DrawPath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 100_000.0, y: 100_000.0 },
                PathSegment::LineTo { x: 900_000.0, y: 100_000.0 },
                PathSegment::LineTo { x: 900_000.0, y: 500_000.0 },
                PathSegment::LineTo { x: 100_000.0, y: 500_000.0 },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: Some(VectorPaint::Solid(fill)),
        stroke: None,
        transform,
        effects: Box::new(effects),
    });
    slide
}

fn raster(effects: ShapeEffects, transform: AffineTransform) -> RgbaSlide {
    raster_filled(effects, transform, FILL)
}

fn raster_filled(effects: ShapeEffects, transform: AffineTransform, fill: [u8; 4]) -> RgbaSlide {
    raster_at(effects, transform, fill, W, H)
}

fn raster_at(
    effects: ShapeEffects,
    transform: AffineTransform,
    fill: [u8; 4],
    width_px: u32,
    height_px: u32,
) -> RgbaSlide {
    let slide = slide_with(effects, transform, fill);
    TinySkiaRasterizer::new()
        .rasterize(&slide, &RenderTargetSpec { width_px, height_px }, &NoAssets)
        .expect("rasterize")
}

fn px(img: &RgbaSlide, x: usize, y: usize) -> [u8; 4] {
    px_in(img, W, x, y)
}

fn px_in(img: &RgbaSlide, width: u32, x: usize, y: usize) -> [u8; 4] {
    let base = (y * width as usize + x) * 4;
    [img.pixels[base], img.pixels[base + 1], img.pixels[base + 2], img.pixels[base + 3]]
}

/// Alpha of a black effect composited over the white background.
fn black_alpha(img: &RgbaSlide, x: usize, y: usize) -> f32 {
    black_alpha_in(img, W, x, y)
}

fn black_alpha_in(img: &RgbaSlide, width: u32, x: usize, y: usize) -> f32 {
    1.0 - px_in(img, width, x, y)[0] as f32 / 255.0
}

fn shadow(blur_emu: i64, dist_emu: i64, rotate_with_shape: bool) -> VectorOuterShadow {
    VectorOuterShadow {
        color: [0, 0, 0, 255],
        blur_radius_emu: blur_emu,
        distance_emu: dist_emu,
        direction_deg: 90.0,
        scale_x: 1.0,
        scale_y: 1.0,
        skew_x_deg: 0.0,
        skew_y_deg: 0.0,
        rotate_with_shape,
    }
}

#[test]
fn blur_sigma_is_the_authored_radius() {
    // ref effects.ts:applyOuterShadow — `ctx.filter='blur(r)'` takes r as the
    // Gaussian standard deviation. A 10 px blur must therefore still ink well
    // past 10 px, and nothing past the kernel extent (~3σ).
    let effects = ShapeEffects {
        outer_shadow: Some(shadow(100_000, 0, false)),
        ..Default::default()
    };
    let img = raster(effects, AffineTransform::default());

    // 10.5 px right of the body edge: 1 - Φ(1.1) ≈ 0.13.
    let at_sigma = black_alpha(&img, 100, 30);
    assert!(
        (0.08..0.25).contains(&at_sigma),
        "sigma must equal the authored radius, alpha at 1σ was {at_sigma}"
    );
    let nearer = black_alpha(&img, 95, 30);
    assert!(
        nearer > at_sigma + 0.08,
        "shadow tail must decay outward: {nearer} vs {at_sigma}"
    );
    // 34 px out: past the finite kernel support, so no ink at all.
    assert_eq!(px(&img, 124, 30), [255, 255, 255, 255], "no ink past ~3σ");
    assert_eq!(px(&img, 50, 30), FILL, "the body stays on top");
}

#[test]
fn blur_reach_is_not_capped_below_the_canvas() {
    // No MAX_BLUR_PX clamp: a 150 px authored blur keeps its real reach.
    // 400 x 300 px target: 1 px == 5000 EMU, body x 20..180, y 20..100.
    let effects = ShapeEffects {
        outer_shadow: Some(shadow(750_000, 0, false)),
        ..Default::default()
    };
    let img = raster_at(effects, AffineTransform::default(), FILL, 400, 300);

    // 140 px right of the body edge. sigma = 150 px leaves 0.024 there; a
    // 64 px clamp collapses the tail to 0.004 (and to 0 two pixels further).
    let far = black_alpha_in(&img, 400, 320, 60);
    assert!(
        (0.015..0.05).contains(&far),
        "a blur above 64 px must keep its reach, alpha was {far}"
    );
}

#[test]
fn layer_order_is_shadow_then_reflection_then_body() {
    // ref renderer.ts:3794-3830 — shadow first, reflection on top of it, body
    // above both.
    let effects = ShapeEffects {
        outer_shadow: Some(shadow(0, 200_000, false)), // 20 px down
        reflection: Some(VectorReflection {
            blur_radius_emu: 0,
            start_opacity: 0.5,
            end_opacity: 0.5,
            distance_emu: 0,
            direction_deg: 90.0,
            ..VectorReflection::default()
        }),
        ..Default::default()
    };
    let img = raster(effects, AffineTransform::default());

    // (50, 52) carries both the opaque shadow band (30..70) and the mirrored
    // body (50..90): 50 % of the body fill over black.
    let b = px(&img, 50, 52);
    let expected = [FILL[0] / 2, FILL[1] / 2, FILL[2] / 2];
    for channel in 0..3 {
        assert!(
            (b[channel] as i32 - expected[channel] as i32).abs() <= 2,
            "reflection must paint over the shadow, got {b:?}"
        );
    }
    assert_eq!(px(&img, 50, 30), FILL, "body still paints over both");
}

#[test]
fn glow_has_no_hot_rim() {
    // ref renderer.ts:2701 — `shadowBlur = rad` ⇒ sigma = rad / 2 with no
    // intensity curve, so the halo at the silhouette edge is half strength.
    let effects = ShapeEffects {
        glow: Some(VectorGlow { color: [255, 0, 0, 255], radius_emu: 200_000 }),
        ..Default::default()
    };
    // Body painted in the glow colour so rim and centre compare directly.
    let img = raster_filled(effects, AffineTransform::default(), [255, 0, 0, 255]);

    // 0.5 px outside the left edge (σ = 10 px): alpha ≈ 0.48 ⇒ G ≈ 132.
    let rim = px(&img, 9, 30);
    assert!(
        (115..155).contains(&rim[1]),
        "rim alpha must be ~0.48 of the glow, got {rim:?}"
    );
    // 8.5 px out: alpha ≈ 0.19 ⇒ G ≈ 207.
    let halo = px(&img, 1, 30);
    assert!(halo[1] > 185, "halo must decay outward, got {halo:?}");
    assert_eq!(px(&img, 50, 30), [255, 0, 0, 255], "body covers the glow core");
}

#[test]
fn soft_edge_bleeds_outward_and_keeps_the_interior() {
    // ref effects.ts:applySoftEdge — symmetric feather, sigma = rad / 3, with
    // the colour layer stretched rad outward past the geometry.
    let effects = ShapeEffects {
        soft_edge: Some(VectorSoftEdge { radius_emu: 100_000 }), // 10 px
        ..Default::default()
    };
    let img = raster(effects, AffineTransform::default());

    // 0.5 px outside the edge: feathered alpha ≈ 0.42 ⇒ R ≈ 176, never white.
    let outside = px(&img, 9, 30);
    assert!(
        outside[0] < 235 && outside[0] > 100,
        "soft edge must bleed outward, got {outside:?}"
    );
    // Past the bleed reach only the Gaussian tail is left (<= 1/255).
    let far = px(&img, 1, 30);
    assert!(far[0] >= 250, "the bleed must not reach 9 px out, got {far:?}");
    assert!(outside[0] + 60 < far[0], "the feather must decay outward");
    // §20.1.8.53: "the fill is not affected" — the interior stays opaque.
    assert_eq!(px(&img, 50, 30), FILL);
}

#[test]
fn inner_shadow_paints_only_inside_the_body() {
    let effects = ShapeEffects {
        inner_shadow: Some(VectorInnerShadow {
            color: [0, 0, 0, 255],
            blur_radius_emu: 20_000,  // 2 px
            distance_emu: 100_000,    // 10 px
            direction_deg: 90.0,
        }),
        ..Default::default()
    };
    let img = raster(effects, AffineTransform::default());

    // Clipped to the interior: nothing lands outside the silhouette.
    assert_eq!(px(&img, 50, 5), [255, 255, 255, 255], "no ink above the body");
    assert_eq!(px(&img, 2, 30), [255, 255, 255, 255], "no ink left of the body");
    // The inset band hugs the edge opposite the offset.
    let top = px(&img, 50, 12);
    assert!(top[0] < 120, "inset band missing at the top edge, got {top:?}");
    // The centre is beyond the shadow reach (|dist| + 3σ < 20 px).
    assert!(px(&img, 50, 30)[0] >= 60, "the centre must stay clear");
}

#[test]
fn rot_with_shape_controls_the_shadow_offset_frame() {
    // 90° clockwise about the body centre; the rotated body spans x 30..70,
    // y -10..70 px.
    let rotated = AffineTransform {
        sx: 0.0,
        ky: 1.0,
        kx: -1.0,
        sy: 0.0,
        tx: 800_000.0,
        ty: -200_000.0,
    };
    let effects = |rot: bool| ShapeEffects {
        outer_shadow: Some(shadow(0, 200_000, rot)),
        ..Default::default()
    };

    // rotWithShape="0": the 20 px offset stays page-oriented (dir 90 ⇒ down).
    let page = raster(effects(false), rotated);
    assert!(black_alpha(&page, 50, 75) > 0.5, "shadow must land below the body");
    assert!(black_alpha(&page, 25, 30) < 0.05, "no shadow left of the body");

    // rotWithShape="1": dir rotates with the shape (90 + 90 ⇒ 180 ⇒ left).
    let shape = raster(effects(true), rotated);
    assert!(black_alpha(&shape, 25, 30) > 0.5, "shadow must land left of the body");
    assert!(black_alpha(&shape, 50, 75) < 0.05, "no shadow below the body");
}


// ---------------------------------------------------------------------------
// Reflection — ECMA-376 §20.1.8.50, ref effects.ts:562-640 `applyReflection`.
// The body is the rect x 10..90, y 10..50 px; the mirrored copy hangs off its
// bottom edge (dir 90) with the band height h = 40 px, so the default copy
// covers y 50..90. Every probe uses a black body, so R = 255·(1 - alpha).
// ---------------------------------------------------------------------------

fn refl(
    st_a: f32,
    end_a: f32,
    st_pos: f32,
    end_pos: f32,
    dist_emu: i64,
) -> VectorReflection {
    VectorReflection {
        blur_radius_emu: 0,
        start_opacity: st_a,
        start_pos: st_pos,
        end_opacity: end_a,
        end_pos,
        distance_emu: dist_emu,
        direction_deg: 90.0,
        ..VectorReflection::default()
    }
}

/// Rasterize a reflection-only scene and return it with the alpha probes.
fn refl_image(r: VectorReflection) -> RgbaSlide {
    let effects = ShapeEffects { reflection: Some(r), ..Default::default() };
    raster_filled(effects, AffineTransform::default(), [0, 0, 0, 255])
}

fn close(actual: f32, expected: f32, tol: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= tol,
        "{what}: expected {expected} ± {tol}, got {actual}"
    );
}

#[test]
fn reflection_fades_to_zero_at_end_pos() {
    // stPos=0, endPos=50% ⇒ the ramp spans the first half of the band and the
    // band is fully transparent from half height down (effects.ts:612-622).
    let img = refl_image(refl(1.0, 0.0, 0.0, 0.5, 0));

    // y 55 → source row 45 → t = 5/40 = 0.125 ⇒ alpha = 1 - 0.125/0.5 = 0.75.
    close(black_alpha(&img, 50, 55), 0.75, 0.05, "alpha at t=0.125");
    // y 60 → t = 0.25 ⇒ 0.5.
    close(black_alpha(&img, 50, 60), 0.5, 0.05, "alpha at t=0.25");
    // y 69 → t = 0.475 ⇒ 0.05.
    close(black_alpha(&img, 50, 69), 0.05, 0.05, "alpha at t=0.475");
    // Half height and below: `endA` holds, so no ink at all.
    for y in [70, 71, 75, 89] {
        assert_eq!(px(&img, 50, y), [255, 255, 255, 255], "endPos=50% must clear row {y}");
    }
}

#[test]
fn reflection_end_alpha_zero_leaves_no_ink_past_the_fade() {
    // Control: with the default endPos=100% the ramp keeps fading over the
    // whole band, so row 75 still carries ink.
    let full = refl_image(refl(1.0, 0.0, 0.0, 1.0, 0));
    close(black_alpha(&full, 50, 75), 0.375, 0.05, "alpha at t=0.625");

    // endA=0 with endPos=50%: nothing survives past the fade.
    let cut = refl_image(refl(1.0, 0.0, 0.0, 0.5, 0));
    assert_eq!(px(&cut, 50, 75), [255, 255, 255, 255]);
    assert_eq!(px(&cut, 20, 80), [255, 255, 255, 255]);
    // ... while the stA band right under the shape is still near-opaque (the
    // ramp has only advanced by t = 1/40 at the first row).
    assert!(
        black_alpha(&cut, 50, 51) > 0.85,
        "stA band at the shape edge must stay opaque"
    );
}

#[test]
fn reflection_direction_90_mirrors_below_the_body() {
    // dir 90° (the reference default): mirror about the bottom edge y = 50.
    let img = refl_image(refl(1.0, 1.0, 0.0, 1.0, 0));

    // The mirrored silhouette keeps the body's x range and lands in y 50..90.
    close(black_alpha(&img, 20, 60), 1.0, 0.02, "mirrored body below");
    close(black_alpha(&img, 50, 88), 1.0, 0.02, "band bottom");
    assert_eq!(px(&img, 5, 60), [255, 255, 255, 255], "nothing left of the body");
    assert_eq!(px(&img, 95, 60), [255, 255, 255, 255], "nothing right of the body");
    assert_eq!(px(&img, 50, 95), [255, 255, 255, 255], "nothing past the band");
}

#[test]
fn reflection_direction_0_mirrors_to_the_right() {
    // dir 0° turns the mirror axis a quarter turn, so the copy leaves through
    // the shape's right side (effects.ts:626-639, generalised for dir != 90).
    let mut r = refl(1.0, 1.0, 0.0, 1.0, 0);
    r.direction_deg = 0.0;
    let img = refl_image(r);

    // The pivot is the bottom-centre anchor (50, 50); the copy covers
    // x 50..90, y 10..90.
    close(black_alpha(&img, 60, 70), 1.0, 0.02, "mirrored copy right of the pivot");
    close(black_alpha(&img, 88, 20), 1.0, 0.02, "copy reaches the body's top");
    assert_eq!(px(&img, 20, 70), [255, 255, 255, 255], "nothing left of the pivot");
    assert_eq!(px(&img, 60, 95), [255, 255, 255, 255], "nothing past the copy");
}

#[test]
fn reflection_scale_y_halves_the_copy() {
    // sy = 0.5 ⇒ the mirrored copy is half the band height: y 50..70.
    let mut r = refl(1.0, 1.0, 0.0, 1.0, 0);
    r.scale_y = 0.5;
    let img = refl_image(r);

    close(black_alpha(&img, 50, 60), 1.0, 0.02, "inside the halved copy");
    close(black_alpha(&img, 50, 68), 1.0, 0.02, "copy bottom edge");
    assert_eq!(px(&img, 50, 72), [255, 255, 255, 255], "copy must stop at half height");
    assert_eq!(px(&img, 50, 85), [255, 255, 255, 255], "copy must stop at half height");
}

#[test]
fn reflection_scale_x_narrows_the_copy() {
    // sx = 0.5 about the bottom-centre pivot ⇒ the copy spans x 30..70.
    let mut r = refl(1.0, 1.0, 0.0, 1.0, 0);
    r.scale_x = 0.5;
    let img = refl_image(r);

    close(black_alpha(&img, 40, 60), 1.0, 0.02, "inside the narrowed copy");
    close(black_alpha(&img, 68, 60), 1.0, 0.02, "narrowed right edge");
    assert_eq!(px(&img, 80, 60), [255, 255, 255, 255], "no ink right of the copy");
    assert_eq!(px(&img, 20, 60), [255, 255, 255, 255], "no ink left of the copy");
}

#[test]
fn reflection_distance_leaves_a_gap() {
    // dist = 10 px along dir 90 ⇒ the copy starts at y 60, not y 50.
    let img = refl_image(refl(1.0, 1.0, 0.0, 1.0, 100_000));

    for y in [51, 55, 58] {
        assert_eq!(px(&img, 50, y), [255, 255, 255, 255], "dist must leave a 10 px gap (y {y})");
    }
    close(black_alpha(&img, 50, 62), 1.0, 0.02, "copy starts after the gap");
    close(black_alpha(&img, 50, 95), 1.0, 0.02, "copy shifted down by dist");
}

#[test]
fn reflection_alpha_decreases_monotonically_down_the_band() {
    // stA=1 → endA=0.1 between stPos=20% and endPos=80% (effects.ts:612-622).
    let img = refl_image(refl(1.0, 0.1, 0.2, 0.8, 0));

    // The stA plateau holds until t = 0.2 ⇒ y 58.
    for y in [51, 54, 57] {
        close(black_alpha(&img, 50, y), 1.0, 0.02, "stA plateau");
    }
    // t = 0.5 at y 70 ⇒ 1 - 0.9·(0.3/0.6) = 0.55.
    close(black_alpha(&img, 50, 70), 0.55, 0.05, "mid-ramp");
    // Past endPos the endA plateau holds.
    for y in [86, 88] {
        close(black_alpha(&img, 50, y), 0.1, 0.04, "endA plateau");
    }

    let mut previous = 2.0f32;
    for y in 50..90 {
        let alpha = black_alpha(&img, 50, y);
        assert!(
            alpha <= previous + 0.02,
            "alpha must not increase down the band: row {y} is {alpha}, previous {previous}"
        );
        previous = alpha;
    }
}

#[test]
fn reflection_blur_uses_the_authored_radius_as_sigma() {
    // ref effects.ts:601-604 — the aux is painted with `filter='blur(r)'`, so
    // sigma is the authored radius: a 5 px blur inks past the band edge.
    let mut blurry = refl(1.0, 1.0, 0.0, 1.0, 0);
    blurry.blur_radius_emu = 50_000;
    let img = refl_image(blurry);

    let near = black_alpha(&img, 50, 91);
    let mid = black_alpha(&img, 50, 94);
    let far = black_alpha(&img, 50, 98);
    assert!((0.25..0.7).contains(&near), "sigma=5 px must bleed past the edge, got {near}");
    assert!(near > mid && mid > far, "the bleed must decay: {near} / {mid} / {far}");

    // Control: no blur ⇒ a hard edge exactly at the band bottom.
    let sharp = refl_image(refl(1.0, 1.0, 0.0, 1.0, 0));
    assert_eq!(px(&sharp, 50, 91), [255, 255, 255, 255], "no bleed without blur");
}
