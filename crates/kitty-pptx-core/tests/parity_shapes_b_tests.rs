//! 1:1 parity ports of the reference vitest specs for batch `shapes-b`.
//!
//! Reference specs (read-only): `/tmp/oooxml/packages/pptx/src/`
//! * `bevel-band-conversion.test.ts` — ECMA-376 §20.1.5.3 bevel `w`/`h` are EMU
//!   lengths; the device band is `w(EMU) × cssScale × devScale`.
//! * `shape-effect-transform.test.ts` — `transformedEffectBounds` returns the
//!   AABB of the transformed shape box (`renderer.ts:3603`).
//! * `shape-inner-shadow.test.ts` — a no-fill shape must not synthesize an
//!   inner-shadow surface (`renderer.ts:paintWithRasterEffects`).
//! * `shape-orthographic-bevel.test.ts` — mitered outline coverage.
//! * `shape-effect-line-decoration.test.ts` — connector leader stays in the
//!   effect silhouette / in the 3-D projection path.
//!
//! Cases with no Rust API are listed in
//! `tests/parity/coverage/shapes-b.md` as BLOCKED or N/A, never silently
//! dropped.

use crossbeam_channel::unbounded;
use kitty_pptx_core::*;
use pptx_model::Slide;

#[path = "shapes_b_support/bevel_effect_geometry.rs"]
mod bevel_effect_geometry;
#[path = "shapes_b_support/line_end_cases.rs"]
mod line_end_cases;

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

/// One EMU-per-px conversion used by every fixture below (AGENTS.md §4).
fn px(pixels: f32) -> i64 {
    (pixels * 9525.0).round() as i64
}

/// Slide-6 reference viewport: 12 192 000 EMU across 1920 px, i.e. 6350 EMU
/// per device pixel (`cssScale = 1920 / 12192000`).
const REF_SLIDE_W: i64 = 12_192_000;
const REF_SLIDE_H: i64 = 2_540_000;

fn ref_px(pixels: f32) -> i64 {
    (pixels * 6350.0).round() as i64
}

fn compile_slide(elements: Vec<serde_json::Value>, w_px: u32, h_px: u32) -> VectorSlide {
    compile_slide_emu(elements, px(w_px as f32), px(h_px as f32))
}

fn compile_slide_emu(elements: Vec<serde_json::Value>, w_emu: i64, h_emu: i64) -> VectorSlide {
    let (tx, rx) = unbounded();
    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "background": { "fillType": "solid", "color": "FFFFFF" },
        "elements": elements,
        "elementSources": [],
        "comments": []
    });
    let slide: Slide = serde_json::from_value(slide_json).expect("deserialize slide");
    let vector = DefaultSlideCompiler::new()
        .compile(&slide, w_emu, h_emu, &tx)
        .expect("compile");
    drop(rx);
    vector
}

fn raster(slide: &VectorSlide, w: u32, h: u32) -> RgbaSlide {
    TinySkiaRasterizer::new()
        .rasterize(slide, &RenderTargetSpec { width_px: w, height_px: h }, &NoAssets)
        .expect("rasterize")
}

fn at(img: &RgbaSlide, x: usize, y: usize) -> [u8; 4] {
    let base = (y * img.width as usize + x) * 4;
    [
        img.pixels[base],
        img.pixels[base + 1],
        img.pixels[base + 2],
        img.pixels[base + 3],
    ]
}

const WHITE: [u8; 4] = [255, 255, 255, 255];

// ── bevel-band-conversion.test.ts ───────────────────────────────────────────

/// The reference fixture: slide 12 192 000 EMU wide rendered 1920 px wide, so
/// `cssScale = 1920 / 12192000`; bevel `w` = 304 800 EMU (24 pt), `h` =
/// 152 400 EMU (12 pt). `ref renderer.ts:6302-6330` (`buildBevelInputs`):
/// `widthPx = w × scale × devScale`, and the band must not pick up a second
/// scale factor (the slide-6 "flat cut" suspicion).
fn beveled_rect(bevel: Option<(i64, i64)>) -> serde_json::Value {
    let mut shape = serde_json::json!({
        "type": "shape",
        "x": ref_px(200.0),
        "y": ref_px(100.0),
        "width": ref_px(1520.0),
        "height": ref_px(200.0),
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "rect",
        "fill": { "fillType": "solid", "color": "4472C4" },
        "scene3d": {
            "camera": { "prst": "orthographicFront" },
            "lightRig": { "rig": "matte", "dir": "t" }
        }
    });
    if let Some((w, h)) = bevel {
        shape["sp3d"] = serde_json::json!({
            "prstMaterial": "matte",
            "bevelT": { "w": w, "h": h, "prst": "hardEdge" }
        });
    }
    shape
}

