//! The parse-error card: a slide whose part failed to parse paints a labelled
//! placeholder instead of (empty) content.
//!
//! Ported from `drawParseErrorPlaceholder` (ref renderer.ts:8188-8274). The
//! reference paints into a dpr-scaled canvas whose CSS size is
//! `widthPx × heightPx`; here the slide box is EMU, so every authoring number is
//! converted at the 96-DPI basis `1 px = 9525 EMU` (AGENTS.md §4). The card is
//! drawn as ordinary display-list commands, so both backends inherit it and a
//! rasterized card measures exactly like the reference's.

use crate::compiler::options::ErrorPlaceholder;
use crate::contracts::{
    AffineTransform, FillRule, LineCap, LineJoin, PathSegment, PositionedGlyph, PositionedGlyphRun, VectorCommand,
    VectorPaint, VectorPath, VectorStroke, EMU_PER_PX,
};

/// CSS px → pt (`1 px = 0.75 pt`), for `font_size_pt`.
const PT_PER_PX: f32 = 0.75;
/// Fallback advance width per character, as a fraction of the em box. The card
/// uses one application face; the reference measures with `measureText` on
/// `${n}px sans-serif`, so the wrap width matches to within the face's own
/// advance.
const ADVANCE_EM: f32 = 0.55;
const MAX_DETAIL_LINES: usize = 4;

const CARD_BG: [u8; 4] = [0xf7, 0xf7, 0xf8, 255];
const CARD_BORDER: [u8; 4] = [0xc8, 0xcc, 0xd2, 255];
const GLYPH_COLOR: [u8; 4] = [0xb2, 0x3b, 0x3b, 255];
const HEAD_COLOR: [u8; 4] = [0x33, 0x33, 0x33, 255];
const DETAIL_COLOR: [u8; 4] = [0x66, 0x66, 0x66, 255];

fn identity() -> AffineTransform {
    AffineTransform {
        sx: 1.0,
        ky: 0.0,
        kx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    }
}

fn rect_path(w: f32, h: f32) -> VectorPath {
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    )
}

/// `max(floor, share × min(w, h))` in CSS px.
fn scaled(min_px: f32, share: f32, floor: f32) -> f32 {
    (min_px * share).max(floor)
}

