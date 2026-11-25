//! M0: pipeline warning plumbing (PRD 13 §10.5).
//!
//! Compile/expansion diagnostics used to be written into throwaway channels.
//! `DefaultRenderPipeline` now owns a buffer and exposes `drain_warnings`, so
//! the TUI warnings drawer and the CLI can see fidelity warnings.

use std::sync::Arc;
use std::time::Duration;

use office_terminal_core::contracts::{PipelineEntry, PrefetchPlan, RenderTargetSpec};
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::pipeline::DefaultRenderPipeline;
use office_terminal_core::{AssetError, DefaultSlideCompiler, RenderPipeline, TinySkiaRasterizer, WarningEvent};

struct FixtureStore {
    bytes: Vec<u8>,
}

impl AssetStore for FixtureStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::UnsupportedFormat(rel_id.to_string()))
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        if rel_id.ends_with(".emf") {
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

/// Empty EMF (header + EOF): playback succeeds but paints nothing, so
/// expansion emits the placeholder + `MediaFailed`.
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

fn presentation() -> Arc<pptx_model::Presentation> {
    let picture: pptx_model::PictureElement = serde_json::from_value(serde_json::json!({
        "x": 0, "y": 0, "width": 9_144_000, "height": 6_858_000,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "imagePath": "ppt/media/image1.emf",
        "mimeType": "image/x-emf"
    }))
    .expect("picture element");
    let slide = pptx_model::Slide {
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
    };
    Arc::new(pptx_model::Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides: vec![slide],
        default_text_color: Some("000000".into()),
        major_font: Some("Calibri".into()),
        minor_font: Some("Calibri".into()),
        hlink_color: None,
        fol_hlink_color: None,
    })
}

fn pipeline() -> DefaultRenderPipeline {
    DefaultRenderPipeline::new(
        presentation(),
        Arc::new(FixtureStore { bytes: minimal_emf() }),
        Arc::new(DefaultSlideCompiler::new()),
        Arc::new(TinySkiaRasterizer::new()),
    )
}

fn target() -> RenderTargetSpec {
    RenderTargetSpec {
        width_px: 320,
        height_px: 180,
    }
}

fn assert_metafile_warning(warnings: &[(usize, WarningEvent)]) {
    assert!(
        warnings.iter().any(|(slide, event)| {
            *slide == 0
                && matches!(
                    event,
                    WarningEvent::MediaFailed { rel_id, .. } if rel_id == "ppt/media/image1.emf"
                )
        }),
        "expected a MediaFailed warning for slide 0, got {warnings:?}"
    );
}

#[test]
fn sync_path_surfaces_compile_warnings() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &target()).expect("render");

    let warnings = pipeline.drain_warnings();
    assert_metafile_warning(&warnings);
    assert!(pipeline.drain_warnings().is_empty(), "drain must empty the buffer");
}

#[test]
fn worker_path_surfaces_compile_warnings() {
    let pipeline = Arc::new(pipeline());
    pipeline.prefetch(
        &PrefetchPlan {
            center: 0,
            behind: 0,
            ahead: 0,
            stage: false,
        },
        &target(),
    );

    let mut ready = false;
    for _ in 0..500 {
        if matches!(pipeline.entry_at(0, &target()), PipelineEntry::Ready { .. }) {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(ready, "worker must produce the first scene");

    let warnings = pipeline.drain_warnings();
    assert_metafile_warning(&warnings);
}

#[test]
fn cache_hits_do_not_replay_warnings() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &target()).expect("render");
    let _ = pipeline.drain_warnings();

    // Cached scene: no compile, therefore no new warning events.
    pipeline.render_sync(0, &target()).expect("cached render");
    assert!(pipeline.drain_warnings().is_empty());
}
