use std::cell::RefCell;
use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
};
use crate::contracts::{AffineTransform, PositionedGlyphRun};

thread_local! {
    static FONT_CONTEXT: RefCell<(FontSystem, SwashCache)> = RefCell::new({
        let fs = FontSystem::new();
        let sc = SwashCache::new();
        (fs, sc)
    });
}

pub fn render_text_runs(
    pixmap: &mut tiny_skia::Pixmap,
    runs: &[PositionedGlyphRun],
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    if runs.is_empty() {
        return;
    }

    let pix_w = pixmap.width() as i32;
    let pix_h = pixmap.height() as i32;

    FONT_CONTEXT.with(|ctx| {
        let (ref mut font_system, ref mut swash_cache) = *ctx.borrow_mut();

        for run in runs {
            if run.text.is_empty() {
                continue;
            }

            let sz_px = (run.font_size_pt * 12700.0 * scale).max(6.0);
            let [r, g, b, a] = run.color;

            // Get start offset in shape coordinates from first glyph
            let (first_gx, first_gy) = run
                .glyphs
                .first()
                .map(|g| (g.x, g.y))
                .unwrap_or((0.0, 0.0));

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

            let draw_x = origin_x.round() as i32;
            let draw_y = (origin_y - sz_px * 0.85).round() as i32;

            let text_color = Color::rgba(r, g, b, a);
            let pix_data = pixmap.data_mut();

            buffer.draw(
                font_system,
                swash_cache,
                text_color,
                |x, y, w, h, col| {
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
                },
            );

            // Underline
            if run.underline {
                let u_thickness = (sz_px * 0.06).max(1.0).round() as i32;
                let u_y = origin_y.round() as i32 + 2;
                let run_w = estimate_run_px_width(run, scale);
                draw_rect_fill(
                    pix_data,
                    pix_w,
                    pix_h,
                    draw_x,
                    u_y,
                    run_w,
                    u_thickness,
                    r,
                    g,
                    b,
                    a,
                );
            }

            // Strikethrough
            if run.strikethrough {
                let s_thickness = (sz_px * 0.05).max(1.0).round() as i32;
                let s_y = (origin_y - sz_px * 0.35).round() as i32;
                let run_w = estimate_run_px_width(run, scale);
                draw_rect_fill(
                    pix_data,
                    pix_w,
                    pix_h,
                    draw_x,
                    s_y,
                    run_w,
                    s_thickness,
                    r,
                    g,
                    b,
                    a,
                );
            }
        }
    });
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
