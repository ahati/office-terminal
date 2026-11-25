//! `powerpoint-line-metrics.test.ts` — the pinned face catalogue (#1610).
//!
//! The reference resolves every line box from a **static** table of the faces
//! PowerPoint ships (`packages/core/src/fonts/reference-font-metrics-data.json`,
//! copied verbatim to `assets/fonts/office-face-metrics.json`). The numbers
//! below are that table's, so they pass regardless of which fonts the host
//! installs — which is the point: Arial is 1854 / 2288, not the host `hhea`.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts:39-141`
//!   (`OBSERVED_SUBSTITUTES`, `chosenProfiles`, `resolveShare`)
//! - `:143-221` the glyph / Excel / decoration boxes
//! - `:223-279` the interned face metrics and the bounded LRU
//! - `packages/pptx/src/powerpoint-line-metrics.test.ts:83-102`, `:195-211`
//!
//! Cases needing the emitter's model dispatch (a body with **any** unresolved
//! face keeps the ordinary line model; the painting-face selection for absent
//! authored faces) stay BLOCKED in `tests/parity/coverage/text-box-a.md`: the
//! catalogue lookup and the production ascent-share wiring are what this file
//! pins.

#[path = "text_box_a_support/mod.rs"]
mod support;

use office_terminal_core::text::faces::{
    face_cache_len, power_point_ascent_share, power_point_face_metrics, share_cache_len, FaceMetricsFull,
    SHARE_CACHE_LIMIT,
};
use support::{body, close_tol, compile, line_pts, para, text_face, units, Para, Stack};

/// Every face of the #1610 controls, its catalogued share and the exported
/// baselines. The shares are `usWinAscent / (usWinAscent + usWinDescent)`, or
/// the typo triplet for a `USE_TYPO_METRICS` face (Gabriola).
const CONTROLS: [(&str, f32, f32, [i32; 2]); 9] = [
    ("Arial", 1854.0 / 2288.0, 72.0, [97, 217]),
    ("Calibri", 1950.0 / 2500.0, 72.0, [94, 214]),
    ("Yu Gothic", 2017.0 / 2636.0, 72.0, [92, 212]),
    ("Gabriola", 0.813_873_33, 72.0, [98, 218]),
    ("Baskerville Old Face", 1805.0 / 2336.0, 200.0, [258, 591]),
    ("Stencil", 1860.0 / 2427.0, 200.0, [255, 589]),
    ("Times New Roman", 0.804_673_7, 200.0, [268, 602]),
    ("Georgia", 0.807_047_7, 200.0, [269, 602]),
    ("Courier New", 1705.0 / 2320.0, 200.0, [245, 578]),
];

/// `it('splits each face by its usWin (or USE_TYPO_METRICS typo) ascent
/// share')` — a second assertion pass over the first port: the share the
/// production layout uses for each control face is the catalogue's, and the
/// compiled baselines are the exported ones.
#[test]
fn catalogue_shares_drive_the_compiled_baselines() {
    for (name, share, size, expected) in CONTROLS {
        let pinned =
            power_point_ascent_share(name, false, false).unwrap_or_else(|| panic!("{name} must be catalogued"));
        close_tol(pinned, share, 1e-6, name);
        // End to end: `compile_text_body` reads the catalogue, not the host.
        let paragraphs: Vec<support::ParaSpec> = (0..2)
            .map(|_| para(vec![text_face("Hg", f64::from(size), name)]))
            .collect();
        let y = line_pts(&compile(&body(&paragraphs), 2000.0, 800.0));
        assert_eq!(units(&y), expected.to_vec(), "{name} {size} pt");
        // The first baseline is the catalogued split of the 1.2 × size box.
        close_tol(y[0], 1.2 * size * share, 1e-2, name);
    }
}

/// `it('resolves a face name that is one cut of its family to that cut
/// (#1630)')`: "Calibri Light" names the weight-300 Calibri cut, so every
/// requested weight resolves to that cut's 1950 / 2500 usWin pair.
#[test]
fn calibri_light_resolves_to_its_own_cut() {
    let expected = 1950.0 / 2500.0;
    for (bold, italic) in [(false, false), (false, true), (true, false)] {
        let share = power_point_ascent_share("Calibri Light", bold, italic)
            .unwrap_or_else(|| panic!("Calibri Light must be catalogued"));
        close_tol(share, expected, 1e-6, "Calibri Light cut");
    }
    // A name shared by several cuts still needs the requested weight.
    close_tol(
        power_point_ascent_share("Calibri", false, false).expect("Calibri"),
        expected,
        1e-6,
        "Calibri regular",
    );
    // #1630 control: a 44 pt Calibri Light title's first baseline sat 41.16 pt
    // below the text top, `0.7795` of the 52.8 pt line box.
    let y = line_pts(&compile(
        &body(&[para(vec![text_face("Hg", 44.0, "Calibri Light")])]),
        2000.0,
        800.0,
    ));
    close_tol(y[0], 41.16, 0.05, "Calibri Light 44 pt baseline");
}

