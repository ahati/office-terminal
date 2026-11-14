//! Shared chart frame: measured title / legend / axis-title bands and the plot
//! rectangle, plus the band painters.
//!
//! Reference: `/tmp/oooxml/packages/core/src/chart/layout.ts` (`computeChartFrame`,
//! 700-799) and `packages/core/src/chart/shared/title.ts` / `shared/legend.ts`
//! for the measured bands (see [`super::bands`]).

use ooxml_common::chart::{ChartManualLayout, ChartSeries};
use pptx_model::ChartModel;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, PositionedGlyph, PositionedGlyphRun,
    VectorCommand, VectorPaint, VectorPath, VectorStroke,
};
use crate::compiler::style::parse_hex_color;
use super::types::ChartFamily;

pub use super::bands::{
    axis_label_font_pt, axis_title_font_pt, axis_title_margin, chart_text_font_pt, elide_to_width,
    legend_bands, legend_font_pt, make_axis_title, measure_legend, pack_rows,
    series_data_label_font_pt, text_width, title_band, AxisTitleBand, ChartTitleBand, LegendItem,
    LegendReserve, CHART_FONT, EMU_PER_PT,
};

/// EMU per CSS pixel at 96 dpi.
const EMU_PER_PX: f32 = 9525.0;
/// Legend side inset and per-side horizontal padding
/// (ref core/src/chart/shared/legend.ts:836-848).
const LEGEND_SIDE_PADDING: f32 = 8.0 * EMU_PER_PX;
const LEGEND_HORIZONTAL_INSET: f32 = 4.0 * EMU_PER_PX;
const LEGEND_HORIZONTAL_PADDING: f32 = 8.0 * EMU_PER_PX;
const LEGEND_SWATCH_TEXT_GAP: f32 = 4.0 * EMU_PER_PX;
const LEGEND_ITEM_GAP: f32 = 12.0 * EMU_PER_PX;
/// Radial gap below the title/legend before centring the plot
/// (ref core/src/chart/families/pie.ts:310-315).
const RADIAL_GAP_FRAC: f32 = 0.02;

#[derive(Clone, Debug)]
pub struct ChartLayout {
    pub w: f32,
    pub h: f32,
    pub plot_x: f32,
    pub plot_y: f32,
    pub plot_w: f32,
    pub plot_h: f32,
    pub title_box: Option<(f32, f32, f32, f32)>,
    pub title: ChartTitleBand,
    pub legend_box: Option<(f32, f32, f32, f32)>,
    pub legend_pos: String,
    pub show_legend: bool,
    pub legend: Option<LegendReserve>,
    pub cat_title: Option<AxisTitleBand>,
    pub val_title: Option<AxisTitleBand>,
    /// An authored `<c:plotArea><c:manualLayout>` positioned the plot rect.
    pub manual_layout_applied: bool,
}

/// Resolve an authored manual layout, ref core/src/chart/layout.ts:674-702
/// (`resolveManualLayoutRect`): factor dimensions are chart-space fractions of
/// the frame, edge dimensions are right/bottom coordinates. A null `w`/`h`
/// keeps the default extent.
fn resolve_manual_rect(
    ml: &ChartManualLayout,
    chart: (f32, f32, f32, f32),
    default: (f32, f32, f32, f32),
) -> Option<(f32, f32, f32, f32)> {
    let edge = |mode: &str| mode.eq_ignore_ascii_case("edge");
    let (cx, cy, cw, ch) = chart;
    let (dx, dy, dw, dh) = default;
    let x = if edge(&ml.x_mode) { cx + ml.x as f32 * cw } else { dx + ml.x as f32 * cw };
    let y = if edge(&ml.y_mode) { cy + ml.y as f32 * ch } else { dy + ml.y as f32 * ch };
    let w = match ml.w {
        None => dw,
        Some(v) if edge(&ml.w_mode) => cx + v as f32 * cw - x,
        Some(v) => v as f32 * cw,
    };
    let h = match ml.h {
        None => dh,
        Some(v) if edge(&ml.h_mode) => cy + v as f32 * ch - y,
        Some(v) => v as f32 * ch,
    };
    if w <= 0.0 || h <= 0.0 || !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
        return None;
    }
    Some((x, y, w, h))
}

