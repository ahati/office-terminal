//! Preset geometry compilation (PRD 02b / PRD 11).
//!
//! `build_shape_path` is the single entry point used by the element
//! compilers. Presets resolve through a tiered cascade:
//!
//! 1. Tier 1 exact parametric presets (`presets_t1`, `presets_rects`,
//!    `presets_arrows`, `presets_arrows_curved`, `presets_callouts`,
//!    `presets_symbols`, `presets_buttons`).
//! 2. Tier 2 planar stand-ins (`presets_symbols_extra`, `presets_arrow_callouts`,
//!    `presets_t2`).
//! 3. Tier 3 placeholder rectangle plus `WarningEvent::UnsupportedPreset`.
//!
//! Every builder is a pure function of the local pixel bounds and the raw
//! adjustment handles; normalization and arc decomposition live in `util` and
//! `arc` so the individual presets stay declarative.

pub mod arc;
pub mod presets_arrow_callouts;
pub mod presets_arrows;
pub mod presets_arrows_curved;
pub mod presets_buttons;
pub mod presets_callouts;
pub mod presets_rects;
pub mod presets_symbols;
pub mod presets_symbols_extra;
pub mod presets_t1;
pub mod presets_t2;
pub mod util;

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

    // Tier 1 fast-path cascade over the extended preset library.
    if let Some(path) = presets_t1::build_t1_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_rects::build_rect_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_arrows::build_arrow_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_arrows_curved::build_curved_arrow_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_callouts::build_callout_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_arrow_callouts::build_arrow_callout_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_symbols::build_symbol_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_symbols_extra::build_symbol_extra_preset(geometry, w, h, adj) {
        return path;
    }
    if let Some(path) = presets_buttons::build_button_preset(geometry, w, h, adj) {
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
