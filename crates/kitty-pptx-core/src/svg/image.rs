use base64::prelude::*;
use crate::contracts::{AffineTransform, NormalizedRect, RectF32};
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
        return String::new();
    };

    if bytes.is_empty() {
        return String::new();
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
