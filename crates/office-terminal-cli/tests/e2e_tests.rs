use office_terminal_cli::cli::{ExportFormat, FitMode};
use office_terminal_cli::commands::export::run_export_with_options;
use office_terminal_cli::commands::{run_dump, run_export, run_info, run_view, source_from_path};
use office_terminal_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, DefaultSlideSvgSerializer, PresentationLoader, Source,
    SvgTextMode, TinySkiaRasterizer,
};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn create_test_pptx() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zw = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default();

        let mut put = |path: &str, content: &[u8]| {
            zw.start_file(path, opts).unwrap();
            zw.write_all(content).unwrap();
        };

        // [Content_Types].xml
        put(
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
  <Override PartName="/ppt/slides/slide2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
  <Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
  <Override PartName="/ppt/notesSlides/notesSlide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>
  <Override PartName="/ppt/comments/comment1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.comments+xml"/>
  <Override PartName="/ppt/commentAuthors.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml"/>
</Types>"#,
        );

        // _rels/.rels
        put(
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#,
        );

        // ppt/presentation.xml
        put(
            "ppt/presentation.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst>
  <p:sldIdLst>
    <p:sldId id="256" r:id="rIdSlide1"/>
    <p:sldId id="257" r:id="rIdSlide2"/>
  </p:sldIdLst>
  <p:sldSz cx="9144000" cy="5143500"/>
</p:presentation>"#,
        );

        // ppt/_rels/presentation.xml.rels
        put(
            "ppt/_rels/presentation.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
  <Relationship Id="rIdSlide1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
  <Relationship Id="rIdSlide2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/>
  <Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
  <Relationship Id="rIdAuthors" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/commentAuthors" Target="commentAuthors.xml"/>
</Relationships>"#,
        );

        // ppt/theme/theme1.xml
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

        // ppt/slideMasters/slideMaster1.xml
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

        // ppt/slideMasters/_rels/slideMaster1.xml.rels
        put(
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#,
        );

        // ppt/slideLayouts/slideLayout1.xml
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

        // ppt/slideLayouts/_rels/slideLayout1.xml.rels
        put(
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#,
        );

        // ppt/slides/slide1.xml
        put(
            "ppt/slides/slide1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name="g"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
    <p:sp>
      <p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
      <p:spPr><a:xfrm><a:off x="500000" y="500000"/><a:ext cx="4000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
      <p:txBody><a:bodyPr/><a:p><a:r><a:t>Slide One Title</a:t></a:r></a:p></p:txBody>
    </p:sp>
    <p:pic>
      <p:nvPicPr><p:cNvPr id="3" name="Picture 1"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
      <p:blipFill><a:blip r:embed="rIdImg1"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
      <p:spPr><a:xfrm><a:off x="1000000" y="2000000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
    </p:pic>
  </p:spTree></p:cSld>
</p:sld>"#,
        );

        // ppt/slides/_rels/slide1.xml.rels
        put(
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
  <Relationship Id="rIdNotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/>
  <Relationship Id="rIdComments" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments/comment1.xml"/>
  <Relationship Id="rIdImg1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
</Relationships>"#,
        );

        // ppt/slides/slide2.xml (Hidden slide)
        put(
            "ppt/slides/slide2.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" show="0">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name="g"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
    <p:sp>
      <p:nvSpPr><p:cNvPr id="2" name="Title 2"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
      <p:spPr><a:xfrm><a:off x="500000" y="500000"/><a:ext cx="4000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
      <p:txBody><a:bodyPr/><a:p><a:r><a:t>Hidden Slide Two</a:t></a:r></a:p></p:txBody>
    </p:sp>
  </p:spTree></p:cSld>
</p:sld>"#,
        );

        // ppt/slides/_rels/slide2.xml.rels
        put(
            "ppt/slides/_rels/slide2.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#,
        );

        // ppt/notesSlides/notesSlide1.xml
        put(
            "ppt/notesSlides/notesSlide1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree><a:p><a:r><a:t>Speaker note for slide 1</a:t></a:r></a:p></p:spTree></p:cSld>
</p:notes>"#,
        );

        // ppt/comments/comment1.xml
        put(
            "ppt/comments/comment1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:cmLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cm authorId="0" dt="2024-01-01T12:00:00Z"><p:text>Great slide presentation!</p:text></p:cm>
</p:cmLst>"#,
        );

        // ppt/commentAuthors.xml
        put(
            "ppt/commentAuthors.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:cmAuthorLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cmAuthor id="0" name="Alice Reviewer" initials="AR"/>
</p:cmAuthorLst>"#,
        );

        // ppt/media/image1.png: 2x2 red PNG
        let img = image::RgbaImage::from_fn(2, 2, |_, _| image::Rgba([255, 0, 0, 255]));
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .unwrap();
        put("ppt/media/image1.png", &png_bytes);

        zw.finish().unwrap();
    }
    buf
}

#[test]
fn test_source_from_path() {
    let stdin_src = source_from_path(PathBuf::from("-"));
    assert!(matches!(stdin_src, Source::Stdin));

    let file_src = source_from_path(PathBuf::from("my_deck.pptx"));
    match file_src {
        Source::File(p) => assert_eq!(p.to_str().unwrap(), "my_deck.pptx"),
        _ => panic!("Expected Source::File"),
    }
}

#[test]
fn test_run_info_and_deck_parsing() {
    let pptx_bytes = create_test_pptx();
    let temp_file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(temp_file.path(), &pptx_bytes).expect("write bytes");

    let loader = DefaultPresentationLoader;
    let code = run_info(temp_file.path().to_path_buf(), &loader);
    assert_eq!(code, ExitCode::SUCCESS);

    // Non-existent file should return exit code 2
    let bad_code = run_info(PathBuf::from("/non/existent/deck.pptx"), &loader);
    assert_eq!(bad_code, ExitCode::from(2));
}

