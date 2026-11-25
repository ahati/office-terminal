//! Measured chart frame bands: title, legend and axis titles.
//!
//! Every band here is measured from text metrics instead of a fixed fraction of
//! the frame, matching the reference renderer's shared chart layout
//! (`/tmp/oooxml/packages/core/src/chart/layout.ts`):
//! - title band = `fontPx + topPad + bottomPad` (layout.ts:287-306, 455-470),
//! - legend reserve from measured entry widths, capped at 30 % (layout.ts:236-268
//!   + shared/legend.ts:950),
//! - axis-title gutters from the authored font size (layout.ts:497-524).

use crate::compiler::style::parse_hex_color;
use ooxml_common::chart::ChartSeries;
use pptx_model::ChartModel;
// EMU bases come from the shared contracts constants (`contracts::geometry`),
// the same 96-DPI pixel and typographic point every other module uses.
use super::color::{resolve_series_color, resolve_slice_color};
use super::types::{get_category_label, ChartFamily};
use crate::contracts::{EMU_PER_PT, EMU_PER_PX};

/// Points per CSS pixel at the reference renderer's 96 dpi host scale.
const PT_PER_PX: f32 = 0.75;
/// Face used for chart text measurement (matches the emitted runs).
pub const CHART_FONT: &str = "Carlito";

// Legend metrics, verbatim from the reference measured-legend layout
// (core/src/chart/shared/legend.ts:836-860).
const LEGEND_SWATCH_TEXT_GAP: f32 = 4.0 * EMU_PER_PX;
const LEGEND_ITEM_GAP: f32 = 12.0 * EMU_PER_PX;
const LEGEND_ROW_EXTRA: f32 = 4.0 * EMU_PER_PX;
const LEGEND_HORIZONTAL_PADDING: f32 = 8.0 * EMU_PER_PX;
const LEGEND_SIDE_PADDING: f32 = 8.0 * EMU_PER_PX;
const LEGEND_VERTICAL_PADDING: f32 = 4.0 * EMU_PER_PX;
/// Office keeps a filled legend key at a fixed 7 pt square.
const FILLED_LEGEND_KEY_PT: f32 = 7.0;
/// ref core/src/chart/layout.ts:245-253: library policy caps the legend band.
const LEGEND_MAX_FRAC: f32 = 0.30;
const LEGEND_MIN_WIDTH_PX: f32 = 80.0;

// Title band, ref core/src/chart/layout.ts:289-306 / 455-470
// (`TITLE_TOP_PAD_FONT_FRAC`, `cartesianTitleBand`).
const TITLE_TOP_PAD_FONT_FRAC: f32 = 0.62;
const TITLE_BAND_FONT_FRAC: f32 = 2.25;
/// Radial (pie/doughnut) title pads, ref core/src/chart/families/pie.ts:310-315.
const RADIAL_TITLE_PAD_FRAC: f32 = 0.035;
/// Axis-title fallback, ref core/src/chart/layout.ts:405.
const AXIS_TITLE_FALLBACK_PT: f32 = 10.0;

/// Convert an OOXML `ST_TextFontSize` (hundredths of a point) to points.
///
/// ref core/src/chart/layout.ts:146-154 (`chartTextFontSizePx`): the schema
/// range is 100..=400000 hpt and the conversion is `sizeHpt / 100`.
pub fn chart_text_font_pt(size_hpt: Option<i32>) -> Option<f32> {
    size_hpt
        .filter(|hpt| (100..=400_000).contains(hpt))
        .map(|hpt| hpt as f32 / 100.0)
}

/// Axis tick-label font size in points (`axisLabelPx`).
///
/// ref core/src/chart/shared/axis.ts:584:
/// `chartTextFontSizePx(...) ?? Math.max(8, h * 0.045)`.
pub fn axis_label_font_pt(size_hpt: Option<i32>, chart_h: f32) -> f32 {
    chart_text_font_pt(size_hpt).unwrap_or_else(|| ((chart_h / EMU_PER_PX) * 0.045).max(8.0) * PT_PER_PX)
}

