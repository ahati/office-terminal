//! Table rendering parity: shared-gridline ownership, two-pass paint order,
//! RTL geometry, spanned-edge subdivision, Office minimum-row growth, cell
//! horizontal clipping and `cmpd` compound borders.
//! ref `renderer.ts:7591-8110`, `table-border-conflict.test.ts`,
//! `table-border-single-draw.test.ts:204-535`, `renderer.ts:7843-7973`.

mod table_support;

use kitty_pptx_core::contracts::{VectorCommand, VectorPath};
use serde_json::{json, Value};
use table_support::*;


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

// ── Case 7: cells clip text to their own x span ───────────────────────────
/// One paragraph, one unbreakable run, no insets, no wrapping — the
/// horizontal-overflow probe. `vert` selects the body direction the
/// reference's clip admits.
fn one_word_body(word: &str, vert: &str) -> Value {
    json!({
        "verticalAnchor": "t", "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "none", "vert": vert, "autoFit": "none", "fontScale": null,
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
        "anchorCtr": false,
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null, "lvl": 0,
            "bullet": {"type": "none"}, "defFontSize": 18.0, "defColor": null,
            "defBold": false, "defFontFamily": "Arial", "tabStops": [],
            "rtl": false, "eaLnBrk": true, "runs": [text_run(word)],
        }],
    })
}

/// Glyph x extent, in slide space, of every `DrawText` command in `slice`.
fn glyph_span(slice: &[VectorCommand]) -> (f32, f32) {
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for command in slice {
        if let VectorCommand::DrawText { runs, transform } = command {
            for glyph in runs.iter().flat_map(|run| run.glyphs.iter()) {
                let (x, _) = transform.transform_point(glyph.x, glyph.y);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    (lo, hi)
}

/// The clip path wrapping the cell text plus the glyph span it encloses.
fn text_clip(commands: &[VectorCommand]) -> Option<(VectorPath, f32, f32)> {
    let push = commands
        .iter()
        .position(|c| matches!(c, VectorCommand::PushClip(_)))?;
    let pop = commands
        .iter()
        .position(|c| matches!(c, VectorCommand::PopClip))?;
    let VectorCommand::PushClip(path) = &commands[push] else {
        return None;
    };
    let (lo, hi) = glyph_span(&commands[push + 1..pop]);
    Some((path.clone(), lo, hi))
}

#[test]
fn cell_text_clips_to_the_cell_x_span_by_default() {
    // ref renderer.ts:7861-7879 — the schema default is clip.
    let word = "W".repeat(200);
    let cmds = TableSpec::new(
        vec![vec![Cell::new().text(one_word_body(&word, "horz"))]],
        vec![20],
        vec![60],
    )
    .compile();
    let (clip, lo, hi) = text_clip(&cmds).expect("default cells clip their text");
    let (x0, y0, x1, y1) = rect_bounds(&clip).expect("clip is a rectangle");
    assert_eq!((x0, x1), (0.0, 60.0 * EMU as f32), "clip is the cell x span");
    assert!(hi > x1 && lo >= x0, "the word really overruns the cell: {lo}..{hi}");
    // The band is taller than the cell so vertical overflow still shows
    // (canvas-clip.ts:1-10 clips x only, over the whole viewport).
    assert!(y0 < 0.0 && y1 > 20.0 * EMU as f32, "no vertical clipping");
}

#[test]
fn overflow_and_non_horizontal_bodies_are_not_clipped() {
    // `horzOverflow="overflow"` opts out. ref renderer.ts:7876-7878
    let word = "W".repeat(200);
    let cmds = TableSpec::new(
        vec![vec![Cell::new().text(one_word_body(&word, "horz")).overflow()]],
        vec![20],
        vec![60],
    )
    .compile();
    assert!(text_clip(&cmds).is_none(), "overflow keeps the text unclipped");
    let (_, hi) = glyph_span(&cmds);
    assert!(hi > 60.0 * EMU as f32, "and the word paints outside the cell");

    // Only an effective horizontal body is admitted. ref renderer.ts:7870-7874
    let cmds = TableSpec::new(
        vec![vec![Cell::new().text(one_word_body(&word, "vert270"))]],
        vec![20],
        vec![60],
    )
    .compile();
    assert!(text_clip(&cmds).is_none(), "rotated bodies keep their paint");
}

// ── Case 8: `cmpd` borders paint as rails, not as one thick line ──────────
fn rails(cmds: &[VectorCommand], color: [u8; 4]) -> Vec<(f32, f32, f32, f32)> {
    filled_rects(cmds)
        .into_iter()
        .filter(|r| r.4 == color)
        .map(|r| (r.0, r.1, r.2, r.3))
        .collect()
}

#[test]
fn compound_double_border_paints_two_rails_with_a_gap() {
    // ref renderer.ts:7951-7973 + double-border.ts:40-70.
    let cmds = TableSpec::new(
        vec![vec![Cell::new().border_cmpd("b", "112233", 3, "dbl")]],
        vec![20],
        vec![60],
    )
    .compile();
    let grey = [0x11, 0x22, 0x33, 255];
    let bands = rails(&cmds, grey);
    assert_eq!(bands.len(), 2, "dbl paints two rails");
    assert!(segments(&cmds).is_empty(), "and no plain stroke");
    let (top, bottom) = (bands[0].1, bands[1].3);
    let (rail, gap) = (bands[0].3 - bands[0].1, bands[1].1 - bands[0].3);
    assert!((rail - EMU as f32).abs() < 1.0, "w/3 rail: {rail}");
    assert!((gap - rail).abs() < 1.0, "gap equals the rail");
    let edge = 20.0 * EMU as f32;
    assert!((top + bottom - 2.0 * edge).abs() < 1.0, "centred on the edge");
    assert_eq!(bands[0].0, 0.0);
    assert_eq!(bands[0].2, 60.0 * EMU as f32);
}

#[test]
fn compound_tri_border_paints_three_bands() {
    let cmds = TableSpec::new(
        vec![vec![Cell::new().border_cmpd("r", "112233", 5, "tri")]],
        vec![20],
        vec![60],
    )
    .compile();
    let bands = rails(&cmds, [0x11, 0x22, 0x33, 255]);
    assert_eq!(bands.len(), 3, "tri paints three bands");
    assert!(segments(&cmds).is_empty(), "and no plain stroke");
    let widths: Vec<f32> = bands.iter().map(|b| b.2 - b.0).collect();
    let fifth = EMU as f32; // 5 pt / 5
    assert!((widths[0] - fifth).abs() < 1.0, "w/5 band: {}", widths[0]);
    assert!((widths[1] - 3.0 * fifth).abs() < 1.0, "3w/5 band: {}", widths[1]);
    assert_eq!(bands[0].1, 0.0);
    assert_eq!(bands[0].3, 20.0 * EMU as f32);
}

#[test]
fn non_axis_aligned_compound_border_falls_back_to_a_stroke() {
    // Rails are axis-aligned band fills; a diagonal keeps the plain stroke.
    let cmds = TableSpec::new(
        vec![vec![Cell::new().border_cmpd("tl", "112233", 3, "dbl")]],
        vec![20],
        vec![60],
    )
    .compile();
    assert!(rails(&cmds, [0x11, 0x22, 0x33, 255]).is_empty());
    assert_eq!(segments(&cmds).len(), 1, "one diagonal stroke");
}
