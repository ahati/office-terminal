//! Compile-time-shaped glyph painting for the straight (untransformed) path
//! (issue #33 subtask 4).
//!
//! The IR already carries the shaped glyph ids and their positions from the
//! compile-time cosmic-text pass; the raster used to re-shape every run into a
//! fresh `Buffer` on every raster — a full shape per run that re-pays on every
//! zoom, resize, sorter card and cache-eviction re-raster. Runs whose ids all
//! came from the family's own cosmic-selected face ([`PositionedGlyphRun::
//! single_face`]) paint straight from the IR through cosmic's own swash cache
//! instead: the same face (resolved again through the same match table), the
//! same coverage rasters, and the buffer path's exact placement arithmetic.
//! Every other run keeps the re-shaping path unchanged.

use cosmic_text::{CacheKey, CacheKeyFlags, Color, FontSystem, SwashCache};
use rustybuzz::ttf_parser::Face;

use crate::contracts::{AffineTransform, PositionedGlyphRun};
use crate::text::fonts::{cosmic_family_face_id, with_face_db};

/// Paint one `single_face` run from the IR.
///
/// Returns the `(draw_x, origin_y)` the caller needs for the decoration bands,
/// or `None` when the face cannot be re-resolved (the caller then falls back
/// to the buffer re-shaping path).
#[allow(clippy::too_many_arguments)]
pub(super) fn render_shaped_run(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
) -> Option<(i32, f32)> {
    let face_id = with_face_db(|db| {
        cosmic_family_face_id(db, &run.font_family, run.bold, run.italic)
    })?;

    // The same hhea pair cosmic's shaper reads per glyph (shape.rs:114): the
    // rustybuzz face over the same database bytes, never a re-shape.
    let metrics = with_face_db(|db| {
        db.with_face_data(face_id, face_metrics)?
    })?;

    let pix_w = pixmap.width() as i32;
    let pix_h = pixmap.height() as i32;
    let sz_px = (run.font_size_pt * 12700.0 * scale).max(6.0);
    let [r, g, b, a] = run.color;

    // Placement arithmetic of the buffer path: anchor on the first glyph's
    // position, round the anchor, and land the line's baseline on it.
    let (first_gx, first_gy) = run.glyphs.first().map(|g| (g.x, g.y))?;
    let (tx, ty) = transform.transform_point(first_gx, first_gy);
    let origin_x = offset_x + tx * scale;
    let origin_y = offset_y + ty * scale;

    // cosmic's single-line layout: `line_y = centering + max_ascent` with the
    // maxima over the line's faces (ref layout.rs:1481-1482, buffer.rs:126) —
    // here the one face the run was shaped through.
    let max_ascent = sz_px * metrics.0;
    let max_descent = sz_px * metrics.1;
    let line_height = sz_px * 1.25;
    let line_y = (line_height - (max_ascent + max_descent)) / 2.0 + max_ascent;
    let line_y_int = line_y as i32;

    let draw_x = origin_x.round() as i32;
    let draw_y = (origin_y - line_y).round() as i32;

    let text_color = Color::rgba(r, g, b, a);
    let pix_data = pixmap.data_mut();

    for glyph in &run.glyphs {
        // The buffer path's in-run x, recovered from the IR: the compile-time
        // shaped position relative to the run's first glyph.
        let rel_x = (glyph.x - first_gx) * scale;
        // `glyph.y` is the run baseline in the IR (combining-mark y offsets
        // are not carried); the base glyph's in-buffer y is zero.
        let (cache_key, gx, gy) = CacheKey::new(
            face_id,
            glyph.glyph_id,
            sz_px,
            (rel_x, 0.0),
            CacheKeyFlags::empty(),
        );
        swash_cache.with_pixels(font_system, cache_key, text_color, |x, y, color| {
            let (cr, cg, cb, ca) = (
                color.r() as u32,
                color.g() as u32,
                color.b() as u32,
                color.a() as u32,
            );
            if ca == 0 {
                return;
            }
            let target_x = draw_x + gx + x;
            let target_y = draw_y + line_y_int + gy + y;
            if target_x < 0 || target_x >= pix_w || target_y < 0 || target_y >= pix_h {
                return;
            }
            let idx = ((target_y * pix_w + target_x) * 4) as usize;
            if idx + 3 >= pix_data.len() {
                return;
            }

            // Premultiply source
            let src_a = (ca * a as u32) / 255;
            let src_r = (cr * src_a) / 255;
            let src_g = (cg * src_a) / 255;
            let src_b = (cb * src_a) / 255;

            let dst_r = pix_data[idx] as u32;
            let dst_g = pix_data[idx + 1] as u32;
            let dst_b = pix_data[idx + 2] as u32;
            let dst_a = pix_data[idx + 3] as u32;

            // Porter-Duff Over: out = src + dst * (1 - src_a)
            let inv_a = 255 - src_a;
            let out_r = (src_r + (dst_r * inv_a + 127) / 255).min(255);
            let out_g = (src_g + (dst_g * inv_a + 127) / 255).min(255);
            let out_b = (src_b + (dst_b * inv_a + 127) / 255).min(255);
            let out_a = (src_a + (dst_a * inv_a + 127) / 255).min(255);

            pix_data[idx] = out_r as u8;
            pix_data[idx + 1] = out_g as u8;
            pix_data[idx + 2] = out_b as u8;
            pix_data[idx + 3] = out_a as u8;
        });
    }

    Some((draw_x, origin_y))
}

/// The face's `(ascender, descender)` as fractions of the em, the pair cosmic
/// attributes to every glyph of a shaped run (shape.rs:114-115).
fn face_metrics(data: &[u8], index: u32) -> Option<(f32, f32)> {
    let face = Face::parse(data, index).ok()?;
    let upem = face.units_per_em() as f32;
    if upem <= 0.0 {
        return None;
    }
    Some((
        face.ascender() as f32 / upem,
        -(face.descender() as f32) / upem,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping};

    /// The replicated `line_y` must equal cosmic's own layout for the run's
    /// face and size, or the straight painter would land the baseline
    /// elsewhere than the buffer path it replaces.
    #[test]
    fn line_y_matches_cosmic_layout() {
        crate::text::fonts::with_font_system(|font_system| {
            for (family, size) in [
                ("Carlito", 18.0f32),
                ("Liberation Sans", 24.0),
                ("Caladea", 12.5),
            ] {
                let face_id = crate::text::fonts::with_face_db(|db| {
                    cosmic_family_face_id(db, family, false, false)
                })
                .unwrap_or_else(|| panic!("{family} must resolve"));
                let metrics = with_face_db(|db| {
                    db.with_face_data(face_id, face_metrics)?
                })
                .unwrap_or_else(|| panic!("{family} metrics"));
                let max_ascent = size * metrics.0;
                let max_descent = size * metrics.1;
                let replicated = (size * 1.25 - (max_ascent + max_descent)) / 2.0 + max_ascent;

                let mut buffer =
                    Buffer::new(font_system, Metrics::new(size, size * 1.25));
                buffer.set_size(font_system, None, None);
                buffer.set_text(
                    font_system,
                    "Typ g",
                    Attrs::new().family(Family::Name(family)),
                    Shaping::Advanced,
                );
                buffer.shape_until_scroll(font_system, false);
                let cosmic_line_y = buffer
                    .layout_runs()
                    .next()
                    .map(|layout| layout.line_y)
                    .unwrap_or(0.0);
                assert!(
                    (replicated - cosmic_line_y).abs() < 1e-3,
                    "{family} @ {size}: replicated {replicated} vs cosmic {cosmic_line_y}"
                );
            }
        });
    }
}
