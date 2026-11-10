//! Action button presets: a rounded button plate with a vector glyph cut-out.
//!
//! PRD 11 §2.6 describes a two-layer composition (beveled casing plus interior
//! icon). The `VectorPath` contract carries a single paint, so the casing is
//! emitted as the filled plate and the glyph subpaths are cut out with the
//! even-odd rule, which renders as a crisp terminal-friendly icon.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::util::{push_ellipse, push_polygon, push_round_rect, sample_arc};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Back,
    Forward,
    Beginning,
    End,
    Home,
    Help,
    Information,
    Movie,
    Sound,
    Blank,
    Document,
    Return,
}

pub fn build_button_preset(
    geometry: &str,
    w: f32,
    h: f32,
    _adj: &[Option<f64>],
) -> Option<VectorPath> {
    let glyph = match geometry {
        "actionButtonBackPrevious" => Glyph::Back,
        "actionButtonForwardNext" => Glyph::Forward,
        "actionButtonBeginning" => Glyph::Beginning,
        "actionButtonEnd" => Glyph::End,
        "actionButtonHome" => Glyph::Home,
        "actionButtonHelp" => Glyph::Help,
        "actionButtonInformation" => Glyph::Information,
        "actionButtonMovie" => Glyph::Movie,
        "actionButtonSound" => Glyph::Sound,
        "actionButtonBlank" => Glyph::Blank,
        "actionButtonDocument" => Glyph::Document,
        "actionButtonReturn" => Glyph::Return,
        _ => return None,
    };
    Some(button_path(w, h, glyph))
}