pub fn compute_chart_layout(chart: &ChartModel, w: f32, h: f32, family: ChartFamily) -> ChartLayout {
    let w = w.max(1.0);
    let h = h.max(1.0);
    let legend = measure_legend(chart, w, h, family);
    let (leg_l, leg_r, leg_t, leg_b) =
        legend_bands(legend.as_ref(), chart.legend_overlay == Some(true));
    let mut title = title_band(chart, h, family);

    let cartesian = family.is_cartesian();
    let is_h = family.is_horizontal_bar();
    // Base gutters keep the families' previous policy fractions: the tick-label
    // gutter plus the 4 % frame pads. The measured title/legend/axis-title bands
    // are added on top (ref layout.ts:730-760 builds `pad` the same way).
    let axis_left = if !cartesian {
        0.0
    } else if is_h {
        0.16 * w
    } else {
        0.10 * w
    };
    let axis_bottom = if !cartesian {
        0.0
    } else if is_h {
        0.08 * h
    } else {
        0.09 * h
    };
    let mut left = 0.04 * w + axis_left + leg_l;
    let top = if title.is_empty() {
        0.02 * h
    } else {
        title.band_h + 0.03 * h
    } + leg_t;
    let right = 0.04 * w + leg_r;
    let mut bottom = 0.04 * h + axis_bottom + leg_b;

    let cat_title = make_axis_title(
        chart.cat_axis_title.as_ref(),
        chart.cat_axis_title_font_size_hpt,
        chart.cat_axis_title_font_color.as_deref(),
        chart.cat_axis_title_font_bold,
        chart.cat_axis_title_font_italic,
        chart.cat_axis_title_text_vertical_inset_emu,
        h,
    );
    let val_title = make_axis_title(
        chart.val_axis_title.as_ref(),
        chart.val_axis_title_font_size_hpt,
        chart.val_axis_title_font_color.as_deref(),
        chart.val_axis_title_font_bold,
        chart.val_axis_title_font_italic,
        chart.val_axis_title_text_vertical_inset_emu,
        w,
    );
    if cartesian {
        // Horizontal bars swap the semantic axes: the category title sits left.
        let (left_band, bottom_band) = if is_h {
            (cat_title.as_ref(), val_title.as_ref())
        } else {
            (val_title.as_ref(), cat_title.as_ref())
        };
        left += left_band.map_or(0.0, |b| b.thickness);
        bottom += bottom_band.map_or(0.0, |b| b.thickness);
    }

    let (mut plot_x, mut plot_y, mut plot_w, mut plot_h) = if cartesian {
        (
            left,
            top,
            (w - left - right).max(1.0),
            (h - top - bottom).max(1.0),
        )
    } else {
        // ref layout.ts:730-737: radial charts centre the plot in the space the
        // title and legend bands leave, minus the radial gap.
        let gap = h * RADIAL_GAP_FRAC;
        (
            leg_l,
            title.band_h + leg_t + gap,
            (w - leg_l - leg_r).max(1.0),
            (h - title.band_h - leg_t - leg_b - gap).max(1.0),
        )
    };

    // Manual plot-area layout (ECMA-376 §21.2.2.88). Factor `x`/`y`/`w`/`h` are
    // chart-space fractions of the frame; `layoutTarget="outer"` names the
    // axis-inclusive rectangle, whose inner data region subtracts the automatic
    // gutters (§21.2.2.89, ref layout.ts:793-813).
    let mut manual_layout_applied = false;
    if let Some(pml) = chart.plot_area_manual_layout.as_ref() {
        let insets = if pml.layout_target.as_deref() == Some("outer") {
            (left, top, right, bottom)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        let frame_rect = (0.0, 0.0, w, h);
        if let Some(r) = resolve_manual_rect(pml, frame_rect, frame_rect) {
            if r.2 > insets.0 + insets.2 && r.3 > insets.1 + insets.3 {
                plot_x = r.0 + insets.0;
                plot_y = r.1 + insets.1;
                plot_w = r.2 - insets.0 - insets.2;
                plot_h = r.3 - insets.1 - insets.3;
                manual_layout_applied = true;
            }
        }
    }

    // Elide the title to the band it shares with the legend. Plain titles are
    // painted across the frame in the reference (`shared/title.ts:150-170`
    // centres at `x + w/2`), so the bound is the frame width minus the side
    // legend bands; `frame.plotRect.pw` would clip a title the reference draws
    // in full.
    let title_avail = (w - leg_l - leg_r).max(1.0);
    if !title.is_empty() {
        title.text = elide_to_width(
            &title.text,
            title_avail,
            title.font_pt * EMU_PER_PT,
            chart.title_font_bold.unwrap_or(false),
            chart.title_font_italic.unwrap_or(false),
        );
    }

    let title_box = if title.is_empty() {
        None
    } else {
        Some((0.0, 0.0, w, title.band_h))
    };
    let legend_box = legend.as_ref().map(|leg| {
        let inset = (LEGEND_SIDE_PADDING / 2.0).min(leg.reserve_w / 2.0);
        match leg.side.as_str() {
            "l" => (inset, plot_y, (leg.reserve_w - 2.0 * inset).max(1.0), plot_h),
            "t" => (
                LEGEND_HORIZONTAL_INSET,
                title.band_h,
                (w - LEGEND_HORIZONTAL_PADDING).max(1.0),
                leg.reserve_h,
            ),
            "b" => (
                LEGEND_HORIZONTAL_INSET,
                h - leg.reserve_h,
                (w - LEGEND_HORIZONTAL_PADDING).max(1.0),
                leg.reserve_h,
            ),
            _ => (
                w - leg.reserve_w + inset,
                plot_y,
                (leg.reserve_w - 2.0 * inset).max(1.0),
                plot_h,
            ),
        }
    });

    // Paint anchors for the axis titles: their bands sit at the frame edges.
    let mut cat_title = cat_title;
    let mut val_title = val_title;
    if cartesian {
        for (band, is_cat) in [(&mut cat_title, true), (&mut val_title, false)] {
            if let Some(band) = band.as_mut() {
                let font = band.font_pt * EMU_PER_PT;
                band.vertical = if is_h { is_cat } else { !is_cat };
                if band.vertical {
                    band.x = band.thickness / 2.0;
                    band.y = plot_y + plot_h / 2.0 + font * 0.35;
                } else {
                    band.x = plot_x + plot_w / 2.0;
                    band.y = h - band.thickness / 2.0 + font * 0.35;
                }
            }
        }
    }

    ChartLayout {
        w,
        h,
        plot_x,
        plot_y,
        plot_w,
        plot_h,
        title_box,
        title,
        legend_box,
        legend_pos: legend
            .as_ref()
            .map(|l| l.side.clone())
            .unwrap_or_else(|| "r".to_string()),
        show_legend: legend.is_some(),
        legend,
        cat_title,
        val_title,
        manual_layout_applied,
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
        commands.push(VectorCommand::draw_path(
            rect_path(0.0, 0.0, layout.w, layout.h),
            Some(VectorPaint::Solid(parse_hex_color(bg))),
            None,
            transform,
        ));
    }

    // Chart-area border
    if let Some(ref border_color) = chart.chart_border_color {
        let stroke = VectorStroke {
            color: parse_hex_color(border_color),
            width_emu: chart.chart_border_width_emu.unwrap_or(12700) as i64,
            dash_segments: None,
            line_cap: LineCap::Square,
            line_join: LineJoin::Miter,
            paint: None,
            ..VectorStroke::default()
        };
        commands.push(VectorCommand::draw_path(
            rect_path(0.0, 0.0, layout.w, layout.h),
            None,
            Some(stroke),
            transform,
        ));
    }

    // Plot-area background
    if let Some(ref bg) = chart.plot_area_bg {
        commands.push(VectorCommand::draw_path(
            rect_path(layout.plot_x, layout.plot_y, layout.plot_w, layout.plot_h),
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
    let band = &layout.title;
    let (_bx, by, _bw, _bh) = match layout.title_box {
        Some(box_) => box_,
        None => return Vec::new(),
    };
    if band.text.is_empty() {
        return Vec::new();
    }
    let color = chart
        .title_font_color
        .as_deref()
        .map(parse_hex_color)
        .unwrap_or([40, 40, 40, 255]);
    // ref shared/title.ts:150-170: the title draws top-aligned at `band.topPad`
    // inside the reserved band, centred on the frame minus the legend bands.
    let (leg_l, leg_r, _, _) =
        legend_bands(layout.legend.as_ref(), chart.legend_overlay == Some(true));
    let avail = (layout.w - leg_l - leg_r).max(1.0);
    let cx = leg_l + avail / 2.0;
    let baseline = by + band.top_pad + band.font_pt * EMU_PER_PT * 0.8;
    let run = make_simple_run(&band.text, band.font_pt, color, cx, baseline, "center");
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
    let (leg, (lx, ly, lw, lh)) = match (layout.legend.as_ref(), layout.legend_box) {
        (Some(leg), Some(box_)) => (leg, box_),
        _ => return Vec::new(),
    };
    let font = leg.font_pt * EMU_PER_PT;
    let text_color = chart
        .legend_font_color
        .as_deref()
        .map(parse_hex_color)
        .unwrap_or([60, 60, 60, 255]);
    let mut commands = Vec::new();
    let horizontal = leg.side == "t" || leg.side == "b";
    let rows: Vec<Vec<usize>> = if horizontal {
        pack_rows(&leg.item_widths, lw.max(1.0), LEGEND_ITEM_GAP)
    } else {
        (0..leg.items.len()).map(|idx| vec![idx]).collect()
    };
    let block_h = rows.len() as f32 * leg.row_h;
    let start_y = if horizontal {
        ly + (lh - block_h) / 2.0
    } else {
        ly + leg.row_h * 0.5
    };
    for (row_idx, row) in rows.iter().enumerate() {
        let total = row.iter().map(|&idx| leg.item_widths[idx]).sum::<f32>()
            + LEGEND_ITEM_GAP * (row.len().saturating_sub(1)) as f32;
        let row_x = if horizontal { lx + (lw - total) / 2.0 } else { lx };
        let row_y = start_y + (row_idx as f32) * leg.row_h;
        // A side band as tall as the plot cannot paint more rows than fit; the
        // overflow is dropped rather than spilled over the marks.
        if !horizontal && row_y - leg.row_h / 2.0 > ly + lh {
            break;
        }
        let mut pen = row_x;
        for &idx in row {
            let item = &leg.items[idx];
            if matches!(family, ChartFamily::Line) {
                // ref shared/legend.ts:38-50 (`legendSwatchStyle`): line-shaped
                // keys for the line family.
                let seg = VectorPath::new(
                    vec![
                        PathSegment::MoveTo { x: pen, y: row_y },
                        PathSegment::LineTo { x: pen + leg.swatch, y: row_y },
                    ],
                    FillRule::NonZero,
                );
                let stroke = VectorStroke {
                    color: item.color,
                    width_emu: 25400,
                    dash_segments: None,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    paint: None,
                    ..VectorStroke::default()
                };
                commands.push(VectorCommand::draw_path(seg, None, Some(stroke), transform));
            } else {
                commands.push(VectorCommand::draw_path(
                    rect_path(pen, row_y - leg.swatch / 2.0, leg.swatch, leg.swatch),
                    Some(VectorPaint::Solid(item.color)),
                    None,
                    transform,
                ));
            }
            let tx = pen + leg.swatch + LEGEND_SWATCH_TEXT_GAP;
            let ty = row_y + font * 0.35;
            commands.push(VectorCommand::DrawText {
                runs: vec![make_simple_run(&item.label, leg.font_pt, text_color, tx, ty, "left")],
                transform,
            });
            pen += leg.item_widths[idx] + LEGEND_ITEM_GAP;
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

/// Authored `dLblPos`, series block first then chart level.
///
/// ref core/src/chart/shared/data-labels.ts:118
/// (`ovr?.position ?? seriesDef?.position ?? defaultPos`).
pub fn data_label_position(chart: &ChartModel, series: &ChartSeries, default: &str) -> String {
    series
        .series_data_labels
        .as_ref()
        .and_then(|labels| labels.position.clone())
        .or_else(|| chart.data_label_position.clone())
        .unwrap_or_else(|| default.to_string())
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
    let advance = |ch: char| {
        if ch.is_ascii_punctuation() || ch == ' ' {
            sz_emu * 0.3
        } else if ch.is_ascii_digit() || ch.is_ascii_uppercase() {
            sz_emu * 0.6
        } else if ch.is_ascii_lowercase() {
            sz_emu * 0.5
        } else {
            sz_emu * 0.9
        }
    };
    let total_width: f32 = text.chars().map(advance).sum();
    let start_x = match alignment {
        "center" => x_pos - (total_width / 2.0),
        "right" => x_pos - total_width,
        _ => x_pos,
    };

    let mut glyphs = Vec::new();
    let mut cur_x = start_x;
    for (i, ch) in text.chars().enumerate() {
        let gw = advance(ch);
        glyphs.push(PositionedGlyph {
            glyph_id: i as u16,
            x: cur_x,
            y: y_pos,
            advance: gw,
            warp_u: None,
        });
        cur_x += gw;
    }

    PositionedGlyphRun {
        text: text.to_string(),
        font_family: CHART_FONT.to_string(),
        font_size_pt: sz_pt,
        color,
        bold: false,
        italic: false,
        glyphs,
        underline: false,
        strikethrough: false,
        warp: None,
    }
}
