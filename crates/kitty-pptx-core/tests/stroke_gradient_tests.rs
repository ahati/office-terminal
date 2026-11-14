//! Stroke normalisation and DrawingML gradient-frame parity with the reference
//! renderer (`packages/core/src/shape/stroke-geometry.ts`,
//! `packages/core/src/draw/dash.ts`, `packages/core/src/shape/paint.ts`).

use std::sync::Arc;

use kitty_pptx_core::compiler::style::{
    compile_fill, compile_stroke, linear_gradient_axis, linear_gradient_axis_for_path, path_bounds,
    radial_gradient_frame, radial_gradient_frame_for_path,
};
use kitty_pptx_core::{
    AffineTransform, AssetError, AssetStore, AssetStoreStats, DecodedImage,
    DefaultSlideSvgSerializer, FillRule, LineCap, PathSegment, RenderTargetSpec, RgbaSlide,
    SizeHint, SlideRasterizer, SlideSvgSerializer, SvgTextMode, TinySkiaRasterizer,
    VectorBackground, VectorCommand,
    VectorGradient, VectorPaint, VectorPath, VectorSlide, VectorStroke,
};

struct EmptyAssetStore;

impl AssetStore for EmptyAssetStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn stroke_with_style(dash_style: Option<&str>) -> pptx_model::Stroke {
    pptx_model::Stroke {
        color: "000000".to_string(),
        width: 12_700,
        fill: None,
        dash_style: dash_style.map(str::to_string),
        custom_dash: Vec::new(),
        line_cap: None,
        line_join: None,
        miter_limit: None,
        alignment: None,
        head_end: None,
        tail_end: None,
        cmpd: None,
    }
}

fn stop(position: f64, color: &str) -> pptx_model::GradStop {
    pptx_model::GradStop {
        position,
        color: color.to_string(),
    }
}

fn gradient_fill(stops: Vec<pptx_model::GradStop>) -> pptx_model::Fill {
    pptx_model::Fill::Gradient {
        stops,
        angle: 0.0,
        grad_type: "linear".to_string(),
        scaled: None,
        path: None,
        fill_to_rect: None,
        tile_rect: None,
        flip: None,
        rot_with_shape: None,
    }
}

fn rect_path(w: f32, h: f32) -> VectorPath {
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        fill_rule: FillRule::NonZero,
    }
}

fn slide_with(width: i64, height: i64, path: VectorPath, fill: Option<VectorPaint>) -> VectorSlide {
    VectorSlide {
        width_emu: width,
        height_emu: height,
        background: Some(VectorBackground::Solid([255, 255, 255, 255])),
        commands: vec![VectorCommand::DrawPath {
            path,
            fill,
            stroke: None,
            transform: AffineTransform::default(),
            effects: Box::default(),
        }],
        hyperlinks: Vec::new(),
    }
}

fn render(slide: &VectorSlide, width: u32, height: u32) -> RgbaSlide {
    let store = Arc::new(EmptyAssetStore);
    TinySkiaRasterizer::new()
        .rasterize(
            slide,
            &RenderTargetSpec {
                width_px: width,
                height_px: height,
            },
            store.as_ref(),
        )
        .expect("rasterization failed")
}

fn svg(slide: &VectorSlide) -> String {
    let store = Arc::new(EmptyAssetStore);
    DefaultSlideSvgSerializer::new()
        .serialize_svg(slide, store.as_ref(), SvgTextMode::VectorGlyphs)
        .expect("svg serialization failed")
}

fn pixel(slide: &RgbaSlide, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * slide.width + x) * 4) as usize;
    [
        slide.pixels[offset],
        slide.pixels[offset + 1],
        slide.pixels[offset + 2],
        slide.pixels[offset + 3],
    ]
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, label: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{label}: expected {expected}, got {actual}"
    );
}

fn gradient_axis_length(((x1, y1), (x2, y2)): ((f32, f32), (f32, f32))) -> f32 {
    ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt()
}

// ---------------------------------------------------------------------------
// Stroke geometry
// ---------------------------------------------------------------------------

