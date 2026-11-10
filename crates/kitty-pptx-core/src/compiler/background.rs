use crate::contracts::{VectorBackground, VectorGradient};
use super::style::parse_hex_color;

pub fn compile_slide_background(bg: Option<&pptx_model::Fill>) -> Option<VectorBackground> {
    match bg {
        Some(pptx_model::Fill::Solid { color }) => {
            Some(VectorBackground::Solid(parse_hex_color(color)))
        }
        Some(pptx_model::Fill::Gradient {
            stops,
            angle,
            ..
        }) => {
            let gradient_stops: Vec<(f32, [u8; 4])> = stops
                .iter()
                .map(|stop| (stop.position as f32, parse_hex_color(&stop.color)))
                .collect();
            Some(VectorBackground::Gradient(VectorGradient {
                angle_deg: *angle as f32,
                stops: gradient_stops,
            }))
        }
        Some(pptx_model::Fill::Image { image_path, .. }) => {
            Some(VectorBackground::Image {
                rel_id: image_path.clone(),
            })
        }
        _ => Some(VectorBackground::Solid([255, 255, 255, 255])),
    }
}
