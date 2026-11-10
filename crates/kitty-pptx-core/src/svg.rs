use std::fmt::Write;
use crate::contracts::*;
use crate::error::RenderError;
use crate::loader::AssetStore;

pub trait SlideSvgSerializer: Send + Sync {
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        assets: &dyn AssetStore,
        text_mode: SvgTextMode,
    ) -> Result<String, RenderError>;
}

pub struct DefaultSlideSvgSerializer;

impl DefaultSlideSvgSerializer {
    pub fn new() -> Self {
        Self
    }
}

impl SlideSvgSerializer for DefaultSlideSvgSerializer {
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        _assets: &dyn AssetStore,
        _text_mode: SvgTextMode,
    ) -> Result<String, RenderError> {
        let mut svg = String::new();
        let w = scene.width_emu;
        let h = scene.height_emu;

        writeln!(&mut svg, r##"<?xml version="1.0" encoding="UTF-8"?>"##).unwrap();
        writeln!(&mut svg, r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">"##, w, h).unwrap();

        // 1. Background
        if let Some(ref bg) = scene.background {
            match bg {
                VectorBackground::Solid([r, g, b, a]) => {
                    let opacity = *a as f32 / 255.0;
                    writeln!(&mut svg, r##"  <rect width="{}" height="{}" fill="rgb({},{},{})" fill-opacity="{:.2}"/>"##, w, h, r, g, b, opacity).unwrap();
                }
                _ => {
                    writeln!(&mut svg, r##"  <rect width="{}" height="{}" fill="#ffffff"/>"##, w, h).unwrap();
                }
            }
        } else {
            writeln!(&mut svg, r##"  <rect width="{}" height="{}" fill="#ffffff"/>"##, w, h).unwrap();
        }

        // 2. Render commands
        for cmd in &scene.commands {
            match cmd {
                VectorCommand::DrawPath { path, fill, stroke, transform } => {
                    let mut d = String::new();
                    for seg in &path.segments {
                        match seg {
                            PathSegment::MoveTo { x, y } => {
                                write!(&mut d, "M {} {} ", x, y).unwrap();
                            }
                            PathSegment::LineTo { x, y } => {
                                write!(&mut d, "L {} {} ", x, y).unwrap();
                            }
                            PathSegment::QuadTo { x1, y1, x, y } => {
                                write!(&mut d, "Q {} {}, {} {} ", x1, y1, x, y).unwrap();
                            }
                            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                                write!(&mut d, "C {} {}, {} {}, {} {} ", x1, y1, x2, y2, x, y).unwrap();
                            }
                            PathSegment::Close => {
                                write!(&mut d, "Z ").unwrap();
                            }
                        }
                    }

                    let fill_attr = match fill {
                        Some(VectorPaint::Solid([r, g, b, a])) => {
                            format!(r#"fill="rgb({},{},{})" fill-opacity="{:.2}""#, r, g, b, *a as f32 / 255.0)
                        }
                        _ => r#"fill="none""#.to_string(),
                    };

                    let stroke_attr = match stroke {
                        Some(s) => {
                            let [r, g, b, a] = s.color;
                            format!(r#"stroke="rgb({},{},{})" stroke-opacity="{:.2}" stroke-width="{}""#, r, g, b, a as f32 / 255.0, s.width_emu)
                        }
                        None => r#"stroke="none""#.to_string(),
                    };

                    writeln!(
                        &mut svg,
                        r#"  <path transform="translate({}, {})" d="{}" {} {}/>"#,
                        transform.tx, transform.ty, d.trim(), fill_attr, stroke_attr
                    ).unwrap();
                }
                VectorCommand::DrawText { runs, transform } => {
                    let mut cur_y = 0.0;
                    for r in runs {
                        let [cr, cg, cb, ca] = r.color;
                        let opacity = ca as f32 / 255.0;
                        let size_emu = r.font_size_pt * 12700.0;
                        cur_y += size_emu;
                        writeln!(
                            &mut svg,
                            r#"  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="rgb({},{},{})" fill-opacity="{:.2}">Text Run</text>"#,
                            transform.tx, transform.ty + cur_y, r.font_family, size_emu, cr, cg, cb, opacity
                        ).unwrap();
                    }
                }
                _ => {}
            }
        }

        writeln!(&mut svg, "</svg>").unwrap();
        Ok(svg)
    }
}
