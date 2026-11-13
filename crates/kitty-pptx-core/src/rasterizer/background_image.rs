//! `<a:blipFill>` slide background painting (ECMA-376 §20.1.8.14).
//!
//! Ports the reference background paint exactly (ref renderer.ts:2218-2362):
//! an opaque white base is laid down first, the blip is clipped to the slide
//! rectangle, and the image is placed either stretched into the `fillRect`
//! destination (§20.1.8.56) or repeated at its native size for `a:tile`
//! (§20.1.8.58). `alphaModFix` modulates the whole placement.

use crate::contracts::{
    AffineTransform, BackgroundTile, NormalizedRect, RectF32, VectorBackground,
};
use crate::loader::{AssetStore, SizeHint};

/// EMU per pixel at the 96-DPI bitmap basis used for native tile sizes
/// (ref renderer.ts:2453-2455, `pixelToEmu = EMU_PER_PX * 96 / dpi`, dpi = 96).
const EMU_PER_PX: f32 = 9525.0;

/// Upper bound on the number of tile draws for one background. A tile smaller
/// than a device pixel would otherwise loop effectively forever.
const MAX_TILE_DRAWS: i64 = 8192;

/// Upper bound on source pixels re-sampled by one tiled background. Each tile
/// re-decodes the cropped slice (ref renderer.ts:2513-2520 draws the cropped
/// bitmap per cell), so a pathological `sx`/`sy` must not spin for minutes;
/// beyond the budget the white base is left in place.
const MAX_TILE_WORK: f64 = 64_000_000.0;

