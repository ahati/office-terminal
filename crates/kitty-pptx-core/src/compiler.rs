use crossbeam_channel::Sender;
use crate::contracts::*;
use crate::error::RenderError;

pub trait SlideCompiler: Send + Sync {
    fn compile(
        &self,
        slide: &pptx_model::Slide,
        slide_width: i64,
        slide_height: i64,
        warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError>;
}

pub struct DefaultSlideCompiler;

impl DefaultSlideCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl SlideCompiler for DefaultSlideCompiler {
    fn compile(
        &self,
        slide: &pptx_model::Slide,
        slide_width: i64,
        slide_height: i64,
        _warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        let width_emu = slide_width;
        let height_emu = slide_height;

        let mut commands = Vec::new();

        // 1. Process slide elements
        for elem in &slide.elements {
            match elem {
                pptx_model::SlideElement::Shape(shape) => {
                    let transform = AffineTransform {
                        sx: 1.0,
                        ky: 0.0,
                        kx: 0.0,
                        sy: 1.0,
                        tx: shape.x as f32,
                        ty: shape.y as f32,
                    };

                    let path = VectorPath {
                        segments: vec![
                            PathSegment::MoveTo { x: 0.0, y: 0.0 },
                            PathSegment::LineTo { x: shape.width as f32, y: 0.0 },
                            PathSegment::LineTo { x: shape.width as f32, y: shape.height as f32 },
                            PathSegment::LineTo { x: 0.0, y: shape.height as f32 },
                            PathSegment::Close,
                        ],
                        fill_rule: FillRule::NonZero,
                    };

                    let fill = match &shape.fill {
                        Some(pptx_model::Fill::Solid { color }) => {
                            Some(VectorPaint::Solid(parse_hex_color(color)))
                        }
                        Some(pptx_model::Fill::Gradient { angle, stops, .. }) => {
                            let gradient_stops = stops.iter().map(|stop| {
                                (stop.position as f32, parse_hex_color(&stop.color))
                            }).collect();
                            Some(VectorPaint::LinearGradient(VectorGradient {
                                angle_deg: *angle as f32,
                                stops: gradient_stops,
                            }))
                        }
                        _ => None,
                    };

                    let stroke = shape.stroke.as_ref().map(|s| {
                        VectorStroke {
                            color: parse_hex_color(&s.color),
                            width_emu: s.width,
                            dash_segments: None,
                            line_cap: LineCap::Square,
                            line_join: LineJoin::Miter,
                        }
                    });

                    commands.push(VectorCommand::DrawPath {
                        path,
                        fill,
                        stroke,
                        transform,
                    });

                    // Text body compilation
                    if let Some(ref text_body) = shape.text_body {
                        let mut runs = Vec::new();
                        for p in &text_body.paragraphs {
                            for r in &p.runs {
                                if let pptx_model::TextRun::Text(data) = r {
                                    runs.push(PositionedGlyphRun {
                                        font_family: data.font_family.clone().unwrap_or_else(|| "Carlito".to_string()),
                                        font_size_pt: data.font_size.map(|sz| sz as f32).unwrap_or(14.0),
                                        color: data.color.as_deref().map(parse_hex_color).unwrap_or([0, 0, 0, 255]),
                                        glyphs: Vec::new(),
                                        underline: data.underline,
                                        strikethrough: data.strikethrough,
                                    });
                                }
                            }
                        }
                        if !runs.is_empty() {
                            commands.push(VectorCommand::DrawText { runs, transform });
                        }
                    }
                }
                pptx_model::SlideElement::Picture(pic) => {
                    commands.push(VectorCommand::DrawImage {
                        rel_id: pic.image_path.clone(),
                        src_rect: NormalizedRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
                        dest_rect: RectF32 {
                            x: pic.x as f32,
                            y: pic.y as f32,
                            w: pic.width as f32,
                            h: pic.height as f32,
                        },
                        opacity: 1.0,
                        transform: AffineTransform::default(),
                    });
                }
                pptx_model::SlideElement::Table(tbl) => {
                    let transform = AffineTransform {
                        sx: 1.0, ky: 0.0, kx: 0.0, sy: 1.0,
                        tx: tbl.x as f32, ty: tbl.y as f32,
                    };
                    let path = VectorPath {
                        segments: vec![
                            PathSegment::MoveTo { x: 0.0, y: 0.0 },
                            PathSegment::LineTo { x: tbl.width as f32, y: 0.0 },
                            PathSegment::LineTo { x: tbl.width as f32, y: tbl.height as f32 },
                            PathSegment::LineTo { x: 0.0, y: tbl.height as f32 },
                            PathSegment::Close,
                        ],
                        fill_rule: FillRule::NonZero,
                    };
                    commands.push(VectorCommand::DrawPath {
                        path,
                        fill: Some(VectorPaint::Solid([240, 240, 240, 255])),
                        stroke: Some(VectorStroke {
                            color: [100, 100, 100, 255],
                            width_emu: 12700,
                            dash_segments: None,
                            line_cap: LineCap::Square,
                            line_join: LineJoin::Miter,
                        }),
                        transform,
                    });
                }
                _ => {}
            }
        }

        Ok(VectorSlide {
            width_emu,
            height_emu,
            background: Some(VectorBackground::Solid([255, 255, 255, 255])),
            commands,
        })
    }
}

fn parse_hex_color(hex: &str) -> [u8; 4] {
    let clean = hex.trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
        [r, g, b, 255]
    } else if clean.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
        let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
        [r, g, b, a]
    } else {
        [0, 0, 0, 255]
    }
}
