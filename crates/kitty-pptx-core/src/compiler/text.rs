use std::cell::RefCell;
use std::sync::Arc;

use super::math::{self, MathParams};
use super::style::parse_hex_color;
use super::text_warp::{build_warp_envelope, follow_path_u_scale};
use crate::contracts::{
    AffineTransform, FidelityClass, PositionedGlyph, PositionedGlyphRun, RunWarp, VectorCommand,
    WarningEvent,
};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};
use crossbeam_channel::Sender;

thread_local! {
    static FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(FontSystem::new());
}

struct ShapedText {
    glyphs: Vec<PositionedGlyph>,
    width: f32,
}

/// Shape one run with the same font stack used by the rasterizer so run
/// advances are real font metrics, not per-character heuristics.
fn shape_text_run(
    text: &str,
    font_family: &str,
    font_size_emu: f32,
    bold: bool,
    italic: bool,
) -> ShapedText {
    FONT_SYSTEM.with(|cell| {
        let font_system = &mut *cell.borrow_mut();
        let size_px = (font_size_emu / 12700.0).max(1.0);
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
                    x: glyph.x * 12700.0,
                    y: 0.0,
                    advance: glyph.w * 12700.0,
                    warp_u: None,
                });
            }
        }
        ShapedText {
            glyphs,
            width: width * 12700.0,
        }
    })
}

