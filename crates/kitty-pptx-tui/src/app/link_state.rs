use std::process::{Command, Stdio};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use kitty_pptx_core::{
    sanitize_hyperlink_url, HyperlinkTarget, SecurityError, SlideAction, SlideLink,
    SlideLinkIndex,
};

use crate::app::events::AppAction;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// Error produced while dispatching an external hyperlink.
#[derive(Debug, thiserror::Error)]
pub enum BrowserLaunchError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("failed to spawn web browser: {0}")]
    Spawn(String),
    #[error("web browser launch is unsupported on this platform")]
    UnsupportedPlatform,
}

/// Spawn the OS default browser without blocking the TUI event loop.
///
/// The child gets a separate process group and null stdio so it cannot write
/// over the terminal or receive the presenter's Ctrl-C.
pub fn launch_external_browser(url: &str) -> Result<(), BrowserLaunchError> {
    let sanitized = sanitize_hyperlink_url(url)?;

    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-Command", "Start-Process"]);
        command
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = sanitized;
        return Err(BrowserLaunchError::UnsupportedPlatform);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .arg(&sanitized)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| BrowserLaunchError::Spawn(error.to_string()))?;

    // Reap the short-lived launcher on a helper thread so the TUI never blocks
    // and no zombie remains after xdg-open/open exits.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Result of activating a hyperlink from the picker or a mouse click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkDispatch {
    /// Nothing was activated.
    None,
    /// The caller should spawn a browser for this validated URL.
    OpenExternal(String),
    /// Slide navigation completed; `changed` reports whether the index moved.
    Navigate { changed: bool },
    /// A caller-owned internal click (the `onHyperlinkClick` delegation seam):
    /// the enriched slide index is reported and the viewer does not navigate.
    /// ref viewer-internal-nav.test.ts:52-72
    Internal { slide_index: usize },
    /// `ppaction://hlinkshowjump?jump=endshow` — terminate the presentation.
    EndShow,
}

/// Selection state for the active slide's hyperlink picker modal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkPickerState {
    links: Vec<SlideLink>,
    selected: usize,
}

impl LinkPickerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_links(&mut self, links: Vec<SlideLink>) {
        self.links = links;
        self.selected = 0;
    }

    pub fn links(&self) -> &[SlideLink] {
        &self.links
    }

    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_link(&self) -> Option<&SlideLink> {
        self.links.get(self.selected)
    }

    pub fn select_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_down(&mut self) {
        if !self.links.is_empty() {
            self.selected = (self.selected + 1).min(self.links.len() - 1);
        }
    }

    /// Map `1..=9` to the first nine entries and `0` to the tenth.
    pub fn quick_key_index(ch: char) -> Option<usize> {
        match ch {
            '1'..='9' => Some(ch as usize - '1' as usize),
            '0' => Some(9),
            _ => None,
        }
    }

    pub fn activate_quick_key(&mut self, ch: char) -> Option<SlideLink> {
        let index = Self::quick_key_index(ch)?;
        let link = self.links.get(index)?.clone();
        self.selected = index;
        Some(link)
    }
}

/// PresentationState extensions for hyperlink picker + mouse hit testing.
impl PresentationState {
    pub fn link_picker(&self) -> &LinkPickerState {
        &self.link_picker
    }

    pub fn current_slide_links(&self) -> &[SlideLink] {
        self.deck.slide_links_for(self.current_slide())
    }

    pub fn current_slide_link_count(&self) -> usize {
        self.current_slide_links().len()
    }

    pub fn current_slide_link_index(&self) -> SlideLinkIndex {
        SlideLinkIndex::build(self.current_slide_links().iter().cloned())
    }

    pub fn toggle_link_picker(&mut self) {
        if self.mode() == AppMode::LinkPickerModal {
            self.close_link_picker();
        } else {
            self.open_link_picker();
        }
    }

    pub fn open_link_picker(&mut self) {
        if self.current_slide_links().is_empty() {
            self.set_toast("No hyperlinks on this slide");
            return;
        }
        let links = self.current_slide_links().to_vec();
        self.link_picker.set_links(links);
        self.enter_overlay(AppMode::LinkPickerModal);
    }

    pub fn close_link_picker(&mut self) {
        if self.mode() == AppMode::LinkPickerModal {
            self.exit_overlay();
        }
    }

    pub fn link_picker_up(&mut self) {
        self.link_picker.select_up();
    }

    pub fn link_picker_down(&mut self) {
        self.link_picker.select_down();
    }

