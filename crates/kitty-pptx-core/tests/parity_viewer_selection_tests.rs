//! Viewer-selection parity batch — ports of the reference viewer's element
//! hit-testing, run-level find controller, text-layer run boxes and highlight
//! layer geometry.
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/`
//! `element-selection.test.ts` (5 cases), `find.test.ts` (3),
//! `text-layer.test.ts` (11), `text-layer-hyperlink.test.ts` (4),
//! `find-highlight-layer.test.ts` (8), `selection-context.test.ts` (1),
//! `slide-cursor-operation.test.ts` (3).
//!
//! The Rust pipeline has no DOM: the overlay/highlight layer cases are N/A
//! (browser `Range`/`<span>`), and the geometry they position is owned by
//! `compiler::hyperlinks` — a run's hyperlink is emitted with that run's own
//! advance-width box, clamped to the shape's text frame. That half is asserted
//! here (ref text-layer.test.ts:64-108, text-layer-hyperlink.test.ts:81-104).
//!
//! BLOCKED (no production API, see `tests/parity/coverage/viewer.md`):
//! `hitTestPptxSlideContext` / `hitTestPptxElement` / `limitPptxElementContext`
//! (no slide-element hit-test or element-context projection),
//! `PptxFindController` / `UnitFindController` (no run-level find index),
//! `buildPptxHighlightLayer` (no find highlight geometry) and
//! `loadPptxSlideFromCursor` (no pull/ACK cursor operation).
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_viewer_selection_tests.rs.txt`; the
//! ledger keeps their rows.

use std::collections::BTreeMap;

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{HyperlinkTarget, RectF32, SlideLink, SlideLinkIndex};
use kitty_pptx_core::extract_slide_hyperlinks;
use pptx_model::{
    Bullet, Paragraph, ShapeElement, Slide, SlideElement, TextBody, TextRun, TextRunData,
};

// ── model builders ─────────────────────────────────────────────────────────

/// One text run; `hyperlink` is the `rPr/hlinkClick` rel Target.
fn run(text: &str, size_pt: f64, hyperlink: Option<&str>) -> TextRunData {
    TextRunData {
        text: text.to_string(),
        bold: None,
        italic: None,
        underline: false,
        underline_style: None,
        underline_color: None,
        underline_fill: None,
        underline_line: None,
        underline_line_no_fill: false,
        strikethrough: false,
        strike_double: false,
        font_size: Some(size_pt),
        color: None,
        pattern_fill: None,
        glyph_fill: None,
        no_fill: false,
        font_family: None,
        font_family_ea: None,
        font_family_cs: None,
        font_family_sym: None,
        lang: None,
        alt_lang: None,
        baseline: None,
        caps: None,
        letter_spacing: None,
        field_type: None,
        hyperlink: hyperlink.map(str::to_string),
        hyperlink_uses_text_fill: false,
        hyperlink_action: None,
        hyperlink_mouse_over: None,
        hyperlink_mouse_over_action: None,
        shadow: None,
        reflection: None,
        outline: None,
        highlight: None,
        character_attributes: BTreeMap::new(),
        character_child_attributes: BTreeMap::new(),
    }
}

fn paragraph(runs: Vec<TextRunData>) -> Paragraph {
    Paragraph {
        alignment: "l".to_string(),
        mar_l: 0,
        mar_r: 0,
        indent: 0,
        space_before: None,
        space_after: None,
        space_before_pct: None,
        space_after_pct: None,
        space_line: None,
        lvl: 0,
        bullet: Bullet::None,
        def_font_size: None,
        def_color: None,
        def_bold: None,
        def_italic: None,
        def_font_family: None,
        tab_stops: Vec::new(),
        def_tab_sz: None,
        rtl: false,
        ea_ln_brk: true,
        font_algn: None,
        runs: runs.into_iter().map(TextRun::Text).collect(),
        math_fallbacks: Vec::new(),
        end_run_properties: None,
    }
}

