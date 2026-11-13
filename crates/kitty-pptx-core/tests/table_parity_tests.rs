//! Table rendering parity: shared-gridline ownership, two-pass paint order,
//! RTL geometry, spanned-edge subdivision and Office minimum-row growth.
//! ref `renderer.ts:7591-8110`, `table-border-conflict.test.ts`,
//! `table-border-single-draw.test.ts:204-535`.

use crossbeam_channel::unbounded;
use kitty_pptx_core::compiler::tables::compile_table_element;
use kitty_pptx_core::contracts::{PathSegment, VectorCommand, VectorPath, VectorPaint};
use pptx_model::TableElement;
use serde_json::{json, Value};

const EMU: i64 = 12700;

#[derive(Clone, Default)]
struct Cell {
    text: Option<Value>,
    fill: Option<Value>,
    border_l: Option<Value>,
    border_r: Option<Value>,
    border_t: Option<Value>,
    border_b: Option<Value>,
    grid_span: u32,
    row_span: u32,
    h_merge: bool,
    v_merge: bool,
}

impl Cell {
    fn new() -> Self {
        Self {
            grid_span: 1,
            row_span: 1,
            ..Self::default()
        }
    }

    fn text(mut self, body: Value) -> Self {
        self.text = Some(body);
        self
    }

    fn fill(mut self, color: &str) -> Self {
        self.fill = Some(json!({"fillType": "solid", "color": color}));
        self
    }

    fn border(mut self, side: &str, color: &str, width_pt: i64) -> Self {
        let spec = json!({"color": color, "width": width_pt * EMU});
        match side {
            "l" => self.border_l = Some(spec),
            "r" => self.border_r = Some(spec),
            "t" => self.border_t = Some(spec),
            "b" => self.border_b = Some(spec),
            _ => unreachable!(),
        }
        self
    }

    fn span(mut self, grid: u32, rows: u32) -> Self {
        self.grid_span = grid;
        self.row_span = rows;
        self
    }

    fn merge(mut self, horizontal: bool) -> Self {
        if horizontal {
            self.h_merge = true;
        } else {
            self.v_merge = true;
        }
        self
    }

    fn json(&self) -> Value {
        json!({
            "textBody": self.text,
            "fill": self.fill,
            "borderL": self.border_l,
            "borderR": self.border_r,
            "borderT": self.border_t,
            "borderB": self.border_b,
            "gridSpan": self.grid_span,
            "rowSpan": self.row_span,
            "hMerge": self.h_merge,
            "vMerge": self.v_merge,
        })
    }
}

