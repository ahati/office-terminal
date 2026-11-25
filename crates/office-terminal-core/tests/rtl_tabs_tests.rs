//! RTL multi-cell tab lines (T4).

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{AffineTransform, VectorCommand};
use office_terminal_core::text::emit::compile_text_body;
use serde_json::json;

const PT: f32 = 12_700.0;

fn body_with(rtl: bool, stops: serde_json::Value, text: &str, indent: i64, alignment: &str) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": alignment,
            "marL": 0, "marR": 0, "indent": indent,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0, "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null,
            "tabStops": stops,
            "defTabSz": null, "rtl": rtl, "eaLnBrk": true, "fontAlgn": null,
            "runs": [{
                "type": "text", "text": text, "bold": null,
                "italic": null, "underline": false, "strikethrough": false,
                "strikeDouble": false, "hyperlinkUsesTextFill": false,
                "fieldType": null, "fontSize": 20.0, "color": "000000",
                "fontFamily": "Arial"
            }]
        }],
        "defaultFontSize": 20,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": "horz", "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false
    });
    serde_json::from_value(value).expect("body")
}

fn body(rtl: bool) -> pptx_model::TextBody {
    body_with(
        rtl,
        json!([
            { "pos": (300.0 * PT) as i64, "algn": "r" },
            { "pos": (560.0 * PT) as i64, "algn": "r" }
        ]),
        "title\tvalue\tpage",
        0,
        if rtl { "r" } else { "l" },
    )
}

fn runs_of(body: &pptx_model::TextBody) -> Vec<(String, f32, f32)> {
    let (tx, _rx) = unbounded();
    let commands = compile_text_body(
        body,
        (600.0 * PT) as i64,
        (400.0 * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    commands
        .into_iter()
        .filter_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => Some(runs),
            _ => None,
        })
        .flatten()
        .map(|run| {
            let x = run.glyphs.first().map(|g| g.x).unwrap_or(0.0);
            let w: f32 = run.glyphs.iter().map(|g| g.advance).sum();
            (run.text.clone(), x / PT, w / PT)
        })
        .collect()
}

fn runs(rtl: bool) -> Vec<(String, f32, f32)> {
    runs_of(&body(rtl))
}

fn run_pos(runs: &[(String, f32, f32)], text: &str) -> (f32, f32) {
    let (_, x, w) = runs
        .iter()
        .find(|(value, _, _)| value == text)
        .unwrap_or_else(|| panic!("missing run {text}"));
    (*x, *w)
}

#[test]
fn ltr_multi_cell_tabs_end_each_cell_on_its_stop() {
    let runs = runs(false);
    assert_eq!(runs.len(), 3);
    let (title_x, _) = run_pos(&runs, "title");
    let (value_x, value_w) = run_pos(&runs, "value");
    let (page_x, page_w) = run_pos(&runs, "page");
    assert!((title_x - 0.0).abs() < 0.01);
    assert!((value_x + value_w - 300.0).abs() < 0.01, "value ends on stop 300");
    assert!((page_x + page_w - 560.0).abs() < 0.01, "page ends on stop 560");
}

#[test]
fn rtl_multi_cell_tabs_mirror_the_cells() {
    let runs = runs(true);
    assert_eq!(runs.len(), 3);
    let (title_x, title_w) = run_pos(&runs, "title");
    let (value_x, _) = run_pos(&runs, "value");
    let (page_x, _) = run_pos(&runs, "page");
    assert!(page_x < value_x && value_x < title_x, "visual order reversed");
    assert!((value_x - 300.0).abs() < 0.01, "value leading edge on mirrored stop");
    assert!((page_x - 40.0).abs() < 0.01, "page leading edge on mirrored stop");
    assert!(
        (title_x + title_w - 600.0).abs() < 0.01,
        "title ends at the leading edge"
    );
}

#[test]
fn start_tab_materialises_the_gap() {
    let body = body_with(
        false,
        json!([{ "pos": (100.0 * PT) as i64, "algn": "l" }]),
        "A\tB",
        0,
        "l",
    );
    let runs = runs_of(&body);
    let (a_x, _) = run_pos(&runs, "A");
    let (b_x, _) = run_pos(&runs, "B");
    assert!((a_x - 0.0).abs() < 0.01);
    assert!((b_x - 100.0).abs() < 0.01, "B sits on the start stop");
}

#[test]
fn positive_indent_shifts_the_pen_but_not_the_absolute_stop() {
    let body = body_with(
        false,
        json!([{ "pos": (300.0 * PT) as i64, "algn": "r" }]),
        "title\tvalue",
        (50.0 * PT) as i64,
        "l",
    );
    let runs = runs_of(&body);
    let (title_x, _) = run_pos(&runs, "title");
    let (value_x, value_w) = run_pos(&runs, "value");
    assert!((title_x - 50.0).abs() < 0.01, "first line starts at the indent");
    assert!((value_x + value_w - 300.0).abs() < 0.01, "cell ends on the stop");
}
