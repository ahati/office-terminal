pub mod arc;
pub mod presets_t1;
pub mod presets_t2;

use crossbeam_channel::Sender;
use crate::contracts::{FillRule, PathSegment, VectorPath, WarningEvent};

pub fn build_shape_path(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
    cust_geom: Option<&[Vec<pptx_model::PathCmd>]>,
    warning_sink: &Sender<WarningEvent>,
) -> VectorPath {
    if geometry == "custGeom" {
        if let Some(paths) = cust_geom {
            return build_custom_geometry(paths, w, h);
        }
    }

    // Try Tier 1 presets
    if let Some(path) = presets_t1::build_t1_preset(geometry, w, h, adj) {
        return path;
    }

    // Try Tier 2 presets
    if let Some(path) = presets_t2::build_t2_preset(geometry, w, h, adj) {
        return path;
    }

    // Fallback Tier 3: emit warning, return placeholder rectangle
    let _ = warning_sink.send(WarningEvent::UnsupportedPreset {
        name: geometry.to_string(),
    });

    VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    )
}

fn build_custom_geometry(paths: &[Vec<pptx_model::PathCmd>], w: f32, h: f32) -> VectorPath {
    let mut segments = Vec::new();
    let mut current_pt = (0.0f32, 0.0f32);

    for path_cmds in paths {
        for cmd in path_cmds {
            match cmd {
                pptx_model::PathCmd::MoveTo { x, y } => {
                    let pt = (*x as f32 * w, *y as f32 * h);
                    segments.push(PathSegment::MoveTo { x: pt.0, y: pt.1 });
                    current_pt = pt;
                }
                pptx_model::PathCmd::LineTo { x, y } => {
                    let pt = (*x as f32 * w, *y as f32 * h);
                    segments.push(PathSegment::LineTo { x: pt.0, y: pt.1 });
                    current_pt = pt;
                }
                pptx_model::PathCmd::QuadBezTo { x1, y1, x, y } => {
                    let pt = (*x as f32 * w, *y as f32 * h);
                    segments.push(PathSegment::QuadTo {
                        x1: *x1 as f32 * w,
                        y1: *y1 as f32 * h,
                        x: pt.0,
                        y: pt.1,
                    });
                    current_pt = pt;
                }
                pptx_model::PathCmd::CubicBezTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    let pt = (*x as f32 * w, *y as f32 * h);
                    segments.push(PathSegment::CubicTo {
                        x1: *x1 as f32 * w,
                        y1: *y1 as f32 * h,
                        x2: *x2 as f32 * w,
                        y2: *y2 as f32 * h,
                        x: pt.0,
                        y: pt.1,
                    });
                    current_pt = pt;
                }
                pptx_model::PathCmd::ArcTo {
                    wr,
                    hr,
                    st_ang,
                    sw_ang,
                } => {
                    let semi_w = *wr as f32 * w;
                    let semi_h = *hr as f32 * h;
                    // In DrawingML ArcTo, the arc starts from current_pt
                    // st_ang and sw_ang are in degrees
                    arc::decompose_arc_to_cubic(
                        current_pt.0,
                        current_pt.1,
                        semi_w,
                        semi_h,
                        *st_ang as f32,
                        *sw_ang as f32,
                        &mut segments,
                    );
                }
                pptx_model::PathCmd::Close => {
                    segments.push(PathSegment::Close);
                }
            }
        }
    }

    VectorPath::new(segments, FillRule::NonZero)
}
