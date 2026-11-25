//! Row growth, cell text-body normalisation and cell text measurement.
//! ref `packages/pptx/src/renderer.ts:7645-7723`, `5554-5556`.

use std::borrow::Cow;

use crossbeam_channel::Sender;

use crate::compiler::text::compile_text_body;
use crate::contracts::{AffineTransform, VectorCommand, WarningEvent};

/// ECMA-376 §21.1.3.18 / [MS-OE376] §2.1.1347: `a:tr@h` is a MINIMUM row
/// height; PowerPoint grows a row to fit its tallest laid-out cell text.
/// ref renderer.ts:7645-7723
pub(super) fn grown_row_heights(tbl: &pptx_model::TableElement, warning_sink: &Sender<WarningEvent>) -> Vec<f32> {
    let mut row_h: Vec<f32> = tbl.rows.iter().map(|r| r.height as f32).collect();
    let authored_total: i64 = tbl.rows.iter().map(|r| r.height).sum();
    // Slack between the authored frame and the sum of the positive `a:tr@h`
    // minima signals that text-driven growth was present when authored. When
    // the minima already fill the frame, do not invent growth just because the
    // font stack wraps differently. ref renderer.ts:7654-7658
    let growth_signal = tbl.height > authored_total;
    let cols: Vec<f32> = tbl.cols.iter().map(|w| *w as f32).collect();
    let width_of = |ci: usize, span: usize| -> f32 { (0..span).filter_map(|s| cols.get(ci + s)).sum() };

    // Pass 1: single-row cells set their own row's minimum.
    for (ri, row) in tbl.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            if cell.h_merge || cell.v_merge || cell.row_span.max(1) > 1 {
                continue;
            }
            let Some(body) = table_cell_body(cell) else {
                continue;
            };
            if row.height > 0 && !growth_signal {
                continue;
            }
            let Some(slot) = row_h.get_mut(ri) else {
                continue;
            };
            let needed = measure_cell_text(&body, width_of(ci, cell.grid_span.max(1) as usize), warning_sink);
            if needed > *slot {
                *slot = needed;
            }
        }
    }

    // Pass 2: a rowSpan cell distributes its deficit across the rows it covers
    // so the merged area grows without inflating any single row beyond what its
    // own content needs. ref renderer.ts:7682-7710
    for (ri, row) in tbl.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            if cell.h_merge || cell.v_merge {
                continue;
            }
            let span = cell.row_span.max(1) as usize;
            if span <= 1 {
                continue;
            }
            let Some(body) = table_cell_body(cell) else {
                continue;
            };
            let end = (ri + span).min(tbl.rows.len());
            let has_auto_row = tbl.rows[ri..end].iter().any(|r| r.height == 0);
            if !has_auto_row && !growth_signal {
                continue;
            }
            let needed = measure_cell_text(&body, width_of(ci, cell.grid_span.max(1) as usize), warning_sink);
            let covered = (ri..(ri + span).min(row_h.len())).collect::<Vec<_>>();
            let have: f32 = covered.iter().map(|r| row_h[*r]).sum();
            if needed > have {
                let extra = (needed - have) / span as f32;
                for r in covered {
                    row_h[r] += extra;
                }
            }
        }
    }
    row_h
}

/// Content height (EMU) a cell's text body needs at `width_emu` — the parity
/// stand-in for the reference's `renderTextBody(..., measureOnly=true)` return
/// value `tPad + requiredHeight + bPad`. ref renderer.ts:5554-5556, 7659-7668
///
/// `crate::text::emit` exposes no measure-only seam (and must not change), so
/// the laid-out block height is read off the real layout: the same body
/// compiled at `box_h = 0` starts its block at `tIns` under `anchor="t"`, while
/// `anchor="ctr"` centres that block about the same inset, so the first
/// baseline shifts by exactly half the laid-out height
/// (`metrics::block_top`, ref core/src/drawingml-text/metrics.ts:133).
pub(super) fn measure_cell_text(
    body: &pptx_model::TextBody,
    width_emu: f32,
    warning_sink: &Sender<WarningEvent>,
) -> f32 {
    let insets = (body.t_ins + body.b_ins) as f32;
    let box_w = width_emu.max(0.0).round() as i64;
    let transform = AffineTransform::identity();
    let baseline_at = |anchor: &str| -> Option<f32> {
        let mut probe = body.clone();
        probe.vertical_anchor = anchor.to_string();
        first_baseline(&compile_text_body(&probe, box_w, 0, &transform, None, warning_sink))
    };
    match (baseline_at("t"), baseline_at("ctr")) {
        (Some(top), Some(ctr)) => (insets + 2.0 * (top - ctr)).max(0.0),
        // No glyphs (empty body, or a warp body the probe cannot read):
        // the reference returns the padding alone.
        _ => insets,
    }
}

/// First (topmost) glyph baseline of a compiled body, in EMU.
pub(super) fn first_baseline(commands: &[VectorCommand]) -> Option<f32> {
    let mut min_y: Option<f32> = None;
    for cmd in commands {
        if let VectorCommand::DrawText { runs, .. } = cmd {
            for glyph in runs.iter().flat_map(|run| run.glyphs.iter()) {
                min_y = Some(min_y.map_or(glyph.y, |m: f32| m.min(glyph.y)));
            }
        }
    }
    min_y
}

/// PowerPoint ignores the `spcFirstLastPara` edge exception inside `a:tc`:
/// a first paragraph's `spcBef` and a last paragraph's `spcAft` still move
/// glyphs. Applied to BOTH measurement and paint.
/// ref renderer.ts:7591-7602
pub(super) fn table_cell_body(cell: &pptx_model::TableCell) -> Option<Cow<'_, pptx_model::TextBody>> {
    let body = cell.text_body.as_ref()?;
    if body.spc_first_last_para {
        let mut owned = body.clone();
        owned.spc_first_last_para = false;
        Some(Cow::Owned(owned))
    } else {
        Some(Cow::Borrowed(body))
    }
}
