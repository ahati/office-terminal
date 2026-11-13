//! Shared fixtures for the `text-box-a` parity ports.
//!
//! Split out of the three `parity_text_box_a_*_tests.rs` targets so the face
//! share table, the layout-stack probe and the `TextBody` JSON builder exist
//! once. Not a test target itself (it lives in a directory and is pulled in
//! with `#[path = "text_box_a_support/mod.rs"] mod support;`), so no test runs
//! from here.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts` — `powerPointNaturalLine`
//!   (1.2 × size unioned per face and rescaled), `powerPointExactLinePoints`.
//! - `packages/pptx/src/renderer.ts:5351-5476` — how a line derives its natural
//!   box, its percentage-spacing unit and its `spcFirstLastPara` edges.
//! - `packages/pptx/src/renderer.ts:5530-5575` — `requiredHeight`, the
//!   last-line-natural-descent anchor height and the `t`/`ctr`/`b` cursors.
//! - `packages/core/src/fonts/reference-font-metrics-data.json` — the OS/2
//!   `usWin` / `USE_TYPO_METRICS` numbers behind every share below.
#![allow(dead_code)] // each port file uses a different subset of these helpers

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::text::metrics::{self, FaceMetrics, SpaceLine};

/// The reference export's device unit: one 1/100 in at the specs' 1 pt = 1
/// unit scale (`powerpoint-line-metrics.test.ts:9-16`).
pub const UNIT: f32 = 0.72;

/// Whole points → EMU (1 pt = 12700 EMU).
pub const EMU_PER_PT: f32 = 12_700.0;

/// `Math.floor(y / U + 0.5)` (`powerpoint-line-metrics.test.ts:64-67`).
pub fn units(ys: &[f32]) -> Vec<i32> {
    ys.iter().map(|y| (y / UNIT + 0.5).floor() as i32).collect()
}

pub fn close(actual: f32, expected: f32, what: &str) {
    close_tol(actual, expected, 1e-3, what);
}

pub fn close_tol(actual: f32, expected: f32, tol: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= tol,
        "{what}: expected {expected}, got {actual} (tol {tol})"
    );
}

// ── Reference face shares ───────────────────────────────────────────────────
//
// `powerPointAscentShare` (`powerpoint-line-metrics.ts:115-134`) reads
// `usWinAscent / (usWinAscent + usWinDescent)`, or the typo triplet for a
// `USE_TYPO_METRICS` face. Rust resolves the share from the host face
// (`text/shape.rs:104-160`, an hhea proxy), not from this catalogue, so the
// ports that depend on a face's *catalogued* split pass it in explicitly.

pub const fn win(ascent: f32, descent: f32) -> FaceMetrics {
    FaceMetrics {
        ascent_share: ascent / (ascent + descent),
    }
}

/// `(sTypoAscender + max(0, sTypoLineGap)) / (sTypoAscender − sTypoDescender +
/// sTypoLineGap)` (`powerpoint-line-metrics.ts:119-124`).
pub const fn typo(ascender: f32, descender: f32, line_gap: f32) -> FaceMetrics {
    let above = ascender + line_gap;
    FaceMetrics {
        ascent_share: above / (above - descender),
    }
}

pub const ARIAL: FaceMetrics = win(1854.0, 434.0);
pub const CALIBRI: FaceMetrics = win(1950.0, 550.0);
/// Regular cut (`office-mac`); the Light cut shares the same OS/2 pair.
pub const YU_GOTHIC: FaceMetrics = win(2017.0, 619.0);
pub const GABRIOLA: FaceMetrics = typo(2800.0, -1296.0, 2867.0);
pub const BASKERVILLE: FaceMetrics = win(1805.0, 531.0);
pub const STENCIL: FaceMetrics = win(1860.0, 567.0);
/// `macos-supplemental` copy, the one PowerPoint embeds.
pub const TIMES: FaceMetrics = win(1825.0, 443.0);
pub const GEORGIA: FaceMetrics = win(1878.0, 449.0);
pub const COURIER: FaceMetrics = win(1705.0, 615.0);
pub const MEIRYO: FaceMetrics = win(2171.0, 901.0);
pub const MEIRYO_UI: FaceMetrics = win(2171.0, 430.0);
pub const MS_GOTHIC: FaceMetrics = win(220.0, 36.0);
pub const MS_PGOTHIC: FaceMetrics = win(220.0, 36.0);
/// `Palatino` is substituted by `Palatino Linotype` (`OBSERVED_SUBSTITUTES`).
pub const PALATINO_LINOTYPE: FaceMetrics = win(2150.0, 613.0);
/// `packages/core/src/text/synthetic-em-box.ts:15` — an unresolved face.
pub const SYNTHETIC: FaceMetrics = FaceMetrics { ascent_share: 0.8 };

