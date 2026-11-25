//! Preset geometry compilation (PRD 02b / PRD 11).
//!
//! [`build_shape_paths`] is the dispatch point used by the element compilers.
//! Everything except `custGeom` resolves through the ECMA-376 preset engine
//! ([`preset_exec::build_preset`] over the embedded `presets.json`), which
//! returns one [`PresetSubpath`] per authored `<a:path>` together with the
//! authored paint flags (`<a:path@fill>` / `@stroke`, ECMA-376 §20.1.10.37).
//! The reference painter walks exactly that list
//! (`packages/core/src/shape/preset-geometry/index.ts:44-99`), so the display
//! list carries one `DrawPath` per subpath.
//!
//! The hand-written `presets_*` modules stay compiled as the legacy reference
//! implementation (and as the unit-test target of `preset_extended_tests`),
//! but dispatch no longer consults them.
//!
//! [`build_shape_path`] is the single-path compatibility wrapper for callers
//! that need one `VectorPath` (picture geometry, legacy tests): it merges the
//! painted subpaths and drops the stroke-only overlays.

pub mod arc;
pub mod custgeom;
pub mod preset_data;
pub mod preset_eval;
pub mod preset_exec;
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

use crate::contracts::{FidelityClass, FillRule, PathSegment, VectorPaint, VectorPath, WarningEvent};

pub use self::custgeom::{build_custom_geometry, build_custom_geometry_path};
pub use self::preset_exec::{build_preset, PathFill, PresetSubpath};

/// Multi-path dispatch used by the element compilers.
///
/// * `custGeom` replays the authored path list through [`build_custom_geometry`]
///   (unchanged behaviour: one painted subpath);
/// * every other name is evaluated by the ECMA-376 engine, giving one
///   [`PresetSubpath`] per `<a:path>` in declaration order
///   (ref `preset-geometry/index.ts:29-99`);
/// * an unknown name emits [`WarningEvent::UnsupportedPreset`] and degrades to
///   the bounding rectangle (ref `renderPresetShape` returning `false`).
///
/// Non-finite adjust values degrade to "unset" (preset default) so a corrupt
/// handle can never leak NaN geometry into the display list; the engine itself
/// keeps the reference's raw arithmetic (`preset_eval.rs:282-289`).
pub fn build_shape_paths(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
    cust_geom: Option<&[Vec<pptx_model::PathCmd>]>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<PresetSubpath> {
    build_shape_paths_painted(geometry, w, h, adj, cust_geom, None, warning_sink)
}

/// [`build_shape_paths`] with the authored per-path paint flags
/// (`custGeomPaint`, ECMA-376 §20.1.9.15).
#[allow(clippy::too_many_arguments)]
pub fn build_shape_paths_painted(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
    cust_geom: Option<&[Vec<pptx_model::PathCmd>]>,
    cust_geom_paint: Option<&[pptx_model::PathPaint]>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<PresetSubpath> {
    if geometry == "custGeom" {
        if let Some(paths) = cust_geom {
            if cust_geom_paint.is_none() {
                return vec![PresetSubpath {
                    path: build_custom_geometry(paths, w, h),
                    fill: PathFill::Norm,
                    stroke: true,
                }];
            }
            return paths
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let paint = cust_geom_paint.and_then(|paints| paints.get(index));
                    PresetSubpath {
                        path: build_custom_geometry_path(path, w, h),
                        fill: PathFill::from_mode(paint.and_then(|p| p.fill.as_deref())),
                        stroke: paint.is_none_or(|p| p.stroke),
                    }
                })
                .collect();
        }
    }

    // ECMA `rect` is deliberately absent from the reference's preset table
    // (`renderer.ts:4208-4213` dispatches only `custGeom`/known presets to the
    // engine and draws everything else as the bounding rectangle), so it must
    // resolve silently rather than warn.
    if geometry.eq_ignore_ascii_case("rect") {
        return vec![rect_subpath(w, h)];
    }

    let finite_adj: Vec<Option<f64>> = adj.iter().map(|value| (*value).filter(|v| v.is_finite())).collect();

    if let Some(subpaths) = build_preset(geometry, w as f64, h as f64, &finite_adj) {
        if !subpaths.is_empty() {
            if coordinates_are_finite(&subpaths) {
                return subpaths;
            }
            // The reference happily computes 0/0 for a collapsed sweep (e.g.
            // `circulararrow` with `stAng == enAng`) or a degenerate box
            // (`halfframe` at 0×0), and the canvas then drops the path. A NaN
            // must never reach the rasterizer, so degrade to the placeholder.
            let _ = warning_sink.send(WarningEvent::FidelityDegraded {
                element_id: geometry.to_string(),
                feature: "preset_geometry_nonfinite",
                class: FidelityClass::Placeholder,
            });
            return vec![rect_subpath(w, h)];
        }
    }

    // ref `index.ts:28`: an unknown preset paints nothing and the caller falls
    // back to a rectangle; a warning keeps the degradation visible in
    // `dump`/`info` output.
    let _ = warning_sink.send(WarningEvent::UnsupportedPreset {
        name: geometry.to_string(),
    });
    vec![rect_subpath(w, h)]
}

