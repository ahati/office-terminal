//! SVG def emission and command bookkeeping split out of `svg/mod.rs`: paint
//! servers (gradient / image / pattern fills and `a:ln` paints), clip paths,
//! and the bounds used for hyperlink hit testing.

use std::fmt::Write;

use super::{image, path};
use crate::contracts::{
    sanitize_hyperlink_url, AffineTransform, FillRule, HyperlinkTarget, PathSegment, RectF32,
    SlideLink, VectorCommand, VectorPaint, VectorPath, VectorStroke,
};
use crate::loader::AssetStore;

/// Emit the `fill` paint server of one command into `defs` and return its id.
/// Linear/radial gradients frame on the painted path, an `a:blipFill` on the
/// path's box and an `a:pattFill` on the 1/8 pt cell grid
/// (ref renderer.ts:2620-2700, paint.ts:459-479).
pub(super) fn emit_fill_def(
    defs: &mut String,
    counter: &mut usize,
    assets: &dyn AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
) -> Option<String> {
    match fill? {
        VectorPaint::LinearGradient(grad) => {
            *counter += 1;
            let id = format!("grad_{}", counter);
            path::emit_linear_gradient_def(defs, &id, grad, path);
            Some(id)
        }
        VectorPaint::RadialGradient(grad) => {
            *counter += 1;
            let id = format!("grad_{}", counter);
            path::emit_radial_gradient_def(defs, &id, grad, path);
            Some(id)
        }
        VectorPaint::Image(image_fill) => {
            *counter += 1;
            let id = format!("imgfill_{}", counter);
            let fill_box = path::path_bounds(path)
                .map(|(x0, y0, x1, y1)| (x0, y0, x1 - x0, y1 - y0));
            match fill_box {
                Some(fill_box)
                    if image::emit_image_fill_def(assets, defs, &id, image_fill, fill_box) =>
                {
                    Some(id)
                }
                _ => None,
            }
        }
        VectorPaint::Pattern(pattern) => {
            *counter += 1;
            let id = format!("pattfill_{}", counter);
            image::emit_pattern_fill_def(defs, &id, pattern);
            Some(id)
        }
        VectorPaint::Solid(_) => None,
    }
}

/// `a:ln` paints its own gradient/pattern, which replaces the flat stroke colour
/// (ref renderer.ts:7542-7565).
pub(super) fn emit_stroke_def(
    defs: &mut String,
    counter: &mut usize,
    path: &VectorPath,
    stroke: Option<&VectorStroke>,
) -> Option<String> {
    match stroke?.paint.as_deref()? {
        VectorPaint::LinearGradient(grad) => {
            *counter += 1;
            let id = format!("sgrad_{}", counter);
            path::emit_linear_gradient_def(defs, &id, grad, path);
            Some(id)
        }
        VectorPaint::RadialGradient(grad) => {
            *counter += 1;
            let id = format!("sgrad_{}", counter);
            path::emit_radial_gradient_def(defs, &id, grad, path);
            Some(id)
        }
        VectorPaint::Pattern(pattern) => {
            *counter += 1;
            let id = format!("spattfill_{}", counter);
            image::emit_pattern_fill_def(defs, &id, pattern);
            Some(id)
        }
        VectorPaint::Solid(_) | VectorPaint::Image(_) => None,
    }
}

/// Bounds of a path after its command transform, control points included
/// (matching the rasterizer's conservative bounds).
pub(super) fn transformed_path_bounds(
    path: &VectorPath,
    transform: &AffineTransform,
) -> Option<RectF32> {
    let mut bounds: Option<RectF32> = None;
    for segment in &path.segments {
        let points: &[(f32, f32)] = match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => &[(*x, *y)],
            PathSegment::QuadTo { x1, y1, x, y } => &[(*x1, *y1), (*x, *y)],
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => &[(*x1, *y1), (*x2, *y2), (*x, *y)],
            PathSegment::Close => &[],
        };
        for &(x, y) in points {
            let (tx, ty) = transform.transform_point(x, y);
            include_point(&mut bounds, tx, ty);
        }
    }
    bounds
}

