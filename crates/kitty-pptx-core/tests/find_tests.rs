//! Run-level find (`find.test.ts`, V2).

use kitty_pptx_core::text::find::{FindController, FindRun};
use pptx_model::{Presentation, Slide, SlideElement, SlideElementSource};

fn text_run(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "text", "text": text, "bold": null, "italic": null,
        "underline": false, "strikethrough": false, "strikeDouble": false,
        "hyperlinkUsesTextFill": false, "fieldType": null,
        "fontSize": 20, "color": "000000", "fontFamily": "Arial"
    })
}

fn body(runs: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0, "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null, "tabStops": [],
            "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
            "runs": runs.iter().map(|text| text_run(text)).collect::<Vec<_>>()
        }],
        "defaultFontSize": 20, "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": "horz", "autoFit": "none"
    })
}

fn shape(runs: &[&str], id: &str) -> SlideElement {
    let value = serde_json::json!({
        "type": "shape", "x": 0, "y": 0, "width": 1000, "height": 500,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "rect", "fill": null, "stroke": null,
        "textBody": body(runs), "defaultTextColor": null, "custGeom": null,
        "adj": null, "adj2": null, "adj3": null, "adj4": null,
        "adj5": null, "adj6": null, "adj7": null, "adj8": null,
        "shadow": null, "innerShadow": null, "glow": null,
        "softEdge": null, "reflection": null, "id": id, "name": "Text",
        "hyperlink": null, "hyperlinkAction": null,
        "placeholderType": null, "placeholderIdx": null,
        "textRect": null, "scene3d": null, "sp3d": null
    });
    serde_json::from_value(value).expect("shape")
}

fn slide(index: usize, elements: Vec<SlideElement>, hidden: bool) -> Slide {
    Slide {
        theme: None,
        index,
        slide_number: index + 1,
        part_name: Some(format!("ppt/slides/slide{}.xml", index + 1)),
        background: None,
        element_sources: elements
            .iter()
            .map(|_| SlideElementSource {
                origin: pptx_model::SlideElementOrigin::Slide,
            })
            .collect(),
        elements,
        notes: None,
        comments: Vec::new(),
        hidden,
        parse_error: None,
    }
}

fn presentation(slides: Vec<Slide>) -> Presentation {
    Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides,
        default_text_color: Some("000000".into()),
        major_font: Some("Calibri".into()),
        minor_font: Some("Calibri".into()),
        hlink_color: None,
        fol_hlink_color: None,
    }
}

#[test]
fn maps_shared_search_results_and_highlights_to_slides() {
    let deck = presentation(vec![
        slide(0, vec![shape(&["Hel", "lo"], "1")], false),
        slide(1, vec![shape(&["hello"], "2")], false),
    ]);
    let mut find = FindController::from_presentation(&deck);
    let matches = find.find("hello", false);
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches.iter().map(|item| item.slide).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(find.next().map(|item| item.slide), Some(0));
    assert_eq!(find.active_slide(), Some(0));

    let highlights = find.highlights(0);
    assert_eq!(highlights.len(), 1);
    assert_eq!(highlights[0].slices.len(), 2);
    assert!(highlights[0].active);

    let runs = find.runs(1).expect("slide 1 runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "hello");
}

#[test]
fn matches_within_a_container_but_rejects_cross_cell_text() {
    let deck = presentation(vec![slide(0, vec![shape(&["Hel", "lo"], "1")], false)]);
    let mut find = FindController::from_presentation(&deck);
    assert_eq!(find.find("Hello", false).len(), 1);

    find.set_slide_runs(
        0,
        vec![
            FindRun {
                text: "foo".into(),
                element_index: 0,
                origin: "slide",
                shape_id: Some("1".into()),
                table_cell: Some((0, 0)),
            },
            FindRun {
                text: "bar".into(),
                element_index: 0,
                origin: "slide",
                shape_id: Some("1".into()),
                table_cell: Some((0, 1)),
            },
        ],
    );
    assert!(find.find("foobar", false).is_empty());
    assert_eq!(find.find("foo", false).len(), 1);
}

#[test]
fn skip_hidden_excludes_hidden_slides_from_matching() {
    let deck = presentation(vec![
        slide(0, vec![shape(&["visible"], "1")], false),
        slide(1, vec![shape(&["hidden hello"], "2")], true),
    ]);
    let mut find = FindController::from_presentation(&deck);
    find.set_skip_hidden(true);
    let matches = find.find("hello", false);
    assert!(matches.is_empty(), "hidden slide must not match: {matches:?}");

    find.set_skip_hidden(false);
    assert_eq!(find.find("hello", false).len(), 1);
}

#[test]
fn matches_are_case_insensitive_and_non_overlapping() {
    let deck = presentation(vec![slide(0, vec![shape(&["aaAA"], "1")], false)]);
    let mut find = FindController::from_presentation(&deck);
    assert_eq!(find.find("aa", false).len(), 2);
    assert_eq!(find.find("AA", false).len(), 2);
    assert_eq!(find.find("AA", true).len(), 1);
    assert_eq!(find.find("aa", true).len(), 1);
}
