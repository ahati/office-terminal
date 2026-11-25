//! Inline tab-stop resolution — 1:1 port of
//! `packages/core/src/drawingml-text/tab.ts:20-59` / `packages/pptx/src/tab-layout.ts:38-97`
//! (reference revision 1f5c6e1d).
//!
//! ECMA-376 §21.1.2.1.x: a tab advances the pen to the nearest explicit
//! `a:pPr/a:tabLst/a:tab` stop strictly past it, placing the following cell (the
//! content up to the next tab or the line end) so that it starts on (`l`),
//! centres on (`ctr`), or ends on (`r`/`dec`) that stop. `dec` aligns like `r`:
//! true decimal-point splitting is not implemented, the same approximation the
//! reference uses.
//!
//! With no reachable explicit stop, §21.1.2.1.1 / §21.1.2.2.7 `defTabSz` (a
//! LEFT stop at the next grid multiple strictly past the pen) applies; when
//! `defTabSz` is 0 the tab degrades to `no_stop_gap` (a space width).
//!
//! Two clamps mirror the reference: a cell whose far edge would cross `limit`
//! (the trailing text-inset edge) is pinned to it, and a tab never moves the pen
//! backwards. `limit` is `f32::INFINITY` during wrapping so overflow stays
//! visible to the breaker, while the paint pass passes the finite edge.

use super::linebreak::LineItem;
use super::measure::{TabAlign, TabStop};

/// Width a [`LineItem`] currently occupies.
fn item_width(item: &LineItem) -> f32 {
    match item {
        LineItem::Text { width, .. } | LineItem::Tab { width, .. } | LineItem::Object { width, .. } => *width,
    }
}

/// `resolveDrawingMlTabWidths` (`tab.ts:20-59`): rewrite every
/// [`LineItem::Tab`]'s `width` for a line that starts at `start_pen`
/// (the leading / first-line indent).
///
/// Non-tab items keep their width. The result is in logical reading order; a
/// bidi pass reorders painted cells, so each logical gap is also its visual gap.
///
/// - `stops` — `a:tabLst` positions in reading-frame px, any order.
/// - `limit` — trailing text edge, or `f32::INFINITY` for the natural extent.
/// - `no_stop_gap` — fallback advance when no stop and `def_tab_sz == 0`.
/// - `def_tab_sz` — `a:pPr@defTabSz`; 0 disables the default grid.
pub fn resolve_tab_widths(
    items: &mut [LineItem],
    stops: &[TabStop],
    start_pen: f32,
    limit: f32,
    no_stop_gap: f32,
    def_tab_sz: f32,
) {
    let mut pen = start_pen;
    for index in 0..items.len() {
        if !matches!(items[index], LineItem::Tab { .. }) {
            pen += item_width(&items[index]);
            continue;
        }
        // Nearest explicit stop STRICTLY past the pen (`tab.ts:43-47`).
        let mut stop: Option<TabStop> = None;
        for candidate in stops {
            if candidate.pos > pen && stop.is_none_or(|current| candidate.pos < current.pos) {
                stop = Some(*candidate);
            }
        }
        let Some(stop) = stop else {
            if def_tab_sz > 0.0 {
                // §21.1.2.1.1 default tab grid: `(floor(pen / defTabSz) + 1) *
                // defTabSz`, always strictly past the pen even when the pen sits
                // exactly on a grid line (`tab.ts:48-52`).
                let pos = ((pen / def_tab_sz).floor() + 1.0) * def_tab_sz;
                place_tab(items, index, pos, TabAlign::Left, limit, &mut pen);
            } else if let LineItem::Tab { width, .. } = &mut items[index] {
                // Legacy degradation to a space-width gap (`tab.ts:53-57`).
                *width = no_stop_gap;
                pen += no_stop_gap;
            }
            continue;
        };
        place_tab(items, index, stop.pos, stop.align, limit, &mut pen);
    }
}

