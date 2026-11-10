//! Recursive OMML → [`MathBox`] layout (PRD 09 §2–§3).
//!
//! The dispatcher owns the resource guards from PRD 09 §5.2 (nesting depth and
//! node budget): a breach latches a flag on the context, stops descending, and
//! lets the caller degrade the equation to its authored fallback text or to the
//! partially laid-out prefix plus an ellipsis.

use crossbeam_channel::Sender;

use crate::contracts::WarningEvent;
use ooxml_common::math::MathNode;

use super::box_model::{MathBox, MathBoxKind, MathMetrics, MathStyle};
use super::layout_atoms::{layout_fraction, layout_nary, layout_radical, layout_scripts};
use super::layout_structures::{
    layout_accent, layout_array, layout_bar, layout_border_box, layout_delimiter, layout_group_chr,
};

/// PRD 09 §5.2: maximum AST nesting depth accepted by the layout engine.
pub const MAX_MATH_DEPTH: usize = 64;
/// PRD 09 §5.2: maximum node count per equation block.
pub const MAX_MATH_NODES: usize = 2048;

/// Shared layout state: root metrics, paint, warning sink and resource budget.
pub struct LayoutCtx<'a> {
    pub metrics: MathMetrics,
    pub color: [u8; 4],
    pub sink: &'a Sender<WarningEvent>,
    budget: usize,
    /// Latched guard breach (PRD 09 §5.2 limit name).
    pub breach: Option<&'static str>,
}

impl<'a> LayoutCtx<'a> {
    pub fn new(font_size_pt: f32, color: [u8; 4], sink: &'a Sender<WarningEvent>) -> Self {
        Self {
            metrics: MathMetrics::root(font_size_pt),
            color,
            sink,
            budget: MAX_MATH_NODES,
            breach: None,
        }
    }

    pub fn metrics(&self, style: MathStyle) -> MathMetrics {
        self.metrics.for_style(style)
    }

    fn take_node(&mut self) -> bool {
        if self.budget == 0 {
            self.breach.get_or_insert("math_nodes");
            false
        } else {
            self.budget -= 1;
            true
        }
    }

    fn check_depth(&mut self, depth: usize) -> bool {
        if depth > MAX_MATH_DEPTH {
            self.breach.get_or_insert("math_depth");
            false
        } else {
            true
        }
    }
}

/// Lay out a node list as a baseline-aligned horizontal row.
pub fn layout_nodes(
    nodes: &[MathNode],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    if !ctx.check_depth(depth) {
        return MathBox::empty();
    }
    let mut children: Vec<MathBox> = Vec::new();
    let mut prev_last: Option<char> = None;
    for node in nodes {
        if !ctx.take_node() {
            break;
        }
        let first = edge_char(node, Edge::First);
        if let Some(space) = inter_node_space(prev_last, first, ctx.metrics(style)) {
            children.push(MathBox::space(space));
        }
        children.push(layout_node(node, style, ctx, depth));
        prev_last = edge_char(node, Edge::Last);
    }
    MathBox::hbox(children)
}

