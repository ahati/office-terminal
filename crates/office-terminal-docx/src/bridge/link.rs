//! Hyperlink annotations (issue #40 §7.7.7).
//!
//! Every external URL passes the shared sanitizer gate before it can reach
//! OSC 8, SVG or an OS opener; fragment/bare-bookmark targets resolve to
//! physical page indices through the engine. Unresolvable and forbidden
//! links drop with a warning — never silently, never as a URL.

use office_terminal_core::contracts::{
    HyperlinkTarget, RectF32, SlideLink, WarningEvent,
};
use office_terminal_core::sanitize_hyperlink_url;
use oxml_layout::Rect;

use super::{pt_to_emu, BridgeContext};

/// Map one link annotation onto the slide's hyperlink list.
pub(crate) fn map_link(rect: &Rect, url: &str, slide: &mut office_terminal_core::contracts::VectorSlide, ctx: &BridgeContext<'_>) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let bounds = RectF32::new(
        pt_to_emu(rect.x),
        pt_to_emu(rect.y),
        pt_to_emu(rect.width),
        pt_to_emu(rect.height),
    );

    let target = match sanitize_hyperlink_url(url) {
        Ok(clean) => HyperlinkTarget::ExternalUri(clean),
        Err(_) => {
            // Not a valid external URI: the engine projects internal
            // `w:anchor` links as `#bookmark` or a bare bookmark name
            // (issue #40 §7.7.7). Resolve through the engine's placed
            // bookmarks (0-based physical pages).
            let name = url.strip_prefix('#').unwrap_or(url);
            match ctx.engine.bookmark_page(name) {
                Some(page) => HyperlinkTarget::SlideIndex(page),
                None => {
                    ctx.warn(WarningEvent::FidelityDegraded {
                        element_id: format!("link-{name}"),
                        feature: "docx-link-unresolved",
                        class: office_terminal_core::contracts::FidelityClass::Degraded,
                    });
                    return;
                }
            }
        }
    };

    slide.hyperlinks.push(SlideLink::new(target, bounds, None));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{compile, letter_page, FixtureEngine};
    use oxml_layout::{PositionedElement, Rect};

    fn link(url: &str) -> PositionedElement {
        PositionedElement::LinkAnnotation {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 12.0,
            },
            url: url.to_string(),
        }
    }

    #[test]
    fn external_http_links_survive_sanitization() {
        let page = letter_page(vec![link("https://example.com/docs")]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());
        assert_eq!(slide.hyperlinks.len(), 1);
        assert_eq!(
            slide.hyperlinks[0].target,
            HyperlinkTarget::ExternalUri("https://example.com/docs".to_string())
        );
        assert_eq!(slide.hyperlinks[0].bounds_emu.w, 100.0 * 12_700.0);
    }

    #[test]
    fn fragment_links_resolve_to_physical_pages() {
        let page = letter_page(vec![link("#chapter-2")]);
        let engine = FixtureEngine::new(Vec::new()).with_bookmark("chapter-2", 4);
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());
        assert_eq!(slide.hyperlinks[0].target, HyperlinkTarget::SlideIndex(4));
    }

    #[test]
    fn bare_bookmark_names_resolve_when_the_engine_knows_them() {
        let page = letter_page(vec![link("TopOfDocument")]);
        let engine = FixtureEngine::new(Vec::new()).with_bookmark("TopOfDocument", 0);
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());
        assert_eq!(slide.hyperlinks[0].target, HyperlinkTarget::SlideIndex(0));
    }

    #[test]
    fn unresolved_bookmarks_drop_with_a_warning() {
        let page = letter_page(vec![link("#missing-anchor")]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert!(slide.hyperlinks.is_empty());
        assert!(warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded {
                feature: "docx-link-unresolved",
                ..
            }
        )));
    }

    #[test]
    fn dangerous_schemes_never_become_links() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/passwd",
        ] {
            let page = letter_page(vec![link(url)]);
            let engine = FixtureEngine::new(Vec::new());
            let (slide, warnings) = compile(&page, &engine);
            assert!(slide.hyperlinks.is_empty(), "{url} must not become a link");
            assert!(
                warnings
                    .iter()
                    .any(|w| matches!(w, WarningEvent::FidelityDegraded { .. })),
                "{url} must warn"
            );
        }
    }

    #[test]
    fn empty_rect_links_are_skipped() {
        let page = letter_page(vec![PositionedElement::LinkAnnotation {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 5.0,
            },
            url: "https://example.com".to_string(),
        }]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert!(slide.hyperlinks.is_empty());
        assert!(warnings.is_empty());
    }
}