pub(super) fn emit_clip_path_def(defs: &mut String, id: &str, path: &crate::contracts::VectorPath) {
    let d = path::path_to_svg_d(path);
    let rule = match path.fill_rule {
        FillRule::NonZero => r#" clip-rule="nonzero""#,
        FillRule::EvenOdd => r#" clip-rule="evenodd""#,
    };
    let _ = writeln!(
        defs,
        r#"    <clipPath id="{}" clipPathUnits="userSpaceOnUse">"#,
        id
    );
    let _ = writeln!(defs, r#"      <path d="{}"{}/>"#, d, rule);
    let _ = writeln!(defs, "    </clipPath>");
}

pub(super) fn find_command_link<'a>(links: &'a [SlideLink], command: &VectorCommand) -> Option<&'a SlideLink> {
    let bounds = command_bounds(command)?;
    let mut best: Option<(&SlideLink, f32)> = None;
    for link in links {
        if !matches!(
            link.target,
            HyperlinkTarget::ExternalUri(_) | HyperlinkTarget::SlideIndex(_)
        ) {
            continue;
        }
        let overlap = link.bounds_emu.intersection_area(&bounds);
        if overlap > 0.0 && best.is_none_or(|(_, area)| overlap > area) {
            best = Some((link, overlap));
        }
    }
    best.map(|(link, _)| link)
}

pub(super) fn command_bounds(command: &VectorCommand) -> Option<crate::contracts::RectF32> {
    match command {
        VectorCommand::DrawPath {
            path, transform, ..
        } => transformed_path_bounds(path, transform),
        VectorCommand::DrawLineEnd {
            path, transform, ..
        } => transformed_path_bounds(path, transform),
        VectorCommand::DrawImage {
            dest_rect,
            transform,
            ..
        } => Some(transform_rect(dest_rect, transform)),
        VectorCommand::DrawText { runs, transform } => {
            let mut bounds: Option<crate::contracts::RectF32> = None;
            for run in runs {
                let font_emu = run.font_size_pt * 12700.0;
                for glyph in &run.glyphs {
                    let (tx, ty) = transform.transform_point(glyph.x, glyph.y - font_emu);
                    include_point(&mut bounds, tx, ty);
                    let (tx, ty) = transform.transform_point(glyph.x + glyph.advance, glyph.y);
                    include_point(&mut bounds, tx, ty);
                }
            }
            bounds
        }
        VectorCommand::DrawBitmap { placement, .. } => Some(transform_rect(
            &crate::contracts::RectF32 {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            placement,
        )),
        VectorCommand::DrawMetafile {
            dest_rect,
            transform,
            ..
        } => Some(transform_rect(dest_rect, transform)),
        VectorCommand::PushClip(_) | VectorCommand::PopClip => None,
    }
}

pub(super) fn include_point(bounds: &mut Option<crate::contracts::RectF32>, x: f32, y: f32) {
    match bounds {
        Some(rect) => {
            let min_x = rect.x.min(x);
            let min_y = rect.y.min(y);
            let max_x = rect.right().max(x);
            let max_y = rect.bottom().max(y);
            *rect = crate::contracts::RectF32::new(min_x, min_y, max_x - min_x, max_y - min_y);
        }
        None => {
            *bounds = Some(crate::contracts::RectF32::new(x, y, 0.0, 0.0));
        }
    }
}

fn transform_rect(
    rect: &crate::contracts::RectF32,
    transform: &crate::contracts::AffineTransform,
) -> crate::contracts::RectF32 {
    let corners = [
        (rect.x, rect.y),
        (rect.right(), rect.y),
        (rect.right(), rect.bottom()),
        (rect.x, rect.bottom()),
    ];
    let mut bounds = None;
    for (x, y) in corners {
        let (tx, ty) = transform.transform_point(x, y);
        include_point(&mut bounds, tx, ty);
    }
    bounds.unwrap_or(*rect)
}

pub(super) fn wrap_svg_anchor(element: &str, link: &SlideLink) -> String {
    let (href, target, rel) = match &link.target {
        HyperlinkTarget::ExternalUri(url) => {
            let Ok(safe_url) = sanitize_hyperlink_url(url) else {
                return element.to_string();
            };
            (safe_url, "_blank", " rel=\"noopener noreferrer\"")
        }
        HyperlinkTarget::SlideIndex(index) => {
            (format!("#slide-{}", index + 1), "_self", "")
        }
        HyperlinkTarget::Action(_) => return element.to_string(),
    };
    let href = xml_escape_attr(&href);
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"  <a xlink:href="{href}" href="{href}" target="{target}"{rel}>"#
    );
    if let Some(tooltip) = &link.tooltip {
        let _ = writeln!(out, "    <title>{}</title>", xml_escape_attr(tooltip));
    }
    out.push_str(element);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    let _ = writeln!(out, "  </a>");
    out
}

pub(super) fn xml_escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
