//! Viewer-navigation parity batch (presenter state machine) — the reference
//! viewer resolves an internal hyperlink to a slide index/verb and then applies
//! it to the current slide; in the Rust pipeline that second half is
//! `office_terminal_tui::app::state::PresentationState`.
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/`
//! `slide-nav.test.ts:62-79` (relative show-jump arithmetic),
//! `viewer-internal-nav.test.ts:41-81` (click dispatch branches),
//! `viewer-hidden-skip.test.ts:31-95` (goToSlide in show/skip/dim mode),
//! `scroll-viewer-hidden.test.ts:83-160` (absolute indices, mode switch),
//! `viewer-zoom-contract.test.ts:58-72` (scale clamp + change gate) and
//! `scroll-viewer.test.ts:1198-1204` (fit-page scale, ported to the terminal
//! cell fit).
//!
//! The hidden-slide policy (`HiddenSlideMode`, `next_visible`, `resolve_visible`,
//! `count_visible`) lives in `office_terminal_core::navigation` and is covered here
//! through `PresentationState`; its pure half is asserted in the core nav
//! target. The run-level find controller (`PptxFindController`) is V2 (wave 4);
//! the fuzzy search index here already excludes hidden slides under `skip`.

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{HyperlinkTarget, RectF32, SlideAction, SlideLink};
use office_terminal_core::{extract_slide_hyperlinks, HiddenSlideMode};
use office_terminal_tui::app::link_state::LinkDispatch;
use office_terminal_tui::app::state::PresentationState;
use office_terminal_tui::widget::layout::{compute_slide_rect, DEFAULT_CELL_ASPECT_RATIO};
use pptx_model::Slide;
use ratatui::layout::Rect;

const SLIDE_W: i64 = 9_144_000;
const SLIDE_H: i64 = 6_858_000;

// ── fixtures ────────────────────────────────────────────────────────────────

/// Slide `index` with `part` and an optional shape hyperlink (`link` + `action`).
fn slide(index: usize, hidden: bool, link: Option<&str>, action: Option<&str>) -> Slide {
    slide_full(index, hidden, link, action, None)
}

fn slide_full(index: usize, hidden: bool, link: Option<&str>, action: Option<&str>, text: Option<&str>) -> Slide {
    let mut shape = serde_json::json!({
        "type": "shape", "x": 0, "y": 0, "width": 100_000, "height": 100_000,
        "rotation": 0.0, "flipH": false, "flipV": false, "geometry": "rect"
    });
    if let Some(link) = link {
        shape["hyperlink"] = link.into();
    }
    if let Some(action) = action {
        shape["hyperlinkAction"] = action.into();
    }
    if let Some(text) = text {
        let body_json = format!(
            r#"{{"verticalAnchor":"t","defaultFontSize":20,"defaultBold":null,"defaultItalic":null,"lIns":0,"rIns":0,"tIns":0,"bIns":0,"wrap":"square","vert":"horz","autoFit":"none","paragraphs":[{{"alignment":"l","marL":0,"marR":0,"indent":0,"spaceBefore":null,"spaceAfter":null,"spaceLine":null,"lvl":0,"bullet":{{"type":"none"}},"defFontSize":null,"defColor":null,"defBold":null,"defItalic":null,"defFontFamily":null,"tabStops":[],"defTabSz":null,"rtl":false,"eaLnBrk":true,"fontAlgn":null,"runs":[{{"type":"text","text":"{text}","bold":null,"italic":null,"underline":false,"strikethrough":false,"strikeDouble":false,"hyperlinkUsesTextFill":false,"fieldType":null,"fontSize":20,"color":"000000","fontFamily":"Arial"}}]}}]}}"#
        );
        shape["textBody"] = serde_json::from_str(&body_json).unwrap();
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
    (0..count).map(|index| slide(index, false, None, None)).collect()
}

fn presentation(deck: Vec<Slide>) -> PresentationState {
    let mut state = PresentationState::new(deck.len(), vec![]);
    state.set_slide_dimensions(SLIDE_W, SLIDE_H);
    state.set_layout_dimensions(80, 24);
    state.set_slides(deck);
    state
}

/// A `skip`-mode presenter over `deck`.
fn skip_presentation(deck: Vec<Slide>) -> PresentationState {
    let mut state = presentation(deck);
    state.set_hidden_slide_mode(HiddenSlideMode::Skip);
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
    SlideLink::new(HyperlinkTarget::Action(action), RectF32::new(0.0, 0.0, 0.0, 0.0), None)
}

// ── relative show-jump arithmetic (ref slide-nav.test.ts:62-79) ─────────────

/// "resolves relative show-jump verbs against the current slide".
/// ref slide-nav.test.ts:62-71, core nav `resolveRelativeSlideJump`.
#[test]
fn show_jump_arithmetic_steps_from_the_current_slide() {
    let mut state = presentation(plain_deck(3));
    assert!(state.set_current_slide(1));

    let links = [(SlideAction::NextSlide, 2usize), (SlideAction::PreviousSlide, 0usize)];
    for (action, expected) in links {
        state.set_current_slide(1);
        let link = control(action);
        assert_eq!(
            state.dispatch_link(&link),
            LinkDispatch::Navigate { changed: true },
            "{action:?} navigates"
        );
        assert_eq!(state.current_slide(), expected, "{action:?} steps one slide");
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
        slide(0, false, Some("../slides/slide3.xml"), Some("ppaction://hlinksldjump")),
        slide(1, false, None, None),
        slide(2, false, None, None),
    ];
    let link = link_of(&deck, 0);
    assert_eq!(link.target, HyperlinkTarget::SlideIndex(2));

    let mut state = presentation(deck);
    assert_eq!(state.dispatch_link(&link), LinkDispatch::Navigate { changed: true });
    assert_eq!(state.current_slide(), 2);
}

/// "resolves a relative jump against the CURRENT slide".
/// ref viewer-internal-nav.test.ts:51-60
#[test]
fn relative_jump_resolves_against_the_current_slide() {
    let deck = vec![slide(0, false, None, Some("ppaction://hlinkshowjump?jump=nextslide"))];
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
        slide(0, false, Some("../slides/nope.xml"), Some("ppaction://hlinksldjump")),
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
    assert_eq!(height_limited.width, 27, "10 rows * 2.6667 = 26.67 -> 27 cols");
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

// ── hidden-slide policy (ref viewer-hidden-skip.test.ts / scroll-viewer-hidden)

/// A deck of `count` slides with `hidden` indices flagged `<p:sld show="0">`.
fn deck_with_hidden(count: usize, hidden: &[usize]) -> Vec<Slide> {
    (0..count)
        .map(|index| slide(index, hidden.contains(&index), None, None))
        .collect()
}

/// "goToSlide on a hidden slide lands on the next shown slide, else the
/// previous one" — including the clamped out-of-range target, and no hidden
/// slide is ever drawn. ref viewer-hidden-skip.test.ts:31-48
#[test]
fn skip_mode_go_to_hidden_slide_lands_on_the_next_shown_slide() {
    let mut state = skip_presentation(deck_with_hidden(5, &[1, 2, 4]));
    assert!(state.set_current_slide(1));
    assert_eq!(state.current_slide(), 3);
    // No shown slide after 4 -> the nearest shown slide before it.
    assert!(!state.set_current_slide(4), "the target resolves to 3 again");
    assert_eq!(state.current_slide(), 3);
    // Clamped out-of-range index follows the same rule, slide 0 stays put.
    assert!(!state.set_current_slide(99));
    assert_eq!(state.current_slide(), 3);
    assert!(state.set_current_slide(0), "slide 0 is shown");
    assert_eq!(state.current_slide(), 0);
    assert!(!state.set_current_slide(0));
    assert!(!state.is_slide_hidden(0));
}

/// Sequential navigation jumps over runs of hidden slides and stops at the
/// deck edges — the last slide being hidden leaves the previous shown slide.
/// ref viewer.ts:466-510, scroll-viewer-hidden.test.ts:103-121
#[test]
fn sequential_navigation_skips_hidden_slides() {
    let mut state = skip_presentation(deck_with_hidden(6, &[1, 3, 5]));
    assert!(state.next_slide());
    assert_eq!(state.current_slide(), 2);
    assert!(state.next_slide());
    assert_eq!(state.current_slide(), 4);
    assert!(!state.next_slide(), "the last slide is hidden: stay on 4");
    assert!(state.prev_slide());
    assert_eq!(state.current_slide(), 2);
    assert!(state.prev_slide());
    assert_eq!(state.current_slide(), 0);
    assert!(!state.prev_slide());
    // The presenter preview follows the same policy.
    assert_eq!(state.next_slide_index(), Some(2));
}

/// "every slide hidden: navigates and searches as in show" — `skip` degenerates
/// to `show`, so hidden slides are addressable and searchable again.
/// ref viewer-hidden-skip.test.ts:50-60, scroll-viewer-hidden.test.ts:138-146
#[test]
fn all_hidden_deck_navigates_as_in_show_mode() {
    let mut state = skip_presentation(deck_with_hidden(3, &[0, 1, 2]));
    assert!(state.set_current_slide(1));
    assert_eq!(state.current_slide(), 1);
    assert!(state.next_slide());
    assert_eq!(state.current_slide(), 2);
    assert!(state.prev_slide());
    assert_eq!(state.current_slide(), 1);
    assert_eq!(state.visible_slide_count(), 0);
    state.open_search();
    assert_eq!(state.search_state().matched_indices(), &[0, 1, 2]);
}

/// "show and dim modes still honour goToSlide on a hidden slide".
/// ref viewer-hidden-skip.test.ts:84-90
#[test]
fn show_and_dim_modes_still_honour_go_to_slide_on_a_hidden_slide() {
    for mode in [HiddenSlideMode::Show, HiddenSlideMode::Dim] {
        let mut state = presentation(deck_with_hidden(3, &[1]));
        state.set_hidden_slide_mode(mode);
        assert!(state.set_current_slide(1));
        assert_eq!(state.current_slide(), 1, "{mode:?} keeps hidden addressable");
    }
}

/// "an internal hyperlink to a hidden slide follows the goToSlide rule".
/// ref viewer-hidden-skip.test.ts:93-104
#[test]
fn internal_link_to_a_hidden_slide_follows_the_go_to_slide_rule() {
    let mut deck = deck_with_hidden(3, &[1]);
    deck[0] = slide(0, false, Some("../slides/slide2.xml"), Some("ppaction://hlinksldjump"));
    let link = link_of(&deck, 0);
    assert_eq!(link.target, HyperlinkTarget::SlideIndex(1));
    let mut state = skip_presentation(deck);
    assert_eq!(state.dispatch_link(&link), LinkDispatch::Navigate { changed: true });
    assert_eq!(state.current_slide(), 2, "hidden slide 1 resolves forward");
}

/// Initial slide selection: entering `skip` with a hidden opening slide lands
/// on the first shown one. ref viewer.ts:512-522 (`_initialSlide`), :120-121
#[test]
fn initial_slide_selection_lands_on_the_first_shown_slide() {
    let mut state = PresentationState::new(4, vec![]);
    state.set_hidden_slide_mode(HiddenSlideMode::Skip);
    state.set_slides(deck_with_hidden(4, &[0, 1]));
    assert_eq!(state.current_slide(), 2);
    assert_eq!(state.total_slides(), 4);
    assert_eq!(state.visible_slide_count(), 2);
}

/// `visibleSlideCount` counts shown slides only; indices stay absolute.
/// ref viewer.ts:577-582, scroll-viewer-hidden.test.ts:83-101
#[test]
fn visible_slide_count_reports_shown_slides_only() {
    let state = skip_presentation(deck_with_hidden(6, &[1, 2, 4]));
    assert_eq!(state.visible_slide_count(), 3);
    assert_eq!(state.total_slides(), 6);
    assert!(state.is_slide_hidden(2));
    assert!(!state.is_slide_hidden(3));
}

/// Entering `skip` while on a hidden slide advances forward, else backward.
/// ref viewer.ts:545-566, scroll-viewer-hidden.test.ts:207-219
#[test]
fn entering_skip_mode_moves_off_a_hidden_slide() {
    let deck = deck_with_hidden(4, &[1, 3]);
    let mut state = presentation(deck);
    state.set_current_slide(1);
    assert!(state.set_hidden_slide_mode(HiddenSlideMode::Skip));
    assert_eq!(state.current_slide(), 2);
    // In `show` a hidden slide is addressable; entering `skip` moves backward
    // when no shown slide follows it.
    state.set_hidden_slide_mode(HiddenSlideMode::Show);
    assert!(state.set_current_slide(3));
    assert_eq!(state.current_slide(), 3);
    assert!(state.set_hidden_slide_mode(HiddenSlideMode::Skip));
    assert_eq!(state.current_slide(), 2);
    assert!(!state.set_hidden_slide_mode(HiddenSlideMode::Skip), "same mode");
    // Returning to `show` keeps the current slide.
    assert!(!state.set_hidden_slide_mode(HiddenSlideMode::Show));
    assert_eq!(state.current_slide(), 2);
}

/// "switching to Skip clears the active match and keeps a shown current
/// slide". ref scroll-viewer-hidden.test.ts:185-205
#[test]
fn switching_to_skip_clears_the_active_search_and_keeps_a_shown_slide() {
    let mut state = presentation(deck_with_hidden(6, &[1, 2]));
    state.set_current_slide(3);
    state.open_search();
    assert!(state.search_state().selected_slide_index().is_some());
    state.set_hidden_slide_mode(HiddenSlideMode::Skip);
    assert_eq!(state.current_slide(), 3, "slide 3 is shown");
    assert_eq!(state.search_state().selected_slide_index(), None);
}

/// "find does not match hidden slides" — the fuzzy search index follows the
/// same policy the run-level find controller will (V2).
/// ref scroll-viewer-hidden.test.ts:123-128
#[test]
fn search_never_matches_a_hidden_slide_under_skip() {
    let mut state = skip_presentation(deck_with_hidden(4, &[1]));
    state.open_search();
    assert_eq!(state.search_state().matched_indices(), &[0, 2, 3]);
    state.set_hidden_slide_mode(HiddenSlideMode::Show);
    state.open_search();
    assert_eq!(state.search_state().matched_indices(), &[0, 1, 2, 3]);
}

/// "findText/findNext skip hidden slides" — run-level find excludes hidden slides under skip.
/// ref viewer-hidden-skip.test.ts:95-111
#[test]
fn find_text_excludes_hidden_slides() {
    let deck = vec![
        slide_full(0, false, None, None, Some("target")),
        slide_full(1, true, None, None, Some("target")),
    ];
    let mut state = skip_presentation(deck);
    state.sync_find_index();
    state.open_find();
    for ch in "target".chars() {
        state.find_push_char(ch);
    }
    assert_eq!(state.find_state().matches().len(), 1);
    assert_eq!(state.find_state().matches()[0].slide, 0);

    state.close_find();
    state.set_hidden_slide_mode(HiddenSlideMode::Show);
    state.open_find();
    for ch in "target".chars() {
        state.find_push_char(ch);
    }
    assert_eq!(state.find_state().matches().len(), 2);
}

/// "setScale clamps to [zoomMin, zoomMax]". ref viewer-zoom-contract.test.ts:58-63
#[test]
fn zoom_scale_clamps_to_the_configured_range() {
    let mut state = presentation(plain_deck(3));
    state.set_zoom_bounds(0.5, 2.0);
    assert!(state.set_scale(10.0));
    assert_eq!(state.scale(), 2.0);
    assert!(state.set_scale(0.01));
    assert_eq!(state.scale(), 0.5);
    // Family defaults clamp to [0.1, 4].
    let mut default_state = presentation(plain_deck(3));
    assert_eq!(default_state.zoom_bounds(), (0.1, 4.0));
    default_state.set_scale(99.0);
    assert_eq!(default_state.scale(), 4.0);
}

/// "setScale does not fire onScaleChange when the factor is unchanged".
/// ref viewer-zoom-contract.test.ts:65-72
#[test]
fn set_scale_does_not_fire_for_an_unchanged_factor() {
    let mut state = presentation(plain_deck(3));
    assert!(state.set_scale(1.5));
    assert_eq!(state.scale_changes(), 1);
    assert!(!state.set_scale(1.5), "same factor: no change event");
    assert_eq!(state.scale_changes(), 1);
}

/// "delegates to a caller-supplied onHyperlinkClick with slideIndex populated,
/// and does NOT navigate". ref viewer-internal-nav.test.ts:52-72
#[test]
fn caller_callback_owns_the_internal_click() {
    let deck = vec![
        slide(0, false, Some("../slides/slide3.xml"), Some("ppaction://hlinksldjump")),
        slide(1, false, None, None),
        slide(2, false, None, None),
    ];
    let link = link_of(&deck, 0);
    let mut state = presentation(deck);
    state.set_internal_link_delegation(true);
    assert_eq!(state.dispatch_link(&link), LinkDispatch::Internal { slide_index: 2 });
    assert_eq!(state.current_slide(), 0, "the callback owns the click");
}
