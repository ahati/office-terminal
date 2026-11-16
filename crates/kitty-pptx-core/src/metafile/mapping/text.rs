//! Text runs → shaped `PositionedGlyphRun`s (PRD 13 §4.5).
//!
//! One engine `TextRun` may become several display-list runs: a `dx` override
//! positions every character independently, because both backends paint the
//! axis-aligned path from the run's first glyph origin. Alignment offsets the
//! anchor into a baseline origin, and GDI escapement / engine text transforms
//! become a rotation about that anchor.

use crate::contracts::{
    AffineTransform, DecorationLine, FillRule, PathSegment, PositionedGlyph, PositionedGlyphRun,
    RectF32, TextDecorations, VectorPath,
};

use crate::metafile::diagnostics::MetafileDiagnostic;
use crate::metafile::{MetafileTextRun, TextAlignH, TextAlignV};

/// Text emitted for one engine run, in element-local EMU.
pub struct MappedText {
    pub runs: Vec<PositionedGlyphRun>,
    /// Rotation about the run anchor, composed into `DrawText.transform`.
    pub transform: AffineTransform,
    /// `ETO_OPAQUE` background fill (`path`, straight-RGBA colour).
    pub background: Option<(VectorPath, [u8; 4])>,
    pub diagnostics: Vec<MetafileDiagnostic>,
}

