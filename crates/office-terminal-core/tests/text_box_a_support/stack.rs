//! The two-model line stack the `text-box-a` ports drive.
//!
//! This mirrors `renderer.ts:5351-5575` (`buildLayout` + the anchor block),
//! which `text/emit.rs:226-296` also mirrors: per line, the natural box (the
//! #1610 share model, the #1604 Excel model under `compatLnSpc="0"`, or the
//! `fontAlgn` reference box), then `spaced_line_box`, then the
//! `paragraph_spacing` gaps, then the anchor. It exists because the face
//! *shares* a port pins come from the reference catalogue, which production
//! reads through `text::faces`.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/renderer.ts:5388-5465` (natural box, `pctSpacingUnit`,
//!   `metricNaturalDescent`, `fontAlgn` offsets)
//! - `packages/pptx/src/renderer.ts:5536-5564` (`anchorHeight`, cursors)
//! - `packages/pptx/src/powerpoint-line-metrics.ts:296-469`
//! - `packages/core/src/drawingml-text/metrics.ts` (`drawingMlSpacedLineBox`)

use std::sync::Arc;

use office_terminal_core::text::faces::{self, FaceMetricsFull};
use office_terminal_core::text::metrics::{self, FaceMetrics, SpaceLine};
use office_terminal_core::text::power_point::{self, AlignedRun, FontAlgn};

use super::units;

/// One metric run of a line: its size, the face split, and (for the
/// `compatLnSpc` / `fontAlgn` models) the catalogued family.
#[derive(Clone, Debug)]
pub struct Run {
    pub size: f32,
    pub face: FaceMetrics,
    pub family: Option<&'static str>,
}

impl Run {
    pub fn new(size: f32, face: FaceMetrics) -> Self {
        Self {
            size,
            face,
            family: None,
        }
    }

    /// A run whose face comes from the pinned catalogue by name.
    pub fn named(size: f32, family: &'static str) -> Self {
        Self {
            size,
            face: FaceMetrics::for_family(family, false, false),
            family: Some(family),
        }
    }

    /// The interned face metrics, when the run was built from a family name.
    fn full(&self) -> Option<Arc<FaceMetricsFull>> {
        self.family
            .and_then(|family| faces::power_point_face_metrics(family, false, false))
    }
}

/// One laid-out line: its metric runs plus, after them, the `a:br` /
/// `endParaRPr` mark that contributes a face without drawing a glyph
/// (`followingMark`, `renderer.ts:5341-5346`).
#[derive(Clone, Debug)]
pub struct Line {
    pub runs: Vec<Run>,
    pub mark: Option<Run>,
}

impl Line {
    pub fn of(runs: Vec<(f32, FaceMetrics)>) -> Self {
        Self {
            runs: runs.into_iter().map(|(size, face)| Run::new(size, face)).collect(),
            mark: None,
        }
    }

    /// A line whose runs and mark name catalogued faces.
    pub fn named(runs: Vec<(f32, &'static str)>) -> Self {
        Self {
            runs: runs.into_iter().map(|(size, family)| Run::named(size, family)).collect(),
            mark: None,
        }
    }

    pub fn with_mark(mut self, size: f32, family: &'static str) -> Self {
        self.mark = Some(Run::named(size, family));
        self
    }

    /// Every metric run, mark included.
    fn metric_runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter().chain(self.mark.iter())
    }
}

/// One paragraph: its lines plus the three `EG_TextSpacing` values and
/// `pPr@fontAlgn`.
#[derive(Clone, Debug)]
pub struct Para {
    pub lines: Vec<Line>,
    pub space_line: SpaceLine,
    pub before: SpaceLine,
    pub after: SpaceLine,
    pub font_algn: Option<FontAlgn>,
}

impl Para {
    pub fn of(lines: Vec<Line>) -> Self {
        Self {
            lines,
            space_line: Some((true, 100_000.0)),
            before: None,
            after: None,
            font_algn: None,
        }
    }

    pub fn one(size: f32, face: FaceMetrics) -> Self {
        Self::of(vec![line(size, face)])
    }

    pub fn many(count: usize, size: f32, face: FaceMetrics) -> Vec<Self> {
        (0..count).map(|_| Self::one(size, face)).collect()
    }

    pub fn space_line(mut self, space: SpaceLine) -> Self {
        self.space_line = space;
        self
    }

