//! PowerPoint's two line models beyond the plain baseline split.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts:335-356`
//!   `powerPointCompatOffNaturalLine` — the #1604 Excel natural box selected
//!   by an explicit `a:bodyPr@compatLnSpc="0"`.
//! - `:372-414` `POWERPOINT_FONT_ALGN_UNIT_PT = 0.72`.
//! - `:416-469` `powerPointFontAlgnReference` / `powerPointFontAlgnOffset`:
//!   `a:pPr@fontAlgn` `t` / `ctr` / `b` place each run's box at the line's
//!   alignment reference instead of a baseline, and every offset is a whole
//!   export unit (1/100 in = 0.72 pt at the specs' scale).
//!
//! Both models need the pinned face catalogue ([`super::faces`]): a run whose
//! face has no box for the model makes the whole body fall back to the
//! ordinary line model (`metricOk = false`, `renderer.ts:5401-5428`), which is
//! why every lookup here returns `Option`.

use crate::text::faces::{ExcelLineBox, FaceMetricsFull};
use crate::text::metrics::{self, FaceMetrics};

/// `POWERPOINT_FONT_ALGN_UNIT_PT` (`powerpoint-line-metrics.ts:381`): the
/// reference PDF export's device unit, in points. Kept in f64 like the JS
/// reference (`0.72f32` would widen to `0.720000028…` and skew every rounded
/// offset by up to half an export unit).
pub const FONT_ALGN_UNIT_PT: f64 = 0.72;

/// One run's contribution to a `fontAlgn` line (`PowerPointAlignedRun`).
#[derive(Clone, Copy, Debug)]
pub struct AlignedRun<'a> {
    /// Authored size in layout units (px at the caller's scale).
    pub size_px: f32,
    pub face: &'a FaceMetricsFull,
}

impl<'a> AlignedRun<'a> {
    pub fn new(size_px: f32, face: &'a FaceMetricsFull) -> Self {
        Self { size_px, face }
    }
}

/// `powerPointCompatOffNaturalLine` (`powerpoint-line-metrics.ts:335-356`):
/// under an explicit `compatLnSpc="0"` each run claims its own #1604 natural
/// box, and the line is the largest ascent over the largest descent — with no
/// rescale to `1.2 × size`.
pub fn compat_off_natural_line(runs: &[(f32, ExcelLineBox)]) -> (f32, f32) {
    let mut ascent = 0.0f64;
    let mut descent = 0.0f64;
    for &(size_px, box_) in runs {
        ascent = ascent.max(box_.ascent as f64 * size_px as f64);
        descent = descent.max(box_.descent as f64 * size_px as f64);
    }
    (ascent as f32, descent as f32)
}

/// The box a run takes for the active model: the glyph box in the default
/// model, the #1604 Excel box under `compatLnSpc="0"`.
fn run_box(run: &AlignedRun<'_>, compat_off: bool) -> Option<ExcelLineBox> {
    if compat_off {
        run.face.excel
    } else {
        run.face.glyph
    }
}

/// `fontAlgn` values that move runs off the baseline (`ST_TextFontAlignType`,
/// ECMA-376 §20.1.10.62). Omitted, `auto` and `base` lay out identically and
/// keep the baseline model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontAlgn {
    /// `t` — the run's box top sits on the reference.
    Top,
    /// `ctr` — the run's box centre sits on the reference.
    Center,
    /// `b` — the midpoint of the run's descent sits on the reference.
    Bottom,
}

impl FontAlgn {
    /// The three values the reference aligns by; anything else (`auto`, `base`,
    /// omitted) is not a `fontAlgn` line.
    pub fn of(value: &str) -> Option<Self> {
        match value {
            "t" => Some(FontAlgn::Top),
            "ctr" => Some(FontAlgn::Center),
            "b" => Some(FontAlgn::Bottom),
            _ => None,
        }
    }
}

