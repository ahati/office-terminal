//! Metafile picture tests (PRD 13 / 13a).
//!
//! Covers the compiler→expansion seam and the vendored `metafile-rs` engine
//! (EMF, EMF+ Only, WMF) using small upstream fixtures.

use std::sync::Arc;

use kitty_pptx_core::contracts::{
    AffineTransform, BitmapSampling, RenderTargetSpec, VectorCommand, VectorSlide, WarningEvent,
};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::metafile::{self, mapping};
use kitty_pptx_core::{
    DefaultSlideCompiler, DefaultSlideSvgSerializer, SlideCompiler, SlideRasterizer,
    SlideSvgSerializer, SvgTextMode, TinySkiaRasterizer,
};
use kitty_pptx_core::AssetError;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/metafile");

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

struct FixtureStore {
    rel_id: String,
    bytes: Vec<u8>,
}

impl FixtureStore {
    fn new(rel_id: &str, bytes: Vec<u8>) -> Self {
        Self {
            rel_id: rel_id.to_string(),
            bytes,
        }
    }
}

impl AssetStore for FixtureStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::UnsupportedFormat("metafile".into()))
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        if rel_id == self.rel_id {
            Ok(self.bytes.clone())
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

fn picture_slide(part: &str, mime: &str) -> pptx_model::Slide {
    let picture: pptx_model::PictureElement = serde_json::from_value(serde_json::json!({
        "x": 0, "y": 0, "width": 9_144_000, "height": 6_858_000,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "imagePath": part,
        "mimeType": mime
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

fn compile_slide(slide: &pptx_model::Slide) -> VectorSlide {
    let compiler = DefaultSlideCompiler::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    compiler
        .compile(slide, 9_144_000, 6_858_000, &tx)
        .expect("compile")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURE_DIR}/{name}")).expect("fixture")
}

fn markers(scene: &VectorSlide) -> usize {
    scene
        .commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawMetafile { .. }))
        .count()
}

fn foreground_pixels(scene: &VectorSlide) -> usize {
    let target = RenderTargetSpec {
        width_px: 240,
        height_px: 180,
    };
    let bitmap = TinySkiaRasterizer::new()
        .rasterize(scene, &target, &NullStore)
        .expect("rasterize");
    // The rasterizer clears the canvas to opaque white; a full-bleed picture
    // must not be used as its own background reference.
    bitmap
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel != [255, 255, 255, 255])
        .count()
}

#[test]
fn compiler_emits_a_metafile_marker() {
    let scene = compile_slide(&picture_slide("ppt/media/image1.emf", "image/x-emf"));
    assert_eq!(markers(&scene), 1);
    let draws = scene
        .commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawImage { .. }))
        .count();
    assert_eq!(draws, 0, "metafiles must not compile to DrawImage");
}

#[test]
fn empty_metafile_expands_to_a_placeholder() {
    metafile::clear_global_cache();
    let store = FixtureStore::new("ppt/media/image1.emf", minimal_emf());
    let mut scene = compile_slide(&picture_slide("ppt/media/image1.emf", "image/x-emf"));
    let (tx, rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.expanded, 0, "no drawing records → no ink");
    assert_eq!(summary.placeholders, 1);
    assert_eq!(markers(&scene), 0);
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
    assert!(rx
        .try_iter()
        .any(|warning| matches!(warning, WarningEvent::MediaFailed { .. })));
}

#[test]
fn expansion_keeps_non_metafile_commands() {
    metafile::clear_global_cache();
    let store = FixtureStore::new("ppt/media/image1.emf", minimal_emf());
    let mut scene = compile_slide(&picture_slide("ppt/media/image1.emf", "image/x-emf"));
    let before = scene.commands.len();
    let (tx, _rx) = crossbeam_channel::unbounded::<WarningEvent>();

    metafile::expand_metafiles(&mut scene, &store, &tx);

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
}

#[test]
fn real_emf_paths_play_into_commands() {
    metafile::clear_global_cache();
    let store = FixtureStore::new(
        "ppt/media/image1.emf",
        fixture("windows-emf-paths.emf"),
    );
    let mut scene = compile_slide(&picture_slide("ppt/media/image1.emf", "image/x-emf"));
    let (tx, _rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.expanded, 1, "real EMF must play");
    assert_eq!(summary.placeholders, 0);
    assert_eq!(markers(&scene), 0);
    assert!(!scene.commands.is_empty());
    assert!(foreground_pixels(&scene) > 0, "playback must paint ink");
}

#[test]
fn real_emf_bitmap_uses_draw_bitmap() {
    metafile::clear_global_cache();
    let store = FixtureStore::new(
        "ppt/media/image1.emf",
        fixture("windows-emf-bitmap.emf"),
    );
    let mut scene = compile_slide(&picture_slide("ppt/media/image1.emf", "image/x-emf"));
    let (tx, _rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.expanded, 1);
    let bitmaps = scene
        .commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawBitmap { .. }))
        .count();
    assert!(bitmaps > 0, "bitmap records must compile to DrawBitmap");
}

#[test]
fn real_emfplus_only_plays() {
    metafile::clear_global_cache();
    let store = FixtureStore::new(
        "ppt/media/image1.emf",
        fixture("windows-gdiplus-emfplus-only.emf"),
    );
    let mut scene = compile_slide(&picture_slide("ppt/media/image1.emf", "image/x-emf"));
    let (tx, _rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.expanded, 1, "EMF+ Only stream must play");
    assert_eq!(summary.placeholders, 0);
    assert_eq!(markers(&scene), 0);
}

#[test]
fn real_wmf_plays() {
    metafile::clear_global_cache();
    let store = FixtureStore::new("ppt/media/image1.wmf", fixture("windows-gdi-vector.wmf"));
    let mut scene = compile_slide(&picture_slide("ppt/media/image1.wmf", "image/x-wmf"));
    let (tx, _rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);

    assert_eq!(summary.expanded, 1, "WMF must play");
    assert_eq!(summary.placeholders, 0);
    assert_eq!(markers(&scene), 0);
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
    assert!(svg.contains("<image"), "expected an <image> element");
    assert!(svg.contains("data:image/png;base64,"));
    assert!(svg.contains("preserveAspectRatio=\"none\""));
}

#[test]
fn unexpanded_marker_never_paints_nothing() {
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
    assert!(foreground_pixels(&scene) > 0);
}
