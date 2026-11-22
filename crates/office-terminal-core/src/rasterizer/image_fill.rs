//! Shape blip fills (`a:blipFill`, §20.1.8.14) and preset patterns
//! (`a:pattFill`, §20.1.8.40).
//!
//! Both project the reference's canvas brush onto `tiny-skia` pattern shaders:
//! the fill mode decides `SpreadMode::Pad` (stretch) versus
//! `SpreadMode::Repeat` (tile) and one affine maps the retained bitmap grid onto
//! the device path.
//! ref packages/pptx/src/renderer.ts:2434-2585 (`paintTiledBackground`),
//! 2620-2700 (`paintPreparedShapeImageFill`).
//! Blip paint paths: the `<p:pic>` body ([`render_image`]), a shape's
//! `a:blipFill` (§20.1.8.14, [`fill_path_with_image`]) and a preset
//! `a:pattFill` (§20.1.8.40, [`fill_path_with_pattern`]).
//!
//! Both fill variants project the reference's canvas brush onto `tiny-skia`
//! pattern shaders: the fill mode decides `SpreadMode::Pad` (stretch) versus
//! `SpreadMode::Repeat` (tile) and one affine maps the retained bitmap grid
//! onto the device path.
//! ref packages/pptx/src/renderer.ts:2434-2585 (`paintTiledBackground`),
//! 2620-2700 (`paintPreparedShapeImageFill`).

use crate::contracts::{
    AffineTransform, NormalizedRect, EMU_PER_PX, VectorImageFill, VectorPattern, VectorTile,
};
use crate::contracts::PATTERN_CELL_EMU;
use crate::error::RenderError;
use crate::loader::{AssetStore, DecodedImage, SizeHint};

/// Scratch-surface ceiling for one tiled pattern cell
/// (ref core/src/canvas/clamp.ts:35, `MAX_CANVAS_AREA`).
pub const MAX_TILE_CELL_PIXELS: u64 = 1 << 24;

/// Scratch-surface dimension ceiling for one tiled pattern cell
/// (ref core/src/canvas/clamp.ts:21, `MAX_CANVAS_DIMENSION`).
pub const MAX_TILE_CELL_DIMENSION: u32 = 32767;

/// Brush resolved for one image-fill placement: a retained bitmap plus the
/// affine that maps its pixel grid onto canvas pixels.
pub struct ImageFillBrush {
    pixmap: tiny_skia::Pixmap,
    /// Pattern pixel → canvas pixel.
    transform: tiny_skia::Transform,
    /// `true` repeats the cell (tile mode), `false` pads (stretch mode).
    repeat: bool,
    opacity: f32,
}

impl ImageFillBrush {
    /// Pattern pixel → canvas pixel affine, as
    /// `(sx, ky, kx, sy, tx, ty)` (the reference's `DOMMatrix` order).
    pub fn transform(&self) -> (f32, f32, f32, f32, f32, f32) {
        (
            self.transform.sx,
            self.transform.ky,
            self.transform.kx,
            self.transform.sy,
            self.transform.tx,
            self.transform.ty,
        )
    }

    /// `true` when the cell repeats (tile mode).
    pub fn repeats(&self) -> bool {
        self.repeat
    }

    /// Retained bitmap extent in pixels.
    pub fn bitmap_size(&self) -> (u32, u32) {
        (self.pixmap.width(), self.pixmap.height())
    }

    /// Placement opacity (`a:alphaModFix` × caller alpha).
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
}

/// Native tile extent of the §20.1.8.55 window duplicated by tile mode, in EMU.
/// The logical rectangle can be larger than the bitmap for negative outsets;
/// that extra extent stays transparent.
/// ref renderer.ts:2408-2417 (`tileSourceExtent`) and 2451-2460.
pub fn tile_source_extent_emu(fill: &VectorImageFill, decoded: &DecodedImage) -> (f32, f32) {
    let pixel_to_emu = if fill.dpi > 0.0 {
        EMU_PER_PX * 96.0 / fill.dpi
    } else {
        EMU_PER_PX
    };
    let logical_w = if fill.src_rect.w.is_finite() { fill.src_rect.w } else { 1.0 };
    let logical_h = if fill.src_rect.h.is_finite() { fill.src_rect.h } else { 1.0 };
    (
        decoded.width as f32 * pixel_to_emu * logical_w,
        decoded.height as f32 * pixel_to_emu * logical_h,
    )
}

