use std::fmt::Write;
use crate::compiler::style::gradient;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorGradient, VectorPaint,
    VectorPath, VectorStroke,
};

pub fn path_to_svg_d(path: &VectorPath) -> String {
    let mut d = String::new();
    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } => {
                let _ = write!(&mut d, "M {} {} ", round_coord(*x), round_coord(*y));
            }
            PathSegment::LineTo { x, y } => {
                let _ = write!(&mut d, "L {} {} ", round_coord(*x), round_coord(*y));
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let _ = write!(
                    &mut d,
                    "Q {} {}, {} {} ",
                    round_coord(*x1),
                    round_coord(*y1),
                    round_coord(*x),
                    round_coord(*y)
                );
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let _ = write!(
                    &mut d,
                    "C {} {}, {} {}, {} {} ",
                    round_coord(*x1),
                    round_coord(*y1),
                    round_coord(*x2),
                    round_coord(*y2),
                    round_coord(*x),
                    round_coord(*y)
                );
            }
            PathSegment::Close => {
                d.push_str("Z ");
            }
        }
    }
    d.trim().to_string()
}

/// Local-space bounds of a path (control points included), matching the
/// rasterizer's conservative bounds. Used as the fill box for image/pattern
/// paint servers.
pub fn path_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;
    let mut add = |x: f32, y: f32| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        any = true;
    };
    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => add(*x, *y),
            PathSegment::QuadTo { x1, y1, x, y } => {
                add(*x1, *y1);
                add(*x, *y);
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                add(*x1, *y1);
                add(*x2, *y2);
                add(*x, *y);
            }
            PathSegment::Close => {}
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

