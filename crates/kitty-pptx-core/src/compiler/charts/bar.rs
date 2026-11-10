use pptx_model::ChartModel;
use crate::contracts::{AffineTransform, VectorCommand, VectorPaint};
use super::color::resolve_series_color;
use super::layout::{make_simple_run, rect_path, ChartLayout};
use super::types::{ChartDataBounds, ChartFamily};

pub fn compile_bars(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    family: ChartFamily,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    if family.is_horizontal_bar() {
        compile_horizontal_bars(chart, layout, bounds, family, transform)
    } else {
        compile_vertical_columns(chart, layout, bounds, family, transform)
    }
}

fn compile_vertical_columns(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    family: ChartFamily,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();
    let num_cats = bounds.num_categories.max(1);
    let num_series = chart.series.len().max(1);
    let slot_w = layout.plot_w / (num_cats as f32);
    let span = (bounds.max_val - bounds.min_val).max(1e-6) as f32;

    let is_stacked = family.is_stacked();
    let is_pct = family.is_percent_stacked();

    if is_stacked {
        let bar_w = slot_w * 0.6;
        for c in 0..num_cats {
            let cat_x = layout.plot_x + (c as f32) * slot_w + (slot_w - bar_w) / 2.0;

            let cat_total: f64 = if is_pct {
                chart
                    .series
                    .iter()
                    .filter_map(|s| s.values.get(c).copied().flatten())
                    .filter(|v| *v > 0.0)
                    .sum()
            } else {
                1.0
            };

            let mut cum_y = layout.plot_y + layout.plot_h;

            for (s_idx, s) in chart.series.iter().enumerate() {
                let val = match s.values.get(c).copied().flatten() {
                    Some(v) if v > 0.0 => v,
                    _ => continue,
                };

                let norm_fraction = if is_pct {
                    if cat_total > 0.0 {
                        (val / cat_total) as f32
                    } else {
                        0.0
                    }
                } else {
                    val as f32 / span
                };

                let bar_h = (norm_fraction * layout.plot_h).max(1.0);
                let bar_y = cum_y - bar_h;

                let path = rect_path(cat_x, bar_y, bar_w, bar_h);
                let color = resolve_series_color(s_idx, s);

                commands.push(VectorCommand::draw_path(
                    path,
                    Some(VectorPaint::Solid(color)),
                    None,
                    transform,
                ));

                if chart.show_data_labels {
                    let lbl_text = if is_pct {
                        format!("{:.0}%", norm_fraction * 100.0)
                    } else {
                        format_val(val)
                    };
                    let tx = cat_x + bar_w / 2.0;
                    let ty = bar_y + bar_h / 2.0 + 30_000.0;
                    let run = make_simple_run(&lbl_text, 7.5, [255, 255, 255, 255], tx, ty, "center");
                    commands.push(VectorCommand::DrawText {
                        runs: vec![run],
                        transform,
                    });
                }

                cum_y = bar_y;
            }
        }
    } else {
        // Clustered columns
        let gap_ratio = 0.25f32;
        let inner_w = slot_w * (1.0 - gap_ratio);
        let bar_w = inner_w / (num_series as f32);

        for c in 0..num_cats {
            let cat_start_x = layout.plot_x + (c as f32) * slot_w + (slot_w * gap_ratio) / 2.0;

            for (s_idx, s) in chart.series.iter().enumerate() {
                let val = match s.values.get(c).copied().flatten() {
                    Some(v) => v,
                    None => continue,
                };

                let norm_h = (((val - bounds.min_val) / bounds.max_val.max(1e-6)) as f32).clamp(0.0, 1.0) * layout.plot_h;
                let bar_x = cat_start_x + (s_idx as f32) * bar_w;
                let bar_y = layout.plot_y + layout.plot_h - norm_h;

                let path = rect_path(bar_x, bar_y, bar_w, norm_h);
                let color = resolve_series_color(s_idx, s);

                commands.push(VectorCommand::draw_path(
                    path,
                    Some(VectorPaint::Solid(color)),
                    None,
                    transform,
                ));

                if chart.show_data_labels {
                    let lbl_text = format_val(val);
                    let tx = bar_x + bar_w / 2.0;
                    let ty = bar_y - 20_000.0;
                    let run = make_simple_run(&lbl_text, 7.5, [60, 60, 60, 255], tx, ty, "center");
                    commands.push(VectorCommand::DrawText {
                        runs: vec![run],
                        transform,
                    });
                }
            }
        }
    }

    commands
}

