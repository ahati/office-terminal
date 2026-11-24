use super::{query_face, WARP_FONT_DB};
use crate::contracts::{AffineTransform, PositionedGlyphRun};
use rustybuzz::ttf_parser::{Face, OutlineBuilder};

pub(super) fn needs_affine_text(transform: &AffineTransform) -> bool {
    transform.kx.abs() > 1.0e-4
        || transform.ky.abs() > 1.0e-4
        || transform.sx < 0.0
        || transform.sy < 0.0
}

/// Paint one run by outlining its shaped glyphs and mapping every outline
/// point through `transform`.
///
/// The `VectorGlyphs` SVG backend already does this; the rasterizer used to
/// transform only the run origin and then draw an upright cosmic-text buffer,
/// so a 270-degree shape rotation produced horizontal text. Returns `false`
/// when no usable face is found, letting the caller fall back to the
/// axis-aligned painter rather than dropping the run.
pub(super) fn render_affine_run(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    WARP_FONT_DB.with(|cell| {
        let db = cell.borrow();
        let Some(face_id) = query_face(&db, run) else {
            return false;
        };
        let rendered = db.with_face_data(face_id, |data, index| {
            let Ok(face) = Face::parse(data, index) else {
                return false;
            };
            let units_per_em = face.units_per_em() as f32;
            if units_per_em <= 0.0 {
                return false;
            }
            let emu_per_unit = run.font_size_pt * 12700.0 / units_per_em;
            let [red, green, blue, alpha] = run.color;
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(red, green, blue, alpha);
            paint.anti_alias = true;

            let mut any = false;
            // `run.glyphs` is the shaped stream while `run.text` is the authored
            // character stream; cosmic_text can drop whitespace glyphs, so
            // realign by skipping a whitespace character while glyphs run short.
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
                let Some(glyph_id) = face.glyph_index(*ch) else {
                    continue;
                };
                let mut builder = AffineOutlineBuilder {
                    path: tiny_skia::PathBuilder::new(),
                    origin_x: glyph.x,
                    origin_y: glyph.y,
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
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
                any = true;
            }

            for line in [run.decorations.underline, run.decorations.strike]
                .into_iter()
                .flatten()
            {
                for offset in super::decorations::band_offsets(&line).into_iter().flatten() {
                    any |= paint_affine_bar(
                        pixmap,
                        run,
                        transform,
                        scale,
                        offset_x,
                        offset_y,
                        &line,
                        offset,
                    );
                }
            }
            any
        });
        rendered.unwrap_or(false)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_affine_bar(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    line: &crate::contracts::DecorationLine,
    band_offset: f32,
) -> bool {
    let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last()) else {
        return false;
    };
    let left = first.x;
    let right = (last.x + last.advance).max(left + 1.0);
    let thickness = line.thickness_emu.max(1.0 / scale.max(1.0e-6));
    let top = first.y + line.top_emu + band_offset;
    let bottom = top + thickness;
    let mut builder = tiny_skia::PathBuilder::new();
    let corners = [(left, top), (right, top), (right, bottom), (left, bottom)];
    for (index, (x, y)) in corners.iter().enumerate() {
        let (sx, sy) = transform.transform_point(*x, *y);
        let px = offset_x + sx * scale;
        let py = offset_y + sy * scale;
        if index == 0 {
            builder.move_to(px, py);
        } else {
            builder.line_to(px, py);
        }
    }
    builder.close();
    let Some(path) = builder.finish() else {
        return false;
    };
    let [red, green, blue, alpha] = line.color;
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
    true
}

struct AffineOutlineBuilder {
    path: tiny_skia::PathBuilder,
    origin_x: f32,
    origin_y: f32,
    emu_per_unit: f32,
    transform: AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl AffineOutlineBuilder {
    fn point(&mut self, x: f32, y: f32) -> (f32, f32) {
        let local_x = self.origin_x + x * self.emu_per_unit;
        let local_y = self.origin_y - y * self.emu_per_unit;
        let (slide_x, slide_y) = self.transform.transform_point(local_x, local_y);
        (
            self.offset_x + slide_x * self.scale,
            self.offset_y + slide_y * self.scale,
        )
    }
}

impl OutlineBuilder for AffineOutlineBuilder {
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