/// One run of a text body. Mirrors the compiler-facing `TextRun::Text` JSON.
fn text_run(text: &str) -> Value {
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

fn line_break() -> Value {
    json!({"type": "break"})
}

/// Three explicit lines (`one`, `two`, `three`) in one paragraph, 100 % line
/// spacing, top/bottom inset 3.6 pt. The trailing 12 pt `spcAft` is suppressed
/// inside a cell even with `spcFirstLastPara=1`
/// (`table-border-single-draw.test.ts:133-202`). The reference case expects
/// 72 pt of content: 3 × 21.6 pt line boxes + 7.2 pt insets
/// (`table-border-single-draw.test.ts:319-348`).
fn three_line_body() -> Value {
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

struct TableSpec {
    rows: Vec<Vec<Cell>>,
    row_heights_pt: Vec<i64>,
    col_widths_pt: Vec<i64>,
    height_pt: Option<i64>,
    background: Option<Value>,
    rtl: bool,
}

impl TableSpec {
    fn new(rows: Vec<Vec<Cell>>, row_heights_pt: Vec<i64>, col_widths_pt: Vec<i64>) -> Self {
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

    fn height(mut self, pt: i64) -> Self {
        self.height_pt = Some(pt);
        self
    }

    fn background(mut self, color: &str) -> Self {
        self.background = Some(json!({"fillType": "solid", "color": color}));
        self
    }

    fn rtl(mut self) -> Self {
        self.rtl = true;
        self
    }

    fn compile(&self) -> Vec<VectorCommand> {
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
struct Seg {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: i64,
    color: [u8; 4],
}

fn segments(commands: &[VectorCommand]) -> Vec<Seg> {
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

fn vertical_at(segments: &[Seg], x: i64) -> Vec<&Seg> {
    let x = x as f32;
    segments
        .iter()
        .filter(|s| (s.x1 - s.x2).abs() < 0.01 && (s.x1 - x).abs() <= 1.0)
        .collect()
}

fn horizontal_at(segments: &[Seg], y: i64) -> Vec<&Seg> {
    let y = y as f32;
    segments
        .iter()
        .filter(|s| (s.y1 - s.y2).abs() < 0.01 && (s.y1 - y).abs() <= 1.0)
        .collect()
}

fn rect_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
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

fn filled_rects(commands: &[VectorCommand]) -> Vec<(f32, f32, f32, f32, [u8; 4])> {
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

fn is_border(command: &VectorCommand) -> bool {
    matches!(
        command,
        VectorCommand::DrawPath {
            stroke: Some(_),
            fill: None,
            ..
        }
    )
}

// ── Case 1: one shared gridline, winner by all four conflict rules ─────────
#[test]
fn shared_gridline_stroked_once_with_conflict_winner() {
    // Rule #0 — null on the owner side loses; the facing line is displayed.
    let cmds = TableSpec::new(
        vec![vec![
            Cell::new(),
            Cell::new().border("l", "112233", 1),
        ]],
        vec![20],
        vec![60, 60],
    )
    .compile();
    let segs = segments(&cmds);
    let shared = vertical_at(&segs, 60 * EMU);
    assert_eq!(shared.len(), 1, "shared vertical line drawn once");
    assert_eq!(shared[0].color, [0x11, 0x22, 0x33, 255]);
    assert_eq!(shared[0].width, EMU);

    // Rule #1 — the wider line wins, regardless of paint order.
    let cmds = TableSpec::new(
        vec![vec![
            Cell::new().border("r", "ff0000", 1),
            Cell::new().border("l", "0000ff", 3),
        ]],
        vec![20],
        vec![60, 60],
    )
    .compile();
    let segs = segments(&cmds);
    let shared = vertical_at(&segs, 60 * EMU);
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].color, [0, 0, 255, 255]);
    assert_eq!(shared[0].width, 3 * EMU);

    // Rule #2 — equal width: the darker (smaller Rec.601 luma) colour wins.
    let cmds = TableSpec::new(
        vec![vec![
            Cell::new().border("r", "808080", 2),
            Cell::new().border("l", "404040", 2),
        ]],
        vec![20],
        vec![60, 60],
    )
    .compile();
    let segs = segments(&cmds);
    let shared = vertical_at(&segs, 60 * EMU);
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].color, [0x40, 0x40, 0x40, 255]);

    // Rule #3 — a total tie goes to the owner (physically-left cell). The
    // 8-hex alpha pair is excluded from the luma comparison.
    let cmds = TableSpec::new(
        vec![vec![
            Cell::new().border("r", "ff0000ff", 2),
            Cell::new().border("l", "ff0000aa", 2),
        ]],
        vec![20],
        vec![60, 60],
    )
    .compile();
    let segs = segments(&cmds);
    let shared = vertical_at(&segs, 60 * EMU);
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].color, [255, 0, 0, 255], "owner wins the tie");
}

// ── Case 2: every fill and every text body precedes every border ───────────
#[test]
fn fills_and_text_precede_all_borders() {
    let body = three_line_body();
    let cmds = TableSpec::new(
        vec![vec![
            Cell::new().fill("112233").border("t", "000000", 1)
                .border("l", "000000", 1).border("r", "000000", 1)
                .border("b", "000000", 1).text(body),
        ]],
        vec![80],
        vec![120],
    )
    .background("FAFAFA")
    .compile();

    let first_border = cmds
        .iter()
        .position(is_border)
        .expect("table has border commands");
    assert!(
        cmds[..first_border].iter().any(|c| matches!(
            c,
            VectorCommand::DrawPath { fill: Some(_), .. }
        )),
        "cell/table fills are painted before the border pass"
    );
    assert!(
        cmds[..first_border]
            .iter()
            .any(|c| matches!(c, VectorCommand::DrawText { .. })),
        "cell text is painted before the border pass"
    );
    assert!(
        cmds[first_border..].iter().all(is_border),
        "no fill or text command is interleaved with borders"
    );

    // The table background is the first command and covers the grown table.
    let fills = filled_rects(&cmds);
    assert_eq!(fills[0].4, [0xFA, 0xFA, 0xFA, 255]);
    assert_eq!(fills[0].0, 0.0);
    assert_eq!(fills[0].1, 0.0);
    assert_eq!(fills[0].2, 120.0 * EMU as f32);
}

// ── Case 3: RTL places logical column 0 at the physical right ─────────────
#[test]
fn rtl_places_column_zero_on_the_right() {
    let cmds = TableSpec::new(
        vec![vec![
            Cell::new().fill("ff0000").border("r", "ff0000", 1),
            Cell::new().fill("0000ff").border("l", "0000ff", 3),
        ]],
        vec![20],
        vec![60, 60],
    )
    .rtl()
    .compile();

    let fills = filled_rects(&cmds);
    let red = fills.iter().find(|f| f.4 == [255, 0, 0, 255]).unwrap();
    let blue = fills.iter().find(|f| f.4 == [0, 0, 255, 255]).unwrap();
    assert_eq!((red.0, red.2), (60.0 * EMU as f32, 120.0 * EMU as f32));
    assert_eq!((blue.0, blue.2), (0.0, 60.0 * EMU as f32));

    // Logical borderR of the right-hand cell faces logical borderL of the
    // left-hand cell; the wider blue line wins once at the shared x = 60 pt.
    let segs = segments(&cmds);
    let shared = vertical_at(&segs, 60 * EMU);
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].color, [0, 0, 255, 255]);
    assert_eq!(shared[0].width, 3 * EMU);
    assert!(vertical_at(&segs, 0).is_empty());
    assert!(vertical_at(&segs, 120 * EMU).is_empty());
}

