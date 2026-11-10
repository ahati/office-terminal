use std::process::{Command, Stdio};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use kitty_pptx_core::{
    sanitize_hyperlink_url, HyperlinkTarget, SecurityError, SlideAction, SlideLink,
    SlideLinkIndex,
};
use ratatui::layout::Rect;

use crate::app::events::AppAction;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::widget::layout::{
    compute_slide_rect, DEFAULT_CELL_ASPECT_RATIO,
};

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
        self.slide_links
            .get(self.current_slide())
            .map(Vec::as_slice)
            .unwrap_or(&[])
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
    pub fn link_at_cell(&self, col: u16, row: u16) -> Option<SlideLink> {
        if self.slide_width_emu <= 0
            || self.slide_height_emu <= 0
            || self.layout_cols == 0
            || self.layout_rows == 0
        {
            return None;
        }
        let area = Rect::new(0, 0, self.layout_cols, self.layout_rows);
        let fitted = compute_slide_rect(area, self.aspect_ratio(), DEFAULT_CELL_ASPECT_RATIO);
        if fitted.width == 0
            || fitted.height == 0
            || col < fitted.x
            || row < fitted.y
            || col >= fitted.x.saturating_add(fitted.width)
            || row >= fitted.y.saturating_add(fitted.height)
        {
            return None;
        }
        let x_ratio = (col - fitted.x) as f32 / fitted.width as f32;
        let y_ratio = (row - fitted.y) as f32 / fitted.height as f32;
        let x_emu = x_ratio.clamp(0.0, 1.0) * self.slide_width_emu as f32;
        let y_emu = y_ratio.clamp(0.0, 1.0) * self.slide_height_emu as f32;
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
        LinkDispatch::EndShow => AppAction::Quit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitty_pptx_core::RectF32;

    fn external(url: &str, x: f32) -> SlideLink {
        SlideLink::new(
            HyperlinkTarget::ExternalUri(url.to_string()),
            RectF32::new(x, 0.0, 100.0, 100.0),
            None,
        )
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