    pub fn activate_selected_link(&mut self) -> LinkDispatch {
        let index = self.link_picker.selected_index();
        self.activate_link_index(index)
    }

    pub fn activate_quick_link(&mut self, ch: char) -> LinkDispatch {
        let Some(link) = self.link_picker.activate_quick_key(ch) else {
            return LinkDispatch::None;
        };
        self.close_link_picker();
        self.dispatch_link(&link)
    }

    pub fn activate_link_index(&mut self, index: usize) -> LinkDispatch {
        let Some(link) = self.link_picker.links().get(index).cloned() else {
            return LinkDispatch::None;
        };
        self.close_link_picker();
        self.dispatch_link(&link)
    }

    pub fn dispatch_link(&mut self, link: &SlideLink) -> LinkDispatch {
        match &link.target {
            HyperlinkTarget::ExternalUri(url) => LinkDispatch::OpenExternal(url.clone()),
            HyperlinkTarget::SlideIndex(index) => {
                // A caller-supplied callback owns the click; the reference hands
                // it the enriched target (slide index populated) and does not
                // navigate. ref viewer-internal-nav.test.ts:52-72
                if self.delegates_internal_links() {
                    return LinkDispatch::Internal {
                        slide_index: *index,
                    };
                }
                let changed = self.set_current_slide(*index);
                LinkDispatch::Navigate { changed }
            }
            HyperlinkTarget::Action(action) => self.dispatch_slide_action(*action),
        }
    }

    fn dispatch_slide_action(&mut self, action: SlideAction) -> LinkDispatch {
        match action {
            SlideAction::NextSlide => LinkDispatch::Navigate {
                changed: self.next_slide(),
            },
            SlideAction::PreviousSlide => LinkDispatch::Navigate {
                changed: self.prev_slide(),
            },
            SlideAction::FirstSlide => LinkDispatch::Navigate {
                changed: self.first_slide(),
            },
            SlideAction::LastSlide => LinkDispatch::Navigate {
                changed: self.last_slide(),
            },
            SlideAction::EndShow => LinkDispatch::EndShow,
        }
    }

    /// Map a terminal cell to slide EMU coordinates and hit-test active links.
    ///
    /// Hit-testing uses the placement the renderer actually drew, published
    /// per frame from the render pass's `FrameBindings`. That placement
    /// already encodes the fitted sub-area of the active view (fullscreen
    /// minus the status row, or the presenter-mode slide pane), the
    /// terminal's true cell aspect, and — while the view is zoomed — the
    /// normalized crop window the placement shows, so a click maps onto the
    /// correct slide coordinates at any zoom and pan. Before the first
    /// frame is drawn no placement exists and clicks fall through to plain
    /// slide advance.
    pub fn link_at_cell(&self, col: u16, row: u16) -> Option<SlideLink> {
        if self.deck.slide_width_emu <= 0 || self.deck.slide_height_emu <= 0 {
            return None;
        }
        let placed = self.current_placement?;
        let rect = placed.rect;
        if rect.cols == 0
            || rect.rows == 0
            || col < rect.col
            || row < rect.row
            || col >= rect.col.saturating_add(rect.cols)
            || row >= rect.row.saturating_add(rect.rows)
        {
            return None;
        }
        let x_ratio = (col - rect.col) as f32 / rect.cols as f32;
        let y_ratio = (row - rect.row) as f32 / rect.rows as f32;
        // Zoomed placements show a crop window of the slide: the click lands
        // inside that window, not at the top-left of the slide.
        let (x_norm, y_norm) = match placed.crop {
            Some(crop) => (crop.x + x_ratio * crop.w, crop.y + y_ratio * crop.h),
            None => (x_ratio, y_ratio),
        };
        let x_emu = x_norm.clamp(0.0, 1.0) * self.deck.slide_width_emu as f32;
        let y_emu = y_norm.clamp(0.0, 1.0) * self.deck.slide_height_emu as f32;
        self.current_slide_link_index()
            .hit_test(x_emu, y_emu)
            .cloned()
    }

    /// Route a left click: link hit wins, otherwise advance one slide.
    pub fn handle_mouse_click(&mut self, col: u16, row: u16) -> LinkDispatch {
        if self.mode().is_overlay() || self.is_blank() {
            return LinkDispatch::None;
        }
        if let Some(link) = self.link_at_cell(col, row) {
            return self.dispatch_link(&link);
        }
        LinkDispatch::Navigate {
            changed: self.next_slide(),
        }
    }
}

