//! ECMA-376 §20.1.9.19 text-warp envelopes.
//!
//! The 40 presets are the spec's `presetTextWarpDefinitions.xml` data. Each
//! preset is a pair of open boundary curves (top and bottom). A flat glyph at
//! horizontal fraction `u` is mapped to the local envelope jacobian:
//! translation to the baseline point, rotation to the mean edge tangent,
//! vertical scale to the edge-to-edge gap, and shear for sloped gaps.
//!
//! The preset table is stored as JSON to keep this module focused on the
//! evaluator and geometry. The table is the ECMA-376 preset data; its JSON
//! shape follows the MIT-licensed office-open-xml-viewer preset table.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::contracts::{GlyphTransform, WarpEnvelope, WarpPolyline};

const DEG60K_TO_RAD: f32 = (std::f32::consts::PI * 2.0) / 21_600_000.0;
const FLATTEN_STEPS: usize = 48;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WarpPreset {
    adj: Vec<(String, String)>,
    gd: Vec<(String, String)>,
    paths: Vec<WarpPath>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WarpPath {
    w: Option<f32>,
    h: Option<f32>,
    #[allow(dead_code)]
    fill: Option<String>,
    #[allow(dead_code)]
    stroke: bool,
    #[allow(dead_code)]
    extrusion_ok: bool,
    cmds: Vec<Vec<String>>,
}

static PRESETS: OnceLock<HashMap<String, WarpPreset>> = OnceLock::new();

fn presets() -> &'static HashMap<String, WarpPreset> {
    PRESETS.get_or_init(|| serde_json::from_str(include_str!("text_warp_presets.json")).unwrap_or_default())
}

impl WarpEnvelope {
    fn arc_len(&self) -> f32 {
        self.top.cumulative.last().copied().unwrap_or(0.0)
    }
}

struct Evaluator {
    env: HashMap<String, f32>,
}

impl Evaluator {
    fn new(preset: &WarpPreset, adj: &[i64], w: f32, h: f32) -> Self {
        let ss = w.min(h);
        let ls = w.max(h);
        let mut env = HashMap::new();
        for (name, value) in [
            ("w", w),
            ("h", h),
            ("l", 0.0),
            ("t", 0.0),
            ("r", w),
            ("b", h),
            ("hc", w / 2.0),
            ("vc", h / 2.0),
            ("wd2", w / 2.0),
            ("wd3", w / 3.0),
            ("wd4", w / 4.0),
            ("wd5", w / 5.0),
            ("wd6", w / 6.0),
            ("wd8", w / 8.0),
            ("wd10", w / 10.0),
            ("wd12", w / 12.0),
            ("wd16", w / 16.0),
            ("wd32", w / 32.0),
            ("hd2", h / 2.0),
            ("hd3", h / 3.0),
            ("hd4", h / 4.0),
            ("hd5", h / 5.0),
            ("hd6", h / 6.0),
            ("hd8", h / 8.0),
            ("hd10", h / 10.0),
            ("hd12", h / 12.0),
            ("hd16", h / 16.0),
            ("hd32", h / 32.0),
            ("ss", ss),
            ("ssd2", ss / 2.0),
            ("ssd4", ss / 4.0),
            ("ssd6", ss / 6.0),
            ("ssd8", ss / 8.0),
            ("ssd16", ss / 16.0),
            ("ssd32", ss / 32.0),
            ("ls", ls),
            ("lsd2", ls / 2.0),
            ("lsd4", ls / 4.0),
            ("lsd6", ls / 6.0),
            ("lsd8", ls / 8.0),
            ("lsd16", ls / 16.0),
            ("lsd32", ls / 32.0),
            ("cd", 21_600_000.0),
            ("cd2", 10_800_000.0),
            ("cd4", 5_400_000.0),
            ("cd8", 2_700_000.0),
            ("3cd4", 16_200_000.0),
            ("3cd8", 8_100_000.0),
            ("5cd8", 13_500_000.0),
            ("7cd8", 18_900_000.0),
        ] {
            env.insert(name.to_string(), value);
        }

        let mut evaluator = Self { env };
        for (index, (name, formula)) in preset.adj.iter().enumerate() {
            let value = adj
                .get(index)
                .map(|value| *value as f32)
                .unwrap_or_else(|| evaluator.eval(formula));
            evaluator.env.insert(name.clone(), value);
            if name == "adj" {
                evaluator.env.insert("adj1".to_string(), value);
            } else if name == "adj1" {
                evaluator.env.insert("adj".to_string(), value);
            }
        }
        for (name, formula) in &preset.gd {
            let value = evaluator.eval(formula);
            evaluator.env.insert(name.clone(), value);
        }
        evaluator
    }

