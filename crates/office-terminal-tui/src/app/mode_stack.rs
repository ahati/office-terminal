//! View/overlay mode stack — the single owner of presentation display mode.
//!
//! Replaces the former `mode: AppMode` + `previous_mode: AppMode` +
//! `sorter_return_mode: Option<AppMode>` triple on
//! [`super::state::PresentationState`], where the base view was reconstructed
//! from two fields through three re-implemented "return" paths.
//!
//! Model:
//! * [`BaseView`] — the persistent view underneath any overlay. The sorter is
//!   a base view (it used to be entered through overlay-style code, which is
//!   why the old model needed a dedicated remembered-return field).
//! * [`Overlay`] — a temporary modal or drawer drawn on top of the base view.
//! * [`ModeStack::sorter_return`] — the base captured when the sorter was
//!   entered; consumed when the sorter exits, and preserved across overlay
//!   excursions opened on top of the sorter (help/search over the grid).
//!
//! [`AppMode`] remains the public-facing enum (render dispatch, event routing
//! and all existing call sites match on it); this module is the internal
//! storage and owns every mode transition. The mapping is lossless in both
//! directions:
//!
//! | `AppMode`          | `(BaseView, Option<Overlay>)`   |
//! |-------------------|---------------------------------|
//! | `Fullscreen`      | `(Fullscreen, None)`             |
//! | `PresenterConsole`| `(PresenterConsole, None)`      |
//! | `Sorter`          | `(Sorter, None)`                 |
//! | `Reader`          | `(Reader, None)`                 |
//! | `HelpModal`       | `(base, Some(Help))`            |
//! | `CommentsDrawer`  | `(base, Some(Comments))`        |
//! | `WarningDrawer`   | `(base, Some(Warnings))`        |
//! | `TocSearchModal`  | `(base, Some(Toc))`             |
//! | `FindModal`       | `(base, Some(Find))`            |
//! | `LinkPickerModal` | `(base, Some(LinkPicker))`      |
//!
//! where `base` is whichever base view was active when the overlay opened.

use crate::app::mode::AppMode;

/// Persistent view drawn underneath any overlay.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum BaseView {
    /// Fullscreen slide view with minimal status line.
    #[default]
    Fullscreen,
    /// Split-pane presenter console.
    PresenterConsole,
    /// Thumbnail slide sorter grid.
    Sorter,
    /// Continuous document reader.
    Reader,
}

impl BaseView {
    /// The `AppMode` variant for this base view (total function).
    pub fn to_app_mode(self) -> AppMode {
        match self {
            BaseView::Fullscreen => AppMode::Fullscreen,
            BaseView::PresenterConsole => AppMode::PresenterConsole,
            BaseView::Sorter => AppMode::Sorter,
            BaseView::Reader => AppMode::Reader,
        }
    }

    /// The base view for a base-mode `AppMode` variant; `None` for overlays.
    /// Exhaustive over `AppMode`, so adding a variant forces an update here.
    pub fn from_app_mode(mode: AppMode) -> Option<Self> {
        match mode {
            AppMode::Fullscreen => Some(BaseView::Fullscreen),
            AppMode::PresenterConsole => Some(BaseView::PresenterConsole),
            AppMode::Sorter => Some(BaseView::Sorter),
            AppMode::Reader => Some(BaseView::Reader),
            AppMode::HelpModal
            | AppMode::CommentsDrawer
            | AppMode::WarningDrawer
            | AppMode::TocSearchModal
            | AppMode::FindModal
            | AppMode::LinkPickerModal => None,
        }
    }
}

/// Temporary modal or drawer rendered on top of the active base view.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    /// Help popup modal showing keyboard shortcuts.
    Help,
    /// Reviewer comments drawer for the current slide.
    Comments,
    /// Bottom drawer listing fidelity warnings.
    Warnings,
    /// Fuzzy TOC search modal popup.
    Toc,
    /// In-document text find modal (`Ctrl+F`).
    Find,
    /// Active-slide hyperlink picker modal (`o` / Ctrl+O).
    LinkPicker,
}

impl Overlay {
    /// The `AppMode` variant for this overlay (total function).
    pub fn to_app_mode(self) -> AppMode {
        match self {
            Overlay::Help => AppMode::HelpModal,
            Overlay::Comments => AppMode::CommentsDrawer,
            Overlay::Warnings => AppMode::WarningDrawer,
            Overlay::Toc => AppMode::TocSearchModal,
            Overlay::Find => AppMode::FindModal,
            Overlay::LinkPicker => AppMode::LinkPickerModal,
        }
    }

