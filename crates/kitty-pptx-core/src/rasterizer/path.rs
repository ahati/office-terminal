use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorPaint, VectorPath,
    VectorStroke,
};

#[allow(clippy::too_many_arguments)]
pub fn render_path(
    pixmap: &mut tiny_skia::Pixmap,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    stroke: Option<&VectorStroke>,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let mut pb = tiny_skia::PathBuilder::new();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } => {
                let (tx, ty) = transform.transform_point(*x, *y);
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                update_bounds(px, py, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                pb.move_to(px, py);
            }
            PathSegment::LineTo { x, y } => {
                let (tx, ty) = transform.transform_point(*x, *y);
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                update_bounds(px, py, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                pb.line_to(px, py);
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (tx1, ty1) = transform.transform_point(*x1, *y1);
                let (tx, ty) = transform.transform_point(*x, *y);
                let px1 = offset_x + tx1 * scale;
                let py1 = offset_y + ty1 * scale;
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                update_bounds(px1, py1, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                update_bounds(px, py, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                pb.quad_to(px1, py1, px, py);
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (tx1, ty1) = transform.transform_point(*x1, *y1);
                let (tx2, ty2) = transform.transform_point(*x2, *y2);
                let (tx, ty) = transform.transform_point(*x, *y);
                let px1 = offset_x + tx1 * scale;
                let py1 = offset_y + ty1 * scale;
                let px2 = offset_x + tx2 * scale;
                let py2 = offset_y + ty2 * scale;
                let px = offset_x + tx * scale;
                let py = offset_y + ty * scale;
                update_bounds(px1, py1, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                update_bounds(px2, py2, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                update_bounds(px, py, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
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
                pixmap.fill_path(
                    &ts_path,
                    &paint,
                    ts_fill_rule,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            VectorPaint::LinearGradient(grad) => {
                let w = (max_x - min_x).max(1.0);
                let h = (max_y - min_y).max(1.0);
                let cx = (min_x + max_x) / 2.0;
                let cy = (min_y + max_y) / 2.0;
                let l = 0.5 * (w * w + h * h).sqrt();

                let rad = grad.angle_deg.to_radians();
                let cos = rad.cos();
                let sin = rad.sin();

                let p0 = tiny_skia::Point::from_xy(cx - l * cos, cy - l * sin);
                let p1 = tiny_skia::Point::from_xy(cx + l * cos, cy + l * sin);

                let stops: Vec<tiny_skia::GradientStop> = grad
                    .stops
                    .iter()
                    .map(|(pos, [r, g, b, a])| {
                        tiny_skia::GradientStop::new(
                            *pos,
                            tiny_skia::Color::from_rgba8(*r, *g, *b, *a),
                        )
                    })
                    .collect();

                if let Some(shader) = tiny_skia::LinearGradient::new(
                    p0,
                    p1,
                    stops,
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::Transform::identity(),
                ) {
                    paint.shader = shader;
                    pixmap.fill_path(
                        &ts_path,
                        &paint,
                        ts_fill_rule,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            }
            VectorPaint::RadialGradient(grad) => {
                let w = (max_x - min_x).max(1.0);
                let h = (max_y - min_y).max(1.0);
                let cx = (min_x + max_x) / 2.0;
                let cy = (min_y + max_y) / 2.0;
                let r = 0.5 * w.max(h);

                let center = tiny_skia::Point::from_xy(cx, cy);
                let stops: Vec<tiny_skia::GradientStop> = grad
                    .stops
                    .iter()
                    .map(|(pos, [r, g, b, a])| {
                        tiny_skia::GradientStop::new(
                            *pos,
                            tiny_skia::Color::from_rgba8(*r, *g, *b, *a),
                        )
                    })
                    .collect();

                if let Some(shader) = tiny_skia::RadialGradient::new(
                    center,
                    center,
                    r,
                    stops,
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::Transform::identity(),
                ) {
                    paint.shader = shader;
                    pixmap.fill_path(
                        &ts_path,
                        &paint,
                        ts_fill_rule,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            }
        }
    }

    // 2. Stroke
    if let Some(s) = stroke {
        let mut paint = tiny_skia::Paint {
            anti_alias: true,
            ..Default::default()
        };
        let [r, g, b, a] = s.color;
        paint.set_color_rgba8(r, g, b, a);

        let width_px = (s.width_emu as f32 * scale).max(1.0); // Hairline preservation

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
            miter_limit: 4.0,
            line_cap,
            line_join,
            dash,
        };

        pixmap.stroke_path(
            &ts_path,
            &paint,
            &ts_stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

fn update_bounds(x: f32, y: f32, min_x: &mut f32, min_y: &mut f32, max_x: &mut f32, max_y: &mut f32) {
    if x < *min_x { *min_x = x; }
    if x > *max_x { *max_x = x; }
    if y < *min_y { *min_y = y; }
    if y > *max_y { *max_y = y; }
}