    pub fn before(mut self, space: SpaceLine) -> Self {
        self.before = space;
        self
    }

    pub fn after(mut self, space: SpaceLine) -> Self {
        self.after = space;
        self
    }

    pub fn font_algn(mut self, algn: FontAlgn) -> Self {
        self.font_algn = Some(algn);
        self
    }
}

/// A one-run line.
pub fn line(size: f32, face: FaceMetrics) -> Line {
    Line::of(vec![(size, face)])
}

/// A text body laid out by the production kernels in the reference's order.
#[derive(Clone)]
pub struct Stack {
    pub paras: Vec<Para>,
    pub anchor: &'static str,
    pub box_top: f32,
    pub box_h: f32,
    pub t_ins: f32,
    pub b_ins: f32,
    pub first_last: bool,
    pub reduction: f32,
    pub font_scale: f32,
    /// `a:bodyPr@compatLnSpc="0"` (the #1604 Excel model).
    pub compat_off: bool,
}

struct Entry {
    top_gap: f32,
    ascent: f32,
    descent: f32,
    natural_descent: f32,
    after_gap: f32,
    /// `fontAlgn` baseline offset per run, in run order.
    offsets: Vec<f32>,
}

impl Stack {
    pub fn new(paras: Vec<Para>) -> Self {
        Self {
            paras,
            anchor: "t",
            box_top: 0.0,
            box_h: 0.0,
            t_ins: 0.0,
            b_ins: 0.0,
            first_last: false,
            reduction: 0.0,
            font_scale: 1.0,
            compat_off: false,
        }
    }

    pub fn in_box(mut self, box_top: f32, box_h: f32, t_ins: f32, b_ins: f32) -> Self {
        self.box_top = box_top;
        self.box_h = box_h;
        self.t_ins = t_ins;
        self.b_ins = b_ins;
        self
    }

    pub fn anchor(mut self, anchor: &'static str) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn first_last(mut self, value: bool) -> Self {
        self.first_last = value;
        self
    }

    pub fn reduction(mut self, value: f32) -> Self {
        self.reduction = value;
        self
    }

    pub fn scale(mut self, value: f32) -> Self {
        self.font_scale = value;
        self
    }

    /// An explicit `a:bodyPr@compatLnSpc="0"` (#1604 Excel model).
    pub fn compat_off(mut self, value: bool) -> Self {
        self.compat_off = value;
        self
    }

    /// The aligned runs of one line, plus the per-run offsets.
    fn aligned(&self, para: &Para, line: &Line) -> (Option<(f32, f32)>, Vec<f32>) {
        let Some(algn) = para.font_algn else {
            return (None, Vec::new());
        };
        let fulls: Vec<Arc<FaceMetricsFull>> = line
            .metric_runs()
            .map(|run| {
                run.full()
                    .or_else(|| run.family.and_then(|f| faces::power_point_face_metrics(f, false, false)))
                    .unwrap_or_else(|| panic!("fontAlgn run needs a catalogued family"))
            })
            .collect();
        let sizes: Vec<f32> = line
            .metric_runs()
            .map(|run| run.size * self.font_scale)
            .collect();
        let runs: Vec<AlignedRun<'_>> = sizes
            .iter()
            .zip(fulls.iter())
            .map(|(&size, full)| AlignedRun::new(size, full))
            .collect();
        let reference = power_point::font_algn_reference(algn, &runs, self.compat_off, 0.72)
            .unwrap_or_else(|| panic!("fontAlgn reference needs a face box"));
        let offsets = runs
            .iter()
            .map(|run| {
                power_point::font_algn_offset(algn, run, self.compat_off, 0.72)
                    .unwrap_or_else(|| panic!("fontAlgn offset needs a face box"))
            })
            .collect();
        (Some(reference), offsets)
    }

    /// The natural (pre-`lnSpc`) box of one line in the active model.
    fn natural(&self, line: &Line) -> (f32, f32) {
        if self.compat_off {
            let boxes: Vec<(f32, faces::ExcelLineBox)> = line
                .metric_runs()
                .map(|run| {
                    let full = run.full().unwrap_or_else(|| {
                        panic!("compatLnSpc=\"0\" needs a catalogued family")
                    });
                    (
                        run.size * self.font_scale,
                        full.excel.expect("catalogued Excel box"),
                    )
                })
                .collect();
            return power_point::compat_off_natural_line(&boxes);
        }
        let shares: Vec<(f32, FaceMetrics)> = line
            .metric_runs()
            .map(|run| (run.size * self.font_scale, run.face))
            .collect();
        metrics::natural_line_box(&shares)
    }