/// Depth of the shaded band below the shape's top edge, in device pixels.
/// `rasterizer/bevel.rs:44` shades exactly `dist < widthPx` with `dist` starting
/// at 1 for the first interior row, so the band width is the last shaded row
/// **plus one**.
fn measured_band_depth(
    plain: &RgbaSlide,
    beveled: &RgbaSlide,
    x: usize,
    top: usize,
    max_scan: usize,
) -> usize {
    // The shading factor crosses 1.0 once inside the band (the profile is
    // steepest at the silhouette), so a single row can round back onto the flat
    // colour. The band width is the *last* shaded row, not the first gap.
    // `max_scan` stops before the opposite edge's band (and before the
    // mid-height rows, where the two bands meet).
    (0..max_scan.min(plain.height as usize - top))
        .filter(|d| at(plain, x, top + d) != at(beveled, x, top + d))
        .map(|d| d + 1)
        .max()
        .unwrap_or(0)
}

#[test]
fn bevel_band_is_the_declared_emu_width_at_unit_device_scale() {
    // devScale 1 of the reference loop, with the slide-6 declared values.
    const BEVEL_W_EMU: i64 = 304_800; // 24 pt
    const BEVEL_H_EMU: i64 = 152_400; // 12 pt
    let css_scale = 1920.0 / 12_192_000.0;
    let expect_width = BEVEL_W_EMU as f32 * css_scale;

    let plain = compile_slide_emu(vec![beveled_rect(None)], REF_SLIDE_W, REF_SLIDE_H);
    let beveled = compile_slide_emu(
        vec![beveled_rect(Some((BEVEL_W_EMU, BEVEL_H_EMU)))],
        REF_SLIDE_W,
        REF_SLIDE_H,
    );
    let plain = raster(&plain, 1920, 400);
    let beveled = raster(&beveled, 1920, 400);

    let depth = measured_band_depth(&plain, &beveled, 960, 100, 64);
    // `dist` starts at 1 for the first interior row, so the shaded rows are
    // `1..widthPx` — one fewer row than the converted band width.
    assert!(
        (depth as f32 - (expect_width - 1.0)).abs() <= 1.0,
        "bevel band must be w(EMU)×cssScale = {expect_width} px ±1, measured {depth} shaded rows"
    );
    // Sanity: 24 pt at 1920 px wide ⇒ exactly 48 device px per devScale unit.
    assert!(
        (expect_width - 48.0).abs() < 1e-3,
        "fixture must yield the slide-6 48 px band, got {expect_width}"
    );
    // Shaded inside the band, flat beyond it.
    assert_ne!(at(&beveled, 960, 146), at(&plain, 960, 146), "row at distance 46 is shaded");
    assert_eq!(at(&beveled, 960, 150), at(&plain, 960, 150), "row at distance 50 is flat");
}

#[test]
fn bevel_band_scales_linearly_with_device_scale() {
    // "the band scales LINEARLY with devScale (no super-linear blow-up)".
    // `rasterizer/bevel.rs:19` clamps the band to 48 px, so devScale 4 and 8 of
    // the reference loop are unreachable; linearity is pinned between device
    // scales 1× and ¼× instead, both of which stay inside the clamp.
    const BEVEL_W_EMU: i64 = 304_800;
    let full = {
        let plain = raster(
            &compile_slide_emu(vec![beveled_rect(None)], REF_SLIDE_W, REF_SLIDE_H),
            1920,
            400,
        );
        let beveled = raster(
            &compile_slide_emu(vec![beveled_rect(Some((BEVEL_W_EMU, 152_400)))], REF_SLIDE_W, REF_SLIDE_H),
            1920,
            400,
        );
        measured_band_depth(&plain, &beveled, 960, 100, 64)
    };
    // Same slide, a ¼-size device target: the authored EMU band must shrink by
    // the same factor (`widthPx` is linear in the device scale).
    let quarter = {
        let plain = raster(
            &compile_slide_emu(vec![beveled_rect(None)], REF_SLIDE_W, REF_SLIDE_H),
            480,
            100,
        );
        let beveled = raster(
            &compile_slide_emu(vec![beveled_rect(Some((BEVEL_W_EMU, 152_400)))], REF_SLIDE_W, REF_SLIDE_H),
            480,
            100,
        );
        measured_band_depth(&plain, &beveled, 240, 25, 20)
    };
    assert!(
        ((full + 1) as f32 - 4.0 * (quarter + 1) as f32).abs() <= 1.0,
        "band must scale linearly: {} px at 1× vs {} px at ¼×",
        full + 1,
        quarter + 1
    );
    assert_eq!(full + 1, 48, "the 1× fixture must keep the slide-6 48 px band");
}

