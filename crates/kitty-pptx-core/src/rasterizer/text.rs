use crate::contracts::{AffineTransform, PositionedGlyphRun};
use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
};
use fontdb::{
    Database, Family as DbFamily, Query as DbQuery, Stretch as DbStretch, Style as DbStyle,
    Weight as DbWeight,
};
use rustybuzz::ttf_parser::{Face, OutlineBuilder};
use std::cell::RefCell;

thread_local! {
    static FONT_CONTEXT: RefCell<(FontSystem, SwashCache)> = RefCell::new({
        let mut fs = FontSystem::new();
        crate::text::fonts::install_into_font_system(&mut fs);
        let sc = SwashCache::new();
        (fs, sc)
    });
    static WARP_FONT_DB: RefCell<Database> = RefCell::new({
        let mut db = Database::new();
        db.load_system_fonts();
        crate::text::fonts::install_into_db(&mut db);
        db
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

            if run.warp.is_some()
                && run.glyphs.iter().any(|glyph| glyph.warp_u.is_some())
                && run.text.chars().count() == run.glyphs.len()
            {
                render_warped_run(pixmap, run, transform, scale, offset_x, offset_y);
                continue;
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

fn render_warped_run(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let Some(run_warp) = run.warp.as_ref() else {
        return;
    };
    if std::env::var("KITTY_PPTX_DEBUG_WARP").is_ok() {
        eprintln!(
            "raster warp run {:?} glyphs={} hscale={:.3} linew={:.0} boxh={:.0} band={:.3}",
            run.text,
            run.glyphs.len(),
            run_warp.h_scale,
            run_warp.line_w,
            run_warp.warp_box_h,
            run_warp.band_frac
        );
    }
    WARP_FONT_DB.with(|cell| {
        let db = cell.borrow();
        let Some(face_id) = query_face(&db, run) else {
            return;
        };
        let _ = db.with_face_data(face_id, |data, index| {
            let Ok(face) = Face::parse(data, index) else {
                return;
            };
            let units_per_em = face.units_per_em() as f32;
            if units_per_em <= 0.0 {
                return;
            }
            let emu_per_unit = run.font_size_pt * 12700.0 / units_per_em;
            for (glyph, ch) in run.glyphs.iter().zip(run.text.chars()) {
                let Some(u_center) = glyph.warp_u else {
                    continue;
                };
                let Some(glyph_id) = face.glyph_index(ch) else {
                    continue;
                };
                let mut builder = WarpOutlineBuilder {
                    path: tiny_skia::PathBuilder::new(),
                    run_warp,
                    u_center,
                    advance: glyph.advance.max(0.0),
                    emu_per_unit,
                    transform: *transform,
                    scale,
                    offset_x,
                    offset_y,
                };
                if face.outline_glyph(glyph_id, &mut builder).is_none() {
                    continue;
                }
                let Some(path) = builder.path.finish() else {
                    continue;
                };
                let [red, green, blue, alpha] = run.color;
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(red, green, blue, alpha);
                paint.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        });
    });
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

struct WarpOutlineBuilder<'a> {
    path: tiny_skia::PathBuilder,
    run_warp: &'a crate::contracts::RunWarp,
    u_center: f32,
    advance: f32,
    emu_per_unit: f32,
    transform: AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl WarpOutlineBuilder<'_> {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let local_x = x * self.emu_per_unit;
        let local_y = -y * self.emu_per_unit;
        let centered_x = local_x - self.advance * 0.5;
        let line_w = self.run_warp.line_w.max(1.0);
        let u = (self.u_center + centered_x / line_w).clamp(0.0, 1.0);
        let Some(warp) = crate::compiler::text_warp::warp_glyph_transform(
            &self.run_warp.envelope,
            u,
            self.run_warp.warp_box_h,
            self.run_warp.band_frac,
        ) else {
            return (self.offset_x, self.offset_y);
        };
        let scaled_x = centered_x * self.run_warp.h_scale;
        let scaled_y = local_y * warp.scale_y;
        let sheared_x = scaled_x + warp.shear * scaled_y;
        let cos = warp.angle.cos();
        let sin = warp.angle.sin();
        let warped_x = warp.x + sheared_x * cos - scaled_y * sin;
        let warped_y = warp.y + sheared_x * sin + scaled_y * cos;
        let (slide_x, slide_y) = self.transform.transform_point(warped_x, warped_y);
        (
            self.offset_x + slide_x * self.scale,
            self.offset_y + slide_y * self.scale,
        )
    }
}

impl OutlineBuilder for WarpOutlineBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        self.path.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
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
