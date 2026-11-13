pub mod gradient;
pub mod line_end;
pub mod stroke;

use crate::contracts::{
    GradientFrame, LineCap, LineJoin, NormalizedRect, PathSegment, VectorGradient,
    VectorImageFill, VectorPaint, VectorPath, VectorStroke, VectorTile,
};
/// Synchronous-canvas ceiling kept in parity with the reference parser
/// (`MAX_DRAWINGML_CUSTOM_DASH_STOPS`).
/// ref packages/core/src/draw/dash.ts:143
const MAX_DRAWINGML_CUSTOM_DASH_STOPS: usize = 512;

/// Parse a hex color string (e.g. "RRGGBB", "#RRGGBB", "RRGGBBAA", "#RRGGBBAA").
/// Defaults to magenta `[255, 0, 255, 255]` on error.
pub fn parse_hex_color(hex: &str) -> [u8; 4] {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
        [r, g, b, 255]
    } else if clean.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
        let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
        [r, g, b, a]
    } else {
        [255, 0, 255, 255]
    }
}

/// Apply opacity multiplier to an RGBA color.
pub fn modulate_alpha(mut color: [u8; 4], alpha: f32) -> [u8; 4] {
    let a = (color[3] as f32 * alpha.clamp(0.0, 1.0)).round() as u8;
    color[3] = a;
    color
}

/// Convert a pptx_model::Fill to VectorPaint.
///
/// Shape rotation/reflection is unknown here, so an `a:blipFill` compiles with
/// `rotWithShape` treated as 0°. Element compilers use
/// [`compile_shape_fill`] to thread the authored frame.
pub fn compile_fill(fill: &pptx_model::Fill, alpha: f32) -> Option<VectorPaint> {
    compile_shape_fill(fill, alpha, 0.0, false, false)
}

/// Convert a `CT_ShapeProperties` fill to [`VectorPaint`] with the owning
/// shape's authored frame, which the `rotWithShape="0"` blip-fill
/// counter-rotation needs (ref renderer.ts:2644-2656).
pub fn compile_shape_fill(
    fill: &pptx_model::Fill,
    alpha: f32,
    rotation_deg: f64,
    flip_h: bool,
    flip_v: bool,
) -> Option<VectorPaint> {
    match fill {
        pptx_model::Fill::Solid { color } => {
            let c = modulate_alpha(parse_hex_color(color), alpha);
            Some(VectorPaint::Solid(c))
        }
        pptx_model::Fill::Gradient {
            stops,
            angle,
            grad_type,
            scaled,
            path,
            fill_to_rect,
            tile_rect,
            flip,
            rot_with_shape,
        } => {
            // ref packages/core/src/shape/paint.ts:377-379: a gradient with no
            // stops paints nothing; a single stop paints that solid colour.
            if stops.is_empty() {
                return None;
            }
            if stops.len() == 1 {
                let color = modulate_alpha(parse_hex_color(&stops[0].color), alpha);
                return Some(VectorPaint::Solid(color));
            }

            let gradient_stops: Vec<(f32, [u8; 4])> = stops
                .iter()
                .map(|stop| {
                    let col = modulate_alpha(parse_hex_color(&stop.color), alpha);
                    (stop.position as f32, col)
                })
                .collect();

            // §20.1.8.32 frame options reach the renderers; the projection into
            // an axis/focus frame happens at paint time so both backends agree
            // (ref paint.ts:262-320).
            let grad = VectorGradient {
                angle_deg: *angle as f32,
                stops: gradient_stops,
                frame: GradientFrame {
                    rot_with_shape: rot_with_shape.unwrap_or(true),
                    rotation_deg: rotation_deg as f32,
                    flip_h,
                    flip_v,
                    scaled: *scaled == Some(true),
                    path: path.clone().unwrap_or_default(),
                    fill_to_rect: fill_to_rect.as_ref().and_then(normalized_rect),
                    tile_rect: tile_rect.as_ref().and_then(normalized_rect),
                    flip: flip.clone().unwrap_or_else(|| "none".to_string()),
                },
            };

            if grad_type == "radial" {
                Some(VectorPaint::RadialGradient(grad))
            } else {
                Some(VectorPaint::LinearGradient(grad))
            }
        }
        pptx_model::Fill::Pattern { fg, bg, preset } => {
            // §20.1.8.40 `a:pattFill` is a tiled bitmap whose cell is 1/8 pt; a
            // preset outside ST_PresetPatternVal keeps the reference's solid
            // foreground fallback (ref core/src/shape/paint.ts:474-476).
            if crate::compiler::pattern::pattern_rows(preset).is_none() {
                let c = modulate_alpha(parse_hex_color(fg), alpha);
                return Some(VectorPaint::Solid(c));
            }
            Some(VectorPaint::Pattern(crate::contracts::VectorPattern {
                preset: preset.clone(),
                fg: modulate_alpha(parse_hex_color(fg), alpha),
                bg: modulate_alpha(parse_hex_color(bg), alpha),
            }))
        }
        pptx_model::Fill::None => None,
        pptx_model::Fill::Image {
            image_path,
            svg_image_path,
            dpi,
            rot_with_shape,
            src_rect,
            fill_rect,
            stretch,
            tile,
            alpha: fill_alpha,
            blip_effects,
            ..
        } => compile_image_fill(
            svg_image_path.as_deref().unwrap_or(image_path),
            rotation_deg,
            flip_h,
            flip_v,
            alpha,
            ImageFillSpec {
                src_rect: src_rect.as_ref(),
                fill_rect: fill_rect.as_ref(),
                stretch: *stretch,
                tile: tile.as_ref(),
                alpha: fill_alpha.map(|a| a as f32),
                rot_with_shape: rot_with_shape.unwrap_or(true),
                dpi: dpi.map(|d| d as f32).unwrap_or(0.0),
                effects: blip_effects.clone(),
            },
        ),
    }
}