/// Data-label font size in points; `plot_h` is the plot height in EMU.
///
/// ref core/src/chart/shared/data-labels.ts:118-121: series `dLbls@sz` wins,
/// then the chart-level size; the fallback is `max(9, min(11, ph / 25))` px.
pub fn series_data_label_font_pt(chart: &ChartModel, series: &ChartSeries, plot_h: f32) -> f32 {
    let hpt = series
        .series_data_labels
        .as_ref()
        .and_then(|labels| labels.font_size_hpt)
        .or(chart.data_label_font_size_hpt);
    chart_text_font_pt(hpt).unwrap_or_else(|| ((plot_h / EMU_PER_PX) / 25.0).clamp(9.0, 11.0) * PT_PER_PX)
}

/// Legend entry font size in points; ref core/src/chart/shared/resource.ts:33.
pub fn legend_font_pt(size_hpt: Option<i32>) -> f32 {
    chart_text_font_pt(size_hpt).unwrap_or(10.0)
}

/// Axis-title font size in points; ref core/src/chart/layout.ts:405-420.
pub fn axis_title_font_pt(size_hpt: Option<i32>) -> f32 {
    chart_text_font_pt(size_hpt).unwrap_or(AXIS_TITLE_FALLBACK_PT)
}

/// Margin between the chart edge and an axis title, ref layout.ts:392.
pub fn axis_title_margin(dim: f32) -> f32 {
    (8.0 * EMU_PER_PX).max(dim * 0.02)
}

/// Advance width of `text` in EMU at `font_emu` (the reference `measureText`).
pub fn text_width(text: &str, font_emu: f32, bold: bool, italic: bool) -> f32 {
    crate::text::shape::measure(text, CHART_FONT, font_emu, bold, italic)
}

/// Measured truncation, ref core/src/chart/text-elide.ts:31-77
/// (`elideToWidth`): longest prefix whose `prefix + '…'` fits `max_w`.
pub fn elide_to_width(text: &str, max_w: f32, font_emu: f32, bold: bool, italic: bool) -> String {
    if text.is_empty() || max_w <= 0.0 {
        return String::new();
    }
    if text_width(text, font_emu, bold, italic) <= max_w {
        return text.to_string();
    }
    let ellipsis = '\u{2026}';
    if text_width("\u{2026}", font_emu, bold, italic) > max_w {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut best = 0usize;
    let (mut lo, mut hi) = (0usize, chars.len().saturating_sub(1));
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let mut candidate: String = chars[..mid].iter().collect();
        candidate.push(ellipsis);
        if text_width(&candidate, font_emu, bold, italic) <= max_w {
            best = mid;
            lo = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            hi = mid - 1;
        }
    }
    let mut out: String = chars[..best].iter().collect();
    out.push(ellipsis);
    out
}

/// Title band: `fontPx + topPad + bottomPad`, ref layout.ts:287-306.
#[derive(Clone, Debug, Default)]
pub struct ChartTitleBand {
    pub font_pt: f32,
    pub top_pad: f32,
    pub bottom_pad: f32,
    pub band_h: f32,
    /// Title text elided to the plot width (empty when there is no title).
    pub text: String,
}

impl ChartTitleBand {
    pub fn is_empty(&self) -> bool {
        self.band_h <= 0.0
    }
}

/// Title band, ref core/src/chart/layout.ts:455-470 (`cartesianTitleBand`) and
/// 310-330 (`chartTitleBand`) for the radial families.
pub fn title_band(chart: &ChartModel, h: f32, family: ChartFamily) -> ChartTitleBand {
    let title = chart.title.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if title.is_none() && !chart.title_present {
        return ChartTitleBand::default();
    }
    let font_pt = chart_text_font_pt(chart.title_font_size_hpt).unwrap_or(14.0);
    let font = font_pt * EMU_PER_PT;
    let band_h = if family.is_cartesian() {
        font * TITLE_BAND_FONT_FRAC
    } else {
        font + h * RADIAL_TITLE_PAD_FRAC * 2.0
    };
    // ref layout.ts:293-306: the draw offset is font-proportional, clamped so a
    // shallow band never pushes the title past the plot.
    let top_pad = (band_h - font).clamp(0.0, font * TITLE_TOP_PAD_FONT_FRAC);
    ChartTitleBand {
        font_pt,
        top_pad,
        bottom_pad: band_h - font - top_pad,
        band_h,
        text: title.unwrap_or_default().to_string(),
    }
}

