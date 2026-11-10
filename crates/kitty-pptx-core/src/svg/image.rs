use base64::prelude::*;
use crate::contracts::{AffineTransform, NormalizedRect, RectF32};
use crate::loader::{AssetStore, SizeHint};
use super::path::format_transform_attr;

pub fn serialize_svg_image(
    assets: &dyn AssetStore,
    rel_id: &str,
    _src_rect: &NormalizedRect,
    dest_rect: &RectF32,
    opacity: f32,
    transform: &AffineTransform,
) -> String {
    let (bytes, mime) = if let Ok(raw) = assets.get_raw_bytes(rel_id) {
        let detected_mime = detect_mime(rel_id, &raw);
        (raw, detected_mime)
    } else if let Ok(decoded) = assets.get(rel_id, SizeHint::Native) {
        // Encode decoded RGBA back to PNG
        let mut png_bytes = Vec::new();
        if let Some(rgba) = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.pixels.to_vec()) {
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

    format!(
        r#"  <image href="{}" x="{}" y="{}" width="{}" height="{}"{}{}/>"#,
        data_uri,
        dest_rect.x,
        dest_rect.y,
        dest_rect.w,
        dest_rect.h,
        opacity_attr,
        transform_attr
    )
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