/// Shapes one engine text run into element-local EMU.
pub fn map_text_run(run: &MetafileTextRun, unit_to_emu: &AffineTransform) -> Option<MappedText> {
    if run.text.is_empty() && run.glyph_indices.as_ref().is_none_or(|g| g.is_empty()) {
        return None;
    }
    // EMF+ positions text in page-transformed coordinates and carries the
    // world/container affine separately, so the complete local→EMU mapping is
    // `unit_to_emu ∘ run.transform` (PRD 13 §4.5).
    let local_to_emu = unit_to_emu.post_concat(&run.transform);
    let scale_x = (local_to_emu.sx * local_to_emu.sx + local_to_emu.ky * local_to_emu.ky)
        .sqrt()
        .max(1.0e-6);
    let scale_y = (local_to_emu.kx * local_to_emu.kx + local_to_emu.sy * local_to_emu.sy)
        .sqrt()
        .max(1.0e-6);
    if !local_to_emu.sx.is_finite()
        || !local_to_emu.sy.is_finite()
        || !local_to_emu.ky.is_finite()
        || !local_to_emu.kx.is_finite()
    {
        return None;
    }

    let mut diagnostics = Vec::new();
    let family = resolve_family(&run.family, &mut diagnostics);
    let mut text = run.text.clone();

    if let Some(ref glyph_ids) = run.glyph_indices {
        if !glyph_ids.is_empty() {
            let fallback_chars: Vec<char> = text.chars().collect();
            let (mapped_str, mapped_any) = crate::text::cmap::reverse_map_glyphs(
                &family,
                run.bold,
                run.italic,
                glyph_ids,
                if fallback_chars.is_empty() {
                    None
                } else {
                    Some(&fallback_chars)
                },
            );
            diagnostics.push(MetafileDiagnostic::degraded(
                "metafile_glyph_index_text",
                if mapped_any {
                    "glyph-indexed text mapped via font cmap"
                } else {
                    "glyph-indexed text mapped via glyph index fallback"
                },
            ));
            text = mapped_str;
        }
    }
    if text.is_empty() {
        return None;
    }
    if run.right_to_left {
        // The display list has no bidi base-direction control. RTL scripts are
        // already reordered by the shaper, so only an explicit RTL flag on
        // LTR text needs the character-order approximation.
        if !has_rtl_script(&text) {
            text = text.chars().rev().collect();
        }
        diagnostics.push(MetafileDiagnostic::degraded(
            "metafile_record_ignored",
            "right-to-left text layout approximated",
        ));
    }

    let size_emu = clamp_font_size(run.size, scale_y, &mut diagnostics);
    // The shared line model uses a 1.2-em box (`text::metrics::natural_line_box`),
    // so the ascent is the face share of that box, not of the em.
    let line_height = size_emu * 1.2;
    let ascent = crate::text::shape::ascent_share_for(&family, run.bold, run.italic) * line_height;

    let (anchor_x, anchor_y) = local_to_emu.transform_point(run.x, run.y);
    // Advances run along the transformed x-axis; glyph outlines are sized by
    // the transformed y-axis, so anisotropic scales are folded into the x
    // placement instead of the glyph box.
    let advance_ratio = scale_x / scale_y;
    let chars: Vec<char> = text.chars().collect();
    let dx_override = !run.dx.is_empty() && run.dx.len() == chars.len();
    if !run.dx.is_empty() && !dx_override {
        diagnostics.push(MetafileDiagnostic::degraded(
            "metafile_text_dx",
            "character spacing length does not match the character count",
        ));
    }

    let (mut runs, advance) = if dx_override {
        shaping_with_dx(
            &chars,
            &run.dx,
            &family,
            run,
            Placement {
                size_emu,
                advance_ratio,
                scale_x,
                anchor: (anchor_x, anchor_y),
            },
        )
    } else {
        let shaped = crate::text::shape::shape(&text, &family, size_emu, run.bold, run.italic);
        let glyphs = shaped
            .glyphs
            .iter()
            .map(|(glyph_id, glyph_x, advance)| PositionedGlyph {
                glyph_id: *glyph_id,
                x: anchor_x + *glyph_x * advance_ratio,
                y: anchor_y,
                advance: *advance * advance_ratio,
                warp_u: None,
            })
            .collect::<Vec<_>>();
        let advance = if shaped.width > 0.0 {
            shaped.width * advance_ratio
        } else {
            size_emu * 0.6 * chars.len() as f32 * advance_ratio
        };
        (
            vec![run_with_glyphs(&text, &family, size_emu, run, glyphs)],
            advance,
        )
    };

    if let Some(ref glyph_ids) = run.glyph_indices {
        if runs.len() == 1 && runs[0].glyphs.len() == glyph_ids.len() {
            for (g, &gid) in runs[0].glyphs.iter_mut().zip(glyph_ids.iter()) {
                if gid != 0 {
                    g.glyph_id = gid;
                }
            }
        } else if runs.len() == glyph_ids.len() {
            for (r, &gid) in runs.iter_mut().zip(glyph_ids.iter()) {
                if gid != 0 {
                    if let Some(g) = r.glyphs.first_mut() {
                        g.glyph_id = gid;
                    }
                }
            }
        }
    }

    let origin_x = match run.horizontal_align {
        TextAlignH::Left => anchor_x,
        TextAlignH::Center => anchor_x - advance * 0.5,
        TextAlignH::Right => anchor_x - advance,
    };
    let baseline_y = match run.vertical_align {
        TextAlignV::Top => anchor_y + ascent,
        TextAlignV::Center => anchor_y + ascent - line_height * 0.5,
        TextAlignV::Bottom => anchor_y - (line_height - ascent),
        TextAlignV::Baseline => anchor_y,
    };

    // Shift every run from the anchor to the resolved baseline origin.
    let shift = AffineTransform::translate(origin_x - anchor_x, baseline_y - anchor_y);
    let runs = runs
        .into_iter()
        .map(|shaped| PositionedGlyphRun {
            glyphs: shaped
                .glyphs
                .iter()
                .map(|glyph| {
                    let (x, y) = shift.transform_point(glyph.x, glyph.y);
                    PositionedGlyph { x, y, ..*glyph }
                })
                .collect(),
            ..shaped
        })
        .collect::<Vec<_>>();

    Some(MappedText {
        runs,
        transform: rotation_about(
            anchor_x,
            anchor_y,
            local_to_emu.ky.atan2(local_to_emu.sx).to_degrees() - run.escapement_deg,
        ),
        background: run
            .background
            .zip(run.background_rect)
            .and_then(|(color, rect)| background(rect, color, unit_to_emu)),
        diagnostics,
    })
}

/// Clamps the engine height (output units) to the display-list font range.
fn clamp_font_size(size: f32, emu_per_unit: f32, diagnostics: &mut Vec<MetafileDiagnostic>) -> f32 {
    let raw_emu = size.abs() * emu_per_unit;
    let pt = (raw_emu / 12700.0).clamp(0.5, 400.0);
    let clamped = pt * 12700.0;
    if !raw_emu.is_finite() || (clamped - raw_emu).abs() > 1.0 {
        diagnostics.push(MetafileDiagnostic::degraded(
            "metafile_text_size",
            "text size clamped to the supported range",
        ));
    }
    clamped.max(1.0)
}

/// Whether the text already contains a right-to-left script, which the shaper
/// reorders on its own.
fn has_rtl_script(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch as u32,
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF
        )
    })
}

