//! Additional test cases for stacked/horizontal text font slots, equation
//! fallbacks, and the font catalogue vrt2/vert feature matrix.

use super::*;

#[test]
fn horizontal_cluster_stays_in_the_base_segment() {
    let mut text_body = body("\u{845b}\u{e0100}B e\u{301}", "horz");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.font_family = Some("Arial".to_string());
        run.font_family_ea = Some("Yu Gothic".to_string());
    }
    let commands = compile(&text_body, 10_000.0, 100.0);
    let runs = draw_runs(&commands);
    let run_texts: Vec<String> = runs.iter().map(|r| r.text.clone()).collect();
    assert_eq!(run_texts, ["\u{845b}\u{e0100}", "B e\u{301}"]);
    assert!(runs[0].font_family.contains("Yu Gothic"));
    assert!(!runs[1].font_family.contains("Yu Gothic"));
}

#[test]
fn horizontal_cluster_split_across_runs_stays_in_the_base_segment() {
    let seams = [
        ("葛", "\u{e0100}"),
        ("「", "\u{fe0f}"),
        ("가", "\u{11a8}"),
    ];
    for (base, ext) in seams {
        let mut text_body = body(base, "horz");
        if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
            run.font_family = Some("Yu Gothic".to_string());
            run.bold = Some(false);
        }
        let second = json!({
            "type": "text",
            "text": format!("{ext}B"),
            "bold": true,
            "italic": null,
            "underline": false,
            "strikethrough": false,
            "strikeDouble": false,
            "hyperlinkUsesTextFill": false,
            "fieldType": null,
            "fontSize": FONT_PX,
            "color": "000000",
            "fontFamily": "Yu Gothic",
        });
        text_body.paragraphs[0].runs.push(serde_json::from_value(second).expect("second run"));
        let commands = compile(&text_body, 10_000.0, 100.0);
        let runs = draw_runs(&commands);
        let run_texts: Vec<String> = runs.iter().map(|r| r.text.clone()).collect();
        assert_eq!(run_texts, [format!("{base}{ext}"), "B".to_string()], "seam {base}+{ext}");
        assert!(runs[0].font_family.contains("Yu Gothic"));
        assert!(!runs[0].bold, "base cluster not bold");
        assert!(runs[1].bold, "second run bold");
    }
}

#[test]
fn stacked_equation_without_an_engine_keeps_the_blank_column() {
    for display in [false, true] {
        let make_body = |vert: &str| {
            let mut b = body("A", vert);
            let equation = json!({
                "type": "math",
                "fallbackText": null,
                "nodes": [],
                "display": display,
                "fontSize": 18
            });
            b.paragraphs[0].runs.push(serde_json::from_value(equation).expect("equation"));
            let text_b = json!({
                "type": "text", "text": "B", "bold": null, "italic": null,
                "underline": false, "strikethrough": false, "strikeDouble": false,
                "hyperlinkUsesTextFill": false, "fieldType": null,
                "fontSize": FONT_PX, "color": "000000", "fontFamily": "Arial"
            });
            b.paragraphs[0].runs.push(serde_json::from_value(text_b).expect("run B"));
            b
        };
        for vert in ["wordArtVert", "wordArtVertRtl"] {
            let commands = compile(&make_body(vert), 300.0, 300.0);
            let runs = draw_runs(&commands);
            let calls: Vec<&PositionedGlyphRun> =
                runs.iter().filter(|r| r.text == "A" || r.text == "B").collect();
            assert_eq!(calls.len(), 2, "both A and B drawn");
            if !display {
                let dy = (calls[1].glyphs[0].y - calls[0].glyphs[0].y).abs();
                assert!(dy > 1.5 * CELL * PT, "reserves blank cell: {dy} vs {}", CELL * PT);
            } else {
                let dx = (calls[1].glyphs[0].x - calls[0].glyphs[0].x).abs();
                assert!(dx > 1.5 * CELL * PT, "reserves blank column: {dx} vs {}", CELL * PT);
            }
        }
    }
}