/// Paint `VectorBackground::Image` into `pixmap`.
///
/// `scale` / `offset_x` / `offset_y` map slide EMU to canvas pixels;
/// `box_w` / `box_h` are the slide extent in EMU.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    pixmap: &mut tiny_skia::Pixmap,
    assets: &dyn AssetStore,
    bg: &VectorBackground,
    box_w: f32,
    box_h: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let VectorBackground::Image {
        rel_id,
        src_rect,
        fill_rect,
        tile,
        opacity,
    } = bg
    else {
        return;
    };

    // §20.1.8.14 — opaque white base first, so a partially transparent blip
    // composites over white and a decode failure still leaves a defined
    // background (ref renderer.ts:2229-2236).
    pixmap.fill(tiny_skia::Color::WHITE);

    // Clip to the slide rectangle (ref renderer.ts:2309-2314). The layer bounds
    // are exactly that rectangle, so overscan (negative fillRect insets) and
    // tile bleed are cropped at the slide edge.
    let Some((x0, y0, x1, y1)) = slide_device_box(
        pixmap.width(),
        pixmap.height(),
        box_w,
        box_h,
        scale,
        offset_x,
        offset_y,
    ) else {
        return;
    };
    let Some(mut layer) = tiny_skia::Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else {
        return;
    };

    let local_x = offset_x - x0 as f32;
    let local_y = offset_y - y0 as f32;
    match tile {
        Some(tile) => paint_tiles(
            &mut layer, assets, rel_id, src_rect, tile, box_w, box_h, scale, local_x, local_y,
            *opacity,
        ),
        None => {
            let dest = destination_rect(fill_rect.as_ref(), box_w, box_h);
            super::image::render_image(
                &mut layer,
                assets,
                rel_id,
                src_rect,
                &dest,
                *opacity,
                &AffineTransform::default(),
                &[],
                scale,
                local_x,
                local_y,
            );
        }
    }

    pixmap.draw_pixmap(
        x0,
        y0,
        layer.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

/// The slide rectangle in device pixels, clamped to the canvas.
#[allow(clippy::too_many_arguments)]
fn slide_device_box(
    canvas_w: u32,
    canvas_h: u32,
    box_w: f32,
    box_h: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<(i32, i32, i32, i32)> {
    let left = offset_x;
    let top = offset_y;
    let right = offset_x + box_w * scale;
    let bottom = offset_y + box_h * scale;
    if ![left, top, right, bottom].iter().all(|v| v.is_finite()) {
        return None;
    }
    let x0 = left.floor().max(0.0).min(canvas_w as f32) as i32;
    let y0 = top.floor().max(0.0).min(canvas_h as f32) as i32;
    let x1 = right.ceil().max(0.0).min(canvas_w as f32) as i32;
    let y1 = bottom.ceil().max(0.0).min(canvas_h as f32) as i32;
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

/// `fillRect` destination in slide EMU. `None` (or non-finite values) means
/// the full fill box (ref renderer.ts:2298-2306).
fn destination_rect(fill_rect: Option<&RectF32>, box_w: f32, box_h: f32) -> RectF32 {
    let Some(fr) = fill_rect else {
        return RectF32::new(0.0, 0.0, box_w, box_h);
    };
    let dest = RectF32::new(fr.x * box_w, fr.y * box_h, fr.w * box_w, fr.h * box_h);
    if [dest.x, dest.y, dest.w, dest.h]
        .iter()
        .all(|v| v.is_finite())
    {
        dest
    } else {
        RectF32::new(0.0, 0.0, box_w, box_h)
    }
}

/// Repeat the blip at its native size (ref renderer.ts:2434-2585).
#[allow(clippy::too_many_arguments)]
fn paint_tiles(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn AssetStore,
    rel_id: &str,
    src_rect: &NormalizedRect,
    tile: &BackgroundTile,
    box_w: f32,
    box_h: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    opacity: f32,
) {
    let Ok(decoded) = assets.get(rel_id, SizeHint::Native) else {
        return;
    };
    if decoded.width == 0 || decoded.height == 0 {
        return;
    }
    // §20.1.8.55 applies before the fill mode: the cropped logical rectangle is
    // the content duplicated (Annex L.4.8.4.3, ref renderer.ts:2447-2457).
    let logical_w = src_rect.w;
    let logical_h = src_rect.h;
    if !(logical_w > 0.0 && logical_h > 0.0) {
        return;
    }
    let tile_w = decoded.width as f32 * EMU_PER_PX * logical_w * tile.sx;
    let tile_h = decoded.height as f32 * EMU_PER_PX * logical_h * tile.sy;
    if ![tile_w, tile_h].iter().all(|v| v.is_finite() && *v > 0.0) {
        return;
    }

    let flip_x = tile.flip == "x" || tile.flip == "xy";
    let flip_y = tile.flip == "y" || tile.flip == "xy";
    // A flipped fill repeats a 2x2 mirrored super-cell so the seam between
    // repeats is a mirror line (ref renderer.ts:2464-2471).
    let cell_w = tile_w * if flip_x { 2.0 } else { 1.0 };
    let cell_h = tile_h * if flip_y { 2.0 } else { 1.0 };
    if ![cell_w, cell_h].iter().all(|v| v.is_finite() && *v > 0.0) {
        return;
    }
    let cell_draws: i64 = (1 + flip_x as i64) * (1 + flip_y as i64);

    // Grid registration: align the pattern to the box anchor, then add tx/ty
    // (ref renderer.ts:2543-2559).
    let (ax, ay) = tile_anchor(&tile.algn, box_w, box_h, tile_w, tile_h);
    let start_x = ax + tile.tx as f32;
    let start_y = ay + tile.ty as f32;
    if ![start_x, start_y].iter().all(|v| v.is_finite()) {
        return;
    }

    let (cols, col0) = grid_range(0.0, box_w, start_x, cell_w);
    let (rows, row0) = grid_range(0.0, box_h, start_y, cell_h);
    let Some(cols) = cols else { return };
    let Some(rows) = rows else { return };
    let draws = cols.saturating_mul(rows).saturating_mul(cell_draws);
    if draws > MAX_TILE_DRAWS {
        return;
    }
    let crop_area = f64::from(decoded.width)
        * f64::from(logical_w)
        * f64::from(decoded.height)
        * f64::from(logical_h);
    if draws as f64 * crop_area > MAX_TILE_WORK {
        return;
    }

    for row in 0..rows {
        let cell_y = start_y + (row0 + row) as f32 * cell_h;
        for col in 0..cols {
            let cell_x = start_x + (col0 + col) as f32 * cell_w;
            draw_cell(
                layer, assets, rel_id, src_rect, opacity, scale, offset_x, offset_y, tile_w,
                tile_h, cell_x, cell_y, false, false,
            );
            if flip_x {
                draw_cell(
                    layer, assets, rel_id, src_rect, opacity, scale, offset_x, offset_y, tile_w,
                    tile_h, cell_x, cell_y, true, false,
                );
            }
            if flip_y {
                draw_cell(
                    layer, assets, rel_id, src_rect, opacity, scale, offset_x, offset_y, tile_w,
                    tile_h, cell_x, cell_y, false, true,
                );
            }
            if flip_x && flip_y {
                draw_cell(
                    layer, assets, rel_id, src_rect, opacity, scale, offset_x, offset_y, tile_w,
                    tile_h, cell_x, cell_y, true, true,
                );
            }
        }
    }
}

/// Number of grid cells to paint to cover `[lo, hi]` for a grid starting at
/// `start` with `step`, plus the index of the first cell.
fn grid_range(lo: f32, hi: f32, start: f32, step: f32) -> (Option<i64>, i64) {
    let first = ((lo - start) / step).floor();
    let last = ((hi - start) / step).ceil() - 1.0;
    if !first.is_finite() || !last.is_finite() || last < first {
        return (None, 0);
    }
    // Clamp to a quarter of the i64 range so `count` cannot overflow for an
    // absurd phase/step pair; the caller's draw budget rejects those anyway.
    let first = first.max(i64::MIN as f32 / 4.0) as i64;
    let last = last.min(i64::MAX as f32 / 4.0) as i64;
    let count = last.saturating_sub(first).saturating_add(1);
    if count <= 0 {
        return (None, 0);
    }
    (Some(count), first)
}

/// Anchor of the first tile for an `ST_RectAlignment` value
/// (ref renderer.ts:2377-2403).
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

/// Draw one tile of a super-cell. `mx` / `my` mirror it about its own box
/// (ref renderer.ts:2513-2520, `drawCell`).
#[allow(clippy::too_many_arguments)]
fn draw_cell(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn AssetStore,
    rel_id: &str,
    src_rect: &NormalizedRect,
    opacity: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    tile_w: f32,
    tile_h: f32,
    cell_x: f32,
    cell_y: f32,
    mx: bool,
    my: bool,
) {
    let x = cell_x + if mx { tile_w } else { 0.0 };
    let y = cell_y + if my { tile_h } else { 0.0 };
    let transform = AffineTransform {
        sx: if mx { -1.0 } else { 1.0 },
        sy: if my { -1.0 } else { 1.0 },
        ky: 0.0,
        kx: 0.0,
        tx: if mx { 2.0 * x + tile_w } else { 0.0 },
        ty: if my { 2.0 * y + tile_h } else { 0.0 },
    };
    super::image::render_image(
        layer,
        assets,
        rel_id,
        src_rect,
        &RectF32::new(x, y, tile_w, tile_h),
        opacity,
        &transform,
        &[],
        scale,
        offset_x,
        offset_y,
    );
}
