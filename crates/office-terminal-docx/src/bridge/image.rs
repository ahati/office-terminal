//! Image and metafile placement (PRD 15 §5.2, issue #40 §7.7.5).
//!
//! Raster and SVG images become `DrawImage` with the media bytes registered
//! in the asset store under a content-addressed `rel_id`; Windows metafiles
//! become `DrawMetafile` markers so the core playback engine (PRD 13)
//! expands them into native vector paths — or the standard placeholder.

use office_terminal_core::contracts::{
    AffineTransform, NormalizedRect, RectF32, VectorCommand, VectorSlide,
};
use oxml_layout::{MediaId, Rect};

use super::{pt_to_emu, BridgeContext};
use crate::media::media_rel_id;

/// Whether a content type names a Windows metafile picture.
fn is_metafile_content_type(content_type: &str) -> bool {
    let lowered = content_type.to_ascii_lowercase();
    lowered.contains("emf") || lowered.contains("wmf")
}

/// Map one positioned image.
pub(crate) fn map_image(
    rect: &Rect,
    content_type: &str,
    media_id: MediaId,
    acc: &AffineTransform,
    opacity: f32,
    slide: &mut VectorSlide,
    ctx: &BridgeContext<'_>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        // A degenerate image box carries no ink; the honest treatment is a
        // visible placeholder only when bytes are unusable, never for an
        // authored zero-size frame.
        return;
    }
    let rel_id = media_rel_id(media_id);
    if !ctx.media.contains(&rel_id) {
        // The engine should have registered every positioned image at open;
        // if the store disagrees, the raster/SVG placeholder surfaces it and
        // the warning keeps the loss visible.
        ctx.warn(
            office_terminal_core::contracts::WarningEvent::MediaFailed {
                rel_id: rel_id.clone(),
                reason: "image bytes not registered by the engine".to_string(),
            },
        );
    }

    let dest = RectF32::new(
        pt_to_emu(rect.x),
        pt_to_emu(rect.y),
        pt_to_emu(rect.width),
        pt_to_emu(rect.height),
    );

    if is_metafile_content_type(content_type) {
        // Metafile markers carry a picture-local destination box and the
        // accumulated transform including the picture's translation, exactly
        // like the PPTX compiler (PRD 13 §3.1); `expand_metafiles` then
        // maps record space through that transform.
        let placement = acc.post_concat(&AffineTransform::translate(dest.x, dest.y));
        slide.commands.push(VectorCommand::DrawMetafile {
            rel_id,
            transform: placement,
            src_rect: NormalizedRect::default(),
            dest_rect: RectF32::new(0.0, 0.0, dest.w, dest.h),
            opacity: opacity.clamp(0.0, 1.0),
            effects: Vec::new(),
        });
    } else {
        slide.commands.push(VectorCommand::DrawImage {
            rel_id,
            src_rect: NormalizedRect::default(),
            dest_rect: dest,
            opacity: opacity.clamp(0.0, 1.0),
            transform: *acc,
            effects: Vec::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::DocxAssetStore;
    use crate::test_support::{compile, letter_page, media_bytes, FixtureEngine};
    use oxml_layout::{PositionedElement, Rect};

    fn png_bytes() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&[0; 16]);
        bytes
    }

    fn image_element(content_type: &str) -> (PositionedElement, MediaId) {
        let data = png_bytes();
        let id = MediaId::from_bytes(&data);
        (
            PositionedElement::Image {
                rect: Rect {
                    x: 72.0,
                    y: 72.0,
                    width: 200.0,
                    height: 100.0,
                },
                data,
                content_type: content_type.to_string(),
                media_id: id,
            },
            id,
        )
    }

    #[test]
    fn raster_images_become_draw_image_with_content_addressed_rel_ids() {
        let (element, id) = image_element("image/png");
        let page = letter_page(vec![element]);
        let engine = FixtureEngine::new(Vec::new())
            .with_media(id, media_bytes(&png_bytes(), "image/png"));
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());

        let VectorCommand::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            ..
        } = &slide.commands[0]
        else {
            panic!("expected an image command");
        };
        assert_eq!(rel_id, &media_rel_id(id));
        assert_eq!(*src_rect, NormalizedRect::default());
        assert_eq!(dest_rect.x, 72.0 * 12_700.0);
        assert_eq!(dest_rect.w, 200.0 * 12_700.0);
        assert_eq!(dest_rect.h, 100.0 * 12_700.0);
        assert_eq!(*opacity, 1.0);
        assert_eq!(*transform, AffineTransform::identity());
    }

    #[test]
    fn metafile_images_become_markers_with_picture_local_boxes() {
        let (element, id) = image_element("image/x-emf");
        let page = letter_page(vec![element]);
        let engine = FixtureEngine::new(Vec::new())
            .with_media(id, media_bytes(b"\x01\x00\x00\x00", "image/x-emf"));
        let (slide, _warnings) = compile(&page, &engine);
        let VectorCommand::DrawMetafile {
            rel_id,
            transform,
            dest_rect,
            ..
        } = &slide.commands[0]
        else {
            panic!("expected a metafile marker");
        };
        assert_eq!(rel_id, &media_rel_id(id));
        assert_eq!(dest_rect.x, 0.0);
        assert_eq!(dest_rect.w, 200.0 * 12_700.0);
        // The placement transform carries the picture translation.
        assert_eq!(transform.tx, 72.0 * 12_700.0);
        assert_eq!(transform.ty, 72.0 * 12_700.0);
    }

    #[test]
    fn unregistered_media_warns_and_degrades_to_the_placeholder() {
        let (element, _id) = image_element("image/png");
        let page = letter_page(vec![element]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        // The command still lands; the raster placeholder paints when the
        // store lookup fails (missing_picture path).
        assert_eq!(slide.commands.len(), 1);
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, office_terminal_core::contracts::WarningEvent::MediaFailed { .. }))
        );
    }

    #[test]
    fn degenerate_image_rects_are_skipped() {
        let data = png_bytes();
        let id = MediaId::from_bytes(&data);
        let page = letter_page(vec![PositionedElement::Image {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 10.0,
            },
            data,
            content_type: "image/png".to_string(),
            media_id: id,
        }]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);
        assert!(slide.commands.is_empty());
    }

    #[test]
    fn metafile_detection_covers_the_common_content_types() {
        assert!(is_metafile_content_type("image/x-emf"));
        assert!(is_metafile_content_type("image/emf"));
        assert!(is_metafile_content_type("IMAGE/X-WMF"));
        assert!(is_metafile_content_type("image/wmf"));
        assert!(!is_metafile_content_type("image/png"));
        assert!(!is_metafile_content_type("image/svg+xml"));
    }

    #[test]
    fn store_membership_is_queryable_by_rel_id() {
        let data = png_bytes();
        let id = MediaId::from_bytes(&data);
        let engine = FixtureEngine::new(Vec::new()).with_media(id, media_bytes(&data, "image/png"));
        let store = DocxAssetStore::from_engine(&engine);
        assert!(store.contains(&media_rel_id(id)));
        assert!(!store.contains("docx-media-0000000000000000"));
    }
}
