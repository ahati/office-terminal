//! ODF shrink-to-fit (`TextBody.shrink_on_overflow`): the text compiler
//! derives the largest half-percent font scale whose wrapped layout fits the
//! frame's inner height, LibreOffice Impress style (ODF 1.3 Part 3 §20.358
//! `style:shrink-to-fit`). DrawingML never sets the flag; PowerPoint bodies
//! keep going through their stored `normAutofit@fontScale` (§5, PRD 02c).
//!
//! The cases assert behaviour, not font internals: a fitting body is left
//! alone, an overflowing body shrinks monotonically with the frame, an
//! unsatisfiable body clamps to the 10 % floor with a fidelity warning, and a
//! stored scale always wins.

use std::collections::BTreeMap;

use crossbeam_channel::unbounded;
use office_terminal_core::compiler::text::compile_text_body;
use office_terminal_core::contracts::{AffineTransform, VectorCommand, WarningEvent};
use pptx_model::{Bullet, Paragraph, TextBody, TextRun, TextRunData};

const BOX_W: i64 = 2_743_200; // 3 in — wide enough that nothing wraps.

fn identity() -> AffineTransform {
    AffineTransform {
        sx: 1.0,
        ky: 0.0,
        kx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    }
}

fn text_run(text: &str) -> TextRunData {
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
        font_size: Some(18.0),
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
        hyperlink: None,
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

fn paragraph(text: &str) -> Paragraph {
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
        runs: vec![TextRun::Text(text_run(text))],
        math_fallbacks: Vec::new(),
        end_run_properties: None,
    }
}

/// A one-paragraph, 18 pt body in a 3 in-wide frame with no insets, so the
/// frame height alone decides whether the layout fits.
fn body(shrink_on_overflow: bool, font_scale: Option<f64>) -> TextBody {
    TextBody {
        vertical_anchor: "t".to_string(),
        paragraphs: vec![paragraph("Shrink to fit")],
        shrink_on_overflow,
        default_font_size: Some(18.0),
        default_bold: None,
        default_italic: None,
        l_ins: 0,
        r_ins: 0,
        t_ins: 0,
        b_ins: 0,
        wrap: "square".to_string(),
        vert: "horz".to_string(),
        auto_fit: "none".to_string(),
        font_scale,
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

fn compile_sizes(body: &TextBody, box_h: i64) -> (Vec<f32>, Vec<WarningEvent>) {
    let (tx, rx) = unbounded::<WarningEvent>();
    let commands = compile_text_body(body, BOX_W, box_h, &identity(), None, &tx);
    let sizes: Vec<f32> = commands
        .iter()
        .flat_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => {
                runs.iter().map(|r| r.font_size_pt).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect();
    let mut warnings = Vec::new();
    while let Ok(event) = rx.try_recv() {
        warnings.push(event);
    }
    (sizes, warnings)
}

#[test]
fn body_that_fits_is_not_shrunk() {
    // 2 000 000 EMU ≈ 157 pt: two dozen lines of headroom for one line.
    let (sizes, warnings) = compile_sizes(&body(true, None), 2_000_000);
    assert!(!sizes.is_empty(), "the body must emit glyph runs");
    assert!(
        sizes.iter().all(|&s| s == 18.0),
        "a fitting body keeps its authored size, got {sizes:?}"
    );
    assert!(warnings.is_empty(), "no warning expected: {warnings:?}");
}

#[test]
fn overflowing_body_shrinks_monotonically_with_the_frame() {
    let (huge, huge_warnings) = compile_sizes(&body(true, None), 2_000_000);
    // 200 000 EMU ≈ 15.7 pt: a single 18 pt line box cannot fit.
    let (mid, mid_warnings) = compile_sizes(&body(true, None), 200_000);
    // Half the frame again: the derived scale cannot grow.
    let (tiny, tiny_warnings) = compile_sizes(&body(true, None), 100_000);

    assert!(huge.iter().all(|&s| s == 18.0), "huge frame: {huge:?}");
    assert!(
        mid.iter().all(|&s| s > 0.0 && s < 18.0),
        "the overflowing frame must shrink below the authored size, got {mid:?}"
    );
    let mid_max = mid.iter().copied().fold(0.0_f32, f32::max);
    let tiny_max = tiny.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        tiny_max <= mid_max,
        "a smaller frame cannot derive a larger scale: {tiny_max} vs {mid_max}"
    );
    assert!(huge_warnings.is_empty() && mid_warnings.is_empty() && tiny_warnings.is_empty());
}

#[test]
fn unsatisfiable_body_clamps_to_the_floor_and_warns() {
    // 20 000 EMU ≈ 1.6 pt: even a tenth of the line box overflows.
    let (sizes, warnings) = compile_sizes(&body(true, None), 20_000);
    assert!(
        sizes.iter().all(|&s| (s - 1.8).abs() < 0.01),
        "the floor clamps the scale to a tenth, got {sizes:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(
                w,
                WarningEvent::FidelityDegraded {
                    feature: "shrink_on_overflow_floor",
                    ..
                }
            )),
        "an unsatisfiable fit must warn, got {warnings:?}"
    );
}

#[test]
fn stored_scale_wins_over_the_shrink_flag() {
    // A stored `normAutofit@fontScale` already fixed the size (the DrawingML
    // path); the flag never triggers a search.
    let (sizes, warnings) = compile_sizes(&body(true, Some(0.5)), 2_000_000);
    assert!(
        sizes.iter().all(|&s| (s - 9.0).abs() < 0.01),
        "the stored 50 % scale applies verbatim, got {sizes:?}"
    );
    assert!(warnings.is_empty(), "no warning expected: {warnings:?}");
}

#[test]
fn flag_off_leaves_overflow_unshrunk() {
    let (sizes, warnings) = compile_sizes(&body(false, None), 200_000);
    assert!(
        sizes.iter().all(|&s| s == 18.0),
        "without the flag an overflowing body keeps its size, got {sizes:?}"
    );
    assert!(warnings.is_empty(), "no warning expected: {warnings:?}");
}