/// `powerPointFontAlgnReference` (`powerpoint-line-metrics.ts:416-460`): the
/// line box split at the alignment reference.
///
/// - Default model: the line is the usual `1.2 × largest size`; `t` references
///   its top and `ctr` its middle. `b` references the midpoint between the
///   line's #1610 baseline (rounded to the export unit) and its bottom.
/// - `compatLnSpc="0"`: every run's #1604 box is aligned at the reference
///   (`t` tops, `ctr` centres, `b` descent midpoints) and the line is their
///   union.
///
/// `lnSpc` then re-divides this box with the shared DrawingML rule
/// ([`metrics::spaced_line_box`]), and each run is placed off the reference by
/// [`font_algn_offset`]. `None` when a face has no box for the model.
pub fn font_algn_reference(
    font_algn: FontAlgn,
    runs: &[AlignedRun<'_>],
    compat_off: bool,
    unit_px: f64,
) -> Option<(f32, f32)> {
    if runs.is_empty() {
        return None;
    }
    // The reference runs on JS doubles; computing in f64 keeps the 1.2 em line
    // and the unit-rounded offsets bit-identical to the reference before the
    // final f32 narrowing (`1.2 × 100` is exactly 120 in f64, `120.00001` in f32).
    let mut boxes = Vec::with_capacity(runs.len());
    for run in runs {
        let box_ = run_box(run, compat_off)?;
        boxes.push((
            box_.ascent as f64 * run.size_px as f64,
            box_.descent as f64 * run.size_px as f64,
        ));
    }
    let unit = unit_px;
    if !compat_off {
        // Iterative maxima: a long run can contribute 10^5 entries, too many to
        // spread into a slice maximum (`powerpoint-line-metrics.ts:436-438`).
        let mut largest = 0.0f64;
        for run in runs {
            largest = largest.max(run.size_px as f64);
        }
        let height = 1.2 * largest;
        if font_algn == FontAlgn::Top {
            return Some((0.0, height as f32));
        }
        if font_algn == FontAlgn::Center {
            return Some(((height / 2.0) as f32, (height / 2.0) as f32));
        }
        let shares: Vec<(f32, FaceMetrics)> = runs
            .iter()
            .map(|run| {
                (
                    run.size_px,
                    FaceMetrics {
                        ascent_share: run.face.share,
                    },
                )
            })
            .collect();
        let (natural_ascent, _) = metrics::natural_line_box(&shares);
        let half = (height - (natural_ascent as f64 / unit + 0.5).floor() * unit) / 2.0;
        return Some(((height - half) as f32, half as f32));
    }
    if font_algn == FontAlgn::Bottom {
        let mut above = 0.0f64;
        let mut below = 0.0f64;
        for &(box_ascent, box_descent) in &boxes {
            above = above.max(box_ascent + box_descent / 2.0);
            below = below.max(box_descent / 2.0);
        }
        return Some((above as f32, below as f32));
    }
    let mut height = 0.0f64;
    for &(box_ascent, box_descent) in &boxes {
        height = height.max(box_ascent + box_descent);
    }
    Some(if font_algn == FontAlgn::Top {
        (0.0, height as f32)
    } else {
        ((height / 2.0) as f32, (height / 2.0) as f32)
    })
}

/// `powerPointFontAlgnOffset` (`powerpoint-line-metrics.ts:466-469`): a run's
/// baseline relative to the line's `fontAlgn` reference, positive downwards.
/// `t` puts the run's box top on the reference, `ctr` its centre, `b` the
/// midpoint of its descent. Every offset is a whole export unit; `None` when
/// the face has no box for the model.
pub fn font_algn_offset(
    font_algn: FontAlgn,
    run: &AlignedRun<'_>,
    compat_off: bool,
    unit_px: f64,
) -> Option<f32> {
    let box_ = run_box(run, compat_off)?;
    let unit = unit_px;
    let ascent = box_.ascent as f64 * run.size_px as f64;
    let descent = box_.descent as f64 * run.size_px as f64;
    if font_algn == FontAlgn::Bottom {
        return Some((-round_units(descent / 2.0 / unit) * unit) as f32);
    }
    let offset = if font_algn == FontAlgn::Top {
        ascent
    } else {
        (ascent - descent) / 2.0
    };
    Some((round_units(offset / unit) * unit) as f32)
}

/// `Math.round` (half up) over a finite quotient.
fn round_units(value: f64) -> f64 {
    (value + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(share: f32, glyph: (f32, f32), excel: (f32, f32)) -> FaceMetricsFull {
        FaceMetricsFull {
            share,
            glyph: Some(ExcelLineBox {
                ascent: glyph.0,
                descent: glyph.1,
            }),
            excel: Some(ExcelLineBox {
                ascent: excel.0,
                descent: excel.1,
            }),
            decoration: None,
        }
    }

    #[test]
    fn compat_off_unions_the_excel_boxes_without_rescaling() {
        let a = ExcelLineBox {
            ascent: 0.9,
            descent: 0.2,
        };
        let b = ExcelLineBox {
            ascent: 0.5,
            descent: 0.6,
        };
        // The reference takes the largest ascent (10 = 0.5·20) over the
        // largest descent (12 = 0.6·20); no rescale to 1.2 × size.
        assert_eq!(
            compat_off_natural_line(&[(10.0, a), (20.0, b)]),
            (10.0, 12.0)
        );
        assert_eq!(compat_off_natural_line(&[]), (0.0, 0.0));
    }

    #[test]
    fn font_algn_top_puts_the_glyph_box_top_on_the_reference() {
        let arial = face(0.81, (0.9, 0.2), (0.94, 0.2));
        let run = AlignedRun::new(100.0, &arial);
        assert_eq!(
            font_algn_reference(FontAlgn::Top, &[run], false, 0.72),
            Some((0.0, 120.0))
        );
        assert_eq!(font_algn_offset(FontAlgn::Top, &run, false, 0.72), Some(90.0));
        // The reference is the whole 1.2 em box, not the glyph box.
        assert_eq!(
            font_algn_reference(FontAlgn::Center, &[run], false, 0.72),
            Some((60.0, 60.0))
        );
        assert_eq!(
            font_algn_offset(FontAlgn::Center, &run, false, 0.72),
            Some(35.28)
        );
    }

    #[test]
    fn font_algn_offsets_snap_to_the_export_unit() {
        let arial = face(0.81, (0.9, 0.2), (0.94, 0.2));
        let run = AlignedRun::new(100.0, &arial);
        // 0.2 · 100 / 2 = 10 pt → 13.888… units → 14 units = 10.08 pt down.
        assert_eq!(
            font_algn_offset(FontAlgn::Bottom, &run, false, 0.72),
            Some(-10.08)
        );
        // A face without a box makes the caller keep the baseline model.
        let bare = FaceMetricsFull {
            share: 0.8,
            glyph: None,
            excel: None,
            decoration: None,
        };
        let run = AlignedRun::new(10.0, &bare);
        assert_eq!(font_algn_reference(FontAlgn::Top, &[run], false, 0.72), None);
        assert_eq!(font_algn_offset(FontAlgn::Top, &run, false, 0.72), None);
    }
}
