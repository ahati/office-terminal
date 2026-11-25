//! Shared fixtures for the `shapes-a` parity ports
//! (`custgeom-path-paint`, `frame-transform`, `renderer.gradient`,
//! `pattern-thumbnail`, `renderer-shape-image-fill`).
//!
//! The JSON shapes mirror the reference specs' element fixtures one-for-one, and
//! are compiled through `DefaultSlideCompiler` so every assertion reads the
//! production `VectorSlide` rather than test-side re-implementations.

#![allow(dead_code)]

use crossbeam_channel::unbounded;

use office_terminal_core::compiler::{DefaultSlideCompiler, SlideCompiler};
use office_terminal_core::contracts::{
    PathSegment, VectorCommand, VectorPaint, VectorPath, VectorPattern, VectorStroke,
};
use pptx_model::Slide;
use serde_json::{json, Value};

/// 1 pt in EMU (the reference `width: 12_700` line).
pub const EMU: i64 = 12_700;
/// 1 px at 96 DPI (the reference specs' scale-1 unit).
pub const PX: i64 = 9_525;

/// Compile one slide (9144000 × 6858000 EMU, like every reference `renderSlide`
/// call) holding the given element JSONs.
pub fn compile_elements(elements: Vec<Value>) -> Vec<VectorCommand> {
    let slide_value = json!({
        "index": 0,
        "slideNumber": 1,
        "background": null,
        "elements": elements,
        "elementSources": [],
        "comments": [],
    });
    let slide: Slide = serde_json::from_value(slide_value).expect("slide JSON");
    let (tx, _rx) = unbounded();
    DefaultSlideCompiler::new()
        .compile(&slide, 9_144_000, 6_858_000, &tx)
        .expect("compile")
        .commands
}

pub fn solid(color: &str) -> Value {
    json!({"fillType": "solid", "color": color})
}

pub fn black_stroke() -> Value {
    json!({"color": "000000", "width": EMU})
}

/// The reference `shape()` fixture (`renderer-shape-image-fill.test.ts:60-90`,
/// `custgeom-path-paint.test.ts:36-66`): a 1in × 1in box at (1in, 1in) with a
/// solid `336699` fill and a 1 pt black line.
pub fn shape(geometry: &str) -> Value {
    json!({
        "type": "shape",
        "x": 914_400, "y": 914_400, "width": 914_400, "height": 914_400,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": geometry,
        "fill": solid("336699"),
        "stroke": black_stroke(),
        "textBody": null,
        "defaultTextColor": null,
        "custGeom": null,
    })
}

/// The reference `square` path (`custgeom-path-paint.test.ts:29-34`): a
/// right triangle — `moveTo(0,0)`, two `lineTo`s, `close`.
pub fn square() -> Value {
    json!([
        {"cmd": "moveTo", "x": 0.0, "y": 0.0},
        {"cmd": "lineTo", "x": 1.0, "y": 0.0},
        {"cmd": "lineTo", "x": 1.0, "y": 1.0},
        {"cmd": "close"},
    ])
}

/// One `square` contour scaled into the 914400 × 914400 EMU shape box.
pub fn contour() -> Vec<PathSegment> {
    vec![
        PathSegment::MoveTo { x: 0.0, y: 0.0 },
        PathSegment::LineTo { x: 914_400.0, y: 0.0 },
        PathSegment::LineTo {
            x: 914_400.0,
            y: 914_400.0,
        },
        PathSegment::Close,
    ]
}

pub fn shape_fills(commands: &[VectorCommand]) -> Vec<Option<VectorPaint>> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath { fill, .. } => Some(fill.clone()),
            _ => None,
        })
        .collect()
}

pub fn shape_strokes(commands: &[VectorCommand]) -> Vec<Option<VectorStroke>> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath { stroke, .. } => Some(stroke.clone()),
            _ => None,
        })
        .collect()
}

pub fn draw_image_count(commands: &[VectorCommand]) -> usize {
    commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawImage { .. }))
        .count()
}

/// Concatenated text of every `DrawText` run — the reference's `textRuns`
/// callback (`renderer-shape-image-fill.test.ts:63-83`).
pub fn drawn_text(commands: &[VectorCommand]) -> String {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs),
            _ => None,
        })
        .flat_map(|runs| runs.iter().map(|run| run.text.clone()))
        .collect()
}

