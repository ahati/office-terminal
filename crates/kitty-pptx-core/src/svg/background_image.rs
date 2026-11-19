//! SVG emission for `<a:blipFill>` slide backgrounds (§20.1.8.14).
//!
//! Mirrors the rasterizer's background paint (ref renderer.ts:2218-2362): an
//! opaque white base, then either a stretched `<image>` honouring `srcRect` and
//! `fillRect`, or a `<pattern>` repeating the blip at its native size for
//! `a:tile` (§20.1.8.58). Overscan beyond the slide edge is clipped by the
//! root viewport.

use std::fmt::Write as _;

use super::image::serialize_svg_image;
use crate::contracts::{
    AffineTransform, BackgroundTile, NormalizedRect, RectF32, EMU_PER_PX, VectorBackground,
};
use crate::loader::{AssetStore, SizeHint};

/// Identifier of the background tile pattern in the emitted `<defs>`.
const PATTERN_ID: &str = "bg_tile";

/// Serialize the background body (white base + image/pattern rect), pushing any
/// `<pattern>` definition into `defs`. Returns `None` for non-image fills.
pub(super) fn emit(
    assets: &dyn AssetStore,
    bg: &VectorBackground,
    box_w: f32,
    box_h: f32,
    defs: &mut String,
) -> Option<String> {
    let VectorBackground::Image {
        rel_id,
        src_rect,
        fill_rect,
        tile,
        opacity,
    } = bg
    else {
        return None;
    };

    let mut body = format!(
        r##"  <rect width="{}" height="{}" fill="#ffffff"/>"##,
        box_w, box_h
    );
    body.push('\n');

    let element = match tile {
        Some(tile) => {
            let tile_svg =
                tiled_element(assets, rel_id, src_rect, tile, box_w, box_h, *opacity, defs);
            tile_svg.unwrap_or_default()
        }
        None => {
            let dest = destination_rect(fill_rect.as_ref(), box_w, box_h);
            serialize_svg_image(
                assets,
                rel_id,
                src_rect,
                &dest,
                *opacity,
                &AffineTransform::default(),
                &[],
            )
        }
    };
    if !element.is_empty() {
        body.push_str(&element);
        body.push('\n');
    }
    Some(body)
}

/// `<a:fillRect>` destination in slide EMU (`None` = full box).
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

/// Pattern-filled rect for a tiled blip (ref renderer.ts:2434-2585).
#[allow(clippy::too_many_arguments)]
fn tiled_element(
    assets: &dyn AssetStore,
    rel_id: &str,
    src_rect: &NormalizedRect,
    tile: &BackgroundTile,
    box_w: f32,
    box_h: f32,
    opacity: f32,
    defs: &mut String,
) -> Option<String> {
    let Ok(decoded) = assets.get(rel_id, SizeHint::Native) else {
        return None;
    };
    if decoded.width == 0 || decoded.height == 0 {
        return None;
    }
    // §20.1.8.55 applies before the fill mode: the cropped logical rectangle is
    // the content duplicated (`tileSourceExtent`, ref renderer.ts:2408-2417).
    if !(src_rect.w > 0.0 && src_rect.h > 0.0) {
        return None;
    }
    let tile_w = decoded.width as f32 * EMU_PER_PX * src_rect.w * tile.sx;
    let tile_h = decoded.height as f32 * EMU_PER_PX * src_rect.h * tile.sy;
    if ![tile_w, tile_h].iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    let flip_x = tile.flip == "x" || tile.flip == "xy";
    let flip_y = tile.flip == "y" || tile.flip == "xy";
    let cell_w = tile_w * if flip_x { 2.0 } else { 1.0 };
    let cell_h = tile_h * if flip_y { 2.0 } else { 1.0 };

    // The tile cell is the cropped source stretched over the native tile box;
    // `preserveAspectRatio="none"` keeps that stretch (no letterboxing).
    let cell = serialize_svg_image(
        assets,
        rel_id,
        src_rect,
        &RectF32::new(0.0, 0.0, tile_w, tile_h),
        1.0,
        &AffineTransform::default(),
        &[],
    );
    if cell.is_empty() {
        return None;
    }

    let (ax, ay) = tile_anchor(&tile.algn, box_w, box_h, tile_w, tile_h);
    let phase_x = ax + tile.tx as f32;
    let phase_y = ay + tile.ty as f32;
    if ![phase_x, phase_y, cell_w, cell_h]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }

    let _ = writeln!(
        defs,
        r#"    <pattern id="{PATTERN_ID}" patternUnits="userSpaceOnUse" x="{}" y="{}" width="{}" height="{}">"#,
        phase_x, phase_y, cell_w, cell_h
    );
    let cell_id = format!("{PATTERN_ID}_cell");
    let _ = writeln!(defs, r#"      <g id="{cell_id}">"#);
    for line in cell.lines() {
        defs.push_str("        ");
        defs.push_str(line.trim_start());
        defs.push('\n');
    }
    let _ = writeln!(defs, "      </g>");
    let _ = writeln!(defs, r##"      <use href="#{cell_id}"/>"##);
    // A flipped fill repeats a 2x2 mirrored super-cell, so the seam between
    // repeats is a mirror line (ref renderer.ts:2464-2471, 2540-2544).
    if flip_x {
        let _ = writeln!(
            defs,
            r##"      <use href="#{cell_id}" transform="translate({} 0) scale(-1 1)"/>"##,
            tile_w * 2.0
        );
    }
    if flip_y {
        let _ = writeln!(
            defs,
            r##"      <use href="#{cell_id}" transform="translate(0 {}) scale(1 -1)"/>"##,
            tile_h * 2.0
        );
    }
    if flip_x && flip_y {
        let _ = writeln!(
            defs,
            r##"      <use href="#{cell_id}" transform="translate({} {}) scale(-1 -1)"/>"##,
            tile_w * 2.0,
            tile_h * 2.0
        );
    }
    let _ = writeln!(defs, "    </pattern>");

    let opacity_attr = if (opacity - 1.0).abs() > 1e-3 {
        format!(r#" opacity="{:.2}""#, opacity)
    } else {
        String::new()
    };
    Some(format!(
        r#"  <rect width="{}" height="{}" fill="url(#{})"{}/>"#,
        box_w, box_h, PATTERN_ID, opacity_attr
    ))
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
