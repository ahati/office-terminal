//! Effect bounds and anchors on the projected 3-D body.
//!
//! Port of `packages/pptx/src/renderer.ts:3596-3626` (`transformedEffectBounds`),
//! `:3689-3721` (`projectedEffectGeometry`), `:3656-3687` (`projectedSurface`)
//! and the `ALIGNMENT_UV` table at `:3637-3640`.

use super::camera::{compute_scene3d_quad, CameraInput};
use super::homography::{expand_projected_quad, project_quad_point, Point};
use crate::contracts::AffineTransform;

/// The live 2-D device transform of the shape (`CanvasRenderingContext2D`
/// matrix in the reference: `a b c d e f`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectTransform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl EffectTransform {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn uniform_scale(scale: f64) -> Self {
        Self { a: scale, b: 0.0, c: 0.0, d: scale, e: 0.0, f: 0.0 }
    }

    /// Build from the IR's affine (`sx ky kx sy tx ty`, row-vector convention).
    pub fn from_affine(t: &AffineTransform) -> Self {
        Self {
            a: t.sx as f64,
            b: t.ky as f64,
            c: t.kx as f64,
            d: t.sy as f64,
            e: t.tx as f64,
            f: t.ty as f64,
        }
    }

    /// `sqrt(|det|)`, i.e. the device scale the reference recovers from the
    /// live transform (`devScale`); 1.0 for a degenerate matrix.
    pub fn device_scale(&self) -> f64 {
        let det = (self.a * self.d - self.b * self.c).abs();
        if det > 0.0 {
            det.sqrt()
        } else {
            1.0
        }
    }

    fn point(&self, x: f64, y: f64) -> Point {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

/// Axis-aligned device-pixel box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `ST_RectAlignment` (`algn`) pivot; `b` is the schema default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Alignment {
    /// The unit-square anchor the effect is aligned to (ref `ALIGNMENT_UV`).
    pub fn uv(self) -> (f64, f64) {
        match self {
            Alignment::TopLeft => (0.0, 0.0),
            Alignment::Top => (0.5, 0.0),
            Alignment::TopRight => (1.0, 0.0),
            Alignment::Left => (0.0, 0.5),
            Alignment::Center => (0.5, 0.5),
            Alignment::Right => (1.0, 0.5),
            Alignment::BottomLeft => (0.0, 1.0),
            Alignment::Bottom => (0.5, 1.0),
            Alignment::BottomRight => (1.0, 1.0),
        }
    }

    /// Parse an `a:effectLst` `algn` value; unknown values fall back to `b`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "tl" => Alignment::TopLeft,
            "t" => Alignment::Top,
            "tr" => Alignment::TopRight,
            "l" => Alignment::Left,
            "ctr" => Alignment::Center,
            "r" => Alignment::Right,
            "bl" => Alignment::BottomLeft,
            "b" => Alignment::Bottom,
            "br" => Alignment::BottomRight,
            _ => Alignment::Bottom,
        }
    }
}

/// Axis-aligned device-pixel bounds of a shape after the current transform.
///
/// DrawingML group transforms are already folded into that matrix; effect
/// surfaces must therefore be cropped from these transformed bounds, not from
/// the shape's group-local coordinates (ref `transformedEffectBounds`).
pub fn transformed_effect_bounds(
    transform: &EffectTransform,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Bounds {
    point_bounds([
        transform.point(x, y),
        transform.point(x + w, y),
        transform.point(x + w, y + h),
        transform.point(x, y + h),
    ])
}

fn point_bounds(points: impl IntoIterator<Item = Point>) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (px, py) in points {
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    Bounds { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y }
}

/// One surface plane for body projection, effect bounds/anchors and masks
/// (ref `projectedSurface`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedSurface {
    /// Authored face corners (shape-local px) used for anchor placement.
    pub face: [(f64, f64); 4],
    /// Padded quad used for the painted footprint / effect crop.
    pub corners: [(f64, f64); 4],
    /// Whether the camera exposes the back face.
    pub back_facing: bool,
    /// Edge margin in whole device px.
    pub pad_dev: f64,
    /// Edge margin in CSS px (`pad_dev / devScale`).
    pub pad_css: f64,
}