// ── shape-effect-transform.test.ts ──────────────────────────────────────────

/// The reference rect (464, 120, 146, 200) under `a=1.5, e=72, f=45`
/// (`shape-effect-transform.test.ts:8-16`). `transformedEffectBounds` is the
/// AABB of the four transformed corners; the observable equivalent here is the
/// inked pixel box of the rasterized, transformed path.
fn effect_bounds(transform: AffineTransform, x: f32, y: f32, w: f32, h: f32, cw: u32, ch: u32) -> (u32, u32, u32, u32) {
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x, y },
            PathSegment::LineTo { x: x + w, y },
            PathSegment::LineTo { x: x + w, y: y + h },
            PathSegment::LineTo { x, y: y + h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    let mut slide = VectorSlide::new(cw as i64, ch as i64);
    slide.background = Some(VectorBackground::Solid(WHITE));
    slide.commands.push(VectorCommand::draw_path(
        path,
        Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])),
        None,
        transform,
    ));
    let img = raster(&slide, cw, ch);
    let mut bounds = (u32::MAX, u32::MAX, 0u32, 0u32);
    for yy in 0..img.height as usize {
        for xx in 0..img.width as usize {
            if at(&img, xx, yy) == WHITE {
                continue;
            }
            bounds.0 = bounds.0.min(xx as u32);
            bounds.1 = bounds.1.min(yy as u32);
            bounds.2 = bounds.2.max(xx as u32);
            bounds.3 = bounds.3.max(yy as u32);
        }
    }
    (bounds.0, bounds.1, bounds.2 - bounds.0 + 1, bounds.3 - bounds.1 + 1)
}

#[test]
fn effect_bounds_include_group_translation_and_scale() {
    // expect({ x: 768, y: 225, w: 219, h: 300 })
    let transform = AffineTransform {
        sx: 1.5,
        ky: 0.0,
        kx: 0.0,
        sy: 1.5,
        tx: 72.0,
        ty: 45.0,
    };
    let (x, y, w, h) = effect_bounds(transform, 464.0, 120.0, 146.0, 200.0, 1000, 600);
    for (got, want, label) in [
        (x as i32, 768, "x"),
        (y as i32, 225, "y"),
        (w as i32, 219, "w"),
        (h as i32, 300, "h"),
    ] {
        assert!((got - want).abs() <= 1, "effect bounds {label}: got {got}, want {want}");
    }
}

#[test]
fn effect_bounds_are_the_rotated_aabb() {
    // expect({ x: 400, y: 50, w: 60, h: 80 }) for the 90° matrix
    // (a=0,b=2,c=-2,d=0) — the device AABB of the rotated box.
    let transform = AffineTransform {
        sx: 0.0,
        ky: 2.0,
        kx: -2.0,
        sy: 0.0,
        tx: 500.0,
        ty: 30.0,
    };
    let (x, y, w, h) = effect_bounds(transform, 10.0, 20.0, 40.0, 30.0, 600, 300);
    for (got, want, label) in [
        (x as i32, 400, "x"),
        (y as i32, 50, "y"),
        (w as i32, 60, "w"),
        (h as i32, 80, "h"),
    ] {
        assert!((got - want).abs() <= 1, "rotated effect bounds {label}: got {got}, want {want}");
    }
}

// ── shape-inner-shadow.test.ts ──────────────────────────────────────────────