#[test]
fn stacked_equation_conversion_failure_draws_the_fallback_text() {
    let mut b = body("A", "wordArtVert");
    let equation = json!({
        "type": "math",
        "fallbackText": null,
        "nodes": [{ "kind": "run", "text": "", "style": "roman" }],
        "display": false,
        "fontSize": 18
    });
    b.paragraphs[0].runs.push(serde_json::from_value(equation).expect("equation"));
    let text_b = json!({
        "type": "text", "text": "B", "bold": null, "italic": null,
        "underline": false, "strikethrough": false, "strikeDouble": false,
        "hyperlinkUsesTextFill": false, "fieldType": null,
        "fontSize": FONT_PX, "color": "000000", "fontFamily": "Arial"
    });
    b.paragraphs[0].runs.push(serde_json::from_value(text_b).expect("run B"));
    let commands = compile(&b, 300.0, 300.0);
    let runs = draw_runs(&commands);
    let texts: Vec<String> = runs.iter().map(|r| r.text.clone()).collect();
    let joined = texts.concat();
    assert!(joined.contains("Equation unavailable"), "joined: {joined}");
    assert!(joined.starts_with('A'));
    assert!(joined.ends_with('B'));
}

#[test]
fn stacked_equation_rasterization_failure_draws_the_fallback_text() {
    let mut b = body("A", "wordArtVert");
    let equation = json!({
        "type": "math",
        "fallbackText": "Equation unavailable",
        "nodes": [],
        "display": false,
        "fontSize": 18
    });
    b.paragraphs[0].runs.push(serde_json::from_value(equation).expect("equation"));
    let text_b = json!({
        "type": "text", "text": "B", "bold": null, "italic": null,
        "underline": false, "strikethrough": false, "strikeDouble": false,
        "hyperlinkUsesTextFill": false, "fieldType": null,
        "fontSize": FONT_PX, "color": "000000", "fontFamily": "Arial"
    });
    b.paragraphs[0].runs.push(serde_json::from_value(text_b).expect("run B"));
    let commands = compile(&b, 300.0, 300.0);
    let runs = draw_runs(&commands);
    let texts: Vec<String> = runs.iter().map(|r| r.text.clone()).collect();
    let joined = texts.concat();
    assert!(joined.contains("Equation unavailable"), "joined: {joined}");
}

#[test]
fn font_catalogue_per_family_vert_vrt2_matrix() {
    use kitty_pptx_core::text::face_features::{has_vert_feature, has_vrt2_feature};

    // Arial: neither vert nor vrt2
    for bold in [false, true] {
        assert!(!has_vert_feature("Arial", bold, false, '∥'), "Arial vert ∥");
        assert!(!has_vert_feature("Arial", bold, false, 'ー'), "Arial vert ー");
        assert!(!has_vrt2_feature("Arial", bold, false, '∥'), "Arial vrt2 ∥");
        assert!(!has_vrt2_feature("Arial", bold, false, 'ー'), "Arial vrt2 ー");
    }

    // Calibri: neither vert nor vrt2
    for bold in [false, true] {
        assert!(!has_vert_feature("Calibri", bold, false, '∥'), "Calibri vert ∥");
        assert!(!has_vert_feature("Calibri", bold, false, 'ー'), "Calibri vert ー");
        assert!(!has_vrt2_feature("Calibri", bold, false, '∥'), "Calibri vrt2 ∥");
        assert!(!has_vrt2_feature("Calibri", bold, false, 'ー'), "Calibri vrt2 ー");
    }

    // Yu Gothic: vrt2 only for ∥, vert for ー
    for bold in [false, true] {
        assert!(!has_vert_feature("Yu Gothic", bold, false, '∥'), "Yu Gothic no vert ∥");
        assert!(has_vrt2_feature("Yu Gothic", bold, false, '∥'), "Yu Gothic vrt2 ∥");
        assert!(has_vert_feature("Yu Gothic", bold, false, 'ー'), "Yu Gothic vert ー");
    }

    // MS Gothic: vert for ∥ and ー, no vrt2
    assert!(has_vert_feature("MS Gothic", false, false, '∥'), "MS Gothic vert ∥");
    assert!(!has_vrt2_feature("MS Gothic", false, false, '∥'), "MS Gothic no vrt2 ∥");
    assert!(has_vert_feature("MS Gothic", false, false, 'ー'), "MS Gothic vert ー");

    // Microsoft JhengHei: regular has vert for ー, bold does NOT
    assert!(has_vert_feature("Microsoft JhengHei", false, false, 'ー'), "JhengHei regular vert ー");
    assert!(!has_vert_feature("Microsoft JhengHei", true, false, 'ー'), "JhengHei bold lacks vert ー");
}
