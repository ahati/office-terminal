use pptx_model::ChartModel;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, VectorCommand, VectorPath,
    VectorStroke,
};
use crate::compiler::style::parse_hex_color;
use super::layout::{axis_label_font_pt, make_simple_run, AxisTitleBand, ChartLayout};
use super::types::{get_category_label, ChartDataBounds, ChartFamily};

pub fn compile_axes(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    family: ChartFamily,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    if !family.is_cartesian() {
        return Vec::new();
    }

    let mut commands = Vec::new();
    let is_horizontal = family.is_horizontal_bar();

    let grid_color = chart
        .val_axis_gridline_color
        .as_deref()
        .map(parse_hex_color)
        .unwrap_or([210, 210, 210, 180]);

    let grid_stroke = VectorStroke {
        color: grid_color,
        width_emu: chart.val_axis_gridline_width_emu.unwrap_or(9525) as i64, // ~0.75 pt
        dash_segments: None,
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        paint: None,
    };

    let axis_line_color = [160, 160, 160, 255];
    let axis_stroke = VectorStroke {
        color: axis_line_color,
        width_emu: 12700, // 1 pt
        dash_segments: None,
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        paint: None,
    };

    // ref core/src/chart/shared/axis.ts:584 (`axisLabelPx`): the authored
    // `@sz` is hundredths of a point; absent values use the chart-height
    // fallback. Category and value axes resolve independently.
    let cat_font_pt = axis_label_font_pt(chart.cat_axis_font_size_hpt, layout.h);
    let val_font_pt = axis_label_font_pt(chart.val_axis_font_size_hpt, layout.h);
    let label_color = [80, 80, 80, 255];

    if is_horizontal {
        // Horizontal Bar Chart: Category on Y (left), Value on X (bottom)
        let show_val_grid = chart.val_axis_major_gridlines.unwrap_or(true);
        let span = (bounds.max_val - bounds.min_val).max(1e-6);

        // 1. Value gridlines & tick labels along bottom
        for &val in &bounds.nice_ticks {
            let norm = ((val - bounds.min_val) / span).clamp(0.0, 1.0) as f32;
            let vx = layout.plot_x + norm * layout.plot_w;

            if show_val_grid {
                let grid_path = VectorPath::new(
                    vec![
                        PathSegment::MoveTo {
                            x: vx,
                            y: layout.plot_y,
                        },
                        PathSegment::LineTo {
                            x: vx,
                            y: layout.plot_y + layout.plot_h,
                        },
                    ],
                    FillRule::NonZero,
                );
                commands.push(VectorCommand::draw_path(
                    grid_path,
                    None,
                    Some(grid_stroke.clone()),
                    transform,
                ));
            }

            if !chart.val_axis_hidden {
                let label_text = format_number_label(val, family.is_percent_stacked());
                let ty = layout.plot_y + layout.plot_h + 120_000.0;
                let run = make_simple_run(&label_text, val_font_pt, label_color, vx, ty, "center");
                commands.push(VectorCommand::DrawText {
                    runs: vec![run],
                    transform,
                });
            }
        }

        // 2. Category labels along left Y axis
        let num_cats = bounds.num_categories.max(1);
        let slot_h = layout.plot_h / (num_cats as f32);

        if !chart.cat_axis_hidden {
            for i in 0..num_cats {
                let cy = layout.plot_y + (i as f32 + 0.5) * slot_h;
                let cat_name = get_category_label(chart, i);
                let tx = layout.plot_x - 50_000.0;
                let run = make_simple_run(&cat_name, cat_font_pt, label_color, tx, cy, "right");
                commands.push(VectorCommand::DrawText {
                    runs: vec![run],
                    transform,
                });
            }
        }

        // 3. Category axis line (Y axis)
        if !chart.cat_axis_line_hidden {
            let axis_path = VectorPath::new(
                vec![
                    PathSegment::MoveTo {
                        x: layout.plot_x,
                        y: layout.plot_y,
                    },
                    PathSegment::LineTo {
                        x: layout.plot_x,
                        y: layout.plot_y + layout.plot_h,
                    },
                ],
                FillRule::NonZero,
            );
            commands.push(VectorCommand::draw_path(
                axis_path,
                None,
                Some(axis_stroke.clone()),
                transform,
            ));
        }

        // 4. Value axis line (X axis)
        if !chart.val_axis_line_hidden {
            let axis_path = VectorPath::new(
                vec![
                    PathSegment::MoveTo {
                        x: layout.plot_x,
                        y: layout.plot_y + layout.plot_h,
                    },
                    PathSegment::LineTo {
                        x: layout.plot_x + layout.plot_w,
                        y: layout.plot_y + layout.plot_h,
                    },
                ],
                FillRule::NonZero,
            );
            commands.push(VectorCommand::draw_path(
                axis_path,
                None,
                Some(axis_stroke),
                transform,
            ));
        }
    } else {
        // Vertical Chart (Column, Line, Area): Value on Y (left), Category on X (bottom)
        let show_val_grid = chart.val_axis_major_gridlines.unwrap_or(true);
        let span = (bounds.max_val - bounds.min_val).max(1e-6);

        // 1. Value gridlines & tick labels along Y axis
        for &val in &bounds.nice_ticks {
            let norm = ((val - bounds.min_val) / span).clamp(0.0, 1.0) as f32;
            let vy = layout.plot_y + layout.plot_h - norm * layout.plot_h;

            if show_val_grid {
                let grid_path = VectorPath::new(
                    vec![
                        PathSegment::MoveTo {
                            x: layout.plot_x,
                            y: vy,
                        },
                        PathSegment::LineTo {
                            x: layout.plot_x + layout.plot_w,
                            y: vy,
                        },
                    ],
                    FillRule::NonZero,
                );
                commands.push(VectorCommand::draw_path(
                    grid_path,
                    None,
                    Some(grid_stroke.clone()),
                    transform,
                ));
            }

            if !chart.val_axis_hidden {
                let label_text = format_number_label(val, family.is_percent_stacked());
                let tx = layout.plot_x - 50_000.0;
                let run = make_simple_run(&label_text, val_font_pt, label_color, tx, vy + 30_000.0, "right");
                commands.push(VectorCommand::DrawText {
                    runs: vec![run],
                    transform,
                });
            }
        }

        // 2. Category labels along bottom X axis
        let num_cats = bounds.num_categories.max(1);
        let slot_w = layout.plot_w / (num_cats as f32);

        if !chart.cat_axis_hidden {
            for i in 0..num_cats {
                let cx = layout.plot_x + (i as f32 + 0.5) * slot_w;
                let cat_name = get_category_label(chart, i);
                let ty = layout.plot_y + layout.plot_h + 140_000.0;
                let run = make_simple_run(&cat_name, cat_font_pt, label_color, cx, ty, "center");
                commands.push(VectorCommand::DrawText {
                    runs: vec![run],
                    transform,
                });
            }
        }

        // 3. Category axis baseline (X axis)
        if !chart.cat_axis_line_hidden {
            let axis_path = VectorPath::new(
                vec![
                    PathSegment::MoveTo {
                        x: layout.plot_x,
                        y: layout.plot_y + layout.plot_h,
                    },
                    PathSegment::LineTo {
                        x: layout.plot_x + layout.plot_w,
                        y: layout.plot_y + layout.plot_h,
                    },
                ],
                FillRule::NonZero,
            );
            commands.push(VectorCommand::draw_path(
                axis_path,
                None,
                Some(axis_stroke.clone()),
                transform,
            ));
        }

        // 4. Value axis baseline (Y axis)
        if !chart.val_axis_line_hidden {
            let axis_path = VectorPath::new(
                vec![
                    PathSegment::MoveTo {
                        x: layout.plot_x,
                        y: layout.plot_y,
                    },
                    PathSegment::LineTo {
                        x: layout.plot_x,
                        y: layout.plot_y + layout.plot_h,
                    },
                ],
                FillRule::NonZero,
            );
            commands.push(VectorCommand::draw_path(
                axis_path,
                None,
                Some(axis_stroke),
                transform,
            ));
        }
    }

    commands.extend(compile_axis_titles(layout, transform));

    commands
}

