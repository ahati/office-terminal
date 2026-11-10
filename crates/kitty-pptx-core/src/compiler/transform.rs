use crate::contracts::AffineTransform;

/// Construct an AffineTransform for an element with bounding box (x, y, w, h),
/// rotation (degrees clockwise), and horizontal/vertical flip flags.
/// Rotation and flipping are performed relative to the shape center.
pub fn compute_shape_transform(
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    rotation_deg: f64,
    flip_h: bool,
    flip_v: bool,
) -> AffineTransform {
    let w = width as f32;
    let h = height as f32;
    let cx = w / 2.0;
    let cy = h / 2.0;

    let sx = if flip_h { -1.0 } else { 1.0 };
    let sy = if flip_v { -1.0 } else { 1.0 };

    let rad = (rotation_deg as f32).to_radians();
    let cos = rad.cos();
    let sin = rad.sin();

    // M.sx = cos * sx
    // M.kx = -sin * sy
    // M.ky = sin * sx
    // M.sy = cos * sy
    // M.tx = x + cx - M.sx * cx - M.kx * cy
    // M.ty = y + cy - M.ky * cx - M.sy * cy
    let m_sx = cos * sx;
    let m_kx = -sin * sy;
    let m_ky = sin * sx;
    let m_sy = cos * sy;

    let x_f = x as f32;
    let y_f = y as f32;

    let m_tx = x_f + cx - m_sx * cx - m_kx * cy;
    let m_ty = y_f + cy - m_ky * cx - m_sy * cy;

    AffineTransform {
        sx: m_sx,
        ky: m_ky,
        kx: m_kx,
        sy: m_sy,
        tx: m_tx,
        ty: m_ty,
    }
}
