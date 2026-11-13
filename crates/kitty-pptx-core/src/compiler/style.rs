use crate::contracts::{
    LineCap, LineJoin, PathSegment, VectorGradient, VectorPaint, VectorPath, VectorStroke,
};

/// Synchronous-canvas ceiling kept in parity with the reference parser
/// (`MAX_DRAWINGML_CUSTOM_DASH_STOPS`).
/// ref packages/core/src/draw/dash.ts:143
const MAX_DRAWINGML_CUSTOM_DASH_STOPS: usize = 512;

/// Parse a hex color string (e.g. "RRGGBB", "#RRGGBB", "RRGGBBAA", "#RRGGBBAA").
/// Defaults to magenta `[255, 0, 255, 255]` on error.
pub fn parse_hex_color(hex: &str) -> [u8; 4] {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
        [r, g, b, 255]
    } else if clean.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
        let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
        [r, g, b, a]
    } else {
        [255, 0, 255, 255]
    }
}

/// Apply opacity multiplier to an RGBA color.
pub fn modulate_alpha(mut color: [u8; 4], alpha: f32) -> [u8; 4] {
    let a = (color[3] as f32 * alpha.clamp(0.0, 1.0)).round() as u8;
    color[3] = a;
    color
}

/// Convert a pptx_model::Fill to VectorPaint.
pub fn compile_fill(fill: &pptx_model::Fill, alpha: f32) -> Option<VectorPaint> {
    match fill {
        pptx_model::Fill::Solid { color } => {
            let c = modulate_alpha(parse_hex_color(color), alpha);
            Some(VectorPaint::Solid(c))
        }
        pptx_model::Fill::Gradient {
            stops,
            angle,
            grad_type,
            ..
        } => {
            // ref packages/core/src/shape/paint.ts:377-379: a gradient with no
            // stops paints nothing; a single stop paints that solid colour.
            if stops.is_empty() {
                return None;
            }
            if stops.len() == 1 {
                let color = modulate_alpha(parse_hex_color(&stops[0].color), alpha);
                return Some(VectorPaint::Solid(color));
            }

            let gradient_stops: Vec<(f32, [u8; 4])> = stops
                .iter()
                .map(|stop| {
                    let col = modulate_alpha(parse_hex_color(&stop.color), alpha);
                    (stop.position as f32, col)
                })
                .collect();

            // IR limitation: `VectorGradient` carries only angle + stops, so
            // the authored `scaled`, `path`, `fillToRect`, `tileRect`, `flip`
            // and `rotWithShape` options are dropped here and cannot reach the
            // renderers. The linear/radial frames below are the untiled,
            // full-box, shape-rotating case. ref paint.ts:283-315, 114-140.
            let grad = VectorGradient {
                angle_deg: *angle as f32,
                stops: gradient_stops,
            };

            if grad_type == "radial" {
                Some(VectorPaint::RadialGradient(grad))
            } else {
                Some(VectorPaint::LinearGradient(grad))
            }
        }
        pptx_model::Fill::Pattern { fg, .. } => {
            // Degraded fallback: solid fill in pattern primary foreground color
            let c = modulate_alpha(parse_hex_color(fg), alpha);
            Some(VectorPaint::Solid(c))
        }
        pptx_model::Fill::None => None,
        pptx_model::Fill::Image { .. } => {
            // Image fills are handled as pictures or background images
            None
        }
    }
}

/// Convert a pptx_model::Stroke to VectorStroke.
pub fn compile_stroke(stroke: &pptx_model::Stroke) -> Option<VectorStroke> {
    let color = parse_hex_color(&stroke.color);
    let width_emu = stroke.width;
    let dash_segments = compile_dash_segments(stroke);

    // ref packages/core/src/shape/stroke-geometry.ts:19 — the DrawingML
    // default cap is butt, not square.
    let mut line_cap = match stroke.line_cap.as_deref() {
        Some("rnd") | Some("round") => LineCap::Round,
        Some("sq") | Some("square") => LineCap::Square,
        Some("flat") | Some("butt") | None => LineCap::Butt,
        _ => LineCap::Butt,
    };

    // ref stroke-geometry.ts:20-21 — a zero-length on-segment is painted with a
    // square cap even when the authored cap is butt (Canvas/`tiny-skia` drop it
    // otherwise). Only even indices are on-segments.
    if line_cap == LineCap::Butt && has_zero_on_segment(dash_segments.as_deref()) {
        line_cap = LineCap::Square;
    }

    let line_join = match stroke.line_join.as_deref() {
        Some("round") => LineJoin::Round,
        Some("bevel") => LineJoin::Bevel,
        Some("miter") => LineJoin::Miter,
        _ => LineJoin::Miter,
    };

    Some(VectorStroke {
        color,
        width_emu,
        dash_segments,
        line_cap,
        line_join,
    })
}

/// Authored custom dash wins over the preset; unknown styles fall through to
/// the VML numeric form and finally to a continuous stroke.
/// ref packages/core/src/draw/dash.ts:158-228
fn compile_dash_segments(stroke: &pptx_model::Stroke) -> Option<Vec<f32>> {
    if !stroke.custom_dash.is_empty() {
        let mut segments = Vec::new();
        for seg in stroke
            .custom_dash
            .iter()
            .take(MAX_DRAWINGML_CUSTOM_DASH_STOPS)
        {
            let dash = seg.dash as f32;
            let space = seg.space as f32;
            if !dash.is_finite()
                || !space.is_finite()
                || dash < 0.0
                || space < 0.0
                || (dash == 0.0 && space == 0.0)
            {
                continue;
            }
            segments.push(dash);
            segments.push(space);
        }
        return (!segments.is_empty()).then_some(segments);
    }

    stroke.dash_style.as_deref().and_then(preset_dash_segments)
}

