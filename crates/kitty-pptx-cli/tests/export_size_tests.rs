//! PNG export sizing parity with the reference renderer's canvas contract.
//!
//! ref `packages/pptx/src/renderer.ts:8367-8370`:
//! `scale = targetWidth / slideWidth`, `canvasW = Math.round(targetWidth)`,
//! `canvasH = Math.round(slideHeight * scale)`. `Math.round` is round-half-up.
//!
//! Note: the LibreOffice `ShapePlusImage` fixture is 8640763x5759450 EMU.
//! `--width 1600` reproduces the reference canvas exactly (1600x1066). Without
//! it the width is derived from `--dpi`, and the nearest integer DPI (169)
//! encodes 1596.991 px, which half-up rounds to 1597; the height is then derived
//! from the rounded width so the slide aspect stays exact.

use clap::Parser;
use kitty_pptx_cli::cli::{Cli, Commands};
use kitty_pptx_cli::commands::export::{export_target_size, export_target_size_for_width};

/// LibreOffice `ShapePlusImage` fixture (`sldSz cx="8640763" cy="5759450"`).
const FIXTURE_W: i64 = 8_640_763;
const FIXTURE_H: i64 = 5_759_450;

#[test]
fn fixture_deck_at_169_dpi_rounds_up_instead_of_truncating() {
    // 8640763 / 914400 * 169 = 1596.991 -> 1597 (truncation produced 1596).
    // 5759450 * (1597 / 8640763) = 1064.47 -> 1064.
    assert_eq!(export_target_size(FIXTURE_W, FIXTURE_H, 169), (1597, 1064));
}

#[test]
fn fixture_deck_keeps_reference_aspect() {
    let (w, h) = export_target_size(FIXTURE_W, FIXTURE_H, 169);
    // 1597/1064 and the reference's 1600/1066 agree within one emitted pixel.
    assert!((w as f64 / h as f64 - 1600.0 / 1066.0).abs() < 0.002);
}

#[test]
fn explicit_width_reproduces_reference_canvas() {
    // The reference's 1600x1066 canvas is `targetWidth = 1600` over the fixture.
    assert_eq!(
        export_target_size_for_width(FIXTURE_W, FIXTURE_H, 1600),
        (1600, 1066)
    );
}

#[test]
fn explicit_width_is_exact_for_sixteen_nine() {
    assert_eq!(
        export_target_size_for_width(12_192_000, 6_858_000, 1280),
        (1280, 720)
    );
    assert_eq!(
        export_target_size_for_width(12_192_000, 6_858_000, 1920),
        (1920, 1080)
    );
}

#[test]
fn explicit_width_never_collapses_to_zero() {
    assert_eq!(export_target_size_for_width(FIXTURE_W, FIXTURE_H, 0), (1, 1));
    assert_eq!(export_target_size_for_width(0, 0, 1600), (1600, 1600));
}

#[test]
fn width_flag_parses_for_export() {
    // ref renderer.ts:8368: the caller may pin `targetWidth`; --width wins
    // over --dpi in `run_export`.
    let args = Cli::try_parse_from([
        "kitty-pptx",
        "export",
        "deck.pptx",
        "--out",
        "/tmp/out",
        "--dpi",
        "169",
        "--width",
        "1600",
    ])
    .expect("parsing CLI failed");
    match args.command {
        Some(Commands::Export { dpi, width, .. }) => {
            assert_eq!(dpi, Some(169));
            assert_eq!(width, Some(1600));
        }
        _ => panic!("Expected Export subcommand"),
    }
}

#[test]
fn sixteen_nine_at_96_dpi_is_exact() {
    assert_eq!(export_target_size(12_192_000, 6_858_000, 96), (1280, 720));
}

#[test]
fn round_half_up_matches_js_math_round() {
    // 960120 EMU * 10 dpi / 914400 = exactly 10.5 px: half-up yields 11, and the
    // aspect-preserving height is also 11.
    assert_eq!(export_target_size(960_120, 960_120, 10), (11, 11));
    // 1000000 EMU * 10 / 914400 = 10.936 -> 11.
    assert_eq!(export_target_size(1_000_000, 1_000_000, 10), (11, 11));
}

#[test]
fn dimensions_never_collapse_to_zero() {
    // 1000 EMU * 10 / 914400 = 0.0109 px -> clamped to 1.
    assert_eq!(export_target_size(1_000, 1_000, 10), (1, 1));
    // Degenerate/negative geometry still yields a paint-safe target.
    assert_eq!(export_target_size(0, 0, 96), (1, 1));
    assert_eq!(export_target_size(-12_192_000, -6_858_000, 96), (1, 1));
    // A zero-width slide has no aspect to preserve; stay paint-safe.
    assert_eq!(export_target_size(0, 6_858_000, 96), (1, 1));
    // Zero DPI must not produce a zero-sized canvas either.
    assert_eq!(export_target_size(12_192_000, 6_858_000, 0), (1, 1));
}