fn text_body(paragraphs: Vec<Paragraph>) -> TextBody {
    TextBody {
        vertical_anchor: "t".to_string(),
        paragraphs,
        default_font_size: Some(18.0),
        default_bold: None,
        default_italic: None,
        l_ins: 91_440,
        r_ins: 91_440,
        t_ins: 45_720,
        b_ins: 45_720,
        wrap: "square".to_string(),
        vert: "horz".to_string(),
        auto_fit: "none".to_string(),
        font_scale: None,
        ln_spc_reduction: None,
        num_col: 1,
        spc_col: 0,
        rtl_col: false,
        spc_first_last_para: false,
        anchor_ctr: false,
        compat_ln_spc: None,
        text_warp: None,
    }
}

fn text_shape(x: i64, y: i64, width: i64, height: i64, body: TextBody) -> ShapeElement {
    ShapeElement {
        math_fallback: None,
        x,
        y,
        width,
        height,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        geometry: "rect".to_string(),
        fill: None,
        stroke: None,
        text_body: Some(body),
        default_text_color: None,
        cust_geom: None,
        cust_geom_paint: None,
        adj: None,
        adj2: None,
        adj3: None,
        adj4: None,
        adj5: None,
        adj6: None,
        adj7: None,
        adj8: None,
        shadow: None,
        inner_shadow: None,
        glow: None,
        soft_edge: None,
        reflection: None,
        scene3d: None,
        sp3d: None,
        id: Some("42".to_string()),
        name: Some("Run frame".to_string()),
        hyperlink: None,
        hyperlink_action: None,
        placeholder_type: None,
        placeholder_idx: None,
        text_rect: None,
    }
}

fn slide_with(elements: Vec<SlideElement>) -> Slide {
    serde_json::from_value(serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "partName": "ppt/slides/slide1.xml",
        "elements": [],
        "elementSources": [],
        "comments": []
    }))
    .map(|mut slide: Slide| {
        slide.elements = elements;
        slide
    })
    .expect("slide JSON")
}

fn links_of(slide: &Slide) -> Vec<SlideLink> {
    let (tx, _rx) = unbounded();
    extract_slide_hyperlinks(slide, std::slice::from_ref(slide), &tx)
}

/// 18 pt in EMU (`1 pt = 12,700 EMU`) — the reference's `sizeEmu`.
const SIZE_EMU_18PT: f32 = 18.0 * 12_700.0;
/// A lowercase Latin glyph advances half an em (`estimate_text_width`).
const ADVANCE_LOWER: f32 = SIZE_EMU_18PT * 0.5;

// ── run-frame boxes (ref text-layer.test.ts:64-108) ────────────────────────

/// "positions each run inside its shape frame": the second run of a paragraph
/// starts after the first run's own advance, not at the frame origin.
/// ref text-layer.test.ts:64-100 (`inShapeX` per span), text-layer-hyperlink.test.ts:81-104
#[test]
fn runs_position_inside_the_shape_frame_by_their_own_advance() {
    let body = text_body(vec![paragraph(vec![
        run("abc", 18.0, Some("https://example.com/first")),
        run("de", 18.0, Some("https://example.com/second")),
    ])]);
    let slide = slide_with(vec![SlideElement::Shape(text_shape(
        0, 0, 1_000_000, 500_000, body,
    ))]);

    let links = links_of(&slide);
    assert_eq!(links.len(), 2, "one link per hyperlinked run");

    // x = lIns; y = tIns; width = 3 * 0.5 em; height = 1.2 * 18 pt.
    let first = links[0].bounds_emu;
    assert_eq!(
        first,
        RectF32::new(91_440.0, 45_720.0, 3.0 * ADVANCE_LOWER, SIZE_EMU_18PT * 1.2)
    );

    // The second run starts one "abc" advance to the right of the first.
    let second = links[1].bounds_emu;
    assert_eq!(
        second.x,
        91_440.0 + 3.0 * ADVANCE_LOWER,
        "run 2 is offset by run 1's advance"
    );
    assert_eq!(second.y, 45_720.0);
    assert_eq!(second.w, 2.0 * ADVANCE_LOWER);
    assert_eq!(second.h, SIZE_EMU_18PT * 1.2);
}

