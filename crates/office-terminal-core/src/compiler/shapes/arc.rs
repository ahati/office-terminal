use crate::contracts::PathSegment;

/// Decompose an OOXML ArcTo command into cubic Bezier segments.
/// wr, hr: semi-axes in local units
/// st_ang, sw_ang: start and sweep angles in degrees
/// center_x, center_y: center point of the arc
pub fn decompose_arc_to_cubic(
    center_x: f32,
    center_y: f32,
    wr: f32,
    hr: f32,
    st_ang_deg: f32,
    sw_ang_deg: f32,
    segments: &mut Vec<PathSegment>,
) {
    if sw_ang_deg.abs() < 1e-4 {
        return;
    }

    let total_sweep_rad = sw_ang_deg.to_radians();
    let start_rad = st_ang_deg.to_radians();

    // Divide sweep into sub-arcs of at most 45 degrees (pi/4 radians)
    let max_step = std::f32::consts::FRAC_PI_4;
    let n_steps = (total_sweep_rad.abs() / max_step).ceil().max(1.0) as usize;
    let step_rad = total_sweep_rad / (n_steps as f32);

    let mut current_ang = start_rad;

    for _ in 0..n_steps {
        let alpha = current_ang;
        let beta = current_ang + step_rad;
        let delta = step_rad;

        // k = (4/3) * tan(delta / 4)
        let k = (4.0 / 3.0) * (delta / 4.0).tan();

        // Control points on unit circle:
        // C1 = (cos(alpha) - k * sin(alpha), sin(alpha) + k * cos(alpha))
        // C2 = (cos(beta) + k * sin(beta), sin(beta) - k * cos(beta))
        // End point:
        // Pend = (cos(beta), sin(beta))
        let cos_a = alpha.cos();
        let sin_a = alpha.sin();
        let cos_b = beta.cos();
        let sin_b = beta.sin();

        let c1_x = cos_a - k * sin_a;
        let c1_y = sin_a + k * cos_a;
        let c2_x = cos_b + k * sin_b;
        let c2_y = sin_b - k * cos_b;

        let end_x = cos_b;
        let end_y = sin_b;

        // Scale by (wr, hr) and translate to center
        segments.push(PathSegment::CubicTo {
            x1: center_x + c1_x * wr,
            y1: center_y + c1_y * hr,
            x2: center_x + c2_x * wr,
            y2: center_y + c2_y * hr,
            x: center_x + end_x * wr,
            y: center_y + end_y * hr,
        });

        current_ang = beta;
    }
}