#[test]
fn preset_dash_table_matches_the_reference_for_all_eleven_values() {
    // ref packages/core/src/draw/dash.ts:146-157 (relative to line width).
    let cases: &[(&str, Option<&[f32]>)] = &[
        ("solid", None),
        ("dash", Some(&[6.0, 3.0])),
        ("dot", Some(&[1.5, 3.0])),
        ("dashDot", Some(&[6.0, 3.0, 1.5, 3.0])),
        ("lgDash", Some(&[10.0, 4.0])),
        ("lgDashDot", Some(&[10.0, 4.0, 1.5, 4.0])),
        ("lgDashDotDot", Some(&[10.0, 4.0, 1.5, 4.0, 1.5, 4.0])),
        ("sysDash", Some(&[4.0, 2.0])),
        ("sysDot", Some(&[1.0, 2.0])),
        ("sysDashDot", Some(&[4.0, 2.0, 1.0, 2.0])),
        ("sysDashDotDot", Some(&[4.0, 2.0, 1.0, 2.0, 1.0, 2.0])),
    ];
    assert_eq!(cases.len(), 11);

    for (style, expected) in cases {
        let stroke = stroke_with_style(Some(style));
        let compiled = compile_stroke(&stroke).expect("stroke compiles");
        assert_eq!(
            compiled.dash_segments.as_deref(),
            *expected,
            "dash style {style}"
        );
    }

    let unknown = compile_stroke(&stroke_with_style(Some("notARealStyle")));
    assert_eq!(
        unknown.expect("stroke compiles").dash_segments, None,
        "unknown preset must stay solid"
    );
}

#[test]
fn custom_dash_wins_over_the_preset_and_zero_on_segments_use_a_square_cap() {
    let mut stroke = stroke_with_style(Some("dot"));
    stroke.custom_dash = vec![pptx_model::StrokeDashSegment {
        dash: 2.0,
        space: 1.0,
    }];
    let compiled = compile_stroke(&stroke).expect("stroke compiles");
    assert_eq!(compiled.dash_segments.as_deref(), Some(&[2.0, 1.0][..]));

    // ref stroke-geometry.ts:20-21 — a zero on-segment promotes butt -> square.
    let mut zero_dash = stroke_with_style(None);
    zero_dash.custom_dash = vec![pptx_model::StrokeDashSegment {
        dash: 0.0,
        space: 2.0,
    }];
    let compiled = compile_stroke(&zero_dash).expect("stroke compiles");
    assert_eq!(compiled.dash_segments.as_deref(), Some(&[0.0, 2.0][..]));
    assert_eq!(compiled.line_cap, LineCap::Square);

    // A zero off-segment must keep the flat cap.
    let mut zero_gap = stroke_with_style(None);
    zero_gap.custom_dash = vec![pptx_model::StrokeDashSegment {
        dash: 2.0,
        space: 0.0,
    }];
    assert_eq!(
        compile_stroke(&zero_gap).expect("stroke compiles").line_cap,
        LineCap::Butt
    );
}

#[test]
fn drawingml_default_line_cap_is_butt() {
    // ref packages/core/src/shape/stroke-geometry.ts:19
    assert_eq!(
        compile_stroke(&stroke_with_style(None))
            .expect("stroke compiles")
            .line_cap,
        LineCap::Butt
    );

    let mut flat = stroke_with_style(None);
    flat.line_cap = Some("flat".to_string());
    assert_eq!(
        compile_stroke(&flat).expect("stroke compiles").line_cap,
        LineCap::Butt
    );
}

#[test]
fn hairline_stroke_floors_at_half_a_pixel() {
    let path = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 20.0, y: 150.0 },
            PathSegment::LineTo { x: 280.0, y: 150.0 },
        ],
        fill_rule: FillRule::NonZero,
    };
    let slide = VectorSlide {
        width_emu: 300,
        height_emu: 300,
        background: Some(VectorBackground::Solid([255, 255, 255, 255])),
        commands: vec![VectorCommand::DrawPath {
            path,
            fill: None,
            stroke: Some(VectorStroke {
                color: [0, 0, 0, 255],
                // 0 EMU * 1 px/EMU = 0 px => the reference floor is 0.5 px.
                width_emu: 0,
                dash_segments: None,
                line_cap: LineCap::Butt,
                line_join: kitty_pptx_core::LineJoin::Miter,
                paint: None,
                ..VectorStroke::default()
            }),
            transform: AffineTransform::default(),
            effects: Box::default(),
        }],
        hyperlinks: Vec::new(),
    };
    let rendered = render(&slide, 300, 300);

    // Total ink across the column equals the stroke width in pixels: 0.5 px
    // (a 1.0 px floor would deposit twice as much).
    let darkness: u32 = (140..=160)
        .map(|y| 255 - pixel(&rendered, 150, y)[0] as u32)
        .sum();
    assert!(
        (100..=165).contains(&darkness),
        "expected ~128 ink units for a 0.5 px hairline, got {darkness}"
    );
}