fn resolve_family(requested: &str, diagnostics: &mut Vec<MetafileDiagnostic>) -> String {
    match crate::compiler::text::family_substitute(requested) {
        Some(substitute) if !substitute.eq_ignore_ascii_case(requested) => {
            diagnostics.push(MetafileDiagnostic::font_substituted(requested, substitute));
            substitute.to_string()
        }
        _ => requested.to_string(),
    }
}

/// Shared placement inputs for the per-character `dx` path.
#[derive(Clone, Copy)]
struct Placement {
    size_emu: f32,
    advance_ratio: f32,
    scale_x: f32,
    anchor: (f32, f32),
}

/// One `PositionedGlyphRun` per character, positioned by the `dx` advances.
fn shaping_with_dx(
    chars: &[char],
    dx: &[f32],
    family: &str,
    source: &MetafileTextRun,
    placement: Placement,
) -> (Vec<PositionedGlyphRun>, f32) {
    let mut runs = Vec::with_capacity(chars.len());
    let mut cursor = 0.0f32;
    for (ch, advance) in chars.iter().zip(dx.iter()) {
        let text = ch.to_string();
        let shaped = crate::text::shape::shape(
            &text,
            family,
            placement.size_emu,
            source.bold,
            source.italic,
        );
        // The first glyph sits at the cursor; a combining mark or an
        // unshapeable character still gets a positioned (empty) origin so the
        // character stream stays aligned with the advances.
        let glyphs = shaped
            .glyphs
            .iter()
            .map(|(glyph_id, glyph_x, glyph_advance)| PositionedGlyph {
                glyph_id: *glyph_id,
                x: placement.anchor.0 + cursor + *glyph_x * placement.advance_ratio,
                y: placement.anchor.1,
                advance: *glyph_advance * placement.advance_ratio,
                warp_u: None,
            })
            .collect();
        runs.push(run_with_glyphs(
            &text,
            family,
            placement.size_emu,
            source,
            glyphs,
        ));
        cursor += advance * placement.scale_x;
    }
    (runs, cursor)
}

fn run_with_glyphs(
    text: &str,
    family: &str,
    size_emu: f32,
    source: &MetafileTextRun,
    glyphs: Vec<PositionedGlyph>,
) -> PositionedGlyphRun {
    PositionedGlyphRun {
        text: text.to_string(),
        font_family: family.to_string(),
        font_size_pt: size_emu / 12700.0,
        color: source.color,
        bold: source.bold,
        italic: source.italic,
        glyphs,
        decorations: decorations_for(source, size_emu),
        warp: None,
        reflection: None,
    }
}

fn decorations_for(run: &MetafileTextRun, size_emu: f32) -> TextDecorations {
    TextDecorations {
        underline: run.underline.then(|| DecorationLine {
            color: run.color,
            top_emu: size_emu * 0.12,
            thickness_emu: (size_emu * 0.06).max(1.0),
            gap_emu: size_emu * 0.04,
            style: Default::default(),
        }),
        strike: run.strike_out.then(|| DecorationLine {
            color: run.color,
            top_emu: -size_emu * 0.28,
            thickness_emu: (size_emu * 0.06).max(1.0),
            gap_emu: 0.0,
            style: Default::default(),
        }),
    }
}

/// Rotation by `degrees` clockwise (screen y-down) about `(x, y)`.
fn rotation_about(x: f32, y: f32, degrees: f32) -> AffineTransform {
    if !degrees.is_finite() || degrees.abs() < 1.0e-3 {
        return AffineTransform::identity();
    }
    AffineTransform::translate(x, y)
        .post_concat(&AffineTransform::from_rotation(degrees.to_radians()))
        .post_concat(&AffineTransform::translate(-x, -y))
}

fn background(
    rect: RectF32,
    color: [u8; 4],
    unit_to_emu: &AffineTransform,
) -> Option<(VectorPath, [u8; 4])> {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return None;
    }
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo {
                x: rect.x,
                y: rect.y,
            },
            PathSegment::LineTo {
                x: rect.x + rect.w,
                y: rect.y,
            },
            PathSegment::LineTo {
                x: rect.x + rect.w,
                y: rect.y + rect.h,
            },
            PathSegment::LineTo {
                x: rect.x,
                y: rect.y + rect.h,
            },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    Some((super::transform_path(&path, unit_to_emu), color))
}
