//! Text-box parity (`text-box-b`): WordArt "Follow Path" semantics for
//! single-edge warps (ECMA-376 §20.1.9.19, issue #846).
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/wordart-follow-path.test.ts`.
//!
//! The reference renders a 6.2 in × 1.5 in WordArt box through a mock context
//! that tracks the CTM, and measures the device-space span / centre of the
//! warped glyph origins. The same geometry is available from the two kernels the
//! renderer uses — `follow_path_u_scale` (the natural-width clamp) and
//! `warp_glyph_transform` (the glyph baseline in the envelope) — so the probes
//! below evaluate the envelope at the reference's `u` range directly. The
//! multi-line cases depend on the reference's per-line envelope expansion, which
//! this port does not implement.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_text_box_b_warp_tests.rs.txt`; the
//! ledger keeps their rows.

use kitty_pptx_core::compiler::text_warp::{
    build_warp_envelope, follow_path_u_scale, warp_glyph_transform,
};
use kitty_pptx_core::contracts::WarpEnvelope;

/// The sample-16 WordArt box: 6.2 in × 1.5 in (`:112`).
const BOX_W: f32 = 620.0;
const BOX_H: f32 = 150.0;

fn envelope(preset: &str, adj: &[i64]) -> WarpEnvelope {
    build_warp_envelope(preset, adj, BOX_W, BOX_H)
        .unwrap_or_else(|| panic!("{preset} builds an envelope"))
}

fn arc_length(envelope: &WarpEnvelope) -> f32 {
    envelope.top.cumulative.last().copied().unwrap_or(0.0)
}

/// Device-space baseline x of a glyph at horizontal fraction `u`.
fn glyph_x(envelope: &WarpEnvelope, u: f32, ink_h: f32) -> f32 {
    warp_glyph_transform(envelope, u, ink_h, 0.8)
        .unwrap_or_else(|| panic!("u = {u} maps into the envelope"))
        .x
}

fn glyph_y(envelope: &WarpEnvelope, u: f32, ink_h: f32) -> f32 {
    warp_glyph_transform(envelope, u, ink_h, 0.8)
        .unwrap_or_else(|| panic!("u = {u} maps into the envelope"))
        .y
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected}, got {actual}"
    );
}

/// `:120` "textArchUp centres a centred word within its natural-width arc
/// segment": the natural ink width of `Arch Up` (7 × 10 px = 70 px) is only a
/// short arc segment, so the span stays far below the box width and the segment
/// is centred on `BOX_W / 2`.
#[test]
fn arch_up_centres_the_word_within_its_natural_width_segment() {
    let env = envelope("textArchUp", &[]);
    let follow = follow_path_u_scale(&env, 70.0);
    assert!(
        (follow - 70.0 / arc_length(&env)).abs() < 1e-6,
        "follow = natural / arc, got {follow}"
    );
    // A centred paragraph starts the ink at `0.5 · (1 − follow)`.
    let start = 0.5 * (1.0 - follow);
    let (left, right) = (
        glyph_x(&env, start, 9.0),
        glyph_x(&env, start + follow, 9.0),
    );
    let span = right - left;
    assert!(span < BOX_W * 0.5, "compact arc segment, span {span}");
    let centre = (left + right) / 2.0;
    assert!(
        (centre - BOX_W / 2.0).abs() <= 5.0,
        "centred on the box: {centre}"
    );
}

/// `:135` "places left- and right-aligned text at the corresponding path ends":
/// left-aligned ink ends before the middle, right-aligned ink starts after it.
#[test]
fn alignment_moves_the_segment_to_the_path_ends() {
    let env = envelope("textArchUp", &[]);
    let follow = follow_path_u_scale(&env, 70.0);
    let left_ink = glyph_x(&env, 0.0, 9.0).max(glyph_x(&env, follow, 9.0));
    let right_start = glyph_x(&env, 1.0 - follow, 9.0);
    assert!(left_ink < BOX_W / 2.0, "left: {left_ink}");
    assert!(right_start > BOX_W / 2.0, "right: {right_start}");
}

/// `:145` "offsets an arch baseline by the glyph ink box, not the shape height":
/// `warpGlyphTransform(env, 0.5, inkHeight = 7 + 2, 0.8)` — doubling the ink
/// height to the shape height moves the baseline by
/// `(0.8 − 1) · (inkH − BOX_H) · tx`.
#[test]
fn arch_baseline_uses_the_glyph_ink_box() {
    let env = envelope("textArchUp", &[]);
    let ink = 7.0 + 2.0;
    let apex = warp_glyph_transform(&env, 0.5, ink, 0.8).expect("apex maps");
    // The tangent at the arch apex is horizontal (tx = 1, ty = 0), so the ink
    // offset is perpendicular: y = point + tx · descent, `descent = 0.2 · h`.
    assert!(apex.angle.sin().abs() < 0.01, "flat apex tangent");
    let shift = glyph_y(&env, 0.5, BOX_H) - apex.y;
    let expected = 0.2 * (BOX_H - ink) * apex.angle.cos();
    close(shift, expected, "ink-box baseline offset");
    // The shape-height box would sit 28.2 px lower than the 9 px ink box.
    close(shift, 28.2, "ink vs shape height");
}

