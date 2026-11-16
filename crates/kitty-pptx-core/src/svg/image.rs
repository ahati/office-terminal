use std::fmt::Write as _;

use base64::prelude::*;
use crate::contracts::{AffineTransform, NormalizedRect, RectF32, VectorImageFill, VectorPattern};
use crate::loader::{AssetStore, SizeHint};
use super::path::format_transform_attr;

#[allow(clippy::too_many_arguments)]
pub fn serialize_svg_image(
    assets: &dyn AssetStore,
    rel_id: &str,
    src_rect: &NormalizedRect,
    dest_rect: &RectF32,
    opacity: f32,
    transform: &AffineTransform,
    effects: &[ooxml_common::blip::BlipEffect],
) -> String {
    // §20.1.8.55 crop window. A crop with no visible extent paints nothing,
    // mirroring `drawImageCropped`'s `sw > 0 && sh > 0` guard
    // (packages/core/src/image/crop.ts:100-105, 213-218).
    let source = match sanitized_source_window(src_rect) {
        Some(source) => source,
        None => return String::new(),
    };
    if !dest_rect.w.is_finite()
        || !dest_rect.h.is_finite()
        || dest_rect.w <= 0.0
        || dest_rect.h <= 0.0
    {
        return String::new();
    }

    let raw_asset = if effects.is_empty() {
        assets.get_raw_bytes(rel_id).ok()
    } else {
        None
    };
    let (bytes, mime) = if let Some(raw) = raw_asset {
        let detected_mime = detect_mime(rel_id, &raw);
        (raw, detected_mime)
    } else if let Ok(decoded) = assets.get(rel_id, SizeHint::Native) {
        let mut pixels = decoded.pixels.to_vec();
        if !effects.is_empty() {
            crate::rasterizer::effects::apply_blip_effects(&mut pixels, effects);
        }
        // Encode decoded RGBA back to PNG
        let mut png_bytes = Vec::new();
        if let Some(rgba) = image::RgbaImage::from_raw(decoded.width, decoded.height, pixels) {
            let _ = rgba.write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            );
        }
        (png_bytes, "image/png".to_string())
    } else {
        return serialize_svg_missing_picture(dest_rect, transform, opacity);
    };

    if bytes.is_empty() {
        return serialize_svg_missing_picture(dest_rect, transform, opacity);
    }

    let b64 = BASE64_STANDARD.encode(&bytes);
    let data_uri = format!("data:{};base64,{}", mime, b64);
    let transform_attr = format_transform_attr(transform);
    let opacity_attr = if (opacity - 1.0).abs() > 1e-3 {
        format!(r#" opacity="{:.2}""#, opacity)
    } else {
        String::new()
    };

    if is_full_window(&source) {
        // No crop: one `<image>` covering the destination box, unchanged from
        // the pre-crop serializer output.
        return format!(
            r#"  <image href="{}" x="{}" y="{}" width="{}" height="{}"{}{}/>"#,
            data_uri,
            dest_rect.x,
            dest_rect.y,
            dest_rect.w,
            dest_rect.h,
            opacity_attr,
            transform_attr
        );
    }

    // Cropped: a nested `<svg>` viewport whose `viewBox` is the visible source
    // window and whose content is the whole image placed on the normalized
    // `[0, 1]` source frame. `preserveAspectRatio="none"` stretches that window
    // over the full destination box — the reference stretches the cropped slice
    // rather than letterboxing it (`drawImageCropped`, crop.ts:191-231).
    // A `<g>` carries the shape transform so the nested viewport geometry stays
    // in untransformed picture-local EMU coordinates.
    let view_box = format!(
        "{:.6} {:.6} {:.6} {:.6}",
        source.x, source.y, source.w, source.h
    );
    let viewport = format!(
        r#"<svg x="{}" y="{}" width="{}" height="{}" viewBox="{}" preserveAspectRatio="none">"#,
        dest_rect.x, dest_rect.y, dest_rect.w, dest_rect.h, view_box
    );
    let image = format!(
        r#"<image href="{}" x="0" y="0" width="1" height="1"{}/>"#,
        data_uri, opacity_attr
    );

    if transform_attr.is_empty() {
        format!("  {}{}</svg>", viewport, image)
    } else {
        format!("  <g{}>{}{}</svg></g>", transform_attr, viewport, image)
    }
}

