use ooxml_common::chart::{ChartModel, ChartPlotGroup};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartFamily {
    Column,           // Vertical clustered column
    StackedColumn,    // Vertical stacked column
    StackedColumnPct, // Vertical 100% stacked column
    Bar,              // Horizontal clustered bar
    StackedBar,       // Horizontal stacked bar
    StackedBarPct,    // Horizontal 100% stacked bar
    Line,             // Line chart
    Area,             // Area chart
    StackedArea,      // Stacked area chart
    Pie,              // 2D Pie chart
    Doughnut,         // Doughnut chart
    // Classic families for which this crate has no compiler. They must paint
    // the reference's "Unsupported chart" placeholder, never a column chart.
    Scatter,
    Bubble,
    Radar,
    Stock,
    Surface,
    // Unrecognized, mixed-family or invalid plot-group model.
    Other,
}

impl ChartFamily {
    pub fn is_horizontal_bar(&self) -> bool {
        matches!(
            self,
            ChartFamily::Bar | ChartFamily::StackedBar | ChartFamily::StackedBarPct
        )
    }

    /// Families laid out on cartesian axes by the existing layout code.
    ///
    /// Unsupported families never reach layout (they short-circuit to the
    /// placeholder), so returning `false` for them is safe and keeps the
    /// predicate about paint capability rather than geometry.
    pub fn is_cartesian(&self) -> bool {
        matches!(
            self,
            ChartFamily::Column
                | ChartFamily::StackedColumn
                | ChartFamily::StackedColumnPct
                | ChartFamily::Bar
                | ChartFamily::StackedBar
                | ChartFamily::StackedBarPct
                | ChartFamily::Line
                | ChartFamily::Area
                | ChartFamily::StackedArea
                | ChartFamily::Bubble
        )
    }

    pub fn is_percent_stacked(&self) -> bool {
        matches!(self, ChartFamily::StackedColumnPct | ChartFamily::StackedBarPct)
    }

    pub fn is_stacked(&self) -> bool {
        matches!(
            self,
            ChartFamily::StackedColumn
                | ChartFamily::StackedColumnPct
                | ChartFamily::StackedBar
                | ChartFamily::StackedBarPct
                | ChartFamily::StackedArea
        )
    }

    /// The subset with an implemented series compiler.
    pub fn is_supported(&self) -> bool {
        matches!(
            self,
            ChartFamily::Column
                | ChartFamily::StackedColumn
                | ChartFamily::StackedColumnPct
                | ChartFamily::Bar
                | ChartFamily::StackedBar
                | ChartFamily::StackedBarPct
                | ChartFamily::Line
                | ChartFamily::Area
                | ChartFamily::StackedArea
                | ChartFamily::Pie
                | ChartFamily::Doughnut
                | ChartFamily::Bubble
        )
    }

    /// Stable lower-case name, used for diagnostics/tests and in the
    /// `chart_family_unsupported` warning context.
    pub fn label(&self) -> &'static str {
        match self {
            ChartFamily::Column => "column",
            ChartFamily::StackedColumn => "stackedColumn",
            ChartFamily::StackedColumnPct => "stackedColumnPct",
            ChartFamily::Bar => "bar",
            ChartFamily::StackedBar => "stackedBar",
            ChartFamily::StackedBarPct => "stackedBarPct",
            ChartFamily::Line => "line",
            ChartFamily::Area => "area",
            ChartFamily::StackedArea => "stackedArea",
            ChartFamily::Pie => "pie",
            ChartFamily::Doughnut => "doughnut",
            ChartFamily::Scatter => "scatter",
            ChartFamily::Bubble => "bubble",
            ChartFamily::Radar => "radar",
            ChartFamily::Stock => "stock",
            ChartFamily::Surface => "surface",
            ChartFamily::Other => "other",
        }
    }
}

/// Legacy dispatch from the authored `chartType` string.
///
/// This mirrors the reference's exact family table
/// (`ref core/src/chart/renderer.ts:101-131`) while keeping the historical
/// descriptive names (`clusteredColumn`, `column`, `bar`) accepted by this
/// crate.
///
/// Empty-type rule: a missing/empty `chartType` falls back to `Column` only
/// when the model is otherwise empty (no series, categories, plot groups or
/// title). A populated chart with no type cannot be dispatched, so it takes
/// the unknown-family placeholder path instead of silently drawing columns.
pub fn detect_chart_family(chart: &ChartModel) -> ChartFamily {
    let raw = chart.chart_type.trim().to_lowercase();
    if raw.is_empty() {
        return if chart_is_otherwise_empty(chart) {
            ChartFamily::Column
        } else {
            ChartFamily::Other
        };
    }
    family_for_type(&raw, chart)
}

