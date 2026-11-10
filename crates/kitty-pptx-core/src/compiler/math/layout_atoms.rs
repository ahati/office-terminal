//! Atomic math constructs: fractions, radicals, scripts and N-ary operators.
//!
//! Fraction geometry follows Knuth's Appendix G Rule 11 as restated in PRD 09
//! §3.4 (numerator/denominator shift clearance, axis-centred rule).

use ooxml_common::math::MathNode;

use super::box_model::{MathBox, MathMetrics, MathStyle};
use super::layout::{layout_nodes, LayoutCtx};

/// Build a big-operator box, clamping the em advance so display-size ∑/∫ do not
/// consume an excessive fraction of the line.
fn operator_box(op: &str, size_pt: f32, root_em: f32, color: [u8; 4]) -> MathBox {
    let text = if op.trim().is_empty() { "\u{222B}" } else { op };
    let mut boxed = MathBox::glyph_run(text, size_pt, color);
    boxed.width = boxed.width.min(root_em * 1.10);
    boxed
}

/// `m:f` — fraction with optional bar (PRD 09 §2.2, §3.4 Algorithm 1).
pub fn layout_fraction(
    num: &[MathNode],
    den: &[MathNode],
    has_bar: bool,
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let num_box = layout_nodes(num, style.numerator_style(), ctx, depth);
    let den_box = layout_nodes(den, style.denominator_style(), ctx, depth);

    let thickness = if has_bar { m.rule_thickness } else { 0.0 };
    let (shift_up, shift_down) = if style.is_display() {
        (m.frac_num_display_shift_up, m.frac_den_display_shift_down)
    } else {
        (m.frac_num_shift_up, m.frac_den_shift_down)
    };

    let num_shift = shift_up.max(m.axis_height + thickness * 0.5 + m.frac_num_gap + num_box.depth);
    let den_shift =
        shift_down.max(thickness * 0.5 + m.frac_den_gap + den_box.height - m.axis_height);

    let content_w = num_box.width.max(den_box.width);
    let total_w = content_w + 2.0 * m.frac_margin;
    let num_x = (total_w - num_box.width) * 0.5;
    let den_x = (total_w - den_box.width) * 0.5;

    let mut items = vec![(num_x, num_shift, num_box), (den_x, -den_shift, den_box)];
    if thickness > 0.0 {
        let axis_dy = m.axis_height - thickness * 0.5;
        items.push((0.0, axis_dy, MathBox::rule(total_w, thickness, ctx.color)));
    }
    MathBox::vbox(items)
}

/// `m:rad` — radical with optional degree, vinculum and stretched surd.
pub fn layout_radical(
    index: &[MathNode],
    radicand: &[MathNode],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let content = layout_nodes(radicand, style.cramped(), ctx, depth);

    let vinculum_top = content.height + m.radical_vertical_gap + m.radical_rule_thickness;
    let mut items: Vec<(f32, f32, MathBox)> = Vec::new();
    let mut x_cursor = 0.0f32;
    if !index.is_empty() {
        let degree_style = MathStyle {
            kind: super::box_model::MathStyleKind::ScriptScript,
            cramped: false,
        };
        let degree = layout_nodes(index, degree_style, ctx, depth);
        let top = vinculum_top + m.radical_extra_ascender;
        let degree_dy = top - degree.height;
        x_cursor = degree.width + m.radical_kern_before_degree;
        items.push((0.0, degree_dy, degree));
    }

    // Stretch the surd glyph so its tip reaches the vinculum.
    let surd_size = (vinculum_top / (0.72 * m.em) * m.size_pt)
        .max(m.size_pt)
        .min(m.size_pt * 3.0);
    let surd = MathBox::glyph_run("\u{221A}", surd_size, ctx.color);
    let surd_w = surd.width.max(m.em * 0.32);
    items.push((x_cursor, -content.depth * 0.5, surd));

    let rule_x = x_cursor + surd_w;
    let radicand_x = rule_x + m.em * 0.04;
    let rule_w = content.width + m.em * 0.08;
    items.push((
        rule_x,
        vinculum_top - m.radical_rule_thickness,
        MathBox::rule(rule_w, m.radical_rule_thickness, ctx.color),
    ));
    items.push((radicand_x, 0.0, content));
    MathBox::vbox(items)
}

