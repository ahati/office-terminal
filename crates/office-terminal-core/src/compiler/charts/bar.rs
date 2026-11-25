use super::color::resolve_series_color;
use super::layout::{
    data_label_position, make_simple_run, rect_path, series_data_label_font_pt, series_shows_percent_labels,
    series_shows_value_labels, ChartLayout,
};
use super::types::{ChartDataBounds, ChartFamily};
use crate::contracts::{AffineTransform, VectorCommand, VectorPaint, EMU_PER_PT};
use pptx_model::ChartModel;

/// Bar cluster geometry resolved from the chart group's `gapWidth`/`overlap`.
///
/// ref core/src/chart/families/bar.ts:1289-1333 (`clusterGeometry`):
/// ECMA-376 §21.2.2.13 `gapWidth` is the gap between categories as a percent
/// of one bar width (default 150) and §21.2.2.25 `overlap` is the signed
/// percent by which bars inside a cluster overlap (default 0, clamped to
/// -100..100). Stacked charts render one bar per category (`effective = 1`)
/// with `overlap = 0`, but the authored gap width still applies.
#[derive(Clone, Copy, Debug)]
struct ClusterGeometry {
    /// Bar thickness: width for columns, height for bars.
    bar_w: f32,
    /// Pitch between consecutive bars inside a cluster.
    cluster_gap: f32,
    /// Offset of the cluster from the start of its category slot.
    cat_start: f32,
}

