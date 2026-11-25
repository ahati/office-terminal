//! Structured math constructs: delimiters, arrays, accents, bars, braces and
//! enclosure boxes (PRD 09 §2.5–§2.8).

use ooxml_common::math::MathNode;

use super::box_model::{MathBox, MathMetrics, MathStyle};
use super::layout::{layout_nodes, LayoutCtx};
use super::paths::{self, BorderEdges};

/// `m:d` — extensible delimiters around one or more comma-separated items.
pub fn layout_delimiter(
    beg_char: &str,
    end_char: &str,
    items: &[Vec<MathNode>],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let mut leaves: Vec<MathBox> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            leaves.push(MathBox::glyph_run(", ", m.size_pt, ctx.color));
        }
        leaves.push(layout_nodes(item, style, ctx, depth));
    }
    let content = MathBox::hbox(leaves);

    let inner_h = content.height + content.depth;
    let total_h = (inner_h + 2.0 * m.delimiter_pad).max(m.delimited_min_height);
    let axis = m.axis_height;
    let top = -(axis + total_h * 0.5);
    let bottom = -axis + total_h * 0.5;
    let width = (m.em * 0.26 + total_h * 0.04).clamp(m.em * 0.18, m.em * 0.7);
    let thickness = (m.rule_thickness * 1.1).max(total_h * 0.028);

    let content_dy = axis - (content.height - content.depth) * 0.5;
    let mut children: Vec<MathBox> = Vec::new();
    if let Some(left) = delimiter_part(beg_char, width, top, bottom, thickness, m, ctx) {
        children.push(left);
        children.push(MathBox::space(m.delimiter_pad));
    }
    children.push(MathBox::vbox(vec![(0.0, content_dy, content)]));
    if let Some(right) = delimiter_part(end_char, width, top, bottom, thickness, m, ctx) {
        children.push(MathBox::space(m.delimiter_pad));
        children.push(right);
    }
    MathBox::hbox(children)
}

fn delimiter_part(
    ch: &str,
    width: f32,
    top: f32,
    bottom: f32,
    thickness: f32,
    m: MathMetrics,
    ctx: &LayoutCtx,
) -> Option<MathBox> {
    if ch.trim().is_empty() {
        return None;
    }
    let height = (-top).max(0.0);
    let depth = bottom.max(0.0);
    if let Some(paths) = paths::delimiter_paths(ch, width, top, bottom) {
        return Some(MathBox::paths(paths, width, height, depth, thickness));
    }
    // Unknown delimiter: stretch a glyph to the required height.
    let required = (height + depth).max(m.delimited_min_height);
    let size = (required / (0.9 * m.em) * m.size_pt)
        .max(m.size_pt * 0.8)
        .min(m.size_pt * 3.0);
    let glyph = MathBox::glyph_run(ch.trim(), size, ctx.color);
    if glyph.width <= 0.0 {
        return None;
    }
    let dy = m.axis_height - size * 12700.0 * 0.35;
    Some(MathBox::vbox(vec![(0.0, dy, glyph)]))
}

/// `m:m` / `m:eqArr` — column-aligned grid of cells (PRD 09 §2.7).
pub fn layout_array(
    rows: &[Vec<Vec<MathNode>>],
    align: &str,
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let cells: Vec<Vec<MathBox>> = rows
        .iter()
        .map(|row| row.iter().map(|cell| layout_nodes(cell, style, ctx, depth)).collect())
        .collect();
    let ncols = cells.iter().map(Vec::len).max().unwrap_or(0);
    if ncols == 0 || cells.is_empty() {
        return MathBox::empty();
    }
    let mut col_w = vec![0.0f32; ncols];
    for row in &cells {
        for (i, cell) in row.iter().enumerate() {
            col_w[i] = col_w[i].max(cell.width);
        }
    }
    let centre = align.is_empty() || align == "center";
    let col_gap = if centre {
        m.matrix_col_gap
    } else {
        m.matrix_col_gap * 0.4
    };
    let mut col_x = Vec::with_capacity(ncols);
    let mut x = 0.0f32;
    for w in &col_w {
        col_x.push(x);
        x += w + col_gap;
    }

    let row_h: Vec<f32> = cells
        .iter()
        .map(|row| row.iter().map(|c| c.height).fold(0.0f32, f32::max))
        .collect();
    let row_d: Vec<f32> = cells
        .iter()
        .map(|row| row.iter().map(|c| c.depth).fold(0.0f32, f32::max))
        .collect();
    let total_h: f32 = row_h.iter().zip(&row_d).map(|(h, d)| h + d).sum::<f32>()
        + m.matrix_row_gap * (cells.len().saturating_sub(1)) as f32;
    let top = m.axis_height + total_h * 0.5;

    let mut items: Vec<(f32, f32, MathBox)> = Vec::new();
    let mut cursor = top;
    for (ri, row) in cells.into_iter().enumerate() {
        let baseline = cursor - row_h[ri];
        for (ci, cell) in row.into_iter().enumerate() {
            let dx = if centre {
                col_x[ci] + (col_w[ci] - cell.width) * 0.5
            } else if align == "eq" && ci == 0 {
                col_w[0] - cell.width
            } else {
                col_x[ci]
            };
            items.push((dx, baseline, cell));
        }
        cursor -= row_h[ri] + row_d[ri] + m.matrix_row_gap;
    }
    MathBox::vbox(items)
}

