//! `metafile_core::Renderer` implementation forwarding to `MetafileSink`.

use metafile_core as mc;

use crate::contracts::{AffineTransform, BitmapSampling, FillRule, RectF32, VectorPath};

use super::mapping::{map_brush, map_paint, map_pen, map_stroke};
use super::primitives::{arc_path, closed_polygon, ellipse_path, engine_path, map_clip, open_path, rounded_rect};
use super::{fill_rule, from_engine_rect, rgba, to_engine_error};
use crate::metafile::{
    MetafileBitmap, MetafileClip, MetafileDiagnostic, MetafileError, MetafilePath, MetafileSink, MetafileTextRun,
    TextAlignH, TextAlignV,
};

pub(super) struct SinkRenderer<'a> {
    sink: &'a mut dyn MetafileSink,
    bounds: Option<mc::Rect>,
}

impl<'a> SinkRenderer<'a> {
    pub(super) fn new(sink: &'a mut dyn MetafileSink, bounds: Option<mc::Rect>) -> Self {
        Self { sink, bounds }
    }
}

impl SinkRenderer<'_> {
    fn note(&mut self, tag: &'static str, detail: &str) {
        self.sink.diagnostic(MetafileDiagnostic::degraded(tag, detail));
    }

    fn push_clips(&mut self, clip: Option<&mc::ClipRegion>) -> usize {
        let Some(region) = clip else {
            return 0;
        };
        let (paths, approximation) = map_clip(region);
        if let Some(tag) = approximation {
            self.note(tag, "complex clip region approximated to its bounds");
        }
        let mut pushed = 0;
        for path in paths {
            if self.sink.push_clip(&MetafileClip { path }).is_err() {
                break;
            }
            pushed += 1;
        }
        pushed
    }

    fn pop_clips(&mut self, count: usize) {
        for _ in 0..count {
            self.sink.pop_clip();
        }
    }

    fn run_clipped(
        &mut self,
        clip: Option<&mc::ClipRegion>,
        draw: impl FnOnce(&mut Self) -> Result<(), MetafileError>,
    ) -> mc::Result<()> {
        let pushed = self.push_clips(clip);
        let result = draw(self).map_err(to_engine_error);
        self.pop_clips(pushed);
        result
    }

    fn draw_vector(
        &mut self,
        path: VectorPath,
        fill: Option<&mc::Paint>,
        stroke: Option<&mc::Stroke>,
        rule: FillRule,
    ) -> Result<(), MetafileError> {
        let (mapped_fill, fill_tag) = match fill {
            Some(paint) => map_paint(paint),
            None => (None, None),
        };
        if let Some(tag) = fill_tag {
            self.note(tag, "fill approximated");
        }
        let (mapped_stroke, stroke_tag) = match stroke {
            Some(stroke) => {
                let (pen, tag) = map_stroke(stroke);
                (Some(pen), tag)
            }
            None => (None, None),
        };
        if let Some(tag) = stroke_tag {
            self.note(tag, "stroke approximated");
        }
        if mapped_fill.is_none() && mapped_stroke.is_none() {
            return Ok(());
        }
        self.sink.draw_path(
            &MetafilePath::new(path.segments, rule),
            mapped_fill.as_ref(),
            mapped_stroke.as_ref(),
        )
    }

    fn draw_pen_path(
        &mut self,
        path: VectorPath,
        pen: Option<&mc::Pen>,
        brush: Option<&mc::Brush>,
        rule: FillRule,
    ) -> Result<(), MetafileError> {
        let (fill, fill_tag) = match brush {
            Some(brush) => map_brush(brush),
            None => (None, None),
        };
        if let Some(tag) = fill_tag {
            self.note(tag, "brush approximated");
        }
        let (mapped_pen, pen_tag) = match pen {
            Some(pen) => map_pen(pen),
            None => (None, None),
        };
        if let Some(tag) = pen_tag {
            self.note(tag, "pen approximated");
        }
        self.sink.draw_path(
            &MetafilePath::new(path.segments, rule),
            fill.as_ref(),
            mapped_pen.as_ref(),
        )
    }
}

