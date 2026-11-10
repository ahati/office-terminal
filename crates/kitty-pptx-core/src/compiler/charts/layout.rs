use pptx_model::ChartModel;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, PositionedGlyph, PositionedGlyphRun,
    VectorCommand, VectorPaint, VectorPath, VectorStroke,
};
use crate::compiler::style::parse_hex_color;
use super::color::{resolve_series_color, resolve_slice_color};
use super::types::{get_category_label, ChartFamily};

#[derive(Clone, Debug)]
pub struct ChartLayout {
    pub w: f32,
    pub h: f32,
    pub plot_x: f32,
    pub plot_y: f32,
    pub plot_w: f32,
    pub plot_h: f32,
    pub title_box: Option<(f32, f32, f32, f32)>,
    pub legend_box: Option<(f32, f32, f32, f32)>,
    pub legend_pos: String,
    pub show_legend: bool,
}

pub fn compute_chart_layout(
    chart: &ChartModel,
    w: f32,
    h: f32,
    family: ChartFamily,
) -> ChartLayout {
    let has_title = chart.title.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)
        || chart.title_present;

    let show_legend = chart.show_legend || chart.legend_pos.is_some();
    let legend_pos = chart.legend_pos.as_deref().unwrap_or("r").to_lowercase();

    // Check manual layout for plot area
    if let Some(ref ml) = chart.plot_area_manual_layout {
        let mx = ml.x as f32 * w;
        let my = ml.y as f32 * h;
        let mw = ml.w.unwrap_or(0.8) as f32 * w;
        let mh = ml.h.unwrap_or(0.7) as f32 * h;

        if mw > 10.0 && mh > 10.0 {
            let title_box = if has_title {
                Some((0.0, 0.0, w, (my).max(0.08 * h)))
            } else {
                None
            };
            return ChartLayout {
                w,
                h,
                plot_x: mx,
                plot_y: my,
                plot_w: mw,
                plot_h: mh,
                title_box,
                legend_box: None,
                legend_pos,
                show_legend,
            };
        }
    }

    let title_height = if has_title {
        (0.12 * h).max(250_000.0)
    } else {
        0.02 * h
    };

    let title_box = if has_title {
        Some((0.0, 0.02 * h, w, title_height))
    } else {
        None
    };

    let mut left_margin = 0.04 * w;
    let mut right_margin = 0.04 * w;
    let mut top_margin = title_height + 0.03 * h;
    let mut bottom_margin = 0.04 * h;

    // Allocate space for legend
    let mut legend_box = None;
    if show_legend {
        match legend_pos.as_str() {
            "b" => {
                let leg_h = 0.12 * h;
                legend_box = Some((0.05 * w, h - leg_h, 0.9 * w, leg_h));
                bottom_margin += leg_h;
            }
            "t" => {
                let leg_h = 0.10 * h;
                legend_box = Some((0.05 * w, top_margin, 0.9 * w, leg_h));
                top_margin += leg_h;
            }
            "l" => {
                let leg_w = 0.20 * w;
                legend_box = Some((0.02 * w, top_margin, leg_w, h - top_margin - bottom_margin));
                left_margin += leg_w;
            }
            _ => {
                // "r" or "tr" default
                let leg_w = 0.22 * w;
                legend_box = Some((w - leg_w, top_margin, leg_w, h - top_margin - bottom_margin));
                right_margin += leg_w;
            }
        }
    }

    // Allocate margins for Cartesian axes
    if family.is_cartesian() {
        if family.is_horizontal_bar() {
            left_margin += 0.16 * w;
            bottom_margin += 0.08 * h;
        } else {
            left_margin += 0.10 * w;
            bottom_margin += 0.09 * h;
        }
    }

    let plot_x = left_margin;
    let plot_y = top_margin;
    let plot_w = (w - left_margin - right_margin).max(50.0);
    let plot_h = (h - top_margin - bottom_margin).max(50.0);

    ChartLayout {
        w,
        h,
        plot_x,
        plot_y,
        plot_w,
        plot_h,
        title_box,
        legend_box,
        legend_pos,
        show_legend,
    }
}

