//! `oxml_layout::PageFrame` → `VectorSlide` compilation entry (PRD 15 §5).
//!
//! Geometry arrives in typographic points; the display list is EMU
//! (`1 pt = 12 700 EMU`). Accumulated group transforms are converted once
//! (linear coefficients kept, translations scaled) and carried on each leaf
//! command, preserving gradient orientation and text rotation. Accumulation
//! happens in f64 point space per level and converts at the leaf — rounded
//! values are never fed back into further math.

pub(crate) mod group;
pub(crate) mod image;
pub(crate) mod link;
pub(crate) mod page;
pub(crate) mod paint;
pub(crate) mod path;
pub(crate) mod text;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use office_terminal_core::contracts::{AffineTransform, FidelityClass, VectorSlide, WarningEvent};
use office_terminal_core::RenderError;
use oxml_layout::{PageFrame, PositionedElement};

use crate::engine::DocxEngine;
use crate::media::DocxAssetStore;

/// Shared per-compile context: engine lookups, media, warnings.
pub(crate) struct BridgeContext<'a> {
    pub(crate) engine: &'a dyn DocxEngine,
    pub(crate) media: &'a DocxAssetStore,
    pub(crate) warnings: &'a Sender<WarningEvent>,
    /// Feature-level dedup shared with the page source so a warning fires
    /// once per document, not once per re-compiled surface.
    pub(crate) emitted: &'a Mutex<HashSet<&'static str>>,
}

impl BridgeContext<'_> {
    /// Emit a fidelity warning once per feature per document.
    pub(crate) fn warn_once(&self, feature: &'static str, element_id: String) {
        {
            let mut emitted = self.emitted.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if !emitted.insert(feature) {
                return;
            }
        }
        let _ = self.warnings.send(WarningEvent::FidelityDegraded {
            element_id,
            feature,
            class: FidelityClass::Degraded,
        });
    }

    /// Emit a one-off event without feature dedup (e.g. media failures keyed
    /// by rel id).
    pub(crate) fn warn(&self, event: WarningEvent) {
        let _ = self.warnings.send(event);
    }
}

/// Mutable per-compile state threaded through the recursion.
pub(crate) struct BridgeState {
    /// A group-level shadow awaiting the next emitted path (issue #40
    /// §7.7.6: attach to the single path child, warn when no path consumes it).
    pub(crate) pending_shadow: Option<office_terminal_core::contracts::VectorOuterShadow>,
}

/// Compile one laid-out page into a resolution-independent display list.
pub fn compile_page(
    page: &PageFrame,
    engine: &dyn DocxEngine,
    media: &DocxAssetStore,
    warnings: &Sender<WarningEvent>,
    emitted: Arc<Mutex<HashSet<&'static str>>>,
) -> Result<VectorSlide, RenderError> {
    let mut slide = VectorSlide::new(page_width_emu(page), page_height_emu(page));
    // The page source already flushed engine diagnostics; the shared dedup
    // set rides along so page-level features warn once per document across
    // every surface that re-compiles a page.
    let ctx = BridgeContext {
        engine,
        media,
        warnings,
        emitted: emitted.as_ref(),
    };
    let mut state = BridgeState { pending_shadow: None };
    if let Some(background) = page.background.as_ref() {
        slide.background = page::map_background(background, &ctx);
    }
    map_elements(
        &page.elements,
        &AffineTransform::identity(),
        1.0,
        &mut slide,
        &ctx,
        &mut state,
    );
    Ok(slide)
}

fn page_width_emu(page: &PageFrame) -> i64 {
    (finite(page.width) * 12_700.0).round() as i64
}

fn page_height_emu(page: &PageFrame) -> i64 {
    (finite(page.height) * 12_700.0).round() as i64
}

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// Points → EMU (f32, the display list's working precision).
pub(crate) fn pt_to_emu(pt: f64) -> f32 {
    (finite(pt) * 12_700.0).round() as f32
}

/// `oxml_layout::Transform` (points, PDF coefficient order) → core
/// `AffineTransform` (EMU): linear coefficients are unitless scale/shear and
/// stay verbatim; translations scale with the unit change.
pub(crate) fn core_transform(transform: &oxml_layout::Transform) -> AffineTransform {
    AffineTransform {
        sx: finite(transform.a) as f32,
        ky: finite(transform.b) as f32,
        kx: finite(transform.c) as f32,
        sy: finite(transform.d) as f32,
        tx: pt_to_emu(transform.e),
        ty: pt_to_emu(transform.f),
    }
}

