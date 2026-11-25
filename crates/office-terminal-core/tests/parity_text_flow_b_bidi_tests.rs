//! Parity ports of the reference bidi/tab specs (batch `text-flow-b`):
//! `bidi-line.test.ts` (UAX#9 segment ordering) and `rtl-tab-stops.test.ts`
//! (reading-frame tab stops, issue #831/#916).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/bidi-line.test.ts`,
//! `packages/pptx/src/rtl-tab-stops.test.ts`, whose kernels are
//! `packages/core/src/text/bidi/{segment-line,line-order}.ts` and
//! `packages/core/src/text/bidi/uax9/rules.ts`.
//!
//! The tab cases run the reference's own geometry: box 600 px wide, no insets,
//! a stop 400 px from the LEADING inset edge, a `\t12` cell. The reference spec
//! measures every glyph at the font size with a mock canvas; this port measures
//! with its real metric face, so each case asserts the reference's *geometry*
//! (which edge lands on the mirrored stop, which reading position a cell starts
//! at) from the port's own measured advance instead of the mock's 20 px. Every
//! such expression is marked `[width]`; all other numbers are the spec's.

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use office_terminal_core::text::bidi::{compute_line_visual_order, segments_have_rtl, BidiSegment};
use office_terminal_core::text::emit::compile_text_body;
use office_terminal_core::text::shape;

const PT: f32 = 20.0;
/// `PT_TO_EMU` and the spec's `emuToPx` scale (`rtl-tab-stops.test.ts:34`).
const EMU_PER_PX: f32 = 12700.0;
const BOX_W_PX: f32 = 600.0;
const TAB_POS_PX: f32 = 400.0;
const CELL: &str = "12";

/// The reference mock measures every glyph at the font size; the port measures
/// the same cell with the resolved metric face. `[width]`
fn cell_width_px() -> f32 {
    shape::measure(CELL, "Carlito", PT * EMU_PER_PX, false, false) / EMU_PER_PX
}

fn px(value: f32) -> f32 {
    value / EMU_PER_PX
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected} px, got {actual} px"
    );
}

// ---------------------------------------------------------------------------
// bidi-line.test.ts — UAX#9 L2 segment ordering (issue #916)
// ---------------------------------------------------------------------------

/// The reference's segment view: a text segment, a textless object segment
/// (`{}`, i.e. inline math / picture marker) and an inline tab (`isTab: true`).
fn visual_texts<'a>(segments: &[BidiSegment<'a>], order: &[usize]) -> Vec<&'a str> {
    order
        .iter()
        .map(|index| match segments[*index] {
            BidiSegment::Text(text) => text,
            BidiSegment::Object => "[obj]",
            BidiSegment::Tab => "TAB",
        })
        .collect()
}

/// `:4-11`: detects Arabic/Hebrew, ignores Latin and objects.
#[test]
fn bidi_gate_detects_strong_rtl_and_ignores_latin_and_objects() {
    assert!(!segments_have_rtl(&[BidiSegment::Text("Hello world")]));
    assert!(!segments_have_rtl(&[
        BidiSegment::Text("price 99"),
        BidiSegment::Object
    ]));
    assert!(segments_have_rtl(&[BidiSegment::Text("مرحبا")]));
    assert!(segments_have_rtl(&[
        BidiSegment::Text("abc "),
        BidiSegment::Text("שלום")
    ]));
}

/// `:15-19`: pure-LTR segments keep logical order.
#[test]
fn bidi_keeps_pure_ltr_segments_in_logical_order() {
    let segments = [BidiSegment::Text("Hello "), BidiSegment::Text("world")];
    let visual = compute_line_visual_order(&segments, false);
    assert_eq!(visual.order, [0, 1]);
    assert_eq!(visual.rtl, [false, false]);
}

/// `:21-25`: pure-RTL segments reverse (the last word is visually leftmost).
#[test]
fn bidi_reverses_pure_rtl_segments_and_marks_them_rtl() {
    let segments = [BidiSegment::Text("שלום "), BidiSegment::Text("עולם")];
    let visual = compute_line_visual_order(&segments, true);
    assert_eq!(visual.order, [1, 0]);
    assert_eq!(visual.rtl, [true, true]);
}

