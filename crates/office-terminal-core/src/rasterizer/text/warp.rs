use super::{query_face, WARP_FONT_DB};
use crate::contracts::{AffineTransform, PositionedGlyphRun};
use rustybuzz::ttf_parser::{Face, OutlineBuilder};

pub(super) fn render_warped_run(
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
            // `run.glyphs` is the shaped glyph stream while `run.text` is the
            // authored character stream; cosmic_text drops the odd whitespace
            // glyph (it carries no ink), so a plain zip can silently shift every
            // following glyph onto the wrong character. Realign by skipping a
            // whitespace character while the glyphs run short.
            let chars: Vec<char> = run.text.chars().collect();
            let mut glyph_index = 0usize;
            for (char_index, ch) in chars.iter().enumerate() {
                let Some(glyph) = run.glyphs.get(glyph_index) else {
                    break;
                };
                if ch.is_whitespace() && chars.len() - char_index > run.glyphs.len() - glyph_index {
                    continue;
                }
                glyph_index += 1;
                let Some(u_center) = glyph.warp_u else {
                    continue;
                };
                let Some(glyph_id) = face.glyph_index(*ch) else {
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
    /// Map one outline point (font units, y-up) into device space.
    ///
    /// Paired-edge presets use the exact continuous envelope map
    /// `P(u, v) = (1 − v)·T(u) + v·B(u)` (ref `renderer.ts:3320`
    /// `drawWarpedGlyphStrips`, whose per-strip affines converge to it). The
    /// horizontal fraction `u` comes from the point's FLAT pen offset, so the
    /// envelope itself already widens the glyph by `boxW / lineW`; adding the
    /// advance again (`h_scale`), as the earlier revision did, doubled every
    /// glyph's width and sheared it into the shards this path used to emit.
    ///
    /// `v` is the point's height fraction across the local band: the baseline
    /// sits at `band_frac`, and `warp_box_h` spans the line's share of the gap,
    /// so the flat ink box maps 1:1 onto its band (ref `renderer.ts:3298`
    /// `warpBoxH = inkH / (v1 - v0)`).
    ///
    /// Single-edge (arch/circle/Follow Path) presets keep the rigid per-glyph
    /// placement on the baseline curve: natural size, rotated to the curve
    /// normal, no shear (ref `renderer.ts:3400`, `text-warp.ts:319`).
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
        let paired = crate::compiler::text_warp::warp_point(
            &self.run_warp.envelope,
            u,
            self.run_warp.band_frac + local_y / self.run_warp.warp_box_h.max(1.0),
        );
        let (warped_x, warped_y) = match paired {
            Some(point) => point,
            None => {
                let scaled_x = centered_x * self.run_warp.h_scale;
                let scaled_y = local_y * warp.scale_y;
                let sheared_x = scaled_x + warp.shear * scaled_y;
                let cos = warp.angle.cos();
                let sin = warp.angle.sin();
                (
                    warp.x + sheared_x * cos - scaled_y * sin,
                    warp.y + sheared_x * sin + scaled_y * cos,
                )
            }
        };
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
