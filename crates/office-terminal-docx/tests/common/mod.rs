//! In-process DOCX package fixtures for integration tests.
//!
//! Packages are built with the `zip` writer from the exact OPC shape Word
//! emits (`[Content_Types].xml`, `_rels/.rels`, `word/document.xml`, optional
//! header/footer/media/core-properties parts), so the tests never depend on
//! files outside the repository.

// Shared by the engine crate's roundtrip tests and the CLI's integration
// tests (via `#[path]`); each binary uses a different subset of the fixtures.
#![allow(dead_code)]

use std::io::Write;

const HEADER_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const FOOTER_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const CORE_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.core-properties+xml";

/// `[Content_Types].xml` with the conventional defaults plus the document
/// main part; `extra` carries any additional `<Default/>`/`<Override/>`
/// lines for the parts a particular fixture adds.
fn content_types(extra: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
{extra}</Types>"#
    )
}

const PACKAGE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const PACKAGE_RELS_WITH_CORE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdH1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/more" TargetMode="External"/>
</Relationships>"#;

const DOCUMENT_RELS_HEADER_FOOTER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdHdr1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
  <Relationship Id="rIdFtr1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
</Relationships>"#;

const DOCUMENT_RELS_IMAGE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdImg1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
</Relationships>"#;

/// `docProps/core.xml` declaring a title, subject, author and keywords.
const CORE_PROPS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>Integration Report</dc:title>
  <dc:subject>DOCX engine roundtrip</dc:subject>
  <dc:creator>A. Writer</dc:creator>
  <cp:keywords>docx, bridge</cp:keywords>
</cp:coreProperties>"#;

const HEADER1: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Running header</w:t></w:r></w:p></w:hdr>"#;

/// Footer with a complex `PAGE` field (the `fldChar` form Word writes), so
/// the engine has to evaluate the field per page.
const FOOTER1: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t xml:space="preserve">Footer page </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:ftr>"#;

/// One paragraph: its text and optional paragraph style id (`Heading1`, …).
pub struct Paragraph {
    pub text: String,
    pub style: Option<&'static str>,
}

pub fn paragraph(text: &str) -> Paragraph {
    Paragraph {
        text: text.to_string(),
        style: None,
    }
}

pub fn heading(text: &str, style: &'static str) -> Paragraph {
    Paragraph {
        text: text.to_string(),
        style: Some(style),
    }
}

