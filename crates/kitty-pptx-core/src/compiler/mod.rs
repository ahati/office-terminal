pub mod background;
pub mod charts;
pub mod placeholders;
pub mod pictures;
pub mod shapes;
pub mod style;
pub mod tables;
pub mod text;
pub mod transform;

pub use style::parse_hex_color;

use crossbeam_channel::Sender;
use crate::contracts::{
    FidelityClass, VectorCommand, VectorSlide, WarningEvent,
};
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

impl Default for DefaultSlideCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl SlideCompiler for DefaultSlideCompiler {
    fn compile(
        &self,
        slide: &pptx_model::Slide,
        slide_width: i64,
        slide_height: i64,
        warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        let width_emu = slide_width;
        let height_emu = slide_height;

        let background = background::compile_slide_background(slide.background.as_ref());
        let mut commands = Vec::new();

        for elem in &slide.elements {
            match elem {
                pptx_model::SlideElement::Shape(shape) => {
                    // Inspect shape effects per PRD 02d §2.1
                    if shape.shadow.is_some()
                        || shape.inner_shadow.is_some()
                        || shape.glow.is_some()
                        || shape.soft_edge.is_some()
                        || shape.reflection.is_some()
                        || shape.scene3d.is_some()
                        || shape.sp3d.is_some()
                    {
                        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
                            element_id: shape.geometry.clone(),
                            feature: "shape_effect_omitted",
                            class: FidelityClass::Omitted,
                        });
                    }

                    let transform = transform::compute_shape_transform(
                        shape.x,
                        shape.y,
                        shape.width,
                        shape.height,
                        shape.rotation,
                        shape.flip_h,
                        shape.flip_v,
                    );

                    let adj_slice: Vec<Option<f64>> = vec![
                        shape.adj, shape.adj2, shape.adj3, shape.adj4,
                        shape.adj5, shape.adj6, shape.adj7, shape.adj8,
                    ];

                    let path = shapes::build_shape_path(
                        &shape.geometry,
                        shape.width as f32,
                        shape.height as f32,
                        &adj_slice,
                        shape.cust_geom.as_deref(),
                        warning_sink,
                    );

                    let fill = shape.fill.as_ref().and_then(|f| style::compile_fill(f, 1.0));
                    let stroke = shape.stroke.as_ref().and_then(style::compile_stroke);

                    commands.push(VectorCommand::DrawPath {
                        path,
                        fill,
                        stroke,
                        transform,
                        effects: Box::default(),
                    });

                    // Compile text body if present (supporting SmartArt explicit text_rect)
                    if let Some(ref text_body) = shape.text_body {
                        let (text_w, text_h, text_transform) = if let Some(ref tr) = shape.text_rect {
                            let tr_transform = transform::compute_shape_transform(
                                tr.x,
                                tr.y,
                                tr.width,
                                tr.height,
                                0.0,
                                false,
                                false,
                            );
                            (tr.width, tr.height, tr_transform)
                        } else {
                            (shape.width, shape.height, transform)
                        };

                        let text_cmds = text::compile_text_body(
                            text_body,
                            text_w,
                            text_h,
                            &text_transform,
                            shape.default_text_color.as_deref(),
                            warning_sink,
                        );
                        commands.extend(text_cmds);
                    }
                }

                pptx_model::SlideElement::Picture(pic) => {
                    let pic_cmds = pictures::compile_picture_element(pic, warning_sink);
                    commands.extend(pic_cmds);
                }

                pptx_model::SlideElement::Table(tbl) => {
                    let tbl_cmds = tables::compile_table_element(tbl, warning_sink);
                    commands.extend(tbl_cmds);
                }

                pptx_model::SlideElement::Chart(chart) => {
                    let chart_cmds = charts::compile_chart(chart, warning_sink);
                    commands.extend(chart_cmds);
                }

                pptx_model::SlideElement::Media(media) => {
                    let media_cmds = placeholders::compile_media_placeholder(media);
                    commands.extend(media_cmds);
                }
            }
        }

        Ok(VectorSlide {
            width_emu,
            height_emu,
            background,
            commands,
            hyperlinks: Vec::new(),
        })
    }
}
