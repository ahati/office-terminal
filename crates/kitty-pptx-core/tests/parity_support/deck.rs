//! Synthetic one-slide `.pptx` decks for parity tests.
//!
//! 1:1 port of `/tmp/oooxml/packages/pptx/src/test-support/synthetic-deck.ts`:
//! a minimal PresentationML package (ECMA-376 Part 1 §19.2 presentation,
//! §19.3 slide/layout/master, §20.1.6 theme) built around caller-supplied
//! slide shapes, so tests never depend on third-party decks.
//!
//! The reference writes the ZIP by hand (`storedZip`), i.e. stored entries in
//! the order given, with zeroed MS-DOS timestamps, which makes builds
//! byte-stable. Here the same guarantee comes from an explicit
//! `CompressionMethod::Stored` + epoch timestamp + fixed `System::Unix`
//! entry metadata, so two builds of the same input are byte-identical.
//!
//! Caller-supplied fragments (`shapes`, `text`, attributes) are inserted
//! verbatim, exactly like the TS helpers; callers escaping their own XML is
//! their responsibility.

// Several helpers/constants exist for the other parity workstreams (shapes,
// VRT) and are not all exercised by this target alone.
#![allow(dead_code)]

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

/// PresentationML main namespace (`p:`). ref synthetic-deck.ts:8
pub const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
/// DrawingML main namespace (`a:`). ref synthetic-deck.ts:9
pub const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
/// Office document relationships namespace (`r:`). ref synthetic-deck.ts:10
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// OPC package relationships namespace.
pub const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
/// Prefix of the officeDocument relationship types. ref synthetic-deck.ts:12
pub const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Default slide width in EMU (10in), matching the reference `p:sldSz`.
pub const SLIDE_WIDTH: i64 = 9_144_000;
/// Default slide height in EMU (7.5in), matching the reference `p:sldSz`.
pub const SLIDE_HEIGHT: i64 = 6_858_000;

/// `a:bodyPr` used by [`text_box`] when the caller has no preference.
/// ref synthetic-deck.ts:70
pub const DEFAULT_BODY_PR: &str = r#"<a:bodyPr wrap="square"><a:normAutofit/></a:bodyPr>"#;
/// `a:pPr` used by [`para`] when the caller has no preference.
/// ref synthetic-deck.ts:72
pub const DEFAULT_PPR: &str = r#"<a:pPr><a:buNone/></a:pPr>"#;

/// Frame of the reference text box (`a:off` 457200,457200 / `a:ext`
/// 3200400,2743200). ref synthetic-deck.ts:71
pub const TEXT_BOX_X: i64 = 457_200;
pub const TEXT_BOX_Y: i64 = 457_200;
pub const TEXT_BOX_CX: i64 = 3_200_400;
pub const TEXT_BOX_CY: i64 = 2_743_200;

/// The 11 parts of the synthetic package, in the reference's entry order.
/// ref synthetic-deck.ts:16-64
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

/// A one-slide deck whose slide tree is `shapes` (`p:sp` / `p:graphicFrame`
/// XML) at the reference `p:sldSz` of 9144000x6858000 EMU.
/// ref synthetic-deck.ts:15
pub fn deck(shapes: &str) -> Vec<u8> {
    deck_with_size(shapes, SLIDE_WIDTH, SLIDE_HEIGHT)
}

/// [`deck`] with a caller-chosen `p:sldSz` (EMU), for aspect-ratio tests.
pub fn deck_with_size(shapes: &str, cx: i64, cy: i64) -> Vec<u8> {
    let files = package_parts(shapes, cx, cy);
    stored_zip(&files)
}