fn compile_horizontal_bars(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    family: ChartFamily,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();
    let num_cats = bounds.num_categories.max(1);
    let num_series = chart.series.len().max(1);
    let slot_h = layout.plot_h / (num_cats as f32);
    let span = (bounds.max_val - bounds.min_val).max(1e-6) as f32;

    let is_stacked = family.is_stacked();
    let is_pct = family.is_percent_stacked();

    if is_stacked {
        let bar_h = slot_h * 0.6;
        for c in 0..num_cats {
            let cat_y = layout.plot_y + (c as f32) * slot_h + (slot_h - bar_h) / 2.0;

            let cat_total: f64 = if is_pct {
                chart
                    .series
                    .iter()
                    .filter_map(|s| s.values.get(c).copied().flatten())
                    .filter(|v| *v > 0.0)
                    .sum()
            } else {
                1.0
            };

            let mut cum_x = layout.plot_x;

            for (s_idx, s) in chart.series.iter().enumerate() {
                let val = match s.values.get(c).copied().flatten() {
                    Some(v) if v > 0.0 => v,
                    _ => continue,
                };

                let norm_fraction = if is_pct {
                    if cat_total > 0.0 {
                        (val / cat_total) as f32
                    } else {
                        0.0
                    }
                } else {
                    val as f32 / span
                };

                let bar_w = (norm_fraction * layout.plot_w).max(1.0);
                let path = rect_path(cum_x, cat_y, bar_w, bar_h);
                let color = resolve_series_color(s_idx, s);

                commands.push(VectorCommand::draw_path(
                    path,
                    Some(VectorPaint::Solid(color)),
                    None,
                    transform,
                ));

                if chart.show_data_labels {
                    let lbl_text = if is_pct {
                        format!("{:.0}%", norm_fraction * 100.0)
                    } else {
                        format_val(val)
                    };
                    let tx = cum_x + bar_w / 2.0;
                    let ty = cat_y + bar_h / 2.0 + 30_000.0;
                    let run = make_simple_run(&lbl_text, 7.5, [255, 255, 255, 255], tx, ty, "center");
                    commands.push(VectorCommand::DrawText {
                        runs: vec![run],
                        transform,
                    });
                }

                cum_x += bar_w;
            }
        }
    } else {
        // Clustered horizontal bars
        let gap_ratio = 0.25f32;
        let inner_h = slot_h * (1.0 - gap_ratio);
        let bar_h = inner_h / (num_series as f32);

        for c in 0..num_cats {
            let cat_start_y = layout.plot_y + (c as f32) * slot_h + (slot_h * gap_ratio) / 2.0;

            for (s_idx, s) in chart.series.iter().enumerate() {
                let val = match s.values.get(c).copied().flatten() {
                    Some(v) => v,
                    None => continue,
                };

                let norm_w = (((val - bounds.min_val) / bounds.max_val.max(1e-6)) as f32).clamp(0.0, 1.0) * layout.plot_w;
                let bar_y = cat_start_y + (s_idx as f32) * bar_h;
                let bar_x = layout.plot_x;

                let path = rect_path(bar_x, bar_y, norm_w, bar_h);
                let color = resolve_series_color(s_idx, s);

                commands.push(VectorCommand::draw_path(
                    path,
                    Some(VectorPaint::Solid(color)),
                    None,
                    transform,
                ));

                if chart.show_data_labels {
                    let lbl_text = format_val(val);
                    let tx = bar_x + norm_w + 30_000.0;
                    let ty = bar_y + bar_h / 2.0 + 30_000.0;
                    let run = make_simple_run(&lbl_text, 7.5, [60, 60, 60, 255], tx, ty, "left");
                    commands.push(VectorCommand::DrawText {
                        runs: vec![run],
                        transform,
                    });
                }
            }
        }
    }

    commands
}

fn format_val(v: f64) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{:.0}", v.round())
    } else {
        format!("{:.1}", v)
    }
}
