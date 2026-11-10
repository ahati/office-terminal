use std::cell::RefCell;
use std::fmt::Write;
use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use rustybuzz::ttf_parser::{Face, GlyphId, OutlineBuilder};
use crate::contracts::{AffineTransform, PositionedGlyphRun, SvgTextMode};
use super::path::format_transform_attr;

thread_local! {
    static FONT_DB: RefCell<Database> = RefCell::new({
        let mut db = Database::new();
        db.load_system_fonts();
        db
    });
}

struct SvgGlyphBuilder {
    d: String,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl OutlineBuilder for SvgGlyphBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale; // Invert y for font coordinate space
        let _ = write!(&mut self.d, "M {:.1} {:.1} ", px, py);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(&mut self.d, "L {:.1} {:.1} ", px, py);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p1x = self.offset_x + x1 * self.scale;
        let p1y = self.offset_y - y1 * self.scale;
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(&mut self.d, "Q {:.1} {:.1}, {:.1} {:.1} ", p1x, p1y, px, py);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p1x = self.offset_x + x1 * self.scale;
        let p1y = self.offset_y - y1 * self.scale;
        let p2x = self.offset_x + x2 * self.scale;
        let p2y = self.offset_y - y2 * self.scale;
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(
            &mut self.d,
            "C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1} ",
            p1x, p1y, p2x, p2y, px, py
        );
    }

    fn close(&mut self) {
        self.d.push_str("Z ");
    }
}

pub fn serialize_svg_text(
    runs: &[PositionedGlyphRun],
    transform: &AffineTransform,
    text_mode: SvgTextMode,
) -> String {
    let mut out = String::new();
    let transform_attr = format_transform_attr(transform);

    match text_mode {
        SvgTextMode::SelectableText | SvgTextMode::TextTags => {
            serialize_selectable_text(runs, &transform_attr, &mut out);
        }
        SvgTextMode::VectorGlyphs => {
            let rendered = serialize_vector_glyphs(runs, &transform_attr, &mut out);
            if !rendered {
                // Fallback to selectable text if glyph extraction was unavailable
                serialize_selectable_text(runs, &transform_attr, &mut out);
            }
        }
    }

    out
}

fn serialize_selectable_text(
    runs: &[PositionedGlyphRun],
    transform_attr: &str,
    out: &mut String,
) {
    for run in runs {
        if run.text.is_empty() {
            continue;
        }

        let [r, g, b, a] = run.color;
        let opacity = a as f32 / 255.0;
        let font_size_emu = run.font_size_pt * 12700.0;

        let (first_x, first_y) = run
            .glyphs
            .first()
            .map(|g| (g.x, g.y))
            .unwrap_or((0.0, 0.0));

        let mut decor = Vec::new();
        if run.underline {
            decor.push("underline");
        }
        if run.strikethrough {
            decor.push("line-through");
        }
        let decor_attr = if !decor.is_empty() {
            format!(r#" text-decoration="{}""#, decor.join(" "))
        } else {
            String::new()
        };

        let escaped = xml_escape(&run.text);
        let _ = writeln!(
            out,
            r#"  <text x="{:.1}" y="{:.1}" font-family="{}" font-size="{:.1}" fill="rgb({},{},{})" fill-opacity="{:.2}"{}{}>{}</text>"#,
            first_x,
            first_y,
            xml_escape(&run.font_family),
            font_size_emu,
            r,
            g,
            b,
            opacity,
            decor_attr,
            transform_attr,
            escaped
        );
    }
}

fn serialize_vector_glyphs(
    runs: &[PositionedGlyphRun],
    transform_attr: &str,
    out: &mut String,
) -> bool {
    let mut any_rendered = false;

    FONT_DB.with(|db_cell| {
        let db = db_cell.borrow();

        for run in runs {
            if run.text.is_empty() {
                continue;
            }

            let query = Query {
                families: &[
                    Family::Name(&run.font_family),
                    Family::SansSerif,
                ],
                weight: Weight::NORMAL,
                stretch: Stretch::Normal,
                style: Style::Normal,
            };

            let face_id = match db.query(&query) {
                Some(id) => id,
                None => continue,
            };

            let [r, g, b, a] = run.color;
            let opacity = a as f32 / 255.0;
            let font_size_emu = run.font_size_pt * 12700.0;

            let path_d = db.with_face_data(face_id, |data, index| {
                let face = match Face::parse(data, index) {
                    Ok(f) => f,
                    Err(_) => return String::new(),
                };

                let units_per_em = face.units_per_em() as f32;
                if units_per_em <= 0.0 {
                    return String::new();
                }

                let scale = font_size_emu / units_per_em;
                let mut builder = SvgGlyphBuilder {
                    d: String::new(),
                    scale,
                    offset_x: 0.0,
                    offset_y: 0.0,
                };

                for (idx, ch) in run.text.chars().enumerate() {
                    let (gx, gy) = if let Some(g) = run.glyphs.get(idx) {
                        (g.x, g.y)
                    } else {
                        (idx as f32 * font_size_emu * 0.5, 0.0)
                    };

                    let glyph_id = face.glyph_index(ch).unwrap_or(GlyphId(0));
                    builder.offset_x = gx;
                    builder.offset_y = gy;
                    face.outline_glyph(glyph_id, &mut builder);
                }

                builder.d.trim().to_string()
            }).unwrap_or_default();

            if !path_d.is_empty() {
                let _ = writeln!(
                    out,
                    r#"  <path d="{}" fill="rgb({},{},{})" fill-opacity="{:.2}"{}/>"#,
                    path_d, r, g, b, opacity, transform_attr
                );
                any_rendered = true;
            }
        }
    });

    any_rendered
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