/// The package parts as `(part name, part XML)` in the reference entry order.
/// Useful for asserting payload equality without unzipping.
pub fn package_parts(shapes: &str, cx: i64, cy: i64) -> Vec<(String, String)> {
    PART_NAMES
        .iter()
        .map(|name| {
            let body = match *name {
                "[Content_Types].xml" => content_types(),
                "_rels/.rels" => root_rels(),
                "ppt/presentation.xml" => presentation(cx, cy),
                "ppt/_rels/presentation.xml.rels" => presentation_rels(),
                "ppt/slides/slide1.xml" => slide(shapes),
                "ppt/slides/_rels/slide1.xml.rels" => slide_rels(),
                "ppt/slideLayouts/slideLayout1.xml" => slide_layout(),
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels" => slide_layout_rels(),
                "ppt/slideMasters/slideMaster1.xml" => slide_master(),
                "ppt/slideMasters/_rels/slideMaster1.xml.rels" => slide_master_rels(),
                "ppt/theme/theme1.xml" => theme(),
                // `PART_NAMES` is the single source of truth for the entry set.
                other => unreachable!("unmapped package part {other}"),
            };
            ((*name).to_string(), body)
        })
        .collect()
}

/// One text box at the reference frame with the given `a:bodyPr` and
/// paragraphs. ref synthetic-deck.ts:69-71
///
/// The TS helper pulls shape ids from a module-level counter (`shapeId++`).
/// That would make repeated builds of the same input differ, so the port
/// pins the first id the reference would hand out (2); use
/// [`text_box_with_id`] when a deck needs more than one text box.
pub fn text_box(paragraphs: &str, body_pr: &str) -> String {
    text_box_with_id(2, paragraphs, body_pr)
}

/// [`text_box`] with an explicit `p:cNvPr@id`.
pub fn text_box_with_id(id: u32, paragraphs: &str, body_pr: &str) -> String {
    format!(
        concat!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="T"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>"#,
            r#"<p:spPr><a:xfrm><a:off x="457200" y="457200"/>"#,
            r#"<a:ext cx="3200400" cy="2743200"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#,
            r#"<p:txBody>{}<a:lstStyle/>{}</p:txBody></p:sp>"#,
        ),
        id, body_pr, paragraphs
    )
}

/// `<a:p>` around `pPr` + runs. `ppr` is inserted verbatim and must precede
/// the runs; pass [`DEFAULT_PPR`] for the reference default.
/// ref synthetic-deck.ts:72
pub fn para(runs: &str, ppr: &str) -> String {
    format!("<a:p>{ppr}{runs}</a:p>")
}

/// One `a:r` run with the reference `lang="en-US"` / `sz="2400"` rPr. `rpr`
/// is appended inside `a:rPr`, so a caller adding attributes passes a leading
/// space (` cap="all"`), matching the TS template. ref synthetic-deck.ts:73
pub fn run(text: &str, rpr: &str) -> String {
    format!(r#"<a:r><a:rPr lang="en-US" sz="2400"{rpr}/><a:t>{text}</a:t></a:r>"#)
}

/// [`run`] with no extra run properties.
pub fn plain_run(text: &str) -> String {
    run(text, "")
}

/// A `p:sp` at the reference text-box frame: `geom_xml` is the geometry
/// element (`a:prstGeom` / `a:custGeom`), `sp_pr` the remaining `a:spPr`
/// children (fill, `a:ln`, effects) and `tx_body` the *inner* XML of
/// `p:txBody` (`a:bodyPr` + `a:lstStyle` + paragraphs, as produced by
/// [`para`]). An empty `tx_body` omits `p:txBody` entirely, yielding a
/// geometry-only shape.
pub fn shape(geom_xml: &str, sp_pr: &str, tx_body: &str) -> String {
    shape_with_frame(
        2,
        TEXT_BOX_X,
        TEXT_BOX_Y,
        TEXT_BOX_CX,
        TEXT_BOX_CY,
        geom_xml,
        sp_pr,
        tx_body,
    )
}

/// [`shape`] with an explicit `p:cNvPr@id` and frame (EMU).
#[allow(clippy::too_many_arguments)]
pub fn shape_with_frame(
    id: u32,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    geom_xml: &str,
    sp_pr: &str,
    tx_body: &str,
) -> String {
    let body = if tx_body.is_empty() {
        String::new()
    } else {
        format!("<p:txBody>{tx_body}</p:txBody>")
    };
    format!(
        concat!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="S"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
            r#"<p:spPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
            "{}{}</p:spPr>{}</p:sp>"
        ),
        id, x, y, cx, cy, geom_xml, sp_pr, body
    )
}