/// The reference `textBody` fixture: one 18 pt `"Picture fill text"` paragraph
/// with no insets, anchored top, wrapping square.
pub fn picture_fill_text_body() -> Value {
    json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null, "lvl": 0,
            "bullet": {"type": "inherit"},
            "defFontSize": null, "defColor": null, "defBold": null, "defItalic": null,
            "defFontFamily": null, "tabStops": [], "rtl": false, "eaLnBrk": true,
            "runs": [{
                "type": "text", "text": "Picture fill text",
                "bold": null, "italic": null,
                "underline": false, "strikeDouble": false,
                "hyperlinkUsesTextFill": false, "fieldType": null,
                "strikethrough": false, "fontSize": 18.0,
                "color": null, "fontFamily": "Arial",
            }],
        }],
        "defaultFontSize": 18.0, "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": "horz", "autoFit": "none",
    })
}

// ───────────────────────────────────────────────────────────────────────────
// Slide-level helpers for the image-fill ports (`renderer-shape-image-fill`).
// ───────────────────────────────────────────────────────────────────────────

use std::sync::{Arc, Mutex};

use office_terminal_core::contracts::{RenderTargetSpec, RgbaSlide, VectorSlide};
use office_terminal_core::error::AssetError;
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};

/// Compile one slide (9144000 × 6858000 EMU) into a [`VectorSlide`].
pub fn compile_slide(elements: Vec<Value>) -> VectorSlide {
    let slide_value = json!({
        "index": 0,
        "slideNumber": 1,
        "background": null,
        "elements": elements,
        "elementSources": [],
        "comments": [],
    });
    let slide: Slide = serde_json::from_value(slide_value).expect("slide JSON");
    let (tx, _rx) = unbounded();
    DefaultSlideCompiler::new()
        .compile(&slide, 9_144_000, 6_858_000, &tx)
        .expect("compile")
}

/// Solid-colour bitmap with a distinct quad palette so a painted blip is
/// distinguishable from the white canvas.
pub struct Image {
    pub rel_id: &'static str,
    pub pixels: Vec<u8>,
}

impl Image {
    /// `width` x `height` bitmap filled with `rgba`.
    pub fn solid(rel_id: &'static str, width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for chunk in pixels.as_chunks_mut::<4>().0 {
            *chunk = rgba;
        }
        Self { rel_id, pixels }
    }
}

/// Asset store that records every `get` hint, so the ports can assert the
/// decode plan the reference inspects through `decode.mock.calls` options.
#[derive(Default)]
pub struct RecordingStore {
    images: Vec<(String, u32, u32, Vec<u8>)>,
    requests: Mutex<Vec<String>>,
}

impl RecordingStore {
    pub fn new(images: Vec<(Image, u32, u32)>) -> Self {
        Self {
            images: images
                .into_iter()
                .map(|(image, width, height)| (image.rel_id.to_string(), width, height, image.pixels))
                .collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("request log").clone()
    }
}

impl AssetStore for RecordingStore {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let label = match hint {
            SizeHint::Native => format!("native:{}", rel_id),
            SizeHint::AtMost { width, height } => format!("atmost_{}x{}:{}", width, height, rel_id),
        };
        self.requests.lock().expect("request log").push(label);
        let (_, width, height, pixels) = self
            .images
            .iter()
            .find(|(path, ..)| path == rel_id)
            .ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        Ok(DecodedImage {
            width: *width,
            height: *height,
            pixels: Arc::from(pixels.clone().into_boxed_slice()),
        })
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

/// Rasterize a compiled slide at the reference's 960 CSS px width.
pub fn rasterize(slide: &VectorSlide, store: &RecordingStore) -> RgbaSlide {
    TinySkiaRasterizer::new()
        .rasterize(
            slide,
            &RenderTargetSpec {
                width_px: 960,
                height_px: 720,
            },
            store,
        )
        .expect("rasterize")
}

/// One straight RGBA pixel of a rendered slide.
pub fn pixel(rendered: &RgbaSlide, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * rendered.width + x) * 4) as usize;
    [
        rendered.pixels[i],
        rendered.pixels[i + 1],
        rendered.pixels[i + 2],
        rendered.pixels[i + 3],
    ]
}

/// The compiled image fills of a slide, in command order.
pub fn image_fills(commands: &[VectorCommand]) -> Vec<office_terminal_core::contracts::VectorImageFill> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                fill: Some(VectorPaint::Image(fill)),
                ..
            } => Some((**fill).clone()),
            _ => None,
        })
        .collect()
}

/// The `a:pattFill` brush the compiler must produce (§20.1.8.40).
pub fn pattern_fill(preset: &str, fg: [u8; 4], bg: [u8; 4]) -> VectorPattern {
    VectorPattern {
        preset: preset.to_string(),
        fg,
        bg,
    }
}

/// The first `DrawPath` geometry of a compiled slide.
pub fn first_path(commands: &[VectorCommand]) -> VectorPath {
    commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawPath { path, .. } => Some(path.clone()),
            _ => None,
        })
        .expect("a DrawPath")
}
