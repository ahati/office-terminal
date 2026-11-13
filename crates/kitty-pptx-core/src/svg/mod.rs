mod background_image;
mod filters;
mod image;
mod path;
mod text;

use std::fmt::Write;
use crate::contracts::{
    sanitize_hyperlink_url, FillRule, HyperlinkTarget, PathSegment, SlideLink, SvgTextMode,
    VectorBackground, VectorCommand, VectorPaint, VectorSlide,
};
use crate::error::RenderError;
use crate::loader::AssetStore;

pub trait SlideSvgSerializer: Send + Sync {
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        assets: &dyn AssetStore,
        text_mode: SvgTextMode,
    ) -> Result<String, RenderError>;
}

pub struct DefaultSlideSvgSerializer;

impl DefaultSlideSvgSerializer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultSlideSvgSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl SlideSvgSerializer for DefaultSlideSvgSerializer {
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        assets: &dyn AssetStore,
        text_mode: SvgTextMode,
    ) -> Result<String, RenderError> {
        let mut svg = String::new();
        let w = scene.width_emu;
        let h = scene.height_emu;

        writeln!(&mut svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
        if scene.hyperlinks.is_empty() {
            writeln!(
                &mut svg,
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">"#,
                w, h
            )
            .unwrap();
        } else {
            writeln!(
                &mut svg,
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 {} {}" width="100%" height="100%">"#,
                w, h
            )
            .unwrap();
        }

        // 1. Defs section for gradients and clip paths
        let mut defs = String::new();
        let mut grad_id_counter = 0usize;
        let mut path_grad_ids: Vec<Option<String>> = Vec::new();

        // Background gradient
        let bg_grad_id = match scene.background {
            Some(VectorBackground::Gradient(ref grad)) => {
                let id = "bg_grad".to_string();
                path::emit_linear_gradient_def_for_box(
                    &mut defs,
                    &id,
                    grad,
                    0.0,
                    0.0,
                    scene.width_emu as f32,
                    scene.height_emu as f32,
                );
                Some(id)
            }
            _ => None,
        };

        // Image background (§20.1.8.14): white base plus the stretch/tile
        // body. A tiled fill registers its `<pattern>` in the shared defs
        // buffer (ref renderer.ts:2218-2362).
        let bg_image_body = scene.background.as_ref().and_then(|bg| {
            background_image::emit(
                assets,
                bg,
                scene.width_emu as f32,
                scene.height_emu as f32,
                &mut defs,
            )
        });

        // Command gradients, clip defs, and shape-effect filter graphs
        let mut effect_refs: Vec<Option<filters::EffectRefs>> = Vec::new();
        let mut draw_path_index = 0usize;
        let mut clip_ids: Vec<Option<String>> = Vec::with_capacity(scene.commands.len());
        let mut stroke_paint_ids: Vec<Option<String>> = Vec::with_capacity(scene.commands.len());
        let mut clip_counter = 0usize;
        for cmd in &scene.commands {
            // Picture silhouettes become `<clipPath>` defs, one per PushClip
            // (ref renderer.ts:6985-7036 builds the preset/custom silhouette
            // and calls ctx.clip() before painting the bitmap).
            clip_ids.push(match cmd {
                VectorCommand::PushClip(clip_path) if !clip_path.segments.is_empty() => {
                    clip_counter += 1;
                    let id = format!("clip_{}", clip_counter);
                    emit_clip_path_def(&mut defs, &id, clip_path);
                    Some(id)
                }
                _ => None,
            });

            if let VectorCommand::DrawPath {
                path: v_path,
                fill,
                stroke,
                transform,
                effects,
            } = cmd
            {
                let refs = filters::emit_effect_defs(
                    draw_path_index,
                    v_path,
                    transform,
                    effects,
                    scene.width_emu,
                    &mut defs,
                );
                effect_refs.push(refs);
                draw_path_index += 1;

                match fill {
                    Some(VectorPaint::LinearGradient(ref grad)) => {
                        grad_id_counter += 1;
                        let id = format!("grad_{}", grad_id_counter);
                        path::emit_linear_gradient_def(&mut defs, &id, grad, v_path);
                        path_grad_ids.push(Some(id));
                    }
                    Some(VectorPaint::RadialGradient(ref grad)) => {
                        grad_id_counter += 1;
                        let id = format!("grad_{}", grad_id_counter);
                        path::emit_radial_gradient_def(&mut defs, &id, grad, v_path);
                        path_grad_ids.push(Some(id));
                    }
                    Some(VectorPaint::Image(ref image_fill)) => {
                        grad_id_counter += 1;
                        let id = format!("imgfill_{}", grad_id_counter);
                        let box_ = path::path_bounds(v_path)
                            .map(|(x0, y0, x1, y1)| (x0, y0, x1 - x0, y1 - y0));
                        match box_ {
                            Some(box_) if image::emit_image_fill_def(
                                assets, &mut defs, &id, image_fill, box_,
                            ) => path_grad_ids.push(Some(id)),
                            _ => path_grad_ids.push(None),
                        }
                    }
                    Some(VectorPaint::Pattern(ref pattern)) => {
                        grad_id_counter += 1;
                        let id = format!("pattfill_{}", grad_id_counter);
                        image::emit_pattern_fill_def(&mut defs, &id, pattern);
                        path_grad_ids.push(Some(id));
                    }
                    _ => path_grad_ids.push(None),
                }

                // `a:ln` paints its own gradient/pattern, which replaces the
                // flat stroke colour (ref renderer.ts:7542-7565).
                stroke_paint_ids.push(match stroke.as_ref().and_then(|s| s.paint.as_deref()) {
                    Some(VectorPaint::LinearGradient(ref grad)) => {
                        grad_id_counter += 1;
                        let id = format!("sgrad_{}", grad_id_counter);
                        path::emit_linear_gradient_def(&mut defs, &id, grad, v_path);
                        Some(id)
                    }
                    Some(VectorPaint::RadialGradient(ref grad)) => {
                        grad_id_counter += 1;
                        let id = format!("sgrad_{}", grad_id_counter);
                        path::emit_radial_gradient_def(&mut defs, &id, grad, v_path);
                        Some(id)
                    }
                    Some(VectorPaint::Pattern(ref pattern)) => {
                        grad_id_counter += 1;
                        let id = format!("spattfill_{}", grad_id_counter);
                        image::emit_pattern_fill_def(&mut defs, &id, pattern);
                        Some(id)
                    }
                    _ => None,
                });
            } else {
                effect_refs.push(None);
                path_grad_ids.push(None);
                stroke_paint_ids.push(None);
            }
        }

        if !defs.is_empty() {
            writeln!(&mut svg, "  <defs>").unwrap();
            svg.push_str(&defs);
            writeln!(&mut svg, "  </defs>").unwrap();
        }

        // 2. Background rect
        if let Some(ref bg) = scene.background {
            match bg {
                VectorBackground::Solid([r, g, b, a]) => {
                    let opacity = *a as f32 / 255.0;
                    writeln!(
                        &mut svg,
                        r#"  <rect width="{}" height="{}" fill="rgb({},{},{})" fill-opacity="{:.2}"/>"#,
                        w, h, r, g, b, opacity
                    )
                    .unwrap();
                }
                VectorBackground::Gradient(_) => {
                    if let Some(ref id) = bg_grad_id {
                        writeln!(
                            &mut svg,
                            r#"  <rect width="{}" height="{}" fill="url(#{})"/>"#,
                            w, h, id
                        )
                        .unwrap();
                    }
                }
                VectorBackground::Image { .. } => {
                    if let Some(ref body) = bg_image_body {
                        svg.push_str(body);
                    }
                }
            }
        } else {
            writeln!(
                &mut svg,
                r##"  <rect width="{}" height="{}" fill="#ffffff"/>"##,
                w, h
            )
            .unwrap();
        }

        // 3. Render vector commands. A PushClip opens a `<g clip-path>` group;
        //    nested pushes nest groups so the regions intersect exactly like
        //    the reference's save()/clip() pairs. PopClip closes the innermost
        //    group; unmatched or empty clips are simply skipped.
        let mut open_clips: Vec<bool> = Vec::new();
        for (cmd_idx, cmd) in scene.commands.iter().enumerate() {
            match cmd {
                VectorCommand::PushClip(_) => {
                    match clip_ids.get(cmd_idx).and_then(|id| id.as_deref()) {
                        Some(id) => {
                            let _ = writeln!(&mut svg, r#"  <g clip-path="url(#{})">"#, id);
                            open_clips.push(true);
                        }
                        None => open_clips.push(false),
                    }
                    continue;
                }
                VectorCommand::PopClip => {
                    if open_clips.pop().unwrap_or(false) {
                        let _ = writeln!(&mut svg, "  </g>");
                    }
                    continue;
                }
                _ => {}
            }

            let mut element = String::new();
            match cmd {
                VectorCommand::DrawPath {
                    path: v_path,
                    fill,
                    stroke,
                    transform,
                    effects,
                } => {
                    let d = path::path_to_svg_d(v_path);
                    let grad_id = path_grad_ids.get(cmd_idx).and_then(|id| id.as_deref());
                    let fill_attr = path::format_fill_attr(fill.as_ref(), grad_id);
                    let stroke_id = stroke_paint_ids.get(cmd_idx).and_then(|id| id.as_deref());
                    let stroke_attr = path::format_stroke_attr(stroke.as_ref(), stroke_id);
                    let transform_attr = filters::format_path_transform(
                        v_path,
                        transform,
                        effects,
                        scene.width_emu,
                    );
                    let fill_rule_attr = path::format_fill_rule_attr(v_path.fill_rule);

                    let effect_ref = effect_refs.get(cmd_idx).and_then(|r| r.as_ref());
                    let id_attr = match effect_ref {
                        Some(r) if r.needs_path_id => format!(r#" id="{}""#, r.path_id),
                        _ => String::new(),
                    };
                    let filter_attr = match effect_ref.and_then(|r| r.filter_id.as_deref()) {
                        Some(id) => format!(r#" filter="url(#{})""#, id),
                        None => String::new(),
                    };

                    writeln!(
                        &mut element,
                        r#"  <path d="{}" {} {}{}{}{}{}/>"#,
                        d,
                        fill_attr,
                        stroke_attr,
                        fill_rule_attr,
                        transform_attr,
                        id_attr,
                        filter_attr
                    )
                    .unwrap();

                    if let Some(reflection_use) =
                        effect_ref.and_then(|r| r.reflection_use.as_deref())
                    {
                        element.push_str("  ");
                        element.push_str(reflection_use);
                        element.push('\n');
                    }
                }
                VectorCommand::DrawImage {
                    rel_id,
                    src_rect,
                    dest_rect,
                    opacity,
                    transform,
                    effects,
                } => {
                    let img_svg = image::serialize_svg_image(
                        assets,
                        rel_id,
                        src_rect,
                        dest_rect,
                        *opacity,
                        transform,
                        effects,
                    );
                    if !img_svg.is_empty() {
                        element.push_str(&img_svg);
                        element.push('\n');
                    }
                }
                VectorCommand::DrawText { runs, transform } => {
                    element = text::serialize_svg_text(runs, transform, text_mode);
                }
                VectorCommand::PushClip(_) | VectorCommand::PopClip => {}
            }

            if element.is_empty() {
                continue;
            }
            if let Some(link) = find_command_link(&scene.hyperlinks, cmd) {
                svg.push_str(&wrap_svg_anchor(&element, link));
            } else {
                svg.push_str(&element);
            }
        }

        // Close groups left open by an unbalanced PushClip so the document
        // always stays well-formed.
        let unclosed = open_clips.iter().filter(|open| **open).count();
        for _ in 0..unclosed {
            let _ = writeln!(&mut svg, "  </g>");
        }

        writeln!(&mut svg, "</svg>").unwrap();
        Ok(svg)
    }
}

fn emit_clip_path_def(defs: &mut String, id: &str, path: &crate::contracts::VectorPath) {
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

fn find_command_link<'a>(links: &'a [SlideLink], command: &VectorCommand) -> Option<&'a SlideLink> {
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

fn command_bounds(command: &VectorCommand) -> Option<crate::contracts::RectF32> {
    match command {
        VectorCommand::DrawPath {
            path, transform, ..
        } => {
            let mut bounds: Option<crate::contracts::RectF32> = None;
            for segment in &path.segments {
                let points: &[(f32, f32)] = match segment {
                    PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => {
                        &[(*x, *y)]
                    }
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
        VectorCommand::PushClip(_) | VectorCommand::PopClip => None,
    }
}

fn include_point(bounds: &mut Option<crate::contracts::RectF32>, x: f32, y: f32) {
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

fn wrap_svg_anchor(element: &str, link: &SlideLink) -> String {
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

fn xml_escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