/// §20.1.10.49 ST_PresetLineDashVal relative patterns (line-width units).
/// ref packages/core/src/draw/dash.ts:146-157
fn preset_dash_segments(style: &str) -> Option<Vec<f32>> {
    let relative: &[f32] = match style {
        "dash" => &[6.0, 3.0],
        "dot" => &[1.5, 3.0],
        "dashDot" => &[6.0, 3.0, 1.5, 3.0],
        "lgDash" => &[10.0, 4.0],
        "lgDashDot" => &[10.0, 4.0, 1.5, 4.0],
        "lgDashDotDot" => &[10.0, 4.0, 1.5, 4.0, 1.5, 4.0],
        "sysDash" => &[4.0, 2.0],
        "sysDot" => &[1.0, 2.0],
        "sysDashDot" => &[4.0, 2.0, 1.0, 2.0],
        "sysDashDotDot" => &[4.0, 2.0, 1.0, 2.0, 1.0, 2.0],
        _ => &[],
    };
    if !relative.is_empty() {
        return Some(relative.to_vec());
    }

    // VML legacy numeric dashstyle (Part 4 §19.1.2.21): whitespace/comma
    // separated line-width multiples; an unmatched final value is dropped.
    // ref packages/core/src/draw/dash.ts:218-228
    let mut numeric: Vec<f32> = style
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .map(|token| token.parse::<f32>().unwrap_or(f32::NAN))
        .collect();
    if numeric.len() >= 2 && numeric.iter().all(|v| v.is_finite() && *v >= 0.0) {
        // JS `Number('')` is 0; the reference only rejects NaN/infinite values.
        if numeric.iter().any(|v| *v > 0.0) {
            if !numeric.len().is_multiple_of(2) {
                numeric.pop();
            }
            return Some(numeric);
        }
    }
    None
}

fn has_zero_on_segment(dash: Option<&[f32]>) -> bool {
    dash.is_some_and(|segs| {
        segs.iter()
            .enumerate()
            .any(|(index, length)| index % 2 == 0 && *length == 0.0)
    })
}


/// Local-space bounds of a path (control points included, matching the
/// rasterizer's conservative bounds). Used as the shape box for gradient
/// framing: the IR does not carry the authored shape rect, and a preset path
/// normally spans `(0, 0)..(width, height)` in local coordinates.
pub fn path_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;

    for seg in &path.segments {
        let mut add = |x: f32, y: f32| {
            if x < min_x { min_x = x; }
            if x > max_x { max_x = x; }
            if y < min_y { min_y = y; }
            if y > max_y { max_y = y; }
        };
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

/// Axis endpoints of a DrawingML linear gradient for the fill box
/// `(x, y, w, h)` in shape-local coordinates.
///
/// The reference centres the axis on the box and uses half the sum of the
/// axis-aligned projections of the box edges: `(|cos|·w + |sin|·h) / 2`.
/// ref packages/core/src/shape/paint.ts:290-300
pub fn linear_gradient_axis(
    angle_deg: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> ((f32, f32), (f32, f32)) {
    let rad = angle_deg.to_radians();
    let dx = rad.cos();
    let dy = rad.sin();
    let half = (dx.abs() * w + dy.abs() * h) / 2.0;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    (
        (cx - dx * half, cy - dy * half),
        (cx + dx * half, cy + dy * half),
    )
}

/// [`linear_gradient_axis`] framed on a path's local bounds.
pub fn linear_gradient_axis_for_path(
    angle_deg: f32,
    path: &VectorPath,
) -> Option<((f32, f32), (f32, f32))> {
    let (min_x, min_y, max_x, max_y) = path_bounds(path)?;
    Some(linear_gradient_axis(
        angle_deg,
        min_x,
        min_y,
        max_x - min_x,
        max_y - min_y,
    ))
}

/// Centre and radius of a DrawingML radial (`path="circle"`/`shape`) gradient
/// for the fill box `(x, y, w, h)` in shape-local coordinates.
///
/// With no `fillToRect` the focus frame is the whole box, so the centre is the
/// box centre and the radius is the half-diagonal (`hypot(rx, ry)`), matching
/// the reference's `circle`/`shape` branch. The reference's `path="rect"`
/// branch (`max(rx, ry)`) and authored focus/tile rectangles are unreachable:
/// `VectorGradient` carries neither. ref packages/core/src/shape/paint.ts:114-140
pub fn radial_gradient_frame(x: f32, y: f32, w: f32, h: f32) -> ((f32, f32), f32) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let rx = (cx - x).abs().max((x + w - cx).abs());
    let ry = (cy - y).abs().max((y + h - cy).abs());
    ((cx, cy), (rx * rx + ry * ry).sqrt())
}

/// [`radial_gradient_frame`] framed on a path's local bounds.
pub fn radial_gradient_frame_for_path(path: &VectorPath) -> Option<((f32, f32), f32)> {
    let (min_x, min_y, max_x, max_y) = path_bounds(path)?;
    Some(radial_gradient_frame(
        min_x,
        min_y,
        max_x - min_x,
        max_y - min_y,
    ))
}
