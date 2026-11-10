//! Box-and-glue model for OMML typesetting (PRD 09 §3).
//!
//! Every math construct is compiled into a [`MathBox`] carrying a width, an
//! ascent (`height`) and a descent (`depth`) measured in EMU. The tree is
//! baseline-relative: `height` is above the box baseline, `depth` below it.
//! Emission ([`super::flatten_box`]) walks the tree and converts the y-up box
//! coordinates into the y-down screen coordinates used by `VectorCommand`.

use crate::contracts::{FillRule, PathSegment, VectorPath};

/// Font family used for typeset math glyphs. Cosmics-text/fontdb fall back to
/// an installed math font (STIX, Latin Modern, DejaVu) when unavailable.
pub const MATH_FONT_FAMILY: &str = "Cambria Math";

/// TeX math style quadrants (PRD 09 §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathStyleKind {
    Display,
    Text,
    Script,
    ScriptScript,
}

/// A math style: one of the four size quadrants plus the cramped flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathStyle {
    pub kind: MathStyleKind,
    pub cramped: bool,
}

impl MathStyle {
    pub const fn display() -> Self {
        Self {
            kind: MathStyleKind::Display,
            cramped: false,
        }
    }

    pub const fn text() -> Self {
        Self {
            kind: MathStyleKind::Text,
            cramped: false,
        }
    }

    pub const fn is_display(self) -> bool {
        matches!(self.kind, MathStyleKind::Display)
    }

    pub const fn cramped(self) -> Self {
        Self {
            cramped: true,
            ..self
        }
    }

    /// Optical size factor relative to the root em (PRD 09 §2.4).
    pub const fn size_factor(self) -> f32 {
        match self.kind {
            MathStyleKind::Display | MathStyleKind::Text => 1.0,
            MathStyleKind::Script => 0.70,
            MathStyleKind::ScriptScript => 0.50,
        }
    }

    /// Style of a superscript/subscript attached to this style.
    pub const fn sup_style(self) -> Self {
        let kind = match self.kind {
            MathStyleKind::Display | MathStyleKind::Text => MathStyleKind::Script,
            MathStyleKind::Script | MathStyleKind::ScriptScript => MathStyleKind::ScriptScript,
        };
        Self {
            kind,
            cramped: false,
        }
    }

    /// Numerator style (PRD 09 §3.2: display→text, text→script, …).
    pub const fn numerator_style(self) -> Self {
        let kind = match self.kind {
            MathStyleKind::Display => MathStyleKind::Text,
            MathStyleKind::Text | MathStyleKind::Script => MathStyleKind::Script,
            MathStyleKind::ScriptScript => MathStyleKind::ScriptScript,
        };
        Self {
            kind,
            cramped: false,
        }
    }

    /// Denominator style: cramped numerator style.
    pub const fn denominator_style(self) -> Self {
        self.numerator_style().cramped()
    }
}

/// OpenType-MATH-compatible metrics, in EMU, scaled per math style.
///
/// Real MATH-table access is not available from the current font stack, so the
/// PRD 09 §3.3 fallback proportions are used (1 em = `font_size_pt` × 12,700).
#[derive(Clone, Copy, Debug)]
pub struct MathMetrics {
    pub root_size_pt: f32,
    pub size_pt: f32,
    pub em: f32,
    pub axis_height: f32,
    pub rule_thickness: f32,
    pub frac_num_shift_up: f32,
    pub frac_num_display_shift_up: f32,
    pub frac_den_shift_down: f32,
    pub frac_den_display_shift_down: f32,
    pub frac_num_gap: f32,
    pub frac_den_gap: f32,
    pub frac_margin: f32,
    pub radical_vertical_gap: f32,
    pub radical_rule_thickness: f32,
    pub radical_extra_ascender: f32,
    pub radical_kern_before_degree: f32,
    pub sub_shift_down: f32,
    pub sup_shift_up: f32,
    pub sup_shift_up_cramped: f32,
    pub sub_drop: f32,
    pub sup_drop: f32,
    pub sub_sup_gap_min: f32,
    pub lower_limit_gap_min: f32,
    pub upper_limit_gap_min: f32,
    pub delimited_min_height: f32,
    pub delimiter_pad: f32,
    pub matrix_col_gap: f32,
    pub matrix_row_gap: f32,
    pub accent_gap: f32,
    pub bar_gap: f32,
    pub thin_space: f32,
    pub medium_space: f32,
    pub big_operator_scale: f32,
}

impl MathMetrics {
    /// Metrics at the root style (display or text).
    pub fn root(font_size_pt: f32) -> Self {
        let size = if font_size_pt.is_finite() && font_size_pt > 0.0 {
            font_size_pt.clamp(1.0, 400.0)
        } else {
            18.0
        };
        Self::derive(size, size)
    }

