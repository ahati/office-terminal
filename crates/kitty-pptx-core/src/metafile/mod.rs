//! Metafile picture playback (EMF / WMF / EMF+).
//!
//! Design: [PRD 13](../../../docs/13-metafile-pictures-and-vector-playback.md).
//! This module owns the renderer-neutral seam between the display list and a
//! playback engine. The engine itself (currently a stub) lives in
//! [`engine_metafile_rs`] and is the only place that may name engine types.
//!
//! Flow:
//! 1. `compiler::pictures` classifies a blip and emits `VectorCommand::DrawMetafile`.
//! 2. `expand_metafiles` resolves the marker through [`cache::MetafileResolver`].
//! 3. The engine plays into a [`MetafileSink`]; [`recorder::MetafileRecorder`]
//!    turns the sink calls into `VectorCommand`s.
//! 4. Failure degrades to the standard metafile placeholder plus a warning.

pub mod cache;
pub mod classify;
pub mod diagnostics;
pub mod engine_metafile_rs;
pub mod expand;
pub mod limits;
pub mod mapping;
pub mod recorder;

use crate::contracts::{
    AffineTransform, FillRule, FidelityClass, PathSegment, RectF32, VectorPath, WarningEvent,
};

pub use cache::{clear_global_cache, global_resolver, CompiledMetafile, MetafileResolver};
pub use classify::{classify_bytes, classify_path, is_metafile_path};
pub use diagnostics::MetafileDiagnostic;
pub use expand::{expand_metafiles, MetafileExpansionSummary};
pub use limits::MetafileLimits;

/// Container format detected for a metafile part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetafileFormat {
    Emf,
    Wmf,
    /// gzip-wrapped EMF (`*.emz`).
    Emz,
    /// gzip-wrapped WMF (`*.wmz`).
    Wmz,
}

/// Engine-reported metadata for one metafile.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafileInfo {
    pub format: MetafileFormat,
    /// Drawing bounds in the engine's output units.
    pub bounds: Option<RectF32>,
    /// Physical size in 0.01 mm units, when the header carries it.
    pub physical_size: Option<(f64, f64)>,
}

/// Playback result forwarded to the caller.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackOutcome {
    pub info: MetafileInfo,
    pub diagnostics: Vec<MetafileDiagnostic>,
}

/// Errors surfaced by the metafile engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetafileError {
    /// No playback engine is linked (skeleton state before the engine lands).
    Unavailable,
    /// The container is malformed or truncated.
    Invalid(String),
    /// A record or feature the engine cannot model.
    Unsupported(String),
    /// A resource limit was exceeded.
    Limit(String),
    /// The part could not be read from the archive.
    Asset(String),
}

impl std::fmt::Display for MetafileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "metafile engine unavailable"),
            Self::Invalid(reason) => write!(f, "invalid metafile: {reason}"),
            Self::Unsupported(reason) => write!(f, "unsupported metafile feature: {reason}"),
            Self::Limit(reason) => write!(f, "metafile resource limit: {reason}"),
            Self::Asset(reason) => write!(f, "metafile asset error: {reason}"),
        }
    }
}

impl std::error::Error for MetafileError {}

/// A pen resolved by the engine, in output units.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafilePen {
    pub color: [u8; 4],
    /// Width in output units (`0.0` = cosmetic, one device pixel).
    pub width: f32,
    pub cosmetic: bool,
    /// Dash pattern in pen-width multiples.
    pub dash: Vec<f32>,
    pub line_cap: crate::contracts::LineCap,
    pub line_join: crate::contracts::LineJoin,
    pub miter_limit: f32,
}

impl Default for MetafilePen {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            width: 1.0,
            cosmetic: true,
            dash: Vec::new(),
            line_cap: crate::contracts::LineCap::Butt,
            line_join: crate::contracts::LineJoin::Miter,
            miter_limit: 10.0,
        }
    }
}

/// A brush/paint resolved by the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum MetafilePaint {
    Solid([u8; 4]),
    None,
    /// Linear gradient in output coordinates.
    LinearGradient {
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, [u8; 4])>,
    },
}

/// A path resolved by the engine, in output coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafilePath {
    pub segments: Vec<PathSegment>,
    pub fill_rule: FillRule,
}

