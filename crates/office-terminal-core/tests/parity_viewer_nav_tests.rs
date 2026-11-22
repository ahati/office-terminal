//! Viewer-navigation parity batch — ports of the reference viewer's slide
//! navigation: the `partName → sldIdLst index` map (IX-nav), relative show-jump
//! verbs, internal hyperlink dispatch, the hidden-slide policy and the
//! zoom/render-width contract.
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/`
//! `slide-nav.test.ts` (10 cases: part index + resolvers),
//! `viewer-internal-nav.test.ts` (5 cases: click dispatch branches),
//! `viewer-hidden.test.ts` (1), `viewer-hidden-skip.test.ts` (5, #2181),
//! `viewer-zoom-contract.test.ts` (13, IX9 zoom contract), plus the shared
//! `slide-nav.ts:92-101` verbs and `packages/core/src/nav/visible-index.ts`.
//!
//! The Rust pipeline splits the reference's single resolver three ways:
//! `resolve_internal_slide` (part target → index), `resolve_hyperlink`
//! (action verb → `SlideAction`) and `PresentationState::*_slide` (the
//! arithmetic + clamping). Cases therefore land in this target (pure
//! resolution) or in `office-terminal-tui/tests/parity_viewer_nav_tests.rs` (state
//! machine); each reference case is counted exactly once.
//!
//! The hidden-slide policy is ported in `office_terminal_core::navigation`
//! (`HiddenSlideMode`, `next_visible`, `resolve_visible`, `count_visible`) and
//! covered here; the viewer zoom contract lives on `PresentationState`.

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{HyperlinkTarget, RenderTargetSpec, SlideAction, WarningEvent};
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::navigation::{count_visible, next_visible, resolve_visible};
use office_terminal_core::{
    extract_deck_hyperlinks, resolve_hyperlink, resolve_internal_slide, AssetError,
    DefaultSlideCompiler, SlideCompiler, SlideRasterizer, TinySkiaRasterizer,
};
use pptx_model::Slide;

const SLIDE_W: i64 = 9_144_000;
const SLIDE_H: i64 = 6_858_000;

// ── fixtures ────────────────────────────────────────────────────────────────

/// One slide whose `partName` is `part` (or unrecorded when `None`), carrying a
/// single rect shape whose hyperlink is `link`/`action` when supplied.
fn slide(index: usize, part: Option<&str>, link: Option<&str>, action: Option<&str>) -> Slide {
    let mut shape = serde_json::json!({
        "type": "shape",
        "x": 0,
        "y": 0,
        "width": 400_000,
        "height": 300_000,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "rect"
    });
    if let Some(link) = link {
        shape["hyperlink"] = serde_json::Value::String(link.to_string());
    }
    if let Some(action) = action {
        shape["hyperlinkAction"] = serde_json::Value::String(action.to_string());
    }
    let mut value = serde_json::json!({
        "index": index,
        "slideNumber": index + 1,
        "elements": [shape],
        "elementSources": [],
        "comments": []
    });
    if let Some(part) = part {
        value["partName"] = serde_json::Value::String(part.to_string());
    }
    serde_json::from_value(value).expect("slide JSON")
}

/// A three-slide deck's part names in `sldIdLst` order, as the parser stamps
/// them. ref slide-nav.test.ts:8
const PART_NAMES: [&str; 3] = [
    "ppt/slides/slide1.xml",
    "ppt/slides/slide2.xml",
    "ppt/slides/slide3.xml",
];

fn three_slide_deck() -> Vec<Slide> {
    PART_NAMES
        .iter()
        .enumerate()
        .map(|(index, part)| slide(index, Some(part), None, None))
        .collect()
}

// ── buildSlidePartIndex (ref slide-nav.test.ts:9-26) ────────────────────────

/// "maps each part name to its sldIdLst-order index". The Rust resolver keys on
/// the deck slice itself (`Slide::part_name` in order), so the map is asserted
/// through the resolution it exists for. ref slide-nav.ts:24-34
#[test]
fn part_index_maps_each_part_name_to_its_sld_id_lst_index() {
    let deck = three_slide_deck();
    for (expected, part) in PART_NAMES.iter().enumerate() {
        // Slide-rel target, authored relative to `ppt/slides`.
        let name = part.rsplit('/').next().unwrap_or(part);
        let target = format!("../slides/{name}");
        assert_eq!(
            resolve_internal_slide(&target, PART_NAMES[0], &deck),
            Some(expected),
            "{target} resolves to its sldIdLst index"
        );
        // ... and the canonical (root-relative) form the same part name takes.
        assert_eq!(
            resolve_internal_slide(part, PART_NAMES[0], &deck),
            Some(expected),
            "{part} resolves to its sldIdLst index"
        );
    }
}

/// "skips undefined / empty entries and keeps the first on a duplicate".
/// ref slide-nav.test.ts:16-26, ref slide-nav.ts:36-40
#[test]
fn part_index_skips_unrecorded_parts_and_keeps_the_first_on_a_duplicate() {
    let deck = vec![
        slide(0, Some("ppt/slides/slide1.xml"), None, None),
        slide(1, None, None, None),
        slide(2, Some(""), None, None),
        // Duplicate — first (index 0) wins.
        slide(3, Some("ppt/slides/slide1.xml"), None, None),
    ];

    assert_eq!(
        resolve_internal_slide("../slides/slide1.xml", "ppt/slides/slide1.xml", &deck),
        Some(0),
        "a duplicated part name resolves to the earliest slide"
    );
    assert_eq!(
        resolve_internal_slide("../slides/slide2.xml", "ppt/slides/slide1.xml", &deck),
        None,
        "an unrecorded part name resolves to nothing"
    );
    assert_eq!(
        resolve_internal_slide("ppt/slides/slide1.xml", "ppt/slides/slide1.xml", &deck),
        Some(0),
        "the canonical duplicate also lands on the first slide"
    );
}

// ── resolveSlidePartTarget (ref slide-nav.test.ts:30-53) ────────────────────

/// "resolves a slide-rel target (../slides/slideN.xml) to its index" — the exact
/// `hlinksldjump` rel Target. ref slide-nav.ts:56-70
#[test]
fn slide_rel_target_resolves_to_its_index() {
    let deck = three_slide_deck();
    assert_eq!(
        resolve_internal_slide("../slides/slide3.xml", "ppt/slides/slide1.xml", &deck),
        Some(2)
    );
    assert_eq!(
        resolve_internal_slide("../slides/slide1.xml", "ppt/slides/slide1.xml", &deck),
        Some(0)
    );
}

/// "resolves a root-absolute slide target". ref slide-nav.ts:63-66
#[test]
fn root_absolute_target_resolves_from_the_package_root() {
    let deck = three_slide_deck();
    assert_eq!(
        resolve_internal_slide("/ppt/slides/slide2.xml", "ppt/slides/slide1.xml", &deck),
        Some(1)
    );
}

/// "resolves a canonical target inherited from a master or layout part".
/// ref slide-nav.ts:67-68 (`target.startsWith('ppt/')` bypasses the base dir).
#[test]
fn canonical_master_inherited_target_resolves() {
    let deck = three_slide_deck();
    assert_eq!(
        resolve_internal_slide("ppt/slides/slide2.xml", "ppt/slides/slide1.xml", &deck),
        Some(1)
    );
}

/// "returns undefined for a target that names no known slide".
/// ref slide-nav.test.ts:48-52
#[test]
fn target_naming_no_known_slide_resolves_to_none() {
    let deck = three_slide_deck();
    let source = "ppt/slides/slide1.xml";
    assert_eq!(
        resolve_internal_slide("../slides/slide9.xml", source, &deck),
        None
    );
    assert_eq!(
        resolve_internal_slide("https://example.com/", source, &deck),
        None
    );
    assert_eq!(resolve_internal_slide("", source, &deck), None);
}

/// A deck with no slides can resolve nothing — the reference's
/// `partIndex.size === 0` shows up as an empty `deck` here. ref slide-nav.ts:96-97
#[test]
fn an_empty_deck_resolves_no_internal_target() {
    assert_eq!(
        resolve_internal_slide("../slides/slide1.xml", "ppt/slides/slide1.xml", &[]),
        None
    );
}

/// The part map keys the deck slice in `sldIdLst` order, never the slide's own
/// (possibly stale) `index` field. ref slide-nav.ts:24-34
#[test]
fn part_index_uses_sld_id_lst_order_not_the_slide_index_field() {
    let mut deck = three_slide_deck();
    deck[0].index = 7;
    deck[1].index = 3;
    deck[2].index = 5;
    assert_eq!(
        resolve_internal_slide("../slides/slide2.xml", PART_NAMES[0], &deck),
        Some(1),
        "slide2.xml is the second entry of sldIdLst"
    );
    assert_eq!(
        resolve_internal_slide("ppt/slides/slide3.xml", PART_NAMES[0], &deck),
        Some(2)
    );
}

// ── hidden-slide helpers (ref viewer-hidden.test.ts:27-32) ────────────────

/// "skip navigation jumps over hidden slides via the tested helpers":
/// `nextVisibleIndex(0, 1, i === 1, 3) == 2` and
/// `resolveVisibleIndex(1, i === 1, 3) == 2`; `countVisible` counts shown only.
/// ref core/src/nav/visible-index.ts:8-40
#[test]
fn skip_navigation_jumps_over_hidden_slides() {
    let hidden_one = |index: usize| index == 1;
    assert_eq!(next_visible(0, 1, hidden_one, 3), 2);
    assert_eq!(resolve_visible(1, hidden_one, 3), 2);
    assert_eq!(count_visible(hidden_one, 3), 2);

    // No shown slide after it ⇒ the nearest shown slide before it.
    assert_eq!(resolve_visible(2, |index| index == 2, 3), 1);
    // Every slide hidden ⇒ skip degenerates to show and keeps the index.
    assert_eq!(resolve_visible(1, |_| true, 3), 1);
    assert_eq!(next_visible(1, 1, |_| true, 3), 1);
    // Empty decks resolve nothing.
    assert_eq!(count_visible(|_| false, 0), 0);
    assert_eq!(resolve_visible(0, |_| false, 0), 0);
}

// ── resolveInternalSlideTarget (ref slide-nav.test.ts:57-89) ────────────────

/// "resolves a specific slide-part jump" through the `hlinksldjump` action, the
/// branch `viewer-internal-nav.test.ts:41-49` exercises. ref slide-nav.ts:92-101
#[test]
fn hlinksldjump_action_resolves_the_part_target_to_an_index() {
    let deck = three_slide_deck();
    let (tx, _rx) = unbounded();
    assert_eq!(
        resolve_hyperlink(
            Some("../slides/slide3.xml"),
            Some("ppaction://hlinksldjump"),
            "ppt/slides/slide1.xml",
            &deck,
            "shape-1",
            &tx,
        ),
        Some(HyperlinkTarget::SlideIndex(2))
    );
}

/// The four relative show-jump verbs each name a slide *relative* to the
/// current one; the reference resolves them arithmetically, the Rust pipeline
/// carries them as `SlideAction` for the presenter state machine to apply.
/// ref core/src/nav/internal-target.ts:81-96, slide-nav.test.ts:62-71
#[test]
fn show_jump_verbs_map_to_the_four_presentation_actions() {
    let deck = three_slide_deck();
    let (tx, _rx) = unbounded();
    for (verb, action) in [
        ("firstslide", SlideAction::FirstSlide),
        ("lastslide", SlideAction::LastSlide),
        ("nextslide", SlideAction::NextSlide),
        ("previousslide", SlideAction::PreviousSlide),
    ] {
        let reference = format!("ppaction://hlinkshowjump?jump={verb}");
        assert_eq!(
            resolve_hyperlink(
                None,
                Some(&reference),
                "ppt/slides/slide1.xml",
                &deck,
                "s",
                &tx
            ),
            Some(HyperlinkTarget::Action(action)),
            "{verb} names the matching action"
        );
    }
    // `endshow` has no positional slide, so it maps to a control action.
    assert_eq!(
        resolve_hyperlink(
            None,
            Some("ppaction://hlinkshowjump?jump=endshow"),
            "ppt/slides/slide1.xml",
            &deck,
            "s",
            &tx,
        ),
        Some(HyperlinkTarget::Action(SlideAction::EndShow))
    );
}

/// "returns undefined for an unresolvable internal ref" — a verb outside the
/// navigation list is a no-op that is reported. ref slide-nav.test.ts:83-88
#[test]
fn unresolvable_show_action_yields_no_target_and_degrades() {
    let deck = three_slide_deck();
    let (tx, rx) = unbounded();
    assert_eq!(
        resolve_hyperlink(
            None,
            Some("ppaction://hlinkshowjump?jump=bogus"),
            "ppt/slides/slide1.xml",
            &deck,
            "shape-7",
            &tx,
        ),
        None,
        "an unknown jump verb does not navigate"
    );
    let warnings: Vec<WarningEvent> = rx.try_iter().collect();
    assert_eq!(warnings.len(), 1, "the rejected verb is reported once");
    match &warnings[0] {
        WarningEvent::FidelityDegraded {
            element_id,
            feature,
            ..
        } => {
            assert_eq!(element_id, "shape-7");
            assert_eq!(*feature, "hyperlink_action_unsupported");
        }
        other => panic!("expected a degraded hyperlink warning, got {other:?}"),
    }
}

/// A `hlinksldjump` whose rel Target names no slide is dropped, not treated as a
/// URL. ref slide-nav.test.ts:83-88 + compiler/hyperlinks.rs:265-272
#[test]
fn unresolvable_slide_jump_is_dropped() {
    let deck = three_slide_deck();
    let (tx, _rx) = unbounded();
    assert_eq!(
        resolve_hyperlink(
            Some("../slides/nope.xml"),
            Some("ppaction://hlinksldjump"),
            "ppt/slides/slide1.xml",
            &deck,
            "shape-8",
            &tx,
        ),
        None
    );
}

// ── main/worker part-map equivalence (ref slide-nav.test.ts:108-129) ────────

/// M2: worker mode SERIALIZES each slide's `partName` into
/// `PresentationMeta.partNames` and the main thread rebuilds the index from it.
/// Both paths must resolve every part name to the same index, so a
/// serialization drop or re-order is the only way they could diverge.
/// ref slide-nav.test.ts:108-129
#[test]
fn main_and_worker_part_maps_resolve_identically() {
    let model = three_slide_deck();

    // worker: `partNames: pres.slides.map((s) => s.partName)` after
    // structured-clone — modelled by a JSON round-trip of the whole deck.
    let wire = serde_json::to_string(&model).expect("serialize deck");
    let from_meta: Vec<Slide> = serde_json::from_str(&wire).expect("deserialize deck");
    assert_eq!(from_meta.len(), model.len());

    let main_links = extract_deck_hyperlinks(&model);
    let worker_links = extract_deck_hyperlinks(&from_meta);
    assert_eq!(worker_links.len(), main_links.len());
    assert!(!main_links.is_empty(), "non-degenerate deck");

    for (index, part) in PART_NAMES.iter().enumerate() {
        let target = format!("../slides/{}", part.rsplit('/').next().unwrap_or(part));
        let main = resolve_internal_slide(&target, PART_NAMES[0], &model);
        let worker = resolve_internal_slide(&target, PART_NAMES[0], &from_meta);
        assert_eq!(main, Some(index));
        assert_eq!(worker, main, "{part} resolves identically off the wire");
        assert_eq!(from_meta[index].part_name.as_deref(), Some(*part));
    }
}

// ── zoom contract (ref viewer-zoom-contract.test.ts:36-52) ──────────────────

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("none".into()))
    }
    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn compiled_plain_slide() -> office_terminal_core::contracts::VectorSlide {
    let deck = vec![slide(0, Some(PART_NAMES[0]), None, None)];
    let (tx, _rx) = unbounded();
    DefaultSlideCompiler::new()
        .compile_in_deck(&deck[0], &deck, SLIDE_W, SLIDE_H, &tx)
        .expect("compile slide")
}

/// "renders at opts.width verbatim when no zoom method is called" — the target
/// width reaches the framebuffer unchanged. ref viewer-zoom-contract.test.ts:36-39
#[test]
fn render_width_is_honoured_verbatim() {
    let vector = compiled_plain_slide();
    let target = RenderTargetSpec {
        width_px: 1200,
        height_px: 900,
    };
    let bitmap = TinySkiaRasterizer::new()
        .rasterize(&vector, &target, &NoAssets)
        .expect("rasterize");
    assert_eq!(bitmap.width, 1200);
    assert_eq!(bitmap.height, 900);
}
