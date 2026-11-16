//! Embedded bitmap → inline PNG + affine placement.

use std::io::Cursor;
use std::sync::Arc;

use crate::contracts::AffineTransform;

use crate::metafile::MetafileBitmap;

/// Encodes raw RGBA pixels to PNG; shared by bitmap records and texture brushes.
pub fn encode_raw_png(width: u32, height: u32, rgba: &[u8]) -> Option<Arc<[u8]>> {
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())?;
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(Arc::from(png.into_boxed_slice()))
}

/// Encodes the decoded grid once; the PNG is shared by raster and SVG.
pub fn encode_png(bitmap: &MetafileBitmap) -> Option<Arc<[u8]>> {
    encode_raw_png(bitmap.width, bitmap.height, &bitmap.rgba)
}

/// Output-units → element-local EMU placement for the unit square.
pub fn map_placement(bitmap: &MetafileBitmap, unit_to_emu: &AffineTransform) -> AffineTransform {
    super::compose(unit_to_emu, &bitmap.placement)
}