pub fn format_fill_attr(fill: Option<&VectorPaint>, gradient_id: Option<&str>) -> String {
    match fill {
        Some(VectorPaint::Solid([r, g, b, a])) => {
            format!(
                r#"fill="rgb({},{},{})" fill-opacity="{:.2}""#,
                r,
                g,
                b,
                *a as f32 / 255.0
            )
        }
        Some(VectorPaint::LinearGradient(_)) | Some(VectorPaint::RadialGradient(_)) => {
            if let Some(id) = gradient_id {
                format!(r#"fill="url(#{})""#, id)
            } else {
                r##"fill="#888888""##.to_string()
            }
        }
        // §20.1.8.14 / §20.1.8.40 brushes: both reference a paint-server def
        // (`<pattern>`) collected with the gradients; without one the path keeps
        // an unpainted fill (ref renderer.ts:2620-2700, paint.ts:459-479).
        Some(VectorPaint::Image(_)) | Some(VectorPaint::Pattern(_)) => match gradient_id {
            Some(id) => format!(r#"fill="url(#{})""#, id),
            None => r#"fill="none""#.to_string(),
        },
        None => r#"fill="none""#.to_string(),
    }
}

/// Stroke attributes. `paint_id` is the collected paint server for the line's
/// own `a:ln` gradient/pattern fill, which replaces the flat colour entirely
/// (ref renderer.ts:7542-7565 `applyStroke`).
pub fn format_stroke_attr(stroke: Option<&VectorStroke>, paint_id: Option<&str>) -> String {
    match stroke {
        Some(s) => {
            // A single-stop gradient resolved to a solid paint replaces the
            // authored line colour (ref core/src/shape/paint.ts:377-379).
            let [r, g, b, a] = match s.paint.as_deref() {
                Some(VectorPaint::Solid(color)) => *color,
                _ => s.color,
            };
            let opacity = a as f32 / 255.0;
            let mut out = match paint_id {
                Some(id) => format!(
                    r#"stroke="url(#{})" stroke-opacity="{:.2}" stroke-width="{}""#,
                    id, opacity, s.width_emu
                ),
                None => format!(
                    r#"stroke="rgb({},{},{})" stroke-opacity="{:.2}" stroke-width="{}""#,
                    r, g, b, opacity, s.width_emu
                ),
            };

            match s.line_cap {
                LineCap::Butt => out.push_str(r#" stroke-linecap="butt""#),
                LineCap::Round => out.push_str(r#" stroke-linecap="round""#),
                LineCap::Square => out.push_str(r#" stroke-linecap="square""#),
            }

            match s.line_join {
                LineJoin::Miter => out.push_str(r#" stroke-linejoin="miter""#),
                LineJoin::Round => out.push_str(r#" stroke-linejoin="round""#),
                LineJoin::Bevel => out.push_str(r#" stroke-linejoin="bevel""#),
            }

            // SVG defaults to 4; the renderer uses the DrawingML default 10
            // (authored limits are not in the IR). ref stroke-geometry.ts:22
            out.push_str(r#" stroke-miterlimit="10""#);

            if let Some(ref dash) = s.dash_segments {
                let dash_str: Vec<String> = dash
                    .iter()
                    .map(|v| format!("{:.1}", v * (s.width_emu as f32)))
                    .collect();
                out.push_str(&format!(r#" stroke-dasharray="{}""#, dash_str.join(",")));
            }

            out
        }
        None => r#"stroke="none""#.to_string(),
    }
}

pub fn format_transform_attr(transform: &AffineTransform) -> String {
    if (transform.sx - 1.0).abs() < 1e-4
        && transform.ky.abs() < 1e-4
        && transform.kx.abs() < 1e-4
        && (transform.sy - 1.0).abs() < 1e-4
    {
        if transform.tx.abs() < 1e-4 && transform.ty.abs() < 1e-4 {
            String::new()
        } else {
            format!(
                r#" transform="translate({}, {})""#,
                round_coord(transform.tx),
                round_coord(transform.ty)
            )
        }
    } else {
        format!(
            r#" transform="matrix({:.4} {:.4} {:.4} {:.4} {} {})""#,
            transform.sx,
            transform.ky,
            transform.kx,
            transform.sy,
            round_coord(transform.tx),
            round_coord(transform.ty)
        )
    }
}

pub fn format_fill_rule_attr(rule: FillRule) -> &'static str {
    match rule {
        FillRule::NonZero => r#" fill-rule="nonzero""#,
        FillRule::EvenOdd => r#" fill-rule="evenodd""#,
    }
}

fn round_coord(v: f32) -> String {
    if (v - v.round()).abs() < 1e-3 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}

/// Emit a linear gradient definition in the path's local user space using the
/// same frame math as the rasterizer.
/// ref packages/core/src/shape/paint.ts:290-300
pub fn emit_linear_gradient_def(
    defs: &mut String,
    id: &str,
    grad: &VectorGradient,
    path: &VectorPath,
) {
    if let Some(((x1, y1), (x2, y2))) = gradient::linear_axis_for_path(grad, path) {
        emit_linear_gradient_range(defs, id, grad, x1, y1, x2, y2);
    }
}

/// [`emit_linear_gradient_def`] for a known fill box (used by the slide
/// background, where the box is `0..slide_width` × `0..slide_height`).
pub fn emit_linear_gradient_def_for_box(
    defs: &mut String,
    id: &str,
    grad: &VectorGradient,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let ((x1, y1), (x2, y2)) = gradient::linear_axis(
        grad,
        gradient::GradientBox { x, y, w, h },
    );
    emit_linear_gradient_range(defs, id, grad, x1, y1, x2, y2);
}

fn emit_linear_gradient_range(
    defs: &mut String,
    id: &str,
    grad: &VectorGradient,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) {
    let _ = writeln!(
        defs,
        r#"    <linearGradient id="{}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}">"#,
        id,
        round_coord(x1),
        round_coord(y1),
        round_coord(x2),
        round_coord(y2)
    );
    emit_gradient_stops(defs, grad);
    let _ = writeln!(defs, "    </linearGradient>");
}

/// Emit a radial gradient definition in the path's local user space.
/// ref packages/core/src/shape/paint.ts:114-140
pub fn emit_radial_gradient_def(
    defs: &mut String,
    id: &str,
    grad: &VectorGradient,
    path: &VectorPath,
) {
    let Some(((cx, cy), radius)) = gradient::radial_frame_for_path(grad, path) else {
        return;
    };
    let cx = round_coord(cx);
    let cy = round_coord(cy);
    let _ = writeln!(
        defs,
        r#"    <radialGradient id="{}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" fx="{}" fy="{}" r="{}">"#,
        id,
        cx,
        cy,
        cx,
        cy,
        round_coord(radius)
    );
    emit_gradient_stops(defs, grad);
    let _ = writeln!(defs, "    </radialGradient>");
}

fn emit_gradient_stops(defs: &mut String, grad: &VectorGradient) {
    for (pos, [r, g, b, a]) in &grad.stops {
        let opacity = *a as f32 / 255.0;
        // ref paint.ts:317 clamps stop positions into [0, 1].
        let _ = writeln!(
            defs,
            r#"      <stop offset="{}%" stop-color="rgb({},{},{})" stop-opacity="{:.2}"/>"#,
            round_coord(pos.clamp(0.0, 1.0) * 100.0),
            r,
            g,
            b,
            opacity
        );
    }
}