/// `m:sSub`, `m:sSup`, `m:sSubSup` — optically scaled scripts.
pub fn layout_scripts(
    base: &[MathNode],
    sub: Option<&[MathNode]>,
    sup: Option<&[MathNode]>,
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let base_box = layout_nodes(base, style, ctx, depth);
    let script_style = style.sup_style();
    let sub_box = sub
        .filter(|nodes| !nodes.is_empty())
        .map(|nodes| layout_nodes(nodes, script_style, ctx, depth));
    let sup_box = sup
        .filter(|nodes| !nodes.is_empty())
        .map(|nodes| layout_nodes(nodes, script_style, ctx, depth));
    attach_scripts(base_box, sub_box, sup_box, style, ctx)
}

/// Attach already-laid-out scripts to a base box with Appendix G clearances.
pub fn attach_scripts(
    base: MathBox,
    sub: Option<MathBox>,
    sup: Option<MathBox>,
    style: MathStyle,
    ctx: &LayoutCtx,
) -> MathBox {
    let m = ctx.metrics(style);
    let base_right = base.width + m.em * 0.02;
    let mut up = if style.cramped {
        m.sup_shift_up_cramped
    } else {
        m.sup_shift_up
    }
    .max(base.height - m.sup_drop);
    let mut down = m.sub_shift_down.max(base.depth + m.sub_drop);

    if let (Some(sub), Some(sup)) = (&sub, &sup) {
        let gap = (up - sup.depth) - (sub.height - down);
        if gap < m.sub_sup_gap_min {
            let deficiency = (m.sub_sup_gap_min - gap) * 0.5;
            up += deficiency;
            down += deficiency;
        }
    }

    let mut items = vec![(0.0, 0.0, base)];
    if let Some(sup) = sup {
        items.push((base_right, up, sup));
    }
    if let Some(sub) = sub {
        items.push((base_right, -down, sub));
    }
    MathBox::vbox(items)
}

/// Operators whose limits are stacked under/over in display style by default.
fn is_under_over_operator(op: &str) -> bool {
    op.chars().any(|ch| {
        matches!(
            ch,
            '\u{2211}'
                | '\u{220F}'
                | '\u{2210}'
                | '\u{22C3}'
                | '\u{22C2}'
                | '\u{22C1}'
                | '\u{22C0}'
                | '\u{2A01}'
                | '\u{2A02}'
        )
    })
}

/// `m:nary` — big operator with limits (`undOvr`) or side scripts (`subSup`).
#[allow(clippy::too_many_arguments)]
pub fn layout_nary(
    op: &str,
    lim_loc: &str,
    sub: &[MathNode],
    sup: &[MathNode],
    body: &[MathNode],
    style: MathStyle,
    ctx: &mut LayoutCtx,
    depth: usize,
) -> MathBox {
    let m = ctx.metrics(style);
    let op_size = if style.is_display() {
        m.size_pt * m.big_operator_scale
    } else {
        m.size_pt
    };
    let op_box = operator_box(op, op_size, m.em, ctx.color);
    let op_box = if op_box.width <= 0.01 {
        operator_box("\u{222B}", op_size, m.em, ctx.color)
    } else {
        op_box
    };

    let script_style = style.sup_style();
    let sub_box = (!sub.is_empty()).then(|| layout_nodes(sub, script_style, ctx, depth));
    let sup_box = (!sup.is_empty()).then(|| layout_nodes(sup, script_style, ctx, depth));

    let loc = lim_loc.trim().to_ascii_lowercase();
    let under_over = loc == "undovr"
        || loc == "undover"
        || (loc.is_empty() && style.is_display() && is_under_over_operator(op));

    let with_limits = if under_over {
        stacked_limits(op_box, sub_box, sup_box, m)
    } else {
        attach_scripts(op_box, sub_box, sup_box, style, ctx)
    };

    let body_box = layout_nodes(body, style, ctx, depth);
    if body_box.width <= 0.0 && body_box.height <= 0.0 {
        return with_limits;
    }
    let thin = MathBox::space(m.thin_space);
    MathBox::hbox(vec![with_limits, thin, body_box])
}