fn rect_pts(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<(f32, f32)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn button_path(w: f32, h: f32, glyph: Glyph) -> VectorPath {
    let mut segs = Vec::new();
    let radius = (0.14 * w.min(h)).max(0.0);
    push_round_rect(&mut segs, 0.0, 0.0, w, h, radius, true);

    let x0 = 0.25 * w;
    let y0 = 0.25 * h;
    let x1 = 0.75 * w;
    let y1 = 0.75 * h;
    let wg = (x1 - x0).max(0.0);
    let hg = (y1 - y0).max(0.0);
    // Glyph radii use the smaller inset axis so icons stay inside the bounds
    // for extreme aspect ratios.
    let g = wg.min(hg);
    let mx = w / 2.0;
    let my = h / 2.0;

    match glyph {
        Glyph::Back => push_polygon(&mut segs, &[(x1, y0), (x0, my), (x1, y1)]),
        Glyph::Forward => push_polygon(&mut segs, &[(x0, y0), (x1, my), (x0, y1)]),
        Glyph::Beginning => {
            push_polygon(&mut segs, &rect_pts(x0, y0, x0 + 0.15 * wg, y1));
            push_polygon(&mut segs, &[(x1, y0), (x0 + 0.25 * wg, my), (x1, y1)]);
        }
        Glyph::End => {
            push_polygon(&mut segs, &[(x0, y0), (x1 - 0.25 * wg, my), (x0, y1)]);
            push_polygon(&mut segs, &rect_pts(x1 - 0.15 * wg, y0, x1, y1));
        }
        Glyph::Home => {
            push_polygon(
                &mut segs,
                &[(mx, y0), (x1, y0 + 0.45 * hg), (x0, y0 + 0.45 * hg)],
            );
            push_polygon(
                &mut segs,
                &rect_pts(x0 + 0.15 * wg, y0 + 0.45 * hg, x1 - 0.15 * wg, y1),
            );
            push_polygon(
                &mut segs,
                &rect_pts(mx - 0.12 * wg, y1 - 0.30 * hg, mx + 0.12 * wg, y1),
            );
        }
        Glyph::Help => {
            push_arc_band(
                &mut segs,
                mx,
                y0 + 0.38 * hg,
                0.30 * g,
                0.16 * g,
                180.0,
                200.0,
            );
            push_polygon(
                &mut segs,
                &rect_pts(
                    mx - 0.06 * wg,
                    y0 + 0.55 * hg,
                    mx + 0.06 * wg,
                    y1 - 0.22 * hg,
                ),
            );
            push_ellipse(&mut segs, mx, y1 - 0.10 * hg, 0.08 * g, 0.08 * g, true);
        }
        Glyph::Information => {
            push_ellipse(&mut segs, mx, y0 + 0.15 * hg, 0.08 * g, 0.08 * g, true);
            push_polygon(
                &mut segs,
                &rect_pts(
                    mx - 0.05 * wg,
                    y0 + 0.34 * hg,
                    mx + 0.05 * wg,
                    y1 - 0.10 * hg,
                ),
            );
            push_polygon(
                &mut segs,
                &rect_pts(
                    mx - 0.12 * wg,
                    y0 + 0.26 * hg,
                    mx + 0.12 * wg,
                    y0 + 0.34 * hg,
                ),
            );
        }
        Glyph::Movie => {
            push_polygon(&mut segs, &rect_pts(x0, y0 + 0.28 * hg, x1 - 0.22 * wg, y1));
            push_ellipse(
                &mut segs,
                x0 + 0.18 * wg,
                y0 + 0.12 * hg,
                0.11 * g,
                0.11 * g,
                true,
            );
            push_ellipse(
                &mut segs,
                x0 + 0.48 * wg,
                y0 + 0.12 * hg,
                0.11 * g,
                0.11 * g,
                true,
            );
            push_polygon(
                &mut segs,
                &[
                    (x1 - 0.22 * wg, my - 0.15 * hg),
                    (x1, y0 + 0.22 * hg),
                    (x1, y1 - 0.22 * hg),
                    (x1 - 0.22 * wg, my + 0.15 * hg),
                ],
            );
        }
        Glyph::Sound => {
            push_polygon(
                &mut segs,
                &[
                    (x0, my - 0.18 * hg),
                    (x0 + 0.16 * wg, my - 0.18 * hg),
                    (x0 + 0.34 * wg, y0),
                    (x0 + 0.34 * wg, y1),
                    (x0 + 0.16 * wg, my + 0.18 * hg),
                    (x0, my + 0.18 * hg),
                ],
            );
            let sx = x0 + 0.34 * wg;
            push_arc_band(&mut segs, sx, my, 0.24 * g, 0.19 * g, -50.0, 100.0);
            push_arc_band(&mut segs, sx, my, 0.36 * g, 0.31 * g, -50.0, 100.0);
        }
        Glyph::Blank => {}
        Glyph::Document => {
            push_polygon(
                &mut segs,
                &[
                    (x0, y0),
                    (x1 - 0.18 * wg, y0),
                    (x1, y0 + 0.18 * hg),
                    (x1, y1),
                    (x0, y1),
                ],
            );
            for i in 0..3 {
                let ly = y0 + (0.40 + 0.16 * i as f32) * hg;
                push_polygon(
                    &mut segs,
                    &rect_pts(x0 + 0.14 * wg, ly, x1 - 0.14 * wg, ly + 0.06 * hg),
                );
            }
        }
        Glyph::Return => {
            push_polygon(
                &mut segs,
                &rect_pts(x0 + 0.30 * wg, my - 0.10 * hg, x1, my + 0.10 * hg),
            );
            push_polygon(
                &mut segs,
                &[
                    (x0 + 0.30 * wg, y0 + 0.15 * hg),
                    (x0, my),
                    (x0 + 0.30 * wg, y1 - 0.15 * hg),
                ],
            );
        }
    }

    VectorPath::new(segs, FillRule::EvenOdd)
}

/// Append a closed annular band (an arc "stroke") as a single subpath.
fn push_arc_band(
    segments: &mut Vec<PathSegment>,
    cx: f32,
    cy: f32,
    outer: f32,
    inner: f32,
    start_deg: f32,
    sweep_deg: f32,
) {
    let outer = outer.max(0.0);
    let inner = inner.clamp(0.0, outer);
    let mut points = sample_arc(cx, cy, outer, outer, start_deg, sweep_deg, 12);
    points.extend(sample_arc(
        cx,
        cy,
        inner,
        inner,
        start_deg + sweep_deg,
        -sweep_deg,
        12,
    ));
    push_polygon(segments, &points);
}
