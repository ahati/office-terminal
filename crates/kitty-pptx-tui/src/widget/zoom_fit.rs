//! Zoom-aware slide placement geometry.
//!
//! The zoom contract splits in two halves: the *factor* lives on
//! [`crate::app::nav_state::NavState`] (ladder, clamp, change gate) and the
//! *placement* lives here — a pure function from `(area, aspect, zoom, pan)`
//! to the cell rect an image must be placed into plus, when the zoomed rect
//! overflows the viewport, the normalized window of the slide that is
//! actually visible.
//!
//! This mirrors the reader panes' placement model (base fit × zoom, then a
//! sub-rect crop of one canonical raster) and is consumed identically by the
//! widget (which clears letterbox cells around the placement) and
//! [`crate::app::bindings::FrameBindings`] (which records it for the graphics
//! syncer), so the two can never disagree about where a zoomed slide sits.

use ratatui::layout::Rect;

use super::layout::compute_slide_rect;

/// Visible window of the slide in normalized slide coordinates (`0..=1`).
///
/// `x`/`y` is the top-left corner of the window, `w`/`h` its extent. A crop
/// of `x=0, y=0, w=1, h=1` covers the whole slide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedCrop {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormalizedCrop {
    /// Raster-pixel window of this crop for a `width × height` bitmap.
    ///
    /// Rounds to whole pixels and clamps inside the bitmap so a placement can
    /// never reference pixels outside the transmitted raster (the same
    /// discipline the reader pane layout applies to its crops).
    pub fn to_pixel_rect(&self, width: u32, height: u32) -> kitty_pptx_display::PixelRect {
        let w = width.max(1);
        let h = height.max(1);
        // Clamp the origin to leave room for at least one source row/column,
        // so even a degenerate crop yields a placeable 1px window.
        let x = ((self.x.clamp(0.0, 1.0) * w as f32).round() as u32).min(w - 1);
        let y = ((self.y.clamp(0.0, 1.0) * h as f32).round() as u32).min(h - 1);
        let sw = (((self.w.clamp(0.0, 1.0) * w as f32).round() as u32).max(1)).min(w - x);
        let sh = (((self.h.clamp(0.0, 1.0) * h as f32).round() as u32).max(1)).min(h - y);
        kitty_pptx_display::PixelRect {
            x,
            y,
            width: sw,
            height: sh,
        }
    }
}

/// Where one zoomed slide is placed: the cell rect the image occupies, the
/// crop window when only part of the zoomed rect fits, and the pan bounds the
/// input side needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomFit {
    /// Cell rect to place the image into; always fully inside `area`.
    pub rect: Rect,
    /// `None`: the whole slide maps onto `rect` (the zoomed rect fits).
    /// `Some`: the visible window of the zoomed rect maps onto `rect`.
    pub crop: Option<NormalizedCrop>,
    /// Cell-space overflow of the zoomed rect (`0` per axis that fits).
    /// Pan offsets are clamped to `±overflow/2` around the centered start.
    pub overflow_cols: f32,
    pub overflow_rows: f32,
}

