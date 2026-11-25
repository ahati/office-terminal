use hayro_interpret::font::GlyphRun;
use hayro_interpret::kurbo::{BezPath, PathEl, Rect};
use hayro_interpret::util::TransformExt;
use hayro_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask,
};
use hayro_syntax::page::Page;
use office_terminal_core::{
    AffineTransform, CompoundLine, FillRule, LineCap, LineJoin, PathSegment, ShapeEffects, SlideLink, VectorBackground,
    VectorCommand, VectorPaint, VectorPath, VectorSlide, VectorStroke, EMU_PER_PT,
};

use crate::error::PdfError;

/// Recorder that translates abstract hayro Device commands into VectorSlide IR (Phase V).
pub struct VectorCommandRecorder {
    _page_height_pt: f32,
    commands: Vec<VectorCommand>,
}

impl VectorCommandRecorder {
    pub fn new(page_height_pt: f32) -> Self {
        Self {
            _page_height_pt: page_height_pt,
            commands: Vec::new(),
        }
    }

    fn convert_path(&self, bez: &BezPath, fill_rule: hayro_interpret::FillRule) -> VectorPath {
        let rule = match fill_rule {
            hayro_interpret::FillRule::NonZero => FillRule::NonZero,
            hayro_interpret::FillRule::EvenOdd => FillRule::EvenOdd,
        };

        let mut segments = Vec::new();
        for el in bez.elements() {
            match el {
                PathEl::MoveTo(p) => {
                    segments.push(PathSegment::MoveTo {
                        x: (p.x * f64::from(EMU_PER_PT)) as f32,
                        y: (p.y * f64::from(EMU_PER_PT)) as f32,
                    });
                }
                PathEl::LineTo(p) => {
                    segments.push(PathSegment::LineTo {
                        x: (p.x * f64::from(EMU_PER_PT)) as f32,
                        y: (p.y * f64::from(EMU_PER_PT)) as f32,
                    });
                }
                PathEl::QuadTo(p1, p2) => {
                    segments.push(PathSegment::QuadTo {
                        x1: (p1.x * f64::from(EMU_PER_PT)) as f32,
                        y1: (p1.y * f64::from(EMU_PER_PT)) as f32,
                        x: (p2.x * f64::from(EMU_PER_PT)) as f32,
                        y: (p2.y * f64::from(EMU_PER_PT)) as f32,
                    });
                }
                PathEl::CurveTo(p1, p2, p3) => {
                    segments.push(PathSegment::CubicTo {
                        x1: (p1.x * f64::from(EMU_PER_PT)) as f32,
                        y1: (p1.y * f64::from(EMU_PER_PT)) as f32,
                        x2: (p2.x * f64::from(EMU_PER_PT)) as f32,
                        y2: (p2.y * f64::from(EMU_PER_PT)) as f32,
                        x: (p3.x * f64::from(EMU_PER_PT)) as f32,
                        y: (p3.y * f64::from(EMU_PER_PT)) as f32,
                    });
                }
                PathEl::ClosePath => {
                    segments.push(PathSegment::Close);
                }
            }
        }

        VectorPath::new(segments, rule)
    }

    pub fn finish(self) -> Vec<VectorCommand> {
        self.commands
    }
}

impl<'a> Device<'a> for VectorCommandRecorder {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, draw_mode: &DrawMode) {
        let (fill, stroke, rule) = match draw_mode {
            DrawMode::Invisible => return,
            DrawMode::Fill(f) => {
                let color = match &props.paint {
                    hayro_interpret::Paint::Color(c) => c.to_rgba().to_rgba8(),
                    _ => [0, 0, 0, 255],
                };
                (Some(VectorPaint::Solid(color)), None, *f)
            }
            DrawMode::Stroke(stroke_props) => {
                let color = match &props.paint {
                    hayro_interpret::Paint::Color(c) => c.to_rgba().to_rgba8(),
                    _ => [0, 0, 0, 255],
                };
                let stroke_width_emu = (stroke_props.line_width * EMU_PER_PT).max(1.0) as i64;
                let stroke = VectorStroke {
                    color,
                    width_emu: stroke_width_emu,
                    line_cap: LineCap::Butt,
                    line_join: LineJoin::Miter,
                    dash_segments: None,
                    paint: None,
                    miter_limit: 10.0,
                    compound: CompoundLine::Sng,
                    head_end: None,
                    tail_end: None,
                };
                (None, Some(stroke), hayro_interpret::FillRule::NonZero)
            }
            DrawMode::FillAndStroke(f, stroke_props) => {
                let color = match &props.paint {
                    hayro_interpret::Paint::Color(c) => c.to_rgba().to_rgba8(),
                    _ => [0, 0, 0, 255],
                };
                let stroke_width_emu = (stroke_props.line_width * EMU_PER_PT).max(1.0) as i64;
                let stroke = VectorStroke {
                    color,
                    width_emu: stroke_width_emu,
                    line_cap: LineCap::Butt,
                    line_join: LineJoin::Miter,
                    dash_segments: None,
                    paint: None,
                    miter_limit: 10.0,
                    compound: CompoundLine::Sng,
                    head_end: None,
                    tail_end: None,
                };
                (Some(VectorPaint::Solid(color)), Some(stroke), *f)
            }
        };

        let vpath = self.convert_path(path, rule);
        self.commands.push(VectorCommand::DrawPath {
            path: vpath,
            fill,
            stroke,
            transform: AffineTransform::identity(),
            effects: Box::new(ShapeEffects::default()),
        });
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        let vpath = self.convert_path(&clip_path.path, clip_path.fill);
        self.commands.push(VectorCommand::PushClip(vpath));
    }

    fn push_transparency_group(&mut self, _opacity: f32, _mask: Option<SoftMask<'a>>, _blend_mode: BlendMode) {}

    fn pop_clip(&mut self) {
        self.commands.push(VectorCommand::PopClip);
    }

    fn pop_transparency_group(&mut self) {}

    fn draw_image(&mut self, _image: Image<'a, '_>, _props: ImageDrawProps<'a>) {}

    fn draw_glyph_run(&mut self, _glyph_run: &GlyphRun<'_, 'a>, _props: DrawProps<'a>, _draw_mode: &DrawMode) {}
}

/// Compiles a PDF page to a pure vector display list (Phase V).
pub fn compile_page_vector(page: &Page<'_>, links: Vec<SlideLink>) -> Result<VectorSlide, PdfError> {
    let (w_pt, h_pt) = page.render_dimensions();
    let width_emu = (w_pt * EMU_PER_PT).round() as i64;
    let height_emu = (h_pt * EMU_PER_PT).round() as i64;

    let cache = InterpreterCache::default();
    let settings = InterpreterSettings::default();

    let mut state = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, w_pt as f64, h_pt as f64),
        &cache,
        page.xref(),
        settings,
    );

    let mut recorder = VectorCommandRecorder::new(h_pt);
    interpret_page(page, &mut state, &mut recorder);

    let mut slide = VectorSlide::new(width_emu, height_emu);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));
    slide.commands = recorder.finish();
    slide.hyperlinks = links;

    Ok(slide)
}