/// Clamp a normalized source window to the bitmap and drop windows with no
/// positive extent. Non-finite rectangles mean "no crop" (ref crop.ts:78-79).
fn sanitized_source_window(src: &NormalizedRect) -> Option<NormalizedRect> {
    if ![src.x, src.y, src.w, src.h].iter().all(|v| v.is_finite()) {
        return Some(NormalizedRect::default());
    }
    if src.w <= 0.0 || src.h <= 0.0 {
        return None;
    }
    let x0 = src.x.max(0.0);
    let y0 = src.y.max(0.0);
    let x1 = (src.x + src.w).min(1.0);
    let y1 = (src.y + src.h).min(1.0);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(NormalizedRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

fn is_full_window(src: &NormalizedRect) -> bool {
    const EPS: f32 = 1e-6;
    src.x.abs() < EPS
        && src.y.abs() < EPS
        && (src.w - 1.0).abs() < EPS
        && (src.h - 1.0).abs() < EPS
}

fn detect_mime(path: &str, bytes: &[u8]) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") || bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg".to_string()
    } else if lower.ends_with(".webp") || (bytes.len() >= 12 && &bytes[8..12] == b"WEBP") {
        "image/webp".to_string()
    } else if lower.ends_with(".svg") || bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg") {
        "image/svg+xml".to_string()
    } else {
        "image/png".to_string()
    }
}