/// `it('follows PowerPoint substitutions for faces only macOS itself
/// provides')`: Palatino → Palatino Linotype, Helvetica / Helvetica Neue →
/// Arial, everything else system-only stays unresolved.
#[test]
fn substitutions_follow_power_point_for_system_only_faces() {
    let arial = power_point_ascent_share("Arial", false, false).expect("Arial");
    for face in ["Palatino", "Helvetica", "Helvetica Neue"] {
        assert!(
            power_point_ascent_share(face, false, false).is_some(),
            "{face} must resolve through OBSERVED_SUBSTITUTES"
        );
    }
    close_tol(
        power_point_ascent_share("Helvetica", false, false).expect("Helvetica"),
        arial,
        1e-6,
        "Helvetica → Arial",
    );
    close_tol(
        power_point_ascent_share("Helvetica Neue", false, false).expect("Helvetica Neue"),
        arial,
        1e-6,
        "Helvetica Neue → Arial",
    );
    // Avenir, Menlo and Hiragino Sans fall back to deck-dependent faces.
    for face in ["Avenir", "Menlo", "Hiragino Sans", "No Such Face"] {
        assert_eq!(
            power_point_ascent_share(face, false, false),
            None,
            "{face} is unresolved"
        );
    }
    // Palatino Linotype's own cut (2150 / 2763) and the two exported controls.
    close_tol(
        power_point_ascent_share("Palatino", false, false).expect("Palatino"),
        2150.0 / 2763.0,
        1e-6,
        "Palatino Linotype",
    );
    let palatino = vec![
        para(vec![text_face("Hg0", 200.0, "Palatino")]),
        para(vec![text_face("Hg1", 200.0, "Palatino")]),
    ];
    let ys = line_pts(&compile(&body(&palatino), 2000.0, 800.0));
    assert_eq!(units(&ys), vec![259, 593], "Palatino 200 pt");
    let helvetica = vec![
        para(vec![text_face("Hg0", 200.0, "Helvetica")]),
        para(vec![text_face("Hg1", 200.0, "Helvetica")]),
    ];
    let ys = line_pts(&compile(&body(&helvetica), 2000.0, 800.0));
    assert_eq!(units(&ys), vec![270, 603], "Helvetica 200 pt");
}

/// `it('bounds the share cache while many distinct face names are queried')`:
/// the cache is an LRU of `SHARE_CACHE_LIMIT` entries, and a known face still
/// resolves after its entry was evicted.
#[test]
fn share_cache_is_bounded() {
    for index in 0..SHARE_CACHE_LIMIT * 4 {
        let face = format!("Unresolved Face {index}");
        assert_eq!(power_point_ascent_share(&face, index % 2 == 0, index % 3 == 0), None);
        assert!(
            share_cache_len() <= SHARE_CACHE_LIMIT,
            "share cache grew past its bound"
        );
    }
    assert_eq!(share_cache_len(), SHARE_CACHE_LIMIT);
    // The interned face cache obeys the same bound.
    for index in 0..SHARE_CACHE_LIMIT * 2 {
        assert!(power_point_face_metrics(&format!("Missing Face {index}"), false, false).is_none());
    }
    assert!(face_cache_len() <= SHARE_CACHE_LIMIT);
    close_tol(
        power_point_ascent_share("Arial", false, false).expect("Arial after eviction"),
        1854.0 / 2288.0,
        1e-6,
        "known face after eviction",
    );
}

