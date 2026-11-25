//! DrawingML multi-column text flow (`bodyPr@numCol`, S5).
//!
//! ECMA-376 §21.1.2.1.1 defines columns as overflow containers: if the content
//! fits one column, all of it stays in column 0; otherwise paragraphs are
//! distributed in balanced shares so parallel rows across columns line up.
//! ref `renderer.ts:5616-5680`.

/// Column assignment for every laid-out line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnPlan {
    pub line_col: Vec<usize>,
    pub lines_per_col: usize,
    pub used_columns: usize,
}

/// Assign lines to columns. `content_height` excludes the final paragraph's
/// trailing `spcAft` (it cannot by itself force overflow).
pub fn plan_columns(line_count: usize, content_height: f32, column_capacity: f32, num_col: usize) -> ColumnPlan {
    if line_count == 0 {
        return ColumnPlan {
            line_col: Vec::new(),
            lines_per_col: 0,
            used_columns: 0,
        };
    }
    let num_col = num_col.max(1);
    let fits_one = column_capacity <= 0.0 || content_height <= column_capacity + 0.5;
    if num_col == 1 || fits_one {
        return ColumnPlan {
            line_col: vec![0; line_count],
            lines_per_col: line_count,
            used_columns: 1,
        };
    }
    let lines_per_col = line_count.div_ceil(num_col);
    let mut line_col = Vec::with_capacity(line_count);
    let mut col = 0usize;
    let mut in_col = 0usize;
    for _ in 0..line_count {
        if col + 1 < num_col && in_col >= lines_per_col {
            col += 1;
            in_col = 0;
        }
        line_col.push(col);
        in_col += 1;
    }
    ColumnPlan {
        line_col,
        lines_per_col,
        used_columns: col + 1,
    }
}

/// Horizontal offset of one column; `rtl_col` mirrors the visual index.
pub fn column_x(column: usize, num_col: usize, column_width: f32, column_gap: f32, rtl_col: bool) -> f32 {
    let visual = if rtl_col {
        num_col.saturating_sub(1).saturating_sub(column)
    } else {
        column
    };
    visual as f32 * (column_width + column_gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_content_stays_in_column_zero() {
        let plan = plan_columns(9, 90.0, 100.0, 2);
        assert_eq!(plan.used_columns, 1);
        assert!(plan.line_col.iter().all(|col| *col == 0));
    }

    #[test]
    fn overflowing_content_balances_with_ceil_lines_per_column() {
        let plan = plan_columns(9, 260.0, 100.0, 2);
        assert_eq!(plan.lines_per_col, 5);
        assert_eq!(plan.line_col, vec![0, 0, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(plan.used_columns, 2);
    }

    #[test]
    fn rtl_columns_mirror_the_visual_index() {
        assert_eq!(column_x(0, 2, 100.0, 10.0, false), 0.0);
        assert_eq!(column_x(1, 2, 100.0, 10.0, false), 110.0);
        assert_eq!(column_x(0, 2, 100.0, 10.0, true), 110.0);
        assert_eq!(column_x(1, 2, 100.0, 10.0, true), 0.0);
    }
}