/// Resolve the family that paint dispatch must use.
///
/// `ref core/src/chart/plot-groups.ts:95-208` (`classicPlotDispatch`): models
/// with two or more plot groups dispatch from their validated group kinds;
/// zero/one-group models keep the legacy `chartType` table. This crate only
/// paints bar/line/area/pie/doughnut, so unsupported and mixed-family scenes
/// return a family whose [`ChartFamily::is_supported`] is false. `compile_chart`
/// then emits the reference `"Unsupported chart"` placeholder.
pub fn resolve_chart_family(chart: &ChartModel) -> ChartFamily {
    let Some(groups) = chart.plot_groups.as_deref() else {
        return detect_chart_family(chart);
    };
    if groups.len() <= 1 {
        return detect_chart_family(chart);
    }
    if !plot_groups_are_valid(groups, chart.series.len()) {
        return ChartFamily::Other;
    }
    // Unresolved axis ownership cannot be painted through the primary axes
    // (`ref plot-groups.ts:168-170`).
    if groups.iter().any(|group| {
        matches!(group.category_axis.as_str(), "unresolved")
            || matches!(group.value_axis.as_str(), "unresolved")
            || matches!(group.series_axis.as_str(), "unresolved")
    }) {
        return ChartFamily::Other;
    }
    // Empty groups own no paint range (`ref plot-groups.ts:100`).
    let mut family: Option<ChartFamily> = None;
    for group in groups.iter().filter(|group| group.series_count > 0) {
        let kind = chart_family_for_plot_group(group, chart);
        match family {
            Some(existing) if existing != kind => return ChartFamily::Other,
            Some(_) => {}
            None => family = Some(kind),
        }
    }
    family.unwrap_or_else(|| detect_chart_family(chart))
}

fn chart_is_otherwise_empty(chart: &ChartModel) -> bool {
    chart.series.is_empty()
        && chart.categories.is_empty()
        && chart.plot_groups.as_ref().is_none_or(|groups| groups.is_empty())
        && chart.title.as_deref().is_none_or(|title| title.trim().is_empty())
}

fn family_for_type(raw: &str, chart: &ChartModel) -> ChartFamily {
    // Unsupported classic families first: a `surface3D` name also contains no
    // other family token, but e.g. `stackedLine3D` must not become a line.
    if raw.contains("surface") {
        return ChartFamily::Surface;
    }
    if raw.contains("radar") {
        return ChartFamily::Radar;
    }
    if raw.contains("stock") {
        return ChartFamily::Stock;
    }
    if raw.contains("bubble") {
        return ChartFamily::Bubble;
    }
    if raw.contains("scatter") {
        return ChartFamily::Scatter;
    }
    if raw.contains("doughnut") || raw.contains("donut") {
        return ChartFamily::Doughnut;
    }
    // `ofPie` has a distinct secondary-pie contract and no compiler here.
    if raw.contains("ofpie") {
        return ChartFamily::Other;
    }
    if raw.contains("pie") {
        return ChartFamily::Pie;
    }
    if raw.contains("line") {
        return ChartFamily::Line;
    }
    if raw.contains("area") {
        let stacked = raw.contains("stacked");
        return if stacked {
            ChartFamily::StackedArea
        } else {
            ChartFamily::Area
        };
    }
    if raw.contains("col") || raw.contains("bar") {
        let horizontal = raw.contains("barh")
            || raw == "bar"
            || chart
                .series
                .iter()
                .any(|series| series.bar_group_direction.as_deref() == Some("bar"));
        let stacked = raw.contains("stacked");
        let percent = raw.contains("pct") || raw.contains("percent") || raw.contains("100%");
        return bar_family(horizontal, stacked, percent);
    }
    ChartFamily::Other
}

fn chart_family_for_plot_group(group: &ChartPlotGroup, chart: &ChartModel) -> ChartFamily {
    let grouping = group.grouping.as_deref().unwrap_or("standard");
    match group.kind.as_str() {
        "bar" => {
            let raw = chart.chart_type.trim().to_lowercase();
            let horizontal = group
                .bar_direction
                .as_deref()
                .map(|direction| direction == "bar")
                .unwrap_or_else(|| raw.contains("barh") || raw == "bar");
            let stacked = grouping == "stacked" || grouping == "percentStacked";
            bar_family(horizontal, stacked, grouping == "percentStacked")
        }
        "line" => ChartFamily::Line,
        "area" => {
            if grouping == "stacked" || grouping == "percentStacked" {
                ChartFamily::StackedArea
            } else {
                ChartFamily::Area
            }
        }
        "pie" => ChartFamily::Pie,
        "doughnut" => ChartFamily::Doughnut,
        "scatter" => ChartFamily::Scatter,
        "bubble" => ChartFamily::Bubble,
        "radar" => ChartFamily::Radar,
        "stock" => ChartFamily::Stock,
        "surface" => ChartFamily::Surface,
        // `ofPie`, all 3-D group kinds and unknown kinds have no painter here.
        _ => ChartFamily::Other,
    }
}

