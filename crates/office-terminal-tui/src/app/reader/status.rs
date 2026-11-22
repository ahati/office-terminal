//! The reader status bar (PRD 14a §9).
//!
//! Everything on the bar is a segment — the optional `cover:on` joins
//! like any other instead of smuggling its own `·` inside the value, and
//! the key hints are themselves segments, so the whole line shares one
//! separator policy owned by the single joiner.

use crate::app::reader_state::ReaderState;
use crate::app::reader_types::{ReaderFitMode, ReaderLayout, ReaderSlotPlacement};

/// Separator between status segments — the one policy for the whole bar
/// (PRD 14a §9 statusbar).
const STATUS_SEGMENT_SEPARATOR: &str = " · ";

/// The status bar's key hints, as `key:action` segments sourced from the
/// keymap (PRD 14a §10).
const KEY_HINTS: &[(&str, &str)] = &[("?", "help"), ("q", "quit")];

impl ReaderState {
    /// Status line for the reader viewport.
    ///
    /// `placements` and `columns` are the frame's layout output (recorded
    /// into [`crate::app::bindings::FrameBindings`] by the reader view); the
    /// label reports the primary row so preview rows never leak into it.
    pub fn status_line(
        &self,
        filename: &str,
        placements: &[ReaderSlotPlacement],
        columns: usize,
    ) -> String {
        let primary_placements: Vec<&ReaderSlotPlacement> =
            placements.iter().filter(|p| p.is_primary).collect();
        let source_placements = if primary_placements.is_empty() {
            placements.iter().collect::<Vec<_>>()
        } else {
            primary_placements
        };
        let mut active_pages: Vec<usize> = source_placements.iter().map(|p| p.page).collect();
        active_pages.sort_unstable();
        active_pages.dedup();

        let page_label = if active_pages.len() > 1 {
            format!(
                "spread {}-{}",
                self.page_label(active_pages[0]),
                self.page_label(active_pages[active_pages.len() - 1])
            )
        } else if let Some(&p) = active_pages.first() {
            format!("p.{}", self.page_label(p))
        } else {
            let vis = self.visible_pages();
            if columns > 1 && vis.len() > 1 {
                format!("spread {}-{}", self.page_label(vis[0]), self.page_label(vis[1]))
            } else if !vis.is_empty() {
                format!("p.{}", self.page_label(vis[0]))
            } else {
                "p.0".to_string()
            }
        };

        let focused_page = self.focused_page();
        let view = self.page_view(focused_page);
        let fit_label = match view.fit {
            ReaderFitMode::FitWidth => "width",
            ReaderFitMode::FitPage => "page",
        };
        let zoom_pct = (view.zoom * 100.0).round() as u32;

        // Segments as data: the joiner owns the separator, so the optional
        // cover segment is a plain value, not a `·`-prefixed string.
        let mut segments = vec![
            filename.to_string(),
            format!("{page_label}/{}", self.total_pages()),
            format!("fit:{fit_label} {zoom_pct}%"),
        ];
        if self.layout == ReaderLayout::Spread && self.cover_alone {
            segments.push("cover:on".to_string());
        }
        segments.extend(
            KEY_HINTS
                .iter()
                .map(|(key, action)| format!("{key}:{action}")),
        );
        segments.join(STATUS_SEGMENT_SEPARATOR)
    }
}
