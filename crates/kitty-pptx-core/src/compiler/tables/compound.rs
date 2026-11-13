//! Compound table borders (`a:ln@cmpd`, ECMA-376 §20.1.8.42).
//!
//! A compound rule paints as parallel rails separated by gaps. The reference
//! paints them as FILLS rather than independent strokes so two snapped strokes
//! can never collapse onto one device row, and it reuses the shared
//! double-border painter for the table leg
//! (ref `renderer.ts:7951-7973`, `packages/core/src/draw/double-border.ts:40-70`).
//! Rail placement and thickness follow the reference's `drawCompoundLine`
//! envelope table (ref `renderer.ts:7460-7487`):
//!
//! ```text
//! dbl       -> w/3 rail, w/3 gap, w/3 rail
//! thinThick -> w/4 rail, w/4 gap, w/2 rail
//! thickThin -> w/2 rail, w/4 gap, w/4 rail
//! tri       -> w/5 + 3w/5 + w/5 (abutting bands, as in the reference)
//! ```
//!
//! The reference table leg splits only `cmpd="dbl"`; `thinThick`/`thickThin`/
//! `tri` arrive from the connector painter. A rail is a plain fill, so dashed,
//! custom-dashed and non-solid-paint lines keep the authored stroke instead.

use crate::contracts::{
    AffineTransform, FillRule, PathSegment, VectorCommand, VectorPaint, VectorPath, VectorStroke,
};

/// One device pixel in EMU at DPR = 1. The IR is resolution independent, so the
/// reference's device-pixel floor (`double-border.ts:26-30`) is applied at the
/// 96 DPI reference rate.
const EMU_PER_DEVICE_PX: f32 = 9525.0;

/// Slack for "is this segment axis-aligned?" — border endpoints are produced by
/// exact arithmetic, so only float noise separates them.
const AXIS_EPSILON: f32 = 0.01;

/// Rail centre offset from the authored centre line and thickness, both as a
/// fraction of the authored width. ref renderer.ts:7466-7487
struct Band {
    offset: f32,
    frac: f32,
}

const THIN_THICK: [Band; 2] = [
    Band {
        offset: -3.0 / 8.0,
        frac: 1.0 / 4.0,
    },
    Band {
        offset: 1.0 / 4.0,
        frac: 1.0 / 2.0,
    },
];
const THICK_THIN: [Band; 2] = [
    Band {
        offset: -1.0 / 4.0,
        frac: 1.0 / 2.0,
    },
    Band {
        offset: 3.0 / 8.0,
        frac: 1.0 / 4.0,
    },
];
const TRI: [Band; 3] = [
    Band {
        offset: -2.0 / 5.0,
        frac: 1.0 / 5.0,
    },
    Band {
        offset: 0.0,
        frac: 3.0 / 5.0,
    },
    Band {
        offset: 2.0 / 5.0,
        frac: 1.0 / 5.0,
    },
];

/// `(offset, thickness)` rails in EMU, or `None` for a style with no rail
/// geometry. `dbl` is the one style whose rails are floored to whole device
/// pixels so a hairline double never collapses into a single line
/// (ref `double-border.ts:26-30`): `rail = gap = max(1px, round(lw/3))`.
fn rails(cmpd: &str, total: f32) -> Option<Vec<(f32, f32)>> {
    if cmpd == "dbl" {
        let rail = (total / 3.0).round().max(EMU_PER_DEVICE_PX);
        // Rail centres sit at ±(gap/2 + rail/2) and gap == rail.
        return Some(vec![(-rail, rail), (rail, rail)]);
    }
    let bands: &[Band] = match cmpd {
        "thinThick" => &THIN_THICK,
        "thickThin" => &THICK_THIN,
        "tri" => &TRI,
        _ => return None,
    };
    Some(
        bands
            .iter()
            .map(|band| (total * band.offset, total * band.frac))
            .collect(),
    )
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> VectorPath {
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

/// Paint `from`→`to` as compound rails. Returns `false` when the authored rule
/// cannot be expressed as fills, so the caller falls back to the plain stroke
/// and reports the degradation.
pub(super) fn paint_rails(
    out: &mut Vec<VectorCommand>,
    transform: &AffineTransform,
    spec: &pptx_model::Stroke,
    stroke: &VectorStroke,
    from: (f32, f32),
    to: (f32, f32),
) -> bool {
    let Some(cmpd) = spec.cmpd.as_deref() else {
        return false;
    };
    // Keep dashed compound rules on the stroke leg: a rail-wise dash phase has
    // no representation that preserves the authored dash semantics.
    // ref renderer.ts:7955-7959
    if stroke.dash_segments.is_some() || spec.fill.is_some() {
        return false;
    }
    // Rails are axis-aligned band fills; diagonals keep the plain stroke.
    let horizontal = (from.1 - to.1).abs() < AXIS_EPSILON;
    let vertical = (from.0 - to.0).abs() < AXIS_EPSILON;
    if horizontal == vertical {
        return false;
    }
    let Some(bands) = rails(cmpd, stroke.width_emu as f32) else {
        return false;
    };
    let paint = VectorPaint::Solid(stroke.color);
    for (offset, thickness) in bands {
        if thickness <= 0.0 {
            continue;
        }
        let path = if horizontal {
            let y = from.1 + offset - thickness / 2.0;
            rect_path(from.0.min(to.0), y, (to.0 - from.0).abs(), thickness)
        } else {
            let x = from.0 + offset - thickness / 2.0;
            rect_path(x, from.1.min(to.1), thickness, (to.1 - from.1).abs())
        };
        out.push(VectorCommand::draw_path(
            path,
            Some(paint.clone()),
            None,
            *transform,
        ));
    }
    true
}
