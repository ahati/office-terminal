//! In-process DOCX package fixtures for integration tests.
//!
//! Packages are built with the `zip` writer from the exact OPC shape the
//! samples use (`[Content_Types].xml`, `_rels/.rels`, `word/document.xml`),
//! so the tests never depend on files outside the repository.

use std::io::Write;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const PACKAGE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdH1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/more" TargetMode="External"/>
</Relationships>"#;

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

/// Wrap rendered body content into a full `w:document` part (Letter portrait,
/// one-inch margins).
pub fn document_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720"/></w:sectPr></w:body></w:document>"#
    )
}

/// Assemble a DOCX package from its parts.
pub fn build_docx(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, contents) in parts {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .expect("start entry");
        writer.write_all(contents.as_bytes()).expect("write entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

/// A minimal valid document: two headings and two body paragraphs.
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
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", PACKAGE_RELS),
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
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", &document_xml(&body)),
    ])
}

/// A package whose main part is missing: valid ZIP, not a Word document.
pub fn main_part_missing_docx() -> Vec<u8> {
    build_docx(&[("[Content_Types].xml", CONTENT_TYPES), ("_rels/.rels", PACKAGE_RELS)])
}

/// Truncated ZIP magic with garbage behind it.
pub fn garbage_bytes() -> Vec<u8> {
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    bytes
}
