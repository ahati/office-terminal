//! Embedded bitmap → inline PNG + affine placement.

use std::io::Cursor;
use std::sync::Arc;

use crate::contracts::AffineTransform;

use crate::metafile::MetafileBitmap;

/// Encodes the decoded grid once; the PNG is shared by raster and SVG.
pub fn encode_png(bitmap: &MetafileBitmap) -> Option<Arc<[u8]>> {
    let image = image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.rgba.clone())?;
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(Arc::from(png.into_boxed_slice()))
}

/// Output-units → element-local EMU placement for the unit square.
pub fn map_placement(bitmap: &MetafileBitmap, unit_to_emu: &AffineTransform) -> AffineTransform {
    super::compose(unit_to_emu, &bitmap.placement)
}