/// Select the surface plane and quantise its edge padding.
///
/// MS-OI29500 §2.1.1268 places `bevelB` at the extrusion base: when the camera
/// exposes the back face, the effect footprint and anchor belong to that rear
/// plane, not to the front one. Padding is quantised here so cache/effect bounds
/// include the same device-pixel margin as the raster.
pub fn projected_surface(
    camera: &CameraInput,
    w: f64,
    h: f64,
    back_depth_css: f64,
    edge_pad_css: f64,
    dev_scale: f64,
) -> ProjectedSurface {
    // The selected face and its effect footprint belong to one solid.
    let camera = camera.clone().with_scene_depth(back_depth_css);
    let front = compute_scene3d_quad(&camera, w, h, 0.0);
    // Winding of `TL, TR, BL`: negative = the camera sees the back of the face.
    let [p0, p1, _, p3] = front.corners;
    let signed = (p1.0 - p0.0) * (p3.1 - p0.1) - (p1.1 - p0.1) * (p3.0 - p0.0);
    let back_facing = signed < 0.0;
    let quad = if back_facing && back_depth_css > 0.0 {
        compute_scene3d_quad(&camera, w, h, back_depth_css)
    } else {
        front
    };
    let mut pad_dev = (edge_pad_css * dev_scale).max(0.0).ceil();
    let expanded = if pad_dev > 0.0 && w > 0.0 && h > 0.0 {
        expand_projected_quad(quad.corners, pad_dev / dev_scale / w, pad_dev / dev_scale / h)
    } else {
        None
    };
    if expanded.is_none() {
        pad_dev = 0.0;
    }
    ProjectedSurface {
        face: quad.corners,
        corners: expanded.unwrap_or(quad.corners),
        back_facing,
        pad_dev,
        pad_css: pad_dev / dev_scale,
    }
}

/// Effect bounds and alignment anchor for a (possibly) projected shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedEffectGeometry {
    pub bbox: Bounds,
    pub anchor: (f64, f64),
}

/// Bounds of the padded projected footprint and the anchor the effect aligns
/// to (ref `projectedEffectGeometry`).
///
/// The anchor is the *authored* alignment point pushed through the camera
/// homography, so it stays distinct from the AABB corner for rotated and
/// perspective-projected faces.
#[allow(clippy::too_many_arguments)] // Mirrors the reference signature.
pub fn projected_effect_geometry(
    camera: &CameraInput,
    transform: &EffectTransform,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    edge_pad_css: f64,
    align: Alignment,
    back_depth_css: f64,
) -> ProjectedEffectGeometry {
    let dev_scale = transform.device_scale();
    let surface = projected_surface(camera, w, h, back_depth_css, edge_pad_css, dev_scale);
    let (face, painted) = (surface.face, surface.corners);
    let bbox = point_bounds(painted.map(|(px, py)| transform.point(x + px, y + py)));
    let (u, v) = align.uv();
    let anchor = project_quad_point(face, u, v)
        .map(|(px, py)| transform.point(x + px, y + py))
        .unwrap_or_else(|| {
            let (hx, hy) = authored_alignment_anchor(transform, x, y, w, h, align);
            (hx, hy)
        });
    ProjectedEffectGeometry { bbox, anchor }
}

/// Alignment anchor on the un-projected box, used when the face has no
/// drawable homography (ref `authoredAlignmentAnchor`).
pub fn authored_alignment_anchor(
    transform: &EffectTransform,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    align: Alignment,
) -> (f64, f64) {
    let (u, v) = align.uv();
    transform.point(x + u * w, y + v * h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_transform_keeps_the_box() {
        let bounds = transformed_effect_bounds(&EffectTransform::IDENTITY, 10.0, 20.0, 40.0, 30.0);
        assert_eq!(bounds, Bounds { x: 10.0, y: 20.0, w: 40.0, h: 30.0 });
    }

    #[test]
    fn alignment_table_matches_st_rect_alignment() {
        assert_eq!(Alignment::from_name("tr").uv(), (1.0, 0.0));
        assert_eq!(Alignment::from_name("ctr").uv(), (0.5, 0.5));
        // Unknown / absent values default to bottom centre.
        assert_eq!(Alignment::from_name("nope"), Alignment::Bottom);
    }

    #[test]
    fn a_front_camera_reports_its_dev_scale() {
        assert!((EffectTransform::uniform_scale(2.0).device_scale() - 2.0).abs() < 1e-12);
        let degenerate = EffectTransform { a: 0.0, b: 0.0, c: 0.0, d: 0.0, e: 0.0, f: 0.0 };
        assert_eq!(degenerate.device_scale(), 1.0);
    }
}
