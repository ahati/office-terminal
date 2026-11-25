//! SmartArt fallback contrast helpers (`smartart-fallback-contrast.test.ts`).

use office_terminal_core::compiler::smartart_contrast::{
    background_luminance, hex_luma, is_smart_art_fallback_shape, smart_art_fallback_text_color,
};
use ooxml_common::fill::GradStop;
use pptx_model::Fill;

fn solid(hex: &str) -> Fill {
    Fill::Solid { color: hex.to_string() }
}

#[test]
fn solid_luminance_matches_rec601() {
    assert!(hex_luma("1F3864").is_some_and(|luma| luma < 0.5));
    assert!((hex_luma("FFFFFF").unwrap() - 1.0).abs() < 1e-5);
}

#[test]
fn translucent_solid_composites_over_white() {
    assert!((hex_luma("00000000").unwrap() - 1.0).abs() < 1e-5);
    assert!(hex_luma("000000FF").unwrap() < 1e-5);
}

#[test]
fn gradient_luminance_integrates_the_ramp() {
    let ramp = Fill::Gradient {
        stops: vec![
            GradStop {
                position: 0.0,
                color: "000000".into(),
            },
            GradStop {
                position: 1.0,
                color: "FFFFFF".into(),
            },
        ],
        angle: 90.0,
        grad_type: "linear".into(),
        scaled: None,
        path: None,
        fill_to_rect: None,
        tile_rect: None,
        flip: None,
        rot_with_shape: None,
    };
    assert!((background_luminance(Some(&ramp)).unwrap() - 0.5).abs() < 1e-5);
    assert!(background_luminance(Some(&solid("1F3864"))).unwrap() < 0.5);
    assert_eq!(background_luminance(None), None);
    assert_eq!(background_luminance(Some(&Fill::None)), None);
}

#[test]
fn contrast_default_only_replaces_dark_on_dark() {
    let navy = solid("1F3864");
    let white = solid("FFFFFF");
    assert_eq!(
        smart_art_fallback_text_color(Some(&navy), "383838").as_deref(),
        Some("FFFFFF")
    );
    assert_eq!(smart_art_fallback_text_color(Some(&white), "383838"), None);
    assert_eq!(smart_art_fallback_text_color(None, "383838"), None);
    assert_eq!(
        smart_art_fallback_text_color(Some(&navy), "EEEEEE"),
        None,
        "an already-light theme default is legible on dark"
    );
}

#[test]
fn fallback_fingerprint_requires_the_parser_name_and_no_id() {
    assert!(is_smart_art_fallback_shape(None, Some("SmartArt")));
    assert!(!is_smart_art_fallback_shape(Some("4"), Some("SmartArt")));
    assert!(!is_smart_art_fallback_shape(None, Some("Rectangle 5")));
    assert!(!is_smart_art_fallback_shape(None, None));
}