#[test]
fn sharp_miter_join_is_not_clipped_at_the_reference_limit() {
    // Interior angle 20 deg => miter ratio 1/sin(10 deg) ~= 5.76, which the old
    // limit of 4 beveled but the reference default of 10 keeps.
    let half_rise = 100.0 * 10f32.to_radians().tan();
    let path = VectorPath {
        segments: vec![
            PathSegment::MoveTo {
                x: 250.0,
                y: 150.0 - half_rise,
            },
            PathSegment::LineTo { x: 150.0, y: 150.0 },
            PathSegment::LineTo {
                x: 250.0,
                y: 150.0 + half_rise,
            },
        ],
        fill_rule: FillRule::NonZero,
    };
    let slide = VectorSlide {
        width_emu: 300,
        height_emu: 300,
        background: Some(VectorBackground::Solid([255, 255, 255, 255])),
        commands: vec![VectorCommand::DrawPath {
            path,
            fill: None,
            stroke: Some(VectorStroke {
                color: [0, 0, 0, 255],
                width_emu: 10,
                dash_segments: None,
                line_cap: LineCap::Butt,
                line_join: kitty_pptx_core::LineJoin::Miter,
                paint: None,
                ..VectorStroke::default()
            }),
            transform: AffineTransform::default(),
            effects: Box::default(),
        }],
        hyperlinks: Vec::new(),
    };
    let rendered = render(&slide, 300, 300);

    // Miter tip is ~28.8 px left of the vertex; x=145 lies inside the miter
    // wedge but outside the bevel line of a limit-4 join.
    let at_miter = pixel(&rendered, 145, 150)[0];
    assert!(
        at_miter < 40,
        "miter wedge at (145, 150) should be ink, sampled {at_miter}"
    );
    assert!(
        pixel(&rendered, 100, 150)[0] > 240,
        "background beyond the miter tip must stay white"
    );
}

// ---------------------------------------------------------------------------
// Gradient frames
// ---------------------------------------------------------------------------

#[test]
fn linear_axis_uses_the_projected_box_extent() {
    let horizontal = linear_gradient_axis(0.0, 0.0, 0.0, 300.0, 150.0);
    assert_eq!(horizontal, ((0.0, 75.0), (300.0, 75.0)));
    assert_close(gradient_axis_length(horizontal) / 2.0, 150.0, 1e-3, "horizontal half-axis");
    assert_close(
        (horizontal.0 .0 + horizontal.1 .0) / 2.0,
        150.0,
        1e-3,
        "horizontal centre x",
    );
    assert_close(
        (horizontal.0 .1 + horizontal.1 .1) / 2.0,
        75.0,
        1e-3,
        "horizontal centre y",
    );

    let vertical = linear_gradient_axis(90.0, 0.0, 0.0, 300.0, 150.0);
    assert_close((vertical.0 .0).round(), 150.0, 1e-3, "vertical start x");
    assert_close((vertical.0 .1).round(), 0.0, 1e-3, "vertical start y");
    assert_close((vertical.1 .0).round(), 150.0, 1e-3, "vertical end x");
    assert_close((vertical.1 .1).round(), 150.0, 1e-3, "vertical end y");
    assert_close(gradient_axis_length(vertical) / 2.0, 75.0, 1e-3, "vertical half-axis");

    let diagonal = linear_gradient_axis(45.0, 0.0, 0.0, 300.0, 150.0);
    let rad = 45f32.to_radians();
    let expected = (rad.cos().abs() * 300.0 + rad.sin().abs() * 150.0) / 2.0;
    assert_close(
        gradient_axis_length(diagonal) / 2.0,
        expected,
        1e-2,
        "diagonal half-axis",
    );
}