/// Seat one tab on `stop_pos`: align the following cell on the stop, clamp the
/// cell to `limit`, then never move the pen backwards (`tab.ts:60-72`).
fn place_tab(items: &mut [LineItem], index: usize, stop_pos: f32, align: TabAlign, limit: f32, pen: &mut f32) {
    // The following cell: content up to the next tab or the line end.
    let following: f32 = items[index + 1..]
        .iter()
        .take_while(|item| !matches!(item, LineItem::Tab { .. }))
        .map(item_width)
        .sum();
    let fraction = match align {
        TabAlign::Left => 0.0,
        TabAlign::Center => 0.5,
        TabAlign::Right | TabAlign::Decimal => 1.0,
    };
    let mut target = stop_pos - following * fraction;
    if target + following > limit {
        target = limit - following;
    }
    if target < *pen {
        target = *pen;
    }
    if let LineItem::Tab { width, .. } = &mut items[index] {
        *width = target - *pen;
    }
    *pen = target;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::measure::StyleId;

    fn cell(width: f32) -> LineItem {
        LineItem::Text {
            text: String::from("cell"),
            style: StyleId(0),
            width,
        }
    }

    fn tab() -> LineItem {
        LineItem::Tab {
            width: 0.0,
            style: StyleId(0),
        }
    }

    fn stop(pos: f32, align: TabAlign) -> TabStop {
        TabStop { pos, align }
    }

    /// Resolve and return one width per item, in logical order.
    fn widths(
        mut items: Vec<LineItem>,
        stops: &[TabStop],
        start_pen: f32,
        limit: f32,
        no_stop_gap: f32,
        def_tab_sz: f32,
    ) -> Vec<f32> {
        resolve_tab_widths(&mut items, stops, start_pen, limit, no_stop_gap, def_tab_sz);
        items.iter().map(item_width).collect()
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-4, "expected {expected}, got {actual}");
    }

    /// `tab-layout.test.ts:26-36`: N tab cells, right-aligned on 200 and 400.
    #[test]
    fn supports_n_tab_cells_per_line() {
        let out = widths(
            vec![cell(50.0), tab(), cell(40.0), tab(), cell(40.0)],
            &[stop(200.0, TabAlign::Right), stop(400.0, TabAlign::Right)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [50.0, 110.0, 40.0, 160.0, 40.0]);
        close(50.0 + out[1] + 40.0, 200.0);
        close(50.0 + out[1] + 40.0 + out[3] + 40.0, 400.0);
    }

    /// `tab-layout.test.ts:38-44`: a centre stop centres the following cell.
    #[test]
    fn centre_stop_centres_the_cell() {
        let out = widths(
            vec![tab(), cell(40.0)],
            &[stop(400.0, TabAlign::Center)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [380.0, 40.0]);
        close(out[0] + out[1] / 2.0, 400.0);
    }

    /// `tab-layout.test.ts:46-50`: a left stop materialises the gap.
    #[test]
    fn left_stop_materialises_the_gap() {
        let out = widths(
            vec![tab(), cell(30.0)],
            &[stop(100.0, TabAlign::Left)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [100.0, 30.0]);
    }

    /// `tab-layout.test.ts:52-56`: `dec` aligns like `r`.
    #[test]
    fn decimal_stop_aligns_like_right() {
        let out = widths(
            vec![tab(), cell(40.0)],
            &[stop(400.0, TabAlign::Decimal)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [360.0, 40.0]);
        assert_eq!(
            out,
            widths(
                vec![tab(), cell(40.0)],
                &[stop(400.0, TabAlign::Right)],
                0.0,
                600.0,
                0.0,
                0.0,
            )
        );
    }

    /// `tab-layout.test.ts:58-62`: no reachable stop collapses to the fallback.
    #[test]
    fn unreachable_stop_degrades_to_the_fallback_gap() {
        let out = widths(vec![tab(), cell(30.0)], &[], 0.0, 600.0, 20.0, 0.0);
        assert_eq!(out, [20.0, 30.0]);
    }

    /// `tab-layout.test.ts:64-69`: a cell that would overflow `limit` is pinned
    /// so its far edge sits on the trailing text edge.
    #[test]
    fn overflow_clamps_the_cell_to_the_limit() {
        let out = widths(
            vec![tab(), cell(40.0)],
            &[stop(700.0, TabAlign::Right)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [560.0, 40.0]);
        close(out[0] + out[1], 600.0);
    }

    /// `tab-layout.test.ts:71-79`: a stop behind the pen collapses the tab; the
    /// pen never moves backwards.
    #[test]
    fn never_moves_the_pen_backwards() {
        let out = widths(
            vec![tab(), cell(20.0), tab(), cell(40.0)],
            &[stop(100.0, TabAlign::Left), stop(140.0, TabAlign::Right)],
            50.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [50.0, 20.0, 0.0, 40.0]);
    }

    /// `tab-layout.test.ts:81-88`: the nearest stop strictly past the pen wins,
    /// regardless of the declared order.
    #[test]
    fn selects_the_nearest_stop_in_any_order() {
        let out = widths(
            vec![tab(), cell(10.0), tab(), cell(10.0)],
            &[stop(400.0, TabAlign::Left), stop(200.0, TabAlign::Left)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(out, [200.0, 10.0, 190.0, 10.0]);
    }

    /// `tab-layout.test.ts:90-95`: `defTabSz` grid with no explicit stops.
    #[test]
    fn default_grid_jumps_to_the_next_line() {
        let out = widths(vec![tab(), cell(30.0)], &[], 0.0, 600.0, 0.0, 100.0);
        assert_eq!(out, [100.0, 30.0]);
    }

    /// `tab-layout.test.ts:97-106`: the grid resumes past the last explicit stop.
    #[test]
    fn default_grid_resumes_past_the_last_explicit_stop() {
        let out = widths(
            vec![tab(), cell(30.0), tab(), cell(20.0)],
            &[stop(120.0, TabAlign::Left)],
            0.0,
            600.0,
            0.0,
            100.0,
        );
        assert_eq!(out, [120.0, 30.0, 50.0, 20.0]);
    }

    /// `tab-layout.test.ts:108-113`: a pen exactly on a grid line advances to
    /// the NEXT line, never staying put.
    #[test]
    fn default_grid_advances_when_the_pen_sits_on_a_line() {
        let out = widths(vec![tab(), cell(10.0)], &[], 100.0, 600.0, 0.0, 100.0);
        assert_eq!(out, [100.0, 10.0]);
    }

    /// An explicit stop wins over the `defTabSz` grid (`tab-stop-spc.test.ts:59`).
    #[test]
    fn explicit_stop_wins_over_the_default_grid() {
        let out = widths(
            vec![tab(), cell(30.0)],
            &[stop(120.0, TabAlign::Left)],
            0.0,
            600.0,
            0.0,
            100.0,
        );
        assert_eq!(out, [120.0, 30.0]);
    }

    /// A tab at the line end has no following cell yet still advances to the
    /// next stop (or grid line / fallback gap).
    #[test]
    fn trailing_tab_still_advances_to_the_next_stop() {
        let explicit = widths(
            vec![cell(50.0), tab()],
            &[stop(200.0, TabAlign::Left)],
            0.0,
            600.0,
            0.0,
            0.0,
        );
        assert_eq!(explicit, [50.0, 150.0]);

        let grid = widths(vec![tab()], &[], 0.0, 600.0, 0.0, 100.0);
        assert_eq!(grid, [100.0]);

        let fallback = widths(vec![tab()], &[], 0.0, 600.0, 20.0, 0.0);
        assert_eq!(fallback, [20.0]);
    }

    /// The wrap pass passes `f32::INFINITY` so the natural extent (and hence
    /// overflow) stays visible to the breaker; the paint pass clamps.
    #[test]
    fn infinite_limit_keeps_the_unclamped_extent() {
        let out = widths(
            vec![tab(), cell(40.0)],
            &[stop(700.0, TabAlign::Right)],
            0.0,
            f32::INFINITY,
            0.0,
            0.0,
        );
        assert_eq!(out, [660.0, 40.0]);
    }

    /// Non-tab widths are never rewritten.
    #[test]
    fn keeps_content_widths_untouched() {
        let mut items = vec![cell(50.0), tab(), cell(40.0)];
        resolve_tab_widths(&mut items, &[stop(200.0, TabAlign::Right)], 0.0, 600.0, 0.0, 0.0);
        assert_eq!(item_width(&items[0]), 50.0);
        assert_eq!(item_width(&items[2]), 40.0);
        assert_eq!(item_width(&items[1]), 110.0);
    }
}