/// Stack limits under/over an operator, centred on the operator's axis.
fn stacked_limits(
    op: MathBox,
    sub: Option<MathBox>,
    sup: Option<MathBox>,
    m: MathMetrics,
) -> MathBox {
    let wide = op
        .width
        .max(sub.as_ref().map_or(0.0, |b| b.width))
        .max(sup.as_ref().map_or(0.0, |b| b.width));
    let mut items = vec![((wide - op.width) * 0.5, 0.0, op)];
    if let Some(sup) = sup {
        let dy = items[0].2.height + m.upper_limit_gap_min + sup.depth;
        items.push(((wide - sup.width) * 0.5, dy, sup));
    }
    if let Some(sub) = sub {
        let dy = -(items[0].2.depth + m.lower_limit_gap_min + sub.height);
        items.push(((wide - sub.width) * 0.5, dy, sub));
    }
    MathBox::vbox(items)
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::*;
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

    #[test]
    fn fraction_has_rule_in_the_middle_and_both_parts() {
        with_ctx(|ctx| {
            let frac = layout_fraction(&[run("1")], &[run("x")], true, MathStyle::text(), ctx, 0);
            match &frac.kind {
                crate::compiler::math::box_model::MathBoxKind::VBox(items) => {
                    assert_eq!(items.len(), 3, "numerator + denominator + rule");
                    let rule = &items[2].child;
                    assert!(matches!(
                        rule.kind,
                        crate::compiler::math::box_model::MathBoxKind::Rule { .. }
                    ));
                    assert!(frac.height > 0.0 && frac.depth > 0.0);
                }
                _ => panic!("fraction must be a VBox"),
            }
        });
    }

    #[test]
    fn fraction_without_bar_has_no_rule() {
        with_ctx(|ctx| {
            let frac = layout_fraction(&[run("n")], &[run("k")], false, MathStyle::text(), ctx, 0);
            match &frac.kind {
                crate::compiler::math::box_model::MathBoxKind::VBox(items) => {
                    assert_eq!(items.len(), 2);
                }
                _ => panic!("fraction must be a VBox"),
            }
        });
    }

    #[test]
    fn superscript_is_optical_script_size_and_raised() {
        with_ctx(|ctx| {
            let base = MathBox::glyph_run("x", 18.0, [0, 0, 0, 255]);
            let sup = layout_nodes(&[run("2")], MathStyle::text().sup_style(), ctx, 0);
            let script_size = sup.height.max(sup.depth);
            assert!(script_size > 0.0);
            let composed = attach_scripts(base.clone(), None, Some(sup), MathStyle::text(), ctx);
            assert!(composed.height > base.height);
            assert!((composed.depth - base.depth).abs() < 0.01);
        });
    }

    #[test]
    fn subscript_and_superscript_clear_the_base() {
        with_ctx(|ctx| {
            let base = MathBox::glyph_run("X", 18.0, [0, 0, 0, 255]);
            let sub = layout_nodes(&[run("i")], MathStyle::text().sup_style(), ctx, 0);
            let sup = layout_nodes(&[run("2")], MathStyle::text().sup_style(), ctx, 0);
            let composed =
                attach_scripts(base.clone(), Some(sub), Some(sup), MathStyle::text(), ctx);
            assert!(composed.height > base.height, "superscript raised");
            assert!(composed.depth > base.depth, "subscript lowered");
            assert!(composed.width > base.width);
        });
    }

    #[test]
    fn radical_contains_a_vinculum_rule_and_stretched_surd() {
        with_ctx(|ctx| {
            let rad = layout_radical(&[], &[run("x")], MathStyle::text(), ctx, 0);
            match &rad.kind {
                crate::compiler::math::box_model::MathBoxKind::VBox(items) => {
                    let rules = items
                        .iter()
                        .filter(|placed| {
                            matches!(
                                placed.child.kind,
                                crate::compiler::math::box_model::MathBoxKind::Rule { .. }
                            )
                        })
                        .count();
                    let surds = items
                        .iter()
                        .filter(|placed| {
                            matches!(
                                &placed.child.kind,
                                crate::compiler::math::box_model::MathBoxKind::Glyphs(run)
                                    if run.text == "\u{221A}"
                            )
                        })
                        .count();
                    assert_eq!(rules, 1, "vinculum");
                    assert_eq!(surds, 1, "stretched surd");
                    assert_eq!(items.len(), 3, "surd + vinculum + radicand");
                }
                _ => panic!("radical must be a VBox"),
            }
        });
    }

    #[test]
    fn nary_under_over_stacks_limits_above_and_below() {
        with_ctx(|ctx| {
            let nary = layout_nary(
                "\u{2211}",
                "undOvr",
                &[run("i")],
                &[run("n")],
                &[run("x")],
                MathStyle::display(),
                ctx,
                0,
            );
            assert!(nary.width > 0.0);
            assert!(nary.height > 18.0 * 12700.0 * 0.5);
        });
    }

    #[test]
    fn text_width_is_monotone_in_length() {
        let em = 18.0 * 12700.0;
        assert!(
            super::super::box_model::text_width("ab", em)
                > super::super::box_model::text_width("a", em)
        );
    }
}