/// The `a:blipFill` facts [`compile_image_fill`] projects into the IR.
struct ImageFillSpec<'a> {
    src_rect: Option<&'a ooxml_common::blip::SrcRect>,
    fill_rect: Option<&'a ooxml_common::fill::FillRect>,
    stretch: bool,
    tile: Option<&'a ooxml_common::fill::TileInfo>,
    alpha: Option<f32>,
    rot_with_shape: bool,
    dpi: f32,
    effects: Vec<ooxml_common::blip::BlipEffect>,
}

/// Resolve `a:blipFill` (ECMA-376 §20.1.8.14) to [`VectorImageFill`].
///
/// The two fill modes stay authored: a fill with no mode, or with the
/// schema-invalid `tile` + `stretch` pair, still compiles so the paint path can
/// observe and reject it (`VectorImageFill::mode_is_paintable`).
/// ref renderer.ts:2610-2616, 2620-2700.
fn compile_image_fill(
    rel_id: &str,
    rotation_deg: f64,
    flip_h: bool,
    flip_v: bool,
    alpha: f32,
    spec: ImageFillSpec<'_>,
) -> Option<VectorPaint> {
    if rel_id.is_empty() {
        return None;
    }
    // §20.1.8.55 keeps the *logical* window so negative outsets survive as a
    // wider tiled extent (ref renderer.ts:2408-2417).
    let src_rect = match spec.src_rect {
        Some(sr) if [sr.l, sr.t, sr.r, sr.b].iter().all(|v| v.is_finite()) => NormalizedRect {
            x: sr.l as f32,
            y: sr.t as f32,
            w: 1.0 - (sr.l + sr.r) as f32,
            h: 1.0 - (sr.t + sr.b) as f32,
        },
        _ => NormalizedRect::default(),
    };
    // A source window with no visible area paints nothing (ref crop.ts:100-105).
    if src_rect.w <= 0.0 || src_rect.h <= 0.0 {
        return None;
    }
    let fill_rect = spec.fill_rect.and_then(|fr| {
        if ![fr.l, fr.t, fr.r, fr.b].iter().all(|v| v.is_finite()) {
            return None;
        }
        Some(NormalizedRect {
            x: fr.l as f32,
            y: fr.t as f32,
            w: 1.0 - (fr.l + fr.r) as f32,
            h: 1.0 - (fr.t + fr.b) as f32,
        })
    });
    let tile = spec.tile.map(|tile| VectorTile {
        algn: tile.algn.clone().unwrap_or_else(|| "tl".to_string()),
        tx: tile.tx.unwrap_or(0) as f32,
        ty: tile.ty.unwrap_or(0) as f32,
        sx: tile.sx.map(|v| v as f32).unwrap_or(1.0),
        sy: tile.sy.map(|v| v as f32).unwrap_or(1.0),
        flip: tile.flip.clone().unwrap_or_else(|| "none".to_string()),
    });

    // `a:alphaModFix` is parsed into blip effects; the legacy field (if any)
    // stays a whole-placement multiplier beside the caller alpha.
    let alpha = (spec.alpha.unwrap_or(1.0) * alpha).clamp(0.0, 1.0);

    Some(VectorPaint::Image(Box::new(VectorImageFill {
        rel_id: rel_id.to_string(),
        src_rect,
        fill_rect,
        tile,
        stretch: spec.stretch,
        alpha,
        rot_with_shape: spec.rot_with_shape,
        dpi: spec.dpi,
        effects: spec.effects,
        rotation_deg: rotation_deg as f32,
        flip_h,
        flip_v,
    })))
}

