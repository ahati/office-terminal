//! WordArt `textWarp` compile path (extracted from `compiler::text`).
//!
//! Kept in a submodule so the horizontal dispatcher stays under the file-size
//! limit. The warp geometry kernels live in [`super::super::text_warp`].

use std::sync::Arc;

use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping};
use crossbeam_channel::Sender;

use super::super::math::{self, MathParams};
use super::super::style::parse_hex_color;
use super::super::text_warp::{build_warp_envelope, follow_path_u_scale, warp_envelope_for_line};
use super::{map_bullet_char, resolve_font_family};
use crate::contracts::{
    AffineTransform, FidelityClass, PositionedGlyph, PositionedGlyphRun, RunWarp, VectorCommand, WarningEvent,
    EMU_PER_PT,
};

struct ShapedText {
    glyphs: Vec<PositionedGlyph>,
    width: f32,
}

/// Shape one run with the same font stack used by the rasterizer so run
/// advances are real font metrics, not per-character heuristics. The font
/// system is the crate's ONE shared per-thread runtime
/// (`crate::text::fonts::with_font_system`), the very store the measurer and
/// the painter resolve through.
fn shape_text_run(text: &str, font_family: &str, font_size_emu: f32, bold: bool, italic: bool) -> ShapedText {
    crate::text::fonts::with_font_system(|font_system| {
        let size_px = (font_size_emu / EMU_PER_PT).max(1.0);
        let mut buffer = Buffer::new(font_system, Metrics::new(size_px, size_px * 1.25));
        buffer.set_size(font_system, None, None);
        let mut attrs = Attrs::new().family(Family::Name(font_family));
        if bold {
            attrs = attrs.weight(cosmic_text::Weight::BOLD);
        }
        if italic {
            attrs = attrs.style(cosmic_text::Style::Italic);
        }
        buffer.set_text(font_system, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);

        let mut glyphs = Vec::new();
        let mut width = 0.0f32;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            for glyph in run.glyphs {
                glyphs.push(PositionedGlyph {
                    glyph_id: glyph.glyph_id,
                    x: glyph.x * EMU_PER_PT,
                    y: 0.0,
                    advance: glyph.w * EMU_PER_PT,
                    warp_u: None,
                });
            }
        }
        ShapedText {
            glyphs,
            width: width * EMU_PER_PT,
        }
    })
}

fn warp_space_line(space: &Option<ooxml_common::text::SpaceLine>) -> crate::text::metrics::SpaceLine {
    match space {
        Some(ooxml_common::text::SpaceLine::Pct { val }) => Some((true, *val as f32)),
        Some(ooxml_common::text::SpaceLine::Pts { val }) => Some((false, *val as f32)),
        None => None,
    }
}

fn warp_space_before(paragraph: &pptx_model::Paragraph, natural: f32) -> f32 {
    if let Some(pct) = paragraph.space_before_pct {
        natural * pct as f32 / 100_000.0
    } else {
        paragraph
            .space_before
            .map(|pts| pts as f32 / 100.0 * EMU_PER_PT)
            .unwrap_or(0.0)
    }
}

fn warp_space_after(paragraph: &pptx_model::Paragraph, natural: f32) -> f32 {
    if let Some(pct) = paragraph.space_after_pct {
        natural * pct as f32 / 100_000.0
    } else {
        paragraph
            .space_after
            .map(|pts| pts as f32 / 100.0 * EMU_PER_PT)
            .unwrap_or(0.0)
    }
}