fn paragraph_xml(para: &Paragraph) -> String {
    let style = para
        .style
        .map(|style| format!("<w:pStyle w:val=\"{style}\"/>"))
        .unwrap_or_default();
    format!(
        "<w:p><w:pPr>{style}</w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(&para.text)
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Wrap rendered body content into a full `w:document` part (Letter
/// portrait, one-inch margins). `sect_head` carries the `sectPr` children
/// that precede `w:pgSz` (header/footer references, …).
pub fn document_xml_with_sect(body: &str, sect_head: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:body>{body}<w:sectPr>{sect_head}<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720"/></w:sectPr></w:body></w:document>"#
    )
}

/// Wrap rendered body content into a full `w:document` part (Letter
/// portrait, one-inch margins, no header/footer references).
pub fn document_xml(body: &str) -> String {
    document_xml_with_sect(body, "")
}

/// Assemble a DOCX package from its text parts.
pub fn build_docx(parts: &[(&str, &str)]) -> Vec<u8> {
    let bytes: Vec<(&str, &[u8])> = parts
        .iter()
        .map(|(name, contents)| (*name, contents.as_bytes()))
        .collect();
    build_docx_bytes(&bytes)
}

/// Assemble a DOCX package from raw part bytes (needed for binary media).
pub fn build_docx_bytes(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, contents) in parts {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .expect("start entry");
        writer.write_all(contents).expect("write entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

/// A document with two headings, two body paragraphs and declared core
/// properties.
pub fn simple_docx() -> Vec<u8> {
    let body: String = [
        heading("Integration Heading One", "Heading1"),
        paragraph("First body paragraph with plain text."),
        heading("Integration Heading Two", "Heading2"),
        paragraph("Second body paragraph, also plain."),
    ]
    .iter()
    .map(paragraph_xml)
    .collect();
    build_docx(&[
        ("[Content_Types].xml", &content_types(&format!(
            "<Override PartName=\"/docProps/core.xml\" ContentType=\"{CORE_CONTENT_TYPE}\"/>"
        ))),
        ("_rels/.rels", PACKAGE_RELS_WITH_CORE),
        ("docProps/core.xml", CORE_PROPS),
        ("word/document.xml", &document_xml(&body)),
    ])
}

/// A document with an external hyperlink, an internal bookmark link and a
/// forced page break across two pages.
pub fn hyperlinks_and_pages_docx() -> Vec<u8> {
    let body = format!(
        "{}{}{}{}",
        paragraph_xml(&paragraph("External link: ")),
        "<w:p><w:hyperlink r:id=\"rIdH1\"><w:r><w:t>visit the docs</w:t></w:r></w:hyperlink></w:p>",
        "<w:p><w:bookmarkStart w:id=\"7\" w:name=\"chapter-two\"/><w:r><w:t>Chapter two anchor.</w:t></w:r><w:bookmarkEnd w:id=\"7\"/></w:p>",
        "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p><w:p><w:hyperlink w:anchor=\"chapter-two\"><w:r><w:t>back to chapter two</w:t></w:r></w:hyperlink></w:p>",
    );
    build_docx(&[
        ("[Content_Types].xml", &content_types("")),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", &document_xml(&body)),
    ])
}

/// A 3x3 table with a `gridSpan` header cell, a `vMerge` column, full
/// single-line borders and one shaded cell, framed by body paragraphs.
pub fn table_docx() -> Vec<u8> {
    let cell = |text: &str| {
        format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
    };
    let tc = |props: &str, text: &str| format!("<w:tc><w:tcPr>{props}</w:tcPr>{}</w:tc>", cell(text));
    let width = "<w:tcW w:w=\"2880\" w:type=\"dxa\"/>";
    let table = format!(
        concat!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"8640\" w:type=\"dxa\"/>",
            "<w:tblBorders>",
            "<w:top w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"000000\"/>",
            "<w:left w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"000000\"/>",
            "<w:bottom w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"000000\"/>",
            "<w:right w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"000000\"/>",
            "<w:insideH w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"000000\"/>",
            "<w:insideV w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"000000\"/>",
            "</w:tblBorders></w:tblPr>",
            "<w:tblGrid><w:gridCol w:w=\"2880\"/><w:gridCol w:w=\"2880\"/><w:gridCol w:w=\"2880\"/></w:tblGrid>",
            "<w:tr>",
            "<w:tc><w:tcPr><w:tcW w:w=\"5760\" w:type=\"dxa\"/><w:gridSpan w:val=\"2\"/>",
            "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"D9E2F3\"/></w:tcPr>{spanning}</w:tc>",
            "{third}",
            "</w:tr><w:tr>",
            "<w:tc><w:tcPr>{width}<w:vMerge w:val=\"restart\"/></w:tcPr>{merged}</w:tc>",
            "{b2}{c2}",
            "</w:tr><w:tr>",
            "<w:tc><w:tcPr>{width}<w:vMerge w:val=\"continue\"/></w:tcPr><w:p/></w:tc>",
            "{b3}{c3}",
            "</w:tr></w:tbl>"
        ),
        width = width,
        spanning = cell("Spanning header"),
        third = tc(width, "Third column"),
        merged = cell("Merged cell"),
        b2 = tc(width, "B2"),
        c2 = tc(width, "C2"),
        b3 = tc(width, "B3"),
        c3 = tc(width, "C3"),
    );
    let body = format!(
        "{}{}{}",
        paragraph_xml(&paragraph("Before the table.")),
        table,
        paragraph_xml(&paragraph("After the table."))
    );
    build_docx(&[
        ("[Content_Types].xml", &content_types("")),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/document.xml", &document_xml(&body)),
    ])
}

/// A two-page document with a running header and a footer whose `PAGE`
/// field the engine must evaluate per page.
pub fn header_footer_docx() -> Vec<u8> {
    let body = format!(
        "{}{}{}",
        paragraph_xml(&paragraph("First page body.")),
        "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>",
        paragraph_xml(&paragraph("Second page body.")),
    );
    let sect_head = concat!(
        "<w:headerReference w:type=\"default\" r:id=\"rIdHdr1\"/>",
        "<w:footerReference w:type=\"default\" r:id=\"rIdFtr1\"/>",
    );
    build_docx(&[
        ("[Content_Types].xml", &content_types(&format!(
            "<Override PartName=\"/word/header1.xml\" ContentType=\"{HEADER_CONTENT_TYPE}\"/>\
             <Override PartName=\"/word/footer1.xml\" ContentType=\"{FOOTER_CONTENT_TYPE}\"/>"
        ))),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS_HEADER_FOOTER),
        ("word/header1.xml", HEADER1),
        ("word/footer1.xml", FOOTER1),
        (
            "word/document.xml",
            &document_xml_with_sect(&body, sect_head),
        ),
    ])
}

/// A real 2x1 RGB PNG (opaque `#2060A0` pixels), embedded as bytes so the
/// fixture module stays usable from test binaries without an `image`
/// dependency.
pub const SAMPLE_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x7b, 0x40, 0xe8,
    0xdd, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x50, 0x48, 0x58, 0x00,
    0x44, 0x00, 0x06, 0xe7, 0x02, 0x41, 0xa9, 0xc2, 0xc1, 0x84, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A document with one inline PNG picture (`wp:inline` / `pic:pic`) carrying
/// [`SAMPLE_PNG`].
pub fn image_docx() -> Vec<u8> {
    let drawing = r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="914400" cy="457200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="1" name="Picture 1"/><wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="1" name="image1.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg1"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
    let body = format!(
        "{}<w:p>{drawing}</w:p>{}",
        paragraph_xml(&paragraph("Picture follows.")),
        paragraph_xml(&paragraph("Picture precedes."))
    );
    let png_bytes = SAMPLE_PNG.to_vec();
    build_docx_bytes(&[
        ("[Content_Types].xml", content_types(
            "<Default Extension=\"png\" ContentType=\"image/png\"/>",
        ).as_bytes()),
        ("_rels/.rels", PACKAGE_RELS.as_bytes()),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS_IMAGE.as_bytes()),
        ("word/document.xml", document_xml(&body).as_bytes()),
        ("word/media/image1.png", &png_bytes),
    ])
}

/// A document whose only paragraph is an OMML equation ($E=mc^2$).
pub fn math_docx() -> Vec<u8> {
    let body = concat!(
        "<w:p><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath>",
        "<m:r><m:t>E=m</m:t></m:r>",
        "<m:sSup><m:e><m:r><m:t>c</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup>",
        "</m:oMath></m:oMathPara></w:p>"
    );
    build_docx(&[
        ("[Content_Types].xml", &content_types("")),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/document.xml", &document_xml(body)),
    ])
}

/// A package whose main part is missing: valid ZIP, not a Word document.
pub fn main_part_missing_docx() -> Vec<u8> {
    build_docx(&[
        ("[Content_Types].xml", &content_types("")),
        ("_rels/.rels", PACKAGE_RELS),
    ])
}

/// Truncated ZIP magic with garbage behind it.
pub fn garbage_bytes() -> Vec<u8> {
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    bytes
}