    /// Metrics for a nested style, scaled from the root size.
    pub fn for_style(self, style: MathStyle) -> Self {
        Self::derive(self.root_size_pt * style.size_factor(), self.root_size_pt)
    }

    fn derive(size_pt: f32, root_size_pt: f32) -> Self {
        let em = size_pt * 12700.0;
        Self {
            root_size_pt,
            size_pt,
            em,
            axis_height: em * 0.25,
            rule_thickness: em * 0.05,
            frac_num_shift_up: em * 0.45,
            frac_num_display_shift_up: em * 0.68,
            frac_den_shift_down: em * 0.40,
            frac_den_display_shift_down: em * 0.60,
            frac_num_gap: em * 0.15,
            frac_den_gap: em * 0.15,
            frac_margin: em * 0.12,
            radical_vertical_gap: em * 0.16,
            radical_rule_thickness: em * 0.05,
            radical_extra_ascender: em * 0.10,
            radical_kern_before_degree: em * 0.08,
            sub_shift_down: em * 0.22,
            sup_shift_up: em * 0.42,
            sup_shift_up_cramped: em * 0.36,
            sub_drop: em * 0.05,
            sup_drop: em * 0.386,
            sub_sup_gap_min: em * 0.12,
            lower_limit_gap_min: em * 0.12,
            upper_limit_gap_min: em * 0.12,
            delimited_min_height: em * 0.75,
            delimiter_pad: em * 0.06,
            matrix_col_gap: em * 0.55,
            matrix_row_gap: em * 0.35,
            accent_gap: em * 0.10,
            bar_gap: em * 0.08,
            thin_space: em * 0.1667,
            medium_space: em * 0.2222,
            big_operator_scale: 1.55,
        }
    }
}

/// A child box horizontally placed at `dx` from the row origin.
#[derive(Clone, Debug)]
pub struct Placed {
    pub dx: f32,
    pub child: MathBox,
}

/// A child box placed at `(dx, dy)`; `dy` is the child baseline offset (y-up).
#[derive(Clone, Debug)]
pub struct PlacedVert {
    pub dx: f32,
    pub dy: f32,
    pub child: MathBox,
}

/// A shaped run of math glyphs sharing one baseline.
#[derive(Clone, Debug)]
pub struct MathGlyphRun {
    pub text: String,
    pub font_family: &'static str,
    pub font_size_pt: f32,
    pub color: [u8; 4],
}

/// The geometry kind of a laid-out math box.
#[derive(Clone, Debug)]
pub enum MathBoxKind {
    /// Nothing to draw (also used for the zero-dimension fallback).
    Empty,
    /// Fixed horizontal spacing.
    Space,
    /// Baseline-aligned horizontal composition.
    HBox(Vec<Placed>),
    /// Vertical composition with explicit baseline offsets.
    VBox(Vec<PlacedVert>),
    /// A glyph run (emitted as `VectorCommand::DrawText`).
    Glyphs(MathGlyphRun),
    /// A solid rule of the given thickness (fraction bar, vinculum, …).
    Rule { thickness: f32, color: [u8; 4] },
    /// Pre-built y-down paths (delimiters, braces, borders), stroked with the
    /// given thickness.
    Paths {
        paths: Vec<VectorPath>,
        thickness: f32,
    },
}

/// A laid-out math box: width in EMU plus ascent (`height`) and descent (`depth`).
#[derive(Clone, Debug)]
pub struct MathBox {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub kind: MathBoxKind,
}

