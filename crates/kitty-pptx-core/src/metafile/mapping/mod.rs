//! Engine-neutral values → display-list values (PRD 13 §4).

pub mod bitmap;
pub mod clip;
pub mod paint;
pub mod path;
pub mod pen;
pub mod text;

use crate::contracts::{AffineTransform, PathSegment, RectF32, VectorCommand, VectorPath};

/// `outer ∘ inner` (apply `inner` first).
pub fn compose(outer: &AffineTransform, inner: &AffineTransform) -> AffineTransform {
    outer.post_concat(inner)
}

/// Applies `transform` to every point in `path`.
pub fn transform_path(path: &VectorPath, transform: &AffineTransform) -> VectorPath {
    let point = |x: f32, y: f32| transform.transform_point(x, y);
    let segments = path
        .segments
        .iter()
        .map(|segment| match *segment {
            PathSegment::MoveTo { x, y } => {
                let (x, y) = point(x, y);
                PathSegment::MoveTo { x, y }
            }
            PathSegment::LineTo { x, y } => {
                let (x, y) = point(x, y);
                PathSegment::LineTo { x, y }
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (x1, y1) = point(x1, y1);
                let (x, y) = point(x, y);
                PathSegment::QuadTo { x1, y1, x, y }
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (x1, y1) = point(x1, y1);
                let (x2, y2) = point(x2, y2);
                let (x, y) = point(x, y);
                PathSegment::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                }
            }
            PathSegment::Close => PathSegment::Close,
        })
        .collect();
    VectorPath::new(segments, path.fill_rule)
}

/// Maps engine output units into picture-local EMU for a crop/destination pair.
///
/// `bounds` is the engine's drawing bounds; `src` is the `<a:srcRect>` window;
/// `dest` is the visible destination box in picture-local EMU.
pub fn source_to_emu(
    bounds: &RectF32,
    src: &crate::contracts::NormalizedRect,
    dest: &RectF32,
) -> Option<AffineTransform> {
    let (bw, bh) = (bounds.w, bounds.h);
    let src_w = src.w * bw;
    let src_h = src.h * bh;
    if !(bw > 0.0 && bh > 0.0 && src_w > 0.0 && src_h > 0.0 && dest.w > 0.0 && dest.h > 0.0) {
        return None;
    }
    let sx = dest.w / src_w;
    let sy = dest.h / src_h;
    if !sx.is_finite() || !sy.is_finite() {
        return None;
    }
    Some(AffineTransform {
        sx,
        ky: 0.0,
        kx: 0.0,
        sy,
        tx: dest.x - (bounds.x + src.x * bw) * sx,
        ty: dest.y - (bounds.y + src.y * bh) * sy,
    })
}

/// Axis-aligned bounds of `rect` after `transform`.
pub fn transform_rect(rect: &RectF32, transform: &AffineTransform) -> RectF32 {
    let corners = [
        transform.transform_point(rect.x, rect.y),
        transform.transform_point(rect.x + rect.w, rect.y),
        transform.transform_point(rect.x, rect.y + rect.h),
        transform.transform_point(rect.x + rect.w, rect.y + rect.h),
    ];
    let min_x = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let min_y = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_x = corners.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let max_y = corners.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    RectF32 {
        x: min_x,
        y: min_y,
        w: (max_x - min_x).max(0.0),
        h: (max_y - min_y).max(0.0),
    }
}

/// Composes `outer` into a command's transform/placement (used by expansion).
pub fn transform_command(command: &VectorCommand, outer: &AffineTransform) -> VectorCommand {
    match command {
        VectorCommand::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects,
        } => VectorCommand::DrawPath {
            path: path.clone(),
            fill: fill.clone(),
            stroke: stroke.clone(),
            transform: compose(outer, transform),
            effects: effects.clone(),
        },
        VectorCommand::DrawLineEnd {
            path,
            fill,
            stroke,
            transform,
        } => VectorCommand::DrawLineEnd {
            path: path.clone(),
            fill: fill.clone(),
            stroke: stroke.clone(),
            transform: compose(outer, transform),
        },
        VectorCommand::DrawText { runs, transform } => VectorCommand::DrawText {
            runs: runs.clone(),
            transform: compose(outer, transform),
        },
        VectorCommand::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            effects,
        } => VectorCommand::DrawImage {
            rel_id: rel_id.clone(),
            src_rect: *src_rect,
            dest_rect: *dest_rect,
            opacity: *opacity,
            transform: compose(outer, transform),
            effects: effects.clone(),
        },
        VectorCommand::DrawMetafile {
            rel_id,
            transform,
            src_rect,
            dest_rect,
            opacity,
            effects,
        } => VectorCommand::DrawMetafile {
            rel_id: rel_id.clone(),
            transform: compose(outer, transform),
            src_rect: *src_rect,
            dest_rect: *dest_rect,
            opacity: *opacity,
            effects: effects.clone(),
        },
        VectorCommand::DrawBitmap {
            png,
            width,
            height,
            placement,
            opacity,
            sampling,
        } => VectorCommand::DrawBitmap {
            png: png.clone(),
            width: *width,
            height: *height,
            placement: compose(outer, placement),
            opacity: *opacity,
            sampling: *sampling,
        },
        VectorCommand::PushClip(path) => VectorCommand::PushClip(transform_path(path, outer)),
        VectorCommand::PopClip => VectorCommand::PopClip,
    }
}