/// Build the placeholder display list for a slide with `parse_error`.
///
/// * `slide_number` is the reference's `slide.slideNumber` (1-based).
/// * `message` is the parser's part-tagged diagnostic.
/// * `placeholder` is the caller-resolved override; `None` keeps the built-in
///   heading `Slide N could not be displayed` and the message as detail.
///
/// ref renderer.ts:8477-8487 — the card is painted after the canvas clear and
/// before *any* element or background work.
pub fn compile_parse_error_card(
    width_emu: i64,
    height_emu: i64,
    slide_number: usize,
    message: &str,
    placeholder: Option<&ErrorPlaceholder>,
) -> Vec<VectorCommand> {
    let w_px = width_emu as f32 / EMU_PER_PX;
    let h_px = height_emu as f32 / EMU_PER_PX;
    let min_px = w_px.min(h_px);
    let width = width_emu as f32;
    let height = height_emu as f32;

    let mut commands = Vec::new();

    // Neutral card fill across the whole slide.
    commands.push(VectorCommand::draw_path(
        rect_path(width, height),
        Some(VectorPaint::Solid(CARD_BG)),
        None,
        identity(),
    ));

    // Inset dashed border. `lineWidth = max(1, min*.004)`; the dash pattern is
    // six widths on, five off (ref renderer.ts:8202-8207).
    let pad_px = 12.0f32.max(min_px * 0.04);
    let line_px = 1.0f32.max(min_px * 0.004);
    let line_emu = line_px * EMU_PER_PX;
    let inset_x = pad_px * EMU_PER_PX;
    let inset_y = pad_px * EMU_PER_PX;
    let border = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: inset_x, y: inset_y },
            PathSegment::LineTo {
                x: width - inset_x,
                y: inset_y,
            },
            PathSegment::LineTo {
                x: width - inset_x,
                y: height - inset_y,
            },
            PathSegment::LineTo {
                x: inset_x,
                y: height - inset_y,
            },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    commands.push(VectorCommand::draw_path(
        border,
        None,
        Some(VectorStroke {
            color: CARD_BORDER,
            width_emu: line_emu.round() as i64,
            dash_segments: Some(vec![line_px * 6.0, line_px * 5.0]),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            ..VectorStroke::default()
        }),
        identity(),
    ));

    // Warning glyph at (w/2, h*.34).
    commands.push(text_command(
        "\u{26a0}",
        18.0f32.max(min_px * 0.14),
        GLYPH_COLOR,
        false,
        w_px / 2.0,
        h_px * 0.34,
    ));

    // Heading at (w/2, h*.52). The reference asks for weight 600, i.e. bold.
    let title = match placeholder {
        Some(resolved) => resolved.title.clone(),
        None => format!("Slide {slide_number} could not be displayed"),
    };
    commands.push(text_command(
        &title,
        11.0f32.max(min_px * 0.045),
        HEAD_COLOR,
        true,
        w_px / 2.0,
        h_px * 0.52,
    ));

    // Detail: the part-tagged error, word-wrapped to `w - pad*4` and capped at
    // four lines so a long message never overruns the frame.
    let detail = match placeholder {
        Some(resolved) => resolved.detail.as_deref(),
        None => Some(message),
    };
    let Some(detail) = detail else {
        return commands;
    };
    let detail_px = 9.0f32.max(scaled(min_px, 0.028, 9.0));
    let line_height_px = detail_px * 1.35;
    let max_width_px = w_px - pad_px * 4.0;
    let mut baseline_px = h_px * 0.6 + line_height_px;
    for line in wrap_detail(detail, detail_px, max_width_px) {
        commands.push(text_command(
            &line,
            detail_px,
            DETAIL_COLOR,
            false,
            w_px / 2.0,
            baseline_px,
        ));
        baseline_px += line_height_px;
    }
    commands
}

/// Severe word wrap: break on whitespace once the candidate line exceeds
/// `max_width_px`, stop after [`MAX_DETAIL_LINES`] (ref renderer.ts:8243-8262).
fn wrap_detail(detail: &str, size_px: f32, max_width_px: f32) -> Vec<String> {
    let advance_px = size_px * ADVANCE_EM;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in detail.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        let candidate_px = candidate.chars().count() as f32 * advance_px;
        if candidate_px > max_width_px && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
        if lines.len() >= MAX_DETAIL_LINES {
            break;
        }
    }
    if !line.is_empty() && lines.len() < MAX_DETAIL_LINES {
        lines.push(line);
    }
    lines
}

/// One centred `fillText` line, in EMU (text-align centre, baseline middle).
fn text_command(
    text: &str,
    size_px: f32,
    color: [u8; 4],
    bold: bool,
    center_px: f32,
    baseline_px: f32,
) -> VectorCommand {
    // CSS px ≈ the application face's em box here; `font_size_pt` carries the
    // 0.75 pt-per-px conversion so the raster size matches the reference.
    let size_pt = size_px * PT_PER_PX;
    let size_emu = size_px * EMU_PER_PX;
    let advance = size_emu * ADVANCE_EM;
    let count = text.chars().count() as f32;
    let mut x = center_px * EMU_PER_PX - count * advance / 2.0;
    let y = baseline_px * EMU_PER_PX;
    let mut glyphs = Vec::with_capacity(text.chars().count());
    for (index, _) in text.chars().enumerate() {
        glyphs.push(PositionedGlyph {
            glyph_id: index as u16,
            x,
            y,
            advance,
            warp_u: None,
        });
        x += advance;
    }
    VectorCommand::DrawText {
        runs: vec![PositionedGlyphRun {
            text: text.to_string(),
            font_family: "sans-serif".to_string(),
            font_size_pt: size_pt,
            color,
            bold,
            italic: false,
            glyphs,
            decorations: Default::default(),
            single_face: false,
            face: None,
            warp: None,
            reflection: None,
        }],
        transform: identity(),
    }
}
