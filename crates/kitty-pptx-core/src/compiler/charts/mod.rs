pub mod area;
pub mod axes;
pub mod bands;
pub mod bar;
pub mod color;
pub mod layout;
pub mod line;
pub mod pie;
pub mod types;

use crossbeam_channel::Sender;
use pptx_model::ChartElement;
use crate::contracts::{FidelityClass, VectorCommand, WarningEvent};
use crate::compiler::placeholders;
use crate::compiler::transform::compute_shape_transform;

use self::area::compile_area;
use self::axes::compile_axes;
use self::bar::compile_bars;
use self::layout::{
    compile_chart_background, compile_chart_legend, compile_chart_title, compute_chart_layout,
};
use self::line::compile_lines;
use self::pie::compile_pie;
use self::types::{detect_chart_family, has_plottable_data, ChartDataBounds, ChartFamily};

pub fn compile_chart(
    chart_elem: &ChartElement,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let chart = &chart_elem.chart;

    // If the chart has no plot series or plottable data, gracefully fall back to placeholder box
    if !has_plottable_data(chart) {
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: chart_elem.id.clone().unwrap_or_else(|| "chart".to_string()),
            feature: "chart_series",
            class: FidelityClass::Placeholder,
        });
        return placeholders::compile_chart_placeholder(chart_elem);
    }

    let transform = compute_shape_transform(
        chart_elem.x,
        chart_elem.y,
        chart_elem.width,
        chart_elem.height,
        chart_elem.rotation,
        chart_elem.flip_h,
        chart_elem.flip_v,
    );

    let w = chart_elem.width as f32;
    let h = chart_elem.height as f32;

    let mut family = detect_chart_family(chart);
    if family == ChartFamily::Other {
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: chart_elem.id.clone().unwrap_or_else(|| "chart".to_string()),
            feature: "unsupported_chart_type",
            class: FidelityClass::Degraded,
        });
        family = ChartFamily::Column;
    }

    let bounds = ChartDataBounds::compute(chart, family);
    let layout = compute_chart_layout(chart, w, h, family);

    let mut commands = Vec::new();

    // 1. Chart & plot area backgrounds and borders
    commands.extend(compile_chart_background(chart, &layout, transform));

    // 2. Axes (grid lines, ticks, and category/value labels)
    commands.extend(compile_axes(chart, &layout, &bounds, family, transform));

    // 3. Series data marks
    match family {
        ChartFamily::Column
        | ChartFamily::StackedColumn
        | ChartFamily::StackedColumnPct
        | ChartFamily::Bar
        | ChartFamily::StackedBar
        | ChartFamily::StackedBarPct => {
            commands.extend(compile_bars(chart, &layout, &bounds, family, transform));
        }
        ChartFamily::Line => {
            commands.extend(compile_lines(chart, &layout, &bounds, transform));
        }
        ChartFamily::Pie => {
            commands.extend(compile_pie(chart, &layout, false, transform));
        }
        ChartFamily::Doughnut => {
            commands.extend(compile_pie(chart, &layout, true, transform));
        }
        ChartFamily::Area | ChartFamily::StackedArea => {
            commands.extend(compile_area(chart, &layout, &bounds, family, transform));
        }
        ChartFamily::Other => {
            commands.extend(compile_bars(chart, &layout, &bounds, ChartFamily::Column, transform));
        }
    }

    // 4. Title
    commands.extend(compile_chart_title(chart, &layout, transform));

    // 5. Legend
    commands.extend(compile_chart_legend(chart, &layout, family, transform));

    commands
}