pub fn compile_chart_background(
    chart: &ChartModel,
    layout: &ChartLayout,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();

    // Chart-area background
    if let Some(ref bg) = chart.chart_bg {
        let path = rect_path(0.0, 0.0, layout.w, layout.h);
        commands.push(VectorCommand::draw_path(
            path,
            Some(VectorPaint::Solid(parse_hex_color(bg))),
            None,
            transform,
        ));
    }

    // Chart-area border
    if let Some(ref border_color) = chart.chart_border_color {
        let path = rect_path(0.0, 0.0, layout.w, layout.h);
        let stroke = VectorStroke {
            color: parse_hex_color(border_color),
            width_emu: chart.chart_border_width_emu.unwrap_or(12700) as i64,
            dash_segments: None,
            line_cap: LineCap::Square,
            line_join: LineJoin::Miter,
        };
        commands.push(VectorCommand::draw_path(
            path,
            None,
            Some(stroke),
            transform,
        ));
    }

    // Plot-area background
    if let Some(ref bg) = chart.plot_area_bg {
        let path = rect_path(layout.plot_x, layout.plot_y, layout.plot_w, layout.plot_h);
        commands.push(VectorCommand::draw_path(
            path,
            Some(VectorPaint::Solid(parse_hex_color(bg))),
            None,
            transform,
        ));
    }

    commands
}

pub fn compile_chart_title(
    chart: &ChartModel,
    layout: &ChartLayout,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let title_text = match chart.title.as_deref() {
        Some(t) if !t.trim().is_empty() => t.trim(),
        _ => return Vec::new(),
    };

    let (_tx, ty, tw, th) = match layout.title_box {
        Some(b) => b,
        None => return Vec::new(),
    };

    let font_size_pt = chart
        .title_font_size_hpt
        .map(|hpt| (hpt as f32) / 2.0)
        .unwrap_or(14.0);

    let color = chart
        .title_font_color
        .as_deref()
        .map(parse_hex_color)
        .unwrap_or([40, 40, 40, 255]);

    let cx = tw / 2.0;
    let cy = ty + th * 0.6;
    let run = make_simple_run(title_text, font_size_pt, color, cx, cy, "center");

    vec![VectorCommand::DrawText {
        runs: vec![run],
        transform,
    }]
}

