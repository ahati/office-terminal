//! `metafile-rs` playback backend (vendored under `crates/metafile-*`).
//!
//! This is the only file that names the engine crates: a
//! [`metafile_core::Renderer`] implementation forwards every primitive to our
//! [`MetafileSink`], which records display-list commands (PRD 13 §3.3).

use metafile_core as mc;

use crate::contracts::{FillRule, RectF32};

use super::{
    MetafileDiagnostic, MetafileEngine, MetafileError, MetafileFormat, MetafileInfo, MetafileLimits,
    MetafileSink, PlaybackOutcome,
};

mod mapping;
mod primitives;
mod renderer;

use renderer::SinkRenderer;

/// `metafile-rs`-backed playback engine.
#[derive(Default)]
pub struct MetafileRsEngine;

impl MetafileEngine for MetafileRsEngine {
    fn format(&self, bytes: &[u8]) -> Option<MetafileFormat> {
        super::classify::classify_bytes(bytes)
    }

    fn inspect(&self, bytes: &[u8]) -> Result<MetafileInfo, MetafileError> {
        inspect_bytes(bytes)
    }

    fn output_bounds(
        &self,
        bytes: &[u8],
        limits: &MetafileLimits,
    ) -> Result<Option<RectF32>, MetafileError> {
        // Play once into a discarding sink: the renderer coordinates are only
        // known after the engine applies its window/viewport mapping.
        let mut sink = super::NullMetafileSink;
        let outcome = self.play(bytes, limits, &mut sink)?;
        Ok(outcome.info.bounds)
    }

    fn play(
        &self,
        bytes: &[u8],
        limits: &MetafileLimits,
        sink: &mut dyn MetafileSink,
    ) -> Result<PlaybackOutcome, MetafileError> {
        let bounds = inspect_bytes(bytes).ok().and_then(|info| info.bounds);
        let options = mc::RenderOptions {
            strict: false,
            limits: to_engine_limits(limits),
        };
        let mut renderer = SinkRenderer::new(sink, bounds.map(to_engine_rect));

        if is_emf(bytes) {
            let result = metafile_emf::playback(bytes, &options, &mut renderer).map_err(map_error)?;
            Ok(PlaybackOutcome {
                info: MetafileInfo {
                    format: MetafileFormat::Emf,
                    bounds: result
                        .output_bounds
                        .or(Some(result.metadata.bounds))
                        .map(from_engine_rect),
                    physical_size: Some((
                        result.metadata.frame.width(),
                        result.metadata.frame.height(),
                    )),
                },
                diagnostics: result.diagnostics.into_iter().map(map_diagnostic).collect(),
            })
        } else if is_wmf(bytes) {
            let result = metafile_wmf::playback(bytes, &options, &mut renderer).map_err(map_error)?;
            Ok(PlaybackOutcome {
                info: MetafileInfo {
                    format: MetafileFormat::Wmf,
                    bounds: result
                        .output_bounds
                        .or(result.metadata.bounds)
                        .map(from_engine_rect),
                    physical_size: result.physical_size,
                },
                diagnostics: result.diagnostics.into_iter().map(map_diagnostic).collect(),
            })
        } else {
            Err(MetafileError::Unsupported(
                "unrecognised metafile container".into(),
            ))
        }
    }
}

fn is_emf(bytes: &[u8]) -> bool {
    bytes.len() >= 44 && bytes[0..4] == [0x01, 0x00, 0x00, 0x00] && &bytes[40..44] == b" EMF"
}

fn is_wmf(bytes: &[u8]) -> bool {
    matches!(
        super::classify::classify_bytes(bytes),
        Some(MetafileFormat::Wmf)
    )
}