pub(crate) fn map_elements(
    elements: &[PositionedElement],
    acc: &AffineTransform,
    opacity: f32,
    slide: &mut VectorSlide,
    ctx: &BridgeContext<'_>,
    state: &mut BridgeState,
) {
    for element in elements {
        match element {
            PositionedElement::Text(run) => {
                text::map_glyph_run(run, acc, opacity, slide, ctx);
            }
            PositionedElement::MultilingualText(run) => {
                text::map_multilingual_run(run, acc, opacity, slide, ctx);
            }
            PositionedElement::Line {
                start,
                end,
                width,
                color,
                dash_pattern,
            } => {
                path::map_line(start, end, *width, color, *dash_pattern, acc, opacity, slide);
            }
            PositionedElement::FilledRect { rect, color } => {
                path::map_filled_rect(rect, color, acc, opacity, slide);
            }
            PositionedElement::Image {
                rect,
                content_type,
                media_id,
                ..
            } => {
                image::map_image(rect, content_type, *media_id, acc, opacity, slide, ctx);
            }
            PositionedElement::LinkAnnotation { rect, url } => {
                link::map_link(rect, url, acc, slide, ctx);
            }
            PositionedElement::Path(path_element) => {
                path::map_path(path_element, acc, opacity, slide, ctx, state);
            }
            PositionedElement::Group(group) => {
                group::map_group(group, acc, opacity, slide, ctx, state);
            }
            PositionedElement::MarkedContent { children, .. } => {
                // Tagged-PDF semantics carry no ink; recurse for the children.
                map_elements(children, acc, opacity, slide, ctx, state);
            }
            // `PositionedElement` is `#[non_exhaustive]`: an unknown leaf
            // is an honest degradation with a warning, never a silent hole
            // and never a failed page (issue #40 §7.7.8).
            other => {
                let kind = element_kind(other);
                ctx.warn_once("docx-element-unknown", format!("page-element-{kind}"));
            }
        }
    }
}

fn element_kind(element: &PositionedElement) -> &'static str {
    match element {
        PositionedElement::Text(_) => "text",
        PositionedElement::MultilingualText(_) => "multilingual-text",
        PositionedElement::Line { .. } => "line",
        PositionedElement::FilledRect { .. } => "filled-rect",
        PositionedElement::Image { .. } => "image",
        PositionedElement::LinkAnnotation { .. } => "link",
        PositionedElement::Path(_) => "path",
        PositionedElement::Group(_) => "group",
        PositionedElement::MarkedContent { .. } => "marked-content",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{MediaBytes, ResolvedFont};
    use oxml_layout::{FontId, MediaId};

    struct FixtureEngine;

    impl DocxEngine for FixtureEngine {
        fn page_count(&self) -> usize {
            1
        }
        fn page(&self, index: usize) -> Option<Arc<PageFrame>> {
            (index == 0).then(|| Arc::new(PageFrame::new(1, 612.0, 792.0, Vec::new())))
        }
        fn resolve_font(&self, _id: FontId) -> Option<&ResolvedFont> {
            None
        }
        fn media_ids(&self) -> Vec<MediaId> {
            Vec::new()
        }
        fn media(&self, _id: MediaId) -> Option<MediaBytes> {
            None
        }
        fn outlines(&self) -> &[oxml_layout::OutlineEntry] {
            &[]
        }
        fn bookmark_page(&self, _name: &str) -> Option<usize> {
            None
        }
        fn diagnostics(&self) -> &[String] {
            &[]
        }
        fn metadata(&self) -> Option<&oxml_layout::DocumentMetadata> {
            None
        }
    }

    #[test]
    fn page_geometry_converts_to_emu() {
        let page = PageFrame::new(1, 612.0, 792.0, Vec::new());
        let engine = FixtureEngine;
        let media = DocxAssetStore::from_engine(&engine);
        let (tx, _rx) = crossbeam_channel::unbounded();
        let slide = compile_page(&page, &engine, &media, &tx, Arc::new(Mutex::new(HashSet::new()))).expect("compile");
        assert_eq!(slide.width_emu, 612 * 12_700);
        assert_eq!(slide.height_emu, 792 * 12_700);
    }

    #[test]
    fn non_finite_geometry_degrades_to_zero_not_nan() {
        let mut page = PageFrame::new(1, 612.0, 792.0, Vec::new());
        page.width = f64::NAN;
        page.height = f64::INFINITY;
        let slide = compile_page(
            &page,
            &FixtureEngine,
            &DocxAssetStore::from_engine(&FixtureEngine),
            &crossbeam_channel::unbounded().0,
            Arc::new(Mutex::new(HashSet::new())),
        )
        .expect("compile");
        assert_eq!(slide.width_emu, 0);
        assert_eq!(slide.height_emu, 0);
    }

    #[test]
    fn core_transform_converts_translations_and_keeps_linear_terms() {
        let transform = oxml_layout::Transform {
            a: 2.0,
            b: 0.0,
            c: 0.5,
            d: 3.0,
            e: 72.0,
            f: 96.0,
        };
        let core = core_transform(&transform);
        assert_eq!(core.sx, 2.0);
        assert_eq!(core.kx, 0.5);
        assert_eq!(core.sy, 3.0);
        assert_eq!(core.tx, 72.0 * 12_700.0);
        assert_eq!(core.ty, 96.0 * 12_700.0);
    }
}
