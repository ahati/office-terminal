//! Parametric path synthesis for math geometry (PRD 09 §2.3, §2.6, §2.8).
//!
//! OpenType `MATH` glyph assemblies are not reachable from the current font
//! stack, so extensible delimiters, group-character braces and enclosures are
//! synthesized from path segments. All paths are expressed in y-down
//! baseline-relative math coordinates (the same space the emitter uses), so a
//! path box only needs a translation when flattened.

use crate::contracts::{FillRule, LineCap, LineJoin, PathSegment, VectorPath, VectorStroke};

fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> VectorPath {
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x: x1, y: y1 },
            PathSegment::LineTo { x: x2, y: y2 },
        ],
        FillRule::NonZero,
    )
}

fn polyline(points: &[(f32, f32)]) -> VectorPath {
    let mut segments = Vec::with_capacity(points.len() + 1);
    for (i, (x, y)) in points.iter().enumerate() {
        if i == 0 {
            segments.push(PathSegment::MoveTo { x: *x, y: *y });
        } else {
            segments.push(PathSegment::LineTo { x: *x, y: *y });
        }
    }
    VectorPath::new(segments, FillRule::NonZero)
}

fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)) -> VectorPath {
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x: p0.0, y: p0.1 },
            PathSegment::CubicTo {
                x1: p1.0,
                y1: p1.1,
                x2: p2.0,
                y2: p2.1,
                x: p3.0,
                y: p3.1,
            },
        ],
        FillRule::NonZero,
    )
}

/// Stroke descriptor matching a math rule's color and thickness.
pub fn stroke(color: [u8; 4], thickness: f32) -> VectorStroke {
    VectorStroke {
        color,
        width_emu: thickness.max(1.0) as i64,
        dash_segments: None,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
    }
}

/// Synthesize an extensible delimiter spanning `top..bottom` (y-down) inside a
/// box `width` wide. Returns `None` when the character has no parametric form
/// and must be drawn as a scaled glyph instead.
pub fn delimiter_paths(ch: &str, width: f32, top: f32, bottom: f32) -> Option<Vec<VectorPath>> {
    let w = width.max(1.0);
    let h = (bottom - top).max(1.0);
    let mid = (top + bottom) * 0.5;
    let ch = ch.trim();
    let paths = match ch {
        "" => return None,
        "(" => vec![cubic(
            (w, top),
            (0.0, top + h * 0.28),
            (0.0, bottom - h * 0.28),
            (w, bottom),
        )],
        ")" => vec![cubic(
            (0.0, top),
            (w, top + h * 0.28),
            (w, bottom - h * 0.28),
            (0.0, bottom),
        )],
        "[" => vec![polyline(&[
            (w, top),
            (0.0, top),
            (0.0, bottom),
            (w, bottom),
        ])],
        "]" => vec![polyline(&[
            (0.0, top),
            (w, top),
            (w, bottom),
            (0.0, bottom),
        ])],
        "{" => vec![polyline(&[
            (w, top),
            (w * 0.55, top + h * 0.10),
            (w * 0.42, mid - h * 0.06),
            (0.0, mid),
            (w * 0.42, mid + h * 0.06),
            (w * 0.55, bottom - h * 0.10),
            (w, bottom),
        ])],
        "}" => vec![polyline(&[
            (0.0, top),
            (w * 0.45, top + h * 0.10),
            (w * 0.58, mid - h * 0.06),
            (w, mid),
            (w * 0.58, mid + h * 0.06),
            (w * 0.45, bottom - h * 0.10),
            (0.0, bottom),
        ])],
        "|" | "\u{2223}" => vec![line(w * 0.5, top, w * 0.5, bottom)],
        "\u{2016}" | "\u{2225}" | "||" => vec![
            line(w * 0.28, top, w * 0.28, bottom),
            line(w * 0.72, top, w * 0.72, bottom),
        ],
        "<" | "\u{2329}" | "\u{27E8}" => {
            vec![polyline(&[(w, top), (0.0, mid), (w, bottom)])]
        }
        ">" | "\u{232A}" | "\u{27E9}" => {
            vec![polyline(&[(0.0, top), (w, mid), (0.0, bottom)])]
        }
        "\u{2308}" | "\u{23A1}" => {
            vec![polyline(&[(w, top), (0.0, top), (0.0, bottom)])]
        }
        "\u{2309}" | "\u{23A4}" => {
            vec![polyline(&[(0.0, top), (w, top), (w, bottom)])]
        }
        "\u{230A}" | "\u{23A3}" => {
            vec![polyline(&[(0.0, top), (0.0, bottom), (w, bottom)])]
        }
        "\u{230B}" | "\u{23A6}" => {
            vec![polyline(&[(w, top), (w, bottom), (0.0, bottom)])]
        }
        "/" | "\u{2215}" => vec![line(0.0, bottom, w, top)],
        "\\" | "\u{2216}" => vec![line(0.0, top, w, bottom)],
        _ => return None,
    };
    Some(paths)
}

