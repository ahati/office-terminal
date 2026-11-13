//! `table-text-selection.test.ts` parity — the `cellHorzOverflow` admission
//! table (ECMA-376 §21.1.3.17 / §20.1.10.69).
//!
//! The reference's `PptxTextRunInfo` callback does not exist in this crate, so
//! the emitted per-cell `PushClip` is the observable form of
//! `cellHorzOverflow === 'clip'`: the clip is emitted exactly when the admission
//! block would set it.
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/table-text-selection.test.ts`.

// The table fixture is a deeply nested `json!` literal; serde_json's macro
// expands a few internal layers per JSON level.
#![recursion_limit = "512"]

use kitty_pptx_core::compiler::tables::compile_table_element;
use kitty_pptx_core::contracts::VectorCommand;
use serde_json::{json, Value};

// ───────────────────────────────────────────────────────────────────────────
// table-text-selection.test.ts — the `cellHorzOverflow` admission table.
// ref renderer.ts:7861-7878: only an effective horizontal body clips, and
// `^[ \t\r\n]*horz[ \t\r\n]*$` is the whole admission test (NBSP and internal
// whitespace cannot mean `horz`). The emitted `PushClip` is the observable
// form of `cellHorzOverflow === 'clip'`.
// ───────────────────────────────────────────────────────────────────────────

fn table_cell_json(vert: &str, horz_overflow: Option<&str>) -> Value {
    json!({
        "height": 20 * 12_700,
        "cells": [{
            "textBody": {
                "verticalAnchor": "ctr",
                "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
                "wrap": "square",
                "vert": vert,
                "autoFit": "none",
                "numCol": 1,
                "paragraphs": [{
                    "alignment": "l",
                    "marL": 0, "marR": 0, "indent": 0,
                    "spaceBefore": null,
                    "spaceAfter": null,
                    "spaceLine": null,
                    "lvl": 0,
                    "bullet": {"type": "none"},
                    "defFontSize": 12.0,
                    "tabStops": [],
                    "eaLnBrk": true,
                    "rtl": false,
                    "runs": [{
                        "type": "text",
                        "text": "A",
                        "bold": false,
                        "italic": false,
                        "underline": false,
                        "strikethrough": false,
                        "strikeDouble": false,
                        "hyperlinkUsesTextFill": false,
                        "fieldType": null,
                        "fontSize": 12.0,
                        "color": "000000",
                        "fontFamily": "Arial",
                    }],
                }],
            },
            "fill": null,
            "horzOverflow": horz_overflow,
            "borderL": null, "borderR": null, "borderT": null, "borderB": null,
            "gridSpan": 1, "rowSpan": 1,
            "hMerge": false, "vMerge": false,
        }],
    })
}

fn cell_clips(vert: &str) -> bool {
    cell_clips_with(vert, None)
}

fn cell_clips_with(vert: &str, horz_overflow: Option<&str>) -> bool {
    let value = json!({
        "x": 0, "y": 0,
        "width": 60 * 12_700, "height": 20 * 12_700,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "cols": [60 * 12_700],
        "rows": [table_cell_json(vert, horz_overflow)],
        "rtl": false,
    });
    let table: pptx_model::TableElement = serde_json::from_value(value).expect("table fixture");
    let (tx, _rx) = crossbeam_channel::unbounded();
    compile_table_element(&table, &tx)
        .iter()
        .any(|command| matches!(command, VectorCommand::PushClip(_)))
}

#[test]
fn table_cell_clips_for_model_direction_horz() {
    assert!(cell_clips("horz"));
}

#[test]
fn table_cell_clips_for_padded_horz_direction() {
    // `' \t horz\r\n'` — ST_TextVerticalType is xsd:token; only U+0020/TAB/LF/CR
    // padding may be collapsed.
    assert!(cell_clips(" \t horz\r\n"));
}

#[test]
fn table_cell_does_not_clip_for_vertical_model_direction() {
    assert!(!cell_clips("vert"));
}

#[test]
fn table_cell_does_not_clip_when_nbsp_wraps_horz() {
    assert!(!cell_clips("\u{a0}horz\u{a0}"));
}

#[test]
fn table_cell_does_not_clip_for_internal_whitespace_horz() {
    assert!(!cell_clips("ho\trz"));
}

#[test]
fn table_cell_does_not_clip_for_unknown_model_direction() {
    assert!(!cell_clips("unknown"));
}

#[test]
fn table_cell_horz_overflow_disables_the_horizontal_clip() {
    // Same admission block: `horzOverflow="overflow"` keeps the body unclipped.
    assert!(!cell_clips_with("horz", Some("overflow")));
    assert!(cell_clips_with("horz", Some("clip")));
}