impl MetafilePath {
    pub fn new(segments: Vec<PathSegment>, fill_rule: FillRule) -> Self {
        Self {
            segments,
            fill_rule,
        }
    }
}

/// A text run resolved by the engine, in output coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafileTextRun {
    pub text: String,
    pub family: String,
    /// Height in output units.
    pub size: f32,
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike_out: bool,
    /// Baseline origin in output coordinates.
    pub x: f32,
    pub y: f32,
    /// GDI escapement in degrees (0 = horizontal).
    pub escapement_deg: f32,
    /// Optional per-character advances in output units.
    pub dx: Vec<f32>,
}

/// A bitmap decoded by the engine, in output coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafileBitmap {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8 pixels (`width * height * 4`).
    pub rgba: Vec<u8>,
    /// Maps the bitmap's `[0, 1]` square into output coordinates.
    pub placement: AffineTransform,
    pub opacity: f32,
    pub sampling: crate::contracts::BitmapSampling,
}

/// A clipping region resolved by the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafileClip {
    pub path: VectorPath,
}

/// Renderer-neutral sink. Coordinates are in the engine's output units.
pub trait MetafileSink {
    fn push_clip(&mut self, clip: &MetafileClip) -> Result<(), MetafileError>;
    fn pop_clip(&mut self);
    fn draw_path(
        &mut self,
        path: &MetafilePath,
        fill: Option<&MetafilePaint>,
        stroke: Option<&MetafilePen>,
    ) -> Result<(), MetafileError>;
    fn draw_text(&mut self, run: &MetafileTextRun) -> Result<(), MetafileError>;
    fn draw_bitmap(&mut self, bitmap: &MetafileBitmap) -> Result<(), MetafileError>;
    fn diagnostic(&mut self, diagnostic: MetafileDiagnostic);
}

/// Swappable playback backend.
pub trait MetafileEngine: Send + Sync {
    fn format(&self, bytes: &[u8]) -> Option<MetafileFormat>;
    fn inspect(&self, bytes: &[u8]) -> Result<MetafileInfo, MetafileError>;
    fn play(
        &self,
        bytes: &[u8],
        limits: &MetafileLimits,
        sink: &mut dyn MetafileSink,
    ) -> Result<PlaybackOutcome, MetafileError>;

    /// Coordinate space the primitives are delivered in.
    ///
    /// The engine maps EMF window/viewport transforms before calling the
    /// renderer, so playback coordinates need not match the header bounds.
    /// Engines that cannot report it up front fall back to `inspect` bounds.
    fn output_bounds(
        &self,
        bytes: &[u8],
        limits: &MetafileLimits,
    ) -> Result<Option<RectF32>, MetafileError> {
        let _ = limits;
        Ok(self.inspect(bytes)?.bounds)
    }
}

/// Sink that discards every primitive; used for the bounds probe.
pub(crate) struct NullMetafileSink;

impl MetafileSink for NullMetafileSink {
    fn push_clip(&mut self, _clip: &MetafileClip) -> Result<(), MetafileError> {
        Ok(())
    }
    fn pop_clip(&mut self) {}
    fn draw_path(
        &mut self,
        _path: &MetafilePath,
        _fill: Option<&MetafilePaint>,
        _stroke: Option<&MetafilePen>,
    ) -> Result<(), MetafileError> {
        Ok(())
    }
    fn draw_text(&mut self, _run: &MetafileTextRun) -> Result<(), MetafileError> {
        Ok(())
    }
    fn draw_bitmap(&mut self, _bitmap: &MetafileBitmap) -> Result<(), MetafileError> {
        Ok(())
    }
    fn diagnostic(&mut self, _diagnostic: MetafileDiagnostic) {}
}

/// Maps an engine diagnostic class onto the public warning taxonomy.
pub(crate) fn warning_for(
    element_id: String,
    diagnostic: &MetafileDiagnostic,
) -> WarningEvent {
    match diagnostic.class {
        FidelityClass::Placeholder | FidelityClass::Omitted => WarningEvent::MediaFailed {
            rel_id: element_id,
            reason: diagnostic.detail.clone(),
        },
        _ => WarningEvent::FidelityDegraded {
            element_id,
            feature: diagnostic.tag,
            class: diagnostic.class,
        },
    }
}
