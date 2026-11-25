//! Shared harness for the `text-flow-a` parity ports.
//!
//! Every probe uses the reference spec's own stub `measureText` (20 px per code
//! point, 20 px space for the `pptx-tab-wrap`/`pptx-multi-tab`/`rtl-tab-stops`
//! specs) and reconstructs drawn pieces from a broken line's item widths the way
//! `renderer.ts:5885-5925` walks its segments.
//!
//! Units: the reference specs build canvas px from EMU through `SCALE = 1/12700`
//! (1 pt → 1 px); every kernel here is linear and unit-agnostic, so these probes
//! stay in px and pass the specs' numbers verbatim.

#![allow(dead_code)]

use office_terminal_core::text::justify::{justify_line, JustifyMode, JustifyPlan};
use office_terminal_core::text::linebreak::{break_paragraph, BreakItem, BreakOptions, BrokenLine, LineItem};
use office_terminal_core::text::tabs::resolve_tab_widths;
use office_terminal_core::text::{StyleId, TabAlign, TabStop, TextMeasure};

pub const S: StyleId = StyleId(0);
pub const B: StyleId = StyleId(1);

/// `pptx-tab-wrap.test.ts` / `pptx-multi-tab.test.ts` / `rtl-tab-stops.test.ts`
/// stub: `FONT_PX = 20` per code point (a space is 20 too).
pub struct Cell20;

impl TextMeasure for Cell20 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 20.0
    }
    fn space(&self, _: StyleId) -> f32 {
        20.0
    }
}

pub static CELLS: Cell20 = Cell20;

pub fn text_item(text: &str) -> BreakItem {
    BreakItem::Text {
        text: text.to_string(),
        style: S,
    }
}

pub fn text_item_styled(text: &str, style: StyleId) -> BreakItem {
    BreakItem::Text {
        text: text.to_string(),
        style,
    }
}

pub fn line_from_items(items: &[LineItem]) -> LineItem {
    items[0].clone()
}

pub fn item(text: &str, width: f32) -> LineItem {
    LineItem::Text {
        text: text.to_string(),
        style: S,
        width,
    }
}

pub fn tab_item() -> LineItem {
    LineItem::Tab { width: 0.0, style: S }
}

pub fn stop(pos: f32, align: TabAlign) -> TabStop {
    TabStop { pos, align }
}

pub fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-3,
        "{what}: expected {expected}, got {actual}"
    );
}

pub fn width_of(item: &LineItem) -> f32 {
    match item {
        LineItem::Text { width, .. } | LineItem::Tab { width, .. } | LineItem::Object { width, .. } => *width,
    }
}

pub fn plan(
    items: &[LineItem],
    avail: f32,
    natural: f32,
    mode: JustifyMode,
    last_line: bool,
    ends_with_break: bool,
) -> JustifyPlan {
    justify_line(items, avail, natural, mode, last_line, ends_with_break)
        .unwrap_or_else(|| panic!("expected a justification plan for {items:?}"))
}

/// One drawn piece of a broken line: `(text, pen x, measured width)`.
#[derive(Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub x: f32,
    pub width: f32,
}

/// Reconstruct a line's drawn pieces from its item widths, the way the reference
/// draw loop walks `segments` (`renderer.ts:5885-5925`): the pen starts at the
/// leading origin (`line_x`, which carries the first-line indent) and
/// accumulates every item's resolved advance.
pub fn pieces_at(line: &BrokenLine, line_x: f32) -> Vec<Piece> {
    let mut pen = line_x;
    let mut out = Vec::new();
    for item in &line.items {
        match item {
            LineItem::Text { text, width, .. } => {
                out.push(Piece {
                    text: text.clone(),
                    x: pen,
                    width: *width,
                });
                pen += width;
            }
            LineItem::Tab { width, .. } => pen += width,
            LineItem::Object { width, .. } => pen += width,
        }
    }
    out
}

pub fn pieces(line: &BrokenLine) -> Vec<Piece> {
    pieces_at(line, 0.0)
}

/// The concatenated text of a line, minus the tab markers.
pub fn line_text(line: &BrokenLine) -> String {
    line.items
        .iter()
        .map(|item| match item {
            LineItem::Text { text, .. } => text.as_str(),
            LineItem::Tab { .. } => "",
            LineItem::Object { .. } => "",
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

pub fn wrap(items: &[BreakItem], width: f32, stops: &[TabStop], grid: f32) -> Vec<BrokenLine> {
    break_paragraph(
        items,
        &BreakOptions {
            max_width: width,
            first_line_indent: 0.0,
            tab_stops: stops,
            default_tab_size: grid,
            east_asian_line_break: true,
            measurer: &CELLS,
        },
    )
}

pub fn wrap_indented(items: &[BreakItem], width: f32, indent: f32, stops: &[TabStop]) -> Vec<BrokenLine> {
    break_paragraph(
        items,
        &BreakOptions {
            max_width: width,
            first_line_indent: indent,
            tab_stops: stops,
            default_tab_size: 0.0,
            east_asian_line_break: true,
            measurer: &CELLS,
        },
    )
}
/// `tab-layout.test.ts:23`: a non-tab cell of a given width.
pub fn cell(width: f32) -> LineItem {
    item("cell", width)
}

/// Resolve one line's tab gaps and report the resulting widths in item order.
pub fn tab_widths(
    items: &mut [LineItem],
    stops: &[TabStop],
    start_pen: f32,
    limit: f32,
    no_stop_gap: f32,
    def_tab_sz: f32,
) -> Vec<f32> {
    resolve_tab_widths(items, stops, start_pen, limit, no_stop_gap, def_tab_sz);
    items.iter().map(width_of).collect()
}
