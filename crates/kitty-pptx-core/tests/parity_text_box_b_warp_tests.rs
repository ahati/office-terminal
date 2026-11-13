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

/// `:159` "keeps the last clockwise line on the authored arch and expands
/// preceding lines": `bottomY − topY > 20`, `topY < 0`, `bottomY < 10`.
#[test]
#[ignore = "blocked: no per-line envelope expansion / line-anchored arch placement in the warp path"]
fn multi_line_arch_anchors_the_last_line() {
    // Reference: bottomY - topY > 20, topY < 0, bottomY < 10 for a two-line
    // arch whose LAST line stays on the authored arch.
    let env = envelope("textArchUp", &[]);
    assert!(arc_length(&env) > BOX_W, "the arch is wider than the box");
}

/// `:169` "expands later counterclockwise lines outside the lower arch":
/// `glyphs[1].y − glyphs[0].y ≈ 48` and `glyphs[0].y > BOX_H`.
#[test]
#[ignore = "blocked: no per-line envelope expansion (textArchDown line stacking)"]
fn counterclockwise_lines_expand_outside_the_lower_arch() {
    // Reference: glyphs[1].y - glyphs[0].y == 48 and glyphs[0].y > BOX_H.
    let env = envelope("textArchDown", &[]);
    assert!(arc_length(&env) > 0.0);
}

/// `:178` "honours fixed line spacing independently of the font size":
/// `spaceLine = { type: 'pts', val: 18 }` → `glyphs[1].y − glyphs[0].y ≈ 18`.
#[test]
#[ignore = "blocked: the warp path does not apply a:lnSpc between its lines"]
fn warp_honours_fixed_line_spacing() {
    // Reference: glyphs[1].y - glyphs[0].y == 18 for spaceLine { pts, 18 }.
    let env = envelope("textArchUp", &[]);
    assert!(glyph_y(&env, 0.5, 9.0).is_finite());
}

/// `:189` "retains paragraph spacing only between paragraphs":
/// `spaceBefore 1200` + `spaceAfter 600` → `glyphs[1].y − glyphs[0].y ≈ 66`.
#[test]
#[ignore = "blocked: the warp path does not apply spcBef/spcAft between its paragraphs"]
fn warp_keeps_paragraph_spacing_between_paragraphs() {
    // Reference: glyphs[1].y - glyphs[0].y == 66 (spaceBefore 1200 + 600), and
    // the second baseline stays above y = 10.
    let env = envelope("textArchUp", &[]);
    assert!(glyph_y(&env, 0.5, 9.0).is_finite());
}

/// `:200` "uses 50 % authored line spacing": `48 × 50 / 100`.
#[test]
#[ignore = "blocked: the warp path does not apply a:lnSpc between its lines"]
fn warp_uses_50_percent_line_spacing() {
    // Reference: glyphs[1].y - glyphs[0].y == 48 * 50 / 100 == 24.
    close(48.0 * 50.0 / 100.0, 24.0, "half line");
}

/// `:200` "uses 100 % authored line spacing".
#[test]
#[ignore = "blocked: the warp path does not apply a:lnSpc between its lines"]
fn warp_uses_100_percent_line_spacing() {
    // Reference: glyphs[1].y - glyphs[0].y == 48 * 100 / 100 == 48.
    close(48.0 * 100.0 / 100.0, 48.0, "one line");
}

/// `:200` "uses 150 % authored line spacing".
#[test]
#[ignore = "blocked: the warp path does not apply a:lnSpc between its lines"]
fn warp_uses_150_percent_line_spacing() {
    // Reference: glyphs[1].y - glyphs[0].y == 48 * 150 / 100 == 72.
    close(48.0 * 150.0 / 100.0, 72.0, "1.5 lines");
}

/// `:200` "uses 200 % authored line spacing".
#[test]
#[ignore = "blocked: the warp path does not apply a:lnSpc between its lines"]
fn warp_uses_200_percent_line_spacing() {
    // Reference: glyphs[1].y - glyphs[0].y == 48 * 200 / 100 == 96.
    close(48.0 * 200.0 / 100.0, 96.0, "double line");
}

/// `:211` "accumulates mixed-size line boxes before anchoring the last
/// baseline": 16/32/48 pt lines step by `34.56` then `53.76`.
#[test]
#[ignore = "blocked: the warp path does not accumulate mixed-size line boxes"]
fn warp_accumulates_mixed_size_line_boxes() {
    close(1.2 * 16.0 * 1.8, 34.56, "first step");
    close(1.2 * 48.0 * 0.933, 53.76, "second step");
}

/// `:221` "uses path winding for a 135 degree downward arch": the second line
/// sits more than 40 px ABOVE the first.
#[test]
#[ignore = "blocked: no per-line envelope expansion along the path winding (textArchDown)"]
fn warp_follows_the_path_winding_at_135_degrees() {
    let env = envelope("textArchDown", &[135 * 60_000]);
    assert!(arc_length(&env) > 0.0);
}

/// `:221` "uses path winding for a 225 degree downward arch".
#[test]
#[ignore = "blocked: no per-line envelope expansion along the path winding (textArchDown)"]
fn warp_follows_the_path_winding_at_225_degrees() {
    let env = envelope("textArchDown", &[225 * 60_000]);
    assert!(arc_length(&env) > 0.0);
}

/// `:232` "anchors the final circle line and expands preceding lines radially":
/// the two lines differ by `48` in x and the second sits past `BOX_W − 10`.
#[test]
#[ignore = "blocked: no per-line radial expansion in the textCircle path"]
fn circle_anchors_the_final_line_and_expands_radially() {
    // Reference: glyphs[0].x - glyphs[1].x == 48 and glyphs[1].x > BOX_W - 10.
    let env = envelope("textCircle", &[]);
    assert!(glyph_x(&env, 0.0, 9.0).is_finite());
}

/// `:242` "recomputes each line on its expanded ellipse, including arc-length
/// alignment": the two lines of a 200 × 180 box match a single line laid out on
/// a 296 × 276 box at `(−48, −48)`.
#[test]
#[ignore = "blocked: no per-line expanded ellipse (the warp path maps every line through one envelope)"]
fn expanded_ellipse_recomputes_each_line() {}

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

/// `:279` "does not create wrap-only fragments for infinite-width WordArt
/// layout": the flat no-wrap pass measures the full segment a small fixed number
/// of times, never every growing prefix.
#[test]
#[ignore = "blocked: no measurer call recording on the warp compile path"]
fn warp_layout_measures_the_full_segment_once() {}
