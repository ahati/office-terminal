use ooxml_common::chart::ChartModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartFamily {
    Column,          // Vertical clustered column
    StackedColumn,   // Vertical stacked column
    StackedColumnPct,// Vertical 100% stacked column
    Bar,             // Horizontal clustered bar
    StackedBar,      // Horizontal stacked bar
    StackedBarPct,   // Horizontal 100% stacked bar
    Line,            // Line chart
    Area,            // Area chart
    StackedArea,     // Stacked area chart
    Pie,             // 2D Pie chart
    Doughnut,        // Doughnut chart
    Other,           // Unrecognized or complex chart
}

impl ChartFamily {
    pub fn is_horizontal_bar(&self) -> bool {
        matches!(self, ChartFamily::Bar | ChartFamily::StackedBar | ChartFamily::StackedBarPct)
    }

    pub fn is_cartesian(&self) -> bool {
        !matches!(self, ChartFamily::Pie | ChartFamily::Doughnut | ChartFamily::Other)
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
}

pub fn detect_chart_family(chart: &ChartModel) -> ChartFamily {
    let raw = chart.chart_type.to_lowercase();
    let is_h = raw.contains("barh")
        || (raw.starts_with("bar") && !raw.contains("col"))
        || chart.series.iter().any(|s| {
            s.bar_group_direction.as_deref() == Some("bar")
        });

    if raw.contains("pie") || raw == "ofpie" {
        ChartFamily::Pie
    } else if raw.contains("doughnut") {
        ChartFamily::Doughnut
    } else if raw.contains("line") {
        ChartFamily::Line
    } else if raw.contains("area") {
        if raw.contains("stacked") {
            ChartFamily::StackedArea
        } else {
            ChartFamily::Area
        }
    } else if raw.contains("col") || raw.contains("bar") {
        let is_pct = raw.contains("pct") || raw.contains("percent");
        let is_stacked = raw.contains("stacked");

        if is_h {
            if is_pct {
                ChartFamily::StackedBarPct
            } else if is_stacked {
                ChartFamily::StackedBar
            } else {
                ChartFamily::Bar
            }
        } else if is_pct {
            ChartFamily::StackedColumnPct
        } else if is_stacked {
            ChartFamily::StackedColumn
        } else {
            ChartFamily::Column
        }
    } else {
        ChartFamily::Other
    }
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