/// Single-path compatibility wrapper around [`build_shape_paths`].
///
/// Merges the painted subpaths into one `VectorPath`, preserving subpath order
/// (so multi-contour presets such as `donut` keep their holes). Stroke-only
/// overlay paths are dropped, because they carry no geometry a single-path
/// caller can express. A preset whose paths are all stroke-only (connectors,
/// `line`) falls back to every subpath so the line survives.
pub fn build_shape_path(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
    cust_geom: Option<&[Vec<pptx_model::PathCmd>]>,
    warning_sink: &Sender<WarningEvent>,
) -> VectorPath {
    let subpaths = build_shape_paths(geometry, w, h, adj, cust_geom, warning_sink);

    let painted: Vec<&PresetSubpath> = subpaths.iter().filter(|s| s.fill.is_filled()).collect();
    let merged: Vec<&PresetSubpath> = if painted.is_empty() {
        subpaths.iter().collect()
    } else {
        painted
    };

    let fill_rule = merged
        .first()
        .map_or(FillRule::NonZero, |subpath| subpath.path.fill_rule);
    let mut segments = Vec::new();
    for subpath in merged {
        segments.extend(subpath.path.segments.iter().copied());
    }
    VectorPath::new(segments, fill_rule)
}

/// Recolour a resolved shape fill for one `<a:path fill>` modifier.
///
/// The reference paints the base fill and then source-over composites a
/// translucent tint on top of the very same path (ref `geometry.ts:349-399`
/// `pathFillModeOverlay` + `index.ts:59-77`): black at 102/255 for `darken`,
/// white at 102/255 for `lighten`, and 50/255 for the "less" modes. Folding the
/// overlay into one opaque paint yields the measured per-channel map:
///
/// * `darken`       `v * 153/255`
/// * `darkenLess`   `v * 205/255`
/// * `lighten`      `255 - (255 - v) * 153/255`
/// * `lightenLess`  `255 - (255 - v) * 205/255`
///
/// A translucent base fill also takes the overlay's alpha (`a' = w + (1 - w)a`),
/// so the folded colour matches the reference's two-pass composite exactly.
///
/// `Norm` keeps the fill. `None` drops it (stroke-only path). A gradient has no
/// flat channel map — the reference overlays a translucent rect over the
/// gradient — and `VectorPaint` cannot express a tinted gradient, so gradients
/// are left untinted here rather than approximated.
pub fn apply_path_fill_mode(fill: Option<&VectorPaint>, mode: PathFill) -> Option<VectorPaint> {
    if !mode.is_filled() {
        return None;
    }
    let paint = fill?;
    match paint {
        VectorPaint::Solid(color) => Some(VectorPaint::Solid(tint_rgba(*color, mode))),
        // Cannot express: tinting a gradient needs the overlay composited over
        // the resolved gradient, which `VectorGradient` has no slot for; leave
        // it untinted rather than approximating (documented limitation).
        VectorPaint::LinearGradient(_) | VectorPaint::RadialGradient(_) => Some(paint.clone()),
        // An untinted image/pattern brush stays as authored: the reference's
        // overlay is a translucent rect composited over the resolved brush, and
        // the IR has no tinted-brush slot (documented limitation).
        VectorPaint::Image(_) | VectorPaint::Pattern(_) | VectorPaint::Texture(_) => Some(paint.clone()),
    }
}

/// Opaque-black / opaque-white source-over weight of a fill mode at 8-bit
/// resolution: 102/255 for the full modes, 50/255 for the "less" modes.
/// ref `geometry.ts:391-399`.
fn tint_weight(mode: PathFill) -> Option<f32> {
    match mode {
        PathFill::Darken | PathFill::Lighten => Some(102.0 / 255.0),
        PathFill::DarkenLess | PathFill::LightenLess => Some(50.0 / 255.0),
        PathFill::Norm | PathFill::None => None,
    }
}

/// Source-over composite of the reference's tint overlay onto one RGBA colour.
fn tint_rgba(color: [u8; 4], mode: PathFill) -> [u8; 4] {
    let Some(weight) = tint_weight(mode) else {
        return color;
    };
    let toward = match mode {
        PathFill::Lighten | PathFill::LightenLess => 255.0,
        _ => 0.0,
    };
    let base_alpha = f32::from(color[3]) / 255.0;
    let out_alpha = weight + (1.0 - weight) * base_alpha;
    if out_alpha <= f32::EPSILON {
        return [0, 0, 0, 0];
    }
    let channel = |value: u8| -> u8 {
        let premultiplied = weight * toward + (1.0 - weight) * base_alpha * f32::from(value);
        (premultiplied / out_alpha).clamp(0.0, 255.0).round() as u8
    };
    [
        channel(color[0]),
        channel(color[1]),
        channel(color[2]),
        (out_alpha * 255.0).round() as u8,
    ]
}

/// Whether every emitted coordinate is finite.
///
/// Guards the rasterizer: `tiny-skia` rejects non-finite input, which would
/// fail the whole slide instead of one shape.
fn coordinates_are_finite(subpaths: &[PresetSubpath]) -> bool {
    subpaths.iter().all(|subpath| {
        subpath.path.segments.iter().all(|segment| match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => x.is_finite() && y.is_finite(),
            PathSegment::QuadTo { x1, y1, x, y } => x1.is_finite() && y1.is_finite() && x.is_finite() && y.is_finite(),
            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                x1.is_finite() && y1.is_finite() && x2.is_finite() && y2.is_finite() && x.is_finite() && y.is_finite()
            }
            PathSegment::Close => true,
        })
    })
}

/// The axis-aligned rectangle: the ECMA `rect` preset and, like the reference's
/// `buildShapePath` fallback, the placeholder for an unknown preset.
fn rect_subpath(w: f32, h: f32) -> PresetSubpath {
    PresetSubpath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: w, y: 0.0 },
                PathSegment::LineTo { x: w, y: h },
                PathSegment::LineTo { x: 0.0, y: h },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: PathFill::Norm,
        stroke: true,
    }
}
