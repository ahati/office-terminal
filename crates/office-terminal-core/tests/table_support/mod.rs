//! Shared builders and command inspectors for the table parity cases.
//! ref `renderer.ts:7591-8110`, `table-border-conflict.test.ts`,
//! `table-border-single-draw.test.ts:204-535`.

#![allow(dead_code)]

use crossbeam_channel::unbounded;
use office_terminal_core::compiler::tables::compile_table_element;
use office_terminal_core::contracts::{PathSegment, VectorCommand, VectorPaint, VectorPath};
use pptx_model::TableElement;
use serde_json::{json, Value};

pub const EMU: i64 = 12700;

#[derive(Clone, Default)]
pub struct Cell {
    text: Option<Value>,
    fill: Option<Value>,
    border_l: Option<Value>,
    border_r: Option<Value>,
    border_t: Option<Value>,
    border_b: Option<Value>,
    diagonal_tl: Option<Value>,
    horz_overflow: Option<String>,
    grid_span: u32,
    row_span: u32,
    h_merge: bool,
    v_merge: bool,
}

impl Cell {
    pub fn new() -> Self {
        Self {
            grid_span: 1,
            row_span: 1,
            ..Self::default()
        }
    }

    pub fn text(mut self, body: Value) -> Self {
        self.text = Some(body);
        self
    }

    pub fn fill(mut self, color: &str) -> Self {
        self.fill = Some(json!({"fillType": "solid", "color": color}));
        self
    }

    pub fn border(mut self, side: &str, color: &str, width_pt: i64) -> Self {
        let spec = json!({"color": color, "width": width_pt * EMU});
        match side {
            "l" => self.border_l = Some(spec),
            "r" => self.border_r = Some(spec),
            "t" => self.border_t = Some(spec),
            "b" => self.border_b = Some(spec),
            "tl" => self.diagonal_tl = Some(spec),
            _ => unreachable!(),
        }
        self
    }

    /// The same border with `a:ln@cmpd` set.
    pub fn border_cmpd(self, side: &str, color: &str, width_pt: i64, cmpd: &str) -> Self {
        let mut cell = self.border(side, color, width_pt);
        let slot = match side {
            "l" => &mut cell.border_l,
            "r" => &mut cell.border_r,
            "t" => &mut cell.border_t,
            "b" => &mut cell.border_b,
            "tl" => &mut cell.diagonal_tl,
            _ => unreachable!(),
        };
        if let Some(spec) = slot {
            spec["cmpd"] = json!(cmpd);
        }
        cell
    }

    /// `a:tcPr@horzOverflow="overflow"`.
    pub fn overflow(mut self) -> Self {
        self.horz_overflow = Some("overflow".to_string());
        self
    }

    pub fn span(mut self, grid: u32, rows: u32) -> Self {
        self.grid_span = grid;
        self.row_span = rows;
        self
    }

    pub fn merge(mut self, horizontal: bool) -> Self {
        if horizontal {
            self.h_merge = true;
        } else {
            self.v_merge = true;
        }
        self
    }

    pub fn json(&self) -> Value {
        json!({
            "textBody": self.text,
            "fill": self.fill,
            "borderL": self.border_l,
            "borderR": self.border_r,
            "borderT": self.border_t,
            "borderB": self.border_b,
            "diagonalTl": self.diagonal_tl,
            "horzOverflow": self.horz_overflow,
            "gridSpan": self.grid_span,
            "rowSpan": self.row_span,
            "hMerge": self.h_merge,
            "vMerge": self.v_merge,
        })
    }
}

/// One run of a text body. Mirrors the compiler-facing `TextRun::Text` JSON.
pub fn text_run(text: &str) -> Value {
    json!({
        "type": "text",
        "text": text,
        "bold": false,
        "italic": false,
        "underline": false,
        "strikeDouble": false,
        "hyperlinkUsesTextFill": false,
        "fieldType": null,
        "strikethrough": false,
        "fontSize": 18.0,
        "color": null,
        "fontFamily": "Arial",
    })
}

pub fn line_break() -> Value {
    json!({"type": "break"})
}

/// Three explicit lines (`one`, `two`, `three`) in one paragraph, 100 % line
/// spacing, top/bottom inset 3.6 pt. The trailing 12 pt `spcAft` is suppressed
/// inside a cell even with `spcFirstLastPara=1`
/// (`table-border-single-draw.test.ts:133-202`). The reference case expects
/// 72 pt of content: 3 × 21.6 pt line boxes + 7.2 pt insets
/// (`table-border-single-draw.test.ts:319-348`).
pub fn three_line_body() -> Value {
    json!({
        "verticalAnchor": "ctr",
        "lIns": 0,
        "rIns": 0,
        "tIns": 36 * EMU / 10,
        "bIns": 36 * EMU / 10,
        "wrap": "square",
        "vert": "horz",
        "autoFit": "none",
        "fontScale": null,
        "numCol": 1,
        "spcCol": 0,
        "rtlCol": false,
        "spcFirstLastPara": true,
        "anchorCtr": false,
        "paragraphs": [{
            "alignment": "l",
            "marL": 0,
            "marR": 0,
            "indent": 0,
            "spaceBefore": null,
            "spaceAfter": 1200,
            "spaceLine": {"type": "pct", "val": 100000},
            "lvl": 0,
            "bullet": {"type": "none"},
            "defFontSize": 18.0,
            "defColor": null,
            "defBold": false,
            "defFontFamily": "Arial",
            "tabStops": [],
            "rtl": false,
            "eaLnBrk": true,
            "runs": [
                text_run("one"), line_break(),
                text_run("two"), line_break(),
                text_run("three"),
            ],
        }],
    })
}