/// Handle keyboard input while the link picker modal is active.
pub fn handle_link_picker_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => {
            state.close_link_picker();
            AppAction::Redraw
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.link_picker_up();
            AppAction::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.link_picker_down();
            AppAction::Redraw
        }
        KeyCode::Enter => action_for_dispatch(state.activate_selected_link()),
        KeyCode::Char(ch @ ('0'..='9')) => action_for_dispatch(state.activate_quick_link(ch)),
        _ => AppAction::None,
    }
}

/// Route terminal mouse events to hyperlink hit-testing / slide advance.
pub fn handle_mouse_event(mouse: MouseEvent, state: &mut PresentationState) -> AppAction {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return AppAction::None;
    }
    action_for_dispatch(state.handle_mouse_click(mouse.column, mouse.row))
}

fn action_for_dispatch(dispatch: LinkDispatch) -> AppAction {
    match dispatch {
        LinkDispatch::None => AppAction::None,
        LinkDispatch::OpenExternal(url) => AppAction::OpenExternal(url),
        LinkDispatch::Navigate { changed: true } => AppAction::SlideChanged,
        LinkDispatch::Navigate { changed: false } => AppAction::Redraw,
        // The delegated target belongs to a caller callback this app does not
        // install; the terminal presenter keeps the current slide.
        LinkDispatch::Internal { .. } => AppAction::None,
        LinkDispatch::EndShow => AppAction::Quit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::bindings::{FrameBindings, SlidePlacement};
    use crate::widget::zoom_fit::NormalizedCrop;
    use kitty_pptx_core::RectF32;
    use kitty_pptx_display::CellRect;

    fn external(url: &str, x: f32) -> SlideLink {
        SlideLink::new(
            HyperlinkTarget::ExternalUri(url.to_string()),
            RectF32::new(x, 0.0, 100.0, 100.0),
            None,
        )
    }

    /// Plain (unzoomed) slide placement for a recorded rect.
    fn plain_placement(rect: CellRect) -> SlidePlacement {
        SlidePlacement {
            rect,
            crop: None,
            zoom: 1.0,
            overflow_cols: 0.0,
            overflow_rows: 0.0,
        }
    }

    /// State whose last drawn frame placed the slide at `placement` (a
    /// presenter-style sub-rect of the layout grid), carrying one link on the
    /// right half of the slide.
    fn state_with_right_half_link(placement: SlidePlacement) -> PresentationState {
        let mut state = PresentationState::new(2, vec![]);
        state.set_slide_dimensions(12_192_000, 6_858_000);
        state.set_layout_dimensions(100, 30);
        state.publish_frame_bindings(FrameBindings {
            current: Some(placement),
            ..FrameBindings::default()
        });
        // Right half of a 16:9 slide: x in [6_096_000, 12_192_000] EMU.
        state.deck.slide_links = vec![vec![SlideLink::new(
            HyperlinkTarget::ExternalUri("https://example.com".to_string()),
            RectF32::new(6_096_000.0, 0.0, 6_096_000.0, 6_858_000.0),
            None,
        )]];
        state
    }

    #[test]
    fn link_hits_use_the_recorded_placement_rect() {
        // Presenter-mode pane: cols 5..55 of a 100-col layout (the renderer
        // fits the slide into a sub-rect, not the full grid).
        let state = state_with_right_half_link(plain_placement(CellRect {
            row: 3,
            col: 5,
            rows: 17,
            cols: 50,
        }));

        // Clicking the right half of the recorded rect maps onto the link's
        // half of the slide.
        let hit = state
            .link_at_cell(45, 11)
            .expect("right half of the recorded rect must hit the link");
        assert_eq!(
            hit.target,
            HyperlinkTarget::ExternalUri("https://example.com".to_string())
        );

        // The left half of the recorded rect misses the right-half link.
        assert!(state.link_at_cell(6, 11).is_none());
    }

    #[test]
    fn clicks_outside_the_recorded_rect_never_hit_even_in_layout_bounds() {
        // Col 60 and row 25 lie inside the old full-grid fitted area (a
        // 100x30 grid letterboxes 16:9 into cols 0..100, rows 1..29) but
        // outside the recorded pane rect (cols 5..55, rows 3..20): the old
        // recomputed geometry hit the link from there; the recorded rect
        // correctly refuses.
        let state = state_with_right_half_link(plain_placement(CellRect {
            row: 3,
            col: 5,
            rows: 17,
            cols: 50,
        }));
        assert!(state.link_at_cell(60, 11).is_none(), "right of the pane");
        assert!(state.link_at_cell(45, 25).is_none(), "below the pane");
        assert!(state.link_at_cell(0, 0).is_none(), "above-left of the pane");

        // handle_mouse_click routes through the same rect: an outside click
        // falls through to slide advance.
        let mut state = state;
        assert_eq!(
            state.handle_mouse_click(60, 11),
            LinkDispatch::Navigate { changed: true }
        );
        assert_eq!(state.current_slide(), 1);
    }

    #[test]
    fn zoomed_placements_hit_test_through_the_crop_window() {
        // The placement covers the whole 100x30 grid; the crop window says
        // which part of the slide is on screen. Both crops below are panned
        // so the zoomed mapping disagrees with the unzoomed one: the link
        // occupies the slide's right half (slide x >= 50%).
        let base_rect = CellRect {
            row: 0,
            col: 0,
            rows: 30,
            cols: 100,
        };

        // Panned right: the window shows slide x in [60%, 100%]. A click at
        // col 10 maps to 0.6 + 0.1 * 0.4 = 64% of the slide - a hit, where
        // the unzoomed mapping (10%) would have missed.
        let panned_right = state_with_right_half_link(SlidePlacement {
            rect: base_rect,
            crop: Some(NormalizedCrop {
                x: 0.6,
                y: 0.25,
                w: 0.4,
                h: 0.5,
            }),
            zoom: 2.0,
            overflow_cols: 100.0,
            overflow_rows: 30.0,
        });
        let hit = panned_right
            .link_at_cell(10, 15)
            .expect("click maps through the panned crop window");
        assert!(matches!(
            hit.target,
            HyperlinkTarget::ExternalUri(ref url) if url == "https://example.com"
        ));

        // Panned left: the window shows slide x in [0%, 40%]. A click at
        // col 90 maps to 0.9 * 0.4 = 36% of the slide - a miss, where the
        // unzoomed mapping (90%) would have hit.
        let panned_left = state_with_right_half_link(SlidePlacement {
            rect: base_rect,
            crop: Some(NormalizedCrop {
                x: 0.0,
                y: 0.25,
                w: 0.4,
                h: 0.5,
            }),
            zoom: 2.0,
            overflow_cols: 100.0,
            overflow_rows: 30.0,
        });
        assert!(panned_left.link_at_cell(90, 15).is_none());
    }

    #[test]
    fn clicks_before_the_first_frame_do_not_hit() {
        // No frame drawn yet: no published rect, so nothing can hit and a
        // click falls through to slide advance (safe default).
        let mut state = state_with_right_half_link(plain_placement(CellRect {
            row: 3,
            col: 5,
            rows: 17,
            cols: 50,
        }));
        state.publish_frame_bindings(FrameBindings::default());
        assert!(state.link_at_cell(45, 11).is_none());
        assert_eq!(
            state.handle_mouse_click(45, 11),
            LinkDispatch::Navigate { changed: true }
        );
    }

    #[test]
    fn quick_keys_map_one_to_nine_and_zero() {
        let mut picker = LinkPickerState::new();
        picker.set_links((0..10).map(|index| external("https://example.com", index as f32)).collect());
        assert_eq!(LinkPickerState::quick_key_index('1'), Some(0));
        assert_eq!(LinkPickerState::quick_key_index('9'), Some(8));
        assert_eq!(LinkPickerState::quick_key_index('0'), Some(9));
        assert_eq!(LinkPickerState::quick_key_index('x'), None);
        assert_eq!(picker.activate_quick_key('3').map(|l| l.bounds_emu.x), Some(2.0));
        assert_eq!(picker.selected_index(), 2);
        assert!(picker.activate_quick_key('0').is_some());

        let mut short = LinkPickerState::new();
        short.set_links((0..3).map(|index| external("https://example.com", index as f32)).collect());
        assert!(short.activate_quick_key('4').is_none());
    }

    #[test]
    fn selection_clamps_at_both_ends() {
        let mut picker = LinkPickerState::new();
        picker.set_links(vec![external("https://a.example", 0.0), external("https://b.example", 1.0)]);
        assert_eq!(picker.selected_index(), 0);
        picker.select_up();
        assert_eq!(picker.selected_index(), 0);
        picker.select_down();
        picker.select_down();
        assert_eq!(picker.selected_index(), 1);
    }

    #[test]
    fn browser_gate_rejects_dangerous_scheme_without_spawning() {
        assert!(matches!(
            launch_external_browser("javascript:alert(1)"),
            Err(BrowserLaunchError::Security(SecurityError::ForbiddenScheme(_)))
        ));
    }
}