// ── Case 4: spanned edges subdivide at every neighbour boundary ────────────
#[test]
fn spanned_edges_split_at_neighbour_boundaries() {
    // gridSpan: one 2-column above cell faces two below cells with different
    // top borders (#824). Each half resolves against its own neighbour.
    let cmds = TableSpec::new(
        vec![
            vec![Cell::new().span(2, 1), Cell::new().merge(true)],
            vec![
                Cell::new().border("t", "111111", 1),
                Cell::new().border("t", "222222", 3),
            ],
        ],
        vec![20, 20],
        vec![60, 60],
    )
    .compile();
    let segs = segments(&cmds);
    let edge = horizontal_at(&segs, 20 * EMU);
    let left = edge
        .iter()
        .find(|s| s.x1.min(s.x2) <= 1.0 && s.x1.max(s.x2) <= 60.0 * EMU as f32)
        .expect("left sub-segment");
    let right = edge
        .iter()
        .find(|s| s.x1.min(s.x2) >= 60.0 * EMU as f32 - 1.0)
        .expect("right sub-segment");
    assert_eq!(left.color, [0x11, 0x11, 0x11, 255]);
    assert_eq!(left.width, EMU);
    assert_eq!(right.color, [0x22, 0x22, 0x22, 255]);
    assert_eq!(right.width, 3 * EMU);

    // rowSpan: one 2-row left cell faces two right cells with different left
    // borders; its physical-right edge splits at the row boundary.
    let cmds = TableSpec::new(
        vec![
            vec![Cell::new().span(1, 2), Cell::new().border("l", "111111", 1)],
            vec![Cell::new().merge(false), Cell::new().border("l", "222222", 3)],
        ],
        vec![20, 20],
        vec![60, 60],
    )
    .compile();
    let segs = segments(&cmds);
    let edge = vertical_at(&segs, 60 * EMU);
    let top = edge
        .iter()
        .find(|s| s.y1.min(s.y2) <= 1.0 && s.y1.max(s.y2) <= 20.0 * EMU as f32)
        .expect("top sub-segment");
    let bottom = edge
        .iter()
        .find(|s| s.y1.min(s.y2) >= 20.0 * EMU as f32 - 1.0)
        .expect("bottom sub-segment");
    assert_eq!(top.color, [0x11, 0x11, 0x11, 255]);
    assert_eq!(bottom.color, [0x22, 0x22, 0x22, 255]);
    assert_eq!(bottom.width, 3 * EMU);
}

// ── Case 5: a 3-line cell grows a 1-line authored row ─────────────────────
#[test]
fn three_line_cell_grows_a_one_line_authored_row() {
    let cmds = TableSpec::new(
        vec![vec![Cell::new().text(three_line_body()).border("b", "000000", 1)]],
        vec![20],
        vec![200],
    )
    .height(80)
    .compile();
    let segs = segments(&cmds);
    let bottom = horizontal_at(&segs, 72 * EMU);
    assert_eq!(bottom.len(), 1, "grown bottom border at 72 pt");
    assert!(bottom[0].y1 > 20.0 * EMU as f32, "authored 20 pt row grew");
}

// ── Case 6: no growth when authored minima already fill the frame ─────────
#[test]
fn authored_heights_preserved_when_frame_is_not_growing() {
    let cmds = TableSpec::new(
        vec![vec![Cell::new().text(three_line_body()).border("b", "000000", 1)]],
        vec![20],
        vec![200],
    )
    .height(20)
    .compile();
    let segs = segments(&cmds);
    let bottom = horizontal_at(&segs, 20 * EMU);
    assert_eq!(bottom.len(), 1, "authored 20 pt row is preserved");
    assert!(horizontal_at(&segs, 72 * EMU).is_empty());
}
