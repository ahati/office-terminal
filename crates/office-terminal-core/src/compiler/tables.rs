//! DrawingML table (`<a:tbl>`) compiler — 1:1 parity port.
//! ref `packages/pptx/src/renderer.ts:7591-8110` (`renderTable`).
//!
//! Two paint passes: (1) table background + every cell's fill and text body,
//! (2) every border. A neighbouring cell's fill must never overpaint the half
//! of a shared gridline an adjacent cell already drew (renderer.ts:7779-7792);
//! each gridline is painted EXACTLY ONCE via single ownership, resolved
//! between the two facing strokes with a deterministic rule
//! (`table-border-conflict.ts:79`).
//!
//! Cell text is clipped to the cell's x interval unless the model asks for
//! `horzOverflow="overflow"` (`clip.rs`, ref renderer.ts:7843-7898). `cmpd`
//! compound borders paint as rails (`compound.rs`, ref renderer.ts:7951-7973).
//!
//! Out of IR scope (reported, not painted): `miter_limit` and device-pixel
//! crisp nudging (`renderer.ts:7921`).

mod border_conflict;
mod clip;
mod compound;
mod growth;

use border_conflict::resolve_border_conflict;
use crossbeam_channel::Sender;

use super::style::{compile_fill, compile_stroke};
use super::text::compile_text_body;
use super::transform::compute_shape_transform;
use crate::contracts::{
    AffineTransform, FidelityClass, FillRule, PathSegment, VectorCommand, VectorPath, WarningEvent,
};

/// One non-continuation `<a:tc>` with its physical box on the grown grid.
/// ref renderer.ts:7700-7706 (`jobs[]`).
struct CellJob<'a> {
    cell: &'a pptx_model::TableCell,
    col_x: f32,
    row_y: f32,
    cell_w: f32,
    cell_h: f32,
    /// Grid column of the anchor slot (`hMerge`/`vMerge` continuations skipped).
    ci: usize,
    ri: usize,
    span: usize,
    last_ri: usize,
}

/// Grown grid geometry plus the slot -> job occupancy map.
/// ref renderer.ts:7707-7734 (`colLeft`, `rowTop`, `occupancy`).
struct Grid<'a> {
    jobs: Vec<CellJob<'a>>,
    occupancy: Vec<Vec<i32>>,
    col_w: Vec<f32>,
    col_left: Vec<f32>,
    row_top: Vec<f32>,
    row_h: Vec<f32>,
    rtl: bool,
}

impl<'a> Grid<'a> {
    fn new(tbl: &'a pptx_model::TableElement, row_h: Vec<f32>) -> Self {
        let col_w: Vec<f32> = tbl.cols.iter().map(|w| *w as f32).collect();
        let num_cols = col_w.len();
        // ECMA-376 §21.1.3.13: rtl places column 0 at the right edge, columns
        // advancing leftward. A coordinate flip, never `scale(-1,1)`.
        // ref renderer.ts:7739-7754
        let table_w: f32 = col_w.iter().sum();
        let mut col_left = vec![0.0f32; num_cols];
        if tbl.rtl {
            let mut right = table_w;
            for ci in 0..num_cols {
                right -= col_w[ci];
                col_left[ci] = right;
            }
        } else {
            let mut left = 0.0;
            for ci in 0..num_cols {
                col_left[ci] = left;
                left += col_w[ci];
            }
        }
        let mut row_top = vec![0.0f32; row_h.len()];
        let mut y = 0.0;
        for (ri, h) in row_h.iter().enumerate() {
            row_top[ri] = y;
            y += h;
        }

        let mut grid = Self {
            jobs: Vec::new(),
            occupancy: vec![vec![-1i32; num_cols]; row_h.len()],
            col_w,
            col_left,
            row_top,
            row_h,
            rtl: tbl.rtl,
        };

        for (ri, row) in tbl.rows.iter().enumerate() {
            if ri >= grid.row_h.len() {
                break;
            }
            for (ci, cell) in row.cells.iter().enumerate() {
                // Merged-cell continuations are drawn by their anchor cell.
                if cell.h_merge || cell.v_merge {
                    continue;
                }
                let span = cell.grid_span.max(1) as usize;
                let rspan = cell.row_span.max(1) as usize;
                let cell_w = grid.spanned_width(ci, span);
                let cell_h: f32 = (0..rspan).filter_map(|s| grid.row_h.get(ri + s)).sum();
                let col_x = grid.spanned_left(ci, span);
                let last_ri = (ri + rspan - 1).min(grid.row_h.len() - 1);
                let idx = grid.jobs.len() as i32;
                let row_y = grid.row_top[ri];
                grid.jobs.push(CellJob {
                    cell,
                    col_x,
                    row_y,
                    cell_w,
                    cell_h,
                    ci,
                    ri,
                    span,
                    last_ri,
                });
                // A spanning cell owns every slot it covers, so interior-edge
                // neighbour lookups resolve back to this anchor job.
                #[allow(clippy::needless_range_loop)]
                for rj in ri..=last_ri {
                    for cj in ci..(ci + span).min(num_cols) {
                        grid.occupancy[rj][cj] = idx;
                    }
                }
            }
        }
        grid
    }

