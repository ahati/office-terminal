//! Document-level part-failure delivery (V4).

use office_terminal_core::collect_part_failures;

fn slide(index: usize, parse_error: Option<&str>) -> pptx_model::Slide {
    serde_json::from_value(serde_json::json!({
        "index": index,
        "slideNumber": index + 1,
        "partName": format!("ppt/slides/slide{}.xml", index + 1),
        "elements": [],
        "elementSources": [],
        "comments": [],
        "parseError": parse_error,
    }))
    .expect("slide")
}

fn deck(slides: Vec<pptx_model::Slide>) -> pptx_model::Presentation {
    pptx_model::Presentation {
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
fn failures_are_sorted_by_slide_and_deduplicated() {
    let presentation = deck(vec![
        slide(0, Some("ppt/slides/slide1.xml: bad")),
        slide(1, None),
        slide(2, Some("ppt/slides/slide3.xml: worse")),
    ]);
    let failures = collect_part_failures(&presentation);
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0].slide_index, 0);
    assert_eq!(failures[0].slide_number, 1);
    assert_eq!(failures[0].kind, "slide");
    assert!(failures[0].message.contains("slide1.xml"));
    assert_eq!(failures[1].slide_index, 2);
    assert_eq!(failures[1].slide_number, 3);
}

#[test]
fn healthy_decks_report_nothing() {
    let failures = collect_part_failures(&deck(vec![slide(0, None), slide(1, None)]));
    assert!(failures.is_empty());
}
