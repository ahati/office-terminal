use hayro_syntax::object::dict::keys::ANNOTS;
use hayro_syntax::object::String as PdfString;
use hayro_syntax::object::{Array, Dict, Name, Rect};
use hayro_syntax::page::Page;
use kitty_pptx_core::{HyperlinkTarget, RectF32, SlideLink, EMU_PER_PT};

/// Extracts hyperlink annotations from a PDF page.
pub fn extract_page_links(page: &Page<'_>) -> Vec<SlideLink> {
    let mut links = Vec::new();
    let Some(annots) = page.raw().get::<Array<'_>>(ANNOTS) else {
        return links;
    };

    let (_w_pt, h_pt) = page.render_dimensions();

    for annot in annots.iter::<Dict<'_>>() {
        let is_link = annot
            .get::<Name<'_>>(b"Subtype")
            .map(|n| n.as_ref() == b"Link")
            .unwrap_or(false);

        if !is_link {
            continue;
        }

        let Some(rect) = annot.get::<Rect>(b"Rect") else {
            continue;
        };

        // Transform y-up PDF user points to y-down top-left EMU
        let x_emu = (rect.x0 * f64::from(EMU_PER_PT)).max(0.0) as f32;
        let y_emu = ((h_pt as f64 - rect.y1) * f64::from(EMU_PER_PT)).max(0.0) as f32;
        let w_emu = ((rect.x1 - rect.x0).abs() * f64::from(EMU_PER_PT)).max(0.0) as f32;
        let h_emu = ((rect.y1 - rect.y0).abs() * f64::from(EMU_PER_PT)).max(0.0) as f32;

        if w_emu <= 0.0 || h_emu <= 0.0 {
            continue;
        }

        let bounds_emu = RectF32::new(x_emu, y_emu, w_emu, h_emu);

        let target = if let Some(action) = annot.get::<Dict<'_>>(b"A") {
            let s = action.get::<Name<'_>>(b"S");
            if s.as_ref().map(|n| n.as_ref()) == Some(b"URI") {
                action
                    .get::<PdfString<'_>>(b"URI")
                    .map(|u| HyperlinkTarget::ExternalUri(std::string::String::from_utf8_lossy(u.as_bytes()).into_owned()))
            } else if s.as_ref().map(|n| n.as_ref()) == Some(b"GoTo") {
                action
                    .get::<hayro_syntax::object::Number>(b"D")
                    .map(|d| HyperlinkTarget::SlideIndex(d.as_i64().max(0) as usize))
            } else {
                None
            }
        } else {
            annot
                .get::<hayro_syntax::object::Number>(b"Dest")
                .map(|dest| HyperlinkTarget::SlideIndex(dest.as_i64().max(0) as usize))
        };

        if let Some(target) = target {
            links.push(SlideLink {
                bounds_emu,
                target,
                tooltip: None,
            });
        }
    }

    links
}