/// "keeps text selectable when a rendered line overflows its shape frame": the
/// run box is clamped to the frame, and degenerates to the frame when the line
/// starts outside it. ref text-layer.test.ts:81-85, compiler/hyperlinks.rs:470-484
#[test]
fn run_boxes_are_clamped_to_the_shape_frame() {
    // Frame narrower than the run: the box is clipped at the right edge.
    let body = text_body(vec![paragraph(vec![run(
        "abc",
        18.0,
        Some("https://example.com/"),
    )])]);
    let narrow = slide_with(vec![SlideElement::Shape(text_shape(
        0,
        0,
        200_000,
        500_000,
        body.clone(),
    ))]);
    let link = &links_of(&narrow)[0];
    assert_eq!(link.bounds_emu.x, 91_440.0);
    assert_eq!(link.bounds_emu.w, 200_000.0 - 91_440.0);
    assert_eq!(link.bounds_emu.h, SIZE_EMU_18PT * 1.2);

    // Frame narrower than the left inset: nothing survives, the frame is kept.
    let tiny = slide_with(vec![SlideElement::Shape(text_shape(
        0, 0, 50_000, 500_000, body,
    ))]);
    let link = &links_of(&tiny)[0];
    assert_eq!(link.bounds_emu, RectF32::new(0.0, 0.0, 50_000.0, 500_000.0));
}

/// "makes a hyperlink run clickable ... and leaves a plain run plain": only the
/// hyperlinked run contributes a link, and it carries that run's own target.
/// ref text-layer-hyperlink.test.ts:81-104
#[test]
fn only_the_hyperlinked_run_carries_a_target() {
    let body = text_body(vec![paragraph(vec![
        run("link", 18.0, Some("https://example.com/")),
        run("plain", 18.0, None),
    ])]);
    let slide = slide_with(vec![SlideElement::Shape(text_shape(
        0, 0, 1_000_000, 500_000, body,
    ))]);

    let links = links_of(&slide);
    assert_eq!(links.len(), 1, "the plain run adds no link");
    assert_eq!(
        links[0].target,
        HyperlinkTarget::ExternalUri("https://example.com/".to_string())
    );
    assert_eq!(links[0].bounds_emu.x, 91_440.0);
    assert_eq!(links[0].bounds_emu.w, 4.0 * ADVANCE_LOWER);
}

// ── element selection (ref element-selection.test.ts:38-48) ────────────────

/// "returns the topmost ... context": paint order decides which element a point
/// belongs to. The pipeline keeps the same source order in `SlideLinkIndex`, so
/// an overlapping point resolves to the LATER link. ref element-selection.test.ts:50-64
#[test]
fn topmost_link_wins_at_an_overlapping_point() {
    let bounds = RectF32::new(0.0, 0.0, 100.0, 50.0);
    let index = SlideLinkIndex::build(vec![
        SlideLink::new(
            HyperlinkTarget::ExternalUri("https://example.com/back".to_string()),
            bounds,
            None,
        ),
        SlideLink::new(
            HyperlinkTarget::ExternalUri("https://example.com/front".to_string()),
            bounds,
            None,
        ),
    ]);
    assert_eq!(index.len(), 2);
    assert_eq!(
        index.hit_test(20.0, 20.0).map(|link| link.target.clone()),
        Some(HyperlinkTarget::ExternalUri(
            "https://example.com/front".to_string()
        )),
        "the later (topmost) link wins"
    );
    // The same point still resolves to both when the caller wants the stack.
    assert_eq!(
        index.links_in_rect(RectF32::new(0.0, 0.0, 1.0, 1.0)).len(),
        2
    );
    // Outside every box nothing is hit.
    assert!(index.hit_test(500.0, 500.0).is_none());
}