/// Fits the slide into `area` at `zoom` with a centered `pan` offset.
///
/// The base fit is [`compute_slide_rect`]'s letterboxed rect; `zoom` scales
/// it about its center. While the zoomed rect fits inside `area` it is placed
/// whole (letterbox bars around it). Once an axis overflows, the placed rect
/// becomes the viewport intersection and the visible window is reported as a
/// [`NormalizedCrop`] for the graphics syncer to place as a raster sub-rect.
///
/// `pan_x`/`pan_y` are signed cell-unit offsets from the centered position,
/// clamped to the per-axis overflow (so extreme pan values, or pan applied
/// while the view changed, can never push content off-screen or leave gaps).
pub fn compute_zoomed_slide_fit(
    area: Rect,
    aspect_ratio: f32,
    cell_aspect_ratio: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> ZoomFit {
    if area.width == 0 || area.height == 0 {
        return ZoomFit {
            rect: Rect {
                x: area.x,
                y: area.y,
                width: 0,
                height: 0,
            },
            crop: None,
            overflow_cols: 0.0,
            overflow_rows: 0.0,
        };
    }

    // Sanitize the factor: NavState clamps into [0.1, 4.0], but this is pure
    // geometry and must stay total for any caller.
    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        1.0
    };

    let base = compute_slide_rect(area, aspect_ratio, cell_aspect_ratio);
    let zoomed_w = base.width as f32 * zoom;
    let zoomed_h = base.height as f32 * zoom;

    let overflow_cols = (zoomed_w - area.width as f32).max(0.0);
    let overflow_rows = (zoomed_h - area.height as f32).max(0.0);

    // Symmetric pan range around the centered position.
    let pan_x = pan_x.clamp(-overflow_cols / 2.0, overflow_cols / 2.0);
    let pan_y = pan_y.clamp(-overflow_rows / 2.0, overflow_rows / 2.0);

    // Zoomed rect origin in cell space; may lie outside `area` per axis.
    // Positive pan moves the *view* right/down, so the zoomed rect itself
    // slides left/up: `crop.x` grows with `pan_x`.
    let origin_x = area.x as f32 + (area.width as f32 - zoomed_w) / 2.0 - pan_x;
    let origin_y = area.y as f32 + (area.height as f32 - zoomed_h) / 2.0 - pan_y;

    // No axis overflows: the zoomed rect fits whole. Derive it from the base
    // rect (scaled about the base's own center) so zoom 1 is bit-identical to
    // the plain letterbox fit — a re-derived intersection would differ by the
    // base fit's integer-division offset rounding.
    if overflow_cols == 0.0 && overflow_rows == 0.0 {
        // Pan is clamped to zero without overflow, so the rect is centered.
        let x0 = base.x as f32 + (base.width as f32 - zoomed_w) / 2.0;
        let y0 = base.y as f32 + (base.height as f32 - zoomed_h) / 2.0;
        let rect = Rect {
            x: x0.round() as u16,
            y: y0.round() as u16,
            width: (zoomed_w.round() as u16).clamp(1, area.width.max(1)),
            height: (zoomed_h.round() as u16).clamp(1, area.height.max(1)),
        };
        return ZoomFit {
            rect,
            crop: None,
            overflow_cols: 0.0,
            overflow_rows: 0.0,
        };
    }

    // An axis overflows: the placed rect is the viewport intersection of the
    // zoomed rect, and the visible window becomes a normalized crop.
    let vis_x0 = origin_x.max(area.x as f32);
    let vis_y0 = origin_y.max(area.y as f32);
    let vis_x1 = (origin_x + zoomed_w).min(area.right() as f32);
    let vis_y1 = (origin_y + zoomed_h).min(area.bottom() as f32);

    let mut rect = Rect {
        x: vis_x0.round() as u16,
        y: vis_y0.round() as u16,
        width: 0,
        height: 0,
    };
    rect.width = ((vis_x1 - vis_x0).round() as u16)
        .clamp(1, area.right().saturating_sub(rect.x).max(1));
    rect.height = ((vis_y1 - vis_y0).round() as u16)
        .clamp(1, area.bottom().saturating_sub(rect.y).max(1));

    ZoomFit {
        rect,
        crop: Some(NormalizedCrop {
            x: ((vis_x0 - origin_x) / zoomed_w).clamp(0.0, 1.0),
            y: ((vis_y0 - origin_y) / zoomed_h).clamp(0.0, 1.0),
            w: ((vis_x1 - vis_x0) / zoomed_w).clamp(0.0, 1.0),
            h: ((vis_y1 - vis_y0) / zoomed_h).clamp(0.0, 1.0),
        }),
        overflow_cols,
        overflow_rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: f32 = 0.5;
    const SLIDE: f32 = 16.0 / 9.0;

    fn fit(area: Rect, zoom: f32, pan: (f32, f32)) -> ZoomFit {
        compute_zoomed_slide_fit(area, SLIDE, CELL, zoom, pan.0, pan.1)
    }

    #[test]
    fn zoom_one_matches_the_plain_letterbox_fit() {
        let area = Rect::new(0, 0, 80, 24);
        let plain = compute_slide_rect(area, SLIDE, CELL);
        let zoomed = fit(area, 1.0, (0.0, 0.0));
        assert_eq!(zoomed.rect, plain);
        assert_eq!(zoomed.crop, None);
        assert_eq!(zoomed.overflow_cols, 0.0);
        assert_eq!(zoomed.overflow_rows, 0.0);
    }

    #[test]
    fn zoom_out_shrinks_centered_without_a_crop() {
        let area = Rect::new(0, 0, 80, 24);
        let z = fit(area, 0.5, (99.0, 99.0)); // pan is meaningless unzoomed
        assert_eq!(z.rect.width, 40);
        assert_eq!(z.rect.height, 12);
        assert_eq!(z.rect.x, 20);
        assert_eq!(z.rect.y, 6);
        assert_eq!(z.crop, None, "whole slide stays visible when zoomed out");
        assert_eq!(z.overflow_cols, 0.0);
    }

    #[test]
    fn zoom_in_crops_around_the_center_by_default() {
        // 80 cols x 24 rows: plain fit is 80 x 23 (letterboxed 1 row).
        let area = Rect::new(0, 0, 80, 24);
        let z = fit(area, 2.0, (0.0, 0.0));
        assert_eq!(z.rect, area, "zoomed rect covers the whole viewport");
        assert_eq!(z.overflow_cols, 80.0);
        assert!((z.overflow_rows - 22.0).abs() < 0.01);

        let crop = z.crop.expect("overflow requires a crop");
        assert!((crop.w - 0.5).abs() < 1e-4, "half the slide width");
        assert!((crop.h - 24.0 / 46.0).abs() < 1e-3, "visible height fraction");
        assert!((crop.x - 0.25).abs() < 1e-4, "centered: equal cut both sides");
        assert!((crop.y - (46.0 - 24.0) / 2.0 / 46.0).abs() < 1e-3);
    }

    #[test]
    fn pan_shifts_the_crop_window_and_clamps_at_the_edges() {
        let area = Rect::new(0, 0, 80, 24);
        let center = fit(area, 2.0, (0.0, 0.0)).crop.expect("crop");

        let right = fit(area, 2.0, (40.0, 0.0)).crop.expect("crop"); // +overflow/2
        assert!((right.x - 0.5).abs() < 1e-4, "panned fully right");
        assert!((right.w - center.w).abs() < 1e-6);

        // Clamped: pushing past the bound pins at the same window.
        let past = fit(area, 2.0, (500.0, 0.0)).crop.expect("crop");
        assert!((past.x - right.x).abs() < 1e-6);

        let left = fit(area, 2.0, (-40.0, 0.0)).crop.expect("crop");
        assert!(left.x < 1e-4, "panned fully left shows the slide start");
    }

    #[test]
    fn partial_overflow_crops_only_the_overflowing_axis() {
        // Pillarboxed base (narrow slide in a wide area) overflows rows only.
        let area = Rect::new(0, 0, 120, 10);
        let z = compute_zoomed_slide_fit(area, SLIDE, CELL, 1.5, 0.0, 0.0);
        // plain fit: 120/3.5556 -> 34 rows > 10, so height-fit: 10 rows, 36 cols
        let crop = z.crop.expect("row overflow requires a crop");
        assert_eq!(z.rect.width, 54, "zoomed width fits: 36 * 1.5 whole");
        assert!((crop.x - 0.0).abs() < 1e-6);
        assert!((crop.w - 1.0).abs() < 1e-6, "full width visible");
        assert!(crop.h < 1.0, "rows cropped");
        assert_eq!(z.rect.height, 10);
    }

    #[test]
    fn degenerate_area_and_factor_stay_total() {
        let z = compute_zoomed_slide_fit(Rect::new(3, 3, 0, 10), SLIDE, CELL, 2.0, 0.0, 0.0);
        assert_eq!(z.rect.width, 0);
        assert_eq!(z.crop, None);

        let z = compute_zoomed_slide_fit(Rect::new(0, 0, 80, 24), SLIDE, CELL, f32::NAN, 0.0, 0.0);
        assert_eq!(
            z.rect,
            compute_slide_rect(Rect::new(0, 0, 80, 24), SLIDE, CELL),
            "NaN factor degrades to the plain fit"
        );
    }

    #[test]
    fn crop_to_pixel_rect_maps_onto_the_raster() {
        let crop = NormalizedCrop {
            x: 0.25,
            y: 0.5,
            w: 0.5,
            h: 0.25,
        };
        let px = crop.to_pixel_rect(2560, 1440);
        assert_eq!((px.x, px.y, px.width, px.height), (640, 720, 1280, 360));

        // Out-of-range crops clamp inside the bitmap and keep a 1px floor:
        // the origin clamps to the last pixel, never past the edge.
        let bogus = NormalizedCrop {
            x: -1.0,
            y: 2.0,
            w: 99.0,
            h: 0.0,
        };
        let px = bogus.to_pixel_rect(100, 100);
        assert_eq!(px.x, 0);
        assert_eq!(px.y, 99);
        assert_eq!((px.width, px.height), (100, 1));
    }

    #[test]
    fn fractional_pans_move_the_window_smoothly() {
        let area = Rect::new(0, 0, 80, 24);
        let half = fit(area, 2.0, (20.0, 0.0)).crop.expect("crop");
        assert!((half.x - 0.375).abs() < 1e-4, "quarter of the overflow");
        assert!((half.w - 0.5).abs() < 1e-4, "window size is pan-invariant");
    }
}