/// Paint-server def for a non-background `a:blipFill` (§20.1.8.14) on the shape
/// box `(x, y, w, h)` in shape-local EMU.
///
/// Both fill modes become a `<pattern patternUnits="userSpaceOnUse">` so the
/// brush keeps the shape's own transform and the two placements stay one code
/// path: stretch registers one cell over the `fillRect` destination, tile
/// registers the native repeated cell at its `algn`/`tx`/`ty` phase.
/// ref renderer.ts:2434-2585, 2620-2700.
pub fn emit_image_fill_def(
    assets: &dyn AssetStore,
    defs: &mut String,
    id: &str,
    fill: &VectorImageFill,
    fill_box: (f32, f32, f32, f32),
) -> bool {
    if !fill.mode_is_paintable() {
        return false;
    }
    let (box_x, box_y, box_w, box_h) = fill_box;
    if !(box_w > 0.0 && box_h > 0.0) {
        return false;
    }
    // §20.1.8.58 tile period: the cropped logical source extent at its authored
    // DPI, scaled by sx/sy (ref renderer.ts:2451-2460).
    let pixel_to_emu = if fill.dpi > 0.0 {
        9525.0 * 96.0 / fill.dpi
    } else {
        9525.0
    };
    let (cell_w, cell_h, phase_x, phase_y) = match &fill.tile {
        Some(tile) => {
            let Ok(decoded) = assets.get(&fill.rel_id, SizeHint::Native) else {
                return false;
            };
            if decoded.width == 0 || decoded.height == 0 {
                return false;
            }
            let tile_w = decoded.width as f32 * pixel_to_emu * fill.src_rect.w * tile.sx;
            let tile_h = decoded.height as f32 * pixel_to_emu * fill.src_rect.h * tile.sy;
            if ![tile_w, tile_h].iter().all(|v| v.is_finite() && *v > 0.0) {
                return false;
            }
            let (ax, ay) = tile_anchor(&tile.algn, box_w, box_h, tile_w, tile_h);
            let phase_x = ax + tile.tx;
            let phase_y = ay + tile.ty;
            if ![phase_x, phase_y].iter().all(|v| v.is_finite()) {
                return false;
            }
            let cell_w = tile_w * if tile.flip_x() { 2.0 } else { 1.0 };
            let cell_h = tile_h * if tile.flip_y() { 2.0 } else { 1.0 };
            (cell_w, cell_h, phase_x, phase_y)
        }
        None => {
            let (l, t, w, h) = match &fill.fill_rect {
                Some(fr) => (fr.x, fr.y, fr.w, fr.h),
                None => (0.0, 0.0, 1.0, 1.0),
            };
            let dw = w * box_w;
            let dh = h * box_h;
            if !(dw.is_finite() && dh.is_finite() && dw > 0.0 && dh > 0.0) {
                return false;
            }
            (dw, dh, box_x + l * box_w, box_y + t * box_h)
        }
    };

    let tile_element = |w: f32, h: f32| {
        serialize_svg_image(
            assets,
            &fill.rel_id,
            &fill.src_rect,
            &RectF32::new(0.0, 0.0, w, h),
            1.0,
            &AffineTransform::default(),
            &fill.effects,
        )
    };
    let tile_w = cell_w / if fill.tile.as_ref().is_some_and(|t| t.flip_x()) {
        2.0
    } else {
        1.0
    };
    let tile_h = cell_h / if fill.tile.as_ref().is_some_and(|t| t.flip_y()) {
        2.0
    } else {
        1.0
    };
    let cell = tile_element(tile_w, tile_h);
    if cell.is_empty() {
        return false;
    }

    let _ = writeln!(
        defs,
        r#"    <pattern id="{id}" patternUnits="userSpaceOnUse" x="{phase_x}" y="{phase_y}" width="{cell_w}" height="{cell_h}">"#
    );
    let cell_id = format!("{id}_cell");
    let _ = writeln!(defs, r#"      <g id="{cell_id}">"#);
    for line in cell.lines() {
        defs.push_str("        ");
        defs.push_str(line.trim_start());
        defs.push('\n');
    }
    let _ = writeln!(defs, "      </g>");
    let _ = writeln!(defs, r##"      <use href="#{cell_id}"/>"##);
    if let Some(tile) = &fill.tile {
        // The mirrored neighbours reproduce the tile-flip cadence
        // (ref renderer.ts:2462-2471, 2511-2520).
        if tile.flip_x() {
            let _ = writeln!(
                defs,
                r##"      <use href="#{cell_id}" transform="translate({} 0) scale(-1 1)"/>"##,
                tile_w * 2.0
            );
        }
        if tile.flip_y() {
            let _ = writeln!(
                defs,
                r##"      <use href="#{cell_id}" transform="translate(0 {}) scale(1 -1)"/>"##,
                tile_h * 2.0
            );
        }
        if tile.flip_x() && tile.flip_y() {
            let _ = writeln!(
                defs,
                r##"      <use href="#{cell_id}" transform="translate({} {}) scale(-1 -1)"/>"##,
                tile_w * 2.0,
                tile_h * 2.0
            );
        }
    }
    let _ = writeln!(defs, "    </pattern>");
    true
}

/// Paint-server def for a preset `a:pattFill` (§20.1.8.40).
///
/// The measured 64×64 cell is emitted on the `PATTERN_CELL_EMU` grid anchored at
/// the slide origin, so the point grid is independent of the brush path
/// (ref core/src/shape/paint.ts:459-479, pattern-bitmaps.ts:80-108).
pub fn emit_pattern_fill_def(defs: &mut String, id: &str, pattern: &VectorPattern) {
    let Some(rows) = crate::compiler::pattern::pattern_rows(&pattern.preset) else {
        return;
    };
    let rows: Vec<&str> = rows.split('/').collect();
    let cell = crate::contracts::PATTERN_CELL_EMU;
    let midpoint = [
        u16::from(pattern.fg[0]).wrapping_add(u16::from(pattern.bg[0])).div_ceil(2) as u8,
        u16::from(pattern.fg[1]).wrapping_add(u16::from(pattern.bg[1])).div_ceil(2) as u8,
        u16::from(pattern.fg[2]).wrapping_add(u16::from(pattern.bg[2])).div_ceil(2) as u8,
        u16::from(pattern.fg[3]).wrapping_add(u16::from(pattern.bg[3])).div_ceil(2) as u8,
    ];
    let _ = writeln!(
        defs,
        r#"    <pattern id="{id}" patternUnits="userSpaceOnUse" x="0" y="0" width="{tile}" height="{tile}">"#,
        tile = cell * 8.0
    );
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.as_bytes().iter().enumerate() {
            let [r, g, b, a] = match ch {
                b'#' => pattern.fg,
                b'+' => midpoint,
                _ => pattern.bg,
            };
            let _ = writeln!(
                defs,
                r#"      <rect x="{}" y="{}" width="{cell}" height="{cell}" fill="rgb({r},{g},{b})" fill-opacity="{:.3}"/>"#,
                x as f32 * cell,
                y as f32 * cell,
                a as f32 / 255.0
            );
        }
    }
    let _ = writeln!(defs, "    </pattern>");
}

/// Paint-server def for an inline texture fill (`VectorPaint::Texture`).
pub fn emit_texture_fill_def(defs: &mut String, id: &str, texture: &crate::contracts::VectorTextureFill) {
    let b64 = BASE64_STANDARD.encode(&texture.png);
    let t = &texture.transform;
    let transform_attr = format_transform_attr(t);
    let opacity_attr = if (texture.opacity - 1.0).abs() > 1e-3 {
        format!(r#" opacity="{:.2}""#, texture.opacity)
    } else {
        String::new()
    };
    let w = texture.width;
    let h = texture.height;
    let _ = writeln!(
        defs,
        r#"    <pattern id="{id}" patternUnits="userSpaceOnUse" x="0" y="0" width="{w}" height="{h}"{transform_attr}>
      <image x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" href="data:image/png;base64,{b64}"{opacity_attr}/>
    </pattern>"#
    );
}

/// Anchor of the first tile for an `ST_RectAlignment` value
/// (ref renderer.ts:2377-2403, `tileAnchorOffset`).
fn tile_anchor(algn: &str, box_w: f32, box_h: f32, tile_w: f32, tile_h: f32) -> (f32, f32) {
    let ax = match algn {
        "t" | "ctr" | "b" => (box_w - tile_w) / 2.0,
        "tr" | "r" | "br" => box_w - tile_w,
        _ => 0.0,
    };
    let ay = match algn {
        "l" | "ctr" | "r" => (box_h - tile_h) / 2.0,
        "bl" | "b" | "br" => box_h - tile_h,
        _ => 0.0,
    };
    (ax, ay)
}

/// Serializes an inline bitmap (`VectorCommand::DrawBitmap`) as a data URI.
pub fn serialize_svg_bitmap(
    png: &[u8],
    placement: &AffineTransform,
    opacity: f32,
) -> String {
    let b64 = BASE64_STANDARD.encode(png);
    let transform_attr = format_transform_attr(placement);
    let opacity_attr = if (opacity - 1.0).abs() > 1e-3 {
        format!(r#" opacity="{:.2}""#, opacity)
    } else {
        String::new()
    };
    format!(
        r#"  <image x="0" y="0" width="1" height="1" preserveAspectRatio="none" href="data:image/png;base64,{b64}"{transform_attr}{opacity_attr}/>"#
    )
}

/// Neutral placeholder for a picture whose asset is missing or undecodable.
///
/// Also used for an unexpanded `DrawMetafile` marker. A dashed frame keeps the
/// failure visible; no `<image>` with an empty data URI is emitted.
pub fn serialize_svg_missing_picture(
    dest_rect: &RectF32,
    transform: &AffineTransform,
    opacity: f32,
) -> String {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return String::new();
    }
    let local = AffineTransform {
        tx: dest_rect.x,
        ty: dest_rect.y,
        ..AffineTransform::default()
    };
    let placed = transform.post_concat(&local);
    let transform_attr = format_transform_attr(&placed);
    format!(
        r##"  <rect x="0" y="0" width="{:.1}" height="{:.1}" fill="#f2f2f2" fill-opacity="{:.2}" stroke="#969696" stroke-opacity="{:.2}" stroke-width="1.5" stroke-dasharray="4 3"{transform_attr}/>"##,
        dest_rect.w.max(1.0),
        dest_rect.h.max(1.0),
        0.6 * opacity,
        opacity
    )
}

/// Backwards-compatible name for the metafile-marker fallback.
pub fn serialize_svg_missing_metafile(
    dest_rect: &RectF32,
    transform: &AffineTransform,
) -> String {
    serialize_svg_missing_picture(dest_rect, transform, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AssetError;
    use crate::loader::{AssetStoreStats, DecodedImage};

    struct MissingAsset;

    impl AssetStore for MissingAsset {
        fn get(&self, rel_id: &str, _: SizeHint) -> Result<DecodedImage, AssetError> {
            Err(AssetError::NotFound(rel_id.to_string()))
        }

        fn stats(&self) -> AssetStoreStats {
            AssetStoreStats::default()
        }
    }

    #[test]
    fn missing_picture_serializes_a_visible_placeholder() {
        let svg = serialize_svg_image(
            &MissingAsset,
            "ppt/media/missing.png",
            &NormalizedRect::default(),
            &RectF32::new(10.0, 20.0, 30.0, 40.0),
            1.0,
            &AffineTransform::default(),
            &[],
        );
        assert!(svg.contains("<rect "), "{svg}");
        assert!(svg.contains("stroke-dasharray"), "{svg}");
        assert!(!svg.contains("<image"), "no empty image URI: {svg}");
    }
}

#[cfg(test)]
mod opacity_tests {
    use super::*;
    use crate::error::AssetError;
    use crate::loader::{AssetStoreStats, DecodedImage};

    struct MissingAsset;

    impl AssetStore for MissingAsset {
        fn get(&self, rel_id: &str, _: SizeHint) -> Result<DecodedImage, AssetError> {
            Err(AssetError::NotFound(rel_id.to_string()))
        }

        fn stats(&self) -> AssetStoreStats {
            AssetStoreStats::default()
        }
    }

    #[test]
    fn fully_transparent_missing_picture_emits_nothing() {
        let svg = serialize_svg_image(
            &MissingAsset,
            "ppt/media/missing.png",
            &NormalizedRect::default(),
            &RectF32::new(10.0, 20.0, 30.0, 40.0),
            0.0,
            &AffineTransform::default(),
            &[],
        );
        assert!(svg.is_empty(), "an invisible picture must stay invisible: {svg}");
    }
}