// ── element selection contexts & find (ref element-selection / find) ────────

#[allow(clippy::too_many_arguments)]
fn sel_shape(
    x: i64, y: i64, w: i64, h: i64, rot: f64, geom: &str, id: &str, body: Option<serde_json::Value>,
) -> SlideElement {
    serde_json::from_value(serde_json::json!({
        "type": "shape", "x": x, "y": y, "width": w, "height": h, "rotation": rot, "flipH": false, "flipV": false,
        "geometry": geom, "fill": null, "stroke": null, "textBody": body, "id": id, "name": id,
    }))
    .expect("shape")
}

fn sel_slide(elements: Vec<SlideElement>) -> Slide {
    Slide {
        theme: None, index: 0, slide_number: 1, part_name: Some("ppt/slides/slide1.xml".into()), background: None,
        element_sources: elements.iter().enumerate().map(|(i, _)| pptx_model::SlideElementSource {
            origin: if i == 0 { pptx_model::SlideElementOrigin::Master } else { pptx_model::SlideElementOrigin::Slide },
        }).collect(),
        elements, notes: None, comments: Vec::new(), hidden: false, parse_error: None,
    }
}

fn sel_body_runs(runs: &[&str]) -> serde_json::Value {
    let run_values: Vec<serde_json::Value> = runs
        .iter()
        .map(|text| {
            serde_json::json!({
                "type": "text", "text": text, "bold": null, "italic": null,
                "underline": false, "strikethrough": false, "strikeDouble": false,
                "hyperlinkUsesTextFill": false, "fieldType": null,
                "fontSize": 20, "color": "000000", "fontFamily": "Arial"
            })
        })
        .collect();
    serde_json::json!({
        "verticalAnchor": "t", "defaultFontSize": 20, "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": "horz", "autoFit": "none",
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0, "lvl": 0, "bullet": { "type": "inherit" }, "tabStops": [], "rtl": false, "eaLnBrk": true,
            "runs": run_values
        }]
    })
}

fn sel_body(text: &str) -> serde_json::Value {
    sel_body_runs(&[text])
}

#[test]
fn topmost_element_context_reports_identity_and_origin() {
    use kitty_pptx_core::selection::{hit_test_slide_context, ElementContextOptions, SlidePoint};
    let slide = sel_slide(vec![
        sel_shape(0, 0, 100, 50, 0.0, "rect", "1", None),
        sel_shape(0, 0, 100, 50, 0.0, "roundRect", "2", None),
    ]);
    let ctx = hit_test_slide_context(0, &slide, SlidePoint { x: 20.0, y: 20.0 }, ElementContextOptions::default()).unwrap().unwrap();
    assert_eq!(ctx.element_index, 1);
    assert_eq!(ctx.origin, "slide");
    assert_eq!(ctx.shape_id.as_deref(), Some("2"));
    assert_eq!(ctx.geometry.as_deref(), Some("roundRect"));
}

#[test]
fn rotation_inversion_and_line_tolerance() {
    use kitty_pptx_core::selection::{hit_test_slide_element, SlidePoint};
    let rotated = sel_shape(25, 0, 50, 100, 90.0, "rect", "1", None);
    assert!(hit_test_slide_element(&rotated, SlidePoint { x: 0.0, y: 25.0 }, 0.0));
    let line = sel_shape(0, 0, 100, 0, 0.0, "line", "2", None);
    assert!(!hit_test_slide_element(&line, SlidePoint { x: 50.0, y: 4.0 }, 3.0));
    assert!(hit_test_slide_element(&line, SlidePoint { x: 50.0, y: 4.0 }, 5.0));
}