#[test]
fn inner_shadow_does_not_synthesize_a_surface_for_a_no_fill_shape() {
    // ref shape-inner-shadow.test.ts:63-77 — a shapeless interior (fill: null)
    // must leave the canvas untouched instead of painting the inner-shadow
    // surface. Fixture numbers are the spec's: 914 400 EMU offsets, 4 572 000 ×
    // 2 286 000 EMU box, blur 63 500 / dist 50 800 / dir 315.
    let shape = serde_json::json!({
        "type": "shape",
        "x": 914_400,
        "y": 914_400,
        "width": 4_572_000,
        "height": 2_286_000,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "rect",
        "fill": null,
        "stroke": null,
        "innerShadow": { "color": "000000", "alpha": 0.5, "blur": 63_500, "dist": 50_800, "dir": 315 }
    });
    let slide = compile_slide(vec![shape], 960, 540);
    let img = raster(&slide, 960, 540);
    let stray = (0..img.height as usize)
        .flat_map(|y| (0..img.width as usize).map(move |x| (x, y)))
        .find(|&(x, y)| at(&img, x, y) != WHITE);
    assert_eq!(stray, None, "a no-fill shape must paint no inner-shadow surface");
}

// ── shape-orthographic-bevel.test.ts ────────────────────────────────────────

#[test]
fn inner_shadow_covers_the_complete_mitered_outline() {
    // ref shape-orthographic-bevel.test.ts:117-137 — `triangle` with a 20 pt
    // miter/square stroke and an inner shadow offset past the whole shape: every
    // opaque outline pixel (including the miter outside the nominal geometry)
    // must receive 50 % black.
    //
    // Adapted: the Rust effect layer is clipped to the target pixmap, so the
    // authored 400 px offset (which exceeds the canvas) becomes 150 px — still
    // larger than the 96 px shape — and the shape is inset 250 px so the whole
    // shifted inverse mask stays inside the layer (the reference's aux canvases
    // are padded by |dist|, the Rust layer is not).
    fn fixture(inner_shadow: bool) -> serde_json::Value {
        let mut shape = serde_json::json!({
            "type": "shape",
            "x": px(250.0),
            "y": 914_400,
            "width": 914_400,
            "height": 914_400,
            "rotation": 0.0,
            "flipH": false,
            "flipV": false,
            "geometry": "triangle",
            "fill": { "fillType": "solid", "color": "4472C4" },
            "stroke": { "color": "4472C4", "width": 190_500, "lineJoin": "miter", "lineCap": "square" }
        });
        if inner_shadow {
            shape["innerShadow"] = serde_json::json!({
                "color": "000000", "alpha": 0.5, "blur": 0, "dist": 150 * 9525_i64, "dir": 0
            });
        }
        shape
    }

    let plain = raster(&compile_slide(vec![fixture(false)], 400, 400), 400, 400);
    let shadowed = raster(&compile_slide(vec![fixture(true)], 400, 400), 400, 400);

    let mut opaque = 0usize;
    for y in 0..400usize {
        for x in 0..400usize {
            if at(&plain, x, y) != [0x44, 0x72, 0xC4, 255] {
                continue;
            }
            opaque += 1;
            let got = at(&shadowed, x, y);
            for (channel, want) in [34u8, 57, 98].into_iter().enumerate() {
                assert!(
                    (got[channel] as i32 - want as i32).abs() <= 1,
                    "outline pixel ({x},{y}) must take 50% black: got {got:?}"
                );
            }
        }
    }
    assert!(opaque > 100, "fixture must have a solid opaque outline, counted {opaque}");
}

// ── shape-effect-line-decoration.test.ts ────────────────────────────────────

/// A connector leader: an open 80 px segment stroked 5 px with the spec's
/// outer shadow (blur 63 500, dist 50 800, dir 45).
fn leader(with_stroke: bool) -> VectorSlide {
    let mut slide = VectorSlide::new(px(200.0), px(100.0));
    slide.background = Some(VectorBackground::Solid(WHITE));
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: px(10.0) as f32, y: px(50.0) as f32 },
            PathSegment::LineTo { x: px(90.0) as f32, y: px(50.0) as f32 },
        ],
        FillRule::NonZero,
    );
    let mut effects = ShapeEffects {
        outer_shadow: Some(VectorOuterShadow {
            color: [0, 0, 0, 128],
            blur_radius_emu: 63_500,
            distance_emu: 50_800,
            direction_deg: 45.0,
            ..VectorOuterShadow::default()
        }),
        ..ShapeEffects::default()
    };
    if !with_stroke {
        effects.outer_shadow = None;
    }
    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill: None,
        stroke: with_stroke.then_some(VectorStroke {
            color: [0x44, 0x72, 0xC4, 255],
            width_emu: 47_625, // 5 px
            dash_segments: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            paint: None,
        }),
        transform: AffineTransform::identity(),
        effects: Box::new(effects),
    });
    slide
}

