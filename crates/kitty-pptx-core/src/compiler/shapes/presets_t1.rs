use crate::contracts::{FillRule, PathSegment, VectorPath};
use super::arc::decompose_arc_to_cubic;

pub fn build_t1_preset(
    name: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) -> Option<VectorPath> {
    let get_adj = |idx: usize, default: f32| -> f32 {
        adj.get(idx)
            .and_then(|&val| val)
            .map(|val| (val as f32) / 100000.0)
            .unwrap_or(default)
    };

    match name {
        "rect" | "flowChartProcess" => {
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: w, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "roundRect" => {
            let a1 = get_adj(0, 0.16667).clamp(0.0, 0.5);
            let r = w.min(h) * a1;
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: r, y: 0.0 });
            segs.push(PathSegment::LineTo { x: w - r, y: 0.0 });
            decompose_arc_to_cubic(w - r, r, r, r, 270.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w, y: h - r });
            decompose_arc_to_cubic(w - r, h - r, r, r, 0.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: r, y: h });
            decompose_arc_to_cubic(r, h - r, r, r, 90.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: 0.0, y: r });
            decompose_arc_to_cubic(r, r, r, r, 180.0, 90.0, &mut segs);
            segs.push(PathSegment::Close);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "ellipse" => {
            let rx = w / 2.0;
            let ry = h / 2.0;
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: w, y: ry });
            decompose_arc_to_cubic(rx, ry, rx, ry, 0.0, 360.0, &mut segs);
            segs.push(PathSegment::Close);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "triangle" | "isoscelesTriangle" => {
            let a1 = get_adj(0, 0.5).clamp(0.0, 1.0);
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: w * a1, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "diamond" | "flowChartDecision" => {
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: w / 2.0, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h / 2.0 },
                    PathSegment::LineTo { x: w / 2.0, y: h },
                    PathSegment::LineTo { x: 0.0, y: h / 2.0 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "parallelogram" | "flowChartInputOutput" => {
            let a1 = get_adj(0, 0.25).clamp(0.0, 1.0);
            let dx = w * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: dx, y: 0.0 },
                    PathSegment::LineTo { x: w, y: 0.0 },
                    PathSegment::LineTo { x: w - dx, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "trapezoid" => {
            let a1 = get_adj(0, 0.25).clamp(0.0, 0.5);
            let dx = w * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: dx, y: 0.0 },
                    PathSegment::LineTo { x: w - dx, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "pentagon" => {
            let cx = w / 2.0;
            let cy = h / 2.0;
            let rx = w / 2.0;
            let ry = h / 2.0;
            let mut segs = Vec::new();
            for i in 0..5 {
                let ang = -std::f32::consts::FRAC_PI_2 + (i as f32) * (2.0 * std::f32::consts::PI / 5.0);
                let x = cx + rx * ang.cos();
                let y = cy + ry * ang.sin();
                if i == 0 {
                    segs.push(PathSegment::MoveTo { x, y });
                } else {
                    segs.push(PathSegment::LineTo { x, y });
                }
            }
            segs.push(PathSegment::Close);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "hexagon" => {
            let a1 = get_adj(0, 0.25).clamp(0.0, 0.5);
            let dx = w * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: dx, y: 0.0 },
                    PathSegment::LineTo { x: w - dx, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h / 2.0 },
                    PathSegment::LineTo { x: w - dx, y: h },
                    PathSegment::LineTo { x: dx, y: h },
                    PathSegment::LineTo { x: 0.0, y: h / 2.0 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "octagon" => {
            let a1 = get_adj(0, 0.29289).clamp(0.0, 0.5);
            let d = w.min(h) * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: d, y: 0.0 },
                    PathSegment::LineTo { x: w - d, y: 0.0 },
                    PathSegment::LineTo { x: w, y: d },
                    PathSegment::LineTo { x: w, y: h - d },
                    PathSegment::LineTo { x: w - d, y: h },
                    PathSegment::LineTo { x: d, y: h },
                    PathSegment::LineTo { x: 0.0, y: h - d },
                    PathSegment::LineTo { x: 0.0, y: d },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "rightArrow" => {
            let a1 = get_adj(0, 0.5).clamp(0.0, 1.0); // head length
            let a2 = get_adj(1, 0.5).clamp(0.0, 1.0); // shaft height
            let y1 = h * (1.0 - a2) / 2.0;
            let y2 = h * (1.0 + a2) / 2.0;
            let x_head = w * (1.0 - a1);
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: y1 },
                    PathSegment::LineTo { x: x_head, y: y1 },
                    PathSegment::LineTo { x: x_head, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h / 2.0 },
                    PathSegment::LineTo { x: x_head, y: h },
                    PathSegment::LineTo { x: x_head, y: y2 },
                    PathSegment::LineTo { x: 0.0, y: y2 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "leftArrow" => {
            let a1 = get_adj(0, 0.5).clamp(0.0, 1.0);
            let a2 = get_adj(1, 0.5).clamp(0.0, 1.0);
            let y1 = h * (1.0 - a2) / 2.0;
            let y2 = h * (1.0 + a2) / 2.0;
            let x_head = w * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: w, y: y1 },
                    PathSegment::LineTo { x: x_head, y: y1 },
                    PathSegment::LineTo { x: x_head, y: 0.0 },
                    PathSegment::LineTo { x: 0.0, y: h / 2.0 },
                    PathSegment::LineTo { x: x_head, y: h },
                    PathSegment::LineTo { x: x_head, y: y2 },
                    PathSegment::LineTo { x: w, y: y2 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "upArrow" => {
            let a1 = get_adj(0, 0.5).clamp(0.0, 1.0);
            let a2 = get_adj(1, 0.5).clamp(0.0, 1.0);
            let x1 = w * (1.0 - a2) / 2.0;
            let x2 = w * (1.0 + a2) / 2.0;
            let y_head = h * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: x1, y: h },
                    PathSegment::LineTo { x: x1, y: y_head },
                    PathSegment::LineTo { x: 0.0, y: y_head },
                    PathSegment::LineTo { x: w / 2.0, y: 0.0 },
                    PathSegment::LineTo { x: w, y: y_head },
                    PathSegment::LineTo { x: x2, y: y_head },
                    PathSegment::LineTo { x: x2, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "downArrow" => {
            let a1 = get_adj(0, 0.5).clamp(0.0, 1.0);
            let a2 = get_adj(1, 0.5).clamp(0.0, 1.0);
            let x1 = w * (1.0 - a2) / 2.0;
            let x2 = w * (1.0 + a2) / 2.0;
            let y_head = h * (1.0 - a1);
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: x1, y: 0.0 },
                    PathSegment::LineTo { x: x2, y: 0.0 },
                    PathSegment::LineTo { x: x2, y: y_head },
                    PathSegment::LineTo { x: w, y: y_head },
                    PathSegment::LineTo { x: w / 2.0, y: h },
                    PathSegment::LineTo { x: 0.0, y: y_head },
                    PathSegment::LineTo { x: x1, y: y_head },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "leftRightArrow" => {
            let a1 = get_adj(0, 0.25).clamp(0.0, 0.5);
            let a2 = get_adj(1, 0.5).clamp(0.0, 1.0);
            let y1 = h * (1.0 - a2) / 2.0;
            let y2 = h * (1.0 + a2) / 2.0;
            let x1 = w * a1;
            let x2 = w * (1.0 - a1);
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: h / 2.0 },
                    PathSegment::LineTo { x: x1, y: 0.0 },
                    PathSegment::LineTo { x: x1, y: y1 },
                    PathSegment::LineTo { x: x2, y: y1 },
                    PathSegment::LineTo { x: x2, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h / 2.0 },
                    PathSegment::LineTo { x: x2, y: h },
                    PathSegment::LineTo { x: x2, y: y2 },
                    PathSegment::LineTo { x: x1, y: y2 },
                    PathSegment::LineTo { x: x1, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "chevron" => {
            let a1 = get_adj(0, 0.5).clamp(0.0, 1.0);
            let dx = w * a1;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: w - dx, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h / 2.0 },
                    PathSegment::LineTo { x: w - dx, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::LineTo { x: dx, y: h / 2.0 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "homePlate" => {
            let offset = (h / 2.0).min(w);
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: w - offset, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h / 2.0 },
                    PathSegment::LineTo { x: w - offset, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "star5" => build_star_polygon(5, w, h, get_adj(0, 0.38197)),
        "star6" => build_star_polygon(6, w, h, get_adj(0, 0.5)),
        "star8" => build_star_polygon(8, w, h, get_adj(0, 0.6)),

        "mathPlus" => {
            let a1 = get_adj(0, 0.25).clamp(0.05, 0.95);
            let x1 = (w - w * a1) / 2.0;
            let x2 = (w + w * a1) / 2.0;
            let y1 = (h - h * a1) / 2.0;
            let y2 = (h + h * a1) / 2.0;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: x1, y: 0.0 },
                    PathSegment::LineTo { x: x2, y: 0.0 },
                    PathSegment::LineTo { x: x2, y: y1 },
                    PathSegment::LineTo { x: w, y: y1 },
                    PathSegment::LineTo { x: w, y: y2 },
                    PathSegment::LineTo { x: x2, y: y2 },
                    PathSegment::LineTo { x: x2, y: h },
                    PathSegment::LineTo { x: x1, y: h },
                    PathSegment::LineTo { x: x1, y: y2 },
                    PathSegment::LineTo { x: 0.0, y: y2 },
                    PathSegment::LineTo { x: 0.0, y: y1 },
                    PathSegment::LineTo { x: x1, y: y1 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "mathMinus" => {
            let a1 = get_adj(0, 0.25).clamp(0.05, 0.95);
            let y1 = (h - h * a1) / 2.0;
            let y2 = (h + h * a1) / 2.0;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: y1 },
                    PathSegment::LineTo { x: w, y: y1 },
                    PathSegment::LineTo { x: w, y: y2 },
                    PathSegment::LineTo { x: 0.0, y: y2 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "flowChartTerminator" => {
            // Pill / stadium shape
            let r = (w.min(h)) / 2.0;
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: r, y: 0.0 });
            segs.push(PathSegment::LineTo { x: w - r, y: 0.0 });
            decompose_arc_to_cubic(w - r, r, r, r, 270.0, 180.0, &mut segs);
            segs.push(PathSegment::LineTo { x: r, y: h });
            decompose_arc_to_cubic(r, r, r, r, 90.0, 180.0, &mut segs);
            segs.push(PathSegment::Close);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "line" => {
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h },
                ],
                FillRule::NonZero,
            ))
        }

        _ => None,
    }
}

fn build_star_polygon(points: usize, w: f32, h: f32, inner_ratio: f32) -> Option<VectorPath> {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx_outer = w / 2.0;
    let ry_outer = h / 2.0;
    let rx_inner = rx_outer * inner_ratio.clamp(0.1, 0.9);
    let ry_inner = ry_outer * inner_ratio.clamp(0.1, 0.9);

    let total_steps = points * 2;
    let angle_step = std::f32::consts::PI / (points as f32);
    let mut segs = Vec::new();

    for i in 0..total_steps {
        let ang = -std::f32::consts::FRAC_PI_2 + (i as f32) * angle_step;
        let (rx, ry) = if i % 2 == 0 {
            (rx_outer, ry_outer)
        } else {
            (rx_inner, ry_inner)
        };
        let x = cx + rx * ang.cos();
        let y = cy + ry * ang.sin();
        if i == 0 {
            segs.push(PathSegment::MoveTo { x, y });
        } else {
            segs.push(PathSegment::LineTo { x, y });
        }
    }
    segs.push(PathSegment::Close);
    Some(VectorPath::new(segs, FillRule::NonZero))
}
