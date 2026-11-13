pub mod background;
pub mod charts;
pub mod hyperlinks;
pub mod math;
pub mod placeholders;
pub mod pictures;
pub mod shape_effects;
pub mod shapes;
pub mod style;
pub mod tables;
pub mod text;
pub mod text_warp;
pub mod transform;

pub use style::parse_hex_color;
pub use hyperlinks::{
    extract_deck_hyperlinks, extract_slide_hyperlinks, resolve_hyperlink, resolve_internal_slide,
};

use crossbeam_channel::Sender;
use crate::contracts::{
    FidelityClass, ShapeEffects, VectorCommand, VectorSlide, WarningEvent,
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

    /// Compile with deck context so internal slide-jump hyperlinks can be
    /// resolved to 0-based presentation indices. Defaults to `compile`.
    fn compile_in_deck(
        &self,
        slide: &pptx_model::Slide,
        deck: &[pptx_model::Slide],
        slide_width: i64,
        slide_height: i64,
        warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        let _ = deck;
        self.compile(slide, slide_width, slide_height, warning_sink)
    }
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
    fn compile_in_deck(
        &self,
        slide: &pptx_model::Slide,
        deck: &[pptx_model::Slide],
        slide_width: i64,
        slide_height: i64,
        warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        let mut vector_slide = self.compile(slide, slide_width, slide_height, warning_sink)?;
        vector_slide.hyperlinks =
            hyperlinks::extract_slide_hyperlinks(slide, deck, warning_sink);
        Ok(vector_slide)
    }

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
                    // PRD 08: 2D effects are fully supported; 3D bevel and
                    // extrusion surfaces are approximated as planar contours.
                    if shape.sp3d.is_some() {
                        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
                            element_id: shape.geometry.clone(),
                            feature: "shape_3d_approximated",
                            class: FidelityClass::Degraded,
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

                    let subpaths = shapes::build_shape_paths(
                        &shape.geometry,
                        shape.width as f32,
                        shape.height as f32,
                        &adj_slice,
                        shape.cust_geom.as_deref(),
                        warning_sink,
                    );

                    let fill = shape.fill.as_ref().and_then(|f| style::compile_fill(f, 1.0));
                    let stroke = shape.stroke.as_ref().and_then(style::compile_stroke);
                    let effects = shape_effects::extract_shape_effects(shape);

                    // ref `preset-geometry/index.ts:44-99`: one paint pass per
                    // authored `<a:path>`; `fill` tints the base fill, `stroke`
                    // decides whether the outline follows that subpath. The
                    // reference clears the shadow after the first painted path
                    // (`index.ts:71-77`), so only that subpath carries effects.
                    let shadow_owner = subpaths
                        .iter()
                        .position(|subpath| subpath.fill.is_filled())
                        .unwrap_or(0);

                    for (index, subpath) in subpaths.iter().enumerate() {
                        commands.push(VectorCommand::DrawPath {
                            path: subpath.path.clone(),
                            fill: shapes::apply_path_fill_mode(fill.as_ref(), subpath.fill),
                            stroke: if subpath.stroke { stroke.clone() } else { None },
                            transform,
                            effects: Box::new(if index == shadow_owner {
                                effects.clone()
                            } else {
                                ShapeEffects::default()
                            }),
                        });
                    }

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
            hyperlinks: hyperlinks::extract_slide_hyperlinks(slide, &[], warning_sink),
        })
    }
}