pub struct TableSpec {
    pub rows: Vec<Vec<Cell>>,
    pub row_heights_pt: Vec<i64>,
    pub col_widths_pt: Vec<i64>,
    pub height_pt: Option<i64>,
    pub background: Option<Value>,
    pub rtl: bool,
}

impl TableSpec {
    pub fn new(rows: Vec<Vec<Cell>>, row_heights_pt: Vec<i64>, col_widths_pt: Vec<i64>) -> Self {
        let height_pt = Some(row_heights_pt.iter().sum());
        Self {
            rows,
            row_heights_pt,
            col_widths_pt,
            height_pt,
            background: None,
            rtl: false,
        }
    }

    pub fn height(mut self, pt: i64) -> Self {
        self.height_pt = Some(pt);
        self
    }

    pub fn background(mut self, color: &str) -> Self {
        self.background = Some(json!({"fillType": "solid", "color": color}));
        self
    }

    pub fn rtl(mut self) -> Self {
        self.rtl = true;
        self
    }

    pub fn compile(&self) -> Vec<VectorCommand> {
        let rows: Vec<Value> = self
            .rows
            .iter()
            .zip(&self.row_heights_pt)
            .map(|(cells, height)| {
                json!({
                    "height": height * EMU,
                    "cells": cells.iter().map(Cell::json).collect::<Vec<_>>(),
                })
            })
            .collect();
        let value = json!({
            "x": 0,
            "y": 0,
            "width": self.col_widths_pt.iter().sum::<i64>() * EMU,
            "height": self.height_pt.unwrap_or_default() * EMU,
            "rotation": 0.0,
            "flipH": false,
            "flipV": false,
            "background": self.background,
            "cols": self.col_widths_pt.iter().map(|w| w * EMU).collect::<Vec<_>>(),
            "rows": rows,
            "rtl": self.rtl,
        });
        let table: TableElement = serde_json::from_value(value).expect("table JSON");
        let (tx, _rx) = unbounded();
        compile_table_element(&table, &tx)
    }
}

#[derive(Debug)]
pub struct Seg {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub width: i64,
    pub color: [u8; 4],
}

pub fn segments(commands: &[VectorCommand]) -> Vec<Seg> {
    let mut out = Vec::new();
    for command in commands {
        if let VectorCommand::DrawPath {
            path,
            stroke: Some(stroke),
            ..
        } = command
        {
            if let [PathSegment::MoveTo { x: x1, y: y1 }, PathSegment::LineTo { x: x2, y: y2 }] =
                path.segments.as_slice()
            {
                out.push(Seg {
                    x1: *x1,
                    y1: *y1,
                    x2: *x2,
                    y2: *y2,
                    width: stroke.width_emu,
                    color: stroke.color,
                });
            }
        }
    }
    out
}

pub fn vertical_at(segments: &[Seg], x: i64) -> Vec<&Seg> {
    let x = x as f32;
    segments
        .iter()
        .filter(|s| (s.x1 - s.x2).abs() < 0.01 && (s.x1 - x).abs() <= 1.0)
        .collect()
}

pub fn horizontal_at(segments: &[Seg], y: i64) -> Vec<&Seg> {
    let y = y as f32;
    segments
        .iter()
        .filter(|s| (s.y1 - s.y2).abs() < 0.01 && (s.y1 - y).abs() <= 1.0)
        .collect()
}

pub fn rect_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for segment in &path.segments {
        if let PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } = segment {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
    }
    (max_x > min_x && max_y > min_y).then_some((min_x, min_y, max_x, max_y))
}

pub fn filled_rects(commands: &[VectorCommand]) -> Vec<(f32, f32, f32, f32, [u8; 4])> {
    let mut out = Vec::new();
    for command in commands {
        if let VectorCommand::DrawPath {
            path,
            fill: Some(VectorPaint::Solid(color)),
            ..
        } = command
        {
            if let Some((x0, y0, x1, y1)) = rect_bounds(path) {
                out.push((x0, y0, x1, y1, *color));
            }
        }
    }
    out
}

pub fn is_border(command: &VectorCommand) -> bool {
    matches!(
        command,
        VectorCommand::DrawPath {
            stroke: Some(_),
            fill: None,
            ..
        }
    )
}
