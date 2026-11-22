//! Font-metric regression for the bundled Nunito Sans faces.
//!
//! The committed `sample-1` reference PNGs are the reference project's adopted
//! canvas outputs (see `tests/fixtures/parity/README.md`). Their title metrics
//! are Arial, not Nunito Sans: the measured ink widths (`STATE OF` ~448 px,
//! `THE` ~186 px, `FOREST` ~380 px) match Arial/Arimo (`446.9` / `185.7` /
//! `379.3` at 96 px/em), and Arial's `THE FOREST` advance is `608.0 px`, just
//! over the 606 px title budget, which is why the reference wraps onto three
//! lines. The capture host therefore substituted the unavailable webfont with
//! its generic sans fallback.
//!
//! The reference renderer requests Nunito Sans and its fixture loads Nunito
//! Sans v19 via `@fontsource/nunito-sans@5.2.7`; that face's HarfBuzz-kerned
//! `THE FOREST` advance is `570.7 px`, so it wraps onto two lines. We bundle
//! the same face (see `assets/fonts/NOTICE.md`): the upright static weights are
//! 5-9 % narrower than the Arial-substituted reference on `FOREST`, and no
//! instance satisfies both reference numbers and the 606 px wrap condition
//! (reaching 606 px requires a width scale that pushes `STATE OF` past 455 px).
//! These tests pin the bundled metrics so a face swap cannot silently change
//! wrapping away from the reference renderer's own face.
//!
//! Run with `--nocapture` to print the measured table.

const EMU_PER_PX: f32 = 12_700.0;
const EMU_PER_EM_96: f32 = 96.0 * EMU_PER_PX;

/// Advance width in CSS px at a 96 px/em (72 pt at 96 dpi), the reference
/// scale. The production measurer is `cosmic-text`/HarfBuzz, so kerning is
/// applied exactly as the canvas renderer applies it.
fn px(text: &str, bold: bool, italic: bool) -> f32 {
    office_terminal_core::text::shape::measure(text, "Nunito Sans", EMU_PER_EM_96, bold, italic)
        / EMU_PER_PX
}

fn assert_px(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() <= 2.0,
        "{label}: expected {expected:.1} px +/- 2.0, measured {actual:.1} px"
    );
}

#[test]
fn bundled_family_resolves_by_name() {
    let regular = px("STATE OF", false, false);
    let bold = px("STATE OF", true, false);
    let italic = px("STATE OF", false, true);

    assert!(
        regular > 0.0,
        "'Nunito Sans' regular did not resolve through text::shape::measure"
    );
    assert!(
        bold > regular,
        "bold face did not resolve or is not wider: regular {regular:.1}, bold {bold:.1}"
    );
    assert!(
        italic > 0.0,
        "italic face did not resolve through text::shape::measure"
    );
    assert_px(italic, regular, "italic 400 advance");
}

#[test]
fn regular_face_metrics_match_fontsource_v19() {
    // fontsource v19 / Google Fonts static v19, 96 px/em, HarfBuzz kerning.
    let table = [
        ("STATE OF", 435.5),
        ("THE", 186.8),
        ("FOREST", 359.2),
        ("THE FOREST", 570.7),
    ];
    for (text, expected) in table {
        let actual = px(text, false, false);
        println!("regular {text:>10}: {actual:6.1} px (expected {expected:6.1})");
        assert_px(actual, expected, text);
    }
}

#[test]
fn bold_and_italic_metrics_match_bundled_statics() {
    for (text, bold, italic, expected) in [
        ("STATE OF", true, false, 447.4),
        ("THE FOREST", true, false, 586.1),
        ("STATE OF", false, true, 435.5),
        ("THE FOREST", false, true, 570.7),
    ] {
        let actual = px(text, bold, italic);
        let label = format!("{text} bold={bold} italic={italic}");
        println!("{label:>32}: {actual:6.1} px (expected {expected:6.1})");
        assert_px(actual, expected, &label);
    }
}

#[test]
fn reference_wrap_threshold_needs_arial_substitution() {
    // The reference PNG wraps "STATE OF / THE / FOREST" because its
    // Arial-substituted face measures "THE FOREST" at 608.0 px > 606 px.
    let the_forest = px("THE FOREST", false, false);
    let state_of = px("STATE OF", false, false);

    println!("Nunito Sans regular: STATE OF {state_of:.1} px, THE FOREST {the_forest:.1} px");
    assert!(
        the_forest < 606.0,
        "bundled Nunito Sans unexpectedly reached the PowerPoint/Arial wrap width: {the_forest:.1} px"
    );

    // Guard the reason this face cannot reproduce the reference wrap: any
    // uniform widening that reaches 606 px also pushes "STATE OF" past 455 px.
    let scale_to_wrap = 606.0 / the_forest;
    let widened_state_of = state_of * scale_to_wrap;
    assert!(
        widened_state_of > 455.0,
        "the reference wrap condition and the STATE OF bound cannot be satisfied together \
         for this face: STATE OF would be {widened_state_of:.1} px at the wrap width"
    );
}