/// Tile cell in bitmap pixels for the `sx`/`sy` scale and `flip` cadence,
/// before the resource budget. A flipped tile repeats a 2×2 mirrored super-cell
/// so the seam between repeats is a mirror line.
/// ref renderer.ts:2462-2471.
pub fn tile_cell_pixels(
    source: (f32, f32),
    tile: &VectorTile,
) -> Result<(u32, u32), RenderError> {
    // `source` is the authored tile extent; the retained cell holds one period
    // per author unit, doubled on each mirrored axis.
    let cell_w = source.0 * if tile.flip_x() { 2.0 } else { 1.0 };
    let cell_h = source.1 * if tile.flip_y() { 2.0 } else { 1.0 };
    if !(cell_w.is_finite() && cell_h.is_finite()) {
        // ref renderer.ts:2475-2477 — "Unrepresentable picture tile cell".
        return Err(RenderError::UnsupportedImageEffect(
            "unrepresentable picture tile cell",
        ));
    }
    let scratch_w = cell_w.abs().ceil().max(1.0);
    let scratch_h = cell_h.abs().ceil().max(1.0);
    if !(scratch_w > 0.0 && scratch_h > 0.0) {
        return Err(RenderError::UnsupportedImageEffect(
            "unrepresentable picture tile size",
        ));
    }
    Ok((scratch_w as u32, scratch_h as u32))
}