    fn num_cols(&self) -> usize {
        self.col_w.len()
    }

    /// Spanned width starting at grid column `ci`; off-grid columns add 0,
    /// mirroring `colWidths[ci + s] ?? 0`. ref renderer.ts:7612-7616
    fn spanned_width(&self, ci: usize, span: usize) -> f32 {
        (0..span).filter_map(|s| self.col_w.get(ci + s)).sum()
    }

    /// Left physical edge of a span; under RTL a span grows leftward, so the
    /// merged cell's left edge is the leftmost covered column.
    /// ref renderer.ts:7756-7758
    fn spanned_left(&self, ci: usize, span: usize) -> f32 {
        let idx = if self.rtl { ci + span.saturating_sub(1) } else { ci };
        self.col_left.get(idx).copied().unwrap_or(0.0)
    }

    /// Job index covering slot (`ri`, `ci`), or -1 when empty / off-grid.
    /// ref renderer.ts:7844-7850 (`jobAt`).
    fn slot(&self, ri: usize, ci: i64) -> i32 {
        if ci < 0 {
            return -1;
        }
        self.occupancy
            .get(ri)
            .and_then(|row| row.get(ci as usize))
            .copied()
            .unwrap_or(-1)
    }

    fn job_at(&self, ri: i64, ci: i64) -> Option<&CellJob<'a>> {
        if ri < 0 {
            return None;
        }
        let idx = self.slot(ri as usize, ci);
        if idx < 0 {
            None
        } else {
            self.jobs.get(idx as usize)
        }
    }
}