/// Wrap a part body in the reference XML declaration. ref synthetic-deck.ts:16
fn xml(body: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>{body}"#)
}

/// ref synthetic-deck.ts:18-26
fn content_types() -> String {
    xml(concat!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" "#,
        r#"ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/ppt/presentation.xml" "#,
        r#"ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>"#,
        r#"<Override PartName="/ppt/slides/slide1.xml" "#,
        r#"ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
        r#"<Override PartName="/ppt/slideLayouts/slideLayout1.xml" "#,
        r#"ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#,
        r#"<Override PartName="/ppt/slideMasters/slideMaster1.xml" "#,
        r#"ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>"#,
        r#"<Override PartName="/ppt/theme/theme1.xml" "#,
        r#"ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#,
        "</Types>"
    ))
}

/// ref synthetic-deck.ts:27
fn root_rels() -> String {
    xml(&format!(
        concat!(
            r#"<Relationships xmlns="{}">"#,
            r#"<Relationship Id="rId1" Type="{}/officeDocument" "#,
            r#"Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        REL, OFFICE_REL
    ))
}

/// ref synthetic-deck.ts:28-31
fn presentation(cx: i64, cy: i64) -> String {
    xml(&format!(
        concat!(
            r#"<p:presentation xmlns:p="{}" xmlns:r="{}">"#,
            r#"<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>"#,
            r#"<p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst>"#,
            r#"<p:sldSz cx="{}" cy="{}"/>"#,
            r#"<p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#,
        ),
        P, R, cx, cy
    ))
}

/// ref synthetic-deck.ts:32-35
fn presentation_rels() -> String {
    xml(&format!(
        concat!(
            r#"<Relationships xmlns="{}">"#,
            r#"<Relationship Id="rId1" Type="{}/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
            r#"<Relationship Id="rId2" Type="{}/slide" Target="slides/slide1.xml"/>"#,
            r#"<Relationship Id="rId3" Type="{}/theme" Target="theme/theme1.xml"/></Relationships>"#,
        ),
        REL, OFFICE_REL, OFFICE_REL, OFFICE_REL
    ))
}

/// ref synthetic-deck.ts:36-38
fn slide(shapes: &str) -> String {
    xml(&format!(
        concat!(
            r#"<p:sld xmlns:p="{}" xmlns:a="{}" xmlns:r="{}"><p:cSld><p:spTree>"#,
            r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>"#,
            "{}</p:spTree></p:cSld></p:sld>",
        ),
        P, A, R, shapes
    ))
}

/// ref synthetic-deck.ts:39-40
fn slide_rels() -> String {
    xml(&format!(
        concat!(
            r#"<Relationships xmlns="{}"><Relationship Id="rId1" "#,
            r#"Type="{}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#,
        ),
        REL, OFFICE_REL
    ))
}

/// ref synthetic-deck.ts:41-43
fn slide_layout() -> String {
    xml(&format!(
        concat!(
            r#"<p:sldLayout xmlns:p="{}" xmlns:a="{}"><p:cSld><p:spTree>"#,
            r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>"#,
            r#"<p:grpSpPr/></p:spTree></p:cSld></p:sldLayout>"#,
        ),
        P, A
    ))
}

/// ref synthetic-deck.ts:44-45
fn slide_layout_rels() -> String {
    xml(&format!(
        concat!(
            r#"<Relationships xmlns="{}"><Relationship Id="rId1" "#,
            r#"Type="{}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#,
        ),
        REL, OFFICE_REL
    ))
}

/// ref synthetic-deck.ts:46-51
fn slide_master() -> String {
    xml(&format!(
        concat!(
            r#"<p:sldMaster xmlns:p="{}" xmlns:a="{}" xmlns:r="{}"><p:cSld><p:spTree>"#,
            r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>"#,
            r#"<p:grpSpPr/></p:spTree></p:cSld>"#,
            r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" "#,
            r#"accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" "#,
            r#"accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#,
            r#"<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/>"#,
            r#"</p:sldLayoutIdLst></p:sldMaster>"#,
        ),
        P, A, R
    ))
}

/// ref synthetic-deck.ts:52-55
fn slide_master_rels() -> String {
    xml(&format!(
        concat!(
            r#"<Relationships xmlns="{}">"#,
            r#"<Relationship Id="rId1" Type="{}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#,
            r#"<Relationship Id="rId2" Type="{}/theme" Target="../theme/theme1.xml"/></Relationships>"#,
        ),
        REL, OFFICE_REL, OFFICE_REL
    ))
}

/// ref synthetic-deck.ts:56-64
fn theme() -> String {
    xml(&format!(
        concat!(
            r#"<a:theme xmlns:a="{}" name="T"><a:themeElements>"#,
            r#"<a:clrScheme name="C"><a:dk1><a:srgbClr val="000000"/></a:dk1>"#,
            r#"<a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F497D"/></a:dk2>"#,
            r#"<a:lt2><a:srgbClr val="EEECE1"/></a:lt2>"#,
            r#"<a:accent1><a:srgbClr val="4F81BD"/></a:accent1>"#,
            r#"<a:accent2><a:srgbClr val="C0504D"/></a:accent2>"#,
            r#"<a:accent3><a:srgbClr val="9BBB59"/></a:accent3>"#,
            r#"<a:accent4><a:srgbClr val="8064A2"/></a:accent4>"#,
            r#"<a:accent5><a:srgbClr val="4BACC6"/></a:accent5>"#,
            r#"<a:accent6><a:srgbClr val="F79646"/></a:accent6>"#,
            r#"<a:hlink><a:srgbClr val="0000FF"/></a:hlink>"#,
            r#"<a:folHlink><a:srgbClr val="800080"/></a:folHlink></a:clrScheme>"#,
            r#"<a:fontScheme name="F"><a:majorFont><a:latin typeface="Calibri Light"/>"#,
            r#"<a:ea typeface=""/><a:cs typeface=""/></a:majorFont>"#,
            r#"<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/>"#,
            r#"<a:cs typeface=""/></a:minorFont></a:fontScheme>"#,
            r#"<a:fmtScheme name="M"><a:fillStyleLst><a:noFill/><a:noFill/><a:noFill/></a:fillStyleLst>"#,
            r#"<a:lnStyleLst><a:ln/><a:ln/><a:ln/></a:lnStyleLst>"#,
            r#"<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle>"#,
            r#"<a:effectStyle><a:effectLst/></a:effectStyle>"#,
            r#"<a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>"#,
            r#"<a:bgFillStyleLst><a:noFill/><a:noFill/><a:noFill/></a:bgFillStyleLst>"#,
            r#"</a:fmtScheme></a:themeElements></a:theme>"#,
        ),
        A
    ))
}

/// MS-DOS epoch, the earliest representable ZIP timestamp.
fn zip_epoch() -> DateTime {
    // `DateTime::default()` is documented as 1980-01-01 00:00:00; the explicit
    // constructor keeps the intent visible and falls back to that same value.
    DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap_or_default()
}

/// Deterministic stored-only ZIP writer. Mirrors the reference `storedZip`
/// (`packages/core/src/testing/non-ooxml-fixture.ts:46`): entries appear in
/// the order given, uncompressed, with fixed metadata, so the produced bytes
/// depend only on the part list.
fn stored_zip(files: &[(String, String)]) -> Vec<u8> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip_epoch())
        .system(System::Unix)
        .unix_permissions(0o644);

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in files {
        writer
            .start_file(name.as_str(), options)
            .expect("synthetic deck: start_file in an in-memory ZIP cannot fail");
        writer
            .write_all(body.as_bytes())
            .expect("synthetic deck: writing to an in-memory ZIP cannot fail");
    }
    writer
        .finish()
        .expect("synthetic deck: finishing an in-memory ZIP cannot fail")
        .into_inner()
}
