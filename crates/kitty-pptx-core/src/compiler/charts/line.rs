use pptx_model::ChartModel;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorCommand, VectorPath,
    VectorStroke,
};
use crate::compiler::shapes::arc::decompose_arc_to_cubic;
use super::color::{resolve_series_color, resolve_series_line_color};
use super::layout::{make_simple_run, ChartLayout};
use super::types::ChartDataBounds;

pub fn compile_lines(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();
    let num_cats = bounds.num_categories.max(1);
    let slot_w = layout.plot_w / (num_cats as f32);
    let span = (bounds.max_val - bounds.min_val).max(1e-6) as f32;

    for (s_idx, s) in chart.series.iter().enumerate() {
        let mut points: Vec<(f32, f32, f64)> = Vec::new();

        for c in 0..num_cats {
            let cx = layout.plot_x + (c as f32 + 0.5) * slot_w;
            if let Some(val) = s.values.get(c).copied().flatten() {
                let norm = (((val - bounds.min_val) as f32) / span).clamp(0.0, 1.0);
                let cy = layout.plot_y + layout.plot_h - norm * layout.plot_h;
                points.push((cx, cy, val));
            }
        }

        if points.is_empty() {
            continue;
        }

        let line_color = resolve_series_line_color(s_idx, s);
        let stroke_w = s.line_width_emu.unwrap_or(25400); // 2 pt default

        // 1. Draw polyline path
        let mut segs = Vec::with_capacity(points.len());
        for (i, &(px, py, _)) in points.iter().enumerate() {
            if i == 0 {
                segs.push(PathSegment::MoveTo { x: px, y: py });
            } else {
                segs.push(PathSegment::LineTo { x: px, y: py });
            }
        }

        let line_path = VectorPath::new(segs, FillRule::NonZero);
        let stroke = VectorStroke {
            color: line_color,
            width_emu: stroke_w as i64,
            dash_segments: None,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
        };

        commands.push(VectorCommand::draw_path(
            line_path,
            None,
            Some(stroke),
            transform,
        ));

        // 2. Draw point markers
        let show_markers = s.show_marker.unwrap_or(true);
        let marker_r = 3.5 * 12700.0; // 3.5 pt
        let marker_color = resolve_series_color(s_idx, s);

        if show_markers {
            let marker_stroke = VectorStroke {
                color: [255, 255, 255, 255],
                width_emu: 12700, // 1 pt white border
                dash_segments: None,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
            };

            for &(px, py, _) in &points {
                let mut marker_segs = Vec::new();
                marker_segs.push(PathSegment::MoveTo {
                    x: px + marker_r,
                    y: py,
                });
                decompose_arc_to_cubic(px, py, marker_r, marker_r, 0.0, 360.0, &mut marker_segs);
                marker_segs.push(PathSegment::Close);

                let marker_path = VectorPath::new(marker_segs, FillRule::NonZero);
                commands.push(VectorCommand::draw_path(
                    marker_path,
                    Some(crate::contracts::VectorPaint::Solid(marker_color)),
                    Some(marker_stroke.clone()),
                    transform,
                ));
            }
        }

        // 3. Optional data labels
        if chart.show_data_labels {
            for &(px, py, val) in &points {
                let label_text = if (val - val.round()).abs() < 1e-4 {
                    format!("{:.0}", val.round())
                } else {
                    format!("{:.1}", val)
                };
                let run = make_simple_run(
                    &label_text,
                    7.5,
                    [60, 60, 60, 255],
                    px,
                    py - marker_r - 20_000.0,
                    "center",
                );
                commands.push(VectorCommand::DrawText {
                    runs: vec![run],
                    transform,
                });
            }
        }
    }

    commands
}