pub fn compile_text_body(
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

    for (p_idx, p) in text_body.paragraphs.iter().enumerate() {
        // Space before
        let space_before_emu = if p_idx > 0 || text_body.spc_first_last_para {
            if let Some(pts) = p.space_before {
                (pts as f32 / 100.0) * 12700.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        let def_sz = p
            .def_font_size
            .or(text_body.default_font_size)
            .unwrap_or(18.0) as f32;
        let def_color = p
            .def_color
            .as_deref()
            .or(default_shape_text_color)
            .unwrap_or("000000");

        let mut current_line_runs: Vec<PositionedGlyphRun> = Vec::new();
        let mut current_line_math_runs: Vec<PositionedGlyphRun> = Vec::new();
        let mut current_line_paths: Vec<math::MathPath> = Vec::new();
        let mut cur_line_w = 0.0f32;
        let mut cur_line_h = def_sz * 12700.0 * 1.2 * font_scale;

        // Process bullet if present
        let bullet_str = match &p.bullet {
            pptx_model::Bullet::Char {
                ch, font_family, ..
            } => {
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
            let b_sz_emu = b_sz * 12700.0;
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
                underline: false,
                strikethrough: false,
                warp: None,
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
                    let sz_emu = sz_pt * 12700.0;
                    cur_line_h = cur_line_h.max(sz_emu * 1.2);

                    let color_hex = data
                        .color
                        .as_deref()
                        .or(p.def_color.as_deref())
                        .or(default_shape_text_color)
                        .unwrap_or(def_color);
                    let color = parse_hex_color(color_hex);

                    let bold = data
                        .bold
                        .or(p.def_bold)
                        .or(text_body.default_bold)
                        .unwrap_or(false);
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
                        font_family,
                        font_size_pt: sz_pt,
                        color,
                        bold,
                        italic,
                        glyphs,
                        underline: data.underline,
                        strikethrough: data.strikethrough,
                        warp: None,
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
                        line_h: cur_line_h,
                        alignment: p.alignment.clone(),
                        mar_l: p.mar_l as f32,
                        avail_w,
                    });
                    cur_line_w = 0.0;
                    cur_line_h = def_sz * 12700.0 * 1.2 * font_scale;
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
                                line_h: cur_line_h,
                                alignment: p.alignment.clone(),
                                mar_l: p.mar_l as f32,
                                avail_w,
                            });
                            cur_line_h = def_sz * 12700.0 * 1.2 * font_scale;
                        }
                        // `m:oMathParaPr/m:jc` is not surfaced by the parser, so
                        // display math uses the PRD default: centred (only an
                        // explicit right paragraph alignment is honoured).
                        let alignment = if p.alignment == "r" {
                            "r".to_string()
                        } else {
                            "ctr".to_string()
                        };
                        let display_h = piece_h + sz_pt * 12700.0 * 0.6;
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
                        cur_line_h = def_sz * 12700.0 * 1.2 * font_scale;
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
                line_h: cur_line_h + space_before_emu,
                alignment: p.alignment.clone(),
                mar_l: p.mar_l as f32,
                avail_w,
            });
        }
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
        let baseline_y = cur_y + line.line_h * 0.75;
        let line_w = line.line_w.max(1.0);
        let max_size = line
            .runs
            .iter()
            .map(|run| run.font_size_pt)
            .fold(0.0f32, f32::max)
            .max(1.0);
        let ink_h = max_size * 12700.0;

        let (band_frac, warp_box_h, h_scale, follow, follow_offset) =
            if let Some(envelope) = &warp_envelope {
                if envelope.single_edge {
                    let follow = follow_path_u_scale(envelope, line_w);
                    let offset = match line.alignment.as_str() {
                        "r" => 1.0,
                        "ctr" => 0.5,
                        _ => 0.0,
                    } * (1.0 - follow);
                    (0.8, ink_h, 1.0, follow, offset)
                } else {
                    let v0 = line_index as f32 / line_count as f32;
                    let v1 = (line_index + 1) as f32 / line_count as f32;
                    let band_frac = v0 + 0.8 * (v1 - v0);
                    let band_height = (v1 - v0).max(f32::EPSILON);
                    let h_scale = box_w as f32 / line_w;
                    (band_frac, ink_h / band_height, h_scale, 1.0, 0.0)
                }
            } else {
                (0.8, ink_h, 1.0, 1.0, 0.0)
            };
        let line_warp = warp_envelope.as_ref().map(|envelope| {
            Arc::new(RunWarp {
                envelope: Arc::clone(envelope),
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
                    let u = (follow_offset + ((pen_w + advance * 0.5) / line_w) * follow)
                        .clamp(0.0, 1.0);
                    glyph.warp_u = Some(u);
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

pub fn resolve_font_family(name: &str, warning_sink: &Sender<WarningEvent>) -> String {
    let lower = name.to_lowercase();
    if lower.contains("calibri") {
        "Carlito".to_string()
    } else if lower.contains("cambria") {
        "Caladea".to_string()
    } else if lower.contains("aptos") {
        let _ = warning_sink.send(WarningEvent::FontSubstituted {
            requested: name.to_string(),
            substitute: "Carlito".to_string(),
        });
        "Carlito".to_string()
    } else if lower.contains("arial") {
        "Liberation Sans".to_string()
    } else if lower.contains("times") {
        "Liberation Serif".to_string()
    } else if lower.contains("consolas") || lower.contains("courier") {
        "Liberation Mono".to_string()
    } else {
        name.to_string()
    }
}

pub fn map_bullet_char(ch: &str, font_family: Option<&str>) -> String {
    let is_symbol_font = font_family
        .map(|f| {
            let fl = f.to_lowercase();
            fl.contains("wingdings") || fl.contains("symbol") || fl.contains("webdings")
        })
        .unwrap_or(false);

    if let Some(first_char) = ch.chars().next() {
        let code = first_char as u32;
        if is_symbol_font {
            match code {
                0x6C | 0xF06C => return "●".to_string(),
                0x6E | 0xF06E => return "■".to_string(),
                0x75 | 0xF075 => return "◆".to_string(),
                0xD8 | 0xF0D8 => return "➢".to_string(),
                0xFC | 0xF0FC => return "✔".to_string(),
                0xB7 | 0xF0B7 => return "•".to_string(),
                _ => {}
            }
        }
    }

    if ch.is_empty() {
        "•".to_string()
    } else {
        ch.to_string()
    }
}
