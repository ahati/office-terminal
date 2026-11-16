use pptx_model::ChartModel;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, NormalizedRect, PathSegment,
    VectorCommand, VectorGradient, VectorPaint, VectorPath, VectorStroke,
};
use crate::compiler::shapes::arc::decompose_arc_to_cubic;
use super::color::resolve_series_color;
use super::layout::{
    make_simple_run, series_data_label_font_pt, series_shows_value_labels, ChartLayout,
};
use super::types::ChartDataBounds;

/// Compile a bubble / bubble3D chart series into vector display list commands.
pub fn compile_bubbles(
    chart: &ChartModel,
    layout: &ChartLayout,
    bounds: &ChartDataBounds,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();

    let bubble_scale = chart.bubble_scale.unwrap_or(100).clamp(0, 300) as f32;
    if bubble_scale <= 0.0 {
        return commands;
    }

    let size_represents_w = chart.bubble_size_represents.as_deref() == Some("w");
    let show_neg = chart.show_negative_bubbles.unwrap_or(false);

    // 1. Compute maximum magnitude across all visible bubbles
    let mut max_magnitude = 0.0f32;
    for series in &chart.series {
        if let Some(sizes) = &series.bubble_sizes {
            for sz in sizes.iter().flatten() {
                if sz.is_finite() && *sz != 0.0 {
                    if *sz < 0.0 && !show_neg {
                        continue;
                    }
                    let abs_sz = sz.abs() as f32;
                    let mag = if size_represents_w {
                        abs_sz
                    } else {
                        abs_sz.sqrt()
                    };
                    if mag > max_magnitude {
                        max_magnitude = mag;
                    }
                }
            }
        }
    }
    if max_magnitude <= 0.0 {
        max_magnitude = 1.0;
    }

    // 2. Scale factor for bubble diameters
    let short_side = layout.plot_w.min(layout.plot_h);
    let max_diameter = short_side * bubble_scale / (300.0 + bubble_scale);
    let scale_factor = max_diameter / max_magnitude;

    // 3. Coordinate bounds mapping
    let num_cats = bounds.num_categories.max(1);
    let span_y = (bounds.max_val - bounds.min_val).max(1e-6) as f32;

    let mut x_min = chart.cat_axis_min.unwrap_or(f64::MAX);
    let mut x_max = chart.cat_axis_max.unwrap_or(f64::MIN);
    let mut numeric_x = chart.cat_axis_min.is_some() || chart.cat_axis_max.is_some();

    let cats: Vec<Option<f64>> = chart
        .categories
        .iter()
        .map(|c| c.trim().parse::<f64>().ok())
        .collect();
    if cats.iter().any(|c| c.is_some()) {
        numeric_x = true;
        for c in cats.iter().flatten() {
            if chart.cat_axis_min.is_none() {
                x_min = x_min.min(*c);
            }
            if chart.cat_axis_max.is_none() {
                x_max = x_max.max(*c);
            }
        }
    }
    if !numeric_x || x_min >= x_max {
        x_min = 0.0;
        x_max = num_cats as f64;
    }
    let span_x = (x_max - x_min).max(1e-6) as f32;

    // 4. Render bubbles series by series
    for (s_idx, series) in chart.series.iter().enumerate() {
        let is_3d = series
            .bubble_3d
            .unwrap_or_else(|| series.bubble_3d_group_default.unwrap_or(false));
        let base_color = resolve_series_color(s_idx, series);
        let show_labels = series_shows_value_labels(chart, series);
        let label_font_pt = series_data_label_font_pt(chart, series, layout.plot_h);

        let pt_count = series.values.len();
        for pt_idx in 0..pt_count {
            let y_val = match series.values.get(pt_idx).copied().flatten() {
                Some(y) if y.is_finite() => y,
                _ => continue,
            };

            let x_val = if let Some(Some(x)) = cats.get(pt_idx) {
                *x
            } else {
                pt_idx as f64 + 0.5
            };

            let raw_size = series
                .bubble_sizes
                .as_ref()
                .and_then(|sizes| sizes.get(pt_idx).copied().flatten())
                .unwrap_or(1.0);
            if !raw_size.is_finite() || raw_size == 0.0 || (raw_size < 0.0 && !show_neg) {
                continue;
            }

            let abs_sz = raw_size.abs() as f32;
            let mag = if size_represents_w {
                abs_sz
            } else {
                abs_sz.sqrt()
            };
            let diameter = (mag * scale_factor).max(12700.0); // min 1 pt diameter
            let radius = diameter / 2.0;

            let norm_x = (((x_val - x_min) as f32) / span_x).clamp(0.0, 1.0);
            let cx = layout.plot_x + norm_x * layout.plot_w;

            let norm_y = (((y_val - bounds.min_val) as f32) / span_y).clamp(0.0, 1.0);
            let cy = layout.plot_y + layout.plot_h - norm_y * layout.plot_h;

            let circle = circle_path(cx, cy, radius);

            // Base bubble fill
            commands.push(VectorCommand::draw_path(
                circle.clone(),
                Some(VectorPaint::Solid(base_color)),
                None,
                transform,
            ));

            // 3D bubble material lighting layers
            if is_3d {
                commands.push(VectorCommand::PushClip(circle.clone()));

                let rect = super::layout::rect_path(
                    cx - radius,
                    cy - radius,
                    radius * 2.0,
                    radius * 2.0,
                );

                // Layer 1: Diffuse highlight (4 stops)
                let diffuse_paint = bubble_3d_gradient(
                    NormalizedRect {
                        x: 0.42,
                        y: 0.33,
                        w: 0.0,
                        h: 0.0,
                    },
                    vec![
                        (0.00, [255, 255, 255, 184]),
                        (0.14, [255, 255, 255, 122]),
                        (0.38, [255, 255, 255, 26]),
                        (1.00, [255, 255, 255, 0]),
                    ],
                );
                commands.push(VectorCommand::draw_path(
                    rect.clone(),
                    Some(diffuse_paint),
                    None,
                    transform,
                ));

                // Layer 2: Shade (5 stops)
                let shade_paint = bubble_3d_gradient(
                    NormalizedRect {
                        x: 0.42,
                        y: 0.32,
                        w: 0.0,
                        h: 0.0,
                    },
                    vec![
                        (0.00, [0, 0, 0, 0]),
                        (0.30, [0, 0, 0, 0]),
                        (0.46, [0, 0, 0, 56]),
                        (0.66, [0, 0, 0, 122]),
                        (1.00, [0, 0, 0, 158]),
                    ],
                );
                commands.push(VectorCommand::draw_path(
                    rect.clone(),
                    Some(shade_paint),
                    None,
                    transform,
                ));

                // Layer 3: Lower rim (6 stops)
                let rim_paint = bubble_3d_gradient(
                    NormalizedRect {
                        x: 0.30,
                        y: 0.05,
                        w: 0.0,
                        h: 0.0,
                    },
                    vec![
                        (0.00, [255, 255, 255, 0]),
                        (0.76, [255, 255, 255, 0]),
                        (0.82, [255, 255, 255, 13]),
                        (0.87, [255, 255, 255, 31]),
                        (0.95, [255, 255, 255, 71]),
                        (1.00, [255, 255, 255, 0]),
                    ],
                );
                commands.push(VectorCommand::draw_path(
                    rect,
                    Some(rim_paint),
                    None,
                    transform,
                ));

                commands.push(VectorCommand::PopClip);
            }

            // Bubble border stroke
            let stroke = VectorStroke {
                color: if is_3d {
                    [255, 255, 255, 180]
                } else {
                    [255, 255, 255, 255]
                },
                width_emu: 12700, // 1 pt
                dash_segments: None,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                paint: None,
                ..VectorStroke::default()
            };
            commands.push(VectorCommand::draw_path(
                circle,
                None,
                Some(stroke),
                transform,
            ));

            // Data label
            if show_labels {
                let text = format!("{y_val:.1}");
                let run = make_simple_run(
                    &text,
                    label_font_pt,
                    [50, 50, 50, 255],
                    cx,
                    cy + label_font_pt * 6350.0,
                    "center",
                );
                commands.push(VectorCommand::DrawText {
                    runs: vec![run],
                    transform,
                });
            }
        }
    }

    commands
}

fn circle_path(cx: f32, cy: f32, r: f32) -> VectorPath {
    let mut segs = Vec::with_capacity(6);
    segs.push(PathSegment::MoveTo { x: cx + r, y: cy });
    decompose_arc_to_cubic(cx, cy, r, r, 0.0, 360.0, &mut segs);
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn bubble_3d_gradient(
    fill_to_rect: NormalizedRect,
    stops: Vec<(f32, [u8; 4])>,
) -> VectorPaint {
    let mut grad = VectorGradient::new(0.0, stops);
    grad.frame.fill_to_rect = Some(fill_to_rect);
    VectorPaint::RadialGradient(grad)
}
