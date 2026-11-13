//! Viewer-navigation parity batch (presenter state machine) — the reference
//! viewer resolves an internal hyperlink to a slide index/verb and then applies
//! it to the current slide; in the Rust pipeline that second half is
//! `kitty_pptx_tui::app::state::PresentationState`.
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/`
//! `slide-nav.test.ts:62-79` (relative show-jump arithmetic),
//! `viewer-internal-nav.test.ts:41-81` (click dispatch branches),
//! `viewer-hidden-skip.test.ts:31-95` (goToSlide in show/dim mode),
//! `viewer-zoom-contract.test.ts:113-118` and `scroll-viewer.test.ts:1198-1204`
//! (fit-page scale, ported to the terminal cell fit).
//!
//! BLOCKED (no production API, see `tests/parity/coverage/viewer.md`):
//! `HiddenSlideMode` / `nextVisibleIndex` / `resolveVisibleIndex` (no
//! hidden-slide policy reaches navigation), `onHyperlinkClick` (no delegation
//! seam; `dispatch_link` always navigates) and `setScale` / `zoomMin` /
//! `zoomMax` (no presenter zoom).
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_viewer_nav_tests.rs.txt`; the
//! ledger keeps their rows.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{HyperlinkTarget, RectF32, SlideAction, SlideLink};
use kitty_pptx_core::extract_slide_hyperlinks;
use kitty_pptx_tui::app::link_state::LinkDispatch;
use kitty_pptx_tui::app::state::PresentationState;
use kitty_pptx_tui::widget::layout::{compute_slide_rect, DEFAULT_CELL_ASPECT_RATIO};
use pptx_model::Slide;
use ratatui::layout::Rect;

const SLIDE_W: i64 = 9_144_000;
const SLIDE_H: i64 = 6_858_000;

// ── fixtures ────────────────────────────────────────────────────────────────

