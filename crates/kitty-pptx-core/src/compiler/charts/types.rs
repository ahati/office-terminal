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
        matches!(self, ChartFamily::Bar | ChartFamily::StackedBar | ChartFamily::StackedBarPct)
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
    pub fn compute(chart: &ChartModel, family: ChartFamily) -> Self {
        let num_categories = chart.categories.len().max(
            chart
                .series
                .iter()
                .map(|s| s.values.len())
                .max()
                .unwrap_or(0),
        ).max(1);

        if family.is_percent_stacked() {
            return Self {
                min_val: 0.0,
                max_val: 100.0,
                num_categories,
                nice_ticks: vec![0.0, 25.0, 50.0, 75.0, 100.0],
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

        let mut min_val = chart.val_min.unwrap_or(if raw_min >= 0.0 {
            0.0
        } else {
            raw_min
        });

        let mut max_val = chart.val_max.unwrap_or(if raw_max <= 0.0 && min_val < 0.0 {
            0.0
        } else {
            raw_max
        });

        if max_val <= min_val {
            max_val = min_val + 1.0;
        }

        let nice_ticks = compute_ticks(min_val, max_val);
        if let (Some(&first), Some(&last)) = (nice_ticks.first(), nice_ticks.last()) {
            if chart.val_min.is_none() && first < min_val {
                min_val = first;
            }
            if chart.val_max.is_none() && last > max_val {
                max_val = last;
            }
        }

        Self {
            min_val,
            max_val,
            num_categories,
            nice_ticks,
        }
    }
}

fn compute_ticks(min: f64, max: f64) -> Vec<f64> {
    let span = max - min;
    if span <= 0.0 {
        return vec![min, min + 1.0];
    }

    let raw_step = span / 4.0;
    let magnitude = 10f64.powf(raw_step.log10().floor());
    let normalized = raw_step / magnitude;

    let step_mult = if normalized < 1.5 {
        1.0
    } else if normalized < 3.0 {
        2.0
    } else if normalized < 7.0 {
        5.0
    } else {
        10.0
    };

    let step = step_mult * magnitude;
    let start = (min / step).floor() * step;
    let end = (max / step).ceil() * step;

    let mut ticks = Vec::new();
    let mut current = start;
    let count_limit = 20;
    let mut count = 0;

    while current <= end + step * 0.001 && count < count_limit {
        ticks.push(current);
        current += step;
        count += 1;
    }

    if ticks.is_empty() {
        vec![min, max]
    } else {
        ticks
    }
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
