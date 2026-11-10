use std::fmt::Write;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorPaint, VectorPath,
    VectorStroke,
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
        None => r#"fill="none""#.to_string(),
    }
}

pub fn format_stroke_attr(stroke: Option<&VectorStroke>) -> String {
    match stroke {
        Some(s) => {
            let [r, g, b, a] = s.color;
            let opacity = a as f32 / 255.0;
            let mut out = format!(
                r#"stroke="rgb({},{},{})" stroke-opacity="{:.2}" stroke-width="{}""#,
                r, g, b, opacity, s.width_emu
            );

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
