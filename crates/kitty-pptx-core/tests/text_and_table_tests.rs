use kitty_pptx_core::*;
use crossbeam_channel::unbounded;
use pptx_model::*;

#[test]
fn test_text_layout_and_font_substitution() {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let slide_json = r#"{
        "index": 0,
        "slideNumber": 1,
        "elements": [
            {
                "type": "shape",
                "x": 0,
                "y": 0,
                "width": 1000000,
                "height": 500000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "geometry": "rect",
                "textBody": {
                    "verticalAnchor": "ctr",
                    "lIns": 91440,
                    "rIns": 91440,
                    "tIns": 45720,
                    "bIns": 45720,
                    "wrap": "square",
                    "vert": "horz",
                    "autoFit": "none",
                    "fontScale": 1.0,
                    "numCol": 1,
                    "spcCol": 0,
                    "rtlCol": false,
                    "spcFirstLastPara": false,
                    "anchorCtr": false,
                    "paragraphs": [
                        {
                            "alignment": "ctr",
                            "marL": 0,
                            "marR": 0,
                            "indent": 0,
                            "spaceBefore": 1000,
                            "lvl": 0,
                            "bullet": {
                                "type": "char",
                                "char": "l",
                                "fontFamily": "Wingdings"
                            },
                            "defFontSize": 24.0,
                            "defColor": "003366",
                            "defBold": true,
                            "defFontFamily": "Calibri",
                            "tabStops": [],
                            "rtl": false,
                            "eaLnBrk": true,
                            "runs": [
                                {
                                    "type": "text",
                                    "text": "Hello Antigravity",
                                    "bold": true,
                                    "italic": false,
                                    "underline": true,
                                    "strikeDouble": false,
                                    "hyperlinkUsesTextFill": false,
                                    "fieldType": null,
                                    "strikethrough": false,
                                    "fontSize": 24.0,
                                    "color": "003366",
                                    "fontFamily": "Aptos"
                                }
                            ]
                        }
                    ]
                }
            }
        ],
        "elementSources": [],
        "comments": []
    }"#;

    let slide: Slide = serde_json::from_str(slide_json).unwrap();
    let vs = compiler.compile(&slide, 2000000, 1000000, &tx).unwrap();

    // Two commands: DrawPath (the rect shape) and DrawText (the text)
    assert_eq!(vs.commands.len(), 2);

    match &vs.commands[1] {
        VectorCommand::DrawText { runs, .. } => {
            assert_eq!(runs.len(), 2); // Bullet run + text run
            // Bullet translated from Wingdings 'l' -> ●
            assert!(runs[0].text.contains('●'));
            // Aptos substituted with Carlito
            assert_eq!(runs[1].font_family, "Carlito");
            assert!(runs[1].underline);
        }
        _ => panic!("Expected DrawText command"),
    }

    // Aptos substitution emits FontSubstituted warning
    let warning = rx.try_recv().expect("expected FontSubstituted warning");
    match warning {
        WarningEvent::FontSubstituted { requested, substitute } => {
            assert_eq!(requested, "Aptos");
            assert_eq!(substitute, "Carlito");
        }
        _ => panic!("wrong warning event"),
    }
}

#[test]
fn test_table_compilation() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let slide_json = r#"{
        "index": 0,
        "slideNumber": 1,
        "elements": [
            {
                "type": "table",
                "id": "tbl1",
                "x": 100000,
                "y": 100000,
                "width": 600000,
                "height": 400000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "background": {
                    "fillType": "solid",
                    "color": "FAFAFA"
                },
                "cols": [300000, 300000],
                "rows": [
                    {
                        "height": 200000,
                        "cells": [
                            {
                                "fill": {
                                    "fillType": "solid",
                                    "color": "112233"
                                },
                                "borderT": {
                                    "color": "000000",
                                    "width": 12700
                                },
                                "gridSpan": 1,
                                "rowSpan": 1,
                                "hMerge": false,
                                "vMerge": false
                            },
                            {
                                "gridSpan": 1,
                                "rowSpan": 1,
                                "hMerge": false,
                                "vMerge": false
                            }
                        ]
                    },
                    {
                        "height": 200000,
                        "cells": [
                            {
                                "gridSpan": 2,
                                "rowSpan": 1,
                                "hMerge": false,
                                "vMerge": false
                            },
                            {
                                "gridSpan": 1,
                                "rowSpan": 1,
                                "hMerge": true,
                                "vMerge": false
                            }
                        ]
                    }
                ],
                "rtl": false
            }
        ],
        "elementSources": [],
        "comments": []
    }"#;

    let slide: Slide = serde_json::from_str(slide_json).unwrap();
    let vs = compiler.compile(&slide, 1000000, 1000000, &tx).unwrap();

    // Table bg + cell(0,0) fill + cell(0,0) top border
    assert!(!vs.commands.is_empty());
}
