use crossbeam_channel::Sender;
use crate::contracts::{
    FidelityClass, NormalizedRect, RectF32, VectorCommand, WarningEvent,
};
use super::shapes::build_shape_path;
use super::style::{compile_fill, compile_stroke};
use super::transform::compute_shape_transform;

pub fn compile_picture_element(
    pic: &pptx_model::PictureElement,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();

    let transform = compute_shape_transform(
        pic.x,
        pic.y,
        pic.width,
        pic.height,
        pic.rotation,
        pic.flip_h,
        pic.flip_v,
    );

    // Compute normalized crop rectangle
    let src_rect = if let Some(ref sr) = pic.src_rect {
        let x = sr.l as f32 / 100000.0;
        let y = sr.t as f32 / 100000.0;
        let w = (1.0 - (sr.l + sr.r) as f32 / 100000.0).max(0.01);
        let h = (1.0 - (sr.t + sr.b) as f32 / 100000.0).max(0.01);
        NormalizedRect { x, y, w, h }
    } else {
        NormalizedRect::default()
    };

    let dest_rect = RectF32::new(0.0, 0.0, pic.width as f32, pic.height as f32);
    let opacity = pic.alpha.map(|a| a as f32).unwrap_or(1.0);

    // Determine if picture has non-rectangular clip
    let prst_name = pic.prst_geom.as_deref().unwrap_or("rect");
    let has_custom_shape = prst_name != "rect" || pic.cust_geom.is_some();

    let clip_path = if has_custom_shape {
        let adj_slice: Vec<Option<f64>> = pic
            .prst_adjust
            .as_ref()
            .map(|v| v.iter().map(|&a| Some(a as f64)).collect())
            .unwrap_or_default();
        Some(build_shape_path(
            prst_name,
            pic.width as f32,
            pic.height as f32,
            &adj_slice,
            pic.cust_geom.as_deref(),
            warning_sink,
        ))
    } else {
        None
    };

    // 1. Draw background fill behind picture if present
    if let Some(ref fill) = pic.fill {
        let shape_path = clip_path.clone().unwrap_or_else(|| {
            build_shape_path(
                "rect",
                pic.width as f32,
                pic.height as f32,
                &[],
                None,
                warning_sink,
            )
        });
        if let Some(paint) = compile_fill(fill, 1.0) {
            commands.push(VectorCommand::DrawPath {
                path: shape_path,
                fill: Some(paint),
                stroke: None,
                transform,
            });
        }
    }

    // 2. Push clip if shaped
    if let Some(ref path) = clip_path {
        // Transform the clip path into slide coordinates
        let transformed_path = transform_vector_path(path, &transform);
        commands.push(VectorCommand::PushClip(transformed_path));
    }

    // 3. Draw image with blip effects
    // Collect blip effects and emit fidelity warnings per PRD 02d §2.2
    let mut effects = pic.blip_effects.clone();
    if let Some(ref duo) = pic.duotone {
        let has_duotone = effects
            .iter()
            .any(|e| matches!(e, ooxml_common::blip::BlipEffect::Duotone { .. }));
        if !has_duotone {
            effects.push(ooxml_common::blip::BlipEffect::Duotone {
                clr1: duo.clr1.clone(),
                clr2: duo.clr2.clone(),
            });
        }
    }

    for _ in &effects {
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: pic.image_path.clone(),
            feature: "blip_effect",
            class: FidelityClass::Degraded,
        });
    }

    // Prefer svg_image_path if available
    let rel_id = pic
        .svg_image_path
        .as_ref()
        .unwrap_or(&pic.image_path)
        .clone();

    commands.push(VectorCommand::DrawImage {
        rel_id,
        src_rect,
        dest_rect,
        opacity,
        transform,
        effects,
    });

    // 4. Pop clip if shaped
    if clip_path.is_some() {
        commands.push(VectorCommand::PopClip);
    }

    // 5. Draw border stroke if present
    if let Some(ref stroke) = pic.stroke {
        let border_path = clip_path.unwrap_or_else(|| {
            build_shape_path(
                "rect",
                pic.width as f32,
                pic.height as f32,
                &[],
                None,
                warning_sink,
            )
        });
        if let Some(v_stroke) = compile_stroke(stroke) {
            commands.push(VectorCommand::DrawPath {
                path: border_path,
                fill: None,
                stroke: Some(v_stroke),
                transform,
            });
        }
    }

    commands
}

fn transform_vector_path(
    path: &crate::contracts::VectorPath,
    transform: &crate::contracts::AffineTransform,
) -> crate::contracts::VectorPath {
    let mut segs = Vec::new();
    for seg in &path.segments {
        match seg {
            crate::contracts::PathSegment::MoveTo { x, y } => {
                let (tx, ty) = transform.transform_point(*x, *y);
                segs.push(crate::contracts::PathSegment::MoveTo { x: tx, y: ty });
            }
            crate::contracts::PathSegment::LineTo { x, y } => {
                let (tx, ty) = transform.transform_point(*x, *y);
                segs.push(crate::contracts::PathSegment::LineTo { x: tx, y: ty });
            }
            crate::contracts::PathSegment::QuadTo { x1, y1, x, y } => {
                let (tx1, ty1) = transform.transform_point(*x1, *y1);
                let (tx, ty) = transform.transform_point(*x, *y);
                segs.push(crate::contracts::PathSegment::QuadTo {
                    x1: tx1,
                    y1: ty1,
                    x: tx,
                    y: ty,
                });
            }
            crate::contracts::PathSegment::CubicTo {
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
                segs.push(crate::contracts::PathSegment::CubicTo {
                    x1: tx1,
                    y1: ty1,
                    x2: tx2,
                    y2: ty2,
                    x: tx,
                    y: ty,
                });
            }
            crate::contracts::PathSegment::Close => {
                segs.push(crate::contracts::PathSegment::Close);
            }
        }
    }
    crate::contracts::VectorPath::new(segs, path.fill_rule)
}
