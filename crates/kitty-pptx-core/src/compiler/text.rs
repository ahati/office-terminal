use crossbeam_channel::Sender;
use crate::contracts::{
    AffineTransform, PositionedGlyph, PositionedGlyphRun, VectorCommand, WarningEvent,
};
use super::style::parse_hex_color;

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

    // First pass: lay out paragraphs into lines to calculate total block height
    struct LineLayout {
        runs: Vec<PositionedGlyphRun>,
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
        let mut cur_line_w = 0.0f32;
        let mut cur_line_h = def_sz * 12700.0 * 1.2 * font_scale;

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
            let b_sz_emu = b_sz * 12700.0;
            let run_w = estimate_text_width(b_text, b_sz_emu);
            let b_color = parse_hex_color(def_color);

            let mut glyphs = Vec::new();
            let mut gx = 0.0f32;
            for (idx, ch) in b_text.chars().enumerate() {
                let gw = estimate_char_width(ch, b_sz_emu);
                glyphs.push(PositionedGlyph {
                    glyph_id: idx as u16,
                    x: gx,
                    y: 0.0,
                    advance: gw,
                });
                gx += gw;
            }

            current_line_runs.push(PositionedGlyphRun {
                text: b_text.clone(),
                font_family: "Carlito".to_string(),
                font_size_pt: b_sz,
                color: b_color,
                glyphs,
                underline: false,
                strikethrough: false,
            });
            cur_line_w += run_w;
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

                    let run_w = estimate_text_width(&data.text, sz_emu);

                    let mut glyphs = Vec::new();
                    let mut gx = 0.0f32;
                    for (idx, ch) in data.text.chars().enumerate() {
                        let gw = estimate_char_width(ch, sz_emu);
                        glyphs.push(PositionedGlyph {
                            glyph_id: idx as u16,
                            x: gx,
                            y: 0.0,
                            advance: gw,
                        });
                        gx += gw;
                    }

                    current_line_runs.push(PositionedGlyphRun {
                        text: data.text.clone(),
                        font_family,
                        font_size_pt: sz_pt,
                        color,
                        glyphs,
                        underline: data.underline,
                        strikethrough: data.strikethrough,
                    });
                    cur_line_w += run_w;
                }
                pptx_model::TextRun::Break { .. } => {
                    // Flush line
                    lines.push(LineLayout {
                        runs: std::mem::take(&mut current_line_runs),
                        line_w: cur_line_w,
                        line_h: cur_line_h,
                        alignment: p.alignment.clone(),
                        mar_l: p.mar_l as f32,
                        avail_w,
                    });
                    cur_line_w = 0.0;
                }
                _ => {}
            }
        }

        if !current_line_runs.is_empty() || lines.is_empty() {
            lines.push(LineLayout {
                runs: current_line_runs,
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

    // Second pass: position lines according to alignment
    let mut final_runs = Vec::new();

    for line in lines {
        let align_offset = match line.alignment.as_str() {
            "ctr" => ((line.avail_w - line.line_w) / 2.0).max(0.0),
            "r" => (line.avail_w - line.line_w).max(0.0),
            _ => 0.0,
        };

        let mut cur_x = l_ins as f32 + line.mar_l + align_offset;
        let baseline_y = cur_y + line.line_h * 0.75;

        for mut run in line.runs {
            for g in &mut run.glyphs {
                g.x += cur_x;
                g.y = baseline_y;
            }
            let run_w = estimate_text_width(&run.text, run.font_size_pt * 12700.0);
            cur_x += run_w;
            final_runs.push(run);
        }

        cur_y += line.line_h;
    }

    if final_runs.is_empty() {
        Vec::new()
    } else {
        vec![VectorCommand::DrawText {
            runs: final_runs,
            transform: *base_transform,
        }]
    }
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

fn estimate_char_width(ch: char, font_size_emu: f32) -> f32 {
    let em = font_size_emu;
    if ch.is_ascii_punctuation() || ch == ' ' {
        em * 0.3
    } else if ch.is_ascii_digit() || ch.is_ascii_uppercase() {
        em * 0.6
    } else if ch.is_ascii_lowercase() {
        em * 0.5
    } else {
        em * 1.0 // CJK or wide characters
    }
}

fn estimate_text_width(text: &str, font_size_emu: f32) -> f32 {
    text.chars().map(|c| estimate_char_width(c, font_size_emu)).sum()
}