/// Resolve the brush for a shape blip fill, or `Ok(None)` when the fill paints
/// nothing (no authored mode, a schema-invalid tile+stretch pair, or an
/// undecodable blip).
///
/// `box` is the fill box in shape-local EMU (`x`, `y`, `w`, `h`); `scale`,
/// `offset_x`/`offset_y` map slide EMU onto canvas pixels.
/// ref renderer.ts:2620-2700.
#[allow(clippy::too_many_arguments)]
pub fn prepare_image_fill(
    assets: &dyn AssetStore,
    fill: &VectorImageFill,
    box_local: (f32, f32, f32, f32),
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Result<Option<ImageFillBrush>, RenderError> {
    let (box_x, box_y, box_w, box_h) = box_local;
    if ![box_x, box_y, box_w, box_h, scale].iter().all(|v| v.is_finite())
        || box_w < 0.0
        || box_h < 0.0
    {
        return Err(RenderError::UnsupportedImageEffect(
            "invalid image fill geometry",
        ));
    }
    // ref renderer.ts:2626, 2629: empty box or no decodable blip paints nothing.
    if box_w <= 0.0 || box_h <= 0.0 || !fill.mode_is_paintable() {
        return Ok(None);
    }
    // Shape blips keep their native grid: tile mode duplicates the retained
    // bitmap (`tileSourceExtent`), so a resize target would change the period.
    let Ok(decoded) = assets.get(&fill.rel_id, SizeHint::Native) else {
        return Ok(None);
    };
    let Some(mut pixmap) = cropped_pixmap(&decoded, &fill.src_rect, &fill.effects) else {
        return Ok(None);
    };

    // Counter-rotation (§20.1.8.33): keep the clipped geometry, cancel the
    // shape rotation about the fill-box centre. ref renderer.ts:2644-2656.
    let rotation = fill.counter_rotation_radians();
    let (center_x, center_y) = (box_x + box_w / 2.0, box_y + box_h / 2.0);

    let (local_x, local_y, local_w, local_h, repeat) = match &fill.tile {
        Some(tile) => {
            // §20.1.8.55 applies before the fill mode: the cropped logical
            // rectangle is the content duplicated, so its native extent is the
            // tile period (ref renderer.ts:2447-2460).
            let (source_w, source_h) = tile_source_extent_emu(fill, &decoded);
            let tile_w = source_w * tile.sx;
            let tile_h = source_h * tile.sy;
            if !(tile_w.is_finite() && tile_h.is_finite() && tile_w > 0.0 && tile_h > 0.0) {
                return Err(RenderError::UnsupportedImageEffect(
                    "unrepresentable picture tile size",
                ));
            }
            let (ax, ay) = tile_anchor(&tile.algn, box_w, box_h, tile_w, tile_h);
            let start_x = ax + tile.tx;
            let start_y = ay + tile.ty;
            if !(start_x.is_finite() && start_y.is_finite()) {
                return Err(RenderError::UnsupportedImageEffect(
                    "unrepresentable picture tile phase",
                ));
            }
            (start_x, start_y, tile_w, tile_h, true)
        }
        None => {
            // ref renderer.ts:2668-2677 — `fillRect` insets the destination;
            // an omitted rect covers the whole box.
            let (l, t, w, h) = match &fill.fill_rect {
                Some(fr) => (fr.x, fr.y, fr.w, fr.h),
                None => (0.0, 0.0, 1.0, 1.0),
            };
            let dx = box_x + l * box_w;
            let dy = box_y + t * box_h;
            let dw = w * box_w;
            let dh = h * box_h;
            if ![dx, dy, dw, dh].iter().all(|v| v.is_finite()) {
                return Err(RenderError::UnsupportedImageEffect(
                    "unrepresentable image fill rectangle",
                ));
            }
            if dw <= 0.0 || dh <= 0.0 {
                return Ok(None);
            }
            (dx, dy, dw, dh, false)
        }
    };

    // §20.1.8.58 mirror cadence: a flipped tile repeats a 2x2 mirrored cell, so
    // the retained grid keeps the authored period and seam line.
    // ref renderer.ts:2462-2471.
    let (cell_w, cell_h) = match &fill.tile {
        Some(tile) => {
            // The retained grid is the bitmap extent, doubled per mirrored axis;
            // the pattern transform stretches it onto the authored period.
            let cell = tile_cell_pixels(
                (pixmap.width() as f32, pixmap.height() as f32),
                tile,
            )?;
            if cell.0 > MAX_TILE_CELL_DIMENSION || cell.1 > MAX_TILE_CELL_DIMENSION {
                return Err(RenderError::UnsupportedImageEffect(
                    "unrepresentable picture tile cell",
                ));
            }
            if tile.flip_x() || tile.flip_y() {
                pixmap = mirror_super_cell(&pixmap, tile.flip_x(), tile.flip_y());
            }
            cell
        }
        None => (pixmap.width(), pixmap.height()),
    };
    if cell_w == 0 || cell_h == 0 {
        return Ok(None);
    }
    if u64::from(cell_w) * u64::from(cell_h) > MAX_TILE_CELL_PIXELS {
        // Resource policy: the retained cell is capped to its bitmap extent
        // (the pattern transform restores the authored extent).
        // ref renderer.ts:2484-2508.
        pixmap = crop_to_budget(&pixmap, MAX_TILE_CELL_PIXELS);
        if pixmap.width() == 0 || pixmap.height() == 0 {
            return Err(RenderError::UnsupportedImageEffect(
                "picture tile surface unavailable",
            ));
        }
    }

    // Pattern pixel grid → shape-local EMU → slide EMU → canvas pixels.
    let local = AffineTransform {
        sx: local_w / cell_w as f32,
        ky: 0.0,
        kx: 0.0,
        sy: local_h / cell_h as f32,
        tx: local_x,
        ty: local_y,
    };
    let rotate = rotation_about(rotation, center_x, center_y);
    let device = compose_device(transform, scale, offset_x, offset_y);
    let brush = device.post_concat(&rotate).post_concat(&local);
    let brush_transform = tiny_skia::Transform::from_row(
        brush.sx,
        brush.ky,
        brush.kx,
        brush.sy,
        brush.tx,
        brush.ty,
    );

    Ok(Some(ImageFillBrush {
        pixmap,
        transform: brush_transform,
        repeat,
        opacity: fill.alpha,
    }))
}

/// Shrink a retained tile cell to the pixel budget, preserving aspect.
/// ref renderer.ts:2496-2508.
fn crop_to_budget(cell: &tiny_skia::Pixmap, budget: u64) -> tiny_skia::Pixmap {
    let w = f64::from(cell.width());
    let h = f64::from(cell.height());
    let reduction = (budget as f64 / (w * h)).sqrt();
    let out_w = ((w * reduction).floor() as u32).max(1);
    let out_h = ((h * reduction).floor() as u32).max(1);
    let Some(mut out) = tiny_skia::Pixmap::new(out_w, out_h) else {
        return cell.clone();
    };
    let mut paint = tiny_skia::PixmapPaint {
        quality: tiny_skia::FilterQuality::Nearest,
        ..Default::default()
    };
    paint.opacity = 1.0;
    out.draw_pixmap(
        0,
        0,
        cell.as_ref(),
        &paint,
        tiny_skia::Transform::from_scale(
            out_w as f32 / cell.width() as f32,
            out_h as f32 / cell.height() as f32,
        ),
        None,
    );
    out
}

/// Paint a shape blip fill inside `path` (canvas device space).
///
/// The reference clips to the shape geometry, isolates alpha/crop state and
/// then paints the prepared brush; here the path fill itself is the clip, so
/// anything outside the silhouette stays untouched.
/// ref renderer.ts:2630-2690.
#[allow(clippy::too_many_arguments)]
pub fn fill_path_with_image(
    pixmap: &mut tiny_skia::Pixmap,
    assets: &dyn AssetStore,
    fill: &VectorImageFill,
    path_local: &crate::contracts::VectorPath,
    device_path: &tiny_skia::Path,
    fill_rule: tiny_skia::FillRule,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Result<(), RenderError> {
    let Some(bounds) = super::path::local_bounds(path_local) else {
        return Ok(());
    };
    let box_local = (
        bounds.0,
        bounds.1,
        bounds.2 - bounds.0,
        bounds.3 - bounds.1,
    );
    // Fail-soft on the paint path: the typed diagnostic is available from
    // `prepare_image_fill`, and a broken brush must not abort the slide.
    let brush = prepare_image_fill(assets, fill, box_local, transform, scale, offset_x, offset_y)
        .unwrap_or_default();
    let Some(brush) = brush else {
        return Ok(());
    };
    fill_path_with_brush(pixmap, device_path, fill_rule, &brush);
    Ok(())
}

/// Paint a preset pattern fill inside `path` (canvas device space) on the
/// `PATTERN_CELL_EMU` grid anchored at the caller's coordinate origin.
/// ref core/src/shape/paint.ts:459-479, 486-508.
pub fn fill_path_with_pattern(
    pixmap: &mut tiny_skia::Pixmap,
    pattern: &VectorPattern,
    device_path: &tiny_skia::Path,
    fill_rule: tiny_skia::FillRule,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let Some(pixels) =
        crate::compiler::pattern::build_pattern_bitmap(&pattern.preset, pattern.fg, pattern.bg)
    else {
        // Unknown preset: the reference falls back to the foreground colour, but
        // `compile_shape_fill` already replaced those, so nothing is painted.
        return;
    };
    let edge = crate::compiler::pattern::PATTERN_BITMAP_EDGE;
    let Some(mut cell) = tiny_skia::Pixmap::new(edge, edge) else {
        return;
    };
    premultiply_into(cell.data_mut(), &pixels);
    // One bitmap pixel is 1/8 pt; the grid is anchored at the canvas origin
    // (`e`/`f` = 0 in `resolvePatternFill`'s pattern transform).
    let step = PATTERN_CELL_EMU * scale;
    let brush = ImageFillBrush {
        pixmap: cell,
        transform: tiny_skia::Transform::from_row(step, 0.0, 0.0, step, offset_x, offset_y),
        repeat: true,
        opacity: 1.0,
    };
    fill_path_with_brush(pixmap, device_path, fill_rule, &brush);
}

fn fill_path_with_brush(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    fill_rule: tiny_skia::FillRule,
    brush: &ImageFillBrush,
) {
    let spread = if brush.repeat {
        tiny_skia::SpreadMode::Repeat
    } else {
        tiny_skia::SpreadMode::Pad
    };
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.shader = tiny_skia::Pattern::new(
        brush.pixmap.as_ref(),
        spread,
        tiny_skia::FilterQuality::Bilinear,
        brush.opacity.clamp(0.0, 1.0),
        brush.transform,
    );
    pixmap.fill_path(
        path,
        &paint,
        fill_rule,
        tiny_skia::Transform::identity(),
        None,
    );
}

/// Slide EMU → canvas pixels.
fn compose_device(
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> AffineTransform {
    let device = AffineTransform {
        sx: scale,
        ky: 0.0,
        kx: 0.0,
        sy: scale,
        tx: offset_x,
        ty: offset_y,
    };
    device.post_concat(transform)
}

/// Rotation by `radians` about (`cx`, `cy`) in shape-local EMU.
fn rotation_about(radians: f32, cx: f32, cy: f32) -> AffineTransform {
    if radians == 0.0 || !radians.is_finite() {
        return AffineTransform::default();
    }
    let (sin, cos) = radians.sin_cos();
    AffineTransform {
        sx: cos,
        ky: sin,
        kx: -sin,
        sy: cos,
        tx: cx - (cos * cx - sin * cy),
        ty: cy - (sin * cx + cos * cy),
    }
}

/// Anchor of the first tile for an `ST_RectAlignment` value
/// (ref renderer.ts:2377-2403, `tileAnchorOffset`).
pub fn tile_anchor(algn: &str, box_w: f32, box_h: f32, tile_w: f32, tile_h: f32) -> (f32, f32) {
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

/// A 2×2 mirrored super-cell: the top-left is un-mirrored, the X/Y neighbours
/// are mirrored about the shared seam (ref renderer.ts:2511-2520 `drawCell`).
fn mirror_super_cell(cell: &tiny_skia::Pixmap, flip_x: bool, flip_y: bool) -> tiny_skia::Pixmap {
    let w = cell.width();
    let h = cell.height();
    let cols = if flip_x { 2 } else { 1 };
    let rows = if flip_y { 2 } else { 1 };
    let Some(mut out) = tiny_skia::Pixmap::new(w * cols, h * rows) else {
        return cell.clone();
    };
    for row in 0..rows {
        for col in 0..cols {
            for y in 0..h {
                for x in 0..w {
                    let sx = if col == 1 { w - 1 - x } else { x };
                    let sy = if row == 1 { h - 1 - y } else { y };
                    let src = ((sy * w + sx) * 4) as usize;
                    let dst = (((row * h + y) * w * cols + col * w + x) * 4) as usize;
                    let data = cell.data();
                    out.data_mut()[dst..dst + 4].copy_from_slice(&data[src..src + 4]);
                }
            }
        }
    }
    out
}

/// Crop the decoded bitmap to the §20.1.8.55 window and run the `CT_Blip`
/// effect chain, returning a premultiplied pixmap ready for a pattern shader.
fn cropped_pixmap(
    decoded: &DecodedImage,
    src_rect: &NormalizedRect,
    effects: &[ooxml_common::blip::BlipEffect],
) -> Option<tiny_skia::Pixmap> {
    if decoded.width == 0 || decoded.height == 0 {
        return None;
    }
    let crop = visible_window(src_rect);
    let (crop_x, crop_y, crop_w, crop_h) = {
        let x = (crop.x * decoded.width as f32).round().max(0.0) as u32;
        let y = (crop.y * decoded.height as f32).round().max(0.0) as u32;
        let x = x.min(decoded.width - 1);
        let y = y.min(decoded.height - 1);
        let w = (crop.w * decoded.width as f32)
            .round()
            .clamp(1.0, (decoded.width - x) as f32) as u32;
        let h = (crop.h * decoded.height as f32)
            .round()
            .clamp(1.0, (decoded.height - y) as f32) as u32;
        (x, y, w, h)
    };
    let mut pixmap = tiny_skia::Pixmap::new(crop_w, crop_h)?;
    let orig_w = decoded.width as usize;
    let mut rgba = vec![0u8; (crop_w * crop_h * 4) as usize];
    for row in 0..crop_h as usize {
        let src = ((crop_y as usize + row) * orig_w + crop_x as usize) * 4;
        let dst = row * crop_w as usize * 4;
        let len = crop_w as usize * 4;
        if src + len <= decoded.pixels.len() {
            rgba[dst..dst + len].copy_from_slice(&decoded.pixels[src..src + len]);
        }
    }
    if !effects.is_empty() {
        super::effects::apply_blip_effects(&mut rgba, effects);
    }
    premultiply_into(pixmap.data_mut(), &rgba);
    Some(pixmap)
}

/// The visible part of a §20.1.8.55 source window, clamped to `[0, 1]`.
/// Negative outsets stay outside the bitmap, so they contribute no pixels.
/// ref core/src/image/crop.ts:88-97.
pub fn visible_window(src: &NormalizedRect) -> NormalizedRect {
    if ![src.x, src.y, src.w, src.h].iter().all(|v| v.is_finite()) {
        return NormalizedRect::default();
    }
    let x0 = src.x.max(0.0);
    let y0 = src.y.max(0.0);
    let x1 = (src.x + src.w).min(1.0);
    let y1 = (src.y + src.h).min(1.0);
    NormalizedRect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
}

/// Straight RGBA8 → premultiplied RGBA8, as `tiny-skia` pixmaps store.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub(crate) fn premultiply_into(dst: &mut [u8], src: &[u8]) {
    for (out, inp) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let a = inp[3] as u32;
        match a {
            0 => out.copy_from_slice(&[0, 0, 0, 0]),
            255 => out.copy_from_slice(inp),
            _ => {
                out[0] = ((inp[0] as u32 * a + 127) / 255) as u8;
                out[1] = ((inp[1] as u32 * a + 127) / 255) as u8;
                out[2] = ((inp[2] as u32 * a + 127) / 255) as u8;
                out[3] = a as u8;
            }
        }
    }
}