pub fn compile_table_element(
    tbl: &pptx_model::TableElement,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let transform = compute_shape_transform(
        tbl.x,
        tbl.y,
        tbl.width,
        tbl.height,
        tbl.rotation,
        tbl.flip_h,
        tbl.flip_v,
    );

    let row_h = growth::grown_row_heights(tbl, warning_sink);
    let grid = Grid::new(tbl, row_h);

    let mut commands = Vec::new();
    // tblBg uses the full grown table rectangle, once, below alpha cell fills.
    paint_table_background(tbl, &grid, &transform, &mut commands);
    paint_cell_fills_and_text(&grid, &transform, warning_sink, &mut commands);
    paint_cell_borders(&grid, &transform, warning_sink, &mut commands);
    commands
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

/// ref renderer.ts:7760-7790 (`paintTableFill` for `tblBg`).
fn paint_table_background(
    tbl: &pptx_model::TableElement,
    grid: &Grid<'_>,
    transform: &AffineTransform,
    out: &mut Vec<VectorCommand>,
) {
    let Some(paint) = tbl.background.as_ref().and_then(|fill| compile_fill(fill, 1.0)) else {
        return;
    };
    let rect = rect_path(0.0, 0.0, grid.col_w.iter().sum(), grid.row_h.iter().sum());
    out.push(VectorCommand::draw_path(rect, Some(paint), None, *transform));
}

/// Pass 1: every cell's fill, then its text body — wrapped in the cell's
/// horizontal clip when the model does not opt out. No border is painted here.
/// ref renderer.ts:7793-7838, 7879-7899
fn paint_cell_fills_and_text(
    grid: &Grid<'_>,
    transform: &AffineTransform,
    warning_sink: &Sender<WarningEvent>,
    out: &mut Vec<VectorCommand>,
) {
    for job in &grid.jobs {
        if let Some(paint) = job.cell.fill.as_ref().and_then(|fill| compile_fill(fill, 1.0)) {
            let rect = rect_path(job.col_x, job.row_y, job.cell_w, job.cell_h);
            out.push(VectorCommand::draw_path(rect, Some(paint), None, *transform));
        }
        if let Some(body) = growth::table_cell_body(job.cell) {
            // ctx.save() / clip / restore is PushClip / PopClip around the body.
            let clip = clip::cell_clip(job, grid, transform);
            if let Some(path) = &clip {
                out.push(VectorCommand::PushClip(path.clone()));
            }
            let cell_transform = transform.post_concat(&AffineTransform::translate(job.col_x, job.row_y));
            out.extend(compile_text_body(
                &body,
                job.cell_w.round() as i64,
                job.cell_h.round() as i64,
                &cell_transform,
                job.cell.text_color.as_deref(),
                warning_sink,
            ));
            if clip.is_some() {
                out.push(VectorCommand::PopClip);
            }
        }
    }
}

/// Pass 2: borders on top of every fill. Gated by single ownership so each
/// gridline is painted once. ref renderer.ts:7842-7985
fn paint_cell_borders(
    grid: &Grid<'_>,
    transform: &AffineTransform,
    warning_sink: &Sender<WarningEvent>,
    out: &mut Vec<VectorCommand>,
) {
    let num_cols = grid.num_cols();
    let num_rows = grid.row_h.len();
    for job in &grid.jobs {
        let cell = job.cell;
        // Map LOGICAL border specs onto PHYSICAL sides. Under RTL logical
        // column 0 sits at the physical RIGHT, so `borderL` paints on the
        // physical-right edge and the facing neighbour is a LOWER column.
        // ref renderer.ts:7845-7856
        let (phys_left, phys_right) = if grid.rtl {
            (cell.border_r.as_ref(), cell.border_l.as_ref())
        } else {
            (cell.border_l.as_ref(), cell.border_r.as_ref())
        };
        let left_outer = if grid.rtl {
            job.ci + job.span >= num_cols
        } else {
            job.ci == 0
        };
        let right_outer = if grid.rtl {
            job.ci == 0
        } else {
            job.ci + job.span >= num_cols
        };
        let right_nbr_ci: i64 = if grid.rtl {
            job.ci as i64 - 1
        } else {
            (job.ci + job.span) as i64
        };

        // TOP: only the outer top row draws its own top; interior tops are
        // owned by the cell above (its bottom edge). ref renderer.ts:7861-7863
        if job.ri == 0 {
            if let Some(spec) = cell.border_t.as_ref() {
                let y = job.row_y;
                stroke_segment(
                    out,
                    transform,
                    spec,
                    warning_sink,
                    (job.col_x, y),
                    (job.col_x + job.cell_w, y),
                );
            }
        }
        // PHYSICAL LEFT: outer columns only; interior lefts are owned by the
        // physically-left neighbour. ref renderer.ts:7866-7868
        if left_outer {
            if let Some(spec) = phys_left {
                let x = job.col_x;
                stroke_segment(
                    out,
                    transform,
                    spec,
                    warning_sink,
                    (x, job.row_y),
                    (x, job.row_y + job.cell_h),
                );
            }
        }
        // BOTTOM: outer bottom row draws its own spec; interior edges are
        // resolved against the below neighbour, subdivided at each neighbour
        // boundary so a wider above-cell cannot swallow a finer below-cell's
        // border (#824). ref renderer.ts:7870-7897
        let bottom_y = job.row_y + job.cell_h;
        if job.last_ri + 1 >= num_rows {
            if let Some(spec) = cell.border_b.as_ref() {
                stroke_segment(
                    out,
                    transform,
                    spec,
                    warning_sink,
                    (job.col_x, bottom_y),
                    (job.col_x + job.cell_w, bottom_y),
                );
            }
        } else {
            let below_ri = job.last_ri + 1;
            let c_end_max = (job.ci + job.span).min(num_cols);
            let mut cj = job.ci;
            while cj < c_end_max {
                let idx = grid.slot(below_ri, cj as i64);
                let mut c_end = cj + 1;
                while c_end < c_end_max && grid.slot(below_ri, c_end as i64) == idx {
                    c_end += 1;
                }
                let below = grid.job_at(below_ri as i64, cj as i64);
                let facing = below.and_then(|b| b.cell.border_t.as_ref());
                if let Some(spec) = resolve_border_conflict(cell.border_b.as_ref(), facing) {
                    let left = grid.spanned_left(cj, c_end - cj);
                    let width = grid.spanned_width(cj, c_end - cj);
                    stroke_segment(
                        out,
                        transform,
                        spec,
                        warning_sink,
                        (left, bottom_y),
                        (left + width, bottom_y),
                    );
                }
                cj = c_end;
            }
        }
        // PHYSICAL RIGHT: outer columns draw their own spec; interior edges are
        // resolved against the physically-right neighbour, subdivided at each
        // neighbour boundary down the rows this cell spans. ref renderer.ts:7899-7925
        let right_x = job.col_x + job.cell_w;
        if right_outer {
            if let Some(spec) = phys_right {
                stroke_segment(
                    out,
                    transform,
                    spec,
                    warning_sink,
                    (right_x, job.row_y),
                    (right_x, job.row_y + job.cell_h),
                );
            }
        } else {
            let mut rj = job.ri;
            while rj <= job.last_ri {
                let idx = grid.slot(rj, right_nbr_ci);
                let mut r_end = rj;
                while r_end < job.last_ri && grid.slot(r_end + 1, right_nbr_ci) == idx {
                    r_end += 1;
                }
                let right = grid.job_at(rj as i64, right_nbr_ci);
                let facing = right.and_then(|b| {
                    if grid.rtl {
                        b.cell.border_r.as_ref()
                    } else {
                        b.cell.border_l.as_ref()
                    }
                });
                if let Some(spec) = resolve_border_conflict(phys_right, facing) {
                    let top = grid.row_top[rj];
                    let bottom = grid.row_top[r_end] + grid.row_h[r_end];
                    stroke_segment(out, transform, spec, warning_sink, (right_x, top), (right_x, bottom));
                }
                rj = r_end + 1;
            }
        }

        // Diagonals are never shared between cells, so they are always drawn
        // per cell. ref renderer.ts:7935-7951
        if let Some(spec) = cell.diagonal_tl.as_ref() {
            stroke_segment(
                out,
                transform,
                spec,
                warning_sink,
                (job.col_x, job.row_y),
                (job.col_x + job.cell_w, job.row_y + job.cell_h),
            );
        }
        if let Some(spec) = cell.diagonal_tr.as_ref() {
            stroke_segment(
                out,
                transform,
                spec,
                warning_sink,
                (job.col_x + job.cell_w, job.row_y),
                (job.col_x, job.row_y + job.cell_h),
            );
        }
    }
}

fn stroke_segment(
    out: &mut Vec<VectorCommand>,
    transform: &AffineTransform,
    spec: &pptx_model::Stroke,
    warning_sink: &Sender<WarningEvent>,
    from: (f32, f32),
    to: (f32, f32),
) {
    let Some(stroke) = compile_stroke(spec) else {
        return;
    };
    // `cmpd="sng"` is the schema default and means a single line, so only a
    // real compound style takes the rail leg. ref renderer.ts:7951-7973
    let compound = spec.cmpd.as_deref().is_some_and(|cmpd| cmpd != "sng");
    if compound && compound::paint_rails(out, transform, spec, &stroke, from, to) {
        return;
    }
    if compound {
        // Dashed, non-solid or diagonal compound rules keep the plain stroke.
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: "a:tbl".to_string(),
            feature: "compound_border",
            class: FidelityClass::Degraded,
        });
    }
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: from.0, y: from.1 },
            PathSegment::LineTo { x: to.0, y: to.1 },
        ],
        FillRule::NonZero,
    );
    out.push(VectorCommand::draw_path(path, None, Some(stroke), *transform));
}