// ── Layout-stack probe ─────────────────────────────────────────────────────

/// One laid-out line: its metric runs as `(size in pt, face share)`.
#[derive(Clone, Debug)]
pub struct Line(pub Vec<(f32, FaceMetrics)>);

pub fn line(size: f32, face: FaceMetrics) -> Line {
    Line(vec![(size, face)])
}

/// One paragraph: its lines plus the three `EG_TextSpacing` values.
#[derive(Clone, Debug)]
pub struct Para {
    pub lines: Vec<Line>,
    pub space_line: SpaceLine,
    pub before: SpaceLine,
    pub after: SpaceLine,
}

impl Para {
    pub fn of(lines: Vec<Line>) -> Self {
        Self {
            lines,
            space_line: Some((true, 100_000.0)),
            before: None,
            after: None,
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
}

/// A text body laid out by the production kernels in the reference's order.
///
/// This mirrors `renderer.ts:5351-5575` (`buildLayout` + the anchor block),
/// which `text/emit.rs:226-296` also mirrors: per line
/// `natural_line_box` → `spaced_line_box` → `paragraph_spacing` gaps, then the
/// anchor. It exists because the face *shares* here come from the reference
/// catalogue, which production resolves from the host instead.
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
}

struct Entry {
    top_gap: f32,
    ascent: f32,
    descent: f32,
    natural_descent: f32,
    after_gap: f32,
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