#[test]
fn outer_shadow_silhouette_includes_the_connector_leader() {
    // ref shape-effect-line-decoration.test.ts:120-145 — the shadow aux canvas
    // must receive the *stroke* of a fill-less connector (`strokes > 0`), so the
    // shadow follows the leader instead of the (empty) fill.
    let shadowed = raster(&leader(true), 200, 100);
    let bare = raster(&leader(false), 200, 100);

    // The stroke spans y 47.5..52.5; its shadow is the same band pushed
    // (2.83, 2.83) px down-right and blurred with σ = 6.67 px, so the shadow
    // core sits at y ≈ 52.8 with a 5/(6.67·2.51) = 0.29 coverage ⇒
    // 0.5·0.29 = 0.15 alpha, i.e. 255·(1−0.15) ≈ 218.
    let ink = at(&shadowed, 40, 53);
    assert!(
        (210..226).contains(&ink[0]),
        "shadow must be a real 50%-black band over white, red channel {}",
        ink[0]
    );
    assert!(ink[1] == ink[0] && ink[2] == ink[0], "the shadow is neutral, got {ink:?}");
    // The shadow follows the whole leader, not just one point of it.
    assert!((210..226).contains(&at(&shadowed, 20, 53)[0]), "shadow at the leader start");
    assert!((210..226).contains(&at(&shadowed, 80, 53)[0]), "shadow at the leader end");
    // Past the leader (x = 90) and its 3σ reach (∼20 px) nothing is painted.
    assert_eq!(at(&shadowed, 150, 53), WHITE, "no shadow beyond the leader");
    assert_eq!(at(&bare, 40, 53), WHITE, "no stroke ⇒ no shadow");
    // The body itself still paints on top.
    assert_eq!(at(&shadowed, 50, 50), [0x44, 0x72, 0xC4, 255]);
}

#[test]
fn partly_off_viewport_shape_stays_in_the_projection_path() {
    // ref shape-effect-line-decoration.test.ts:147-176 — a shadowed shape that
    // hangs off the left edge must still be painted through the 3-D camera
    // projection, not flattened back to a billboard rect.
    //
    // `isometricLeftUp` (lat −35.264°, lon −45°) maps the 100 × 60 local box to
    // the parallelogram (14.645,−14.9) (85.355,25.92) (14.645,34.08)
    // (85.355,74.9); rebased on the (−40, 20) shape origin that is
    // x ∈ [−25.36, 45.36]. The flat rect would cover x ∈ [−40, 60].
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 60.0 },
            PathSegment::LineTo { x: 0.0, y: 60.0 },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    let mut slide = VectorSlide::new(200, 100);
    slide.background = Some(VectorBackground::Solid(WHITE));
    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill: Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])),
        stroke: None,
        transform: AffineTransform::translate(-40.0, 20.0),
        effects: Box::new(ShapeEffects {
            scene_3d: Some(VectorScene3d {
                camera_type: "isometricLeftUp".to_string(),
                fov_deg: 0.0,
                zoom: 1.0,
                rot: None,
                light_rig_type: "threePt".to_string(),
                light_direction_deg: 270.0,
            }),
            outer_shadow: Some(VectorOuterShadow {
                color: [0, 0, 0, 255],
                blur_radius_emu: 0,
                distance_emu: 38_100, // 4 px
                direction_deg: 45.0,
                ..VectorOuterShadow::default()
            }),
            ..ShapeEffects::default()
        }),
    });
    let img = raster(&slide, 200, 100);

    // Inside the projected parallelogram (top edge at y≈37 for x=30).
    assert_eq!(at(&img, 30, 60), [0x44, 0x72, 0xC4, 255], "projected face is painted");
    // Inside the flat rect (x < 60) but right of the projected silhouette
    // (x > 45.36), and up-right of the 45° shadow: still background.
    assert_eq!(at(&img, 56, 30), WHITE, "no flat billboard fallback");
}