#[test]
fn bounded_element_text_never_splits_unicode() {
    use kitty_pptx_core::selection::{hit_test_slide_context, limit_element_context, ElementContextOptions, SlidePoint};
    let slide = sel_slide(vec![sel_shape(0, 0, 100, 50, 0.0, "rect", "1", Some(sel_body("\u{1F600}x")))]);
    let point = SlidePoint { x: 10.0, y: 10.0 };
    let empty = hit_test_slide_context(0, &slide, point, ElementContextOptions { tolerance: 0.0, max_text_characters: Some(1) }).unwrap().unwrap();
    assert_eq!(empty.text.as_deref(), Some(""));
    let emoji = hit_test_slide_context(0, &slide, point, ElementContextOptions { tolerance: 0.0, max_text_characters: Some(2) }).unwrap().unwrap();
    assert_eq!(emoji.text.as_deref(), Some("\u{1F600}"));
    let limited = limit_element_context(emoji, 1).unwrap();
    assert_eq!(limited.text.as_deref(), Some(""));
}

#[test]
fn chart_labels_stream_into_bounded_context_text() {
    use kitty_pptx_core::selection::bounded_chart_context_text;
    use ooxml_common::chart::ChartSeries;
    use pptx_model::{ChartElement, ChartModel};
    let chart = ChartElement {
        id: Some("7".into()), x: 0, y: 0, width: 100, height: 50, rotation: 0.0, flip_h: false, flip_v: false,
        chart: ChartModel {
            chart_type: "bar".into(), title: Some("Rev".into()), categories: vec!["Q1".into()],
            series: vec![ChartSeries { name: "FY".into(), values: vec![Some(10.0)], ..Default::default() }],
            ..Default::default()
        },
    };
    let summary = bounded_chart_context_text(&chart.chart, None);
    assert!(summary.text.as_deref().unwrap().contains("Rev"));
}

#[test]
fn find_maps_matches_and_highlights_to_slides() {
    use kitty_pptx_core::text::find::FindController;
    use pptx_model::Presentation;
    let s0 = sel_slide(vec![sel_shape(0, 0, 100, 50, 0.0, "rect", "1", Some(sel_body_runs(&["Hel", "lo"])))]);
    let s1 = sel_slide(vec![sel_shape(0, 0, 100, 50, 0.0, "rect", "2", Some(sel_body("hello")))]);
    let pres = Presentation { slide_width: 1000, slide_height: 1000, slides: vec![s0, s1], default_text_color: None, major_font: None, minor_font: None, hlink_color: None, fol_hlink_color: None };
    let mut find = FindController::from_presentation(&pres);
    let matches = find.find("hello", false);
    assert_eq!(matches.len(), 2);
    assert_eq!(find.next().map(|m| m.slide), Some(0));
    assert_eq!(find.next().map(|m| m.slide), Some(1));
    let hl0 = find.highlights(0);
    assert_eq!(hl0.len(), 1);
    assert!(!hl0[0].active);
    let hl1 = find.highlights(1);
    assert_eq!(hl1.len(), 1);
    assert!(hl1[0].active);
}

#[test]
fn find_rejects_matches_crossing_table_cells() {
    use kitty_pptx_core::text::find::{FindController, FindRun};
    use pptx_model::Presentation;
    let s0 = sel_slide(vec![sel_shape(0, 0, 100, 50, 0.0, "rect", "1", Some(sel_body("Hello")))]);
    let pres = Presentation { slide_width: 1000, slide_height: 1000, slides: vec![s0], default_text_color: None, major_font: None, minor_font: None, hlink_color: None, fol_hlink_color: None };
    let mut find = FindController::from_presentation(&pres);
    find.set_slide_runs(0, vec![
        FindRun { text: "foo".into(), element_index: 0, origin: "slide", shape_id: Some("1".into()), table_cell: Some((0, 0)) },
        FindRun { text: "bar".into(), element_index: 0, origin: "slide", shape_id: Some("1".into()), table_cell: Some((0, 1)) },
    ]);
    assert!(find.find("foobar", false).is_empty());
    assert_eq!(find.find("foo", false).len(), 1);
}
