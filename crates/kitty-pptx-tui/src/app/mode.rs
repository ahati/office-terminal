/// Application display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    /// Fullscreen slide view with minimal status line (or hidden until toggled).
    Fullscreen,
    /// Split-pane presenter console (current slide, next preview, notes, timers, gauge).
    PresenterConsole,
    /// Help popup modal showing keyboard shortcuts.
    HelpModal,
    /// Thumbnail slide sorter grid mode.
    Sorter,
    /// Reviewer comments drawer for current slide.
    CommentsDrawer,
    /// Bottom drawer listing fidelity warnings.
    WarningDrawer,
    /// Fuzzy TOC search modal popup.
    TocSearchModal,
}

/// Backwards-compatible alias for `AppMode`.
pub type PresenterMode = AppMode;

#[allow(non_upper_case_globals)]
impl AppMode {
    pub const Normal: AppMode = AppMode::Fullscreen;
    pub const ThumbnailGrid: AppMode = AppMode::Sorter;

    /// Whether this mode is a temporary overlay drawn on top of a base view.
    pub fn is_overlay(&self) -> bool {
        matches!(
            self,
            AppMode::HelpModal
                | AppMode::CommentsDrawer
                | AppMode::WarningDrawer
                | AppMode::TocSearchModal
        )
    }

    /// Whether this mode is a persistent base view.
    pub fn is_base_mode(&self) -> bool {
        !self.is_overlay()
    }
}

/// Blank screen mode for pausing talks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlankMode {
    None,
    Black,
    White,
}
