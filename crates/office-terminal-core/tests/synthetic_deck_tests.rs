//! Synthetic-deck parity tests.
//!
//! Port of the package builder in
//! `/tmp/oooxml/packages/pptx/src/test-support/synthetic-deck.ts` plus the
//! round-trip checks the reference derives from it: the hand-built package
//! must be a valid one-slide PresentationML deck that the real loader opens,
//! and repeated builds of the same input must be byte-identical.
//!
//! Only `parity_support/deck.rs` is included (not the shared `mod.rs`) so
//! this target stays isolated from the other parity workstreams.

#[path = "parity_support/deck.rs"]
mod deck_support;

use std::io::{Cursor, Read};

use deck_support::{
    deck, deck_with_size, package_parts, para, plain_run, run, shape, text_box, text_box_with_id,
    DEFAULT_BODY_PR, DEFAULT_PPR, SLIDE_HEIGHT, SLIDE_WIDTH,
};
use office_terminal_core::{DefaultPresentationLoader, PresentationLoader, Source};
use pptx_model::{SlideElement, TextRun};

/// Entry order of the reference package. ref synthetic-deck.ts:16-64
const PART_NAMES: [&str; 11] = [
    "[Content_Types].xml",
    "_rels/.rels",
    "ppt/presentation.xml",
    "ppt/_rels/presentation.xml.rels",
    "ppt/slides/slide1.xml",
    "ppt/slides/_rels/slide1.xml.rels",
    "ppt/slideLayouts/slideLayout1.xml",
    "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
    "ppt/slideMasters/slideMaster1.xml",
    "ppt/slideMasters/_rels/slideMaster1.xml.rels",
    "ppt/theme/theme1.xml",
];

fn text_box_deck() -> Vec<u8> {
    deck(&text_box_deck_shapes())
}

fn text_box_deck_shapes() -> String {
    text_box(
        &para(&plain_run("Hello parity"), DEFAULT_PPR),
        DEFAULT_BODY_PR,
    )
}

fn read_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("readable ZIP");
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("entry");
        let name = file.name().expect("UTF-8 entry name").to_string();
        let mut payload = Vec::new();
        file.read_to_end(&mut payload).expect("entry payload");
        entries.push((name, payload));
    }
    entries
}