pub fn compile_chart_legend(
    chart: &ChartModel,
    layout: &ChartLayout,
    family: ChartFamily,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    if !layout.show_legend {
        return Vec::new();
    }

    let (lx, ly, lw, lh) = match layout.legend_box {
        Some(b) => b,
        None => return Vec::new(),
    };

    let is_pie_family = matches!(family, ChartFamily::Pie | ChartFamily::Doughnut);
    let items_count = if is_pie_family {
        chart.categories.len().max(
            chart.series.first().map(|s| s.values.len()).unwrap_or(0),
        )
    } else {
        chart.series.len()
    };

    if items_count == 0 {
        return Vec::new();
    }

    let mut commands = Vec::new();
    let font_size_pt = 9.5;
    let swatch_size = font_size_pt * 12700.0 * 0.75;
    let text_color = [60, 60, 60, 255];

    let is_horizontal = layout.legend_pos == "b" || layout.legend_pos == "t";

    if is_horizontal {
        let item_w = (lw / (items_count as f32)).max(swatch_size * 4.0);
        for idx in 0..items_count {
            let item_x = lx + (idx as f32) * item_w;
            let item_y = ly + lh * 0.5 - swatch_size * 0.5;

            let (label, color) = if is_pie_family {
                let lbl = get_category_label(chart, idx);
                let col = resolve_slice_color(idx, chart.series.first());
                (lbl, col)
            } else {
                let s = &chart.series[idx];
                let lbl = if s.name.trim().is_empty() {
                    format!("Series {}", idx + 1)
                } else {
                    s.name.clone()
                };
                let col = resolve_series_color(idx, s);
                (lbl, col)
            };

            // Swatch rect
            let swatch_path = rect_path(item_x, item_y, swatch_size, swatch_size);
            commands.push(VectorCommand::draw_path(
                swatch_path,
                Some(VectorPaint::Solid(color)),
                None,
                transform,
            ));

            // Swatch label
            let tx = item_x + swatch_size * 1.4;
            let ty = item_y + swatch_size * 0.85;
            let run = make_simple_run(&label, font_size_pt, text_color, tx, ty, "left");
            commands.push(VectorCommand::DrawText {
                runs: vec![run],
                transform,
            });
        }
    } else {
        let row_h = (font_size_pt * 12700.0 * 1.6).max(swatch_size * 1.5);
        for idx in 0..items_count {
            let item_x = lx + 0.02 * layout.w;
            let item_y = ly + (idx as f32) * row_h;

            if item_y + row_h > ly + lh + 1000.0 {
                break; // Stop if overflow
            }

            let (label, color) = if is_pie_family {
                let lbl = get_category_label(chart, idx);
                let col = resolve_slice_color(idx, chart.series.first());
                (lbl, col)
            } else {
                let s = &chart.series[idx];
                let lbl = if s.name.trim().is_empty() {
                    format!("Series {}", idx + 1)
                } else {
                    s.name.clone()
                };
                let col = resolve_series_color(idx, s);
                (lbl, col)
            };

            // Swatch
            let swatch_path = rect_path(item_x, item_y, swatch_size, swatch_size);
            commands.push(VectorCommand::draw_path(
                swatch_path,
                Some(VectorPaint::Solid(color)),
                None,
                transform,
            ));

            // Text
            let tx = item_x + swatch_size * 1.4;
            let ty = item_y + swatch_size * 0.85;
            let run = make_simple_run(&label, font_size_pt, text_color, tx, ty, "left");
            commands.push(VectorCommand::DrawText {
                runs: vec![run],
                transform,
            });
        }
    }

    commands
}

pub fn rect_path(x: f32, y: f32, w: f32, h: f32) -> VectorPath {
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x, y },
            PathSegment::LineTo { x: x + w, y },
            PathSegment::LineTo { x: x + w, y: y + h },
            PathSegment::LineTo { x, y: y + h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    )
}

pub fn make_simple_run(
    text: &str,
    sz_pt: f32,
    color: [u8; 4],
    x_pos: f32,
    y_pos: f32,
    alignment: &str,
) -> PositionedGlyphRun {
    let sz_emu = sz_pt * 12700.0;
    let mut glyphs = Vec::new();

    let total_width: f32 = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_punctuation() || ch == ' ' {
                sz_emu * 0.3
            } else if ch.is_ascii_digit() || ch.is_ascii_uppercase() {
                sz_emu * 0.6
            } else if ch.is_ascii_lowercase() {
                sz_emu * 0.5
            } else {
                sz_emu * 0.9
            }
        })
        .sum();

    let start_x = match alignment {
        "center" => x_pos - (total_width / 2.0),
        "right" => x_pos - total_width,
        _ => x_pos,
    };

    let mut cur_x = start_x;
    for (i, ch) in text.chars().enumerate() {
        let gw = if ch.is_ascii_punctuation() || ch == ' ' {
            sz_emu * 0.3
        } else if ch.is_ascii_digit() || ch.is_ascii_uppercase() {
            sz_emu * 0.6
        } else if ch.is_ascii_lowercase() {
            sz_emu * 0.5
        } else {
            sz_emu * 0.9
        };

        glyphs.push(PositionedGlyph {
            glyph_id: i as u16,
            x: cur_x,
            y: y_pos,
            advance: gw,
        });
        cur_x += gw;
    }

    PositionedGlyphRun {
        text: text.to_string(),
        font_family: "Carlito".to_string(),
        font_size_pt: sz_pt,
        color,
        glyphs,
        underline: false,
        strikethrough: false,
    }
}
