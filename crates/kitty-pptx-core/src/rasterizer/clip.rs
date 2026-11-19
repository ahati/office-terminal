//! Nested clip regions for the raster pass (`PushClip` / `PopClip` pairs).
//!
//! The reference wraps picture silhouettes in `ctx.save()/clip()/restore()`
//! (ref renderer.ts:6985-7036); the rasterizer delimits the same nested
//! region with [`ClipStack`], and every command drawn while the stack is
//! non-empty is composited through the clip intersection. The stack also owns
//! the bbox-sized scratch layer that masked commands render into, reusing the
//! allocation across commands when the size did not change.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::path;

/// One active clip region, in device-pixel bounds.
struct ClipLayer {
    path: VectorPath,
    /// `(min_x, min_y, max_x, max_y)` in canvas pixels (pre-clamp).
    bounds: (f32, f32, f32, f32),
}

/// Placement of a scratch layer plus the canvas mapping used to build masks.
#[derive(Clone, Copy)]
pub(super) struct LayerPlacement {
    pub(super) origin_x: i32,
    pub(super) origin_y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale: f32,
    pub(super) offset_x: f32,
    pub(super) offset_y: f32,
}

/// Nested clip regions declared by `PushClip` / `PopClip` pairs.
#[derive(Default)]
pub(super) struct ClipStack {
    /// `None` entries are no-op pushes (empty silhouettes); they still occupy a
    /// slot so an unbalanced-looking `PopClip` stays matched.
    layers: Vec<Option<ClipLayer>>,
}

impl ClipStack {
    pub(super) fn push(&mut self, path: &VectorPath, scale: f32, offset_x: f32, offset_y: f32) {
        // An empty silhouette carries no region to clip by (and tiny-skia
        // refuses to fill degenerate paths), so treat it as a no-op.
        if path.segments.is_empty() {
            self.layers.push(None);
            return;
        }
        match clip_bounds_px(path, scale, offset_x, offset_y) {
            Some(bounds) => self.layers.push(Some(ClipLayer {
                path: path.clone(),
                bounds,
            })),
            None => self.layers.push(None),
        }
    }

    pub(super) fn pop(&mut self) {
        self.layers.pop();
    }

    fn active(&self) -> impl Iterator<Item = &ClipLayer> {
        self.layers.iter().flatten()
    }

    pub(super) fn is_active(&self) -> bool {
        self.layers.iter().any(Option::is_some)
    }

    /// Intersection bounding box in whole pixels, clamped to the canvas.
    pub(super) fn device_bbox(&self, width: u32, height: u32) -> Option<(i32, i32, i32, i32)> {
        let mut iter = self.active();
        let first = iter.next()?;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = first.bounds;
        for layer in iter {
            min_x = min_x.max(layer.bounds.0);
            min_y = min_y.max(layer.bounds.1);
            max_x = max_x.min(layer.bounds.2);
            max_y = max_y.min(layer.bounds.3);
        }
        // One pixel of slack so the silhouette's anti-aliased edge survives.
        let x0 = (min_x - 1.0).floor().clamp(0.0, width as f32);
        let y0 = (min_y - 1.0).floor().clamp(0.0, height as f32);
        let x1 = (max_x + 1.0).ceil().clamp(0.0, width as f32);
        let y1 = (max_y + 1.0).ceil().clamp(0.0, height as f32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0 as i32, y0 as i32, x1 as i32, y1 as i32))
    }

    /// Intersection mask of every active clip, in scratch-layer coordinates.
    pub(super) fn build_mask(&self, place: LayerPlacement) -> Option<tiny_skia::Mask> {
        let local_x = place.offset_x - place.origin_x as f32;
        let local_y = place.offset_y - place.origin_y as f32;
        let mut mask: Option<tiny_skia::Mask> = None;
        for layer in self.active() {
            let rule = match layer.path.fill_rule {
                FillRule::NonZero => tiny_skia::FillRule::Winding,
                FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
            };
            let Some(ts_path) = device_path(&layer.path, place.scale, local_x, local_y) else {
                // Degenerate silhouette clips everything away.
                return tiny_skia::Mask::new(place.width, place.height);
            };
            match &mut mask {
                Some(existing) => existing.intersect_path(
                    &ts_path,
                    rule,
                    true,
                    tiny_skia::Transform::identity(),
                ),
                None => {
                    let mut fresh = tiny_skia::Mask::new(place.width, place.height)?;
                    fresh.fill_path(
                        &ts_path,
                        rule,
                        true,
                        tiny_skia::Transform::identity(),
                    );
                    mask = Some(fresh);
                }
            }
        }
        mask
    }
}

/// Conservative device-pixel bounds of a slide-EMU clip path.
fn clip_bounds_px(
    path: &VectorPath,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<(f32, f32, f32, f32)> {
    let (min_x, min_y, max_x, max_y) = path::local_bounds(path)?;
    let bounds = (
        offset_x + min_x * scale,
        offset_y + min_y * scale,
        offset_x + max_x * scale,
        offset_y + max_y * scale,
    );
    let (x0, y0, x1, y1) = bounds;
    if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
        Some(bounds)
    } else {
        None
    }
}

/// Convert a slide-EMU path into a device-space `tiny_skia::Path`.
fn device_path(
    path: &VectorPath,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<tiny_skia::Path> {
    let map = |x: f32, y: f32| -> Option<(f32, f32)> {
        let px = offset_x + x * scale;
        let py = offset_y + y * scale;
        (px.is_finite() && py.is_finite()).then_some((px, py))
    };
    let mut builder = tiny_skia::PathBuilder::new();
    for segment in &path.segments {
        match *segment {
            PathSegment::MoveTo { x, y } => {
                let (px, py) = map(x, y)?;
                builder.move_to(px, py);
            }
            PathSegment::LineTo { x, y } => {
                let (px, py) = map(x, y)?;
                builder.line_to(px, py);
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (c1x, c1y) = map(x1, y1)?;
                let (px, py) = map(x, y)?;
                builder.quad_to(c1x, c1y, px, py);
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1x, c1y) = map(x1, y1)?;
                let (c2x, c2y) = map(x2, y2)?;
                let (px, py) = map(x, y)?;
                builder.cubic_to(c1x, c1y, c2x, c2y, px, py);
            }
            PathSegment::Close => {
                builder.close();
            }
        }
    }
    builder.finish()
}

/// Borrow a cleared scratch layer of exactly `width` x `height`, reusing the
/// allocation across commands when the size did not change.
pub(super) fn scratch_layer(
    slot: &mut Option<tiny_skia::Pixmap>,
    width: u32,
    height: u32,
) -> Option<&mut tiny_skia::Pixmap> {
    let needs_alloc = match slot {
        Some(pixmap) => pixmap.width() != width || pixmap.height() != height,
        None => true,
    };
    if needs_alloc {
        *slot = Some(tiny_skia::Pixmap::new(width, height)?);
    }
    let pixmap = slot.as_mut()?;
    pixmap.data_mut().fill(0);
    Some(pixmap)
}
