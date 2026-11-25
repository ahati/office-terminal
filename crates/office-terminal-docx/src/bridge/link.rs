//! Hyperlink annotations (issue #40 §7.7.7).
//!
//! Every external URL passes the shared sanitizer gate before it can reach
//! OSC 8, SVG or an OS opener; fragment/bare-bookmark targets resolve to
//! physical page indices through the engine. Unresolvable and forbidden
//! links drop with a warning — never silently, never as a URL.

use office_terminal_core::contracts::{AffineTransform, HyperlinkTarget, RectF32, SlideLink, WarningEvent};
use office_terminal_core::sanitize_hyperlink_url;
use oxml_layout::Rect;

use super::{pt_to_emu, BridgeContext};

/// Map one link annotation onto the slide's hyperlink list.
pub(crate) fn map_link(
    rect: &Rect,
    url: &str,
    acc: &AffineTransform,
    slide: &mut office_terminal_core::contracts::VectorSlide,
    ctx: &BridgeContext<'_>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let x0 = pt_to_emu(rect.x);
    let y0 = pt_to_emu(rect.y);
    let x1 = pt_to_emu(rect.x + rect.width);
    let y1 = pt_to_emu(rect.y + rect.height);

    let (p0_x, p0_y) = acc.transform_point(x0, y0);
    let (p1_x, p1_y) = acc.transform_point(x1, y0);
    let (p2_x, p2_y) = acc.transform_point(x1, y1);
    let (p3_x, p3_y) = acc.transform_point(x0, y1);

    let min_x = p0_x.min(p1_x).min(p2_x).min(p3_x);
    let max_x = p0_x.max(p1_x).max(p2_x).max(p3_x);
    let min_y = p0_y.min(p1_y).min(p2_y).min(p3_y);
    let max_y = p0_y.max(p1_y).max(p2_y).max(p3_y);

    let bounds = RectF32::new(min_x, min_y, (max_x - min_x).max(0.0), (max_y - min_y).max(0.0));
    if bounds.w <= 0.0 || bounds.h <= 0.0 {
        return;
    }

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
        for url in ["javascript:alert(1)", "data:text/html,hi", "file:///etc/passwd"] {
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

    #[test]
    fn transformed_group_links_have_page_space_bounds() {
        let group = oxml_layout::GroupElement {
            transform: oxml_layout::Transform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 50.0,
                f: 100.0,
            },
            clip: None,
            opacity: 1.0,
            effects: Vec::new(),
            children: vec![link("https://example.com/target")],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());
        assert_eq!(slide.hyperlinks.len(), 1);
        assert_eq!(slide.hyperlinks[0].bounds_emu.x, 60.0 * 12_700.0);
        assert_eq!(slide.hyperlinks[0].bounds_emu.y, 110.0 * 12_700.0);
        assert_eq!(slide.hyperlinks[0].bounds_emu.w, 100.0 * 12_700.0);
        assert_eq!(slide.hyperlinks[0].bounds_emu.h, 12.0 * 12_700.0);
    }
}
