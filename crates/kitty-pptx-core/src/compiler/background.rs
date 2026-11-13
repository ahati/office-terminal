use crate::contracts::{BackgroundTile, NormalizedRect, RectF32, VectorBackground, VectorGradient};
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
        Some(pptx_model::Fill::Image {
            image_path,
            src_rect,
            fill_rect,
            tile,
            blip_effects,
            alpha,
            ..
        }) => Some(VectorBackground::Image {
            rel_id: image_path.clone(),
            src_rect: source_window(src_rect.as_ref()),
            fill_rect: fill_rect.as_ref().map(destination_rect),
            tile: tile.as_ref().map(background_tile),
            opacity: image_opacity(alpha.as_ref(), blip_effects),
        }),
        _ => Some(VectorBackground::Solid([255, 255, 255, 255])),
    }
}

/// `<a:srcRect>` (§20.1.8.55) as the *logical* source window
/// `[l, t, 1−r, 1−b]` of the native bitmap. The window keeps negative/overscan
/// insets outside `[0, 1]`; renderers clamp it to the bitmap and stretch the
/// visible slice over the destination box (`drawImageCropped`,
/// ref core/src/image/crop.ts:76-111). Non-finite edges mean "no crop".
fn source_window(src_rect: Option<&ooxml_common::blip::SrcRect>) -> NormalizedRect {
    let Some(sr) = src_rect else {
        return NormalizedRect::default();
    };
    if ![sr.l, sr.t, sr.r, sr.b].iter().all(|v| v.is_finite()) {
        return NormalizedRect::default();
    }
    NormalizedRect {
        x: sr.l as f32,
        y: sr.t as f32,
        w: (1.0 - sr.l - sr.r) as f32,
        h: (1.0 - sr.b - sr.t) as f32,
    }
}

/// `<a:stretch><a:fillRect>` (§20.1.8.30) as fractions of the slide box.
/// Negative insets overscan past the box (ref renderer.ts:2298-2306).
fn destination_rect(fill_rect: &pptx_model::FillRect) -> RectF32 {
    RectF32::new(
        fill_rect.l as f32,
        fill_rect.t as f32,
        (1.0 - fill_rect.l - fill_rect.r) as f32,
        (1.0 - fill_rect.t - fill_rect.b) as f32,
    )
}

/// `<a:tile>` (§20.1.8.58) placement facts. Absent attributes keep the schema
/// / host defaults the reference applies: `sx`/`sy` = 1, `flip` = none,
/// `algn` = top-left (ref renderer.ts:2377-2403, 2543-2559).
fn background_tile(tile: &pptx_model::TileInfo) -> BackgroundTile {
    BackgroundTile {
        tx: tile.tx.unwrap_or(0),
        ty: tile.ty.unwrap_or(0),
        sx: tile.sx.unwrap_or(1.0) as f32,
        sy: tile.sy.unwrap_or(1.0) as f32,
        flip: tile.flip.clone().unwrap_or_else(|| "none".to_string()),
        algn: tile.algn.clone().unwrap_or_else(|| "tl".to_string()),
    }
}

/// Blip opacity. `<a:alphaModFix>` is baked into the ordered `blip_effects`
/// chain by the parser (the legacy `alpha` field stays `None`), so the last
/// authored modulation is the effective multiplier — the same value the
/// reference stores as `fill.alpha` and paints through `globalAlpha`
/// (ref renderer.ts:2315).
fn image_opacity(
    legacy_alpha: Option<&f64>,
    blip_effects: &[ooxml_common::blip::BlipEffect],
) -> f32 {
    if let Some(alpha) = legacy_alpha {
        return (*alpha as f32).clamp(0.0, 1.0);
    }
    blip_effects
        .iter()
        .rev()
        .find_map(|effect| match effect {
            ooxml_common::blip::BlipEffect::AlphaModFix { amt } => Some(*amt as f32),
            _ => None,
        })
        .map(|amt| amt.clamp(0.0, 1.0))
        .unwrap_or(1.0)
}