#[test]
fn test_run_dump_all_variants() {
    let pptx_bytes = create_test_pptx();
    let temp_file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(temp_file.path(), &pptx_bytes).expect("write bytes");

    let loader = DefaultPresentationLoader;
    let path = temp_file.path().to_path_buf();

    // 1. --notes
    let code_notes = run_dump(path.clone(), true, false, false, false, false, &loader);
    assert_eq!(code_notes, ExitCode::SUCCESS);

    // 2. --comments
    let code_comments = run_dump(path.clone(), false, true, false, false, false, &loader);
    assert_eq!(code_comments, ExitCode::SUCCESS);

    // 3. --markdown
    let code_md = run_dump(path.clone(), false, false, true, false, false, &loader);
    assert_eq!(code_md, ExitCode::SUCCESS);

    // 4. --json
    let code_json = run_dump(path.clone(), false, false, false, true, false, &loader);
    assert_eq!(code_json, ExitCode::SUCCESS);

    // 5. No flags passed: prints summary, exits 0
    let code_none = run_dump(path, false, false, false, false, false, &loader);
    assert_eq!(code_none, ExitCode::SUCCESS);
}

#[test]
fn test_run_export_png_and_svg() {
    let pptx_bytes = create_test_pptx();
    let temp_file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(temp_file.path(), &pptx_bytes).expect("write bytes");

    let out_dir = tempfile::tempdir().expect("temp dir");

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    // Export both PNG and SVG
    let code = run_export(
        temp_file.path().to_path_buf(),
        out_dir.path().to_path_buf(),
        ExportFormat::All,
        96,
        Some("1".to_string()),
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(code, ExitCode::SUCCESS);

    let png_path = out_dir.path().join("slide_001.png");
    let svg_path = out_dir.path().join("slide_001.svg");
    assert!(png_path.exists(), "PNG should have been exported");
    assert!(svg_path.exists(), "SVG should have been exported");

    // Invalid slide range should return exit code 1
    let bad_range_code = run_export(
        temp_file.path().to_path_buf(),
        out_dir.path().to_path_buf(),
        ExportFormat::Png,
        96,
        Some("99".to_string()),
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(bad_range_code, ExitCode::from(1));
}

#[test]
fn test_run_view_tty_check_and_execution() {
    let pptx_bytes = create_test_pptx();
    let temp_file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(temp_file.path(), &pptx_bytes).expect("write bytes");

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());

    // When stdout is not a TTY (simulated via OT_FORCE_NOT_TTY), exit code is 3
    std::env::set_var("OT_FORCE_NOT_TTY", "1");
    let code_no_tty = run_view(
        temp_file.path().to_path_buf(),
        1,
        false,
        FitMode::Contain,
        &loader,
        &compiler,
        &rasterizer,
    );
    assert_eq!(code_no_tty, ExitCode::from(3));
    std::env::remove_var("OT_FORCE_NOT_TTY");

    // When TTY and Kitty graphics are forced, slide 1 renders successfully.
    std::env::set_var("OT_FORCE_TTY", "1");
    std::env::set_var("OT_PROTOCOL", "kitty");
    let code_ok = run_view(
        temp_file.path().to_path_buf(),
        1,
        false,
        FitMode::Contain,
        &loader,
        &compiler,
        &rasterizer,
    );
    assert_eq!(code_ok, ExitCode::SUCCESS);

    // When slide is out of range, returns exit code 1
    let code_out_of_bounds = run_view(
        temp_file.path().to_path_buf(),
        99,
        false,
        FitMode::Contain,
        &loader,
        &compiler,
        &rasterizer,
    );
    assert_eq!(code_out_of_bounds, ExitCode::from(1));

    // When --all is true, it renders all slides sequentially
    let code_all = run_view(
        temp_file.path().to_path_buf(),
        1,
        true,
        FitMode::Width,
        &loader,
        &compiler,
        &rasterizer,
    );
    assert_eq!(code_all, ExitCode::SUCCESS);

    std::env::remove_var("OT_PROTOCOL");
    std::env::remove_var("OT_FORCE_TTY");
}

#[test]
fn test_stdin_source_loading() {
    let pptx_bytes = create_test_pptx();
    let loader = DefaultPresentationLoader;
    let (pres, assets) = loader.load(Source::Bytes(pptx_bytes)).expect("load bytes");

    assert_eq!(pres.slides.len(), 2);
    assert!(!pres.slides[0].hidden);
    assert!(pres.slides[1].hidden);
    assert_eq!(pres.slides[0].notes.as_deref(), Some("Speaker note for slide 1"));
    assert_eq!(pres.slides[0].comments.len(), 1);
    assert_eq!(pres.slides[0].comments[0].text, "Great slide presentation!");

    // Verify raw bytes retrieval
    let raw = assets.get_raw_bytes("").expect("archive bytes");
    assert!(!raw.is_empty());
}

#[test]
fn test_export_selectable_svg_mode() {
    let pptx_bytes = create_test_pptx();
    let temp_file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(temp_file.path(), &pptx_bytes).expect("write bytes");
    let out_dir = tempfile::tempdir().expect("temp dir");

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    let code = run_export_with_options(
        temp_file.path().to_path_buf(),
        out_dir.path().to_path_buf(),
        ExportFormat::Svg,
        96,
        None,
        Some("1".to_string()),
        SvgTextMode::SelectableText,
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    let svg = std::fs::read_to_string(out_dir.path().join("slide_001.svg")).expect("svg");
    assert!(svg.contains("<text"), "selectable mode keeps real text tags");
    assert!(svg.contains("Slide One Title"), "the title text is selectable");
}