    fn resolve(&self, token: &str) -> f32 {
        if let Some(value) = self.env.get(token) {
            return *value;
        }
        token.parse::<f32>().unwrap_or(0.0)
    }

    fn eval(&self, formula: &str) -> f32 {
        let mut parts = formula.split_whitespace();
        let op = parts.next().unwrap_or("val");
        let args: Vec<f32> = parts.map(|token| self.resolve(token)).collect();
        let a = |index: usize| args.get(index).copied().unwrap_or(0.0);
        match op {
            "val" => a(0),
            "*/" => {
                if a(2) == 0.0 {
                    0.0
                } else {
                    a(0) * a(1) / a(2)
                }
            }
            "+-" => a(0) + a(1) - a(2),
            "+/" => {
                if a(2) == 0.0 {
                    0.0
                } else {
                    (a(0) + a(1)) / a(2)
                }
            }
            "?:" => {
                if a(0) > 0.0 {
                    a(1)
                } else {
                    a(2)
                }
            }
            "abs" => a(0).abs(),
            "min" => a(0).min(a(1)),
            "max" => a(0).max(a(1)),
            "pin" => a(1).clamp(a(0), a(2)),
            "sqrt" => a(0).max(0.0).sqrt(),
            "mod" => (a(0) * a(0) + a(1) * a(1) + a(2) * a(2)).sqrt(),
            "sin" => a(0) * (a(1) * DEG60K_TO_RAD).sin(),
            "cos" => a(0) * (a(1) * DEG60K_TO_RAD).cos(),
            "tan" => a(0) * (a(1) * DEG60K_TO_RAD).tan(),
            "at2" => a(1).atan2(a(0)) / DEG60K_TO_RAD,
            "cat2" => a(0) * a(2).atan2(a(1)).cos(),
            "sat2" => a(0) * a(2).atan2(a(1)).sin(),
            _ => 0.0,
        }
    }
}

fn cumulative(points: &[(f32, f32)]) -> Vec<f32> {
    let mut acc = Vec::with_capacity(points.len());
    acc.push(0.0);
    for index in 1..points.len() {
        let dx = points[index].0 - points[index - 1].0;
        let dy = points[index].1 - points[index - 1].1;
        acc.push(acc[index - 1] + dx.hypot(dy));
    }
    acc
}

fn flatten_path(path: &WarpPath, evaluator: &Evaluator, w: f32, h: f32) -> Vec<(f32, f32)> {
    let sx = path.w.filter(|value| *value > 0.0).map_or(1.0, |value| w / value);
    let sy = path.h.filter(|value| *value > 0.0).map_or(1.0, |value| h / value);
    let mut points = Vec::new();
    let mut pen = (0.0f32, 0.0f32);

    for cmd in &path.cmds {
        let arg = |index: usize| cmd.get(index).map(|token| evaluator.resolve(token)).unwrap_or(0.0);
        match cmd.first().map(String::as_str).unwrap_or("") {
            "m" | "l" => {
                pen = (arg(1) * sx, arg(2) * sy);
                points.push(pen);
            }
            "C" => {
                let c1 = (arg(1) * sx, arg(2) * sy);
                let c2 = (arg(3) * sx, arg(4) * sy);
                let end = (arg(5) * sx, arg(6) * sy);
                for step in 1..=FLATTEN_STEPS {
                    let t = step as f32 / FLATTEN_STEPS as f32;
                    let mt = 1.0 - t;
                    points.push((
                        mt * mt * mt * pen.0 + 3.0 * mt * mt * t * c1.0 + 3.0 * mt * t * t * c2.0 + t * t * t * end.0,
                        mt * mt * mt * pen.1 + 3.0 * mt * mt * t * c1.1 + 3.0 * mt * t * t * c2.1 + t * t * t * end.1,
                    ));
                }
                pen = end;
            }
            "Q" => {
                let control = (arg(1) * sx, arg(2) * sy);
                let end = (arg(3) * sx, arg(4) * sy);
                for step in 1..=FLATTEN_STEPS {
                    let t = step as f32 / FLATTEN_STEPS as f32;
                    let mt = 1.0 - t;
                    points.push((
                        mt * mt * pen.0 + 2.0 * mt * t * control.0 + t * t * end.0,
                        mt * mt * pen.1 + 2.0 * mt * t * control.1 + t * t * end.1,
                    ));
                }
                pen = end;
            }
            "a" => {
                let wr_local = arg(1);
                let hr_local = arg(2);
                let wr = wr_local * sx;
                let hr = hr_local * sy;
                let start_visual = arg(3) * DEG60K_TO_RAD;
                let sweep_visual = arg(4) * DEG60K_TO_RAD;
                if wr.abs() < f32::EPSILON || hr.abs() < f32::EPSILON {
                    continue;
                }
                let visual_to_param = |angle: f32| (wr_local * angle.sin()).atan2(hr_local * angle.cos());
                let start = visual_to_param(start_visual);
                let two_pi = std::f32::consts::PI * 2.0;
                let full = (sweep_visual / two_pi).trunc();
                let remainder = sweep_visual - full * two_pi;
                let mut delta = visual_to_param(start_visual + remainder) - start;
                if remainder > 0.0 && delta < 0.0 {
                    delta += two_pi;
                } else if remainder < 0.0 && delta > 0.0 {
                    delta -= two_pi;
                }
                let sweep = delta + full * two_pi;
                let cx = pen.0 - wr * start.cos();
                let cy = pen.1 - hr * start.sin();
                let steps = ((sweep.abs() / two_pi) * 96.0).ceil().max(FLATTEN_STEPS as f32) as usize;
                for step in 1..=steps {
                    let angle = start + sweep * step as f32 / steps as f32;
                    points.push((cx + wr * angle.cos(), cy + hr * angle.sin()));
                }
                pen = (cx + wr * (start + sweep).cos(), cy + hr * (start + sweep).sin());
            }
            "c" => {}
            _ => {}
        }
    }
    points
}