/// The package is a well-formed OPC ZIP with the reference part set, entry
/// order and timestamps. ref synthetic-deck.ts:16-64
#[test]
fn package_has_reference_parts_in_order() {
    let bytes = text_box_deck();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).expect("readable ZIP");

    assert_eq!(archive.len(), PART_NAMES.len());
    for (index, expected) in PART_NAMES.iter().enumerate() {
        let file = archive.by_index(index).expect("entry");
        assert_eq!(
            file.name().expect("UTF-8 entry name"),
            *expected,
            "entry {index} name"
        );
        assert_eq!(
            file.compression(),
            zip::CompressionMethod::Stored,
            "reference package stores every entry uncompressed"
        );
        // Deterministic writer: every entry carries the 1980-01-01 ZIP epoch.
        assert_eq!(
            file.last_modified(),
            Some(zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).expect("epoch")),
            "entry {index} timestamp"
        );
    }

    // Part payloads are exactly the composed XML, no implicit extras.
    let expected = package_parts(&text_box_deck_shapes(), SLIDE_WIDTH, SLIDE_HEIGHT);
    let expected_entries: Vec<(String, Vec<u8>)> = expected
        .iter()
        .map(|(name, body)| (name.clone(), body.clone().into_bytes()))
        .collect();
    assert_eq!(read_entries(&bytes), expected_entries);

    // The XML declaration the reference prepends. ref synthetic-deck.ts:16
    let content_types = &expected[0].1;
    assert!(content_types.starts_with(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#));
    assert!(content_types.contains(r#"PartName="/ppt/slides/slide1.xml""#));
    assert!(content_types.contains("presentationml.presentation.main+xml"));
}

/// The real loader accepts the synthetic package and surfaces the slide size,
/// the single shape's frame and its run text.
#[test]
fn loader_reads_slide_size_frame_and_text() {
    let (presentation, _assets) = DefaultPresentationLoader
        .load(Source::Bytes(text_box_deck()))
        .expect("synthetic deck loads");

    assert_eq!(presentation.slide_width, SLIDE_WIDTH);
    assert_eq!(presentation.slide_height, SLIDE_HEIGHT);
    // ref synthetic-deck.ts:56-64 theme: clrScheme dk1/hlink + fontScheme.
    assert_eq!(presentation.major_font.as_deref(), Some("Calibri Light"));
    assert_eq!(presentation.minor_font.as_deref(), Some("Calibri"));
    assert_eq!(presentation.default_text_color.as_deref(), Some("000000"));
    assert_eq!(presentation.hlink_color.as_deref(), Some("0000FF"));
    assert_eq!(presentation.fol_hlink_color.as_deref(), Some("800080"));

    assert_eq!(presentation.slides.len(), 1);
    let slide = &presentation.slides[0];
    assert_eq!(slide.index, 0);
    assert_eq!(slide.slide_number, 1);
    assert_eq!(slide.part_name.as_deref(), Some("ppt/slides/slide1.xml"));
    assert!(slide.parse_error.is_none(), "slide must parse cleanly");
    assert_eq!(slide.elements.len(), 1);
    // Per-slide theme resolves the same master theme.
    let theme = slide.theme.as_ref().expect("slide theme");
    assert_eq!(theme.default_text_color.as_deref(), Some("000000"));
    assert_eq!(theme.major_font.as_deref(), Some("Calibri Light"));
    assert_eq!(theme.minor_font.as_deref(), Some("Calibri"));
    assert_eq!(theme.hlink_color.as_deref(), Some("0000FF"));

    let SlideElement::Shape(shape) = &slide.elements[0] else {
        panic!("expected a shape, got {:?}", slide.elements[0]);
    };
    assert_eq!(shape.geometry, "rect");
    assert_eq!(
        (shape.x, shape.y, shape.width, shape.height),
        (457_200, 457_200, 3_200_400, 2_743_200)
    );

    // ref synthetic-deck.ts:70 bodyPr round-trips as wrap + autofit.
    let body = shape.text_body.as_ref().expect("text box carries a txBody");
    assert_eq!(body.wrap, "square");
    assert_eq!(body.auto_fit, "norm");
    assert_eq!(body.vert, "horz");
    // Unset bodyPr insets fall back to the ECMA-376 defaults (§21.1.2.1.1).
    assert_eq!(body.vertical_anchor, "t");
    assert_eq!(
        (body.l_ins, body.r_ins, body.t_ins, body.b_ins),
        (91_440, 91_440, 45_720, 45_720)
    );
    assert_eq!(body.paragraphs.len(), 1);

    // ref synthetic-deck.ts:72 `<a:buNone/>` leaves the paragraph unbulleted.
    let paragraph = &body.paragraphs[0];
    assert_eq!(paragraph.alignment, "l");
    assert!(matches!(paragraph.bullet, pptx_model::Bullet::None));
    assert_eq!(paragraph.runs.len(), 1);

    let TextRun::Text(run) = &paragraph.runs[0] else {
        panic!("expected a text run, got {:?}", paragraph.runs[0]);
    };
    assert_eq!(run.text, "Hello parity");
    // ref synthetic-deck.ts:73 rPr lang/sz survive.
    assert_eq!(run.font_size, Some(24.0));
    // No explicit typeface: the theme minor font resolves through.
    assert_eq!(run.font_family.as_deref(), Some("Calibri"));
}

/// `deck_with_size` overrides `p:sldSz` while keeping the rest of the package.
#[test]
fn deck_with_size_overrides_slide_dimensions() {
    let shapes = text_box_deck_shapes();
    let wide = deck_with_size(&shapes, 12_192_000, SLIDE_HEIGHT);
    let (presentation, _assets) = DefaultPresentationLoader
        .load(Source::Bytes(wide))
        .expect("wide deck loads");
    assert_eq!(presentation.slide_width, 12_192_000);
    assert_eq!(presentation.slide_height, SLIDE_HEIGHT);
    assert_eq!(presentation.slides.len(), 1);

    // The default builder is the same package as an explicit reference size.
    assert_eq!(deck(&shapes), deck_with_size(&shapes, SLIDE_WIDTH, SLIDE_HEIGHT));
}

/// Same input => same bytes, both for the whole archive and entry payloads.
/// ref `storedZip` fixes entry order and timestamps.
#[test]
fn builds_are_byte_identical() {
    let first = text_box_deck();
    let second = text_box_deck();
    assert_eq!(first, second, "repeated builds must be byte-identical");

    let first_entries = read_entries(&first);
    let second_entries = read_entries(&second);
    assert_eq!(first_entries.len(), second_entries.len());
    for ((first_name, first_payload), (second_name, second_payload)) in
        first_entries.iter().zip(second_entries.iter())
    {
        assert_eq!(first_name, second_name);
        assert_eq!(first_payload, second_payload, "payload of {first_name}");
    }

    // A different slide tree must change the slide part and its length only
    // where the shapes live, never the boilerplate.
    let other = deck(&shape(
        r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#,
        r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#,
        "",
    ));
    let base_entries = read_entries(&first);
    let other_entries = read_entries(&other);
    for (index, ((name, base), (other_name, changed))) in
        base_entries.iter().zip(other_entries.iter()).enumerate()
    {
        assert_eq!(name, other_name, "entry {index}");
        if name == "ppt/slides/slide1.xml" {
            assert_ne!(base, changed, "slide part reflects the new shapes");
        } else if index != 0 {
            // `[Content_Types].xml` is size-independent here too; all other
            // boilerplate parts are untouched by the slide tree.
            assert_eq!(base, changed, "{name} must not depend on the slide tree");
        }
    }
}

/// The helpers emit the reference template verbatim.
/// ref synthetic-deck.ts:69-73
#[test]
fn helpers_emit_reference_xml() {
    assert_eq!(
        plain_run("abc"),
        r#"<a:r><a:rPr lang="en-US" sz="2400"/><a:t>abc</a:t></a:r>"#
    );
    assert_eq!(
        run("abc", " cap=\"all\""),
        r#"<a:r><a:rPr lang="en-US" sz="2400" cap="all"/><a:t>abc</a:t></a:r>"#
    );
    assert_eq!(
        para(&plain_run("abc"), DEFAULT_PPR),
        concat!(
            "<a:p><a:pPr><a:buNone/></a:pPr>",
            r#"<a:r><a:rPr lang="en-US" sz="2400"/><a:t>abc</a:t></a:r></a:p>"#
        )
    );
    assert_eq!(
        text_box(&para(&plain_run("abc"), DEFAULT_PPR), DEFAULT_BODY_PR),
        concat!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="T"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>"#,
            r#"<p:spPr><a:xfrm><a:off x="457200" y="457200"/>"#,
            r#"<a:ext cx="3200400" cy="2743200"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#,
            r#"<p:txBody><a:bodyPr wrap="square"><a:normAutofit/></a:bodyPr>"#,
            r#"<a:lstStyle/><a:p><a:pPr><a:buNone/></a:pPr>"#,
            r#"<a:r><a:rPr lang="en-US" sz="2400"/><a:t>abc</a:t></a:r></a:p></p:txBody></p:sp>"#
        )
    );
    // A geometry-only shape omits `p:txBody`; `sp_pr` sits after the geometry.
    assert_eq!(
        shape(
            r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#,
            r#"<a:solidFill><a:srgbClr val="00FF00"/></a:solidFill>"#,
            "",
        ),
        concat!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="S"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
            r#"<p:spPr><a:xfrm><a:off x="457200" y="457200"/>"#,
            r#"<a:ext cx="3200400" cy="2743200"/></a:xfrm>"#,
            r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#,
            r#"<a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></p:spPr></p:sp>"#
        )
    );
}

/// Caller-chosen ids keep multi-shape decks valid and ordered.
#[test]
fn multiple_shapes_keep_document_order() {
    let shapes = text_box_with_id(2, &para(&run("first", ""), DEFAULT_PPR), DEFAULT_BODY_PR)
        + &text_box_with_id(3, &para(&run("second", ""), DEFAULT_PPR), DEFAULT_BODY_PR)
        + &shape(
            r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#,
            r#"<a:solidFill><a:srgbClr val="00FF00"/></a:solidFill>"#,
            "",
        );
    let (presentation, _assets) = DefaultPresentationLoader
        .load(Source::Bytes(deck(&shapes)))
        .expect("multi-shape deck loads");

    let elements = &presentation.slides[0].elements;
    assert_eq!(elements.len(), 3);
    let geometries: Vec<&str> = elements
        .iter()
        .map(|element| match element {
            SlideElement::Shape(shape) => shape.geometry.as_str(),
            other => panic!("unexpected element {other:?}"),
        })
        .collect();
    assert_eq!(geometries, ["rect", "rect", "ellipse"]);

    let texts: Vec<String> = elements
        .iter()
        .filter_map(|element| match element {
            SlideElement::Shape(shape) => shape.text_body.as_ref(),
            _ => None,
        })
        .flat_map(|body| body.paragraphs.iter())
        .flat_map(|paragraph| paragraph.runs.iter())
        .filter_map(|run| match run {
            TextRun::Text(data) => Some(data.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "second"]);
    // A `tx_body`-less shape carries no text body at all.
    assert!(matches!(
        &elements[2],
        SlideElement::Shape(shape) if shape.text_body.is_none()
    ));
}

