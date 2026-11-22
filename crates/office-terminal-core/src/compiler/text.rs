mod text_warp_compile;

use crate::contracts::{AffineTransform, VectorCommand, WarningEvent};
use crossbeam_channel::Sender;

/// Lay out a text body.
///
/// WordArt bodies keep the warp-aware emitter; everything else goes through the
/// parity port in [`crate::text::emit`] (wrap, spacing, anchor, per-line
/// baselines). Reference: `renderer.ts:4989` dispatches the warp path first.
pub fn compile_text_body(
    text_body: &pptx_model::TextBody,
    box_w: i64,
    box_h: i64,
    base_transform: &AffineTransform,
    default_shape_text_color: Option<&str>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    if text_body.text_warp.is_none() {
        return crate::text::emit::compile_text_body(
            text_body,
            box_w,
            box_h,
            base_transform,
            default_shape_text_color,
            warning_sink,
        );
    }
    text_warp_compile::compile_text_body_warped(
        text_body,
        box_w,
        box_h,
        base_transform,
        default_shape_text_color,
        warning_sink,
    )
}

pub fn resolve_font_family(name: &str, warning_sink: &Sender<WarningEvent>) -> String {
    let resolved = family_substitute(name).unwrap_or(name).to_string();
    if name.to_lowercase().contains("aptos") {
        let _ = warning_sink.send(WarningEvent::FontSubstituted {
            requested: name.to_string(),
            substitute: resolved.clone(),
        });
    }
    resolved
}

/// The bundled/system face a requested family is mapped to, if any.
///
/// Shared by the PPTX text compiler and metafile playback so both resolve
/// faces through the same substitution matrix (PRD 13 §4.5).
pub fn family_substitute(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if lower.contains("calibri") {
        Some("Carlito")
    } else if lower.contains("cambria") {
        Some("Caladea")
    } else if lower.contains("aptos") {
        Some("Carlito")
    } else if lower.contains("arial") {
        Some("Liberation Sans")
    } else if lower.contains("times") {
        Some("Liberation Serif")
    } else if lower.contains("consolas") || lower.contains("courier") {
        Some("Liberation Mono")
    } else {
        None
    }
}

pub fn map_bullet_char(ch: &str, font_family: Option<&str>) -> String {
    let is_symbol_font = font_family
        .map(|f| {
            let fl = f.to_lowercase();
            fl.contains("wingdings") || fl.contains("symbol") || fl.contains("webdings")
        })
        .unwrap_or(false);

    if let Some(first_char) = ch.chars().next() {
        let code = first_char as u32;
        if is_symbol_font {
            match code {
                0x6C | 0xF06C => return "●".to_string(),
                0x6E | 0xF06E => return "■".to_string(),
                0x75 | 0xF075 => return "◆".to_string(),
                0xD8 | 0xF0D8 => return "➢".to_string(),
                0xFC | 0xF0FC => return "✔".to_string(),
                0xB7 | 0xF0B7 => return "•".to_string(),
                _ => {}
            }
        }
    }

    if ch.is_empty() {
        "•".to_string()
    } else {
        ch.to_string()
    }
}