fn finite(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn extent(v: f32) -> f32 {
    finite(v).max(0.0)
}

impl MathBox {
    pub fn new(kind: MathBoxKind, width: f32, height: f32, depth: f32) -> Self {
        Self {
            width: extent(width),
            height: extent(height),
            depth: extent(depth),
            kind,
        }
    }

    pub fn empty() -> Self {
        Self::new(MathBoxKind::Empty, 0.0, 0.0, 0.0)
    }

    pub fn space(width: f32) -> Self {
        Self::new(MathBoxKind::Space, width, 0.0, 0.0)
    }

    /// A single glyph run sized by `font_size_pt`.
    pub fn glyph_run(text: &str, font_size_pt: f32, color: [u8; 4]) -> Self {
        if text.is_empty() {
            return Self::empty();
        }
        let size = if font_size_pt.is_finite() && font_size_pt > 0.0 {
            font_size_pt.clamp(1.0, 1000.0)
        } else {
            18.0
        };
        let em = size * 12700.0;
        Self::new(
            MathBoxKind::Glyphs(MathGlyphRun {
                text: text.to_string(),
                font_family: MATH_FONT_FAMILY,
                font_size_pt: size,
                color,
            }),
            text_width(text, em),
            text_ascent(text, em),
            text_descent(text, em),
        )
    }

    /// A solid rule. The box baseline is the rule's bottom edge.
    pub fn rule(width: f32, thickness: f32, color: [u8; 4]) -> Self {
        let t = extent(thickness);
        Self::new(
            MathBoxKind::Rule {
                thickness: t,
                color,
            },
            width,
            t,
            0.0,
        )
    }

    /// A pre-built y-down path set with explicit dimensions.
    pub fn paths(
        paths: Vec<VectorPath>,
        width: f32,
        height: f32,
        depth: f32,
        thickness: f32,
    ) -> Self {
        Self::new(
            MathBoxKind::Paths {
                paths,
                thickness: extent(thickness),
            },
            width,
            height,
            depth,
        )
    }

    /// Baseline-aligned horizontal composition.
    pub fn hbox(children: Vec<Self>) -> Self {
        let mut width = 0.0f32;
        let mut height = 0.0f32;
        let mut depth = 0.0f32;
        let mut placed = Vec::with_capacity(children.len());
        for child in children {
            width += child.width;
            height = height.max(child.height);
            depth = depth.max(child.depth);
            placed.push(Placed {
                dx: width - child.width,
                child,
            });
        }
        Self::new(MathBoxKind::HBox(placed), width, height, depth)
    }

    /// Vertical composition with explicit `(dx, dy)` placements (dy is y-up).
    pub fn vbox(items: Vec<(f32, f32, Self)>) -> Self {
        let mut width = 0.0f32;
        let mut height = 0.0f32;
        let mut depth = 0.0f32;
        let mut placed = Vec::with_capacity(items.len());
        for (dx, dy, child) in items {
            let dx = finite(dx);
            let dy = finite(dy);
            width = width.max(dx + child.width);
            height = height.max(dy + child.height);
            depth = depth.max(-dy + child.depth);
            placed.push(PlacedVert { dx, dy, child });
        }
        Self::new(MathBoxKind::VBox(placed), width, height, depth)
    }
}

/// A single horizontal/vertical rule path in y-down local coordinates.
pub fn rect_path(x: f32, y: f32, w: f32, h: f32) -> VectorPath {
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x, y },
            PathSegment::LineTo { x: x + w, y },
            PathSegment::LineTo { x: x + w, y: y + h },
            PathSegment::LineTo { x, y: y + h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    )
}

/// Approximate advance of a math glyph (fraction of an em).
pub fn glyph_width(ch: char, em: f32) -> f32 {
    let factor = match ch {
        ' ' | '\u{00A0}' => 0.25,
        'i' | 'l' | 'j' | '\'' | '|' => 0.22,
        'f' | 't' | 'r' => 0.33,
        'm' | 'w' | 'M' | 'W' => 0.78,
        ',' | '.' | ':' | ';' => 0.25,
        '(' | ')' | '[' | ']' => 0.35,
        '{' | '}' => 0.42,
        '/' | '\\' => 0.45,
        '+' | '-' | '\u{2212}' | '=' | '\u{00B1}' | '\u{2213}' | '\u{00D7}' | '\u{00F7}'
        | '\u{00B7}' | '\u{22C5}' => 0.62,
        '<' | '>' | '\u{2264}' | '\u{2265}' | '\u{2260}' | '\u{2248}' | '\u{2261}' | '\u{223C}'
        | '\u{2245}' | '\u{2192}' | '\u{2190}' | '\u{2194}' | '\u{21D2}' | '\u{21D4}' => 0.70,
        '\u{2211}' | '\u{220F}' | '\u{2210}' | '\u{222B}' | '\u{222C}' | '\u{222D}'
        | '\u{222E}' | '\u{22C3}' | '\u{22C2}' | '\u{22C1}' | '\u{22C0}' | '\u{2A01}'
        | '\u{2A02}' => 1.0,
        '\u{221A}' => 0.55,
        '\u{221E}' | '\u{2202}' | '\u{2207}' => 0.70,
        '\u{03B1}' | '\u{03B2}' | '\u{03B3}' | '\u{03B4}' | '\u{03B5}' | '\u{03B6}'
        | '\u{03B7}' | '\u{03B8}' | '\u{03B9}' | '\u{03BA}' | '\u{03BB}' | '\u{03BC}'
        | '\u{03BD}' | '\u{03BE}' | '\u{03C0}' | '\u{03C1}' | '\u{03C3}' | '\u{03C4}'
        | '\u{03C5}' | '\u{03C6}' | '\u{03C7}' | '\u{03C8}' | '\u{03C9}' => 0.55,
        _ if ch.is_ascii_digit() => 0.5,
        _ if ch.is_ascii_lowercase() => 0.5,
        _ if ch.is_ascii_uppercase() => 0.65,
        _ if (ch as u32) >= 0x2E80 => 1.0,
        _ => 0.6,
    };
    extent(factor * em)
}