impl mc::Renderer for SinkRenderer<'_> {
    fn drawing_bounds(&self) -> Option<mc::Rect> {
        self.bounds
    }

    fn line(&mut self, from: mc::Point, to: mc::Point, pen: &mc::Pen, clip: Option<&mc::ClipRegion>) -> mc::Result<()> {
        self.run_clipped(clip, |this| {
            this.draw_pen_path(open_path(&[from, to]), Some(pen), None, FillRule::NonZero)
        })
    }

    fn polyline(&mut self, points: &[mc::Point], pen: &mc::Pen, clip: Option<&mc::ClipRegion>) -> mc::Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        self.run_clipped(clip, |this| {
            this.draw_pen_path(open_path(points), Some(pen), None, FillRule::NonZero)
        })
    }

    fn polygon(
        &mut self,
        points: &[mc::Point],
        pen: &mc::Pen,
        brush: &mc::Brush,
        fill_mode: u16,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        self.run_clipped(clip, |this| {
            this.draw_pen_path(closed_polygon(points), Some(pen), Some(brush), fill_rule(fill_mode))
        })
    }

    fn poly_polygon(
        &mut self,
        polygons: &[Vec<mc::Point>],
        pen: &mc::Pen,
        brush: &mc::Brush,
        fill_mode: u16,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        self.run_clipped(clip, |this| {
            for points in polygons {
                if points.is_empty() {
                    continue;
                }
                this.draw_pen_path(closed_polygon(points), Some(pen), Some(brush), fill_rule(fill_mode))?;
            }
            Ok(())
        })
    }

    fn path(
        &mut self,
        path: &mc::Path,
        pen: &mc::Pen,
        brush: &mc::Brush,
        fill_mode: u16,
        stroke: bool,
        fill: bool,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        let mapped = engine_path(path, fill_rule(fill_mode));
        if mapped.segments.is_empty() {
            return Ok(());
        }
        let pen = stroke.then_some(pen);
        let brush = fill.then_some(brush);
        let rule = fill_rule(fill_mode);
        self.run_clipped(clip, move |this| this.draw_pen_path(mapped, pen, brush, rule))
    }

    fn styled_path(
        &mut self,
        path: &mc::Path,
        stroke: Option<&mc::Stroke>,
        fill: Option<&mc::Paint>,
        fill_mode: u16,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        let mapped = engine_path(path, fill_rule(fill_mode));
        if mapped.segments.is_empty() {
            return Ok(());
        }
        self.run_clipped(clip, move |this| {
            this.draw_vector(mapped, fill, stroke, fill_rule(fill_mode))
        })
    }

    fn rectangle(
        &mut self,
        rect: mc::Rect,
        radius: Option<mc::Point>,
        pen: &mc::Pen,
        brush: &mc::Brush,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        let radius = radius.map(|r| (r.x.abs() as f32, r.y.abs() as f32));
        let path = rounded_rect(from_engine_rect(rect), radius);
        self.run_clipped(clip, |this| {
            this.draw_pen_path(path, Some(pen), Some(brush), FillRule::NonZero)
        })
    }

    fn ellipse(
        &mut self,
        rect: mc::Rect,
        pen: &mc::Pen,
        brush: &mc::Brush,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        let path = ellipse_path(from_engine_rect(rect));
        self.run_clipped(clip, |this| {
            this.draw_pen_path(path, Some(pen), Some(brush), FillRule::NonZero)
        })
    }

    fn arc(
        &mut self,
        rect: mc::Rect,
        start: mc::Point,
        end: mc::Point,
        kind: mc::ArcKind,
        clockwise: bool,
        pen: &mc::Pen,
        brush: &mc::Brush,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        let path = arc_path(from_engine_rect(rect), start, end, kind, clockwise);
        self.run_clipped(clip, |this| {
            this.draw_pen_path(path, Some(pen), Some(brush), FillRule::NonZero)
        })
    }

    fn pixel(&mut self, point: mc::Point, color: mc::Color, clip: Option<&mc::ClipRegion>) -> mc::Result<()> {
        let path = rounded_rect(
            RectF32 {
                x: point.x as f32,
                y: point.y as f32,
                w: 1.0,
                h: 1.0,
            },
            None,
        );
        self.run_clipped(clip, |this| {
            this.draw_vector(path, Some(&mc::Paint::Solid(color)), None, FillRule::NonZero)
        })
    }

    fn text(&mut self, run: &mc::TextRun) -> mc::Result<()> {
        let mapped = MetafileTextRun {
            text: run.text.clone(),
            glyph_indices: if run.glyph_indices.is_empty() {
                None
            } else {
                Some(run.glyph_indices.clone())
            },
            family: run.font.family.clone(),
            size: run.font.height as f32,
            color: rgba(run.color),
            bold: run.font.weight >= 600,
            italic: run.font.italic,
            underline: run.font.underline,
            strike_out: run.font.strike_out,
            x: run.position.x as f32,
            y: run.position.y as f32,
            escapement_deg: f32::from(run.font.escapement_tenths) / 10.0,
            dx: run.dx.iter().map(|value| *value as f32).collect(),
            horizontal_align: match run.horizontal_align {
                mc::HorizontalTextAlignment::Left => TextAlignH::Left,
                mc::HorizontalTextAlignment::Center => TextAlignH::Center,
                mc::HorizontalTextAlignment::Right => TextAlignH::Right,
            },
            vertical_align: match run.vertical_align {
                mc::VerticalTextAlignment::Top => TextAlignV::Top,
                mc::VerticalTextAlignment::Center => TextAlignV::Center,
                mc::VerticalTextAlignment::Bottom => TextAlignV::Bottom,
                mc::VerticalTextAlignment::Baseline => TextAlignV::Baseline,
            },
            background: run.background.map(rgba),
            background_rect: run
                .background_rect
                .map(from_engine_rect)
                .or_else(|| run.background_path.as_ref().and_then(super::primitives::path_bounds)),
            transform: text_transform(run.transform),
            right_to_left: run.right_to_left,
        };
        let pushed = self.push_clips(run.clip.as_ref());
        let result = self.sink.draw_text(&mapped).map_err(to_engine_error);
        self.pop_clips(pushed);
        result
    }

    fn bitmap(
        &mut self,
        placement: mc::BitmapPlacement,
        bitmap: &mc::Bitmap,
        sampling: mc::BitmapSampling,
        clip: Option<&mc::ClipRegion>,
    ) -> mc::Result<()> {
        let mapped = MetafileBitmap {
            width: bitmap.width,
            height: bitmap.height,
            rgba: bitmap.rgba.clone(),
            placement: placement_transform(placement),
            opacity: 1.0,
            sampling: match sampling {
                mc::BitmapSampling::Pixelated => BitmapSampling::Nearest,
                mc::BitmapSampling::Auto | mc::BitmapSampling::Smooth => BitmapSampling::Linear,
            },
        };
        let pushed = self.push_clips(clip);
        let result = self.sink.draw_bitmap(&mapped).map_err(to_engine_error);
        self.pop_clips(pushed);
        result
    }
}

fn placement_transform(placement: mc::BitmapPlacement) -> AffineTransform {
    AffineTransform {
        sx: placement.x_axis.x as f32,
        ky: placement.x_axis.y as f32,
        kx: placement.y_axis.x as f32,
        sy: placement.y_axis.y as f32,
        tx: placement.origin.x as f32,
        ty: placement.origin.y as f32,
    }
}

/// The engine's local-text → output affine, rejecting non-finite matrices.
fn text_transform(transform: mc::Transform) -> AffineTransform {
    if !transform.is_finite() {
        return AffineTransform::identity();
    }
    AffineTransform {
        sx: transform.m11 as f32,
        ky: transform.m12 as f32,
        kx: transform.m21 as f32,
        sy: transform.m22 as f32,
        tx: transform.dx as f32,
        ty: transform.dy as f32,
    }
}
