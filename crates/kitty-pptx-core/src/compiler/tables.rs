use crossbeam_channel::Sender;
use crate::contracts::{
    AffineTransform, FillRule, PathSegment, VectorCommand, VectorPath, WarningEvent,
};
use super::style::{compile_fill, compile_stroke};
use super::text::compile_text_body;
use super::transform::compute_shape_transform;

pub fn compile_table_element(
    tbl: &pptx_model::TableElement,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let mut commands = Vec::new();

    let table_transform = compute_shape_transform(
        tbl.x,
        tbl.y,
        tbl.width,
        tbl.height,
        tbl.rotation,
        tbl.flip_h,
        tbl.flip_v,
    );

    // Compute column cumulative x offsets
    let mut col_offsets = Vec::with_capacity(tbl.cols.len() + 1);
    let mut cur_x = 0i64;
    col_offsets.push(cur_x);
    for col_w in &tbl.cols {
        cur_x += col_w;
        col_offsets.push(cur_x);
    }

    // Compute row cumulative y offsets
    let mut row_offsets = Vec::with_capacity(tbl.rows.len() + 1);
    let mut cur_y = 0i64;
    row_offsets.push(cur_y);
    for row in &tbl.rows {
        cur_y += row.height;
        row_offsets.push(cur_y);
    }

    // Table background fill if present
    if let Some(ref bg_fill) = tbl.background {
        if let Some(paint) = compile_fill(bg_fill, 1.0) {
            let bg_path = VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo {
                        x: tbl.width as f32,
                        y: 0.0,
                    },
                    PathSegment::LineTo {
                        x: tbl.width as f32,
                        y: tbl.height as f32,
                    },
                    PathSegment::LineTo {
                        x: 0.0,
                        y: tbl.height as f32,
                    },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            );
            commands.push(VectorCommand::DrawPath {
                path: bg_path,
                fill: Some(paint),
                stroke: None,
                transform: table_transform,
            });
        }
    }

    // Process each cell
    for (r_idx, row) in tbl.rows.iter().enumerate() {
        for (c_idx, cell) in row.cells.iter().enumerate() {
            if cell.h_merge || cell.v_merge {
                continue;
            }

            let cell_x = col_offsets.get(c_idx).copied().unwrap_or(0);
            let cell_y = row_offsets.get(r_idx).copied().unwrap_or(0);

            let col_span = cell.grid_span.max(1) as usize;
            let row_span = cell.row_span.max(1) as usize;

            let end_c = (c_idx + col_span).min(tbl.cols.len());
            let end_r = (r_idx + row_span).min(tbl.rows.len());

            let cell_w = col_offsets.get(end_c).copied().unwrap_or(cell_x) - cell_x;
            let cell_h = row_offsets.get(end_r).copied().unwrap_or(cell_y) - cell_y;

            let x0 = cell_x as f32;
            let y0 = cell_y as f32;
            let x1 = (cell_x + cell_w) as f32;
            let y1 = (cell_y + cell_h) as f32;

            // 1. Cell background fill
            if let Some(ref fill) = cell.fill {
                if let Some(paint) = compile_fill(fill, 1.0) {
                    let cell_rect = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x0, y: y0 },
                            PathSegment::LineTo { x: x1, y: y0 },
                            PathSegment::LineTo { x: x1, y: y1 },
                            PathSegment::LineTo { x: x0, y: y1 },
                            PathSegment::Close,
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: cell_rect,
                        fill: Some(paint),
                        stroke: None,
                        transform: table_transform,
                    });
                }
            }

            // 2. Cell borders
            if let Some(ref stroke) = cell.border_t {
                if let Some(v_stroke) = compile_stroke(stroke) {
                    let border_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x0, y: y0 },
                            PathSegment::LineTo { x: x1, y: y0 },
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: border_path,
                        fill: None,
                        stroke: Some(v_stroke),
                        transform: table_transform,
                    });
                }
            }
            if let Some(ref stroke) = cell.border_b {
                if let Some(v_stroke) = compile_stroke(stroke) {
                    let border_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x0, y: y1 },
                            PathSegment::LineTo { x: x1, y: y1 },
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: border_path,
                        fill: None,
                        stroke: Some(v_stroke),
                        transform: table_transform,
                    });
                }
            }
            if let Some(ref stroke) = cell.border_l {
                if let Some(v_stroke) = compile_stroke(stroke) {
                    let border_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x0, y: y0 },
                            PathSegment::LineTo { x: x0, y: y1 },
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: border_path,
                        fill: None,
                        stroke: Some(v_stroke),
                        transform: table_transform,
                    });
                }
            }
            if let Some(ref stroke) = cell.border_r {
                if let Some(v_stroke) = compile_stroke(stroke) {
                    let border_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x1, y: y0 },
                            PathSegment::LineTo { x: x1, y: y1 },
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: border_path,
                        fill: None,
                        stroke: Some(v_stroke),
                        transform: table_transform,
                    });
                }
            }
            if let Some(ref stroke) = cell.diagonal_tl {
                if let Some(v_stroke) = compile_stroke(stroke) {
                    let border_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x0, y: y0 },
                            PathSegment::LineTo { x: x1, y: y1 },
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: border_path,
                        fill: None,
                        stroke: Some(v_stroke),
                        transform: table_transform,
                    });
                }
            }
            if let Some(ref stroke) = cell.diagonal_tr {
                if let Some(v_stroke) = compile_stroke(stroke) {
                    let border_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: x1, y: y0 },
                            PathSegment::LineTo { x: x0, y: y1 },
                        ],
                        FillRule::NonZero,
                    );
                    commands.push(VectorCommand::DrawPath {
                        path: border_path,
                        fill: None,
                        stroke: Some(v_stroke),
                        transform: table_transform,
                    });
                }
            }

            // 3. Cell text body
            if let Some(ref text_body) = cell.text_body {
                let cell_transform = table_transform.post_concat(&AffineTransform::translate(x0, y0));
                let text_cmds = compile_text_body(
                    text_body,
                    cell_w,
                    cell_h,
                    &cell_transform,
                    cell.text_color.as_deref(),
                    warning_sink,
                );
                commands.extend(text_cmds);
            }
        }
    }

    commands
}