/// §20.1.8.32 `CT_RelativeRect` as the IR's fractional window
/// (`x = l`, `y = t`, `w = 1-l-r`, `h = 1-t-b`). A non-finite edge drops the
/// frame (`ref paint.ts:2467-2477` treats it as absent).
pub fn normalized_rect(rect: &ooxml_common::fill::FillRect) -> Option<NormalizedRect> {
    if ![rect.l, rect.t, rect.r, rect.b]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    Some(NormalizedRect {
        x: rect.l as f32,
        y: rect.t as f32,
        w: 1.0 - (rect.l + rect.r) as f32,
        h: 1.0 - (rect.t + rect.b) as f32,
    })
}

/// Convert a pptx_model::Stroke to VectorStroke.
pub fn compile_stroke(stroke: &pptx_model::Stroke) -> Option<VectorStroke> {
    compile_stroke_for_shape(stroke, 0.0, false, false)
}

/// [`compile_stroke`] with the owning shape's authored frame, which the
/// `a:ln` gradient's `rotWithShape="0"` counter-rotation needs (the reference
/// passes `shapeRotationDeg` into `applyStroke`). ref renderer.ts:7542-7565
pub fn compile_stroke_for_shape(
    stroke: &pptx_model::Stroke,
    rotation_deg: f64,
    flip_h: bool,
    flip_v: bool,
) -> Option<VectorStroke> {
    let color = parse_hex_color(&stroke.color);
    let width_emu = stroke.width;
    let dash_segments = compile_dash_segments(stroke);

    // §20.1.8.14/40: a line's own gradient/pattern fill replaces its flat
    // colour entirely (ref renderer.ts:7542-7565 `applyStroke`).
    let paint = stroke
        .fill
        .as_ref()
        .and_then(|fill| compile_shape_fill(fill, 1.0, rotation_deg, flip_h, flip_v))
        .map(Box::new);

    // ref packages/core/src/shape/stroke-geometry.ts:19 — the DrawingML
    // default cap is butt, not square.
    let mut line_cap = match stroke.line_cap.as_deref() {
        Some("rnd") | Some("round") => LineCap::Round,
        Some("sq") | Some("square") => LineCap::Square,
        Some("flat") | Some("butt") | None => LineCap::Butt,
        _ => LineCap::Butt,
    };

    // ref stroke-geometry.ts:20-21 — a zero-length on-segment is painted with a
    // square cap even when the authored cap is butt (Canvas/`tiny-skia` drop it
    // otherwise). Only even indices are on-segments.
    if line_cap == LineCap::Butt && has_zero_on_segment(dash_segments.as_deref()) {
        line_cap = LineCap::Square;
    }

    let line_join = match stroke.line_join.as_deref() {
        Some("round") => LineJoin::Round,
        Some("bevel") => LineJoin::Bevel,
        Some("miter") => LineJoin::Miter,
        _ => LineJoin::Miter,
    };

    Some(VectorStroke {
        color,
        width_emu,
        dash_segments,
        line_cap,
        line_join,
        paint,
    })
}

/// Authored custom dash wins over the preset; unknown styles fall through to
/// the VML numeric form and finally to a continuous stroke.
/// ref packages/core/src/draw/dash.ts:158-228
fn compile_dash_segments(stroke: &pptx_model::Stroke) -> Option<Vec<f32>> {
    if !stroke.custom_dash.is_empty() {
        let mut segments = Vec::new();
        for seg in stroke
            .custom_dash
            .iter()
            .take(MAX_DRAWINGML_CUSTOM_DASH_STOPS)
        {
            let dash = seg.dash as f32;
            let space = seg.space as f32;
            if !dash.is_finite()
                || !space.is_finite()
                || dash < 0.0
                || space < 0.0
                || (dash == 0.0 && space == 0.0)
            {
                continue;
            }
            segments.push(dash);
            segments.push(space);
        }
        return (!segments.is_empty()).then_some(segments);
    }

    stroke.dash_style.as_deref().and_then(preset_dash_segments)
}

