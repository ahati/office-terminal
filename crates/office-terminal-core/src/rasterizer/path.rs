use crate::compiler::style::gradient;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorGradient, VectorPaint, VectorPath, VectorStroke,
};

#[allow(clippy::too_many_arguments)]
pub fn render_path(
    pixmap: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    stroke: Option<&VectorStroke>,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let mut pb = tiny_skia::PathBuilder::new();

    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } => {
                let (tx, ty) = transform.transform_point(*x, *y);
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                pb.move_to(px, py);
            }
            PathSegment::LineTo { x, y } => {
                let (tx, ty) = transform.transform_point(*x, *y);
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                pb.line_to(px, py);
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (tx1, ty1) = transform.transform_point(*x1, *y1);
                let (tx, ty) = transform.transform_point(*x, *y);
                let px1 = offset_x + tx1 * scale;
                let py1 = offset_y + ty1 * scale;
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                pb.quad_to(px1, py1, px, py);
            }
            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                let (tx1, ty1) = transform.transform_point(*x1, *y1);
                let (tx2, ty2) = transform.transform_point(*x2, *y2);
                let (tx, ty) = transform.transform_point(*x, *y);
                let px1 = offset_x + tx1 * scale;
                let py1 = offset_y + ty1 * scale;
                let px2 = offset_x + tx2 * scale;
                let py2 = offset_y + ty2 * scale;
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                pb.cubic_to(px1, py1, px2, py2, px, py);
            }
            PathSegment::Close => {
                pb.close();
            }
        }
    }

    let ts_path = match pb.finish() {
        Some(p) => p,
        None => return,
    };

    let ts_fill_rule = match path.fill_rule {
        FillRule::NonZero => tiny_skia::FillRule::Winding,
        FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
    };

    // 1. Fill
    if let Some(paint_desc) = fill {
        let mut paint = tiny_skia::Paint {
            anti_alias: true,
            ..Default::default()
        };

        match paint_desc {
            VectorPaint::Solid([r, g, b, a]) => {
                paint.set_color_rgba8(*r, *g, *b, *a);
                pixmap.fill_path(&ts_path, &paint, ts_fill_rule, tiny_skia::Transform::identity(), None);
            }
            VectorPaint::LinearGradient(grad) => {
                // ref packages/core/src/shape/paint.ts:377-379: no stops paint
                // nothing, a single stop paints a solid colour.
                if grad.stops.len() == 1 {
                    fill_solid(pixmap, &ts_path, ts_fill_rule, grad.stops[0].1);
                } else if !grad.stops.is_empty() {
                    // ref paint.ts:290-300: the axis is framed on the shape box,
                    // centred, with half-length (|cos|·w + |sin|·h) / 2.
                    if let Some(shader) = linear_shader(grad, path, transform, scale, offset_x, offset_y) {
                        paint.shader = shader;
                        pixmap.fill_path(&ts_path, &paint, ts_fill_rule, tiny_skia::Transform::identity(), None);
                    }
                }
            }
            VectorPaint::RadialGradient(grad) => {
                if grad.stops.len() == 1 {
                    fill_solid(pixmap, &ts_path, ts_fill_rule, grad.stops[0].1);
                } else if !grad.stops.is_empty() {
                    // ref paint.ts:114-140: focus frame defaults to the whole
                    // shape box, so centre = box centre and the circle radius
                    // is the half-diagonal. `fillToRect`/`path` are not in the
                    // IR; the `rect` max-axis branch is therefore unavailable.
                    if let Some(shader) = radial_shader(grad, path, transform, scale, offset_x, offset_y) {
                        paint.shader = shader;
                        pixmap.fill_path(&ts_path, &paint, ts_fill_rule, tiny_skia::Transform::identity(), None);
                    }
                }
            }
            VectorPaint::Image(fill) => {
                // §20.1.8.14 shape blip: paint the prepared brush inside the
                // shape geometry (ref renderer.ts:2620-2700).
                if let Err(error) = super::image_fill::fill_path_with_image(
                    pixmap,
                    assets,
                    fill,
                    path,
                    &ts_path,
                    ts_fill_rule,
                    transform,
                    scale,
                    offset_x,
                    offset_y,
                ) {
                    let _ = error; // fail-soft: a broken brush paints nothing
                }
            }
            VectorPaint::Pattern(pattern) => {
                // §20.1.8.40 preset pattern: a bitmap cell on the 1/8 pt grid
                // (ref core/src/shape/paint.ts:459-479).
                super::image_fill::fill_path_with_pattern(
                    pixmap,
                    pattern,
                    &ts_path,
                    ts_fill_rule,
                    scale,
                    offset_x,
                    offset_y,
                );
            }
            VectorPaint::Texture(texture) => {
                super::texture_fill::fill_path_with_texture(
                    pixmap,
                    texture,
                    &ts_path,
                    ts_fill_rule,
                    transform,
                    scale,
                    offset_x,
                    offset_y,
                );
            }
        }
    }

    // 2. Stroke
    if let Some(s) = stroke {
        // ref packages/core/src/shape/stroke-geometry.ts:15 — hairline floor
        // is 0.5 px, not a full pixel.
        let width_px = (s.width_emu as f32 * scale).max(0.5);

        let line_cap = match s.line_cap {
            LineCap::Butt => tiny_skia::LineCap::Butt,
            LineCap::Round => tiny_skia::LineCap::Round,
            LineCap::Square => tiny_skia::LineCap::Square,
        };

        let line_join = match s.line_join {
            LineJoin::Miter => tiny_skia::LineJoin::Miter,
            LineJoin::Round => tiny_skia::LineJoin::Round,
            LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
        };

        let dash = s.dash_segments.as_ref().and_then(|segs| {
            let px_segs: Vec<f32> = segs.iter().map(|&seg| seg * width_px).collect();
            tiny_skia::StrokeDash::new(px_segs, 0.0)
        });

        let ts_stroke = tiny_skia::Stroke {
            width: width_px,
            // ref stroke-geometry.ts:21 — authored `a:ln@miterLim`, else 10;
            // an invalid author keeps the default.
            miter_limit: s.effective_miter_limit(),
            line_cap,
            line_join,
            dash,
        };

        stroke_with_paint(
            pixmap, s, &ts_path, &ts_stroke, path, transform, scale, offset_x, offset_y,
        );
    }
}

