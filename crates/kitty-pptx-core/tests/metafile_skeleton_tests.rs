//! Skeleton tests for metafile pictures (PRD 13 / 13a).
//!
//! The engine is not wired yet, so expansion must degrade every metafile to the
//! standard placeholder plus a warning — never an invisible hole.

use std::sync::Arc;

use kitty_pptx_core::contracts::{
    AffineTransform, BitmapSampling, RenderTargetSpec, VectorCommand, VectorSlide, WarningEvent,
};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::AssetError;
use kitty_pptx_core::metafile::{self, mapping};
use kitty_pptx_core::{
    DefaultSlideCompiler, DefaultSlideSvgSerializer, SlideCompiler, SlideRasterizer,
    SlideSvgSerializer, SvgTextMode, TinySkiaRasterizer,
};

/// Smallest well-formed EMF: EMR_HEADER (88 bytes) + EMR_EOF (20 bytes).
fn minimal_emf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&88u32.to_le_bytes());
    for value in [0i32, 0, 100, 100, 0, 0, 100, 100] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(b" EMF");
    data.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    data.extend_from_slice(&108u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 16]);
    data.resize(88, 0);
    data.extend_from_slice(&14u32.to_le_bytes());
    data.extend_from_slice(&20u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&20u32.to_le_bytes());
    data
}

struct EmfStore {
    emf: Vec<u8>,
}

impl AssetStore for EmfStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::UnsupportedFormat("emf".into()))
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        if rel_id.ends_with(".emf") {
            Ok(self.emf.clone())
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

struct NullStore;

impl AssetStore for NullStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn emf_slide() -> pptx_model::Slide {
    let picture: pptx_model::PictureElement = serde_json::from_value(serde_json::json!({
        "x": 0, "y": 0, "width": 9_144_000, "height": 6_858_000,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "imagePath": "ppt/media/image1.emf",
        "mimeType": "image/x-emf"
    }))
    .expect("picture element");
    pptx_model::Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: None,
        background: None,
        elements: vec![pptx_model::SlideElement::Picture(picture)],
        element_sources: vec![pptx_model::SlideElementSource {
            origin: pptx_model::SlideElementOrigin::Slide,
        }],
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    }
}

fn compile_emf_slide() -> VectorSlide {
    let compiler = DefaultSlideCompiler::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    compiler
        .compile(&emf_slide(), 9_144_000, 6_858_000, &tx)
        .expect("compile")
}

#[test]
fn compiler_emits_a_metafile_marker() {
    let scene = compile_emf_slide();
    let markers: Vec<&str> = scene
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawMetafile { rel_id, .. } => Some(rel_id.as_str()),
            _ => None,
        })
        .collect();
    let draws: Vec<&str> = scene
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawImage { rel_id, .. } => Some(rel_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(markers, vec!["ppt/media/image1.emf"]);
    assert!(draws.is_empty(), "metafiles must not compile to DrawImage");
}

#[test]
fn expansion_replaces_the_marker_with_a_placeholder() {
    metafile::clear_global_cache();
    let store = EmfStore { emf: minimal_emf() };
    let mut scene = compile_emf_slide();
    let (tx, rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.expanded, 0, "engine skeleton cannot play yet");
    assert_eq!(summary.placeholders, 1);

    let markers = scene
        .commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawMetafile { .. }))
        .count();
    assert_eq!(markers, 0, "no marker may survive expansion");

    let texts: Vec<&str> = scene
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => runs.first().map(|run| run.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|text| text.contains("METAFILE")),
        "expected the §3 metafile placeholder, got {texts:?}"
    );

    let warnings: Vec<WarningEvent> = rx.try_iter().collect();
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            WarningEvent::MediaFailed { rel_id, .. } if rel_id.ends_with(".emf")
        )),
        "expected a MediaFailed warning, got {warnings:?}"
    );
}

#[test]
fn expansion_keeps_non_metafile_commands() {
    metafile::clear_global_cache();
    let store = EmfStore { emf: minimal_emf() };
    let mut scene = compile_emf_slide();
    let before = scene.commands.len();
    let (tx, _rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.placeholders, 1);
    assert!(scene.commands.len() > before, "placeholder must emit commands");
}

#[test]
fn source_to_emu_maps_bounds_into_the_destination() {
    let bounds = kitty_pptx_core::contracts::RectF32 {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 100.0,
    };
    let src = kitty_pptx_core::contracts::NormalizedRect::default();
    let dest = kitty_pptx_core::contracts::RectF32 {
        x: 0.0,
        y: 0.0,
        w: 1000.0,
        h: 500.0,
    };
    let transform = mapping::source_to_emu(&bounds, &src, &dest).expect("mapping");
    assert_eq!(transform.transform_point(50.0, 50.0), (500.0, 250.0));
    assert!(mapping::source_to_emu(
        &bounds,
        &src,
        &kitty_pptx_core::contracts::RectF32 {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        }
    )
    .is_none());
}

fn red_png() -> Arc<[u8]> {
    let mut image = image::RgbaImage::new(2, 2);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([255, 0, 0, 255]);
    }
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png encode");
    Arc::from(png.into_boxed_slice())
}

fn bitmap_scene() -> VectorSlide {
    let mut scene = VectorSlide::new(100, 100);
    scene.commands.push(VectorCommand::DrawBitmap {
        png: red_png(),
        width: 2,
        height: 2,
        placement: AffineTransform::scale(100.0, 100.0),
        opacity: 1.0,
        sampling: BitmapSampling::Linear,
    });
    scene
}

#[test]
fn draw_bitmap_paints_pixels() {
    let scene = bitmap_scene();
    let target = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let bitmap = TinySkiaRasterizer::new()
        .rasterize(&scene, &target, &NullStore)
        .expect("rasterize");
    let index = ((50 * 100) + 50) * 4;
    assert_eq!(
        &bitmap.pixels[index..index + 4],
        &[255, 0, 0, 255],
        "embedded bitmap must paint at the placement"
    );
}

#[test]
fn draw_bitmap_serializes_as_a_data_uri() {
    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&bitmap_scene(), &NullStore, SvgTextMode::SelectableText)
        .expect("serialize");
    assert!(svg.contains("<image"), "expected an <image> element: {svg}");
    assert!(
        svg.contains("data:image/png;base64,"),
        "expected an inline data URI"
    );
    assert!(svg.contains("preserveAspectRatio=\"none\""));
}

#[test]
fn unexpanded_marker_never_paints_nothing() {
    // A backend given a raw marker (pipeline bug) must still leave visible ink.
    let mut scene = VectorSlide::new(100, 100);
    scene.commands.push(VectorCommand::DrawMetafile {
        rel_id: "ppt/media/image1.emf".to_string(),
        transform: AffineTransform::identity(),
        src_rect: kitty_pptx_core::contracts::NormalizedRect::default(),
        dest_rect: kitty_pptx_core::contracts::RectF32 {
            x: 10.0,
            y: 10.0,
            w: 80.0,
            h: 80.0,
        },
        opacity: 1.0,
    });
    let target = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let bitmap = TinySkiaRasterizer::new()
        .rasterize(&scene, &target, &NullStore)
        .expect("rasterize");
    let background: [u8; 4] = bitmap.pixels[0..4].try_into().expect("rgba");
    let foreground = bitmap
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel != background)
        .count();
    assert!(
        foreground > 0,
        "an unexpanded metafile marker must not render as a blank region"
    );
}