/// `:27-36`: an embedded LTR run inside an RTL line. Logical Hebrew, Latin,
/// Hebrew (base RTL) -> visual L->R `[2, 1, 0]`, the Latin segment LTR.
#[test]
fn bidi_orders_an_embedded_ltr_run_inside_an_rtl_line() {
    let segments = [
        BidiSegment::Text("שלום "),
        BidiSegment::Text("world "),
        BidiSegment::Text("טוב"),
    ];
    let visual = compute_line_visual_order(&segments, true);
    assert_eq!(visual.order, [2, 1, 0]);
    assert!(!visual.rtl[1], "the Latin segment resolves LTR");
    assert!(visual.rtl[0]);
    assert!(visual.rtl[2]);
}

/// `:38-42`: a neutral object between two Hebrew words stays in the RTL flow.
#[test]
fn bidi_places_a_neutral_object_by_surrounding_direction() {
    let segments = [
        BidiSegment::Text("שלום "),
        BidiSegment::Object,
        BidiSegment::Text("טוב"),
    ];
    let visual = compute_line_visual_order(&segments, true);
    assert_eq!(visual.order, [2, 1, 0]);
}

/// `:44-63` (issue #916 item 2): a tab is a Segment Separator, not a neutral
/// object. L1/L2 reset it to the paragraph level and reorder each tab-delimited
/// cell independently, so an RTL paragraph's cells appear mirrored.
/// Logical `[A][TAB][B][TAB][C]`, base RTL -> visual `[C, TAB, B, TAB, A]`.
#[test]
fn bidi_reorders_ltr_cells_in_mirrored_order_under_an_rtl_base() {
    let segments = [
        BidiSegment::Text("A"),
        BidiSegment::Tab,
        BidiSegment::Text("B"),
        BidiSegment::Tab,
        BidiSegment::Text("C"),
    ];
    let visual = compute_line_visual_order(&segments, true);
    assert_eq!(visual_texts(&segments, &visual.order), ["C", "TAB", "B", "TAB", "A"]);
}

/// `:65-68`: the same cells stay in logical order under an LTR base.
#[test]
fn bidi_keeps_tab_delimited_ltr_cells_in_logical_order() {
    let segments = [BidiSegment::Text("A"), BidiSegment::Tab, BidiSegment::Text("B")];
    let visual = compute_line_visual_order(&segments, false);
    assert_eq!(visual.order, [0, 1, 2]);
    assert_eq!(visual.rtl, [false, false, false]);
}

/// `:70-85` (item 2): mixed-direction content INSIDE a tab cell. A cell after a
/// tab holding a Hebrew word then a Latin word under an RTL base puts the Latin
/// word visually LEFT of the Hebrew word: visual L->R `[CD, אב, TAB]`.
#[test]
fn bidi_reorders_mixed_direction_content_inside_a_tab_cell() {
    let segments = [BidiSegment::Tab, BidiSegment::Text("אב"), BidiSegment::Text("CD")];
    let visual = compute_line_visual_order(&segments, true);
    assert_eq!(visual_texts(&segments, &visual.order), ["CD", "אב", "TAB"]);
    assert!(visual.rtl[1], "the Hebrew cell content resolves RTL");
    assert!(!visual.rtl[2], "the Latin cell content resolves LTR");
}

// ---------------------------------------------------------------------------
// rtl-tab-stops.test.ts — reading frame + UAX#9 L2 (issues #831/#916)
// ---------------------------------------------------------------------------

/// One paragraph with one run over exactly the fields
/// `text::emit::build_paragraphs` reads. `stops` is the `a:tabLst` list,
/// `mar_r` the paragraph's logical-leading (physical right) margin in px.
fn json_str(value: &str) -> String {
    serde_json::to_string(value).expect("JSON string literal")
}