fn cluster_geometry(chart: &ChartModel, num_series: usize, is_stacked: bool, category_size: f32) -> ClusterGeometry {
    let effective = if is_stacked { 1.0 } else { num_series.max(1) as f32 };
    let owner = chart.series.first();
    let raw_overlap = owner
        .and_then(|s| s.bar_group_overlap)
        .or(chart.bar_overlap)
        .unwrap_or(0) as f32;
    let overlap_pct = if is_stacked {
        0.0
    } else {
        raw_overlap.clamp(-100.0, 100.0)
    };
    let authored_gap = owner.and_then(|s| s.bar_group_gap_width).or(chart.bar_gap_width);
    // ref category-spacing.ts:126-134 (`resolveCategoryGapWidthPercent`):
    // legacy policy default 150, authored values clamped to 0..500.
    let gap_width_pct = authored_gap.map_or(150.0, |gap| (gap as f32).clamp(0.0, 500.0));
    let denom = 1.0 + (effective - 1.0) * (1.0 - overlap_pct / 100.0) + gap_width_pct / 100.0;
    let bar_w = category_size / denom;
    let cluster_gap = if is_stacked {
        0.0
    } else {
        bar_w * (1.0 - overlap_pct / 100.0)
    };
    let cluster_width = bar_w + (effective - 1.0) * cluster_gap;
    ClusterGeometry {
        bar_w,
        cluster_gap,
        cat_start: (category_size - cluster_width) / 2.0,
    }
}

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
        let geom = cluster_geometry(chart, 1, true, slot_w);
        let bar_w = geom.bar_w;
        for c in 0..num_cats {
            let cat_x = layout.plot_x + (c as f32) * slot_w + geom.cat_start;

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
                let label_pt = series_data_label_font_pt(chart, s, layout.plot_h);

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

                if if is_pct {
                    series_shows_percent_labels(chart, s)
                } else {
                    series_shows_value_labels(chart, s)
                } {
                    let lbl_text = if is_pct {
                        format!("{:.0}%", norm_fraction * 100.0)
                    } else {
                        format_val(val)
                    };
                    let pos = data_label_position(chart, s, "ctr");
                    let tx = cat_x + bar_w / 2.0;
                    let ty = vertical_label_baseline(bar_y, bar_y + bar_h, label_pt * EMU_PER_PT, &pos);
                    let run = make_simple_run(&lbl_text, label_pt, [255, 255, 255, 255], tx, ty, "center");
                    commands.push(VectorCommand::DrawText {
                        runs: vec![run],
                        transform,
                    });
                }

                cum_y = bar_y;
            }
        }
    } else {
        // Clustered columns: ref bar.ts:1289-1333 cluster geometry.
        let geom = cluster_geometry(chart, num_series, false, slot_w);
        let bar_w = geom.bar_w;

        for c in 0..num_cats {
            let cat_start_x = layout.plot_x + (c as f32) * slot_w + geom.cat_start;

            for (s_idx, s) in chart.series.iter().enumerate() {
                let val = match s.values.get(c).copied().flatten() {
                    Some(v) => v,
                    None => continue,
                };
                let label_pt = series_data_label_font_pt(chart, s, layout.plot_h);

                let norm_h =
                    (((val - bounds.min_val) / bounds.max_val.max(1e-6)) as f32).clamp(0.0, 1.0) * layout.plot_h;
                let bar_x = cat_start_x + (s_idx as f32) * geom.cluster_gap;
                let bar_y = layout.plot_y + layout.plot_h - norm_h;

                let path = rect_path(bar_x, bar_y, bar_w, norm_h);
                let color = resolve_series_color(s_idx, s);

                commands.push(VectorCommand::draw_path(
                    path,
                    Some(VectorPaint::Solid(color)),
                    None,
                    transform,
                ));

                if series_shows_value_labels(chart, s) {
                    let lbl_text = format_val(val);
                    let pos = data_label_position(chart, s, "outEnd");
                    let tx = bar_x + bar_w / 2.0;
                    let ty = vertical_label_baseline(bar_y, bar_y + norm_h, label_pt * EMU_PER_PT, &pos);
                    let run = make_simple_run(&lbl_text, label_pt, [60, 60, 60, 255], tx, ty, "center");
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
        let geom = cluster_geometry(chart, 1, true, slot_h);
        let bar_h = geom.bar_w;
        for c in 0..num_cats {
            let cat_y = layout.plot_y + (c as f32) * slot_h + geom.cat_start;

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
                let label_pt = series_data_label_font_pt(chart, s, layout.plot_h);

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

                if if is_pct {
                    series_shows_percent_labels(chart, s)
                } else {
                    series_shows_value_labels(chart, s)
                } {
                    let lbl_text = if is_pct {
                        format!("{:.0}%", norm_fraction * 100.0)
                    } else {
                        format_val(val)
                    };
                    let pos = data_label_position(chart, s, "ctr");
                    let (tx, align) = horizontal_label_anchor(cum_x, cum_x + bar_w, label_pt * EMU_PER_PT, &pos);
                    let ty = cat_y + bar_h / 2.0 + label_pt * EMU_PER_PT * 0.35;
                    let run = make_simple_run(&lbl_text, label_pt, [255, 255, 255, 255], tx, ty, align);
                    commands.push(VectorCommand::DrawText {
                        runs: vec![run],
                        transform,
                    });
                }

                cum_x += bar_w;
            }
        }
    } else {
        // Clustered horizontal bars: ref bar.ts:1289-1333 cluster geometry.
        let geom = cluster_geometry(chart, num_series, false, slot_h);
        let bar_h = geom.bar_w;

        for c in 0..num_cats {
            let cat_start_y = layout.plot_y + (c as f32) * slot_h + geom.cat_start;

            for (s_idx, s) in chart.series.iter().enumerate() {
                let val = match s.values.get(c).copied().flatten() {
                    Some(v) => v,
                    None => continue,
                };
                let label_pt = series_data_label_font_pt(chart, s, layout.plot_h);

                let norm_w =
                    (((val - bounds.min_val) / bounds.max_val.max(1e-6)) as f32).clamp(0.0, 1.0) * layout.plot_w;
                let bar_y = cat_start_y + (s_idx as f32) * geom.cluster_gap;
                let bar_x = layout.plot_x;

                let path = rect_path(bar_x, bar_y, norm_w, bar_h);
                let color = resolve_series_color(s_idx, s);

                commands.push(VectorCommand::draw_path(
                    path,
                    Some(VectorPaint::Solid(color)),
                    None,
                    transform,
                ));

                if series_shows_value_labels(chart, s) {
                    let lbl_text = format_val(val);
                    let pos = data_label_position(chart, s, "outEnd");
                    let (tx, align) = horizontal_label_anchor(bar_x, bar_x + norm_w, label_pt * EMU_PER_PT, &pos);
                    let ty = bar_y + bar_h / 2.0 + label_pt * EMU_PER_PT * 0.35;
                    let run = make_simple_run(&lbl_text, label_pt, [60, 60, 60, 255], tx, ty, align);
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

/// Vertical-bar data-label baseline for an authored `dLblPos`.
///
/// ref core/src/chart/data-label-layout.ts:140-190 (`anchor.kind === 'bar'`):
/// `outEnd` sits one half-em outside the bar end, `inEnd`/`inBase` inside with
/// the same inset, `ctr` at the bar centre. The half-em of each box half is
/// folded into the baseline (0.35 em above the box centre).
fn vertical_label_baseline(top: f32, bottom: f32, font_emu: f32, pos: &str) -> f32 {
    match pos {
        "inBase" => bottom - font_emu * 0.65,
        "inEnd" => top + font_emu * 1.35,
        "ctr" => (top + bottom) / 2.0 + font_emu * 0.35,
        _ => top - font_emu * 0.65,
    }
}

/// Horizontal-bar data-label anchor for an authored `dLblPos` (same rule).
fn horizontal_label_anchor(left: f32, right: f32, font_emu: f32, pos: &str) -> (f32, &'static str) {
    match pos {
        "inBase" => (left + font_emu * 0.5, "left"),
        "inEnd" => (right - font_emu * 0.5, "right"),
        "ctr" => ((left + right) / 2.0, "center"),
        _ => (right + font_emu * 0.5, "left"),
    }
}

fn format_val(v: f64) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{:.0}", v.round())
    } else {
        format!("{:.1}", v)
    }
}
