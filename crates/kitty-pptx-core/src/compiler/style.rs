use crate::contracts::{
    LineCap, LineJoin, VectorGradient, VectorPaint, VectorStroke,
};

/// Parse a hex color string (e.g. "RRGGBB", "#RRGGBB", "RRGGBBAA", "#RRGGBBAA").
/// Defaults to magenta `[255, 0, 255, 255]` on error.
pub fn parse_hex_color(hex: &str) -> [u8; 4] {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
        [r, g, b, 255]
    } else if clean.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
        let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
        [r, g, b, a]
    } else {
        [255, 0, 255, 255]
    }
}

/// Apply opacity multiplier to an RGBA color.
pub fn modulate_alpha(mut color: [u8; 4], alpha: f32) -> [u8; 4] {
    let a = (color[3] as f32 * alpha.clamp(0.0, 1.0)).round() as u8;
    color[3] = a;
    color
}

/// Convert a pptx_model::Fill to VectorPaint.
pub fn compile_fill(fill: &pptx_model::Fill, alpha: f32) -> Option<VectorPaint> {
    match fill {
        pptx_model::Fill::Solid { color } => {
            let c = modulate_alpha(parse_hex_color(color), alpha);
            Some(VectorPaint::Solid(c))
        }
        pptx_model::Fill::Gradient {
            stops,
            angle,
            grad_type,
            ..
        } => {
            let gradient_stops: Vec<(f32, [u8; 4])> = stops
                .iter()
                .map(|stop| {
                    let col = modulate_alpha(parse_hex_color(&stop.color), alpha);
                    (stop.position as f32, col)
                })
                .collect();

            let grad = VectorGradient {
                angle_deg: *angle as f32,
                stops: gradient_stops,
            };

            if grad_type == "radial" {
                Some(VectorPaint::RadialGradient(grad))
            } else {
                Some(VectorPaint::LinearGradient(grad))
            }
        }
        pptx_model::Fill::Pattern { fg, .. } => {
            // Degraded fallback: solid fill in pattern primary foreground color
            let c = modulate_alpha(parse_hex_color(fg), alpha);
            Some(VectorPaint::Solid(c))
        }
        pptx_model::Fill::None => None,
        pptx_model::Fill::Image { .. } => {
            // Image fills are handled as pictures or background images
            None
        }
    }
}

/// Convert a pptx_model::Stroke to VectorStroke.
pub fn compile_stroke(stroke: &pptx_model::Stroke) -> Option<VectorStroke> {
    let color = parse_hex_color(&stroke.color);
    let width_emu = stroke.width;

    // Dash patterns
    let dash_segments = if !stroke.custom_dash.is_empty() {
        let mut segments = Vec::new();
        for seg in &stroke.custom_dash {
            segments.push(seg.dash as f32);
            segments.push(seg.space as f32);
        }
        Some(segments)
    } else if let Some(ref style) = stroke.dash_style {
        match style.as_str() {
            "solid" => None,
            "dot" => Some(vec![1.0, 3.0]),
            "dash" => Some(vec![4.0, 3.0]),
            "lgDash" => Some(vec![8.0, 3.0]),
            "dashDot" => Some(vec![4.0, 3.0, 1.0, 3.0]),
            "lgDashDot" => Some(vec![8.0, 3.0, 1.0, 3.0]),
            "lgDashDotDot" => Some(vec![8.0, 3.0, 1.0, 3.0, 1.0, 3.0]),
            "sysDash" => Some(vec![2.0, 2.0]),
            "sysDot" => Some(vec![1.0, 1.0]),
            _ => None,
        }
    } else {
        None
    };

    let line_cap = match stroke.line_cap.as_deref() {
        Some("rnd") | Some("round") => LineCap::Round,
        Some("sq") | Some("square") => LineCap::Square,
        Some("flat") | Some("butt") => LineCap::Butt,
        _ => LineCap::Square,
    };

    let line_join = match stroke.line_join.as_deref() {
        Some("round") => LineJoin::Round,
        Some("bevel") => LineJoin::Bevel,
        Some("miter") => LineJoin::Miter,
        _ => LineJoin::Miter,
    };

    Some(VectorStroke {
        color,
        width_emu,
        dash_segments,
        line_cap,
        line_join,
    })
}
