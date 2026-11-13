//! Draw-pass parity: tab stops, justification and vertical bodies.
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/renderer.ts`
//! `renderTextBody` — tab resolution (5680-5727), the alignment / justify draw
//! block (5885-5925) and the vertical dispatch (5021-5083); kernels in
//! `packages/core/src/drawingml-text/{tab,text-justify,stacked}.ts`.
//!
//! Every probe compiles a real `TextBody` through `text::emit::compile_text_body`
//! (so the breaker, tab resolver and paint pass all run) and reads the emitted
//! `DrawText` runs back. Assertions are either exact pen positions the tab grid
//! pins, or the *difference* between two box widths 12 EMU apart — the latter
//! needs no font-metric constants: `just`/`dist` spread `slack` over the line's
//! gaps, so widening the box by 12 moves the k-th unit by 6·k.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::emit::compile_text_body;

const POINTS: f32 = 20.0;

/// One paragraph body; `tab_list` is the raw `a:tabLst` JSON (`[]` for none)
/// and `def_tab` the `a:pPr@defTabSz` JSON (`null` for absent).
fn body(
    vert: &str,
    alignment: &str,
    wrap: &str,
    text: &str,
    tab_list: &str,
    def_tab: &str,
) -> pptx_model::TextBody {
    let json = format!(
        r#"{{
            "verticalAnchor": "t",
            "defaultFontSize": {POINTS},
            "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
            "wrap": "{wrap}",
            "vert": "{vert}",
            "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [{{
                "alignment": "{alignment}",
                "marL": 0, "marR": 0, "indent": 0,
                "spaceBefore": null,
                "lvl": 0,
                "bullet": {{ "type": "none" }},
                "defFontSize": {POINTS},
                "tabStops": {tab_list},
                "defTabSz": {def_tab},
                "rtl": false,
                "eaLnBrk": true,
                "runs": [{{
                    "type": "text",
                    "text": "{text}",
                    "bold": false, "italic": false,
                    "underline": false, "strikethrough": false,
                    "strikeDouble": false,
                    "hyperlinkUsesTextFill": false,
                    "fieldType": null,
                    "fontSize": {POINTS},
                    "color": "000000",
                    "fontFamily": "Calibri"
                }}]
            }}]
        }}"#
    );
    serde_json::from_str(&json).expect("text body fixture")
}

/// A body whose only paragraph content is one 1/x fraction, inline (`m:oMath`)
/// or a display block (`m:oMathPara`).
fn math_body(display: bool) -> pptx_model::TextBody {
    let json = format!(
        r#"{{
            "verticalAnchor": "t",
            "defaultFontSize": {POINTS},
            "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
            "wrap": "square",
            "vert": "horz",
            "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [{{
                "alignment": "l",
                "marL": 0, "marR": 0, "indent": 0,
                "spaceBefore": null,
                "lvl": 0,
                "bullet": {{ "type": "none" }},
                "defFontSize": {POINTS},
                "tabStops": [],
                "defTabSz": null,
                "rtl": false,
                "eaLnBrk": true,
                "runs": [{{
                    "type": "math",
                    "display": {display},
                    "fontSize": {POINTS},
                    "nodes": [{{
                        "kind": "fraction",
                        "num": [{{ "kind": "run", "text": "1", "style": "italic" }}],
                        "den": [{{ "kind": "run", "text": "x", "style": "italic" }}]
                    }}]
                }}]
            }}]
        }}"#
    );
    serde_json::from_str(&json).expect("math text body fixture")
}

fn compile(text_body: &pptx_model::TextBody, box_w: i64, box_h: i64) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        text_body,
        box_w,
        box_h,
        &AffineTransform::identity(),
        None,
        &tx,
    )
}

fn draw_text(commands: &[VectorCommand]) -> (&[PositionedGlyphRun], AffineTransform) {
    for command in commands {
        if let VectorCommand::DrawText { runs, transform } = command {
            return (runs, *transform);
        }
    }
    panic!("expected a DrawText command, got {commands:?}");
}

/// Every emitted glyph baseline (the math runs carry their own internal
/// numerator/denominator offsets around it).
fn glyph_ys(commands: &[VectorCommand]) -> Vec<f32> {
    draw_text(commands)
        .0
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.y))
        .collect()
}

/// Runs grouped by baseline, each entry `(text, first glyph x)`.
fn rows(runs: &[PositionedGlyphRun]) -> Vec<Vec<(String, f32)>> {
    let mut out: Vec<Vec<(String, f32)>> = Vec::new();
    let mut last_y: Option<f32> = None;
    for run in runs {
        let Some(first) = run.glyphs.first() else {
            continue;
        };
        if last_y.is_none_or(|y| (y - first.y).abs() > 1.0) {
            out.push(Vec::new());
            last_y = Some(first.y);
        }
        if let Some(row) = out.last_mut() {
            row.push((run.text.clone(), first.x));
        }
    }
    out
}

fn xs(row: &[(String, f32)]) -> Vec<f32> {
    row.iter().map(|(_, x)| *x).collect()
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected}, got {actual}"
    );
}

/// ref renderer.ts:5693-5720: an explicit stop pins the following cell.
#[test]
fn explicit_tab_stops_place_the_cell_on_the_stop() {
    let left = body(
        "horz",
        "l",
        "none",
        "A\\tB",
        r#"[{"pos": 300000, "algn": "l"}]"#,
        "null",
    );
    let commands = compile(&left, 4_000_000, 1_000_000);
    let (runs, _) = draw_text(&commands);
    let row = rows(runs);
    let cells = &row[0];
    assert!(cells.len() >= 2, "expected a cell after the tab: {cells:?}");
    assert_eq!(cells[1].0, "B");
    close(cells[1].1, 300_000.0, "left stop");

    // `r` (and `dec`, which shares its rule) ends the cell on the stop.
    let right = body(
        "horz",
        "l",
        "none",
        "A\\tB",
        r#"[{"pos": 300000, "algn": "r"}]"#,
        "null",
    );
    let commands = compile(&right, 4_000_000, 1_000_000);
    let (runs, _) = draw_text(&commands);
    let row = rows(runs);
    let cell = &row[0][1];
    let advance: f32 = runs
        .iter()
        .find(|run| run.text == "B")
        .map(|run| run.glyphs.iter().map(|glyph| glyph.advance).sum())
        .unwrap_or(0.0);
    close(cell.1 + advance, 300_000.0, "right stop");
}

/// ref renderer.ts:5702-5706: with no reachable stop the pen snaps to the next
/// `defTabSz` grid line (else PowerPoint's 1-inch default).
#[test]
fn default_tab_grid_applies_without_explicit_stops() {
    let grid = body("horz", "l", "none", "A\\tB", "[]", "400000");
    let commands = compile(&grid, 4_000_000, 1_000_000);
    let (runs, _) = draw_text(&commands);
    let row = rows(runs);
    assert_eq!(row[0][1].0, "B");
    close(row[0][1].1, 400_000.0, "defTabSz grid");
}

/// ref renderer.ts:5917-5922 + `should_justify`: under `just` the paragraph's
/// final line stays natural, so widening the box moves nothing.
#[test]
fn just_leaves_the_last_line_ragged() {
    let text = "a b c";
    let width = 1_500_000;
    let just = body("horz", "just", "none", text, "[]", "null");
    let base = rows(draw_text(&compile(&just, width, 1_000_000)).0).remove(0);
    let wider = rows(draw_text(&compile(&just, width + 12, 1_000_000)).0).remove(0);
    assert_eq!(xs(&base).len(), xs(&wider).len());
    for (index, (before, after)) in xs(&base).iter().zip(xs(&wider).iter()).enumerate() {
        close(*after, *before, &format!("unit {index} must stay put"));
    }
}

/// `just` fills a NON-final line: two gaps, slack grows by 12, so the second
/// unit moves 6 and the third 12 (`text-justify.ts:93-141`).
#[test]
fn just_fills_a_non_final_line() {
    // The long tail word forces a second line at both widths.
    let text = "a b c xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let width = 1_500_000;
    let just = body("horz", "just", "square", text, "[]", "null");
    let base = rows(draw_text(&compile(&just, width, 1_000_000)).0);
    let wider = rows(draw_text(&compile(&just, width + 12, 1_000_000)).0);
    assert!(base.len() >= 2, "expected a wrapped paragraph: {base:?}");
    assert_eq!(base[0].len(), 3, "expected three units on line 1");
    let (before, after) = (xs(&base[0]), xs(&wider[0]));
    close(after[0] - before[0], 0.0, "first unit");
    close(after[1] - before[1], 6.0, "second unit");
    close(after[2] - before[2], 12.0, "third unit");
    // The final line is still ragged.
    let (last, wider_last) = (base.len() - 1, wider.len() - 1);
    for (lhs, rhs) in xs(&base[last]).iter().zip(xs(&wider[wider_last]).iter()) {
        close(*rhs, *lhs, "last line must stay natural");
    }
}

/// `dist` justifies every line, the last included (`text-justify.ts:95-96`).
#[test]
fn dist_justifies_a_single_line_paragraph() {
    let text = "a b c";
    let width = 1_500_000;
    let dist = body("horz", "dist", "none", text, "[]", "null");
    let base = rows(draw_text(&compile(&dist, width, 1_000_000)).0).remove(0);
    let wider = rows(draw_text(&compile(&dist, width + 12, 1_000_000)).0).remove(0);
    assert_eq!(base.len(), 3, "expected three justified units");
    let (before, after) = (xs(&base), xs(&wider));
    close(after[0] - before[0], 0.0, "first unit");
    close(after[1] - before[1], 6.0, "second unit");
    close(after[2] - before[2], 12.0, "third unit");
}

/// ref renderer.ts:5031-5083: `vert` lays the body out in the swapped box and
/// rotates the frame by `vertRot = +90` canvas degrees — the y-down clockwise
/// rotation, i.e. -90° in a y-up convention — about the box centre.
#[test]
fn vert_rotates_the_frame_about_the_box_centre() {
    let text_body = body("vert", "l", "none", "Hi", "[]", "null");
    let (box_w, box_h) = (2_000_000.0f32, 1_000_000.0f32);
    let commands = compile(&text_body, box_w as i64, box_h as i64);
    let (_, transform) = draw_text(&commands);
    // Canvas +90: x' = -y, y' = x.
    close(transform.sx, 0.0, "sx");
    close(transform.kx, -1.0, "kx");
    close(transform.ky, 1.0, "ky");
    close(transform.sy, 0.0, "sy");
    // Layout origin (top-left of the swapped box) lands on the box's top-right.
    close(transform.transform_point(0.0, 0.0).0, box_w, "origin x");
    close(transform.transform_point(0.0, 0.0).1, 0.0, "origin y");
    // The layout's +x (line flow) runs down the page, +y stacks leftwards.
    close(transform.transform_point(0.0, box_w).0, 0.0, "stack x");
    close(transform.transform_point(0.0, box_w).1, 0.0, "stack y");
    // The swapped layout centre is the box centre.
    let (cx, cy) = transform.transform_point(box_h / 2.0, box_w / 2.0);
    close(cx, box_w / 2.0, "centre x");
    close(cy, box_h / 2.0, "centre y");

    // `vert270` is the mirror image: `vertRot = -90`.
    let flipped = body("vert270", "l", "none", "Hi", "[]", "null");
    let commands = compile(&flipped, box_w as i64, box_h as i64);
    let (_, transform) = draw_text(&commands);
    close(transform.ky, -1.0, "vert270 ky");
    close(transform.kx, 1.0, "vert270 kx");
    close(
        transform.transform_point(0.0, 0.0).1,
        box_h,
        "vert270 origin",
    );
}

/// ref renderer.ts:5021-5028: `wordArtVert` stacks one grapheme per column,
/// `wordArtVertRtl` mirrors the column order.
#[test]
fn word_art_vert_stacks_one_grapheme_per_line() {
    let stacked = body("wordArtVert", "l", "none", "ABC", "[]", "null");
    let commands = compile(&stacked, 1_000_000, 1_000_000);
    let (runs, _) = draw_text(&commands);
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>(),
        ["A", "B", "C"],
        "one grapheme per column"
    );
    let columns = xs(&rows(runs).remove(0));
    assert!(
        columns.windows(2).all(|pair| pair[1] > pair[0]),
        "wordArtVert reads left to right: {columns:?}"
    );

    let rtl = body("wordArtVertRtl", "l", "none", "ABC", "[]", "null");
    let commands = compile(&rtl, 1_000_000, 1_000_000);
    let (runs, _) = draw_text(&commands);
    let columns = xs(&rows(runs).remove(0));
    assert!(
        columns.windows(2).all(|pair| pair[1] < pair[0]),
        "wordArtVertRtl reads right to left: {columns:?}"
    );
}

/// PRD 09 §1.2 / `compiler/text.rs:314`: an equation claims
/// `(ascent + descent) * 1.4` of the line box, so a display block's taller box
/// pushes the whole equation below where the same equation sits inline.
#[test]
fn display_math_makes_a_taller_line_box_than_inline() {
    let inline = glyph_ys(&compile(&math_body(false), 2_000_000, 1_000_000));
    let display = glyph_ys(&compile(&math_body(true), 2_000_000, 1_000_000));
    assert!(!inline.is_empty() && !display.is_empty());
    let inline_top = inline.iter().copied().fold(f32::INFINITY, f32::min);
    let display_top = display.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(
        display_top > inline_top,
        "display math falls below the inline numerator: inline={inline_top}, display={display_top}"
    );
    let inline_bottom = inline.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let display_bottom = display.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        display_bottom > inline_bottom,
        "display math ends below the inline denominator: inline={inline_bottom}, display={display_bottom}"
    );
}

// ── `rPr@baseline` superscript / subscript ──────────────────────────────────

const EMU_PER_PT: f32 = 12_700.0;

/// One single-run paragraph at `size_pt`, optionally carrying `rPr@baseline`
/// (thousandths of a point; positive = superscript). The run is authored with
/// an underline and a strike so the shrunk path can be checked to keep both.
fn baseline_body(text: &str, size_pt: f32, baseline: Option<i32>) -> pptx_model::TextBody {
    let baseline_field = baseline.map_or_else(String::new, |value| {
        format!(r#", "baseline": {value}"#)
    });
    let json = format!(
        r#"{{
            "verticalAnchor": "t",
            "defaultFontSize": {size_pt},
            "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
            "wrap": "none",
            "vert": "horz",
            "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [{{
                "alignment": "l",
                "marL": 0, "marR": 0, "indent": 0,
                "spaceBefore": null,
                "lvl": 0,
                "bullet": {{ "type": "none" }},
                "defFontSize": {size_pt},
                "tabStops": [],
                "defTabSz": null,
                "rtl": false,
                "eaLnBrk": true,
                "runs": [{{
                    "type": "text",
                    "text": "{text}",
                    "bold": false, "italic": false,
                    "underline": true, "strikethrough": true,
                    "strikeDouble": false,
                    "hyperlinkUsesTextFill": false,
                    "fieldType": null,
                    "fontSize": {size_pt},
                    "color": "000000",
                    "fontFamily": "Calibri"{baseline_field}
                }}]
            }}]
        }}"#
    );
    serde_json::from_str(&json).expect("baseline text body fixture")
}

/// The body's one emitted run.
fn only_run(commands: &[VectorCommand]) -> &PositionedGlyphRun {
    let (runs, _) = draw_text(commands);
    assert_eq!(runs.len(), 1, "expected a single run: {runs:?}");
    &runs[0]
}

fn run_advance(run: &PositionedGlyphRun) -> f32 {
    run.glyphs.iter().map(|glyph| glyph.advance).sum()
}

fn glyph_y(run: &PositionedGlyphRun) -> f32 {
    run.glyphs.first().expect("a shaped glyph").y
}

/// ref renderer.ts:1549-1553 `baselineDrawSizePx` + `:5981`: a `+30000`
/// baseline paints the 20 pt run at 13 pt and lifts it by
/// `0.3 × 20 pt = 6 pt` (the canvas is y-down, so positive = up).
#[test]
fn positive_baseline_superscripts_at_65_percent() {
    let plain_commands = compile(&baseline_body("x", POINTS, None), 4_000_000, 1_000_000);
    let plain = only_run(&plain_commands);
    let sup_commands = compile(
        &baseline_body("x", POINTS, Some(30_000)),
        4_000_000,
        1_000_000,
    );
    let sup = only_run(&sup_commands);

    close(sup.font_size_pt, 13.0, "superscript draw size");
    close(
        glyph_y(sup),
        glyph_y(plain) - 6.0 * EMU_PER_PT,
        "superscript lift",
    );
    // Authored face, weight, colour and decorations survive the shrink.
    assert_eq!(sup.font_family, plain.font_family);
    assert_eq!((sup.bold, sup.italic), (plain.bold, plain.italic));
    assert_eq!(sup.color, plain.color);
    assert!(sup.underline && sup.strikethrough, "decorations kept");

    // The glyph advance is the 13 pt advance, not the authored 20 pt one.
    let thirteen_commands = compile(&baseline_body("x", 13.0, None), 4_000_000, 1_000_000);
    let thirteen = only_run(&thirteen_commands);
    close(run_advance(sup), run_advance(thirteen), "advance at 0.65 size");
    assert!(
        run_advance(sup) < run_advance(plain),
        "the shrunk run must advance less than the authored one: {} vs {}",
        run_advance(sup),
        run_advance(plain)
    );
}

/// `baseline=-25000` on a 12.5 pt run: painted at `12.5 × 0.65 = 8.125 pt`,
/// lowered by `0.25 × 12.5 pt = 3.125 pt`.
#[test]
fn negative_baseline_subscripts_below_the_baseline() {
    let plain_commands = compile(&baseline_body("x", 12.5, None), 4_000_000, 1_000_000);
    let plain = only_run(&plain_commands);
    let sub_commands = compile(
        &baseline_body("x", 12.5, Some(-25_000)),
        4_000_000,
        1_000_000,
    );
    let sub = only_run(&sub_commands);

    close(sub.font_size_pt, 8.125, "subscript draw size");
    close(
        glyph_y(sub),
        glyph_y(plain) + 3.125 * EMU_PER_PT,
        "subscript drop",
    );
    assert!(
        run_advance(sub) < run_advance(plain),
        "the shrunk run must advance less: {} vs {}",
        run_advance(sub),
        run_advance(plain)
    );
}

/// `baseline=0` is the same as an absent baseline: full authored size, no
/// shift (the parser drops an explicit `0`, so both spellings reach the
/// emitter as "not shifted").
#[test]
fn zero_and_absent_baseline_render_identically() {
    let absent_commands = compile(&baseline_body("x", POINTS, None), 4_000_000, 1_000_000);
    let absent = only_run(&absent_commands);
    let zero_commands = compile(&baseline_body("x", POINTS, Some(0)), 4_000_000, 1_000_000);
    let zero = only_run(&zero_commands);

    close(zero.font_size_pt, POINTS, "authored size kept");
    close(zero.font_size_pt, absent.font_size_pt, "size");
    close(glyph_y(zero), glyph_y(absent), "baseline");
    close(run_advance(zero), run_advance(absent), "advance");
}

/// ref renderer.ts:1550 `The authored size still owns line height`: the
/// shrunk run keeps its full 1.2 em box, so the baseline a `+30000` run
/// shares with a plain run of the same authored size is unchanged.
#[test]
fn baseline_run_keeps_the_authored_line_box() {
    let plain_commands = compile(&baseline_body("x", POINTS, None), 4_000_000, 1_000_000);
    let plain = only_run(&plain_commands);
    let sup_commands = compile(
        &baseline_body("x", POINTS, Some(30_000)),
        4_000_000,
        1_000_000,
    );
    let sup = only_run(&sup_commands);
    // Top-anchored single line: the line's baseline sits at the box ascent, so
    // removing the shift must land exactly on the plain 20 pt baseline.
    close(
        glyph_y(sup) + 6.0 * EMU_PER_PT,
        glyph_y(plain),
        "line box height",
    );

    // The line box follows the authored size, not the 13 pt draw size.
    let thirteen_commands = compile(&baseline_body("x", 13.0, None), 4_000_000, 1_000_000);
    let thirteen = only_run(&thirteen_commands);
    assert!(
        (glyph_y(plain) - glyph_y(thirteen)).abs() > 1_000.0,
        "a 13 pt run must sit on a shorter line box: {} vs {}",
        glyph_y(plain),
        glyph_y(thirteen)
    );
}