fn fill_solid(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    fill_rule: tiny_skia::FillRule,
    [r, g, b, a]: [u8; 4],
) {
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(r, g, b, a);
    pixmap.fill_path(path, &paint, fill_rule, tiny_skia::Transform::identity(), None);
}

fn gradient_stops(grad: &VectorGradient) -> Vec<tiny_skia::GradientStop> {
    grad.stops
        .iter()
        .map(|(pos, [r, g, b, a])| {
            // ref paint.ts:317 clamps stop positions into [0, 1].
            tiny_skia::GradientStop::new(pos.clamp(0.0, 1.0), tiny_skia::Color::from_rgba8(*r, *g, *b, *a))
        })
        .collect()
}

fn device_point(
    transform: &AffineTransform,
    x: f32,
    y: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> tiny_skia::Point {
    let (tx, ty) = transform.transform_point(x, y);
    tiny_skia::Point::from_xy(offset_x + tx * scale, offset_y + ty * scale)
}

/// Device-space linear gradient shader for the authored paint frame.
/// ref packages/core/src/shape/paint.ts:290-315
fn linear_shader(
    grad: &VectorGradient,
    path: &VectorPath,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<tiny_skia::Shader<'static>> {
    let ((x0, y0), (x1, y1)) = gradient::linear_axis_for_path(grad, path)?;
    let p0 = device_point(transform, x0, y0, scale, offset_x, offset_y);
    let p1 = device_point(transform, x1, y1, scale, offset_x, offset_y);
    tiny_skia::LinearGradient::new(
        p0,
        p1,
        gradient_stops(grad),
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    )
}

/// Device-space radial gradient shader for the authored focus frame.
/// ref packages/core/src/shape/paint.ts:114-140
fn radial_shader(
    grad: &VectorGradient,
    path: &VectorPath,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<tiny_skia::Shader<'static>> {
    let ((cx, cy), radius) = gradient::radial_frame_for_path(grad, path)?;
    let center = device_point(transform, cx, cy, scale, offset_x, offset_y);
    // tiny-skia only has circular device-space shaders, so a non-uniform
    // transform is approximated by its largest column norm (shape transforms
    // are rotations/flips).
    let radius_px = (radius * scale * transform_scale(transform)).max(1e-6);
    tiny_skia::RadialGradient::new(
        center,
        center,
        radius_px,
        gradient_stops(grad),
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    )
}

/// Paint for a stroked path. The reference replaces `strokeStyle` with the
/// complete gradient/pattern paint whenever the line authors one
/// (`applyStroke`, ref renderer.ts:7542-7565), framing it on the stroked box.
fn stroke_paint(
    stroke: &VectorStroke,
    path: &VectorPath,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    let shader = match stroke.paint.as_deref() {
        Some(VectorPaint::Solid([r, g, b, a])) => {
            paint.set_color_rgba8(*r, *g, *b, *a);
            None
        }
        Some(VectorPaint::LinearGradient(grad)) if grad.stops.len() > 1 => {
            linear_shader(grad, path, transform, scale, offset_x, offset_y)
        }
        Some(VectorPaint::RadialGradient(grad)) if grad.stops.len() > 1 => {
            radial_shader(grad, path, transform, scale, offset_x, offset_y)
        }
        // A single stop paints that solid colour; an image stroke has no
        // paint-server form in `tiny-skia`, so it keeps the flat fallback.
        Some(VectorPaint::LinearGradient(grad)) | Some(VectorPaint::RadialGradient(grad)) => match grad.stops.first() {
            Some((_, [r, g, b, a])) => {
                paint.set_color_rgba8(*r, *g, *b, *a);
                None
            }
            None => None,
        },
        _ => None,
    };
    match shader {
        Some(shader) => paint.shader = shader,
        None if stroke.paint.is_none()
            || matches!(
                stroke.paint.as_deref(),
                Some(VectorPaint::Image(_)) | Some(VectorPaint::Pattern(_)) | Some(VectorPaint::Texture(_))
            ) =>
        {
            let [r, g, b, a] = stroke.color;
            paint.set_color_rgba8(r, g, b, a);
        }
        None => {}
    }
    paint
}

/// Largest column norm of a transform's linear part.
fn transform_scale(transform: &AffineTransform) -> f32 {
    let sx = (transform.sx * transform.sx + transform.ky * transform.ky).sqrt();
    let sy = (transform.kx * transform.kx + transform.sy * transform.sy).sqrt();
    let scale = sx.max(sy);
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Stroke `ts_path` with the authored line paint, including the preset-pattern
/// brush whose cell pixmap has to outlive the shader.
#[allow(clippy::too_many_arguments)]
fn stroke_with_paint(
    pixmap: &mut tiny_skia::Pixmap,
    stroke: &VectorStroke,
    ts_path: &tiny_skia::Path,
    ts_stroke: &tiny_skia::Stroke,
    path: &VectorPath,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    if let Some(VectorPaint::Pattern(pattern)) = stroke.paint.as_deref() {
        if let Some(pixels) = crate::compiler::pattern::build_pattern_bitmap(&pattern.preset, pattern.fg, pattern.bg) {
            let edge = crate::compiler::pattern::PATTERN_BITMAP_EDGE;
            if let Some(mut cell) = tiny_skia::Pixmap::new(edge, edge) {
                super::image_fill::premultiply_into(cell.data_mut(), &pixels);
                let step = crate::contracts::PATTERN_CELL_EMU * scale;
                let mut paint = tiny_skia::Paint {
                    anti_alias: true,
                    ..Default::default()
                };
                paint.shader = tiny_skia::Pattern::new(
                    cell.as_ref(),
                    tiny_skia::SpreadMode::Repeat,
                    tiny_skia::FilterQuality::Bilinear,
                    1.0,
                    tiny_skia::Transform::from_row(step, 0.0, 0.0, step, offset_x, offset_y),
                );
                pixmap.stroke_path(ts_path, &paint, ts_stroke, tiny_skia::Transform::identity(), None);
                return;
            }
        }
    }
    let paint = stroke_paint(stroke, path, transform, scale, offset_x, offset_y);
    pixmap.stroke_path(ts_path, &paint, ts_stroke, tiny_skia::Transform::identity(), None);
}

fn update_bounds(x: f32, y: f32, min_x: &mut f32, min_y: &mut f32, max_x: &mut f32, max_y: &mut f32) {
    if x < *min_x {
        *min_x = x;
    }
    if x > *max_x {
        *max_x = x;
    }
    if y < *min_y {
        *min_y = y;
    }
    if y > *max_y {
        *max_y = y;
    }
}

/// Bounds of the path in its own local coordinate space.
pub fn local_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
    bounds_with(path, |x, y| (x, y))
}

/// Bounds of the path after applying an affine transform (control points
/// included, matching the rasterizer's conservative bounds).
pub fn transformed_bounds(path: &VectorPath, transform: &AffineTransform) -> Option<(f32, f32, f32, f32)> {
    bounds_with(path, |x, y| transform.transform_point(x, y))
}

fn bounds_with(path: &VectorPath, map: impl Fn(f32, f32) -> (f32, f32)) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;
    let mut add = |x: f32, y: f32| {
        let (tx, ty) = map(x, y);
        update_bounds(tx, ty, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
    };

    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => {
                add(*x, *y);
                any = true;
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                add(*x1, *y1);
                add(*x, *y);
                any = true;
            }
            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                add(*x1, *y1);
                add(*x2, *y2);
                add(*x, *y);
                any = true;
            }
            PathSegment::Close => {}
        }
    }

    any.then_some((min_x, min_y, max_x, max_y))
}