#[test]
fn linear_axis_is_framed_on_the_path_box() {
    let path = rect_path(300.0, 150.0);
    assert_eq!(path_bounds(&path), Some((0.0, 0.0, 300.0, 150.0)));
    assert_eq!(
        linear_gradient_axis_for_path(0.0, &path),
        Some(((0.0, 75.0), (300.0, 75.0)))
    );
}

#[test]
fn radial_frame_is_the_box_centre_and_half_diagonal() {
    // ref packages/core/src/shape/paint.ts:114-140 (circle/shape branch).
    let ((cx, cy), radius) = radial_gradient_frame(0.0, 0.0, 300.0, 150.0);
    assert_close(cx, 150.0, 1e-3, "radial centre x");
    assert_close(cy, 75.0, 1e-3, "radial centre y");
    assert_close(radius, 150f32.hypot(75.0), 1e-3, "radial radius");

    let path = rect_path(300.0, 150.0);
    let ((px, py), pr) = radial_gradient_frame_for_path(&path).expect("path has bounds");
    assert_close(px, cx, 1e-3, "path radial centre x");
    assert_close(py, cy, 1e-3, "path radial centre y");
    assert_close(pr, radius, 1e-3, "path radial radius");
}

#[test]
fn single_stop_gradient_paints_solid_and_empty_gradient_paints_nothing() {
    let one = compile_fill(&gradient_fill(vec![stop(0.0, "112233")]), 1.0)
        .expect("single-stop gradient is solid");
    assert_eq!(one, VectorPaint::Solid([0x11, 0x22, 0x33, 255]));

    assert_eq!(compile_fill(&gradient_fill(Vec::new()), 1.0), None);
}

#[test]
fn svg_and_raster_agree_on_linear_axis_endpoints() {
    let path = rect_path(300.0, 150.0);
    let fill = VectorPaint::LinearGradient(VectorGradient::new(
        0.0,
        vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])],
    ));
    let slide = slide_with(300, 150, path, Some(fill));
    let markup = svg(&slide);

    assert!(
        markup.contains(r#"gradientUnits="userSpaceOnUse""#),
        "gradient must use absolute user-space coordinates: {markup}"
    );
    let ((x1, y1), (x2, y2)) = linear_gradient_axis(0.0, 0.0, 0.0, 300.0, 150.0);
    let expected = format!(
        r#"x1="{}" y1="{}" x2="{}" y2="{}""#,
        x1 as i32, y1 as i32, x2 as i32, y2 as i32
    );
    assert!(markup.contains(&expected), "missing axis {expected}: {markup}");

    let rendered = render(&slide, 300, 150);
    assert!(
        pixel(&rendered, 1, 75)[0] < 6,
        "left end should be the first stop, sampled {:?}",
        pixel(&rendered, 1, 75)
    );
    assert!(
        (pixel(&rendered, 150, 75)[0] as i32 - 127).abs() <= 6,
        "centre should be half-way, sampled {:?}",
        pixel(&rendered, 150, 75)
    );
    assert!(
        pixel(&rendered, 298, 75)[0] > 249,
        "right end should be the last stop, sampled {:?}",
        pixel(&rendered, 298, 75)
    );
}

#[test]
fn svg_and_raster_agree_on_the_radial_frame() {
    let path = rect_path(300.0, 150.0);
    let fill = VectorPaint::RadialGradient(VectorGradient::new(
        0.0,
        vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])],
    ));
    let slide = slide_with(300, 150, path, Some(fill));
    let markup = svg(&slide);

    assert!(markup.contains(r#"gradientUnits="userSpaceOnUse""#), "{markup}");
    assert!(
        markup.contains(r#"cx="150" cy="75" fx="150" fy="75" r="167.71""#),
        "radial frame should be centre (150, 75), radius hypot(150, 75): {markup}"
    );

    let rendered = render(&slide, 300, 150);
    assert!(
        pixel(&rendered, 150, 75)[0] < 6,
        "focus should be the first stop, sampled {:?}",
        pixel(&rendered, 150, 75)
    );
    // Distance 75 from the centre / hypot(150, 75) = 0.4472 => ~114.
    let top = pixel(&rendered, 150, 0)[0] as i32;
    assert!(
        (top - 114).abs() <= 8,
        "expected the half-diagonal radius law near the box top, sampled {top}"
    );
}