/// Build a warp envelope for `preset` at shape size `w × h` EMU.
pub fn build_warp_envelope(preset: &str, adj: &[i64], w: f32, h: f32) -> Option<WarpEnvelope> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let def = presets().get(&preset.to_ascii_lowercase())?;
    if def.paths.is_empty() {
        return None;
    }
    let evaluator = Evaluator::new(def, adj, w, h);
    let top = flatten_path(&def.paths[0], &evaluator, w, h);
    if top.len() < 2 {
        return None;
    }
    let single_edge = def.paths.len() == 1;
    let bottom = if single_edge {
        top.clone()
    } else {
        flatten_path(def.paths.last()?, &evaluator, w, h)
    };
    if bottom.len() < 2 {
        return None;
    }
    let top = WarpPolyline {
        cumulative: cumulative(&top),
        points: top,
    };
    let bottom = WarpPolyline {
        cumulative: cumulative(&bottom),
        points: bottom,
    };
    Some(WarpEnvelope {
        top,
        bottom,
        single_edge,
    })
}

/// Follow Path scale for single-edge arch/circle presets.
pub fn follow_path_u_scale(envelope: &WarpEnvelope, natural_width: f32) -> f32 {
    if !envelope.single_edge {
        return 1.0;
    }
    let arc = envelope.arc_len();
    if arc <= 0.0 {
        1.0
    } else {
        (natural_width / arc).clamp(0.0, 1.0)
    }
}

fn sample(polyline: &WarpPolyline, u: f32) -> (f32, f32, f32, f32) {
    let total = polyline.cumulative.last().copied().unwrap_or(0.0);
    if polyline.points.len() == 1 || total <= 0.0 {
        let point = polyline.points.first().copied().unwrap_or((0.0, 0.0));
        return (point.0, point.1, 1.0, 0.0);
    }
    let target = u.clamp(0.0, 1.0) * total;
    let mut lo = 0usize;
    let mut hi = polyline.cumulative.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if polyline.cumulative[mid] <= target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let segment = (polyline.cumulative[hi] - polyline.cumulative[lo]).max(f32::EPSILON);
    let fraction = (target - polyline.cumulative[lo]) / segment;
    let a = polyline.points[lo];
    let b = polyline.points[hi];
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let norm = dx.hypot(dy).max(f32::EPSILON);
    (a.0 + dx * fraction, a.1 + dy * fraction, dx / norm, dy / norm)
}

