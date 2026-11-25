use super::shapes::build_shape_path;
use super::style::{compile_fill, compile_stroke};
use super::transform::compute_shape_transform;
use crate::contracts::{FidelityClass, NormalizedRect, RectF32, VectorCommand, WarningEvent};
use crossbeam_channel::Sender;

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

    // §20.1.8.55 crop projection. `None` means the crop leaves no visible
    // source area, so the bitmap body is not painted at all (the fill and
    // border below still are, matching the reference).
    let crop = project_crop(pic.src_rect.as_ref(), pic.width as f32, pic.height as f32);

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
        let shape_path = clip_path
            .clone()
            .unwrap_or_else(|| build_shape_path("rect", pic.width as f32, pic.height as f32, &[], None, warning_sink));
        if let Some(paint) = compile_fill(fill, 1.0) {
            commands.push(VectorCommand::DrawPath {
                path: shape_path,
                fill: Some(paint),
                stroke: None,
                transform,
                effects: Default::default(),
            });
        }
    }

    // 2. Push clip if shaped (only when there is a bitmap body to clip)
    if crop.is_some() {
        if let Some(ref path) = clip_path {
            // Transform the clip path into slide coordinates
            let transformed_path = transform_vector_path(path, &transform);
            commands.push(VectorCommand::PushClip(transformed_path));
        }
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
    let rel_id = pic.svg_image_path.as_ref().unwrap_or(&pic.image_path).clone();

    if crate::metafile::is_metafile_path(&rel_id) {
        // The compiler never reads archive bytes; the marker is expanded by
        // `metafile::expand_metafiles` where the AssetStore is available.
        // The clip was only pushed when a crop existed, so only then pop it.
        let had_crop = crop.is_some();
        let (src_rect, dest_rect) = match crop {
            Some(crop) => (crop.src, crop.dest),
            None => (
                NormalizedRect::default(),
                RectF32 {
                    x: 0.0,
                    y: 0.0,
                    w: pic.width as f32,
                    h: pic.height as f32,
                },
            ),
        };
        commands.push(VectorCommand::DrawMetafile {
            rel_id,
            transform,
            src_rect,
            dest_rect,
            opacity,
            effects,
        });

        if had_crop && clip_path.is_some() {
            commands.push(VectorCommand::PopClip);
        }
    } else if let Some(crop) = crop {
        commands.push(VectorCommand::DrawImage {
            rel_id,
            src_rect: crop.src,
            dest_rect: crop.dest,
            opacity,
            transform,
            effects,
        });

        // 4. Pop clip if shaped
        if clip_path.is_some() {
            commands.push(VectorCommand::PopClip);
        }
    }

    // 5. Draw border stroke if present
    if let Some(ref stroke) = pic.stroke {
        let border_path = clip_path
            .unwrap_or_else(|| build_shape_path("rect", pic.width as f32, pic.height as f32, &[], None, warning_sink));
        if let Some(v_stroke) = compile_stroke(stroke) {
            commands.push(VectorCommand::DrawPath {
                path: border_path,
                fill: None,
                stroke: Some(v_stroke),
                transform,
                effects: Default::default(),
            });
        }
    }

    commands
}

/// Source window and destination box produced by an `<a:srcRect>` crop.
struct PictureCrop {
    /// Visible source window in normalized `[0, 1]` image coordinates.
    src: NormalizedRect,
    /// Picture-local EMU box the source window is stretched onto.
    dest: RectF32,
}

/// Project an `<a:srcRect>` crop (ECMA-376 §20.1.8.55) onto a picture frame,
/// ported from the reference `imageCropProjection` / `drawImageCropped`
/// (`packages/core/src/image/crop.ts:100-111, 191-231`).
///
/// `pptx_model::SrcRect` already holds signed fractions of the source bitmap —
/// `ooxml-common/src/blip.rs:132` divides the raw `ST_Percentage` by 100000 when
/// parsing — so the authored visible region is `[l, t, 1 - r, 1 - b]`. It must
/// never be re-divided here.
///
/// Positive insets keep the destination at the full picture frame: the source
/// slice is stretched over it (`drawImage` 9-argument semantics). Negative
/// (outset) margins make the authored region larger than the bitmap; the source
/// is clamped to `[0, 1]` and the destination shrinks to the sub-box that the
/// clamped slice maps to (`crop.ts:92-97`). `VectorCommand::DrawImage` carries a
/// single destination rect, so the outset is expressed by shrinking
/// `dest_rect` instead of keeping the full frame and adding transparent
/// gutters; that is geometrically identical because both the rasterizer and the
/// SVG serializer stretch the source window over the whole destination box.
///
/// Returns `None` when the crop leaves no visible source area, which the
/// reference paints as nothing (`crop.ts:100-105`).
fn project_crop(src_rect: Option<&ooxml_common::blip::SrcRect>, frame_w: f32, frame_h: f32) -> Option<PictureCrop> {
    let full = |w: f32, h: f32| PictureCrop {
        src: NormalizedRect::default(),
        dest: RectF32::new(0.0, 0.0, w, h),
    };

    let sr = match src_rect {
        Some(sr) => sr,
        None => return Some(full(frame_w, frame_h)),
    };

    // Non-finite fractions mean "no crop" in the reference; all-zero edges are
    // already filtered by the parser but keep the guard for programmatic input.
    if ![sr.l, sr.t, sr.r, sr.b].iter().all(|v| v.is_finite()) {
        return Some(full(frame_w, frame_h));
    }
    if sr.l == 0.0 && sr.t == 0.0 && sr.r == 0.0 && sr.b == 0.0 {
        return Some(full(frame_w, frame_h));
    }

    let (l, t, r, b) = (sr.l as f32, sr.t as f32, sr.r as f32, sr.b as f32);
    let logical_w = 1.0 - r - l;
    let logical_h = 1.0 - b - t;
    if logical_w <= 0.0 || logical_h <= 0.0 {
        return None;
    }

    // Clamp the authored region to the bitmap (crop.ts:88-89).
    let sx0 = l.max(0.0);
    let sy0 = t.max(0.0);
    let sx1 = (1.0 - r).min(1.0);
    let sy1 = (1.0 - b).min(1.0);
    let sw = (sx1 - sx0).max(0.0);
    let sh = (sy1 - sy0).max(0.0);

    // Destination projection (crop.ts:92-97): where the clamped slice lands
    // inside the authored frame.
    let dx = (sx0 - l) / logical_w;
    let dy = (sy0 - t) / logical_h;
    let dw = sw / logical_w;
    let dh = sh / logical_h;
    if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
        return None;
    }

    Some(PictureCrop {
        src: NormalizedRect {
            x: sx0,
            y: sy0,
            w: sw,
            h: sh,
        },
        dest: RectF32::new(dx * frame_w, dy * frame_h, dw * frame_w, dh * frame_h),
    })
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
            crate::contracts::PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
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
