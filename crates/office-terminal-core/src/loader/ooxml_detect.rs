//! OPC-aware main-part detection for OOXML ZIP packages (PRD 15 §4.1).
//!
//! A ZIP local-file header alone does not identify a format: PresentationML,
//! WordprocessingML and SpreadsheetML packages all share it. Detection is
//! authoritative from the OPC package itself:
//!
//! 1. `_rels/.rels` names the `officeDocument` main part (ECMA-376 Part 2
//!    §9.3); its target classifies the package.
//! 2. Without a usable package rels, the conventional main parts are checked
//!    directly.
//! 3. Anything else (spreadsheets, relocated/unknown main parts) is a typed
//!    rejection — never a mislabelled presentation.
//!
//! Part-name matching follows OPC §6.2.2.3 equivalence (case-folded,
//! percent-normalized) via `ooxml_common::rels::part_name_equivalence_key`,
//! and every part read is bounded so a crafted archive cannot inflate a
//! decompression bomb through the sniffer.

use std::io::Cursor;

use ooxml_common::rels::{parse_rels, part_name_equivalence_key, resolve_target};
use ooxml_common::zip::read_zip_string_head;

use crate::contracts::SourceFormat;
use crate::error::ParseError;

/// Cap for any single part read during sniffing (relaxed XML is read as a
/// bounded head; existence checks never decompress content).
const SNIFF_PART_HEAD_BYTES: usize = 256 * 1024;

/// The `officeDocument` relationship type (ECMA-376 Part 2 §9.3): the main
/// document part every OOXML format defines at the package root.
const OFFICE_DOCUMENT_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

type SniffArchive<'a> = zip::ZipArchive<Cursor<&'a [u8]>>;

/// Classify an OOZIP package as [`SourceFormat`] from its main part.
pub fn detect_ooxml_format(bytes: &[u8]) -> Result<SourceFormat, ParseError> {
    let mut zip = SniffArchive::new(Cursor::new(bytes)).map_err(|error| {
        ParseError::ArchiveError(format!("not a readable OOXML ZIP package: {error}"))
    })?;

    if let Some(main_part) = opc_main_part(&mut zip) {
        return main_part_format(&main_part);
    }

    // No usable package rels: fall back to the conventional main parts.
    if contains_part(&mut zip, "word/document.xml") {
        return Ok(SourceFormat::Docx);
    }
    if contains_part(&mut zip, "ppt/presentation.xml") {
        return Ok(SourceFormat::Pptx);
    }
    if contains_part(&mut zip, "xl/workbook.xml") {
        return Err(spreadsheet_rejection());
    }

    Err(ParseError::ArchiveError(
        "OOXML package has no recognized main document part \
         (expected word/document.xml or ppt/presentation.xml)"
            .to_string(),
    ))
}

/// Resolve the `officeDocument` relationship target to a normalized part name.
///
/// Only an omitted or exactly-internal target mode qualifies (hyperlinks and
/// other external rels never name a package part). An unreadable or malformed
/// rels part yields `None` so the caller can fall back to part probing.
fn opc_main_part(zip: &mut SniffArchive<'_>) -> Option<String> {
    let rels_xml = read_zip_string_head(zip, "_rels/.rels", SNIFF_PART_HEAD_BYTES).ok()?;
    let rels = parse_rels(&rels_xml);
    let target = rels.values().find(|rel| {
        rel.mode == ooxml_common::rels::TargetMode::Internal
            && rel.relationship_type.as_deref() == Some(OFFICE_DOCUMENT_REL_TYPE)
    })?;
    // `_rels/.rels` lives at the package root, so the base directory is empty.
    Some(resolve_target("", &target.target))
}

/// Classify from the resolved main-part name (conventional names only).
fn main_part_format(name: &str) -> Result<SourceFormat, ParseError> {
    let key = part_name_equivalence_key(name);
    if key == part_name_equivalence_key("word/document.xml") {
        Ok(SourceFormat::Docx)
    } else if key == part_name_equivalence_key("ppt/presentation.xml") {
        Ok(SourceFormat::Pptx)
    } else if key == part_name_equivalence_key("xl/workbook.xml") {
        Err(spreadsheet_rejection())
    } else {
        Err(ParseError::ArchiveError(format!(
            "unsupported OOXML main document part '{name}': \
             only presentations (.pptx), documents (.docx) and PDFs can be opened"
        )))
    }
}