    /// The overlay for an overlay-mode `AppMode` variant; `None` for bases.
    /// Exhaustive over `AppMode`, so adding a variant forces an update here.
    pub fn from_app_mode(mode: AppMode) -> Option<Self> {
        match mode {
            AppMode::HelpModal => Some(Overlay::Help),
            AppMode::CommentsDrawer => Some(Overlay::Comments),
            AppMode::WarningDrawer => Some(Overlay::Warnings),
            AppMode::TocSearchModal => Some(Overlay::Toc),
            AppMode::FindModal => Some(Overlay::Find),
            AppMode::LinkPickerModal => Some(Overlay::LinkPicker),
            AppMode::Fullscreen | AppMode::PresenterConsole | AppMode::Sorter | AppMode::Reader => None,
        }
    }
}

/// One owner for the presenter's display-mode state.
///
/// `base` is the single source of truth for the persistent view: the old
/// `base_mode()` reconstruction from `mode` + `previous_mode` (with a
/// defensive `Fullscreen` fallback for a non-base `previous_mode` that could
/// never occur) is now `base.to_app_mode()`.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ModeStack {
    /// Persistent view underneath any overlay.
    pub base: BaseView,
    /// Overlay modal/drawer currently drawn on top of the base view.
    pub overlay: Option<Overlay>,
    /// Base captured when the sorter was entered. Taken on sorter exit;
    /// survives overlays opened on top of the sorter so the grid returns to
    /// the view that opened it (Fullscreen, presenter console or reader).
    /// Never `Some(Sorter)` and, in the sorter, never `None`.
    pub sorter_return: Option<BaseView>,
}

impl ModeStack {
    /// The active display mode: the overlay when one is open, else the base.
    pub fn mode(&self) -> AppMode {
        self.overlay
            .map(Overlay::to_app_mode)
            .unwrap_or_else(|| self.base.to_app_mode())
    }

    /// The persistent view underneath any overlay.
    pub fn base_mode(&self) -> AppMode {
        self.base.to_app_mode()
    }

    /// The base view remembered for the current overlay/sorter session —
    /// the value the former `previous_mode` latch held in every state any
    /// caller could observe (overlay over a plain base: the base itself;
    /// inside the sorter: the captured return target).
    pub fn previous_base(&self) -> BaseView {
        if self.overlay.is_none() && self.base == BaseView::Sorter {
            self.sorter_return.unwrap_or(self.base)
        } else {
            self.base
        }
    }

    /// Make `mode` active: switch the base view, or open/close an overlay.
    ///
    /// Base targets replace the base view (entering the sorter captures the
    /// return target); overlay targets swap the overlay without disturbing
    /// the base underneath.
    pub fn set_app_mode(&mut self, mode: AppMode) {
        if let Some(overlay) = Overlay::from_app_mode(mode) {
            self.open_overlay(overlay);
        } else if let Some(base) = BaseView::from_app_mode(mode) {
            self.switch_base(base);
        }
    }

    /// Switch the persistent base view, dropping any overlay. Entering the
    /// sorter captures `sorter_return` (kept if one is already remembered,
    /// matching the sorter's re-entry contract).
    pub fn switch_base(&mut self, base: BaseView) {
        if base == BaseView::Sorter && self.base != BaseView::Sorter {
            self.sorter_return = self.sorter_return.or(Some(self.base));
        }
        self.base = base;
        self.overlay = None;
    }

    /// Open `overlay` on top of the current base view (overlay-from-overlay
    /// swaps the overlay and keeps the original base).
    pub fn open_overlay(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
    }

    /// Dismiss the active overlay, revealing the base view.
    pub fn close_overlay(&mut self) {
        self.overlay = None;
    }

    /// Enter the sorter grid. Captures the return target on first entry
    /// (preserving one already captured), and dismisses any overlay that
    /// was open over the grid (re-entering the sorter from help/search).
    pub fn open_sorter(&mut self) {
        if self.base == BaseView::Sorter && self.overlay.is_none() {
            return;
        }
        if self.base != BaseView::Sorter {
            self.sorter_return = self.sorter_return.or(Some(self.base));
        }
        self.base = BaseView::Sorter;
        self.overlay = None;
    }

    /// Leave the sorter grid, returning to the captured base view.
    /// Returns the restored base.
    pub fn exit_sorter(&mut self) -> BaseView {
        // Unreachable while the sorter is active (every entry path captures a
        // return target), kept as the old code's defensive fallback.
        let target = self.sorter_return.take().unwrap_or(BaseView::Fullscreen);
        self.base = target;
        self.overlay = None;
        target
    }
}