    fn entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        let last_para = self.paras.len().saturating_sub(1);
        for (p_index, para) in self.paras.iter().enumerate() {
            let last_line = para.lines.len().saturating_sub(1);
            for (l_index, line) in para.lines.iter().enumerate() {
                let aligned = self.aligned(para, line);
                let (nat_ascent, nat_descent) = aligned.0.unwrap_or_else(|| self.natural(line));
                let (ascent, descent) = metrics::spaced_line_box(
                    nat_ascent,
                    nat_descent,
                    para.space_line,
                    1.0,
                    self.reduction,
                );
                let natural = nat_ascent + nat_descent;
                let first = l_index == 0;
                let last = l_index == last_line;
                // renderer.ts:5462-5476: the first paragraph's spcBef and the
                // last paragraph's spcAft move no glyph unless
                // `spcFirstLastPara="1"`.
                let top_gap = if first && (self.first_last || p_index > 0) {
                    metrics::paragraph_spacing(para.before, natural, 1.0)
                } else {
                    0.0
                };
                let after_gap = if last && (self.first_last || p_index < last_para) {
                    metrics::paragraph_spacing(para.after, natural, 1.0)
                } else {
                    0.0
                };
                let mut offsets = aligned.1;
                if offsets.is_empty() {
                    offsets = vec![0.0; line.runs.len().max(1)];
                } else {
                    // A mark contributes to the line box but draws no glyph.
                    offsets.truncate(line.runs.len());
                }
                out.push(Entry {
                    top_gap,
                    ascent,
                    descent,
                    natural_descent: nat_descent,
                    after_gap,
                    offsets,
                });
            }
        }
        out
    }

    /// `totalHeight` — the sum of the spaced lines and the applied gaps.
    pub fn needed_height(&self) -> f32 {
        self.entries()
            .iter()
            .map(|e| e.top_gap + e.ascent + e.descent + e.after_gap)
            .sum()
    }

    /// `anchorHeight` (`renderer.ts:5536-5541`): PowerPoint anchors the block by
    /// the LAST line's natural descent, so a spacing change below the last
    /// baseline does not move a bottom-anchored block.
    pub fn anchor_height(&self) -> f32 {
        let entries = self.entries();
        let total: f32 = entries
            .iter()
            .map(|e| e.top_gap + e.ascent + e.descent + e.after_gap)
            .sum();
        match entries.last() {
            Some(last) => total - last.descent + last.natural_descent,
            None => total,
        }
    }

    /// The cursor the anchor block starts at (`renderer.ts:5546-5564`). Unlike
    /// `metrics::block_top`, `anchor="b"` is NOT clamped: a block taller than
    /// the box overflows both edges (`renderer.ts:5559-5560`).
    pub fn block_y(&self) -> f32 {
        let height = self.anchor_height();
        let content = (self.box_h - self.t_ins - self.b_ins).max(0.0);
        match self.anchor {
            "ctr" => self.box_top + self.t_ins + (content - height) / 2.0,
            "b" => self.box_top + self.box_h - self.b_ins - height,
            _ => self.box_top + self.t_ins,
        }
    }

    /// Every run baseline, in run order, in points from the box top.
    pub fn run_baselines(&self) -> Vec<f32> {
        let mut y = self.block_y();
        let mut out = Vec::new();
        for entry in self.entries() {
            y += entry.top_gap + entry.ascent;
            out.extend(entry.offsets.iter().map(|offset| y + offset));
            y += entry.descent + entry.after_gap;
        }
        out
    }

    /// The baseline of each line's first run, in points from the box top.
    pub fn baselines(&self) -> Vec<f32> {
        let mut y = self.block_y();
        let mut out = Vec::new();
        for entry in self.entries() {
            y += entry.top_gap + entry.ascent;
            out.push(y + entry.offsets.first().copied().unwrap_or(0.0));
            y += entry.descent + entry.after_gap;
        }
        out
    }

    pub fn units(&self) -> Vec<i32> {
        units(&self.baselines())
    }

    /// Every run baseline in export units (`Math.floor(y / U + 0.5)`).
    pub fn run_units(&self) -> Vec<i32> {
        units(&self.run_baselines())
    }
}