    fn entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        let last_para = self.paras.len().saturating_sub(1);
        for (p_index, para) in self.paras.iter().enumerate() {
            let last_line = para.lines.len().saturating_sub(1);
            for (l_index, Line(runs)) in para.lines.iter().enumerate() {
                let sized: Vec<(f32, FaceMetrics)> = runs
                    .iter()
                    .map(|(size, face)| (size * self.font_scale, *face))
                    .collect();
                let (nat_ascent, nat_descent) = metrics::natural_line_box(&sized);
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
                out.push(Entry {
                    top_gap,
                    ascent,
                    descent,
                    natural_descent: nat_descent,
                    after_gap,
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

    /// Every glyph baseline, in points from the box top.
    pub fn baselines(&self) -> Vec<f32> {
        let mut y = self.block_y();
        let mut out = Vec::new();
        for entry in self.entries() {
            y += entry.top_gap + entry.ascent;
            out.push(y);
            y += entry.descent + entry.after_gap;
        }
        out
    }

    pub fn units(&self) -> Vec<i32> {
        units(&self.baselines())
    }
}

// ── `TextBody` compilation probe ───────────────────────────────────────────

/// One authored run: text, size in points, authored face, or a hard break.
#[derive(Clone, Debug)]
pub enum RunSpec {
    Text(&'static str, f64, &'static str),
    Break,
}

/// A run in the body default face.
pub fn text(value: &'static str, size: f64) -> RunSpec {
    RunSpec::Text(value, size, "Calibri")
}

/// A run in an authored face.
pub fn text_face(value: &'static str, size: f64, family: &'static str) -> RunSpec {
    RunSpec::Text(value, size, family)
}

pub const BREAK: RunSpec = RunSpec::Break;

/// One authored paragraph for [`body`].
#[derive(Clone, Debug)]
pub struct ParaSpec {
    pub runs: Vec<RunSpec>,
    pub alignment: &'static str,
    pub mar_l: i64,
    pub indent: i64,
    pub space_line: SpaceLine,
    pub before: SpaceLine,
    pub after: SpaceLine,
    pub bullet: bool,
    pub tab_stop: Option<i64>,
}

pub fn para(runs: Vec<RunSpec>) -> ParaSpec {
    ParaSpec {
        runs,
        alignment: "l",
        mar_l: 0,
        indent: 0,
        space_line: Some((true, 100_000.0)),
        before: None,
        after: None,
        bullet: false,
        tab_stop: None,
    }
}

impl ParaSpec {
    pub fn align(mut self, alignment: &'static str) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn mar_l(mut self, emu: i64) -> Self {
        self.mar_l = emu;
        self
    }

    pub fn indent(mut self, emu: i64) -> Self {
        self.indent = emu;
        self
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

    pub fn bullet(mut self) -> Self {
        self.bullet = true;
        self
    }

    pub fn tab_stop(mut self, emu: i64) -> Self {
        self.tab_stop = Some(emu);
        self
    }
}

/// `a:spcBef` / `a:spcAft`: the two fields are the one exclusive
/// `EG_TextSpacing` choice, so exactly one of `spaceBefore` (1/100 pt) and
/// `spaceBeforePct` (thousandths of a percent) is set.
fn before_json(space: &SpaceLine) -> (String, String) {
    match space {
        Some((false, val)) => (((val * 100.0).round() as i64).to_string(), "null".to_string()),
        Some((true, val)) => ("null".to_string(), val.to_string()),
        None => ("null".to_string(), "null".to_string()),
    }
}

fn space_json(space: &SpaceLine) -> String {
    match space {
        None => "null".to_string(),
        Some((true, val)) => format!(r#"{{"type":"pct","val":{val}}}"#),
        Some((false, val)) => format!(r#"{{"type":"pts","val":{val}}}"#),
    }
}

fn run_json(run: &RunSpec) -> String {
    match run {
        RunSpec::Break => r#"{ "type": "break" }"#.to_string(),
        RunSpec::Text(value, size, family) => format!(
            r#"{{
                "type": "text", "text": "{value}",
                "bold": false, "italic": false,
                "underline": false, "strikethrough": false,
                "strikeDouble": false, "hyperlinkUsesTextFill": false,
                "fontSize": {size}, "color": "000000", "fontFamily": "{family}"
            }}"#
        ),
    }
}

fn para_json(spec: &ParaSpec, def_size: f64) -> String {
    let bullet = if spec.bullet {
        r#"{ "type": "char", "char": "•", "color": null, "sizePct": null, "fontFamily": null }"#
    } else {
        r#"{ "type": "none" }"#
    };
    let stops = match spec.tab_stop {
        Some(pos) => format!(r#"[{{ "pos": {pos}, "algn": "l" }}]"#),
        None => "[]".to_string(),
    };
    let runs: Vec<String> = spec.runs.iter().map(run_json).collect();
    let (before_pts, before_pct) = before_json(&spec.before);
    let (after_pts, after_pct) = before_json(&spec.after);
    format!(
        r#"{{
            "alignment": "{}",
            "marL": {}, "marR": 0, "indent": {},
            "spaceBefore": {before_pts}, "spaceAfter": {after_pts},
            "spaceBeforePct": {before_pct}, "spaceAfterPct": {after_pct},
            "spaceLine": {},
            "lvl": 0, "bullet": {bullet},
            "defFontSize": {def_size},
            "tabStops": {stops}, "defTabSz": null, "rtl": false, "eaLnBrk": true,
            "runs": [{}]
        }}"#,
        spec.alignment,
        spec.mar_l,
        spec.indent,
        space_json(&spec.space_line),
        runs.join(",")
    )
}

/// A `TextBody` with one point per pixel (`scale = 1/12700`), the reference
/// specs' canvas scale. `spc_first_last` is `bodyPr@spcFirstLastPara`.
#[allow(clippy::too_many_arguments)]
pub fn body_with(
    paras: &[ParaSpec],
    anchor: &str,
    box_insets_pt: (f64, f64, f64, f64),
    auto_fit: &str,
    font_scale: Option<f64>,
    reduction: Option<f64>,
    spc_first_last: bool,
    default_size: f64,
) -> pptx_model::TextBody {
    let (l, r, t, b) = box_insets_pt;
    let paragraphs: Vec<String> = paras.iter().map(|p| para_json(p, default_size)).collect();
    let scale = font_scale.map(|v| format!(r#""fontScale": {v},"#)).unwrap_or_default();
    let reduction = reduction
        .map(|v| format!(r#""lnSpcReduction": {v},"#))
        .unwrap_or_default();
    let json = format!(
        r#"{{
            "verticalAnchor": "{anchor}",
            "defaultFontSize": {default_size},
            "lIns": {}, "rIns": {}, "tIns": {}, "bIns": {},
            "wrap": "square", "vert": "horz", "autoFit": "{auto_fit}",
            {scale}{reduction}
            "spcFirstLastPara": {spc_first_last},
            "numCol": 1, "spcCol": 0, "rtlCol": false, "anchorCtr": false,
            "paragraphs": [{}]
        }}"#,
        (l * 12_700.0).round() as i64,
        (r * 12_700.0).round() as i64,
        (t * 12_700.0).round() as i64,
        (b * 12_700.0).round() as i64,
        paragraphs.join(",")
    );
    serde_json::from_str(&json).unwrap_or_else(|err| panic!("body fixture: {err}\n{json}"))
}

/// A body with no insets, `t` anchor and `autoFit="none"`.
pub fn body(paras: &[ParaSpec]) -> pptx_model::TextBody {
    body_with(paras, "t", (0.0, 0.0, 0.0, 0.0), "none", None, None, false, 20.0)
}

/// Compile at the reference scale: `w`/`h` are points.
pub fn compile(text_body: &pptx_model::TextBody, w_pt: f64, h_pt: f64) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        text_body,
        (w_pt * 12_700.0).round() as i64,
        (h_pt * 12_700.0).round() as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    )
}

pub fn drawn_runs(commands: &[VectorCommand]) -> Vec<PositionedGlyphRun> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Every glyph baseline in EMU, keeping only the first glyph of each line
/// (the reference's `draws` list, one entry per `fillText` call).
pub fn glyph_ys(commands: &[VectorCommand]) -> Vec<f32> {
    drawn_runs(commands)
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.y))
        .collect()
}

/// Distinct baselines (lines), in paint order, in EMU.
pub fn line_ys(commands: &[VectorCommand]) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    for y in glyph_ys(commands) {
        if !out.iter().any(|seen| (seen - y).abs() < 1.0) {
            out.push(y);
        }
    }
    out
}

/// Distinct baselines in points.
pub fn line_pts(commands: &[VectorCommand]) -> Vec<f32> {
    line_ys(commands).iter().map(|y| y / EMU_PER_PT).collect()
}

/// The leftmost glyph x (EMU) — the reference's `Math.min(...texts.x)`.
pub fn first_glyph_x(commands: &[VectorCommand]) -> f32 {
    glyph_ys(commands);
    let xs: Vec<f32> = drawn_runs(commands)
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.x))
        .collect();
    xs.into_iter()
        .fold(f32::INFINITY, |low, x| if x < low { x } else { low })
}

/// Per-line leftmost glyph x, in EMU, in baseline order.
pub fn line_first_xs(commands: &[VectorCommand]) -> Vec<f32> {
    let mut lines: Vec<(f32, f32)> = Vec::new();
    for run in drawn_runs(commands) {
        for glyph in &run.glyphs {
            match lines.iter_mut().find(|(y, _)| (*y - glyph.y).abs() < 1.0) {
                Some((_, low)) => {
                    if glyph.x < *low {
                        *low = glyph.x;
                    }
                }
                None => lines.push((glyph.y, glyph.x)),
            }
        }
    }
    lines.into_iter().map(|(_, x)| x).collect()
}
