//! Per-run SVG text emission.
//!
//! The compiler emits one [`PositionedGlyphRun`] per broken line, with
//! `glyph.y` holding the line baseline and `glyph.x`/`glyph.advance` holding
//! the pen positions and advances the rasterizer paints
//! (`text/paint.rs::emit_run`). Both modes here therefore derive their origin
//! from the same glyph data: the first glyph gives the run pen x and the
//! baseline y, and every further glyph keeps its own placed position.

use std::cell::RefCell;
use std::fmt::Write;

use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use rustybuzz::ttf_parser::{Face, GlyphId, OutlineBuilder};

use super::path::format_transform_attr;
use crate::contracts::{AffineTransform, PositionedGlyphRun, SvgTextMode};

thread_local! {
    static FONT_DB: RefCell<Database> = RefCell::new({
        let mut db = Database::new();
        db.load_system_fonts();
        crate::text::fonts::install_into_db(&mut db);
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

/// The run pen origin and line baseline, in shape EMU.
///
/// The first glyph's `x` is the pen position the layout pass handed to the
/// run, and its `y` is the baseline of the broken line it belongs to
/// (`text/paint.rs::emit_run` positions every glyph of a run on one
/// `baseline_y`), so a multi-line body yields one distinct baseline per run.
fn run_origin(run: &PositionedGlyphRun) -> (f32, f32) {
    match run.glyphs.first() {
        Some(glyph) => (glyph.x, glyph.y),
        None => (0.0, 0.0),
    }
}

/// `(measured, natural)`: the width the placed glyphs occupy and the sum of
/// their advances.
///
/// `measured` is the glyph extent `max(x + advance) - min(x)` — the same
/// quantity the rasterizer uses for its underline and strike bars
/// (`rasterizer/text.rs::estimate_run_px_width`). It diverges from `natural`
/// when the shaper kerned a pair tighter than the raw advances, and on a
/// justified line once an inter-word gap lands inside the run.
fn run_advances(run: &PositionedGlyphRun) -> (f32, f32) {
    let (mut left, mut right, mut natural) = (f32::INFINITY, f32::NEG_INFINITY, 0.0_f32);
    for glyph in &run.glyphs {
        left = left.min(glyph.x);
        right = right.max(glyph.x + glyph.advance);
        natural += glyph.advance;
    }
    if run.glyphs.is_empty() {
        return (0.0, 0.0);
    }
    ((right - left).max(0.0), natural)
}

/// Pin the run's advance when the shaper's placement and the glyph advances
/// disagree, so a justified or kerned run keeps the raster's extent instead of
/// whatever the SVG viewer's own shaper would produce for the same string.
fn format_text_length_attr(run: &PositionedGlyphRun) -> String {
    let (measured, natural) = run_advances(run);
    if measured <= 0.0 {
        return String::new();
    }
    // 1/1000 em keeps advance-rounding noise out of the markup.
    let tolerance = (run.font_size_pt * 12700.0 * 1e-3).max(0.5);
    if (measured - natural).abs() <= tolerance {
        return String::new();
    }
    format!(
        r#" textLength="{:.1}" lengthAdjust="spacingAndGlyphs""#,
        measured
    )
}

/// `fill`, face and decoration attributes shared by both text modes.
fn format_run_attrs(run: &PositionedGlyphRun) -> String {
    let [r, g, b, _] = run.color;
    let mut attrs = String::new();
    let _ = write!(
        &mut attrs,
        r#" font-family="{}" font-size="{:.1}" fill="rgb({},{},{})" fill-opacity="{:.2}""#,
        xml_escape(&run.font_family),
        run.font_size_pt * 12700.0,
        r,
        g,
        b,
        run.color[3] as f32 / 255.0
    );
    if run.bold {
        attrs.push_str(r#" font-weight="bold""#);
    }
    if run.italic {
        attrs.push_str(r#" font-style="italic""#);
    }
    let mut decor = Vec::new();
    if run.underline {
        decor.push("underline");
    }
    if run.strikethrough {
        decor.push("line-through");
    }
    if !decor.is_empty() {
        let _ = write!(&mut attrs, r#" text-decoration="{}""#, decor.join(" "));
    }
    attrs
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

        let (x, baseline_y) = run_origin(run);
        let mut attrs = format_run_attrs(run);
        // Leading/trailing spaces are part of the authored run, so XML's
        // default whitespace collapsing must not touch them.
        attrs.push_str(r#" xml:space="preserve""#);
        attrs.push_str(&format_text_length_attr(run));
        attrs.push_str(transform_attr);

        let _ = writeln!(
            out,
            r#"  <text x="{:.1}" y="{:.1}"{}>{}</text>"#,
            x,
            baseline_y,
            attrs,
            xml_escape(&run.text)
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
                families: &[Family::Name(&run.font_family), Family::SansSerif],
                weight: if run.bold { Weight::BOLD } else { Weight::NORMAL },
                stretch: Stretch::Normal,
                style: if run.italic { Style::Italic } else { Style::Normal },
            };

            let face_id = match db.query(&query) {
                Some(id) => id,
                None => continue,
            };

            let [r, g, b, a] = run.color;
            let opacity = a as f32 / 255.0;
            let font_size_emu = run.font_size_pt * 12700.0;
            // Same origin/baseline the `<text>` mode uses, so both modes land
            // the run in the same place (`run_origin`).
            let (origin_x, baseline_y) = run_origin(run);

            let path_d = db
                .with_face_data(face_id, |data, index| {
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
                        let (gx, gy) = match run.glyphs.get(idx) {
                            Some(glyph) => (glyph.x, glyph.y),
                            // A char without a placed glyph keeps the run's
                            // baseline and a half-em pen estimate.
                            None => (origin_x + idx as f32 * font_size_emu * 0.5, baseline_y),
                        };

                        let glyph_id = face.glyph_index(ch).unwrap_or(GlyphId(0));
                        builder.offset_x = gx;
                        builder.offset_y = gy;
                        face.outline_glyph(glyph_id, &mut builder);
                    }

                    builder.d.trim().to_string()
                })
                .unwrap_or_default();

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
