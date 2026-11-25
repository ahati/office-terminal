//! L5 CLI end-to-end coverage for a metafile deck (PRD 13a §5).
//!
//! Builds a minimal PPTX with one EMF picture, then drives the real binary:
//! `export` must paint it, `dump --json` must contain expanded commands (no
//! `drawMetafile`), `--metafile placeholder` must produce the §3 box, and
//! `info` must count the media.

use std::io::{Cursor, Write};
use std::process::Command;

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn record(kind: u32, payload: &[u8]) -> Vec<u8> {
    let size = 8 + payload.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn i32s(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// EMF with a blue-filled rectangle: `EMR_HEADER` + brush/select/rect + EOF.
fn emf_with_rectangle() -> Vec<u8> {
    let mut data = vec![0u8; 88];
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[4..8].copy_from_slice(&88u32.to_le_bytes());
    data[8..24].copy_from_slice(&i32s(&[0, 0, 200, 100]));
    data[24..40].copy_from_slice(&i32s(&[0, 0, 5292, 2646]));
    data[40..44].copy_from_slice(b" EMF");
    data[44..48].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    data[56..58].copy_from_slice(&32u16.to_le_bytes());
    data[72..80].copy_from_slice(&i32s(&[1920, 1080]));
    data[80..88].copy_from_slice(&i32s(&[508, 286]));

    let mut brush = Vec::new();
    brush.extend_from_slice(&2u32.to_le_bytes()); // handle
    brush.extend_from_slice(&0u32.to_le_bytes()); // BS_SOLID
    brush.extend_from_slice(&0x00FF_0000u32.to_le_bytes()); // blue (COLORREF)
    brush.extend_from_slice(&0u32.to_le_bytes()); // hatch
    data.extend_from_slice(&record(39, &brush));
    data.extend_from_slice(&record(37, &2u32.to_le_bytes()));
    data.extend_from_slice(&record(43, &i32s(&[20, 20, 180, 80])));
    data.extend_from_slice(&record(14, &[0u8; 12]));

    let length = data.len() as u32;
    data[48..52].copy_from_slice(&length.to_le_bytes());
    data[52..56].copy_from_slice(&5u32.to_le_bytes());
    data
}

fn deck_bytes() -> Vec<u8> {
    let opts = SimpleFileOptions::default();
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let mut put = |path: &str, content: &[u8]| {
            zip.start_file(path, opts).unwrap();
            zip.write_all(content).unwrap();
        };

        put(
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="emf" ContentType="image/x-emf"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
  <Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"#,
        );
        put(
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#,
        );
        put(
            "ppt/presentation.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst>
  <p:sldIdLst><p:sldId id="256" r:id="rIdSlide1"/></p:sldIdLst>
  <p:sldSz cx="9144000" cy="5143500"/>
</p:presentation>"#,
        );
        put(
            "ppt/_rels/presentation.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
  <Relationship Id="rIdSlide1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
  <Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
</Relationships>"#,
        );
        put(
            "ppt/theme/theme1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme">
  <a:themeElements>
    <a:clrScheme name="Office">
      <a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="111111"/></a:dk2><a:lt2><a:srgbClr val="EEEEEE"/></a:lt2>
      <a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme>
    <a:fontScheme name="Office">
      <a:majorFont><a:latin typeface="Calibri Light"/></a:majorFont>
      <a:minorFont><a:latin typeface="Calibri"/></a:minorFont>
    </a:fontScheme>
    <a:fmtScheme name="Office"><a:fillStyleLst/><a:lnStyleLst/><a:effectStyleLst/><a:bgFillStyleLst/></a:fmtScheme>
  </a:themeElements>
</a:theme>"#,
        );
        put(
            "ppt/slideMasters/slideMaster1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name="g"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
  </p:spTree></p:cSld>
  <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2"
    accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
  <p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rIdLayout"/></p:sldLayoutIdLst>
</p:sldMaster>"#,
        );
        put(
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#,
        );
        put(
            "ppt/slideLayouts/slideLayout1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name="g"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
  </p:spTree></p:cSld>
</p:sldLayout>"#,
        );
        put(
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#,
        );
        put(
            "ppt/slides/slide1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name="g"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
    <p:pic>
      <p:nvPicPr><p:cNvPr id="2" name="EMF picture"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
      <p:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
      <p:spPr><a:xfrm><a:off x="500000" y="500000"/><a:ext cx="4000000" cy="2000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
    </p:pic>
  </p:spTree></p:cSld>
</p:sld>"#,
        );
        put(
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
  <Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.emf"/>
</Relationships>"#,
        );
        put("ppt/media/image1.emf", &emf_with_rectangle());
        zip.finish().unwrap();
    }
    buffer.into_inner()
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ot"))
        .args(args)
        .output()
        .expect("run ot")
}

#[test]
fn cli_export_dump_and_info_agree_on_metafile_decks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let deck = dir.path().join("deck.pptx");
    std::fs::write(&deck, deck_bytes()).expect("write deck");
    let out = dir.path().join("out");
    let deck_arg = deck.to_str().unwrap();
    let out_arg = out.to_str().unwrap();

    let exported = run(&[
        "export", deck_arg, "--out", out_arg, "--format", "png", "--width", "800",
    ]);
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let png = image::open(out.join("slide_001.png")).expect("exported png");
    let rgb = png.to_rgba8();
    assert!(
        rgb.pixels().any(|pixel| pixel.0 != [255, 255, 255, 255]),
        "export must paint the EMF picture"
    );

    let dumped = run(&["dump", deck_arg, "--json"]);
    assert!(dumped.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&dumped.stdout).expect("dump json");
    let commands = &envelope["vectorSlides"][0]["scene"]["commands"];
    let types: Vec<&str> = commands
        .as_array()
        .expect("commands")
        .iter()
        .filter_map(|command| command["type"].as_str())
        .collect();
    assert!(types.contains(&"drawPath"), "{types:?}");
    assert!(!types.contains(&"drawMetafile"), "expansion must remove markers");
    assert!(envelope["warnings"].as_array().is_some());

    let placeholder = run(&["dump", deck_arg, "--json", "--metafile", "placeholder"]);
    assert!(placeholder.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&placeholder.stdout).expect("placeholder json");
    let text: String = envelope["vectorSlides"][0]["scene"]["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .filter_map(|command| command["runs"][0]["text"].as_str())
        .collect();
    assert!(text.contains("METAFILE"), "placeholder text missing: {text:?}");

    let info = run(&["info", deck_arg]);
    assert!(info.status.success());
    let text = String::from_utf8_lossy(&info.stdout);
    assert!(text.contains("Media Assets"), "{text}");

    let embedded = run(&["dump", deck_arg, "--json", "--embed-bitmaps"]);
    assert!(
        embedded.status.success(),
        "{}",
        String::from_utf8_lossy(&embedded.stderr)
    );
}