fn inspect_bytes(bytes: &[u8]) -> Result<MetafileInfo, MetafileError> {
    if is_emf(bytes) {
        let info = metafile_emf::inspect(bytes).map_err(map_error)?;
        Ok(MetafileInfo {
            format: MetafileFormat::Emf,
            bounds: Some(from_engine_rect(info.bounds.normalized())),
            physical_size: Some((info.frame.width(), info.frame.height())),
        })
    } else if is_wmf(bytes) {
        let info = metafile_wmf::inspect(bytes).map_err(map_error)?;
        let bounds = info.bounds.map(from_engine_rect).or_else(|| {
            match (info.estimated_width, info.estimated_height) {
                (Some(width), Some(height)) => Some(RectF32 {
                    x: 0.0,
                    y: 0.0,
                    w: width.max(0.0) as f32,
                    h: height.max(0.0) as f32,
                }),
                _ => None,
            }
        });
        Ok(MetafileInfo {
            format: MetafileFormat::Wmf,
            bounds,
            physical_size: None,
        })
    } else {
        Err(MetafileError::Unsupported(
            "unrecognised metafile container".into(),
        ))
    }
}

fn to_engine_limits(limits: &MetafileLimits) -> mc::ResourceLimits {
    mc::ResourceLimits {
        max_input_bytes: limits.max_input_bytes,
        max_records: limits.max_records,
        max_points_per_record: limits.max_points_per_record,
        max_dc_depth: limits.max_dc_depth,
        max_objects: limits.max_objects,
        max_pixels: limits.max_pixels,
        max_diagnostics: limits.max_diagnostics,
        ..mc::ResourceLimits::default()
    }
}

fn map_error(error: mc::MetafileError) -> MetafileError {
    let text = error.to_string();
    if text.to_ascii_lowercase().contains("resource limit") {
        MetafileError::Limit(text)
    } else {
        MetafileError::Invalid(text)
    }
}

pub(super) fn to_engine_error(error: MetafileError) -> mc::MetafileError {
    mc::MetafileError::UnsupportedCriticalFeature(error.to_string())
}

fn map_diagnostic(diagnostic: mc::Diagnostic) -> MetafileDiagnostic {
    MetafileDiagnostic::new(
        tag_for(&diagnostic.code, &diagnostic.message),
        crate::contracts::FidelityClass::Degraded,
        diagnostic.message,
    )
}

fn tag_for(code: &str, message: &str) -> &'static str {
    let haystack = format!("{code} {message}").to_ascii_lowercase();
    if haystack.contains("rop") {
        "metafile_rop"
    } else if haystack.contains("combine")
        || (haystack.contains("clip")
            && (haystack.contains("union")
                || haystack.contains("xor")
                || haystack.contains("exclude")
                || haystack.contains("complement")))
    {
        "metafile_clip_combine"
    } else if haystack.contains("hatch") {
        "metafile_hatch"
    } else if haystack.contains("pattern") {
        "metafile_pattern_brush"
    } else if haystack.contains("texture") {
        "metafile_texture_transform"
    } else if haystack.contains("bitmap") || haystack.contains("dib") {
        "metafile_bitmap"
    } else if haystack.contains("emf+") || haystack.contains("emfplus") || haystack.contains("dual")
    {
        "metafile_emfplus_fallback"
    } else if haystack.contains("font") {
        "metafile_font"
    } else {
        "metafile_record_ignored"
    }
}

pub(super) fn from_engine_rect(rect: mc::Rect) -> RectF32 {
    let rect = rect.normalized();
    RectF32 {
        x: rect.left as f32,
        y: rect.top as f32,
        w: rect.width() as f32,
        h: rect.height() as f32,
    }
}

pub(super) fn to_engine_rect(rect: RectF32) -> mc::Rect {
    mc::Rect {
        left: f64::from(rect.x),
        top: f64::from(rect.y),
        right: f64::from(rect.x + rect.w),
        bottom: f64::from(rect.y + rect.h),
    }
}

pub(super) fn rgba(value: mc::Color) -> [u8; 4] {
    [value.r, value.g, value.b, value.a]
}

pub(super) fn fill_rule(mode: u16) -> FillRule {
    // GDI: 1 = ALTERNATE (even-odd), 2 = WINDING (non-zero).
    if mode == 2 {
        FillRule::NonZero
    } else {
        FillRule::EvenOdd
    }
}

