//! Unit-square → quad homography, used to extrapolate a projected face.
//!
//! Port of `packages/core/src/shape/scene3d-draw.ts:40-97` (`unitSquareToQuad`,
//! `applyH`), `:432-458` (`expandProjectedQuad`) and `:460-478`
//! (`projectQuadPoint`). Everything is `f64`: the reference is JavaScript, and
//! callers compare projected corners against analytic pinhole bounds.

/// 3×3 homography in row-major order.
pub type Homography = [f64; 9];

/// One projected corner in shape-local CSS px.
pub type Point = (f64, f64);

/// Solve the homography that maps the unit square corners `(0,0), (1,0),
/// (1,1), (0,1)` onto `corners` (same TL, TR, BR, BL order).
///
/// Standard projective mapping of a unit square to a quadrilateral
/// (Heckbert, "Fundamentals of Texture Mapping and Image Warping", §2.2).
/// `None` only when the projective denominator vanishes (collinear quad in the
/// non-affine branch). An affine-degenerate quad (all corners equal) still maps
/// to the reference's constant matrix.
pub fn unit_square_to_quad(corners: [Point; 4]) -> Option<Homography> {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = corners;
    let dx1 = x1 - x2;
    let dx2 = x3 - x2;
    let dx3 = x0 - x1 + x2 - x3;
    let dy1 = y1 - y2;
    let dy2 = y3 - y2;
    let dy3 = y0 - y1 + y2 - y3;

    let (a13, a23) = if dx3.abs() < 1e-12 && dy3.abs() < 1e-12 {
        // Affine (parallelogram) case.
        (0.0, 0.0)
    } else {
        let den = dx1 * dy2 - dx2 * dy1;
        if den.abs() < 1e-12 {
            return None;
        }
        ((dx3 * dy2 - dx2 * dy3) / den, (dx1 * dy3 - dx3 * dy1) / den)
    };
    let a11 = x1 - x0 + a13 * x1;
    let a21 = x3 - x0 + a23 * x3;
    let a12 = y1 - y0 + a13 * y1;
    let a22 = y3 - y0 + a23 * y3;
    // Row-major: [a11 a21 a31; a12 a22 a32; a13 a23 1]
    Some([a11, a21, x0, a12, a22, y0, a13, a23, 1.0])
}

/// Apply a homography to a unit-square coordinate `(u, v)`.
pub fn apply_h(h: &Homography, u: f64, v: f64) -> Point {
    let w = h[6] * u + h[7] * v + h[8];
    ((h[0] * u + h[1] * v + h[2]) / w, (h[3] * u + h[4] * v + h[5]) / w)
}

/// Extrapolate a projected quad outward through the same projective map that
/// produced it: the image of `[-fx, 1+fx] × [-fy, 1+fy]`.
///
/// Used to grow a shape's offscreen / effect bounds by an edge margin without
/// re-deriving the camera from the padded raster size. `None` when the quad is
/// degenerate or an extrapolated corner crosses the projection horizon.
pub fn expand_projected_quad(
    corners: [Point; 4],
    fx: f64,
    fy: f64,
) -> Option<[Point; 4]> {
    let h = unit_square_to_quad(corners)?;
    let uv = [
        (-fx, -fy),
        (1.0 + fx, -fy),
        (1.0 + fx, 1.0 + fy),
        (-fx, 1.0 + fy),
    ];
    let mut out = [(0.0, 0.0); 4];
    for (slot, (u, v)) in out.iter_mut().zip(uv) {
        let w = h[6] * u + h[7] * v + h[8];
        if !(w.is_finite() && w > 1e-9) {
            return None;
        }
        *slot = apply_h(&h, u, v);
    }
    Some(out)
}

/// Map an authored unit-frame point through the same homography as the face.
///
/// Effect placement uses this for DrawingML alignment anchors: deriving a
/// corner or edge midpoint from the projected AABB is wrong for rotated and
/// perspective-projected faces.
pub fn project_quad_point(corners: [Point; 4], u: f64, v: f64) -> Option<Point> {
    let h = unit_square_to_quad(corners)?;
    let denominator = h[6] * u + h[7] * v + h[8];
    if denominator > 1e-9 {
        Some(apply_h(&h, u, v))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_maps_to_the_identity() {
        let square = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let h = unit_square_to_quad(square).expect("homography");
        for (u, v) in [(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)] {
            let (x, y) = apply_h(&h, u, v);
            assert!((x - u).abs() < 1e-12 && (y - v).abs() < 1e-12);
        }
    }

    #[test]
    fn expand_extrapolates_a_parallelogram() {
        let quad = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
        let grown = expand_projected_quad(quad, 0.1, 0.2).expect("expanded");
        assert!((grown[0].0 + 10.0).abs() < 1e-9);
        assert!((grown[0].1 + 10.0).abs() < 1e-9);
        assert!((grown[2].0 - 110.0).abs() < 1e-9);
        assert!((grown[2].1 - 60.0).abs() < 1e-9);
        // The authored corner stays exact.
        let corner = project_quad_point(quad, 1.0, 0.0).expect("corner");
        assert!((corner.0 - 100.0).abs() < 1e-12 && corner.1.abs() < 1e-12);
    }

    #[test]
    fn a_degenerate_quad_matches_the_reference() {
        // Reference `unitSquareToQuad` takes the affine branch for this quad and
        // returns a constant matrix (`scene3d-draw.ts:63-68`).
        let collapsed = [(0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)];
        let h = unit_square_to_quad(collapsed).expect("affine-degenerate quad still maps");
        assert_eq!(apply_h(&h, 0.5, 0.5), (0.0, 0.0));
        assert_eq!(expand_projected_quad(collapsed, 0.1, 0.1), Some([(0.0, 0.0); 4]));

        // A projective-degenerate (collinear, non-affine) quad has no map.
        let collinear = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)];
        assert!(unit_square_to_quad(collinear).is_none());
        assert!(expand_projected_quad(collinear, 0.1, 0.1).is_none());
        assert!(project_quad_point(collinear, 0.5, 0.5).is_none());
    }
}
