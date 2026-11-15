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
use crate::contracts::{AffineTransform, PositionedGlyphRun, RunWarp, SvgTextMode};
use crate::compiler::text_warp::{warp_glyph_transform, warp_point};

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

/// SVG outline builder for a WordArt-warped glyph.
///
/// The path is emitted in shape coordinates; the enclosing `<g transform>` is
/// the element transform, exactly like the raster warp path. `u_center` is the
/// glyph's centre fraction along the warp line and `advance` its flat advance.
struct SvgWarpGlyphBuilder<'a> {
    d: String,
    run_warp: &'a RunWarp,
    u_center: f32,
    advance: f32,
    emu_per_unit: f32,
}

impl SvgWarpGlyphBuilder<'_> {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let local_x = x * self.emu_per_unit;
        let local_y = -y * self.emu_per_unit;
        let centered_x = local_x - self.advance * 0.5;
        let line_w = self.run_warp.line_w.max(1.0);
        let u = (self.u_center + centered_x / line_w).clamp(0.0, 1.0);
        let Some(warp) = warp_glyph_transform(
            &self.run_warp.envelope,
            u,
            self.run_warp.warp_box_h,
            self.run_warp.band_frac,
        ) else {
            return (0.0, 0.0);
        };
        let paired = warp_point(
            &self.run_warp.envelope,
            u,
            self.run_warp.band_frac + local_y / self.run_warp.warp_box_h.max(1.0),
        );
        match paired {
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
        }
    }
}

impl OutlineBuilder for SvgWarpGlyphBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        let _ = write!(&mut self.d, "M {:.1} {:.1} ", x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        let _ = write!(&mut self.d, "L {:.1} {:.1} ", x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        let _ = write!(&mut self.d, "Q {:.1} {:.1}, {:.1} {:.1} ", x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        let _ = write!(
            &mut self.d,
            "C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1} ",
            x1, y1, x2, y2, x, y
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

    // A warped run has no single affine text frame; always use outlines so the
    // per-glyph envelope survives in standalone SVG.
    if runs.iter().any(|run| run.warp.is_some()) {
        let rendered = serialize_vector_glyphs(runs, &transform_attr, &mut out);
        if !rendered {
            serialize_selectable_text(runs, &transform_attr, &mut out);
        }
        return out;
    }

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
                families: &[
                    Family::Name(&run.font_family),
                    Family::Name("Carlito"),
                    Family::Name("Caladea"),
                    Family::SansSerif,
                ],
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
                        if let Some(warp) = run.warp.as_ref() {
                            if let Some(glyph) = run.glyphs.get(idx) {
                                if let Some(u_center) = glyph.warp_u {
                                    let mut warped = SvgWarpGlyphBuilder {
                                        d: String::new(),
                                        run_warp: warp,
                                        u_center,
                                        advance: glyph.advance.max(0.0),
                                        emu_per_unit: scale,
                                    };
                                    face.outline_glyph(glyph_id, &mut warped);
                                    builder.d.push_str(&warped.d);
                                    continue;
                                }
                            }
                        }
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