/// `:258` "textCircle keeps the word compact rather than scattering around the
/// ellipse": the 60 px natural width stays a compact arc segment.
#[test]
fn circle_keeps_the_word_compact() {
    let env = envelope("textCircle", &[]);
    let follow = follow_path_u_scale(&env, 60.0);
    assert!(
        (follow - 60.0 / arc_length(&env)).abs() < 1e-6,
        "follow = natural / arc, got {follow}"
    );
    let start = 0.5 * (1.0 - follow);
    let span = (glyph_x(&env, start + follow, 9.0) - glyph_x(&env, start, 9.0)).abs();
    assert!(span < BOX_W * 0.6, "compact segment: {span}");
}

/// `:268` "a word wider than the arch still fills (clamps to) the full path":
/// 400 px natural against a shorter arc fills the path, and anything longer is
/// clamped to exactly `1.0`.
#[test]
fn a_word_wider_than_the_arch_clamps_to_the_full_path() {
    let env = envelope("textArchUp", &[]);
    let arc = arc_length(&env);
    let long = follow_path_u_scale(&env, 400.0);
    assert!(
        (long - (400.0 / arc).min(1.0)).abs() < 1e-6,
        "clamped follow, got {long}"
    );
    assert_eq!(follow_path_u_scale(&env, arc * 10.0), 1.0, "clamped to 1");
    let span = (glyph_x(&env, 1.0, 9.0) - glyph_x(&env, 0.0, 9.0)).abs();
    assert!(span > BOX_W * 0.5, "fills the arch: {span}");
}

use crossbeam_channel::unbounded;
use kitty_pptx_core::compiler::text_warp::warp_envelope_for_line;
use kitty_pptx_core::contracts::AffineTransform;
use kitty_pptx_core::compiler::text::compile_text_body;
use serde_json::json;

const PT: f32 = 12_700.0;

fn text_run(text: &str, size: f64) -> serde_json::Value {
    json!({
        "type": "text", "text": text, "bold": null, "italic": null,
        "underline": false, "strikethrough": false, "strikeDouble": false,
        "hyperlinkUsesTextFill": false, "fieldType": null,
        "fontSize": size, "color": "000000", "fontFamily": "Arial"
    })
}

fn warp_body(
    preset: &str,
    runs: Vec<serde_json::Value>,
    space_line: serde_json::Value,
    space_before: Option<i64>,
    space_after: Option<i64>,
) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "ctr",
        "paragraphs": [{
            "alignment": "ctr", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": space_before, "spaceAfter": space_after,
            "spaceLine": space_line, "lvl": 0, "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null, "tabStops": [],
            "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
            "runs": runs
        }],
        "defaultFontSize": 40,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 91_440, "rIns": 91_440, "tIns": 45_720, "bIns": 45_720,
        "wrap": "square", "vert": "horz", "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
        "textWarp": { "preset": preset, "adj": [] }
    });
    serde_json::from_value(value).expect("warp body")
}

fn warp_runs(body: &pptx_model::TextBody) -> Vec<kitty_pptx_core::contracts::PositionedGlyphRun> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        body,
        (620.0 * PT) as i64,
        (150.0 * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    )
    .into_iter()
    .filter_map(|command| match command {
        kitty_pptx_core::contracts::VectorCommand::DrawText { runs, .. } => Some(runs),
        _ => None,
    })
    .flatten()
    .collect()
}

#[test]
fn warp_honours_fixed_line_spacing() {
    let body = warp_body(
        "textArchUp",
        vec![
            text_run("A", 40.0),
            json!({"type": "break"}),
            text_run("A", 40.0),
        ],
        json!({"type": "pts", "val": 18.0}),
        None,
        None,
    );
    let runs = warp_runs(&body);
    let lines: Vec<f32> = runs
        .iter()
        .filter_map(|run| run.glyphs.first().map(|glyph| glyph.y))
        .collect();
    assert_eq!(lines.len(), 2, "two runs");
    assert!((lines[1] - lines[0] - 18.0 * PT).abs() < 0.5);
}