/// Typed rejection for SpreadsheetML packages.
fn spreadsheet_rejection() -> ParseError {
    ParseError::ArchiveError(
        "spreadsheet (.xlsx) input is not supported; only .pptx, .pdf and \
         .docx documents can be opened"
            .to_string(),
    )
}

/// Whether the archive carries a part equivalent to `part`, without reading it.
fn contains_part(zip: &mut SniffArchive<'_>, part: &str) -> bool {
    let key = part_name_equivalence_key(part);
    zip.file_names()
        .filter_map(|name| name.ok())
        .any(|name| part_name_equivalence_key(name.as_ref()) == key)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn zip_with(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in entries {
            writer
                .start_file(
                    *name,
                    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const RELS_DOCX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

    const RELS_PPTX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#;

    const RELS_EXTERNAL_ONLY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="https://example.com/doc.xml" TargetMode="External"/>
</Relationships>"#;

    #[test]
    fn docx_package_rels_target_classifies_as_docx() {
        let bytes = zip_with(&[
            ("_rels/.rels", RELS_DOCX),
            ("word/document.xml", "<w:document/>"),
        ]);
        assert_eq!(detect_ooxml_format(&bytes).unwrap(), SourceFormat::Docx);
    }

    #[test]
    fn pptx_package_rels_target_classifies_as_pptx() {
        let bytes = zip_with(&[
            ("_rels/.rels", RELS_PPTX),
            ("ppt/presentation.xml", "<p:presentation/>"),
        ]);
        assert_eq!(detect_ooxml_format(&bytes).unwrap(), SourceFormat::Pptx);
    }

    #[test]
    fn part_names_match_case_insensitively_through_equivalence() {
        let bytes = zip_with(&[("WORD/Document.XML", "<w:document/>")]);
        assert_eq!(detect_ooxml_format(&bytes).unwrap(), SourceFormat::Docx);
    }

    #[test]
    fn missing_rels_falls_back_to_conventional_part_names() {
        let bytes = zip_with(&[("ppt/presentation.xml", "<p:presentation/>")]);
        assert_eq!(detect_ooxml_format(&bytes).unwrap(), SourceFormat::Pptx);
    }

    #[test]
    fn external_office_document_rel_is_ignored_for_classification() {
        let bytes = zip_with(&[
            ("_rels/.rels", RELS_EXTERNAL_ONLY),
            ("word/document.xml", "<w:document/>"),
        ]);
        assert_eq!(detect_ooxml_format(&bytes).unwrap(), SourceFormat::Docx);
    }

    #[test]
    fn spreadsheet_main_part_is_a_typed_rejection() {
        let bytes = zip_with(&[
            ("_rels/.rels", RELS_DOCX.replace("word/document.xml", "xl/workbook.xml").as_str()),
            ("xl/workbook.xml", "<x:workbook/>"),
        ]);
        let err = detect_ooxml_format(&bytes).unwrap_err();
        assert!(err.to_string().contains("spreadsheet"));
    }

    #[test]
    fn unknown_main_part_is_rejected_with_the_part_name() {
        let bytes = zip_with(&[
            ("_rels/.rels", RELS_DOCX.replace("word/document.xml", "custom/other.xml").as_str()),
            ("custom/other.xml", "<o:other/>"),
        ]);
        let err = detect_ooxml_format(&bytes).unwrap_err();
        assert!(err.to_string().contains("custom/other.xml"));
    }

    #[test]
    fn empty_zip_without_main_part_is_rejected() {
        let bytes = zip_with(&[]);
        assert!(detect_ooxml_format(&bytes).is_err());
    }

    #[test]
    fn truncated_zip_is_an_archive_error_not_a_panic() {
        let bytes = zip_with(&[("word/document.xml", "<w:document/>")]);
        let truncated = &bytes[..bytes.len() / 2];
        assert!(detect_ooxml_format(truncated).is_err());
    }
}
