pub mod area;
pub mod axes;
pub mod bands;
pub mod bar;
pub mod bubble;
pub mod color;
pub mod layout;
pub mod line;
pub mod pie;
pub mod types;

use crate::compiler::placeholders;
use crate::compiler::transform::compute_shape_transform;
use crate::contracts::{FidelityClass, VectorCommand, VectorPaint, WarningEvent, EMU_PER_PT};
use crossbeam_channel::Sender;
use pptx_model::ChartElement;

use self::area::compile_area;
use self::axes::compile_axes;
use self::bar::compile_bars;
use self::bubble::compile_bubbles;
use self::layout::{
    compile_chart_background, compile_chart_legend, compile_chart_title, compute_chart_layout, make_simple_run,
    rect_path,
};
use self::line::compile_lines;
use self::pie::compile_pie;
use self::types::{has_plottable_data, resolve_chart_family, ChartDataBounds, ChartFamily};

pub fn compile_chart(chart_elem: &ChartElement, warning_sink: &Sender<WarningEvent>) -> Vec<VectorCommand> {
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

    let family = resolve_chart_family(chart);
    if !family.is_supported() {
        // ref core/src/chart/renderer.ts:318-327: a family without a renderer
        // paints an explicit placeholder instead of guessing another family.
        // The reference label is fixed text; the resolved family name is
        // carried by the dispatch decision (see `ChartFamily::label`).
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: chart_elem.id.clone().unwrap_or_else(|| "chart".to_string()),
            feature: "chart_family_unsupported",
            class: FidelityClass::Placeholder,
        });
        return compile_unsupported_chart_placeholder(chart_elem);
    }

    let layout = compute_chart_layout(chart, w, h, family);
    let axis_len_pt = if family.is_horizontal_bar() {
        layout.plot_w / EMU_PER_PT
    } else {
        layout.plot_h / EMU_PER_PT
    };
    let bounds = ChartDataBounds::compute(chart, family, axis_len_pt);

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
        ChartFamily::Bubble => {
            commands.extend(compile_bubbles(chart, &layout, &bounds, transform));
        }
        // Unsupported families returned the placeholder above. This arm is
        // unreachable; it deliberately emits no geometry so a future family
        // that forgets `is_supported` cannot silently fall back to columns.
        ChartFamily::Scatter | ChartFamily::Radar | ChartFamily::Stock | ChartFamily::Surface | ChartFamily::Other => {}
    }

    // 4. Title
    commands.extend(compile_chart_title(chart, &layout, transform));

    // 5. Legend
    commands.extend(compile_chart_legend(chart, &layout, family, transform));

    commands
}

/// Reference "Unsupported chart" placeholder.
///
/// `ref core/src/chart/renderer.ts:318-327`: the reference bails out of chart
/// painting with one centred `Unsupported chart` label. The task's contract
/// also asks for a light background rect so the placeholder is visibly a chart
/// stand-in rather than an empty shape.
fn compile_unsupported_chart_placeholder(chart_elem: &ChartElement) -> Vec<VectorCommand> {
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

    let background = VectorCommand::draw_path(
        rect_path(0.0, 0.0, w, h),
        Some(VectorPaint::Solid([245, 245, 245, 255])),
        None,
        transform,
    );
    // ref renderer.ts:319-323: `#888` at 11 px, centred in the chart frame.
    let label = make_simple_run(
        "Unsupported chart",
        11.0,
        [136, 136, 136, 255],
        w / 2.0,
        h / 2.0,
        "center",
    );
    vec![
        background,
        VectorCommand::DrawText {
            runs: vec![label],
            transform,
        },
    ]
}