/// Exact paired-edge envelope map `P(u, v) = (1 − v)·T(u) + v·B(u)`.
///
/// This is the continuous warp the reference approximates with its piecewise
/// affine glyph STRIPS (`renderer.ts:3320` `drawWarpedGlyphStrips`): each strip
/// re-anchors the local Jacobian at its own centre-`u`, so as the strip width
/// shrinks the composite converges to exactly this map. Evaluating it per
/// outline point gives the smooth limit directly, with no strip subdivision and
/// no double-counted advance (a strip's `x` offset is already contained in
/// `T(u)`'s motion across the glyph).
///
/// `u` is the arc-length fraction along the flat line, `v` the fraction across
/// the envelope's height (0 = top edge, 1 = bottom edge). Returns `None` for
/// single-edge presets, which have no interior band; callers use
/// [`warp_glyph_transform`] for those (ref `text-warp.ts:319`).
pub fn warp_point(envelope: &WarpEnvelope, u: f32, v: f32) -> Option<(f32, f32)> {
    if envelope.single_edge {
        return None;
    }
    let (top_x, top_y, _, _) = sample(&envelope.top, u);
    let (bottom_x, bottom_y, _, _) = sample(&envelope.bottom, u);
    Some((top_x + (bottom_x - top_x) * v, top_y + (bottom_y - top_y) * v))
}

/// Map a flat glyph baseline into the envelope's local jacobian.
pub fn warp_glyph_transform(
    envelope: &WarpEnvelope,
    u: f32,
    box_height: f32,
    baseline_fraction: f32,
) -> Option<GlyphTransform> {
    let u = u.clamp(0.0, 1.0);
    let box_height = box_height.max(1.0);
    if envelope.single_edge {
        let (x, y, tx, ty) = sample(&envelope.top, u);
        let descent = box_height * (1.0 - baseline_fraction.clamp(0.0, 1.0));
        return Some(GlyphTransform {
            x: x - ty * descent,
            y: y + tx * descent,
            angle: ty.atan2(tx),
            scale_x: 1.0,
            scale_y: 1.0,
            shear: 0.0,
        });
    }

    let (top_x, top_y, top_tx, top_ty) = sample(&envelope.top, u);
    let (bottom_x, bottom_y, bottom_tx, bottom_ty) = sample(&envelope.bottom, u);
    let gap_x = bottom_x - top_x;
    let gap_y = bottom_y - top_y;
    let baseline = baseline_fraction.clamp(0.0, 1.0);
    let angle = (top_ty + bottom_ty).atan2(top_tx + bottom_tx);
    let cos = angle.cos();
    let sin = angle.sin();
    let gap_along = (cos * gap_x + sin * gap_y) / box_height;
    let gap_perp = (-sin * gap_x + cos * gap_y) / box_height;
    let (scale_y, shear) = if gap_perp.abs() > 1e-6 {
        (gap_perp, gap_along / gap_perp)
    } else {
        ((gap_x.hypot(gap_y) / box_height).max(0.01), 0.0)
    };
    Some(GlyphTransform {
        x: top_x + gap_x * baseline,
        y: top_y + gap_y * baseline,
        angle,
        scale_x: 1.0,
        scale_y,
        shear,
    })
}

/// Expand a single-edge envelope radially by `offset` pixels from its centroid.
///
/// PowerPoint anchors the LAST line on the authored arch/circle and expands
/// preceding lines outward by their line pitch. A paired-edge envelope already
/// fills its box and is returned unchanged. ref `renderer.ts:3236-3305`
/// (`expandedEnvelopeForLine`) and `wordart-follow-path.test.ts:159-250`.
pub fn warp_envelope_for_line(envelope: &WarpEnvelope, offset: f32) -> WarpEnvelope {
    if !envelope.single_edge || offset.abs() <= 1e-6 {
        return envelope.clone();
    }
    let points = &envelope.top.points;
    if points.is_empty() {
        return envelope.clone();
    }
    let count = points.len() as f32;
    let center = (
        points.iter().map(|point| point.0).sum::<f32>() / count,
        points.iter().map(|point| point.1).sum::<f32>() / count,
    );
    let expand = |polyline: &WarpPolyline| {
        let mut expanded = Vec::with_capacity(polyline.points.len());
        for &(x, y) in &polyline.points {
            let (dx, dy) = (x - center.0, y - center.1);
            let length = dx.hypot(dy);
            if length <= 1e-6 {
                expanded.push((x, y));
            } else {
                expanded.push((x + dx / length * offset, y + dy / length * offset));
            }
        }
        WarpPolyline {
            cumulative: cumulative(&expanded),
            points: expanded,
        }
    };
    let top = expand(&envelope.top);
    let bottom = if envelope.single_edge {
        top.clone()
    } else {
        expand(&envelope.bottom)
    };
    WarpEnvelope {
        top,
        bottom,
        single_edge: envelope.single_edge,
    }
}
