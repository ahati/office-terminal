//! Media poster decode path (P4).

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::VectorCommand;
use office_terminal_core::{DefaultSlideCompiler, SlideCompiler};
use pptx_model::{
    MediaElement, Presentation, Slide, SlideElement, SlideElementSource,
};

fn media(poster: &str) -> MediaElement {
    MediaElement {
        id: Some("5".into()),
        x: 100_000,
        y: 200_000,
        width: 1_000_000,
        height: 500_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        media_kind: "video".into(),
        poster_path: poster.into(),
        poster_mime_type: "image/png".into(),
        media_path: "ppt/media/media1.mp4".into(),
        mime_type: "video/mp4".into(),
    }
}

fn slide(element: SlideElement) -> Slide {
    Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: Some("ppt/slides/slide1.xml".into()),
        background: None,
        elements: vec![element],
        element_sources: vec![SlideElementSource {
            origin: pptx_model::SlideElementOrigin::Slide,
        }],
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    }
}

fn compile(presentation: &Presentation) -> office_terminal_core::VectorSlide {
    let (tx, _rx) = unbounded();
    DefaultSlideCompiler::new()
        .compile(
            &presentation.slides[0],
            presentation.slide_width,
            presentation.slide_height,
            &tx,
        )
        .expect("compile")
}

fn deck(element: SlideElement) -> Presentation {
    Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides: vec![slide(element)],
        default_text_color: Some("000000".into()),
        major_font: Some("Calibri".into()),
        minor_font: Some("Calibri".into()),
        hlink_color: None,
        fol_hlink_color: None,
    }
}

#[test]
fn media_poster_emits_a_draw_image_before_the_placeholder_chrome() {
    let slide = compile(&deck(SlideElement::Media(media("ppt/media/poster1.png"))));
    let image = slide.commands.iter().find_map(|command| match command {
        VectorCommand::DrawImage {
            rel_id, dest_rect, ..
        } => Some((rel_id, dest_rect)),
        _ => None,
    });
    let (rel_id, dest_rect) = image.expect("poster DrawImage");
    assert_eq!(rel_id, "ppt/media/poster1.png");
    assert_eq!(dest_rect.w, 1_000_000.0);
    assert_eq!(dest_rect.h, 500_000.0);
}

#[test]
fn media_without_a_poster_keeps_the_vector_placeholder_only() {
    let slide = compile(&deck(SlideElement::Media(media(""))));
    assert!(!slide
        .commands
        .iter()
        .any(|command| matches!(command, VectorCommand::DrawImage { .. })));
}
