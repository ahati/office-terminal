use super::arc::decompose_arc_to_cubic;
use crate::contracts::{FillRule, PathSegment, VectorPath};

pub fn build_t2_preset(name: &str, w: f32, h: f32, adj: &[Option<f64>]) -> Option<VectorPath> {
    let _get_adj = |idx: usize, default: f32| -> f32 {
        adj.get(idx)
            .and_then(|&val| val)
            .map(|val| (val as f32) / 100000.0)
            .unwrap_or(default)
    };

    match name {
        "cube" => {
            // Stand-in recipe from PRD 02b App §2:
            // Front rect [0, 0.2 h, 0.8 w, h], top parallelogram, right side parallelogram
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.2 * h },
                    PathSegment::LineTo { x: 0.2 * w, y: 0.0 },
                    PathSegment::LineTo { x: w, y: 0.0 },
                    PathSegment::LineTo { x: w, y: 0.8 * h },
                    PathSegment::LineTo { x: 0.8 * w, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "can" => {
            // Cylinder: top ellipse arc, bottom ellipse arc, vertical sides
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: 0.0, y: 0.15 * h });
            decompose_arc_to_cubic(w / 2.0, 0.15 * h, w / 2.0, 0.15 * h, 180.0, 180.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w, y: 0.85 * h });
            decompose_arc_to_cubic(w / 2.0, 0.85 * h, w / 2.0, 0.15 * h, 0.0, 180.0, &mut segs);
            segs.push(PathSegment::Close);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "foldedCorner" => {
            let fw = 0.2 * w;
            let fh = 0.2 * h;
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: w - fw, y: 0.0 },
                    PathSegment::LineTo { x: w, y: fh },
                    PathSegment::LineTo { x: w, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "heart" => {
            // Heart: two cubic bumps on top, converging to bottom center
            let cx = w / 2.0;
            let segs = vec![
                PathSegment::MoveTo { x: cx, y: 0.25 * h },
                PathSegment::CubicTo {
                    x1: cx,
                    y1: 0.0,
                    x2: 0.0,
                    y2: 0.0,
                    x: 0.0,
                    y: 0.35 * h,
                },
                PathSegment::CubicTo {
                    x1: 0.0,
                    y1: 0.65 * h,
                    x2: cx * 0.7,
                    y2: 0.85 * h,
                    x: cx,
                    y: h,
                },
                PathSegment::CubicTo {
                    x1: cx * 1.3,
                    y1: 0.85 * h,
                    x2: w,
                    y2: 0.65 * h,
                    x: w,
                    y: 0.35 * h,
                },
                PathSegment::CubicTo {
                    x1: w,
                    y1: 0.0,
                    x2: cx,
                    y2: 0.0,
                    x: cx,
                    y: 0.25 * h,
                },
                PathSegment::Close,
            ];
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "lightningBolt" => {
            // 7-point zigzag polygon from PRD 02b App §2:
            // (0.3w, 0) -> (0.7w, 0) -> (0.4w, 0.45h) -> (0.65w, 0.45h) -> (0.2w, h) -> (0.35w, 0.55h) -> (0.1w, 0.55h) -> Close
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.3 * w, y: 0.0 },
                    PathSegment::LineTo { x: 0.7 * w, y: 0.0 },
                    PathSegment::LineTo {
                        x: 0.4 * w,
                        y: 0.45 * h,
                    },
                    PathSegment::LineTo {
                        x: 0.65 * w,
                        y: 0.45 * h,
                    },
                    PathSegment::LineTo { x: 0.2 * w, y: h },
                    PathSegment::LineTo {
                        x: 0.35 * w,
                        y: 0.55 * h,
                    },
                    PathSegment::LineTo {
                        x: 0.1 * w,
                        y: 0.55 * h,
                    },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "cloud" => {
            // Composite cloud outline with 4 rounded bumps and flat bottom
            let segs = vec![
                PathSegment::MoveTo {
                    x: 0.15 * w,
                    y: 0.85 * h,
                },
                PathSegment::CubicTo {
                    x1: 0.0,
                    y1: 0.85 * h,
                    x2: 0.0,
                    y2: 0.55 * h,
                    x: 0.15 * w,
                    y: 0.55 * h,
                },
                PathSegment::CubicTo {
                    x1: 0.15 * w,
                    y1: 0.25 * h,
                    x2: 0.45 * w,
                    y2: 0.15 * h,
                    x: 0.55 * w,
                    y: 0.35 * h,
                },
                PathSegment::CubicTo {
                    x1: 0.65 * w,
                    y1: 0.25 * h,
                    x2: 0.85 * w,
                    y2: 0.35 * h,
                    x: 0.85 * w,
                    y: 0.55 * h,
                },
                PathSegment::CubicTo {
                    x1: w,
                    y1: 0.55 * h,
                    x2: w,
                    y2: 0.85 * h,
                    x: 0.85 * w,
                    y: 0.85 * h,
                },
                PathSegment::Close,
            ];
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "wedgeRectCallout" => {
            // Rectangle with triangular pointer projecting from bottom
            let r = 0.1 * w.min(h);
            Some(VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: r, y: 0.0 },
                    PathSegment::LineTo { x: w - r, y: 0.0 },
                    PathSegment::LineTo { x: w, y: r },
                    PathSegment::LineTo { x: w, y: 0.75 * h },
                    PathSegment::LineTo {
                        x: 0.5 * w,
                        y: 0.75 * h,
                    },
                    PathSegment::LineTo { x: 0.35 * w, y: h },
                    PathSegment::LineTo {
                        x: 0.3 * w,
                        y: 0.75 * h,
                    },
                    PathSegment::LineTo { x: 0.0, y: 0.75 * h },
                    PathSegment::LineTo { x: 0.0, y: r },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ))
        }

        "seal12" => build_seal(12, w, h),
        "seal16" => build_seal(16, w, h),
        "seal24" => build_seal(24, w, h),
        "seal32" => build_seal(32, w, h),

        _ => None,
    }
}

fn build_seal(n: usize, w: f32, h: f32) -> Option<VectorPath> {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx_outer = w / 2.0;
    let ry_outer = h / 2.0;
    let rx_inner = rx_outer * 0.85;
    let ry_inner = ry_outer * 0.85;

    let total_steps = n * 2;
    let angle_step = std::f32::consts::PI / (n as f32);
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