pub(super) fn compile_text_body_warped(
    text_body: &pptx_model::TextBody,
    box_w: i64,
    box_h: i64,
    base_transform: &AffineTransform,
    default_shape_text_color: Option<&str>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let l_ins = text_body.l_ins.max(0);
    let r_ins = text_body.r_ins.max(0);
    let t_ins = text_body.t_ins.max(0);
    let b_ins = text_body.b_ins.max(0);

    let avail_w = (box_w - l_ins - r_ins).max(0) as f32;
    let avail_h = (box_h - t_ins - b_ins).max(0) as f32;

    let font_scale = text_body.font_scale.unwrap_or(1.0) as f32;
    let text_warp = text_body.text_warp.as_ref();
    let warp_envelope = text_warp
        .and_then(|warp| build_warp_envelope(&warp.preset, &warp.adj, box_w as f32, box_h as f32))
        .map(Arc::new);
    if text_warp.is_some() {
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: "text-warp".to_string(),
            feature: "text_warp",
            class: FidelityClass::Degraded,
        });
    }

    // First pass: lay out paragraphs into lines to calculate total block height
    struct LineLayout {
        runs: Vec<PositionedGlyphRun>,
        /// Math runs whose glyph offsets are already line-relative.
        math_runs: Vec<PositionedGlyphRun>,
        /// Math rules/brackets in line-relative (y-down) coordinates.
        paths: Vec<math::MathPath>,
        line_w: f32,
        line_h: f32,
        alignment: String,
        mar_l: f32,
        avail_w: f32,
    }

    let mut lines: Vec<LineLayout> = Vec::new();
    let mut paragraph_ranges: Vec<(usize, usize, usize)> = Vec::new();

    for (p_idx, p) in text_body.paragraphs.iter().enumerate() {
        let first_line = lines.len();
        let def_sz = p.def_font_size.or(text_body.default_font_size).unwrap_or(18.0) as f32;
        let def_color = p.def_color.as_deref().or(default_shape_text_color).unwrap_or("000000");

        let mut current_line_runs: Vec<PositionedGlyphRun> = Vec::new();
        let mut current_line_math_runs: Vec<PositionedGlyphRun> = Vec::new();
        let mut current_line_paths: Vec<math::MathPath> = Vec::new();
        let mut cur_line_w = 0.0f32;
        let mut cur_line_h = 0.0f32;

        // Process bullet if present
        let bullet_str = match &p.bullet {
            pptx_model::Bullet::Char { ch, font_family, .. } => {
                let mapped = map_bullet_char(ch, font_family.as_deref());
                Some(format!("{} ", mapped))
            }
            pptx_model::Bullet::AutoNum { start_at, .. } => {
                let num = start_at.unwrap_or(1) + (p_idx as u32);
                Some(format!("{}. ", num))
            }
            _ => None,
        };

        if let Some(ref b_text) = bullet_str {
            let b_sz = def_sz * font_scale;
            let b_sz_emu = b_sz * EMU_PER_PT;
            let shaped = shape_text_run(b_text, "Carlito", b_sz_emu, false, false);
            let b_color = parse_hex_color(def_color);

            let mut glyphs = shaped.glyphs;
            for glyph in &mut glyphs {
                glyph.x += cur_line_w;
            }

            current_line_runs.push(PositionedGlyphRun {
                text: b_text.clone(),
                font_family: "Carlito".to_string(),
                font_size_pt: b_sz,
                color: b_color,
                bold: false,
                italic: false,
                glyphs,
                decorations: Default::default(),
                single_face: false,
                face: None,
                warp: None,
                reflection: None,
            });
            cur_line_w += shaped.width;
        }

        // Walk runs
        for r in &p.runs {
            match r {
                pptx_model::TextRun::Text(data) => {
                    if data.text.is_empty() {
                        continue;
                    }
                    let raw_family = data
                        .font_family
                        .as_deref()
                        .or(p.def_font_family.as_deref())
                        .unwrap_or("Calibri");
                    let font_family = resolve_font_family(raw_family, warning_sink);

                    let sz_pt = (data.font_size.map(|s| s as f32).unwrap_or(def_sz)) * font_scale;
                    let sz_emu = sz_pt * EMU_PER_PT;
                    cur_line_h = cur_line_h.max(sz_emu * 1.2);

                    let color_hex = data
                        .color
                        .as_deref()
                        .or(p.def_color.as_deref())
                        .or(default_shape_text_color)
                        .unwrap_or(def_color);
                    let color = parse_hex_color(color_hex);

                    let bold = data.bold.or(p.def_bold).or(text_body.default_bold).unwrap_or(false);
                    let italic = data
                        .italic
                        .or(p.def_italic)
                        .or(text_body.default_italic)
                        .unwrap_or(false);

                    let shaped = shape_text_run(&data.text, &font_family, sz_emu, bold, italic);
                    let mut glyphs = shaped.glyphs;
                    for glyph in &mut glyphs {
                        glyph.x += cur_line_w;
                    }

                    current_line_runs.push(PositionedGlyphRun {
                        text: data.text.clone(),
                        font_family: font_family.clone(),
                        font_size_pt: sz_pt,
                        color,
                        bold,
                        italic,
                        glyphs,
                        decorations: crate::text::run_decorations::resolve_raw(
                            &font_family,
                            bold,
                            italic,
                            sz_emu,
                            color,
                            data.underline,
                            data.underline_style.as_deref(),
                            data.underline_color.as_deref().map(parse_hex_color),
                            data.strikethrough,
                            data.strike_double,
                        ),
                        single_face: false,
                        face: None,
                        warp: None,
                        reflection: data.reflection.as_ref().map(crate::compiler::shape_effects::reflection),
                    });
                    cur_line_w += shaped.width;
                }
                pptx_model::TextRun::Break { .. } => {
                    // Flush line
                    lines.push(LineLayout {
                        runs: std::mem::take(&mut current_line_runs),
                        math_runs: std::mem::take(&mut current_line_math_runs),
                        paths: std::mem::take(&mut current_line_paths),
                        line_w: cur_line_w,
                        line_h: if cur_line_h > 0.0 {
                            cur_line_h
                        } else {
                            def_sz * EMU_PER_PT * 1.2 * font_scale
                        },
                        alignment: p.alignment.clone(),
                        mar_l: p.mar_l as f32,
                        avail_w,
                    });
                    cur_line_w = 0.0;
                    cur_line_h = 0.0;
                }
                pptx_model::TextRun::Math {
                    nodes,
                    display,
                    font_size,
                    color: math_color,
                    fallback_text,
                } => {
                    let sz_pt = font_size.map(|s| s as f32).unwrap_or(def_sz) * font_scale;
                    let color_hex = math_color
                        .as_deref()
                        .or(p.def_color.as_deref())
                        .or(default_shape_text_color)
                        .unwrap_or(def_color);
                    let params = MathParams {
                        display: *display,
                        font_size_pt: sz_pt,
                        color: parse_hex_color(color_hex),
                        fallback: fallback_text.as_deref(),
                    };
                    let piece = math::layout_math(nodes, params, warning_sink);
                    if piece.is_empty() {
                        continue;
                    }
                    let piece_h = (piece.ascent + piece.descent) * 1.4;
                    if *display {
                        // Display math is its own centred block (PRD 09 §1.2).
                        if !current_line_runs.is_empty()
                            || !current_line_math_runs.is_empty()
                            || !current_line_paths.is_empty()
                        {
                            lines.push(LineLayout {
                                runs: std::mem::take(&mut current_line_runs),
                                math_runs: std::mem::take(&mut current_line_math_runs),
                                paths: std::mem::take(&mut current_line_paths),
                                line_w: cur_line_w,
                                line_h: if cur_line_h > 0.0 {
                                    cur_line_h
                                } else {
                                    def_sz * EMU_PER_PT * 1.2 * font_scale
                                },
                                alignment: p.alignment.clone(),
                                mar_l: p.mar_l as f32,
                                avail_w,
                            });
                            cur_line_h = 0.0;
                        }
                        // `m:oMathParaPr/m:jc` is not surfaced by the parser, so
                        // display math uses the PRD default: centred (only an
                        // explicit right paragraph alignment is honoured).
                        let alignment = if p.alignment == "r" {
                            "r".to_string()
                        } else {
                            "ctr".to_string()
                        };
                        let display_h = piece_h + sz_pt * EMU_PER_PT * 0.6;
                        lines.push(LineLayout {
                            runs: Vec::new(),
                            math_runs: piece.runs,
                            paths: piece.paths,
                            line_w: piece.width,
                            line_h: display_h.max(cur_line_h),
                            alignment,
                            mar_l: 0.0,
                            avail_w,
                        });
                        cur_line_w = 0.0;
                        cur_line_h = 0.0;
                    } else {
                        let base_x = cur_line_w;
                        current_line_math_runs.extend(math::offset_runs(piece.runs, base_x, 0.0));
                        current_line_paths.extend(math::offset_math_paths(piece.paths, base_x));
                        cur_line_w += piece.width;
                        cur_line_h = cur_line_h.max(piece_h);
                    }
                }
            }
        }

        if !current_line_runs.is_empty()
            || !current_line_math_runs.is_empty()
            || !current_line_paths.is_empty()
            || lines.is_empty()
        {
            lines.push(LineLayout {
                runs: current_line_runs,
                math_runs: current_line_math_runs,
                paths: current_line_paths,
                line_w: cur_line_w,
                line_h: if cur_line_h > 0.0 {
                    cur_line_h
                } else {
                    def_sz * EMU_PER_PT * 1.2 * font_scale
                },
                alignment: p.alignment.clone(),
                mar_l: p.mar_l as f32,
                avail_w,
            });
        }
        paragraph_ranges.push((p_idx, first_line, lines.len()));
    }

    // Apply `a:lnSpc`, `spcBef` and `spcAft` after the line boxes are known.
    let reduction = if text_body.auto_fit == "norm" {
        text_body.ln_spc_reduction.unwrap_or(0.0) as f32
    } else {
        0.0
    };
    for (p_idx, start, end) in paragraph_ranges {
        if start >= end {
            continue;
        }
        let paragraph = &text_body.paragraphs[p_idx];
        let space = warp_space_line(&paragraph.space_line);
        for line in &mut lines[start..end] {
            line.line_h = crate::text::metrics::line_height(line.line_h, space, EMU_PER_PT, reduction);
        }
        let before = warp_space_before(paragraph, lines[start].line_h);
        let after = warp_space_after(paragraph, lines[end - 1].line_h);
        lines[start].line_h += before;
        lines[end - 1].line_h += after;
    }

    if lines.is_empty() {
        return Vec::new();
    }

    // Compute total text block height
    let total_h: f32 = lines.iter().map(|l| l.line_h).sum();

    // Vertical anchor calculation
    let mut cur_y = match text_body.vertical_anchor.as_str() {
        "ctr" => t_ins as f32 + ((avail_h - total_h) / 2.0).max(0.0),
        "b" => t_ins as f32 + (avail_h - total_h).max(0.0),
        _ => t_ins as f32,
    };

    // Second pass: translate every line-relative item by its line origin.
    let mut final_runs = Vec::new();
    let mut final_paths: Vec<VectorCommand> = Vec::new();

    let line_count = lines.len().max(1);
    for (line_index, line) in lines.into_iter().enumerate() {
        let align_offset = match line.alignment.as_str() {
            "ctr" => ((line.avail_w - line.line_w) / 2.0).max(0.0),
            "r" => (line.avail_w - line.line_w).max(0.0),
            _ => 0.0,
        };

        let line_x = l_ins as f32 + line.mar_l + align_offset;
        let baseline_y = cur_y + line.line_h * 0.8;
        let line_w = line.line_w.max(1.0);
        let max_size = line
            .runs
            .iter()
            .map(|run| run.font_size_pt)
            .fold(0.0f32, f32::max)
            .max(1.0);
        let ink_h = max_size * EMU_PER_PT;

        // Paired-edge (envelope) warp parameters, ref `renderer.ts:3236-3305`:
        // the shape's height is split into `line_count` horizontal bands (one
        // per laid-out line, `v0 = li/n` → `v1 = (li+1)/n`) and each line's
        // flat ink box fills its whole band. `warp_box_h` is therefore the
        // line's ink height DIVIDED by its band share, i.e. `inkH / (v1 - v0)`
        // — the band fraction (`band_frac`) places the same line's BASELINE at
        // the synthetic em box's ascent share inside that band, so the ink top
        // lands on `T(u)` and the ink bottom on `B(u)`.
        //
        // `h_scale` is the reference's `hScale = boxW / totalW`; the paired map
        // already contains it (the envelope follows `u` across the glyph), so
        // only single-edge Follow Path callers consume it.
        let ascent_share = crate::text::metrics::SYNTHETIC_ASCENT_EM;
        let (band_frac, warp_box_h, h_scale, follow, follow_offset) = if let Some(envelope) = &warp_envelope {
            if envelope.single_edge {
                // Follow Path keeps the natural glyph size; `band_frac` is
                // the flat renderer's ascent fallback used as the baseline
                // drop below the arc (ref `renderer.ts:3287`).
                let follow = follow_path_u_scale(envelope, line_w);
                let offset = match line.alignment.as_str() {
                    "r" => 1.0,
                    "ctr" => 0.5,
                    _ => 0.0,
                } * (1.0 - follow);
                (ascent_share, ink_h, 1.0, follow, offset)
            } else {
                let v0 = line_index as f32 / line_count as f32;
                let v1 = (line_index + 1) as f32 / line_count as f32;
                let band_frac = v0 + ascent_share * (v1 - v0);
                let band_height = (v1 - v0).max(f32::EPSILON);
                let h_scale = box_w as f32 / line_w;
                (band_frac, ink_h / band_height, h_scale, 1.0, 0.0)
            }
        } else {
            (ascent_share, ink_h, 1.0, 1.0, 0.0)
        };
        let line_envelope = warp_envelope.as_ref().map(|envelope| {
            if envelope.single_edge {
                // `textArchDown` is wound counterclockwise: its FIRST line stays
                // on the authored arch and later lines expand outward. Clockwise
                // arch/circle presets anchor the LAST line instead.
                let counterclockwise = text_warp.is_some_and(|warp| warp.preset.eq_ignore_ascii_case("textArchDown"));
                let steps = if counterclockwise {
                    line_index
                } else {
                    line_count.saturating_sub(1).saturating_sub(line_index)
                };
                let offset = steps as f32 * line.line_h;
                Arc::new(warp_envelope_for_line(envelope, offset))
            } else {
                Arc::clone(envelope)
            }
        });
        let line_warp = line_envelope.map(|envelope| {
            Arc::new(RunWarp {
                envelope,
                band_frac,
                warp_box_h,
                h_scale,
                line_w,
            })
        });
        let mut pen_w = 0.0f32;
        for mut run in line.runs {
            for g in &mut run.glyphs {
                g.x += line_x;
                g.y += baseline_y;
            }
            if line_warp.is_some() {
                run.warp = line_warp.clone();
                for glyph in &mut run.glyphs {
                    let advance = glyph.advance.max(0.0);
                    let u = (follow_offset + ((pen_w + advance * 0.5) / line_w) * follow).clamp(0.0, 1.0);
                    glyph.warp_u = Some(u);
                    if std::env::var("OT_DEBUG_WARP").is_ok() || std::env::var("KITTY_PPTX_DEBUG_WARP").is_ok() {
                        eprintln!(
                            "line {} run {:?} adv={:.0} pen={:.0} u={:.3} hscale={:.3}",
                            line_index + 1,
                            run.text,
                            advance,
                            pen_w,
                            u,
                            h_scale
                        );
                    }
                    pen_w += advance;
                }
            }
            final_runs.push(run);
        }
        for mut run in line.math_runs {
            for g in &mut run.glyphs {
                g.x += line_x;
                g.y += baseline_y;
            }
            final_runs.push(run);
        }
        for path in &line.paths {
            let shifted = math::translate_math_path(path, line_x, baseline_y);
            final_paths.push(math::math_path_command(&shifted, base_transform));
        }

        cur_y += line.line_h;
    }

    let mut commands = final_paths;
    if !final_runs.is_empty() {
        commands.push(VectorCommand::DrawText {
            runs: final_runs,
            transform: *base_transform,
        });
    }
    commands
}