/// `powerPointFaceMetrics` carries the glyph box (`fontAlgn` t/ctr/b, the
/// highlight band), the #1604 Excel box (`compatLnSpc="0"`) and the face's own
/// decoration geometry.
#[test]
fn face_metrics_carry_the_glyph_excel_and_decoration_boxes() {
    let arial: FaceMetricsFull = *power_point_face_metrics("Arial", false, false).expect("Arial");
    let glyph = arial.glyph.expect("Arial usWin box");
    close_tol(glyph.ascent, 1854.0 / 2048.0, 1e-6, "Arial glyph ascent");
    close_tol(glyph.descent, 434.0 / 2048.0, 1e-6, "Arial glyph descent");
    // Excel adds the Windows external leading max(0, lineGap − (usWin box −
    // hhea box)) = 67 design units above the ascent.
    let excel = arial.excel.expect("Arial Excel box");
    close_tol(excel.ascent, (1854.0 + 67.0) / 2048.0, 1e-6, "Arial Excel ascent");
    close_tol(excel.descent, 434.0 / 2048.0, 1e-6, "Arial Excel descent");
    let decoration = arial.decoration.expect("Arial decoration");
    close_tol(decoration.underline_top_em, 217.0 / 2048.0, 1e-6, "underline top");
    close_tol(
        decoration.underline_thickness_em,
        150.0 / 2048.0,
        1e-6,
        "underline thickness",
    );
    close_tol(decoration.strike_top_em, -530.0 / 2048.0, 1e-6, "strike top");
    close_tol(decoration.strike_thickness_em, 102.0 / 2048.0, 1e-6, "strike size");

    // Far-East faces take the 1.3 × usWin Windows box in the Excel model.
    let meiryo = *power_point_face_metrics("Meiryo", false, false).expect("Meiryo");
    let excel = meiryo.excel.expect("Meiryo Excel box");
    let half = 0.15 * (2171.0 + 901.0);
    close_tol(excel.ascent, (2171.0 + half) / 2048.0, 1e-6, "Meiryo Excel ascent");
    close_tol(excel.descent, (901.0 + half) / 2048.0, 1e-6, "Meiryo Excel descent");
    // A macOS Supplemental copy projects with the system hhea tables: Times
    // New Roman keeps its hhea lineGap 87 (1.150 em).
    let times = *power_point_face_metrics("Times New Roman", false, false).expect("Times");
    let excel = times.excel.expect("Times Excel box");
    close_tol(excel.ascent, (1825.0 + 87.0) / 2048.0, 1e-6, "Times Excel ascent");
    // A USE_TYPO_METRICS face takes the typo triplet in both models.
    let gabriola = *power_point_face_metrics("Gabriola", false, false).expect("Gabriola");
    let glyph = gabriola.glyph.expect("Gabriola typo box");
    close_tol(glyph.ascent, 5667.0 / 4096.0, 1e-6, "Gabriola typo ascent");
    close_tol(glyph.descent, 1296.0 / 4096.0, 1e-6, "Gabriola typo descent");
}

/// The kernel-level `absent authored faces` cases need the emitter's
/// painting-face selection; what the catalogue can answer is that those faces
/// are exactly the ones outside it.
#[test]
fn absent_faces_have_no_catalogued_split() {
    // `keeps the ordinary line model instead of the absent face's catalog
    // ascent share`: the reference has no entry for these names.
    for face in ["Avenir", "Menlo", "Hiragino Sans"] {
        assert_eq!(power_point_face_metrics(face, false, false), None, "{face}");
    }
    // ... and it does have one for the painting substitutes that stand in.
    assert!(power_point_face_metrics("Arial", false, false).is_some());
    assert!(power_point_face_metrics("Palatino Linotype", false, false).is_some());
}

/// The kernel helper the layout uses exposes the same catalogue split.
#[test]
fn layout_face_metrics_matches_the_catalogue() {
    for (name, share, _, _) in CONTROLS {
        let face = office_terminal_core::text::metrics::FaceMetrics::for_family(name, false, false);
        close_tol(face.ascent_share, share, 1e-6, name);
    }
    // The stack probe agrees with the reference controls.
    let arial = Stack::new(Para::many(2, 72.0, support::ARIAL)).units();
    assert_eq!(arial, vec![97, 217]);
    let times = Stack::new(Para::many(2, 200.0, support::TIMES));
    assert_eq!(times.units(), vec![268, 602]);
}

/// An authored face whose catalogue entry is bold-only still resolves through
/// its single cut (the `chosenProfiles` fallback).
#[test]
fn single_cut_families_resolve_through_that_cut() {
    for face in ["Yu Gothic Light", "Calibri Light", "Arial Narrow"] {
        let resolved =
            power_point_ascent_share(face, false, false).or_else(|| power_point_ascent_share(face, true, false));
        assert!(resolved.is_some(), "{face} selects a single cut");
    }
    assert_eq!(power_point_ascent_share("   ", false, false), None);
}
