use super::color::{resolve_series_color, resolve_series_line_color, with_alpha};
use super::layout::ChartLayout;
use super::types::{ChartDataBounds, ChartFamily};
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorCommand, VectorPaint, VectorPath, VectorStroke,
};
use pptx_model::ChartModel;

pub fn compile_area(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    family: ChartFamily,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();
    let num_cats = bounds.num_categories.max(1);
    let slot_w = layout.plot_w / (num_cats as f32);
    let span = (bounds.max_val - bounds.min_val).max(1e-6) as f32;
    let base_y = layout.plot_y + layout.plot_h;

    let is_stacked = family.is_stacked();
    let mut prev_cum_y = vec![base_y; num_cats];

    #[allow(clippy::needless_range_loop)]
    for (s_idx, s) in chart.series.iter().enumerate() {
        let mut curr_y = Vec::with_capacity(num_cats);
        let mut xs = Vec::with_capacity(num_cats);

        for (c, &prev_y) in prev_cum_y.iter().enumerate().take(num_cats) {
            let cx = layout.plot_x + (c as f32 + 0.5) * slot_w;
            xs.push(cx);

            let val = s.values.get(c).copied().flatten().unwrap_or(0.0);
            let norm_h = ((val as f32) / span).clamp(0.0, 1.0) * layout.plot_h;

            let y = if is_stacked { prev_y - norm_h } else { base_y - norm_h };
            curr_y.push(y);
        }

        if xs.is_empty() {
            continue;
        }

        let series_color = resolve_series_color(s_idx, s);
        let fill_color = with_alpha(series_color, 0.65);
        let line_color = resolve_series_line_color(s_idx, s);

        // 1. Polygon path
        let mut segs = Vec::new();
        // Top edge: left to right
        segs.push(PathSegment::MoveTo { x: xs[0], y: curr_y[0] });
        for i in 1..num_cats {
            segs.push(PathSegment::LineTo { x: xs[i], y: curr_y[i] });
        }

        // Bottom edge: right to left
        if is_stacked {
            for i in (0..num_cats).rev() {
                segs.push(PathSegment::LineTo {
                    x: xs[i],
                    y: prev_cum_y[i],
                });
            }
        } else {
            segs.push(PathSegment::LineTo {
                x: xs[num_cats - 1],
                y: base_y,
            });
            segs.push(PathSegment::LineTo { x: xs[0], y: base_y });
        }
        segs.push(PathSegment::Close);

        let area_path = VectorPath::new(segs, FillRule::NonZero);
        commands.push(VectorCommand::draw_path(
            area_path,
            Some(VectorPaint::Solid(fill_color)),
            None,
            transform,
        ));

        // 2. Outline stroke along top edge
        let mut top_segs = Vec::new();
        top_segs.push(PathSegment::MoveTo { x: xs[0], y: curr_y[0] });
        for i in 1..num_cats {
            top_segs.push(PathSegment::LineTo { x: xs[i], y: curr_y[i] });
        }

        let top_stroke = VectorStroke {
            color: line_color,
            width_emu: 19050, // 1.5 pt
            dash_segments: None,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            paint: None,
            ..VectorStroke::default()
        };

        commands.push(VectorCommand::draw_path(
            VectorPath::new(top_segs, FillRule::NonZero),
            None,
            Some(top_stroke),
            transform,
        ));

        prev_cum_y = curr_y;
    }

    commands
}