/// Horizontal curly brace for `m:groupChr` (over/under braces).
///
/// `y_arm` is the y-down position of the brace arms; `up` points the middle
/// peak towards the base (over-brace) or away from it (under-brace).
pub fn horizontal_brace(width: f32, y_arm: f32, amplitude: f32, up: bool) -> VectorPath {
    let w = width.max(1.0);
    let a = if up {
        -amplitude.abs()
    } else {
        amplitude.abs()
    };
    let y = y_arm;
    polyline(&[
        (0.0, y),
        (w * 0.28, y),
        (w * 0.42, y + a),
        (w * 0.5, y + a * 0.35),
        (w * 0.58, y + a),
        (w * 0.72, y),
        (w, y),
    ])
}

/// Vector accent arrow (`m:acc` with `→` / U+20D7).
pub fn arrow_path(width: f32, y: f32, head: f32) -> VectorPath {
    let w = width.max(1.0);
    let h = head.max(1.0);
    polyline(&[(0.0, y), (w, y), (w - h, y - h), (w, y), (w - h, y + h)])
}

/// Enclosure edges/strikes for `m:borderBox`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BorderEdges {
    pub hide_top: bool,
    pub hide_bot: bool,
    pub hide_left: bool,
    pub hide_right: bool,
    pub strike_h: bool,
    pub strike_v: bool,
    pub strike_bltr: bool,
    pub strike_tlbr: bool,
}

/// Rectangle edges (as individual stroke paths) and optional strikes.
pub fn border_box_paths(width: f32, top: f32, bottom: f32, edges: BorderEdges) -> Vec<VectorPath> {
    let w = width.max(1.0);
    let mut paths = Vec::new();
    if !edges.hide_top {
        paths.push(line(0.0, top, w, top));
    }
    if !edges.hide_bot {
        paths.push(line(0.0, bottom, w, bottom));
    }
    if !edges.hide_left {
        paths.push(line(0.0, top, 0.0, bottom));
    }
    if !edges.hide_right {
        paths.push(line(w, top, w, bottom));
    }
    let mid = (top + bottom) * 0.5;
    if edges.strike_h {
        paths.push(line(0.0, mid, w, mid));
    }
    if edges.strike_v {
        paths.push(line(w * 0.5, top, w * 0.5, bottom));
    }
    if edges.strike_bltr {
        paths.push(line(0.0, bottom, w, top));
    }
    if edges.strike_tlbr {
        paths.push(line(0.0, top, w, bottom));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point_count(path: &VectorPath) -> usize {
        path.segments.len()
    }

    #[test]
    fn parentheses_and_brackets_are_synthesized() {
        for ch in ["(", ")", "[", "]", "{", "}", "|", "\u{2329}", "\u{2308}"] {
            let paths = delimiter_paths(ch, 4000.0, -9000.0, 9000.0);
            let paths = paths.unwrap_or_default();
            assert!(!paths.is_empty(), "delimiter {ch} must produce geometry");
            assert!(paths.iter().all(|p| point_count(p) >= 2));
        }
    }

    #[test]
    fn empty_and_unknown_delimiters_have_no_paths() {
        assert!(delimiter_paths("", 4000.0, -1000.0, 1000.0).is_none());
        assert!(delimiter_paths("\u{221A}", 4000.0, -1000.0, 1000.0).is_none());
    }

    #[test]
    fn border_box_hides_edges_and_adds_strikes() {
        let edges = BorderEdges {
            hide_top: true,
            hide_right: true,
            strike_tlbr: true,
            ..Default::default()
        };
        let paths = border_box_paths(10000.0, -5000.0, 5000.0, edges);
        // bottom + left + one diagonal strike
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn brace_and_arrow_paths_are_non_empty() {
        assert!(!horizontal_brace(10000.0, -4000.0, 800.0, true)
            .segments
            .is_empty());
        assert!(!arrow_path(10000.0, -5000.0, 600.0).segments.is_empty());
    }
}
