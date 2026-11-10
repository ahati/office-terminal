use pptx_model::ChartModel;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorCommand, VectorPaint,
    VectorPath, VectorStroke,
};
use crate::compiler::shapes::arc::decompose_arc_to_cubic;
use super::color::resolve_slice_color;
use super::layout::{make_simple_run, ChartLayout};

pub fn compile_pie(
    chart: &ChartModel,
    layout: &ChartLayout,
    is_doughnut: bool,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();

    let first_series = match chart.series.first() {
        Some(s) => s,
        None => return commands,
    };

    let total: f64 = first_series
        .values
        .iter()
        .filter_map(|v| *v)
        .filter(|v| *v > 0.0)
        .sum();

    if total <= 0.0 {
        return commands;
    }

    let cx = layout.plot_x + layout.plot_w / 2.0;
    let cy = layout.plot_y + layout.plot_h / 2.0;
    let r_outer = (layout.plot_w.min(layout.plot_h) / 2.0) * 0.85;

    let r_inner = if is_doughnut {
        let hole_pct = chart.hole_size.unwrap_or(50).clamp(10, 90) as f32;
        r_outer * (hole_pct / 100.0)
    } else {
        0.0
    };

    let first_ang_deg = chart.first_slice_angle.unwrap_or(0) as f32;
    // DrawingML 0 deg is 12 o'clock; cartesian 0 deg is 3 o'clock with clockwise Y pointing down
    let mut current_ang_deg = first_ang_deg - 90.0;

    let slice_stroke = VectorStroke {
        color: [255, 255, 255, 255],
        width_emu: 19050, // 1.5 pt white border between slices
        dash_segments: None,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
    };

    for (idx, val_opt) in first_series.values.iter().enumerate() {
        let val = match val_opt {
            Some(v) if *v > 0.0 => *v,
            _ => continue,
        };

        let fraction = (val / total) as f32;
        let sweep_deg = (fraction * 360.0).min(359.99);
        let end_ang_deg = current_ang_deg + sweep_deg;

        let start_rad = current_ang_deg.to_radians();
        let end_rad = end_ang_deg.to_radians();

        let mut segs = Vec::new();

        if r_inner <= 0.0 {
            // Pie sector
            let p_start_x = cx + r_outer * start_rad.cos();
            let p_start_y = cy + r_outer * start_rad.sin();

            segs.push(PathSegment::MoveTo { x: cx, y: cy });
            segs.push(PathSegment::LineTo {
                x: p_start_x,
                y: p_start_y,
            });
            decompose_arc_to_cubic(cx, cy, r_outer, r_outer, current_ang_deg, sweep_deg, &mut segs);
            segs.push(PathSegment::Close);
        } else {
            // Doughnut sector
            let p_out_start_x = cx + r_outer * start_rad.cos();
            let p_out_start_y = cy + r_outer * start_rad.sin();
            let p_in_end_x = cx + r_inner * end_rad.cos();
            let p_in_end_y = cy + r_inner * end_rad.sin();

            segs.push(PathSegment::MoveTo {
                x: p_out_start_x,
                y: p_out_start_y,
            });
            decompose_arc_to_cubic(cx, cy, r_outer, r_outer, current_ang_deg, sweep_deg, &mut segs);
            segs.push(PathSegment::LineTo {
                x: p_in_end_x,
                y: p_in_end_y,
            });
            decompose_arc_to_cubic(cx, cy, r_inner, r_inner, end_ang_deg, -sweep_deg, &mut segs);
            segs.push(PathSegment::Close);
        }

        let slice_path = VectorPath::new(segs, FillRule::NonZero);
        let slice_color = resolve_slice_color(idx, Some(first_series));

        commands.push(VectorCommand::draw_path(
            slice_path,
            Some(VectorPaint::Solid(slice_color)),
            Some(slice_stroke.clone()),
            transform,
        ));

        // Slice label (only if slice is large enough)
        if sweep_deg >= 12.0 {
            let mid_ang_deg = current_ang_deg + sweep_deg / 2.0;
            let mid_rad = mid_ang_deg.to_radians();

            let r_label = if r_inner > 0.0 {
                (r_inner + r_outer) / 2.0
            } else {
                r_outer * 0.65
            };

            let lx = cx + r_label * mid_rad.cos();
            let ly = cy + r_label * mid_rad.sin() + 25_000.0;

            let label_text = format!("{:.0}%", fraction * 100.0);
            let run = make_simple_run(&label_text, 8.5, [255, 255, 255, 255], lx, ly, "center");
            commands.push(VectorCommand::DrawText {
                runs: vec![run],
                transform,
            });
        }

        current_ang_deg = end_ang_deg;
    }

    commands
}