fn format_number_label(val: f64, is_percent: bool) -> String {
    if is_percent {
        return format!("{:.0}%", val);
    }
    if (val - val.round()).abs() < 1e-4 {
        format!("{:.0}", val.round())
    } else {
        format!("{:.1}", val)
    }
}

/// Paint the authored category / value axis titles.
///
/// ref core/src/chart/layout.ts:392-524: the category title is centred under
/// the plot, the value title is rotated bottom-to-top in the left gutter
/// (`axisTitleRotationRad('left') = -PI/2`). The run contract has no rotation,
/// so a stacked title is emitted as one glyph-per-line run, which the rasterizer
/// and the SVG exporter both paint vertically.
pub fn compile_axis_titles(layout: &ChartLayout, transform: AffineTransform) -> Vec<VectorCommand> {
    let mut commands = Vec::new();
    for band in [layout.cat_title.as_ref(), layout.val_title.as_ref()]
        .into_iter()
        .flatten()
    {
        if band.vertical {
            commands.extend(stacked_axis_title(band, transform));
        } else {
            commands.push(VectorCommand::DrawText {
                runs: vec![axis_title_run(band, band.text.clone(), band.x, band.y, "center")],
                transform,
            });
        }
    }
    commands
}

fn stacked_axis_title(band: &AxisTitleBand, transform: AffineTransform) -> Vec<VectorCommand> {
    let font = band.font_pt * 12700.0;
    let count = band.text.chars().count() as f32;
    // Centre the stack on the anchor, growing upward (bottom-to-top text).
    let first = band.y + (count - 1.0) * font / 2.0;
    band.text
        .chars()
        .enumerate()
        .map(|(idx, ch)| VectorCommand::DrawText {
            runs: vec![axis_title_run(
                band,
                ch.to_string(),
                band.x,
                first - idx as f32 * font,
                "center",
            )],
            transform,
        })
        .collect()
}

fn axis_title_run(
    band: &AxisTitleBand,
    text: String,
    x: f32,
    y: f32,
    alignment: &str,
) -> crate::contracts::PositionedGlyphRun {
    let mut run = make_simple_run(&text, band.font_pt, band.color, x, y, alignment);
    run.bold = band.bold;
    run.italic = band.italic;
    run
}