/// Accent mark classification (`m:acc` character).
enum AccentMark {
    Rule,
    Arrow,
    Glyph(char),
}

fn accent_mark(chr: &str) -> AccentMark {
    let Some(ch) = chr.chars().next() else {
        return AccentMark::Glyph('^');
    };
    match ch {
        '\u{0304}' | '\u{0305}' | '\u{00AF}' | '\u{203E}' | '_' | '\u{0332}' => AccentMark::Rule,
        '\u{20D7}' | '\u{2192}' => AccentMark::Arrow,
        '\u{0302}' | '^' => AccentMark::Glyph('^'),
        '\u{0303}' | '~' => AccentMark::Glyph('~'),
        '\u{0307}' | '\u{02D9}' => AccentMark::Glyph('\u{02D9}'),
        '\u{0308}' => AccentMark::Glyph('\u{00A8}'),
        '\u{0301}' => AccentMark::Glyph('\u{00B4}'),
        '\u{0300}' => AccentMark::Glyph('`'),
        '\u{030C}' => AccentMark::Glyph('\u{02C7}'),
        '\u{0306}' => AccentMark::Glyph('\u{02D8}'),
        _ if ('\u{0300}'..='\u{036F}').contains(&ch) => AccentMark::Glyph('^'),
        _ => AccentMark::Glyph(ch),
    }
}

/// `m:acc` — accent mark centred over the base box (PRD 09 §2.8).
pub fn layout_accent(chr: &str, base: &[MathNode], style: MathStyle, ctx: &mut LayoutCtx, depth: usize) -> MathBox {
    let m = ctx.metrics(style);
    let base_box = layout_nodes(base, style, ctx, depth);
    let mark = accent_mark(chr);
    let base_w = base_box.width;
    match mark {
        AccentMark::Rule => {
            let t = m.rule_thickness;
            let width = base_w.max(m.em * 0.4);
            let pad = ((width - base_w) * 0.5).max(0.0);
            let rule = MathBox::rule(width, t, ctx.color);
            let dy = base_box.height + m.accent_gap;
            MathBox::vbox(vec![(pad, 0.0, base_box), (0.0, dy, rule)])
        }
        AccentMark::Arrow => {
            let gap = m.accent_gap + m.em * 0.05;
            let top = -(base_box.height + gap);
            let width = base_w.max(m.em * 0.4);
            let pad = ((width - base_w) * 0.5).max(0.0);
            let path = paths::arrow_path(width, top, m.em * 0.07);
            let path_box = MathBox::paths(vec![path], width, (-top).max(0.0), base_box.depth, m.rule_thickness);
            MathBox::vbox(vec![(pad, 0.0, base_box), (0.0, 0.0, path_box)])
        }
        AccentMark::Glyph(ch) => {
            let accent = MathBox::glyph_run(&ch.to_string(), m.size_pt * 0.95, ctx.color);
            let pad = ((accent.width - base_w) * 0.5).max(0.0);
            let accent_x = pad + (base_w - accent.width) * 0.5;
            let dy = base_box.height + m.accent_gap + accent.depth;
            MathBox::vbox(vec![(pad, 0.0, base_box), (accent_x, dy, accent)])
        }
    }
}

/// `m:bar` — overline / underline rule.
pub fn layout_bar(pos: &str, base: &[MathNode], style: MathStyle, ctx: &mut LayoutCtx, depth: usize) -> MathBox {
    let m = ctx.metrics(style);
    let base_box = layout_nodes(base, style, ctx, depth);
    let t = m.rule_thickness;
    let width = base_box.width.max(m.em * 0.3);
    let rule = MathBox::rule(width, t, ctx.color);
    let dy = if pos == "bot" {
        -(base_box.depth + m.bar_gap + t)
    } else {
        base_box.height + m.bar_gap
    };
    MathBox::vbox(vec![(0.0, 0.0, base_box), (0.0, dy, rule)])
}

