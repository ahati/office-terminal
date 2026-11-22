//! Cell horizontal clipping (ECMA-376 §21.1.3.17 / §20.1.10.69).
//!
//! The reference wraps a cell's text in a horizontal-only clip: the clip is the
//! cell's physical x interval projected across the whole device viewport, so
//! vertical overflow still shows. ref `renderer.ts:7843-7898` and
//! `packages/core/src/internal/canvas-clip.ts:1-40` (`clipCanvasHorizontally`).
//!
//! The IR clip is a path, not an interval, so the same shape is expressed as a
//! rectangle spanning the cell's x range and a tall band around the table: the
//! band is the table's own height above and below it, which no glyph run can
//! outgrow because a row already grows to fit its content (`growth.rs`).

use crate::contracts::{AffineTransform, PathSegment, VectorPath};

use super::{rect_path, CellJob, Grid};

/// Trim only the ASCII separators the reference's `^[ \t\r\n]*horz[ \t\r\n]*$`
/// test admits; NBSP must not read as `horz`. ref renderer.ts:7870-7872
fn is_horizontal(vert: &str) -> bool {
    vert.trim_matches([' ', '\t', '\r', '\n']) == "horz"
}

/// Clip path (slide space) for one cell's text, or `None` when the reference
/// leaves the body unclipped.
///
/// Two admissions, both mirroring the reference:
/// * only an effective horizontal body (`tcPr@vert` folded into `body.vert`,
///   omitted or `horz`) — rotated/stacked bodies keep their unclipped paint;
/// * `a:tcPr@horzOverflow="overflow"` disables the clip; every other value,
///   including the schema default, clips.
///
/// ref renderer.ts:7861-7878
pub(super) fn cell_clip(
    job: &CellJob<'_>,
    grid: &Grid<'_>,
    transform: &AffineTransform,
) -> Option<VectorPath> {
    let body = job.cell.text_body.as_ref()?;
    if !is_horizontal(&body.vert) {
        return None;
    }
    if job.cell.horz_overflow.as_deref() == Some("overflow") {
        return None;
    }
    let band = grid.row_h.iter().sum::<f32>().max(1.0);
    let local = rect_path(
        job.col_x,
        job.row_y - band,
        job.cell_w,
        job.cell_h + 2.0 * band,
    );
    Some(map_path(&local, transform))
}

/// Map a local-space path into slide space; clips live in slide coordinates,
/// unlike `DrawPath` whose transform is carried on the command.
fn map_path(path: &VectorPath, transform: &AffineTransform) -> VectorPath {
    let segments = path
        .segments
        .iter()
        .map(|segment| match segment {
            PathSegment::MoveTo { x, y } => {
                let (x, y) = transform.transform_point(*x, *y);
                PathSegment::MoveTo { x, y }
            }
            PathSegment::LineTo { x, y } => {
                let (x, y) = transform.transform_point(*x, *y);
                PathSegment::LineTo { x, y }
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (x1, y1) = transform.transform_point(*x1, *y1);
                let (x, y) = transform.transform_point(*x, *y);
                PathSegment::QuadTo { x1, y1, x, y }
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (x1, y1) = transform.transform_point(*x1, *y1);
                let (x2, y2) = transform.transform_point(*x2, *y2);
                let (x, y) = transform.transform_point(*x, *y);
                PathSegment::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                }
            }
            PathSegment::Close => PathSegment::Close,
        })
        .collect();
    VectorPath::new(segments, path.fill_rule)
}
