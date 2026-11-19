use crate::contracts::{AffineTransform, PositionedGlyphRun};
use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
};
use fontdb::{
    Database, Family as DbFamily, Query as DbQuery, Stretch as DbStretch, Style as DbStyle,
    Weight as DbWeight,
};
mod affine;
mod decorations;
mod reflection;
mod straight;
mod warp;

use std::cell::RefCell;

// The glyph runs were shaped by the crate's ONE shared per-thread font
// runtime (`crate::text::fonts`), so the painter resolves fonts through the
// same two stores instead of scanning the system again: `with_font_system`
// borrows the shared `cosmic-text` system and the `WARP_FONT_DB` alias keeps
// the outline painters' `super::WARP_FONT_DB` call sites pointing at the one
// shared raw database.
use crate::text::fonts::{with_font_system, FACE_DB as WARP_FONT_DB};
use crate::rasterizer::scratch::EffectScratch;

thread_local! {
    /// Glyph rasterisation cache for the shared per-thread font system.
    /// `SwashCache::new()` performs no font scan, so it stays local; the font
    /// system itself lives in `crate::text::fonts` and is shared with the
    /// layout measurer and the warp compiler.
    static SWASH_CACHE: RefCell<SwashCache> = RefCell::new(SwashCache::new());
}

pub fn render_text_runs(
    pixmap: &mut tiny_skia::Pixmap,
    runs: &[PositionedGlyphRun],
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    scratch: &mut EffectScratch,
) {
    if runs.is_empty() {
        return;
    }

    with_font_system(|font_system| {
        SWASH_CACHE.with(|cell| {
            let swash_cache = &mut *cell.borrow_mut();

            for run in runs {
                if run.text.is_empty() {
                    continue;
                }

                if let Some(ref reflection) = run.reflection {
                    reflection::render_reflected_run(
                        pixmap,
                        run,
                        reflection,
                        transform,
                        scale,
                        offset_x,
                        offset_y,
                        font_system,
                        swash_cache,
                        scratch,
                    );
                }

                render_single_run(
                    pixmap,
                    run,
                    transform,
                    scale,
                    offset_x,
                    offset_y,
                    font_system,
                    swash_cache,
                );
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_single_run(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
) {
    if run.text.is_empty() {
        return;
    }

    if run.warp.is_some()
        && run.glyphs.iter().any(|glyph| glyph.warp_u.is_some())
        && run.glyphs.len() <= run.text.chars().count()
    {
        warp::render_warped_run(pixmap, run, transform, scale, offset_x, offset_y);
        return;
    }

    // Rotated / mirrored text cannot be painted by an axis-aligned
    // cosmic-text buffer: transforming only the first glyph's origin
    // leaves every glyph upright (the old behaviour, most visible on
    // 270-degree footer spines). Outline the shaped glyphs and map
    // every point through the element transform instead, matching the
    // SVG `VectorGlyphs` backend.
    if affine::needs_affine_text(transform)
        && affine::render_affine_run(pixmap, run, transform, scale, offset_x, offset_y)
    {
        return;
    }

    let pix_w = pixmap.width() as i32;
    let pix_h = pixmap.height() as i32;

    // Straight path, IR glyph ids first (issue #33 subtask 4): a run whose
    // ids all came from the family's own compile-time face paints without
    // re-shaping. Any other run (synthetic ids, fallback faces, a face that
    // cannot be re-resolved) falls through to the buffer path below, whose
    // pixels it already matches.
    if run.single_face {
        if let Some((draw_x, origin_y)) = straight::render_shaped_run(
            pixmap,
            run,
            transform,
            scale,
            offset_x,
            offset_y,
            font_system,
            swash_cache,
        ) {
            if !run.decorations.is_empty() {
                decorations::paint_upright(
                    pixmap.data_mut(),
                    pix_w,
                    pix_h,
                    run,
                    draw_x,
                    origin_y,
                    estimate_run_px_width(run, scale),
                    scale,
                );
            }
            return;
        }
    }

    let sz_px = (run.font_size_pt * 12700.0 * scale).max(6.0);
    let [r, g, b, a] = run.color;

    // Get start offset in shape coordinates from first glyph
    let (first_gx, first_gy) = run.glyphs.first().map(|g| (g.x, g.y)).unwrap_or((0.0, 0.0));

    let (tx, ty) = transform.transform_point(first_gx, first_gy);
    let origin_x = offset_x + tx * scale;
    let origin_y = offset_y + ty * scale;

    let line_h = sz_px * 1.25;
    let mut buffer = Buffer::new(font_system, Metrics::new(sz_px, line_h));

    let family = Family::Name(&run.font_family);
    let mut attrs = Attrs::new().family(family);
    if run.bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if run.italic {
        attrs = attrs.style(Style::Italic);
    }
    buffer.set_text(font_system, &run.text, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    // `glyph.y` is the compiled baseline in EMU (reference
    // `renderer.ts:5783`: baseline = cursorY + metric ascent). Place the
    // buffer so that its own baseline lands on that point instead of
    // guessing an ascent fraction (the old `0.85 * size`).
    let line_y = buffer
        .layout_runs()
        .next()
        .map(|layout| layout.line_y)
        .unwrap_or(sz_px * 0.8);
    let draw_x = origin_x.round() as i32;
    let draw_y = (origin_y - line_y).round() as i32;

    let text_color = Color::rgba(r, g, b, a);
    let pix_data = pixmap.data_mut();

    buffer.draw(font_system, swash_cache, text_color, |x, y, w, h, col| {
        let (cr, cg, cb, ca) = (
            col.r() as u32,
            col.g() as u32,
            col.b() as u32,
            col.a() as u32,
        );
        if ca == 0 {
            return;
        }

        for dy in 0..h as i32 {
            let target_y = draw_y + y + dy;
            if target_y < 0 || target_y >= pix_h {
                continue;
            }

            for dx in 0..w as i32 {
                let target_x = draw_x + x + dx;
                if target_x < 0 || target_x >= pix_w {
                    continue;
                }

                let idx = ((target_y * pix_w + target_x) * 4) as usize;
                if idx + 3 >= pix_data.len() {
                    continue;
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
            }
        }
    });

    // Face-derived underline / strike bands from the vector IR (T7).
    if !run.decorations.is_empty() {
        decorations::paint_upright(
            pix_data,
            pix_w,
            pix_h,
            run,
            draw_x,
            origin_y,
            estimate_run_px_width(run, scale),
            scale,
        );
    }
}

fn query_face(db: &Database, run: &PositionedGlyphRun) -> Option<fontdb::ID> {
    let lower = run.font_family.to_ascii_lowercase();
    let serif = lower.contains("serif")
        || lower.contains("cambria")
        || lower.contains("caladea")
        || lower.contains("georgia")
        || lower.contains("times");
    let mono = lower.contains("mono") || lower.contains("consol") || lower.contains("courier");
    let mut families = vec![run.font_family.clone()];
    if serif {
        families.extend([
            "Caladea".to_string(),
            "Liberation Serif".to_string(),
            "DejaVu Serif".to_string(),
            "Times New Roman".to_string(),
        ]);
    } else if mono {
        families.extend([
            "Liberation Mono".to_string(),
            "DejaVu Sans Mono".to_string(),
        ]);
    } else {
        families.extend([
            "Carlito".to_string(),
            "Liberation Sans".to_string(),
            "DejaVu Sans".to_string(),
            "Arial".to_string(),
        ]);
    }

    let requested_weight = if run.bold {
        DbWeight::BOLD
    } else {
        DbWeight::NORMAL
    };
    let requested_style = if run.italic {
        DbStyle::Italic
    } else {
        DbStyle::Normal
    };
    for family in &families {
        for (weight, style) in [
            (requested_weight, requested_style),
            (DbWeight::NORMAL, DbStyle::Normal),
        ] {
            let query = DbQuery {
                families: &[DbFamily::Name(family)],
                weight,
                stretch: DbStretch::Normal,
                style,
            };
            if let Some(id) = db.query(&query) {
                return Some(id);
            }
        }
    }
    None
}

fn estimate_run_px_width(run: &PositionedGlyphRun, scale: f32) -> i32 {
    let last_glyph = run.glyphs.last();
    let first_glyph = run.glyphs.first();
    match (first_glyph, last_glyph) {
        (Some(f), Some(l)) => ((l.x + l.advance - f.x) * scale).max(1.0).round() as i32,
        _ => (run.font_size_pt * 12700.0 * 0.5 * run.text.len() as f32 * scale).round() as i32,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_rect_fill(
    pix_data: &mut [u8],
    pix_w: i32,
    pix_h: i32,
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    if a == 0 || w <= 0 || h <= 0 {
        return;
    }
    let src_r = ((r as u32 * a as u32) / 255) as u8;
    let src_g = ((g as u32 * a as u32) / 255) as u8;
    let src_b = ((b as u32 * a as u32) / 255) as u8;
    let inv_a = 255 - a as u32;

    for y in y0..(y0 + h) {
        if y < 0 || y >= pix_h {
            continue;
        }
        for x in x0..(x0 + w) {
            if x < 0 || x >= pix_w {
                continue;
            }
            let idx = ((y * pix_w + x) * 4) as usize;
            if idx + 3 >= pix_data.len() {
                continue;
            }

            let dst_r = pix_data[idx] as u32;
            let dst_g = pix_data[idx + 1] as u32;
            let dst_b = pix_data[idx + 2] as u32;
            let dst_a = pix_data[idx + 3] as u32;

            pix_data[idx] = (src_r as u32 + (dst_r * inv_a + 127) / 255).min(255) as u8;
            pix_data[idx + 1] = (src_g as u32 + (dst_g * inv_a + 127) / 255).min(255) as u8;
            pix_data[idx + 2] = (src_b as u32 + (dst_b * inv_a + 127) / 255).min(255) as u8;
            pix_data[idx + 3] = (a as u32 + (dst_a * inv_a + 127) / 255).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::transform::compute_shape_transform;
    use crate::contracts::PositionedGlyph;

    fn run(text: &str, x_px: f32, baseline_px: f32) -> PositionedGlyphRun {
        let emu = 12_700.0;
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(index, _)| PositionedGlyph {
                glyph_id: 0,
                x: (x_px + index as f32 * 12.0) * emu,
                y: baseline_px * emu,
                advance: 12.0 * emu,
                warp_u: None,
            })
            .collect();
        PositionedGlyphRun {
            text: text.to_string(),
            font_family: "Arial".to_string(),
            font_size_pt: 20.0,
            color: [0, 0, 0, 255],
            bold: false,
            italic: false,
            glyphs,
            decorations: Default::default(),
            single_face: false,
            warp: None,
            reflection: None,
        }
    }

    fn ink_bounds(pixmap: &tiny_skia::Pixmap) -> (u32, u32, u32, u32) {
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0;
        let mut max_y = 0;
        for y in 0..pixmap.height() {
            for x in 0..pixmap.width() {
                if pixmap.pixel(x, y).is_some_and(|p| p.alpha() > 0) {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        (min_x, min_y, max_x, max_y)
    }

    #[test]
    fn a_rotated_run_paints_along_the_rotated_axis() {
        let mut pixmap = tiny_skia::Pixmap::new(128, 128).expect("pixmap");
        let emu = 12_700.0;
        let transform = compute_shape_transform(
            0,
            0,
            (128.0 * emu) as i64,
            (128.0 * emu) as i64,
            90.0,
            false,
            false,
        );
        render_text_runs(
            &mut pixmap,
            &[run("HELLO", 20.0, 86.0)],
            &transform,
            1.0 / emu,
            0.0,
            0.0,
            &mut crate::rasterizer::scratch::EffectScratch::default(),
        );
        let (min_x, min_y, max_x, max_y) = ink_bounds(&pixmap);
        assert!(max_x > min_x && max_y > min_y, "the rotated run paints ink");
        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;
        assert!(
            height > width * 2,
            "a 90-degree run must be tall and narrow, got {width}x{height}"
        );
    }
}