/// `m:groupChr` — horizontal over/under brace (⏞ / ⏟).
pub fn layout_group_chr(
    chr: &str,
    pos: &str,
    base: &[MathNode],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let base_box = layout_nodes(base, style, ctx, depth);
    let amplitude = m.em * 0.10;
    let width = base_box.width.max(m.em * 0.5);
    let pad = (width - base_box.width) * 0.5;
    let over = pos != "bot";
    let (y_arm, height, box_depth) = if over {
        (
            -(base_box.height + m.accent_gap + amplitude),
            base_box.height + m.accent_gap + 2.0 * amplitude,
            base_box.depth,
        )
    } else {
        (
            base_box.depth + m.accent_gap + amplitude,
            base_box.height,
            base_box.depth + m.accent_gap + 2.0 * amplitude,
        )
    };
    let path = paths::horizontal_brace(width, y_arm, amplitude, over);
    let path_box = MathBox::paths(vec![path], width, height, box_depth, m.rule_thickness);
    if chr.trim().is_empty() {
        return MathBox::vbox(vec![(pad, 0.0, base_box)]);
    }
    MathBox::vbox(vec![(pad, 0.0, base_box), (0.0, 0.0, path_box)])
}

/// `m:borderBox` — rectangular/partial enclosure around the base.
pub fn layout_border_box(
    base: &[MathNode],
    edges: BorderEdges,
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let base_box = layout_nodes(base, style, ctx, depth);
    let pad = m.em * 0.12;
    let width = base_box.width + 2.0 * pad;
    let top = -(base_box.height + pad);
    let bottom = base_box.depth + pad;
    let path_list = paths::border_box_paths(width, top, bottom, edges);
    let path_box = MathBox::paths(
        path_list,
        width,
        base_box.height + pad,
        base_box.depth + pad,
        m.rule_thickness,
    );
    MathBox::vbox(vec![(0.0, 0.0, path_box), (pad, 0.0, base_box)])
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::*;
    use crate::compiler::math::box_model::MathBoxKind;
    use crate::contracts::WarningEvent;

    fn with_ctx<R>(f: impl FnOnce(&mut LayoutCtx) -> R) -> R {
        let (tx, _rx) = unbounded::<WarningEvent>();
        let mut ctx = LayoutCtx::new(18.0, [0, 0, 0, 255], &tx);
        f(&mut ctx)
    }

    fn run(text: &str) -> MathNode {
        MathNode::Run {
            text: text.to_string(),
            style: "italic".to_string(),
            script: None,
        }
    }

    fn contains_paths(b: &MathBox) -> bool {
        match &b.kind {
            MathBoxKind::Paths { .. } => true,
            MathBoxKind::HBox(items) => items.iter().any(|p| contains_paths(&p.child)),
            MathBoxKind::VBox(items) => items.iter().any(|p| contains_paths(&p.child)),
            _ => false,
        }
    }

    #[test]
    fn delimiter_synthesizes_bracket_paths() {
        with_ctx(|ctx| {
            let d = layout_delimiter("(", ")", &[vec![run("x")]], MathStyle::text(), ctx, 0);
            assert!(contains_paths(&d));
            assert!(d.width > 0.0);
        });
    }

    #[test]
    fn delimiter_with_empty_ends_keeps_content() {
        with_ctx(|ctx| {
            let d = layout_delimiter("", "", &[vec![run("x")]], MathStyle::text(), ctx, 0);
            assert!(!contains_paths(&d));
            assert!(d.width > 0.0);
        });
    }

    #[test]
    fn matrix_columns_are_aligned_and_rows_stacked() {
        with_ctx(|ctx| {
            let rows = vec![
                vec![vec![run("a")], vec![run("b")]],
                vec![vec![run("c")], vec![run("d")]],
            ];
            let m = layout_array(&rows, "center", MathStyle::text(), ctx, 0);
            assert!(m.width > 0.0);
            assert!(m.height > 0.0);
        });
    }

    #[test]
    fn eq_array_right_aligns_first_cell() {
        with_ctx(|ctx| {
            let rows = vec![vec![vec![run("x+y")], vec![run("=z")]]];
            let m = layout_array(&rows, "eq", MathStyle::text(), ctx, 0);
            assert!(m.width > 0.0);
        });
    }

    #[test]
    fn accent_rule_and_glyph_are_placed_above_base() {
        with_ctx(|ctx| {
            let bar = layout_accent("\u{0304}", &[run("x")], MathStyle::text(), ctx, 0);
            assert!(bar.height > 0.0);
            let hat = layout_accent("\u{0302}", &[run("x")], MathStyle::text(), ctx, 0);
            assert!(hat.height > bar.depth);
        });
    }

    #[test]
    fn border_box_emits_four_edges_by_default() {
        with_ctx(|ctx| {
            let b = layout_border_box(&[run("x")], BorderEdges::default(), MathStyle::text(), ctx, 0);
            match &b.kind {
                MathBoxKind::VBox(items) => match &items[0].child.kind {
                    MathBoxKind::Paths { paths, .. } => assert_eq!(paths.len(), 4),
                    _ => panic!("expected path box"),
                },
                _ => panic!("border box must be a VBox"),
            }
        });
    }

    #[test]
    fn empty_array_is_empty_box() {
        with_ctx(|ctx| {
            let m = layout_array(&[], "center", MathStyle::text(), ctx, 0);
            assert_eq!(m.width, 0.0);
        });
    }
}