fn layout_node(node: &MathNode, style: MathStyle, ctx: &mut LayoutCtx, depth: usize) -> MathBox {
    let child_depth = depth + 1;
    match node {
        MathNode::Run { text, .. } => layout_run(text, style, ctx),
        MathNode::Fraction { num, den, bar } => {
            layout_fraction(num, den, bar.unwrap_or(true), style, ctx, child_depth)
        }
        MathNode::Radical { index, radicand } => {
            layout_radical(index, radicand, style, ctx, child_depth)
        }
        MathNode::Sup { base, sup } => {
            layout_scripts(base, None, Some(sup), style, ctx, child_depth)
        }
        MathNode::Sub { base, sub } => {
            layout_scripts(base, Some(sub), None, style, ctx, child_depth)
        }
        MathNode::SubSup { base, sub, sup } => {
            layout_scripts(base, Some(sub), Some(sup), style, ctx, child_depth)
        }
        MathNode::SPre { sub, sup, base } => {
            layout_pre_scripts(sub, sup, base, style, ctx, child_depth)
        }
        MathNode::Nary {
            op,
            lim_loc,
            sub,
            sup,
            body,
        } => layout_nary(op, lim_loc, sub, sup, body, style, ctx, child_depth),
        MathNode::Delimiter {
            beg_char,
            end_char,
            items,
        } => layout_delimiter(beg_char, end_char, items, style, ctx, child_depth),
        MathNode::Array { rows, align } => layout_array(rows, align, style, ctx, child_depth),
        MathNode::Accent { chr, base } => layout_accent(chr, base, style, ctx, child_depth),
        MathNode::Bar { pos, base } => layout_bar(pos, base, style, ctx, child_depth),
        MathNode::GroupChr { chr, pos, base } => {
            layout_group_chr(chr, pos, base, style, ctx, child_depth)
        }
        MathNode::Limit { base, lower, upper } => {
            layout_limit(base, lower, upper, style, ctx, child_depth)
        }
        MathNode::Func { name, arg } => {
            let name_box = layout_nodes(name, style, ctx, child_depth);
            let arg_box = layout_nodes(arg, style, ctx, child_depth);
            let thin = MathBox::space(ctx.metrics(style).thin_space);
            MathBox::hbox(vec![name_box, thin, arg_box])
        }
        MathNode::Group { items } => layout_nodes(items, style, ctx, child_depth),
        MathNode::Box { base } => layout_nodes(base, style, ctx, child_depth),
        MathNode::BorderBox {
            hide_top,
            hide_bot,
            hide_left,
            hide_right,
            strike_h,
            strike_v,
            strike_bltr,
            strike_tlbr,
            base,
        } => layout_border_box(
            base,
            super::paths::BorderEdges {
                hide_top: *hide_top,
                hide_bot: *hide_bot,
                hide_left: *hide_left,
                hide_right: *hide_right,
                strike_h: *strike_h,
                strike_v: *strike_v,
                strike_bltr: *strike_bltr,
                strike_tlbr: *strike_tlbr,
            },
            style,
            ctx,
            child_depth,
        ),
        MathNode::Phant {
            show,
            zero_wid,
            zero_asc,
            zero_desc,
            base,
        } => {
            let inner = layout_nodes(base, style, ctx, child_depth);
            let kind = if *show {
                inner.kind
            } else {
                MathBoxKind::Empty
            };
            MathBox::new(
                kind,
                if *zero_wid { 0.0 } else { inner.width },
                if *zero_asc { 0.0 } else { inner.height },
                if *zero_desc { 0.0 } else { inner.depth },
            )
        }
    }
}

fn layout_run(text: &str, style: MathStyle, ctx: &LayoutCtx) -> MathBox {
    let metrics = ctx.metrics(style);
    if text.trim().is_empty() {
        return MathBox::space(super::box_model::text_width(text, metrics.em));
    }
    MathBox::glyph_run(text, metrics.size_pt, ctx.color)
}

/// Pre-scripts (`m:sPre`) are laid out as a script column preceding the base.
fn layout_pre_scripts(
    sub: &[MathNode],
    sup: &[MathNode],
    base: &[MathNode],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let base_box = layout_nodes(base, style, ctx, depth);
    let sub_box = layout_nodes(sub, style.sup_style(), ctx, depth);
    let sup_box = layout_nodes(sup, style.sup_style(), ctx, depth);
    let stacked = stack_scripts(sub_box, sup_box, style, ctx);
    MathBox::hbox(vec![stacked, base_box])
}

fn layout_limit(
    base: &[MathNode],
    lower: &[MathNode],
    upper: &[MathNode],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let base_box = layout_nodes(base, style, ctx, depth);
    let lower_box = layout_nodes(lower, style.sup_style(), ctx, depth);
    let upper_box = layout_nodes(upper, style.sup_style(), ctx, depth);
    let wide = base_box.width.max(lower_box.width).max(upper_box.width);
    let base_w = base_box.width;
    let base_h = base_box.height;
    let base_d = base_box.depth;
    let mut items: Vec<(f32, f32, MathBox)> = vec![((wide - base_w) * 0.5, 0.0, base_box)];
    if lower_box.height + lower_box.depth > 0.0 {
        let dy = -(base_d + m.lower_limit_gap_min + lower_box.height);
        items.push(((wide - lower_box.width) * 0.5, dy, lower_box));
    }
    if upper_box.height + upper_box.depth > 0.0 {
        let dy = base_h + m.upper_limit_gap_min + upper_box.depth;
        items.push(((wide - upper_box.width) * 0.5, dy, upper_box));
    }
    MathBox::vbox(items)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    First,
    Last,
}