fn bar_family(horizontal: bool, stacked: bool, percent: bool) -> ChartFamily {
    match (horizontal, stacked, percent) {
        (true, true, true) => ChartFamily::StackedBarPct,
        (true, true, false) => ChartFamily::StackedBar,
        (true, false, _) => ChartFamily::Bar,
        (false, true, true) => ChartFamily::StackedColumnPct,
        (false, true, false) => ChartFamily::StackedColumn,
        (false, false, _) => ChartFamily::Column,
    }
}

/// Contiguous, complete series ranges (`ref plot-groups.ts:20-30`).
fn plot_groups_are_valid(groups: &[ChartPlotGroup], series_len: usize) -> bool {
    let mut next = 0usize;
    for group in groups {
        if group.series_start != next {
            return false;
        }
        let remaining = series_len.saturating_sub(next);
        if group.series_count > remaining {
            return false;
        }
        next += group.series_count;
    }
    next == series_len
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartDataBounds {
    pub min_val: f64,
    pub max_val: f64,
    pub num_categories: usize,
    pub nice_ticks: Vec<f64>,
}

impl ChartDataBounds {
    pub fn compute(chart: &ChartModel, family: ChartFamily, axis_len_pt: f32) -> Self {
        let num_categories = chart
            .categories
            .len()
            .max(chart.series.iter().map(|s| s.values.len()).max().unwrap_or(0))
            .max(1);

        if family.is_percent_stacked() {
            // Percent axes keep their exact normalized extent; only the auto
            // unit uses the shared 1/2/5 ceiling ladder (ref
            // `automaticPercentMajorUnit`, axis-scale.ts:44-58).
            let horizontal = family.is_horizontal_bar();
            let intervals = if horizontal || axis_len_pt < 120.0 { 5 } else { 10 };
            let step = ceiling_nice_step(100.0 / intervals as f64);
            let min_val = chart.val_min.unwrap_or(0.0);
            let max_val = chart.val_max.unwrap_or(100.0);
            return Self {
                min_val,
                max_val,
                num_categories,
                nice_ticks: indexed_ticks(min_val, max_val, step),
            };
        }

        let mut raw_min = f64::MAX;
        let mut raw_max = f64::MIN;
        let mut has_values = false;

        if family.is_stacked() {
            for cat_idx in 0..num_categories {
                let mut pos_sum = 0.0;
                let mut neg_sum = 0.0;
                for s in &chart.series {
                    if let Some(Some(v)) = s.values.get(cat_idx) {
                        has_values = true;
                        if *v >= 0.0 {
                            pos_sum += *v;
                        } else {
                            neg_sum += *v;
                        }
                    }
                }
                raw_max = raw_max.max(pos_sum);
                raw_min = raw_min.min(neg_sum);
            }
        } else {
            for s in &chart.series {
                for v in s.values.iter().flatten() {
                    has_values = true;
                    raw_min = raw_min.min(*v);
                    raw_max = raw_max.max(*v);
                }
            }
        }

        if !has_values {
            raw_min = 0.0;
            raw_max = 1.0;
        }

        let (min_val, max_val, _step, nice_ticks) = plan_linear_axis(
            raw_min,
            raw_max,
            chart.val_min,
            chart.val_max,
            axis_len_pt,
            family.is_horizontal_bar(),
            chart.val_axis_major_unit,
        );

        Self {
            min_val,
            max_val,
            num_categories,
            nice_ticks,
        }
    }
}

/// Smallest `1 / 2 / 5 × 10ⁿ` step at least as large as `raw`
/// (ref `ceilingNiceStep`, axis-scale.ts:145-157).
fn ceiling_nice_step(raw: f64) -> f64 {
    if raw <= 0.0 || !raw.is_finite() {
        return 1.0;
    }
    let mag = 10f64.powf(raw.log10().floor());
    if mag <= 0.0 || !mag.is_finite() {
        return raw;
    }
    let normalized = raw / mag;
    let factor = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    factor * mag
}

/// Round step yielding roughly `target_steps` intervals
/// (ref `niceStep`, axis-scale.ts:30-38).
fn nice_step(range: f64, target_steps: f64) -> f64 {
    if range == 0.0 {
        return 1.0;
    }
    let raw = range / target_steps;
    let mag = 10f64.powf(raw.log10().floor());
    let normed = raw / mag;
    let nice = if normed < 1.5 {
        1.0
    } else if normed < 3.5 {
        2.0
    } else if normed < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Axis ticks from `min`, stepping by `unit`, bounded at 512 positions
/// (ref `indexedTicks`, axis-scale.ts:201-242).
fn indexed_ticks(min: f64, max: f64, unit: f64) -> Vec<f64> {
    if !(min.is_finite() && max.is_finite() && unit > 0.0 && unit.is_finite()) || max < min {
        return vec![min, max];
    }
    let expected = ((max - min) / unit).floor() + 1.0;
    if !expected.is_finite() || expected < 1.0 {
        return vec![min];
    }
    let count = (expected as usize).min(512);
    let epsilon = unit.max(max - min) * 1.0e-9;
    let mut ticks = Vec::with_capacity(count);
    for index in 0..count {
        let value = min + index as f64 * unit;
        if value > max + epsilon {
            break;
        }
        ticks.push(value);
    }
    if ticks.is_empty() {
        vec![min]
    } else {
        ticks
    }
}

/// Port of `planLinearValueAxis` (`axis-scale.ts:250-388`). The automatic
/// bounds use Excel's zero-anchoring and 5 % padding; fully bounded axes choose
/// their omitted unit from the physical plot length (vertical: one interval per
/// ~28 pt, horizontal: one per ~38 pt).
#[allow(clippy::too_many_arguments)]
fn plan_linear_axis(
    data_min: f64,
    data_max: f64,
    explicit_min: Option<f64>,
    explicit_max: Option<f64>,
    axis_len_pt: f32,
    horizontal: bool,
    authored_major: Option<f64>,
) -> (f64, f64, f64, Vec<f64>) {
    let axis_len_pt = axis_len_pt as f64;
    let mut data_min = if data_min.is_finite() { data_min } else { 0.0 };
    let mut data_max = if data_max.is_finite() { data_max } else { 1.0 };
    if data_min > data_max {
        std::mem::swap(&mut data_min, &mut data_max);
    }

    let (auto_min, auto_max, auto_major) = if data_min == 0.0 && data_max == 0.0 {
        (0.0, 1.0, 0.1)
    } else {
        let mut lo_data = data_min;
        let mut hi_data = data_max;
        if lo_data == hi_data {
            lo_data = lo_data.min(0.0);
            hi_data = hi_data.max(0.0);
        }
        let anchor_positive = lo_data >= 0.0 && (lo_data == 0.0 || hi_data > 1.2 * lo_data);
        let anchor_negative = hi_data <= 0.0 && (hi_data == 0.0 || lo_data.abs() > 1.2 * hi_data.abs());
        let padded_min = if anchor_positive { 0.0 } else { lo_data };
        let padded_max = if anchor_negative { 0.0 } else { hi_data };
        let span = padded_max - padded_min;
        let padding = if span.is_finite() {
            span * 0.05
        } else {
            padded_max * 0.05 - padded_min * 0.05
        };
        let lo = if anchor_positive { 0.0 } else { lo_data - padding };
        let hi = if anchor_negative { 0.0 } else { hi_data + padding };
        let major = ceiling_nice_step(hi / 10.0 - lo / 10.0);
        ((lo / major).floor() * major, (hi / major).ceil() * major, major)
    };

    let authored = authored_major.filter(|unit| unit.is_finite() && *unit > 0.0);
    let min = explicit_min.unwrap_or_else(|| match authored {
        Some(unit) => (auto_min / unit).floor() * unit,
        None => auto_min,
    });
    let max = explicit_max.unwrap_or_else(|| match authored {
        Some(unit) => (auto_max / unit).ceil() * unit,
        None => auto_max,
    });

    let mut automatic_major = auto_major;
    if authored.is_none() {
        if let (Some(lo), Some(hi)) = (explicit_min, explicit_max) {
            if hi > lo {
                if horizontal {
                    let target = if axis_len_pt.is_finite() && axis_len_pt > 0.0 {
                        (axis_len_pt / 38.0).round().max(5.0)
                    } else {
                        8.0
                    };
                    automatic_major = ceiling_nice_step((hi - lo) / target);
                } else {
                    let target = if axis_len_pt.is_finite() && axis_len_pt > 0.0 {
                        (axis_len_pt / 28.0).round().max(5.0)
                    } else {
                        7.0
                    };
                    automatic_major = ceiling_nice_step((hi - lo) / 10.0).max(nice_step((hi - lo) / target, 1.0));
                }
            }
        }
    }

    let step = authored.unwrap_or(automatic_major);
    let mut ticks = indexed_ticks(min, max, step);
    if ticks.len() > 512 {
        ticks.truncate(512);
    }
    (min, max, step, ticks)
}

pub fn get_category_label(chart: &ChartModel, index: usize) -> String {
    if let Some(cat) = chart.categories.get(index) {
        if !cat.trim().is_empty() {
            return cat.clone();
        }
    }
    format!("Cat {}", index + 1)
}

pub fn has_plottable_data(chart: &ChartModel) -> bool {
    if chart.authored_without_series || chart.series.is_empty() {
        return false;
    }
    chart.series.iter().any(|s| s.values.iter().any(|v| v.is_some()))
}