/// Slide `index` with `part` and an optional shape hyperlink (`link` + `action`).
fn slide(index: usize, hidden: bool, link: Option<&str>, action: Option<&str>) -> Slide {
    let mut shape = serde_json::json!({
        "type": "shape",
        "x": 0,
        "y": 0,
        "width": 100_000,
        "height": 100_000,
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
    serde_json::from_value(serde_json::json!({
        "index": index,
        "slideNumber": index + 1,
        "partName": format!("ppt/slides/slide{}.xml", index + 1),
        "elements": [shape],
        "elementSources": [],
        "comments": [],
        "hidden": hidden
    }))
    .expect("slide JSON")
}

fn plain_deck(count: usize) -> Vec<Slide> {
    (0..count)
        .map(|index| slide(index, false, None, None))
        .collect()
}

fn presentation(deck: Vec<Slide>) -> PresentationState {
    let mut state = PresentationState::new(deck.len(), vec![]);
    state.set_slide_dimensions(SLIDE_W, SLIDE_H);
    state.set_layout_dimensions(80, 24);
    state.set_slides(deck);
    state
}

/// The single link extracted from slide `index` of `deck`.
fn link_of(deck: &[Slide], index: usize) -> SlideLink {
    let (tx, _rx) = unbounded();
    let links = extract_slide_hyperlinks(&deck[index], deck, &tx);
    assert_eq!(links.len(), 1, "exactly one link on slide {index}");
    links.into_iter().next().expect("one link")
}

/// A zero-area control link carrying `action` (`ppaction://` verbs never carry
/// geometry; the arrow keys do).
fn control(action: SlideAction) -> SlideLink {
    SlideLink::new(
        HyperlinkTarget::Action(action),
        RectF32::new(0.0, 0.0, 0.0, 0.0),
        None,
    )
}

// ── relative show-jump arithmetic (ref slide-nav.test.ts:62-79) ─────────────

/// "resolves relative show-jump verbs against the current slide".
/// ref slide-nav.test.ts:62-71, core nav `resolveRelativeSlideJump`.
#[test]
fn show_jump_arithmetic_steps_from_the_current_slide() {
    let mut state = presentation(plain_deck(3));
    assert!(state.set_current_slide(1));

    let links = [
        (SlideAction::NextSlide, 2usize),
        (SlideAction::PreviousSlide, 0usize),
    ];
    for (action, expected) in links {
        state.set_current_slide(1);
        let link = control(action);
        assert_eq!(
            state.dispatch_link(&link),
            LinkDispatch::Navigate { changed: true },
            "{action:?} navigates"
        );
        assert_eq!(
            state.current_slide(),
            expected,
            "{action:?} steps one slide"
        );
    }

    // firstslide / lastslide land on the deck ends regardless of position.
    state.set_current_slide(1);
    let first = control(SlideAction::FirstSlide);
    state.dispatch_link(&first);
    assert_eq!(state.current_slide(), 0);

    state.set_current_slide(0);
    let last = control(SlideAction::LastSlide);
    state.dispatch_link(&last);
    assert_eq!(state.current_slide(), 2);
}

/// "clamps nextslide/previousslide at the deck ends".
/// ref slide-nav.test.ts:74-79
#[test]
fn show_jump_clamps_at_the_deck_ends() {
    let mut state = presentation(plain_deck(3));

    state.set_current_slide(2);
    let next = control(SlideAction::NextSlide);
    assert_eq!(
        state.dispatch_link(&next),
        LinkDispatch::Navigate { changed: false },
        "next on the last slide stays put"
    );
    assert_eq!(state.current_slide(), 2);

    state.set_current_slide(0);
    let previous = control(SlideAction::PreviousSlide);
    assert_eq!(
        state.dispatch_link(&previous),
        LinkDispatch::Navigate { changed: false },
        "previous on the first slide stays put"
    );
    assert_eq!(state.current_slide(), 0);
}

/// `ppaction://hlinkshowjump?jump=endshow` is a control action, never a slide.
/// ref slide-nav.test.ts:83-88 (an unresolvable ref does not navigate)
#[test]
fn end_show_is_not_a_slide_navigation() {
    let mut state = presentation(plain_deck(3));
    state.set_current_slide(1);
    let link = control(SlideAction::EndShow);
    assert_eq!(state.dispatch_link(&link), LinkDispatch::EndShow);
    assert_eq!(state.current_slide(), 1, "endshow leaves the slide alone");
}

// ── internal-hyperlink click dispatch (ref viewer-internal-nav.test.ts) ─────

/// "resolves an internal ref via the engine and navigates to that slide".
/// ref viewer-internal-nav.test.ts:41-49
#[test]
fn internal_link_navigates_to_the_resolved_slide() {
    let deck = vec![
        slide(
            0,
            false,
            Some("../slides/slide3.xml"),
            Some("ppaction://hlinksldjump"),
        ),
        slide(1, false, None, None),
        slide(2, false, None, None),
    ];
    let link = link_of(&deck, 0);
    assert_eq!(link.target, HyperlinkTarget::SlideIndex(2));

    let mut state = presentation(deck);
    assert_eq!(
        state.dispatch_link(&link),
        LinkDispatch::Navigate { changed: true }
    );
    assert_eq!(state.current_slide(), 2);
}

/// "resolves a relative jump against the CURRENT slide".
/// ref viewer-internal-nav.test.ts:51-60
#[test]
fn relative_jump_resolves_against_the_current_slide() {
    let deck = vec![slide(
        0,
        false,
        None,
        Some("ppaction://hlinkshowjump?jump=nextslide"),
    )];
    let link = link_of(&deck, 0);
    assert_eq!(link.target, HyperlinkTarget::Action(SlideAction::NextSlide));

    let mut deck = deck;
    deck.extend(plain_deck(6).into_iter().skip(1));
    let mut state = presentation(deck);
    state.set_current_slide(2);
    state.dispatch_link(&link);
    assert_eq!(state.current_slide(), 3, "nextslide(2) == 3");
}

/// "is a safe no-op when the ref resolves to no reachable slide" — no link is
/// extracted, so no dispatch can happen. ref viewer-internal-nav.test.ts:62-70
#[test]
fn unresolvable_internal_ref_yields_no_link() {
    let deck = vec![
        slide(
            0,
            false,
            Some("../slides/nope.xml"),
            Some("ppaction://hlinksldjump"),
        ),
        slide(1, false, None, None),
    ];
    let (tx, _rx) = unbounded();
    assert!(
        extract_slide_hyperlinks(&deck[0], &deck, &tx).is_empty(),
        "an unresolvable slide jump is not offered as a link"
    );
    let state = presentation(deck);
    assert_eq!(state.current_slide_links().len(), 0);
    assert!(state.link_at_cell(0, 0).is_none());
}

/// "opens an external link in a new tab" — the presenter reports the validated
/// URL to its caller instead of spawning from the state machine.
/// ref viewer-internal-nav.test.ts:73-81
#[test]
fn external_link_dispatches_the_sanitized_url() {
    let deck = vec![slide(0, false, Some("https://example.com/"), None)];
    let link = link_of(&deck, 0);
    let mut state = presentation(deck);
    assert_eq!(
        state.dispatch_link(&link),
        LinkDispatch::OpenExternal("https://example.com/".to_string())
    );
    assert_eq!(state.current_slide(), 0, "an external link never navigates");
}

// ── goToSlide / hidden slides (ref viewer-hidden-skip.test.ts) ─────────────

/// The presenter opens on the first slide. ref viewer-hidden-skip.test.ts:120-121
#[test]
fn initial_slide_is_the_first_slide() {
    let state = presentation(plain_deck(6));
    assert_eq!(state.current_slide(), 0);
    assert_eq!(state.total_slides(), 6);
}

/// "Clamped out-of-range index follows the same rule" — in `show` mode (the
/// only mode the presenter implements) the target clamps to the last slide.
/// ref viewer-hidden-skip.test.ts:46-48
#[test]
fn go_to_slide_clamps_out_of_range_targets() {
    let mut state = presentation(plain_deck(5));
    assert!(state.set_current_slide(99));
    assert_eq!(state.current_slide(), 4);
    assert!(state.set_current_slide(0));
    assert_eq!(state.current_slide(), 0);
}

/// "show and dim modes still honour goToSlide on a hidden slide": a hidden
/// slide is addressable — the presenter has no skip policy, so it always
/// behaves like `show`. ref viewer-hidden-skip.test.ts:84-90
#[test]
fn hidden_slide_is_addressable_in_show_mode() {
    let hidden_index = 1usize;
    let mut deck = plain_deck(3);
    deck[hidden_index] = slide(hidden_index, true, None, None);
    let mut state = presentation(deck);
    assert!(state.set_current_slide(hidden_index));
    assert_eq!(state.current_slide(), hidden_index);
    // ... and an internal link to that slide lands on it too.
    let link = SlideLink::new(
        HyperlinkTarget::SlideIndex(hidden_index),
        RectF32::new(0.0, 0.0, 0.0, 0.0),
        None,
    );
    state.set_current_slide(0);
    state.dispatch_link(&link);
    assert_eq!(state.current_slide(), hidden_index);
}

// ── fit-page scale (ref viewer-zoom-contract.test.ts:113-118) ──────────────

/// "takes the tighter of width/height". The single-canvas viewer computes this
/// in CSS pixels; the presenter computes the same fit in terminal cells.
/// ref viewer-zoom-contract.test.ts:113-118, scroll-viewer.test.ts:1198-1204
#[test]
fn fit_page_takes_the_tighter_of_width_and_height() {
    // Container 80x60 cells, 4:3 slide, 0.5 cell aspect:
    // cell_ratio = (4/3) / 0.5 = 2.6667; width fit needs 80/2.6667 = 30 rows.
    let width_limited = compute_slide_rect(Rect::new(0, 0, 80, 60), 4.0 / 3.0, 0.5);
    assert_eq!((width_limited.width, width_limited.height), (80, 30));
    // ... and it is centred in the taller container: (60 - 30) / 2 = 15.
    assert_eq!(width_limited.y, 15);

    // Short container 80x10: the width fit (30 rows) does not fit, so the
    // height wins — exactly `Math.min(w / naturalW, h / naturalH)`.
    let height_limited = compute_slide_rect(Rect::new(0, 0, 80, 10), 4.0 / 3.0, 0.5);
    assert_eq!(height_limited.height, 10);
    assert_eq!(
        height_limited.width, 27,
        "10 rows * 2.6667 = 26.67 -> 27 cols"
    );
    assert!(height_limited.width < 80);
}

/// "base fit scale maps the slide width to the container width" — when the
/// container is a cell grid wider than the slide needs, the fit spans it.
/// ref scroll-viewer.test.ts:1198-1204
#[test]
fn base_fit_spans_the_container_width_when_it_fits() {
    let wide = compute_slide_rect(Rect::new(0, 0, 120, 60), 4.0 / 3.0, 0.5);
    assert_eq!(wide.width, 120);
    assert_eq!(wide.height, 45);
    assert_eq!(wide.x, 0);
    assert_eq!(wide.y, 7, "(60 - 45) / 2");
    // The fitted cell box has the slide's own aspect once the cell aspect is
    // folded in: (cols / rows) * cell_aspect == 4/3.
    let rendered_aspect = (wide.width as f32 / wide.height as f32) * DEFAULT_CELL_ASPECT_RATIO;
    assert!((rendered_aspect - 4.0 / 3.0).abs() < 0.01);
}