/// Approximate ascent of a math glyph (fraction of an em).
pub fn glyph_ascent(ch: char, em: f32) -> f32 {
    let factor = match ch {
        'b' | 'd' | 'f' | 'h' | 'k' | 'l' | 't' => 0.70,
        'a' | 'c' | 'e' | 'g' | 'i' | 'm' | 'n' | 'o' | 'p' | 'q' | 'r' | 's' | 'u' | 'v' | 'w'
        | 'x' | 'y' | 'z' => 0.45,
        'A'..='Z' => 0.70,
        '0'..='9' => 0.65,
        '(' | ')' | '[' | ']' | '{' | '}' | '|' => 0.75,
        '\u{221A}' => 0.75,
        '\u{2211}' | '\u{220F}' | '\u{2210}' | '\u{22C3}' | '\u{22C2}' => 0.85,
        '\u{222B}' | '\u{222C}' | '\u{222D}' | '\u{222E}' => 0.90,
        '\u{221E}' => 0.46,
        ',' | '.' | ';' | ':' => 0.25,
        _ if ch.is_ascii_punctuation() => 0.5,
        _ => 0.6,
    };
    extent(factor * em)
}

/// Approximate descent of a math glyph (fraction of an em).
pub fn glyph_descent(ch: char, em: f32) -> f32 {
    let factor = match ch {
        'g' | 'j' | 'p' | 'q' | 'y' => 0.22,
        ',' | ';' => 0.15,
        '(' | ')' | '[' | ']' | '{' | '}' | '|' => 0.25,
        '\u{222B}' | '\u{222C}' | '\u{222D}' | '\u{222E}' => 0.30,
        '\u{221A}' => 0.05,
        _ => 0.0,
    };
    extent(factor * em)
}

pub fn text_width(text: &str, em: f32) -> f32 {
    text.chars().map(|c| glyph_width(c, em)).sum()
}

pub fn text_ascent(text: &str, em: f32) -> f32 {
    text.chars()
        .map(|c| glyph_ascent(c, em))
        .fold(0.0f32, f32::max)
}

pub fn text_descent(text: &str, em: f32) -> f32 {
    text.chars()
        .map(|c| glyph_descent(c, em))
        .fold(0.0f32, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hbox_is_additive_and_baseline_aligned() {
        let a = MathBox::glyph_run("x", 18.0, [0, 0, 0, 255]);
        let b = MathBox::glyph_run("2", 12.6, [0, 0, 0, 255]);
        let row = MathBox::hbox(vec![a.clone(), b.clone()]);
        assert!((row.width - (a.width + b.width)).abs() < 0.01);
        assert!((row.height - a.height.max(b.height)).abs() < 0.01);
        assert!((row.depth - a.depth.max(b.depth)).abs() < 0.01);
    }

    #[test]
    fn vbox_accounts_for_baseline_offsets() {
        let base = MathBox::glyph_run("x", 18.0, [0, 0, 0, 255]);
        let raised = MathBox::glyph_run("2", 12.6, [0, 0, 0, 255]);
        let dy = 5000.0;
        let stack = MathBox::vbox(vec![(0.0, 0.0, base.clone()), (0.0, dy, raised.clone())]);
        assert!((stack.height - (dy + raised.height)).abs() < 0.01);
        assert!((stack.depth - base.depth).abs() < 0.01);
    }

    #[test]
    fn style_quadrants_scale_and_transition() {
        assert!((MathStyle::text().size_factor() - 1.0).abs() < 1e-6);
        assert!((MathStyle::text().sup_style().size_factor() - 0.70).abs() < 1e-6);
        assert!((MathStyle::text().sup_style().sup_style().size_factor() - 0.50).abs() < 1e-6);
        assert_eq!(
            MathStyle::display().numerator_style().kind,
            MathStyleKind::Text
        );
        assert!(MathStyle::text().denominator_style().cramped);
    }

    #[test]
    fn metrics_scale_with_style_and_are_finite() {
        let root = MathMetrics::root(24.0);
        assert!((root.axis_height - 24.0 * 12700.0 * 0.25).abs() < 0.01);
        let script = root.for_style(MathStyle::text().sup_style());
        assert!((script.size_pt - 16.8).abs() < 0.01);
        assert!((script.rule_thickness - root.rule_thickness * 0.7).abs() < 0.01);
        let invalid = MathMetrics::root(f32::NAN);
        assert!(invalid.em.is_finite() && invalid.em > 0.0);
    }

    #[test]
    fn glyph_metrics_are_finite() {
        for ch in "x2=+∑∫√()∞π ,".chars() {
            let w = glyph_width(ch, 1000.0);
            assert!(w.is_finite() && w > 0.0, "width for {ch}");
            assert!(glyph_ascent(ch, 1000.0).is_finite());
            assert!(glyph_descent(ch, 1000.0).is_finite());
        }
        assert_eq!(text_width("", 1000.0), 0.0);
    }
}