#[test]
fn warp_uses_authored_percentage_line_spacing() {
    for (percent, expected) in [(50, 24.0), (100, 48.0), (150, 72.0), (200, 96.0)] {
        let body = warp_body(
            "textArchUp",
            vec![
                text_run("A", 40.0),
                json!({"type": "break"}),
                text_run("A", 40.0),
            ],
            json!({"type": "pct", "val": percent * 1000}),
            None,
            None,
        );
        let runs = warp_runs(&body);
        let lines: Vec<f32> = runs
            .iter()
            .filter_map(|run| run.glyphs.first().map(|glyph| glyph.y))
            .collect();
        assert!(
            (lines[1] - lines[0] - expected * PT).abs() < 0.5,
            "{percent}%: expected {expected}, got {}",
            (lines[1] - lines[0]) / PT
        );
    }
}

#[test]
fn warp_expands_preceding_lines_outside_the_authored_arch() {
    let envelope = envelope("textArchUp", &[]);
    let expanded = warp_envelope_for_line(&envelope, 48.0);
    assert!(
        expanded.top.points.iter().any(|(_, y)| *y < envelope.top.points.iter().map(|p| p.1).fold(f32::MAX, f32::min)),
        "outer line moves away from the curve"
    );
    assert_eq!(warp_envelope_for_line(&envelope, 0.0), envelope);
}

#[test]
fn warp_multi_line_arch_anchors_the_last_line() {
    let body = warp_body(
        "textArchUp",
        vec![
            text_run("Top", 40.0),
            json!({"type": "break"}),
            text_run("Bottom", 40.0),
        ],
        json!(null),
        None,
        None,
    );
    let runs = warp_runs(&body);
    let warped: Vec<f32> = runs
        .iter()
        .filter_map(|run| {
            let glyph = run.glyphs.first()?;
            let warp = run.warp.as_ref()?;
            let transform = warp_glyph_transform(
                &warp.envelope,
                glyph.warp_u?,
                warp.warp_box_h,
                warp.band_frac,
            )?;
            Some(transform.y)
        })
        .collect();
    assert_eq!(warped.len(), 2);
    assert!(warped[0] < warped[1], "first line expands upward");
    assert!(warped[0] / PT < 0.0, "expanded above the box, got {}", warped[0] / PT);
    assert!(warped[1] / PT < 10.0, "last line stays on the arch, got {}", warped[1] / PT);
}

#[test]
fn warp_keeps_paragraph_spacing_between_paragraphs() {
    let mut body = warp_body(
        "textArchUp",
        vec![text_run("A", 40.0)],
        json!(null),
        Some(1200),
        Some(600),
    );
    let second = body.paragraphs[0].clone();
    body.paragraphs.push(second);
    let runs = warp_runs(&body);
    let lines: Vec<f32> = runs
        .iter()
        .filter_map(|run| run.glyphs.first().map(|glyph| glyph.y))
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(
        ((lines[1] - lines[0]) / PT - 66.0).abs() < 0.5,
        "spaceBefore 12 + spaceAfter 6 + 48: got {}",
        (lines[1] - lines[0]) / PT
    );
}

#[test]
fn circle_anchors_the_final_line_and_expands_radially() {
    let body = warp_body(
        "textCircle",
        vec![
            text_run("A", 40.0),
            json!({"type": "break"}),
            text_run("A", 40.0),
        ],
        json!(null),
        None,
        None,
    );
    let runs = warp_runs(&body);
    let warped: Vec<(f32, f32)> = runs
        .iter()
        .filter_map(|run| {
            let glyph = run.glyphs.first()?;
            let warp = run.warp.as_ref()?;
            let transform = warp_glyph_transform(
                &warp.envelope,
                glyph.warp_u?,
                warp.warp_box_h,
                warp.band_frac,
            )?;
            Some((transform.x, transform.y))
        })
        .collect();
    assert_eq!(warped.len(), 2);
    // The final line stays on the authored circle; the preceding line expands
    // radially outward.
    assert!((warped[0].0 - warped[1].0).abs() / PT > 10.0);
    assert!(warped[0].0 / PT > 0.0 && warped[1].0 / PT > 0.0);
}