/// One resolved legend entry (label + swatch colour).
#[derive(Clone, Debug)]
pub struct LegendItem {
    pub label: String,
    pub color: [u8; 4],
}

/// Measured legend reserve plus everything the painter needs.
#[derive(Clone, Debug)]
pub struct LegendReserve {
    pub side: String,
    pub reserve_w: f32,
    pub reserve_h: f32,
    pub font_pt: f32,
    pub swatch: f32,
    pub row_h: f32,
    pub item_widths: Vec<f32>,
    pub items: Vec<LegendItem>,
}

/// Greedy legend row packing, ref core/src/chart/layout.ts:80-104.
pub fn pack_rows(item_widths: &[f32], max_width: f32, gap: f32) -> Vec<Vec<usize>> {
    let avail = max_width.max(1.0);
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row: Vec<usize> = Vec::new();
    let mut row_w = 0.0;
    for (idx, &width) in item_widths.iter().enumerate() {
        let width = width.clamp(0.0, avail);
        let next = if row.is_empty() { width } else { row_w + gap + width };
        if !row.is_empty() && next > avail {
            rows.push(std::mem::take(&mut row));
            row.push(idx);
            row_w = width;
        } else {
            row.push(idx);
            row_w = next;
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn legend_entries(chart: &ChartModel, family: ChartFamily) -> Vec<LegendItem> {
    if matches!(family, ChartFamily::Pie | ChartFamily::Doughnut) {
        let count = chart
            .categories
            .len()
            .max(chart.series.first().map(|s| s.values.len()).unwrap_or(0));
        (0..count)
            .map(|idx| LegendItem {
                label: get_category_label(chart, idx),
                color: resolve_slice_color(idx, chart.series.first()),
            })
            .collect()
    } else {
        chart
            .series
            .iter()
            .enumerate()
            .map(|(idx, s)| LegendItem {
                label: if s.name.trim().is_empty() {
                    format!("Series {}", idx + 1)
                } else {
                    s.name.clone()
                },
                color: resolve_series_color(idx, s),
            })
            .collect()
    }
}

/// Measure the automatic legend band, ref core/src/chart/layout.ts:236-268
/// (`chartLegendReserve`) + shared/legend.ts:950 (`measuredLegendReserve`):
/// entry width = key + gap + measured text.
pub fn measure_legend(chart: &ChartModel, w: f32, h: f32, family: ChartFamily) -> Option<LegendReserve> {
    if !(chart.show_legend || chart.legend_pos.is_some()) {
        return None;
    }
    let items = legend_entries(chart, family);
    if items.is_empty() {
        return None;
    }
    let side = match chart.legend_pos.as_deref().unwrap_or("r").to_lowercase().as_str() {
        "l" => "l",
        "t" => "t",
        "b" => "b",
        _ => "r",
    };
    let font_pt = legend_font_pt(chart.legend_font_size_hpt);
    let font = font_pt * EMU_PER_PT;
    let bold = chart.legend_font_bold.unwrap_or(false);
    let italic = chart.legend_font_italic.unwrap_or(false);
    // ref shared/legend.ts:853-857: filled keys are 7 pt squares, line-shaped
    // keys stay font-relative (1.6 em).
    let swatch = if matches!(family, ChartFamily::Line) {
        font * 1.6
    } else {
        FILLED_LEGEND_KEY_PT * EMU_PER_PT
    };
    let item_widths: Vec<f32> = items
        .iter()
        .map(|item| swatch + LEGEND_SWATCH_TEXT_GAP + text_width(&item.label, font, bold, italic))
        .collect();
    let row_h = font + LEGEND_ROW_EXTRA;
    let horizontal = side == "t" || side == "b";
    let padding = if horizontal {
        LEGEND_HORIZONTAL_PADDING
    } else {
        LEGEND_SIDE_PADDING
    };
    let widest = item_widths.iter().copied().fold(0.0, f32::max);
    let (reserve_w, reserve_h) = if horizontal {
        let avail = (w - padding).max(1.0);
        let rows = pack_rows(&item_widths, avail, LEGEND_ITEM_GAP).len() as f32;
        (0.0, (rows * row_h + LEGEND_VERTICAL_PADDING).min(h * LEGEND_MAX_FRAC))
    } else {
        let cap = w * LEGEND_MAX_FRAC;
        let measured = (widest + padding).max((LEGEND_MIN_WIDTH_PX * EMU_PER_PX).min(cap));
        (cap.min(measured), 0.0)
    };
    // A band that cannot hold one legend key plus a glyph is dropped rather
    // than reserving space with nothing paintable in it.
    let key_fits = if horizontal {
        reserve_h >= row_h
    } else {
        reserve_w >= swatch + LEGEND_SWATCH_TEXT_GAP + text_width("0", font, bold, italic)
    };
    if !key_fits {
        return None;
    }
    Some(LegendReserve {
        side: side.to_string(),
        reserve_w,
        reserve_h,
        font_pt,
        swatch,
        row_h,
        item_widths,
        items,
    })
}

/// Per-side legend bands, ref core/src/chart/layout.ts:277-288. An overlay
/// legend paints over the plot and reserves nothing (`§21.2.2.132`).
pub fn legend_bands(legend: Option<&LegendReserve>, overlay: bool) -> (f32, f32, f32, f32) {
    if overlay {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let reserve_w = legend.map_or(0.0, |l| l.reserve_w);
    let reserve_h = legend.map_or(0.0, |l| l.reserve_h);
    match legend.map(|leg| leg.side.as_str()) {
        Some("l") => (reserve_w, 0.0, 0.0, 0.0),
        Some("t") => (0.0, 0.0, reserve_h, 0.0),
        Some("b") => (0.0, 0.0, 0.0, reserve_h),
        _ => (0.0, reserve_w, 0.0, 0.0),
    }
}

/// Axis-title band and its paint anchor.
#[derive(Clone, Debug)]
pub struct AxisTitleBand {
    pub text: String,
    pub font_pt: f32,
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    /// Reserved gutter thickness in EMU (height when horizontal, width when
    /// stacked).
    pub thickness: f32,
    /// Stacked glyph-per-line paint (the rotated side title); `false` = one run.
    pub vertical: bool,
    /// Anchor x (run start, or the column centre when stacked).
    pub x: f32,
    /// Anchor baseline y (the lowest line when stacked).
    pub y: f32,
}

/// One axis-title band, ref core/src/chart/layout.ts:497-524
/// (`chartAxisTitleBands`): `fontPx + textInset + margin + 4`.
pub fn make_axis_title(
    text: Option<&String>,
    size_hpt: Option<i32>,
    color: Option<&str>,
    bold: Option<bool>,
    italic: Option<bool>,
    inset_emu: Option<i64>,
    dim: f32,
) -> Option<AxisTitleBand> {
    let text = text.map(|t| t.trim()).filter(|t| !t.is_empty())?;
    let font_pt = axis_title_font_pt(size_hpt);
    let thickness =
        font_pt * EMU_PER_PT + inset_emu.unwrap_or(0).max(0) as f32 + axis_title_margin(dim) + 4.0 * EMU_PER_PX;
    Some(AxisTitleBand {
        text: text.to_string(),
        font_pt,
        color: color.map(parse_hex_color).unwrap_or([60, 60, 60, 255]),
        bold: bold.unwrap_or(false),
        italic: italic.unwrap_or(false),
        thickness,
        vertical: false,
        x: 0.0,
        y: 0.0,
    })
}
