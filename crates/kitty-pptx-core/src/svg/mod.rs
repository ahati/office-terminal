mod background_image;
mod defs;
mod filters;
mod image;
mod path;
mod text;

use std::fmt::Write;
use self::defs::{
    emit_clip_path_def, emit_fill_def, emit_stroke_def, find_command_link, wrap_svg_anchor,
};
use crate::contracts::{
    SvgTextMode, VectorBackground, VectorCommand, VectorSlide,
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

        writeln!(&mut svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#).ok();
        if scene.hyperlinks.is_empty() {
            writeln!(
                &mut svg,
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">"#,
                w, h
            )
            .ok();
        } else {
            writeln!(
                &mut svg,
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 {} {}" width="100%" height="100%">"#,
                w, h
            )
            .ok();
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

            match cmd {
                VectorCommand::DrawPath {
                    path: v_path,
                    fill,
                    stroke,
                    transform,
                    effects,
                } => {
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
                    path_grad_ids.push(emit_fill_def(
                        &mut defs,
                        &mut grad_id_counter,
                        assets,
                        v_path,
                        fill.as_ref(),
                    ));
                    stroke_paint_ids.push(emit_stroke_def(
                        &mut defs,
                        &mut grad_id_counter,
                        v_path,
                        stroke.as_ref(),
                    ));
                }
                // A line-end decoration carries no effect layer of its own; its
                // fill/stroke may still reference a paint server
                // (ref renderer.ts:4513-4535).
                VectorCommand::DrawLineEnd {
                    path: v_path,
                    fill,
                    stroke,
                    ..
                } => {
                    effect_refs.push(None);
                    path_grad_ids.push(emit_fill_def(
                        &mut defs,
                        &mut grad_id_counter,
                        assets,
                        v_path,
                        fill.as_ref(),
                    ));
                    stroke_paint_ids.push(emit_stroke_def(
                        &mut defs,
                        &mut grad_id_counter,
                        v_path,
                        stroke.as_ref(),
                    ));
                }
                _ => {
                    effect_refs.push(None);
                    path_grad_ids.push(None);
                    stroke_paint_ids.push(None);
                }
            }
        }

        if !defs.is_empty() {
            writeln!(&mut svg, "  <defs>").ok();
            svg.push_str(&defs);
            writeln!(&mut svg, "  </defs>").ok();
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
                    .ok();
                }
                VectorBackground::Gradient(_) => {
                    if let Some(ref id) = bg_grad_id {
                        writeln!(
                            &mut svg,
                            r#"  <rect width="{}" height="{}" fill="url(#{})"/>"#,
                            w, h, id
                        )
                        .ok();
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
            .ok();
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
                    .ok();

                    if let Some(reflection_use) =
                        effect_ref.and_then(|r| r.reflection_use.as_deref())
                    {
                        element.push_str("  ");
                        element.push_str(reflection_use);
                        element.push('\n');
                    }
                }
                VectorCommand::DrawLineEnd {
                    path: v_path,
                    fill,
                    stroke,
                    transform,
                } => {
                    let d = path::path_to_svg_d(v_path);
                    let grad_id = path_grad_ids.get(cmd_idx).and_then(|id| id.as_deref());
                    let fill_attr = path::format_fill_attr(fill.as_ref(), grad_id);
                    let stroke_id = stroke_paint_ids.get(cmd_idx).and_then(|id| id.as_deref());
                    let stroke_attr = path::format_stroke_attr(stroke.as_ref(), stroke_id);
                    let transform_attr = path::format_transform_attr(transform);
                    let fill_rule_attr = path::format_fill_rule_attr(v_path.fill_rule);
                    writeln!(
                        &mut element,
                        r#"  <path d="{}" {} {}{}{}/>"#,
                        d, fill_attr, stroke_attr, fill_rule_attr, transform_attr
                    )
                    .ok();
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
                VectorCommand::DrawBitmap {
                    png,
                    placement,
                    opacity,
                    ..
                } => {
                    element = image::serialize_svg_bitmap(png, placement, *opacity);
                }
                VectorCommand::DrawMetafile {
                    dest_rect,
                    transform,
                    ..
                } => {
                    element = image::serialize_svg_missing_metafile(dest_rect, transform);
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

        writeln!(&mut svg, "</svg>").ok();
        Ok(svg)
    }
}
