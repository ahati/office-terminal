//! DrawingML gradient frame projection (`a:gradFill`, ECMA-376 §20.1.8.32).
//!
//! One owner for the authored frame options the reference applies while it
//! resolves a gradient into a canvas paint: the `a:tileRect` repeat frame, the
//! `a:lin@scaled` vector scaling, the `a:path`/`a:fillToRect` focal frame and
//! the `rotWithShape="0"` counter-rotation about the fill-box centre.
//!
//! All geometry stays in *local* shape coordinates (EMU), so the rasterizer and
//! the SVG serializer project the identical axis: only the final user-space →
//! device mapping differs. ref packages/core/src/shape/paint.ts:26-140,
//! 262-320 and packages/core/src/shape/fill-frame.ts:1-17.

use crate::contracts::{GradientFrame, VectorGradient};

/// An axis-aligned fill box in local shape coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl GradientBox {
    pub fn centre(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Fractional rect (`x = l`, `y = t`, `w = 1-l-r`, `h = 1-t-b`) applied to a
/// box, mirroring `box.x + box.w * rect.l` / `box.w * (1 - l - r)`.
/// ref core/src/shape/paint.ts:270-275, 118-122
pub fn apply_rect(rect: Option<&crate::contracts::NormalizedRect>, bx: GradientBox) -> GradientBox {
    match rect {
        None => bx,
        Some(r) => GradientBox {
            x: bx.x + bx.w * r.x,
            y: bx.y + bx.h * r.y,
            w: bx.w * r.w,
            h: bx.h * r.h,
        },
    }
}

/// The tile frame the gradient is painted into: the authored `a:tileRect` over
/// the shape box (ref paint.ts:270-275).
pub fn tile_box(frame: &GradientFrame, shape: GradientBox) -> GradientBox {
    apply_rect(frame.tile_rect.as_ref(), shape)
}

/// `counterRotatedFillTransform` for a single point, with `rotation_deg` the
/// angle already applied to the frame ([`GradientFrame::counter_rotation_deg`]).
/// A zero rotation is the identity.
/// ref core/src/shape/fill-frame.ts:4-17
pub fn counter_rotate(point: (f32, f32), rotation_deg: f32, centre: (f32, f32)) -> (f32, f32) {
    if rotation_deg == 0.0 {
        return point;
    }
    let angle = rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = point.0 - centre.0;
    let y = point.1 - centre.1;
    (centre.0 + cos * x - sin * y, centre.1 + sin * x + cos * y)
}

/// Endpoints of a `a:lin` gradient axis in local coordinates.
///
/// The authored angle is projected onto the tile frame, `scaled="1"` premultiplies
/// the unit vector by the frame extents before normalisation, the axis is
/// centred on that frame with half-length `(|cos|·w + |sin|·h) / 2`, and a
/// `rotWithShape="0"` frame counter-rotates both endpoints about the shape-box
/// centre. ref packages/core/src/shape/paint.ts:290-315
pub fn linear_axis(grad: &VectorGradient, shape: GradientBox) -> ((f32, f32), (f32, f32)) {
    let frame = &grad.frame;
    let tile = tile_box(frame, shape);
    let rad = grad.angle_deg.to_radians();
    let (mut dx, mut dy) = (rad.cos(), rad.sin());
    if frame.scaled {
        // §20.1.8.41: (cos x, sin x) becomes (w·cos x, h·sin x).
        dx *= tile.w;
        dy *= tile.h;
        let magnitude = (dx * dx + dy * dy).sqrt();
        if magnitude > 0.0 {
            dx /= magnitude;
            dy /= magnitude;
        }
    }
    let (cx, cy) = tile.centre();
    let half = (dx.abs() * tile.w + dy.abs() * tile.h) / 2.0;
    let rotation = frame.counter_rotation_deg();
    let centre = shape.centre();
    (
        counter_rotate((cx - dx * half, cy - dy * half), rotation, centre),
        counter_rotate((cx + dx * half, cy + dy * half), rotation, centre),
    )
}

/// Centre and radius of an `a:path` radial gradient in local coordinates.
///
/// The focus frame is the authored `a:fillToRect` inside the tile frame; the
/// radius is the max axis distance for `path="rect"` and the half-diagonal
/// otherwise. ref packages/core/src/shape/paint.ts:114-140
pub fn radial_frame(grad: &VectorGradient, shape: GradientBox) -> ((f32, f32), f32) {
    let frame = &grad.frame;
    let tile = tile_box(frame, shape);
    let focus = apply_rect(frame.fill_to_rect.as_ref(), tile);
    let (cx, cy) = focus.centre();
    let rx = (cx - tile.x).abs().max((tile.x + tile.w - cx).abs());
    let ry = (cy - tile.y).abs().max((tile.y + tile.h - cy).abs());
    let radius = if frame.path == "rect" {
        rx.max(ry)
    } else {
        (rx * rx + ry * ry).sqrt()
    };
    let rotation = frame.counter_rotation_deg();
    (counter_rotate((cx, cy), rotation, shape.centre()), radius)
}

/// The box a gradient paint is framed on: the path's local bounds.
pub fn path_box(path: &crate::contracts::VectorPath) -> Option<GradientBox> {
    let (min_x, min_y, max_x, max_y) = crate::compiler::style::path_bounds(path)?;
    Some(GradientBox {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    })
}

/// [`linear_axis`] framed on a path's local bounds.
pub fn linear_axis_for_path(
    grad: &VectorGradient,
    path: &crate::contracts::VectorPath,
) -> Option<((f32, f32), (f32, f32))> {
    Some(linear_axis(grad, path_box(path)?))
}

/// [`radial_frame`] framed on a path's local bounds.
pub fn radial_frame_for_path(grad: &VectorGradient, path: &crate::contracts::VectorPath) -> Option<((f32, f32), f32)> {
    Some(radial_frame(grad, path_box(path)?))
}