fn tab_body(stops: &[(f32, &str)], rtl: bool, algn: &str, mar_r: f32, text: &str) -> pptx_model::TextBody {
    let stop_list: Vec<String> = stops
        .iter()
        .map(|(pos, algn)| format!(r#"{{"pos": {}, "algn": "{algn}"}}"#, (pos * EMU_PER_PX) as i64))
        .collect();
    let json = format!(
        concat!(
            r#"{{"verticalAnchor": "t", "defaultFontSize": {pt},"#,
            r#""lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,"#,
            r#""wrap": "square", "vert": "horz", "autoFit": "none","#,
            r#""numCol": 1, "spcCol": 0, "rtlCol": false,"#,
            r#""spcFirstLastPara": false, "anchorCtr": false,"#,
            r#""paragraphs": [{{"alignment": "{algn}","#,
            r#""marL": 0, "marR": {mar_r}, "indent": 0, "spaceBefore": null, "lvl": 0,"#,
            r#""bullet": {{"type": "none"}}, "defFontSize": {pt},"#,
            r#""defFontFamily": null, "tabStops": [{stops}],"#,
            r#""rtl": {rtl}, "eaLnBrk": true,"#,
            r#""runs": [{{"type": "text", "text": {text},"#,
            r#""bold": false, "italic": false, "underline": false,"#,
            r#""strikethrough": false, "strikeDouble": false,"#,
            r#""hyperlinkUsesTextFill": false, "fieldType": null,"#,
            r#""fontSize": {pt}, "color": "000000","#,
            r#""fontFamily": null}}]}}]}}"#,
        ),
        pt = PT,
        algn = algn,
        mar_r = (mar_r * EMU_PER_PX) as i64,
        stops = stop_list.join(", "),
        rtl = rtl,
        text = json_str(text),
    );
    serde_json::from_str(&json).expect("text body fixture")
}

fn compile_runs(text_body: &pptx_model::TextBody) -> Vec<PositionedGlyphRun> {
    let (tx, _rx) = unbounded();
    let commands: Vec<VectorCommand> = compile_text_body(
        text_body,
        (BOX_W_PX * EMU_PER_PX) as i64,
        (400.0 * EMU_PER_PX) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    let mut out = Vec::new();
    for command in &commands {
        if let VectorCommand::DrawText { runs, .. } = command {
            out.extend(runs.iter().cloned());
        }
    }
    out
}

/// The pen x (EMU) of the painted run whose text is `text`.
fn pen_of(runs: &[PositionedGlyphRun], text: &str) -> Option<f32> {
    runs.iter()
        .find(|run| run.text == text)
        .and_then(|run| run.glyphs.first())
        .map(|glyph| glyph.x)
}

/// `:194-206` (1): `@algn="r"` in an RTL paragraph. The cell's TRAILING (left)
/// edge lands ON the mirrored stop — canvas x = `BOX_W − pos` = 200 — and the
/// LTR frame (x = 400 − cell width) is the regression guard.
#[test]
fn rtl_tab_right_cell_lands_on_the_mirrored_stop() {
    let body = tab_body(&[(TAB_POS_PX, "r")], true, "r", 0.0, "\t12");
    let runs = compile_runs(&body);
    let cell = pen_of(&runs, CELL).expect("cell run reported");
    close(px(cell), BOX_W_PX - TAB_POS_PX);
    let ltr_frame = TAB_POS_PX - cell_width_px();
    assert!(
        (px(cell) - ltr_frame).abs() > 1.0,
        "the cell took the LTR frame ({ltr_frame} px)"
    );
}

/// `:208-215` (2): `@algn="ctr"` centres the cell on the mirrored stop.
#[test]
fn rtl_tab_centre_cell_centres_on_the_mirrored_stop() {
    let body = tab_body(&[(TAB_POS_PX, "ctr")], true, "r", 0.0, "\t12");
    let runs = compile_runs(&body);
    let cell = pen_of(&runs, CELL).expect("cell run reported");
    close(px(cell) + cell_width_px() / 2.0, BOX_W_PX - TAB_POS_PX);
}

/// `:217-231` (3, item 2): tab-cell content flows through the per-segment UAX#9
/// pass, so a DIGITS cell (Bidi_Class EN -> even level under an RTL base) is
/// drawn LTR instead of the old blanket `rtl` direction.
///
/// BEHAVIOURAL: the port carries the resolved direction on the L2 result rather
/// than on `PositionedGlyphRun`, so the assertion is the kernel's own
/// `rtl[cell] == false`, plus the physical consequence that the digits paint
/// LEFT of the tab gap (case 1's position).
#[test]
fn rtl_tab_digits_cell_resolves_ltr() {
    let segments = [BidiSegment::Tab, BidiSegment::Text(CELL)];
    let visual = compute_line_visual_order(&segments, true);
    assert!(!visual.rtl[1], "a digits cell resolves LTR");
    assert!(visual.rtl[0], "the tab separator stays at the RTL level");
    let body = tab_body(&[(TAB_POS_PX, "r")], true, "r", 0.0, "\t12");
    close(px(pen_of(&compile_runs(&body), CELL).expect("cell run")), 200.0);
}

/// `:233-244` (4): an EXPLICIT `pPr@algn="l"` on an `rtl="1"` paragraph must
/// not turn the mirroring off — the stop frame follows the BASE DIRECTION.
#[test]
fn rtl_tab_mirrors_the_stop_under_an_explicit_left_alignment() {
    let body = tab_body(&[(TAB_POS_PX, "r")], true, "l", 0.0, "\t12");
    let runs = compile_runs(&body);
    close(px(pen_of(&runs, CELL).expect("cell run")), BOX_W_PX - TAB_POS_PX);
}

/// `:246-273` (5): a start (`l`) tab advances the reading-frame pen so a later
/// end stop stays reachable, and the start gap is MATERIALISED (item 3).
///
/// Stops: start@100, end@140, `marR` = 50. 'A' therefore begins at reading
/// position 100 and the cell's trailing edge sits on the 140 stop; the start gap
/// pulls 'A' off the leading (right) text edge instead of leaving it flush.
#[test]
fn rtl_tab_start_gap_is_materialised_and_keeps_the_end_stop_reachable() {
    let stops = [(100.0, "l"), (140.0, "r")];
    let body = tab_body(&stops, true, "r", 50.0, "\tA\t12");
    let runs = compile_runs(&body);
    let cell = pen_of(&runs, CELL).expect("end-stop cell exists");
    let a = pen_of(&runs, "A").expect("the start-tab cell is painted");
    let a_width = shape::measure("A", "Carlito", PT * EMU_PER_PX, false, false) / EMU_PER_PX;
    let leading_edge = BOX_W_PX - 50.0;
    // Item 3: the start ('l') tab gap is MATERIALISED — 50 px (the 100 stop past
    // the 50 reading-frame start pen) between 'A' and the leading (right) edge.
    // The pre-fix model silently advanced the pen, leaving 'A' flush at the edge.
    close(px(a) + a_width, leading_edge - 50.0);
    // The end ('r') stop at reading 140 is still reachable: the cell is seated at
    // reading `140 − cellWidth`, i.e. one cell left of 'A' shifted by the 40 px
    // stop pitch. `[width]`
    close(px(cell), px(a) - 40.0 + a_width);
}

/// `:275-286` (6): a stop past the TRAILING (left) text edge pins the cell at
/// that edge (Word never pushes the cell off the text area); unclamped, the
/// mirrored stop 600 − 700 = −100 would draw the cell at a negative x.
#[test]
fn rtl_tab_pins_the_cell_at_the_trailing_edge_when_the_stop_overflows() {
    let body = tab_body(&[(700.0, "r")], true, "r", 0.0, "\t12");
    let runs = compile_runs(&body);
    close(px(pen_of(&runs, CELL).expect("cell run reported")), 0.0);
}

/// `:288-296` (7): REGRESSION GUARD — the LTR path is unchanged: a right tab
/// still anchors at the LTR stop with the cell's RIGHT edge on it, and the cell
/// is drawn LTR.
#[test]
fn ltr_tab_keeps_the_cell_right_edge_on_the_stop() {
    let body = tab_body(&[(TAB_POS_PX, "r")], false, "l", 0.0, "\t12");
    let runs = compile_runs(&body);
    let cell = pen_of(&runs, CELL).expect("cell run reported");
    close(px(cell) + cell_width_px(), TAB_POS_PX); // `[width]`
    let segments = [BidiSegment::Tab, BidiSegment::Text(CELL)];
    let visual = compute_line_visual_order(&segments, false);
    assert_eq!(visual.order, [0, 1]);
    assert!(!visual.rtl[1], "the LTR cell draws LTR");
}
