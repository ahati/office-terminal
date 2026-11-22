//! Stacked WordArt column placement (`layoutStackedText`).

use crate::contracts::RectF32;
use crate::text::measure::StyleId;

/// One measured stacked glyph, before column placement.
#[derive(Clone, Debug)]
pub struct StackedGlyph {
    pub text: String,
    pub style: StyleId,
    /// Advance along the column (the cell height for an upright glyph).
    pub advance: f32,
    /// Cross-axis glyph advance used to centre the ink in its cell.
    pub width: f32,
    /// Column thickness the glyph claims (its face cell).
    pub thickness: f32,
    pub ascent: f32,
    pub descent: f32,
    /// Clockwise glyph rotation in radians (ASCII brackets turn 90°).
    pub rotation: f32,
    /// Inline math piece index, if this is an equation cell.
    pub object: Option<usize>,
    /// Authored paragraph owning this glyph.
    pub paragraph: usize,
    /// An authored line break starts a new column before this glyph.
    pub break_before: bool,
}

/// A placed stacked glyph: left edge of the glyph box and its baseline.
#[derive(Clone, Debug)]
pub struct StackedSlot {
    pub text: String,
    pub style: StyleId,
    pub x: f32,
    pub baseline: f32,
    pub width: f32,
    pub thickness: f32,
    pub ascent: f32,
    pub descent: f32,
    pub advance: f32,
    pub rotation: f32,
    pub object: Option<usize>,
    pub paragraph: usize,
    /// Column index (0-based, visual LTR order before `rtl` mirroring).
    pub column: usize,
}

impl StackedSlot {
    fn new(glyph: &StackedGlyph, x: f32, baseline: f32, column: usize) -> Self {
        Self {
            text: glyph.text.clone(),
            style: glyph.style,
            x,
            baseline,
            width: glyph.width,
            thickness: glyph.thickness,
            ascent: glyph.ascent,
            descent: glyph.descent,
            advance: glyph.advance,
            rotation: glyph.rotation,
            object: glyph.object,
            paragraph: glyph.paragraph,
            column,
        }
    }
}

/// Column placement of `layoutStackedText` (`stacked.ts:137-260`).
///
/// Glyphs flow down one column until the text rectangle is full, then wrap to
/// the next column. `anchor` places the column block across the columns (`t` =
/// the start side: left for `wordArtVert`, right for `wordArtVertRtl`; `b` =
/// the end side; `ctr` = centred) and `anchor_ctr` centres the block's longest
/// column. `alignment` places each column along the vertical axis (`l` = top,
/// `ctr` = centre, `r`/the fill alignments = bottom).
pub fn place_stacked(
    glyphs: &[StackedGlyph],
    rect: RectF32,
    anchor: &str,
    alignment: &str,
    anchor_ctr: bool,
    rtl: bool,
) -> Vec<StackedSlot> {
    if glyphs.is_empty() {
        return Vec::new();
    }
    let mut columns: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0.0f32;
    for (index, glyph) in glyphs.iter().enumerate() {
        if !current.is_empty()
            && (glyph.break_before || (rect.h > 0.0 && used + glyph.advance > rect.h + 0.01))
        {
            columns.push(std::mem::take(&mut current));
            used = 0.0;
        }
        current.push(index);
        used += glyph.advance;
    }
    if !current.is_empty() {
        columns.push(current);
    }

    let column_thickness: Vec<f32> = columns
        .iter()
        .map(|members| {
            members
                .iter()
                .map(|index| glyphs[*index].thickness)
                .fold(0.0f32, f32::max)
        })
        .collect();
    let total: f32 = column_thickness.iter().sum();
    let block_left = if anchor_ctr {
        rect.x + (rect.w - total) * 0.5
    } else {
        match anchor {
            "b" => rect.right() - total,
            "ctr" => rect.x + (rect.w - total) * 0.5,
            _ => rect.x,
        }
    };

    let mut out = Vec::with_capacity(glyphs.len());
    let mut offset = 0.0f32;
    for (column_index, members) in columns.iter().enumerate() {
        let thickness = column_thickness[column_index];
        // `wordArtVertRtl` stacks the same reading-order columns right to left.
        let left = if rtl {
            block_left + total - offset - thickness
        } else {
            block_left + offset
        };
        let column_height: f32 = members.iter().map(|index| glyphs[*index].advance).sum();
        let mut y = match alignment {
            "ctr" => rect.y + (rect.h - column_height) * 0.5,
            "r" | "just" | "justLow" | "dist" | "thaiDist" => rect.y + rect.h - column_height,
            _ => rect.y,
        };
        for index in members {
            let glyph = &glyphs[*index];
            let baseline = y + glyph.ascent;
            let x = left + (thickness - glyph.width) * 0.5;
            out.push(StackedSlot::new(glyph, x, baseline, column_index));
            y += glyph.advance;
        }
        offset += thickness;
    }
    out
}