/// First/last literal character of a node, used for inter-atom spacing.
fn edge_char(node: &MathNode, edge: Edge) -> Option<char> {
    match node {
        MathNode::Run { text, .. } => match edge {
            Edge::First => text.chars().next(),
            Edge::Last => text.chars().next_back(),
        },
        MathNode::Group { items } | MathNode::Box { base: items } => match edge {
            Edge::First => items.first().and_then(|n| edge_char(n, Edge::First)),
            Edge::Last => items.last().and_then(|n| edge_char(n, Edge::Last)),
        },
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AtomClass {
    Relation,
    Binary,
    Punctuation,
}

fn atom_class(ch: char) -> Option<AtomClass> {
    match ch {
        '=' | '<' | '>' | '\u{2264}' | '\u{2265}' | '\u{2260}' | '\u{2248}' | '\u{2261}'
        | '\u{223C}' | '\u{2245}' | '\u{2192}' | '\u{2190}' | '\u{2194}' | '\u{21D2}'
        | '\u{21D4}' | '\u{2208}' | '\u{2209}' | '\u{2282}' | '\u{2283}' | '\u{2286}'
        | '\u{2287}' | '|' => Some(AtomClass::Relation),
        '+' | '-' | '\u{2212}' | '\u{00B1}' | '\u{2213}' | '\u{00D7}' | '\u{00F7}' | '\u{00B7}'
        | '\u{22C5}' | '\u{222A}' | '\u{2229}' | '\u{2295}' | '\u{2297}' => Some(AtomClass::Binary),
        ',' | ';' | ':' => Some(AtomClass::Punctuation),
        _ => None,
    }
}

fn inter_node_space(prev: Option<char>, next: Option<char>, m: MathMetrics) -> Option<f32> {
    let prev_class = prev.and_then(atom_class);
    let next_class = next.and_then(atom_class);
    if prev_class == Some(AtomClass::Relation) || next_class == Some(AtomClass::Relation) {
        return Some(m.medium_space);
    }
    if prev_class == Some(AtomClass::Binary) || next_class == Some(AtomClass::Binary) {
        return Some(m.thin_space);
    }
    if prev_class == Some(AtomClass::Punctuation) {
        return Some(m.thin_space);
    }
    None
}

/// Stack a sub/sup pair as a script column with anti-collision clearance.
pub fn stack_scripts(sub: MathBox, sup: MathBox, style: MathStyle, ctx: &LayoutCtx) -> MathBox {
    let m = ctx.metrics(style);
    let mut up = m.sup_shift_up.max(m.sub_sup_gap_min + sup.depth);
    let mut down = m.sub_shift_down.max(m.sub_sup_gap_min + sub.height);
    let gap = (up - sup.depth) - (sub.height - down);
    if gap < m.sub_sup_gap_min {
        let deficiency = (m.sub_sup_gap_min - gap) * 0.5;
        up += deficiency;
        down += deficiency;
    }
    let column_w = sup.width.max(sub.width);
    let sup_x = column_w - sup.width;
    let sub_x = column_w - sub.width;
    MathBox::vbox(vec![(sup_x, up, sup), (sub_x, -down, sub)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    fn ctx() -> LayoutCtx<'static> {
        // The channel is leaked for the test-only static context.
        let (tx, _rx) = unbounded();
        let sink: &'static Sender<WarningEvent> = Box::leak(Box::new(tx));
        LayoutCtx::new(18.0, [0, 0, 0, 255], sink)
    }

    fn run(text: &str) -> MathNode {
        MathNode::Run {
            text: text.to_string(),
            style: "italic".to_string(),
            script: None,
        }
    }

    #[test]
    fn empty_node_list_lays_out_to_nothing() {
        let mut ctx = ctx();
        let boxed = layout_nodes(&[], MathStyle::text(), &mut ctx, 0);
        assert_eq!(boxed.width, 0.0);
        assert_eq!(boxed.height, 0.0);
    }

    #[test]
    fn relation_adds_medium_spacing_between_runs() {
        let mut ctx = ctx();
        let row = layout_nodes(
            &[run("x"), run("="), run("1")],
            MathStyle::text(),
            &mut ctx,
            0,
        );
        let single = layout_nodes(&[run("x"), run("1")], MathStyle::text(), &mut ctx, 0);
        assert!(row.width > single.width);
    }

    #[test]
    fn depth_guard_latches_and_stops() {
        let mut ctx = ctx();
        let mut node = run("x");
        for _ in 0..(MAX_MATH_DEPTH + 8) {
            node = MathNode::Fraction {
                num: vec![node],
                den: Vec::new(),
                bar: None,
            };
        }
        let _ = layout_nodes(&[node], MathStyle::text(), &mut ctx, 0);
        assert_eq!(ctx.breach, Some("math_depth"));
    }
}