#[test]
fn warp_follows_the_path_winding_at_135_and_225_degrees() {
    for angle in [135_i64, 225] {
        let value = json!({
            "verticalAnchor": "ctr",
            "paragraphs": [{
                "alignment": "ctr", "marL": 0, "marR": 0, "indent": 0,
                "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
                "lvl": 0, "bullet": { "type": "none" },
                "defFontSize": null, "defColor": null, "defBold": null,
                "defItalic": null, "defFontFamily": null, "tabStops": [],
                "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
                "runs": [
                    text_run("A", 40.0), json!({"type": "break"}), text_run("A", 40.0)
                ]
            }],
            "defaultFontSize": 40,
            "defaultBold": null, "defaultItalic": null,
            "lIns": 91_440, "rIns": 91_440, "tIns": 45_720, "bIns": 45_720,
            "wrap": "square", "vert": "horz", "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "textWarp": { "preset": "textArchDown", "adj": [angle * 60_000] }
        });
        let body: pptx_model::TextBody = serde_json::from_value(value).expect("warp body");
        let runs = warp_runs(&body);
        let warped: Vec<f32> = runs
            .iter()
            .filter_map(|run| {
                let glyph = run.glyphs.first()?;
                let warp = run.warp.as_ref()?;
                let transform = warp_glyph_transform(
                    &warp.envelope,
                    glyph.warp_u?,
                    warp.warp_box_h,
                    warp.band_frac,
                )?;
                Some(transform.y)
            })
            .collect();
        assert_eq!(warped.len(), 2);
        assert!(
            warped[1] < warped[0] - 40.0 * PT,
            "{angle}°: second line above first, got {:?}",
            warped.iter().map(|y| y / PT).collect::<Vec<_>>()
        );
    }
}

#[test]
fn counterclockwise_lines_expand_outside_the_lower_arch() {
    let body = warp_body(
        "textArchDown",
        vec![
            text_run("A", 40.0),
            json!({"type": "break"}),
            text_run("A", 40.0),
        ],
        json!(null),
        None,
        None,
    );
    let runs = warp_runs(&body);
    let warped: Vec<f32> = runs
        .iter()
        .filter_map(|run| {
            let glyph = run.glyphs.first()?;
            let warp = run.warp.as_ref()?;
            let transform = warp_glyph_transform(
                &warp.envelope,
                glyph.warp_u?,
                warp.warp_box_h,
                warp.band_frac,
            )?;
            Some(transform.y)
        })
        .collect();
    assert_eq!(warped.len(), 2);
    assert!((warped[1] - warped[0]) / PT > 20.0, "later line outside the lower arch");
    assert!(warped[0] / PT >= 0.0);
}

#[test]
fn warp_accumulates_mixed_size_line_boxes() {
    let paragraphs: Vec<serde_json::Value> = [16.0, 32.0, 48.0].iter().map(|size| json!({
            "alignment": "ctr", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0, "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null, "tabStops": [],
            "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
            "runs": [text_run("A", *size)]
        })).collect();
    let value = json!({
        "verticalAnchor": "ctr",
        "paragraphs": paragraphs,
        "defaultFontSize": 40,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 91_440, "rIns": 91_440, "tIns": 45_720, "bIns": 45_720,
        "wrap": "square", "vert": "horz", "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
        "textWarp": { "preset": "textArchUp", "adj": [] }
    });
    let body: pptx_model::TextBody = serde_json::from_value(value).expect("warp body");
    let runs = warp_runs(&body);
    let lines: Vec<f32> = runs
        .iter()
        .filter_map(|run| run.glyphs.first().map(|glyph| glyph.y))
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(
        ((lines[1] - lines[0]) / PT - 34.56).abs() < 0.5,
        "first mixed step: {}",
        (lines[1] - lines[0]) / PT
    );
    assert!(
        ((lines[2] - lines[1]) / PT - 53.76).abs() < 0.5,
        "second mixed step: {}",
        (lines[2] - lines[1]) / PT
    );
}

struct NoAssets;

impl kitty_pptx_core::AssetStore for NoAssets {
    fn get(
        &self,
        rel_id: &str,
        _hint: kitty_pptx_core::SizeHint,
    ) -> Result<kitty_pptx_core::DecodedImage, kitty_pptx_core::AssetError> {
        Err(kitty_pptx_core::AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> kitty_pptx_core::AssetStoreStats {
        kitty_pptx_core::AssetStoreStats::default()
    }
}

#[test]
fn warped_svg_uses_per_glyph_outlines() {
    use kitty_pptx_core::contracts::{SvgTextMode, VectorSlide};
    use kitty_pptx_core::svg::SlideSvgSerializer;

    let body = warp_body(
        "textArchUp",
        vec![text_run("Arch Up", 40.0)],
        json!(null),
        None,
        None,
    );
    let (tx, _rx) = unbounded();
    let commands = compile_text_body(
        &body,
        (620.0 * PT) as i64,
        (150.0 * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    let mut slide = VectorSlide::new((620.0 * PT) as i64, (150.0 * PT) as i64);
    slide.commands = commands;
    let serializer = kitty_pptx_core::DefaultSlideSvgSerializer::new();

    for mode in [SvgTextMode::VectorGlyphs, SvgTextMode::SelectableText] {
        let svg = serializer
            .serialize_svg(&slide, &NoAssets, mode)
            .expect("serialize warped svg");
        assert!(svg.contains("<path"), "warped glyphs are outlined");
        assert!(
            !svg.contains("<text"),
            "warped runs must not fall back to a flat text frame"
        );
    }
}