/// §20.1.10.49 ST_PresetLineDashVal relative patterns (line-width units).
/// ref packages/core/src/draw/dash.ts:146-157
fn preset_dash_segments(style: &str) -> Option<Vec<f32>> {
    let relative: &[f32] = match style {
        "dash" => &[6.0, 3.0],
        "dot" => &[1.5, 3.0],
        "dashDot" => &[6.0, 3.0, 1.5, 3.0],
        "lgDash" => &[10.0, 4.0],
        "lgDashDot" => &[10.0, 4.0, 1.5, 4.0],
        "lgDashDotDot" => &[10.0, 4.0, 1.5, 4.0, 1.5, 4.0],
        "sysDash" => &[4.0, 2.0],
        "sysDot" => &[1.0, 2.0],
        "sysDashDot" => &[4.0, 2.0, 1.0, 2.0],
        "sysDashDotDot" => &[4.0, 2.0, 1.0, 2.0, 1.0, 2.0],
        _ => &[],
    };
    if !relative.is_empty() {
        return Some(relative.to_vec());
    }

    // VML legacy numeric dashstyle (Part 4 §19.1.2.21): whitespace/comma
    // separated line-width multiples; an unmatched final value is dropped.
    // ref packages/core/src/draw/dash.ts:218-228
    let mut numeric: Vec<f32> = style
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .map(|token| token.parse::<f32>().unwrap_or(f32::NAN))
        .collect();
    if numeric.len() >= 2 && numeric.iter().all(|v| v.is_finite() && *v >= 0.0) {
        // JS `Number('')` is 0; the reference only rejects NaN/infinite values.
        if numeric.iter().any(|v| *v > 0.0) {
            if !numeric.len().is_multiple_of(2) {
                numeric.pop();
            }
            return Some(numeric);
        }
    }
    None
}

fn has_zero_on_segment(dash: Option<&[f32]>) -> bool {
    dash.is_some_and(|segs| {
        segs.iter()
            .enumerate()
            .any(|(index, length)| index % 2 == 0 && *length == 0.0)
    })
}


/// Local-space bounds of a path (control points included, matching the
/// rasterizer's conservative bounds). Used as the shape box for gradient
/// framing: the IR does not carry the authored shape rect, and a preset path
/// normally spans `(0, 0)..(width, height)` in local coordinates.
pub fn path_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;

    for seg in &path.segments {
        let mut add = |x: f32, y: f32| {
            if x < min_x { min_x = x; }
            if x > max_x { max_x = x; }
            if y < min_y { min_y = y; }
            if y > max_y { max_y = y; }
        };
        match seg {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => {
                add(*x, *y);
                any = true;
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                add(*x1, *y1);
                add(*x, *y);
                any = true;
            }
            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                add(*x1, *y1);
                add(*x2, *y2);
                add(*x, *y);
                any = true;
            }
            PathSegment::Close => {}
        }
    }

    any.then_some((min_x, min_y, max_x, max_y))
}

/// Axis endpoints of a DrawingML linear gradient for the fill box
/// `(x, y, w, h)` in shape-local coordinates.
///
/// The reference centres the axis on the box and uses half the sum of the
/// axis-aligned projections of the box edges: `(|cos|·w + |sin|·h) / 2`.
/// ref packages/core/src/shape/paint.ts:290-300
pub fn linear_gradient_axis(
    angle_deg: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> ((f32, f32), (f32, f32)) {
    let rad = angle_deg.to_radians();
    let dx = rad.cos();
    let dy = rad.sin();
    let half = (dx.abs() * w + dy.abs() * h) / 2.0;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    (
        (cx - dx * half, cy - dy * half),
        (cx + dx * half, cy + dy * half),
    )
}

/// [`linear_gradient_axis`] framed on a path's local bounds.
pub fn linear_gradient_axis_for_path(
    angle_deg: f32,
    path: &VectorPath,
) -> Option<((f32, f32), (f32, f32))> {
    let (min_x, min_y, max_x, max_y) = path_bounds(path)?;
    Some(linear_gradient_axis(
        angle_deg,
        min_x,
        min_y,
        max_x - min_x,
        max_y - min_y,
    ))
}

/// Centre and radius of a DrawingML radial (`path="circle"`/`shape`) gradient
/// for the fill box `(x, y, w, h)` in shape-local coordinates.
///
/// With no `fillToRect` the focus frame is the whole box, so the centre is the
/// box centre and the radius is the half-diagonal (`hypot(rx, ry)`), matching
/// the reference's `circle`/`shape` branch. The reference's `path="rect"`
/// branch (`max(rx, ry)`) and authored focus/tile rectangles are unreachable:
/// `VectorGradient` carries neither. ref packages/core/src/shape/paint.ts:114-140
pub fn radial_gradient_frame(x: f32, y: f32, w: f32, h: f32) -> ((f32, f32), f32) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let rx = (cx - x).abs().max((x + w - cx).abs());
    let ry = (cy - y).abs().max((y + h - cy).abs());
    ((cx, cy), (rx * rx + ry * ry).sqrt())
}

/// [`radial_gradient_frame`] framed on a path's local bounds.
pub fn radial_gradient_frame_for_path(path: &VectorPath) -> Option<((f32, f32), f32)> {
    let (min_x, min_y, max_x, max_y) = path_bounds(path)?;
    Some(radial_gradient_frame(
        min_x,
        min_y,
        max_x - min_x,
        max_y - min_y,
    ))
}
