//! Font-resolution parity: authored family names, the theme/paragraph/run
//! cascade, serif-vs-sans substitute classes, embedded-font slots and font-part
//! extraction.
//!
//! Ported from `packages/pptx/src/*.test.ts` (reference read-only @
//! `1f5c6e1d`). The reference decides families while building **CSS font
//! stacks** (`cssFontStack` / `buildFont` in `renderer.ts`) and while loading
//! `FontFace` objects; this port carries one concrete family per run and
//! resolves it in `compiler/text.rs::resolve_font_family`, so every case below
//! is asserted on the observable family string of the compiled display list
//! (`PositionedGlyphRun::font_family`), on the SVG export of the same run, or
//! on the parser's embedded-font projection. Cases whose reference API has no
//! port counterpart are recorded as N/A in
//! `tests/parity/coverage/fonts.md` with the reason.

#[path = "parity_support/deck.rs"]
mod deck_support;

use crossbeam_channel::unbounded;
use deck_support::{deck, text_box, DEFAULT_BODY_PR, SLIDE_HEIGHT, SLIDE_WIDTH};
use kitty_pptx_core::compiler::text::resolve_font_family;
use kitty_pptx_core::contracts::{PositionedGlyphRun, VectorCommand, VectorSlide};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::{
    AssetError, DefaultPresentationLoader, DefaultSlideCompiler, DefaultSlideSvgSerializer,
    PresentationLoader, SlideCompiler, SlideSvgSerializer, Source, SvgTextMode,
};

/// The reference probes' default body metrics.
const POINTS: f32 = 20.0;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// One paragraph with one text run, over exactly the fields
/// `text::emit::build_paragraphs` reads. ref types.ts `TextBody` /
/// `Paragraph` / `TextRun`.
fn json_str(value: Option<&str>) -> String {
    match value {
        Some(text) => serde_json::to_string(text).expect("JSON string literal"),
        None => "null".to_string(),
    }
}

fn body(
    def_family: Option<&str>,
    run_family: Option<&str>,
    bold: bool,
    text: &str,
) -> pptx_model::TextBody {
    let json = format!(
        concat!(
            r#"{{"verticalAnchor": "t", "defaultFontSize": {POINTS},"#,
            r#""lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,"#,
            r#""wrap": "none", "vert": "horz", "autoFit": "none","#,
            r#""numCol": 1, "spcCol": 0, "rtlCol": false,"#,
            r#""spcFirstLastPara": false, "anchorCtr": false,"#,
            r#""paragraphs": [{{"alignment": "l","#,
            r#""marL": 0, "marR": 0, "indent": 0, "spaceBefore": null, "lvl": 0,"#,
            r#""bullet": {{"type": "none"}}, "defFontSize": {POINTS},"#,
            r#""defFontFamily": {def}, "tabStops": [],"#,
            r#""rtl": false, "eaLnBrk": true,"#,
            r#""runs": [{{"type": "text", "text": {text},"#,
            r#""bold": {bold}, "italic": false, "underline": false,"#,
            r#""strikethrough": false, "strikeDouble": false,"#,
            r#""hyperlinkUsesTextFill": false, "fieldType": null,"#,
            r#""fontSize": {POINTS}, "color": "000000","#,
            r#""fontFamily": {run}}}]}}]}}"#,
        ),
        POINTS = POINTS,
        bold = bold,
        def = json_str(def_family),
        run = json_str(run_family),
        text = json_str(Some(text)),
    );
    serde_json::from_str(&json).expect("text body fixture")
}

fn compile(text_body: &pptx_model::TextBody) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        text_body,
        3_200_400,
        2_743_200,
        &kitty_pptx_core::contracts::AffineTransform::identity(),
        None,
        &tx,
    )
}

fn runs(commands: &[VectorCommand]) -> Vec<PositionedGlyphRun> {
    let mut out = Vec::new();
    for command in commands {
        if let VectorCommand::DrawText { runs, .. } = command {
            out.extend(runs.iter().cloned());
        }
    }
    out
}

/// Compile one run and return its (family, bold) pair — the single observable
/// the reference encodes into its CSS `font` string.
fn compiled_face(
    def_family: Option<&str>,
    run_family: Option<&str>,
    bold: bool,
    text: &str,
) -> (String, bool) {
    let commands = compile(&body(def_family, run_family, bold, text));
    let runs = runs(&commands);
    let run = runs.first().expect("one painted run");
    (run.font_family.clone(), run.bold)
}

fn substituted(name: &str) -> String {
    let (tx, _rx) = unbounded();
    resolve_font_family(name, &tx)
}

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn svg_for(slide: &VectorSlide) -> String {
    DefaultSlideSvgSerializer::new()
        .serialize_svg(slide, &NoAssets, SvgTextMode::SelectableText)
        .expect("svg serialization")
}

// ---------------------------------------------------------------------------
// font-stack.test.ts — serif/sans generic classification (core classifier)
// ---------------------------------------------------------------------------

/// ref font-stack.test.ts `Cambria degrades to a serif (latent pptx fix)`.
///
/// The reference reaches the serif generic through `endsWith('serif')` and only
/// inserts `"Caladea"` once its substitution is loaded. The port bundles
/// Caladea as the Cambria metric face, so the substitute *is* the classification
/// result; the assertion is on that concrete family instead.
#[test]
fn cambria_degrades_to_the_serif_substitute() {
    assert_eq!(substituted("Cambria"), "Caladea");
    let (family, bold) = compiled_face(None, Some("Cambria"), false, "Cambria probe");
    assert_eq!(family, "Caladea");
    assert!(!bold);
}

/// ref font-stack.test.ts `regression: Calibri stays sans, Times New Roman stays
/// serif`. The port resolves each family to its bundled/host metric face; the
/// sans/serif split must survive that mapping.
#[test]
fn calibri_stays_sans_and_times_new_roman_stays_serif() {
    assert_eq!(substituted("Calibri"), "Carlito");
    assert_eq!(substituted("Times New Roman"), "Liberation Serif");
    assert_eq!(compiled_face(None, Some("Calibri"), false, "x").0, "Carlito");
    assert_eq!(
        compiled_face(None, Some("Times New Roman"), false, "x").0,
        "Liberation Serif"
    );
}

/// ref font-stack.test.ts `lets an explicit bold run override a named Medium
/// face`. The reference emits `bold 48px "Franklin Gothic Medium"`; the port
/// carries weight as `bold: bool` on the run, which must win over the face name.
#[test]
fn explicit_bold_run_overrides_a_named_medium_face() {
    let (family, bold) = compiled_face(None, Some("Franklin Gothic Medium"), true, "Bold face");
    assert_eq!(family, "Franklin Gothic Medium");
    assert!(bold);
}

/// ref font-stack.test.ts `keeps unnamed families regular and recognizes
/// hyphenated bold names`.
///
/// The name half of the contract holds: `Yu Gothic` stays regular and keeps its
/// spelling, and the hyphenated `Courier-Bold` still routes to the monospace
/// class. The weight half (`700`) cannot hold: the port carries weight as a
/// `bold` flag from `a:rPr@b` and never parses style tokens out of a face name.
#[test]
fn unnamed_families_stay_regular() {
    let (family, bold) = compiled_face(None, Some("Yu Gothic"), false, "Regular face");
    assert_eq!(family, "Yu Gothic");
    assert!(!bold);
    assert_eq!(
        compiled_face(None, Some("Courier-Bold"), false, "mono").0,
        "Liberation Mono"
    );
}

/// ref font-stack.test.ts `uses the same resolved alias stack for measurement
/// and paint`. Measurement, paint and export must all see one family string:
/// the run that layout measured is the run the SVG serialiser writes.
///
/// The probe text is East Asian with an empty `a:ea` slot, so the face the run
/// resolves is PowerPoint's application default for that slot (issue #1627:
/// `Noto Sans CJK SC` is not an installed Office face, so the sans tier applies);
/// measure, paint and export must still agree on it.
#[test]
fn measurement_paint_and_export_share_one_resolved_family() {
    let commands = compile(&body(None, Some("Noto Sans CJK SC"), false, "中文"));
    let painted = runs(&commands);
    let run = painted.first().expect("painted CJK run");
    assert_eq!(run.font_family, "MS Gothic");
    assert_eq!(run.text, "中文");

    let slide = VectorSlide {
        width_emu: SLIDE_WIDTH,
        height_emu: SLIDE_HEIGHT,
        background: None,
        commands,
        hyperlinks: Vec::new(),
    };
    let svg = svg_for(&slide);
    assert!(
        svg.contains(r#"font-family="MS Gothic""#),
        "SVG must name the same family the run carries: {svg}"
    );
}

// ---------------------------------------------------------------------------
// font-family-serialization.test.ts — authored names survive verbatim
// ---------------------------------------------------------------------------

/// The reference decodes the CSS string literals back with `JSON.parse` so the
/// requested names must survive; the port stores the family as a plain string,
/// so the same contract is "the authored name reaches the run unchanged".
fn survived_verbatim(authored: &str) -> String {
    compiled_face(None, Some(authored), false, "name probe").0
}

/// ref font-family-serialization.test.ts `preserves quotes in the primary
/// authored family as one CSS string`.
#[test]
fn preserves_quotes_in_the_authored_family() {
    assert_eq!(survived_verbatim("Face \"Quoted\""), "Face \"Quoted\"");
}

/// ref font-family-serialization.test.ts `preserves a literal backslash and
/// inserts complete application defaults after an escaped primary`.
///
/// Only the primary survives here: the application-default *tail* is a CSS
/// stack concept the port does not have (see the coverage report).
#[test]
fn preserves_a_literal_backslash_in_the_authored_family() {
    let authored = "Face \\31 \"Quoted\"";
    assert_eq!(survived_verbatim(authored), authored);
}

/// ref font-family-serialization.test.ts `keeps replacement tokens in default
/// family names literal` — `$&` is literal text, never a substitution token.
#[test]
fn keeps_replacement_tokens_literal() {
    assert_eq!(survived_verbatim("Default $&"), "Default $&");
}

// ---------------------------------------------------------------------------
// font-requests.test.ts — theme, paragraph and run resolution
// ---------------------------------------------------------------------------

fn deck_families(paragraphs: &str) -> Vec<(String, String)> {
    let bytes = deck(&text_box(paragraphs, DEFAULT_BODY_PR));
    let (presentation, _assets) = DefaultPresentationLoader
        .load(Source::Bytes(bytes))
        .expect("synthetic deck loads");
    let (tx, _rx) = unbounded();
    let slide = DefaultSlideCompiler::new()
        .compile(&presentation.slides[0], SLIDE_WIDTH, SLIDE_HEIGHT, &tx)
        .expect("slide compiles");
    let mut out = Vec::new();
    for command in &slide.commands {
        if let VectorCommand::DrawText { runs, .. } = command {
            for run in runs {
                out.push((run.text.clone(), run.font_family.clone()));
            }
        }
    }
    out
}

/// ref font-requests.test.ts `resolves theme, paragraph, and run style for every
/// authored face`.
///
/// Three paragraphs: the first inherits the theme minor face through `+mn-lt`,
/// the second names a paragraph face on `a:defRPr`, the third overrides it on
/// the run. Each painted run must report the face its own level resolved to.
#[test]
fn resolves_theme_paragraph_and_run_faces() {
    let paragraphs = concat!(
        r#"<a:p><a:pPr><a:defRPr><a:latin typeface="+mn-lt"/></a:defRPr></a:pPr>"#,
        r#"<a:r><a:rPr lang="en-US" sz="2000"/><a:t>theme</a:t></a:r></a:p>"#,
        r#"<a:p><a:pPr><a:defRPr><a:latin typeface="Franklin Gothic Medium"/>"#,
        r#"</a:defRPr></a:pPr>"#,
        r#"<a:r><a:rPr lang="en-US" sz="2000"/><a:t>paragraph</a:t></a:r></a:p>"#,
        r#"<a:p><a:pPr><a:defRPr><a:latin typeface="Franklin Gothic Medium"/>"#,
        r#"</a:defRPr></a:pPr>"#,
        r#"<a:r><a:rPr lang="en-US" sz="2000"><a:latin typeface="Arial"/></a:rPr>"#,
        r#"<a:t>run</a:t></a:r></a:p>"#,
    );
    let families = deck_families(paragraphs);
    let by_text = |needle: &str| {
        families
            .iter()
            .find(|(text, _)| text == needle)
            .map(|(_, family)| family.clone())
            .unwrap_or_else(|| panic!("no run painted {needle:?}: {families:?}"))
    };
    assert_eq!(by_text("theme"), "Carlito");
    assert_eq!(by_text("paragraph"), "Franklin Gothic Medium");
    assert_eq!(by_text("run"), "Liberation Sans");
}

/// ref font-requests.test.ts `includes slide-local paragraph and run families,
/// not only the first theme fonts` (paragraph half; the `a:ea` half has no port
/// counterpart — see the coverage report).
#[test]
fn keeps_slide_local_paragraph_families() {
    let paragraphs = concat!(
        r#"<a:p><a:pPr><a:defRPr><a:latin typeface="Franklin Gothic Medium"/>"#,
        r#"</a:defRPr></a:pPr>"#,
        r#"<a:r><a:rPr lang="en-US" sz="2000"/><a:t>Title</a:t></a:r></a:p>"#,
    );
    assert_eq!(
        deck_families(paragraphs),
        vec![("Title".to_string(), "Franklin Gothic Medium".to_string())]
    );
}

// ---------------------------------------------------------------------------
// embedded-fonts.test.ts — ECMA-376 §19.2.1.9-.10 slots and Font parts
// ---------------------------------------------------------------------------

fn zip_epoch() -> zip::DateTime {
    zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap_or_default()
}

fn stored_zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .last_modified_time(zip_epoch())
        .system(zip::System::Unix)
        .unix_permissions(0o644);
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in files {
        writer.start_file(name.as_str(), options).expect("start_file");
        writer.write_all(body).expect("write entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

/// One `p:embeddedFont` slot: family, slot element name, part path and the part
/// payload (`None` leaves the part out of the package, the reference's damaged
/// package).
struct EmbeddedSlot(&'static str, &'static str, &'static str, Option<&'static [u8]>);

/// The synthetic deck plus `p:embeddedFontLst`, the font relationships and the
/// `fntdata` content type, i.e. the package the reference feeds
/// `loadEmbeddedFonts`. ref embedded-fonts.test.ts `refs`.
fn embedded_deck(slots: &[EmbeddedSlot]) -> Vec<u8> {
    let mut list = String::from("<p:embeddedFontLst>");
    let mut rels = String::new();
    let mut binary: Vec<(String, Vec<u8>)> = Vec::new();
    let mut families: Vec<&str> = Vec::new();
    for (index, EmbeddedSlot(family, style, path, bytes)) in slots.iter().enumerate() {
        if !families.contains(family) {
            families.push(family);
        }
        let id = format!("f{index}");
        list.push_str(&format!(
            r#"<p:embeddedFont><p:font typeface="{family}"/><p:{style} r:id="{id}"/></p:embeddedFont>"#
        ));
        rels.push_str(&format!(
            concat!(
                r#"<Relationship Id="{}" "#,
                r#"Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" "#,
                r#"Target="{}"/>"#,
            ),
            id,
            path.trim_start_matches("ppt/")
        ));
        if let Some(bytes) = bytes {
            binary.push(((*path).to_string(), bytes.to_vec()));
        }
    }
    list.push_str("</p:embeddedFontLst>");

    let mut parts: Vec<(String, Vec<u8>)> =
        deck_support::package_parts("", SLIDE_WIDTH, SLIDE_HEIGHT)
            .into_iter()
            .map(|(name, body)| (name, body.into_bytes()))
            .collect();
    for (name, body) in &mut parts {
        let patch: Option<String> = match name.as_str() {
            "[Content_Types].xml" => Some(String::from_utf8_lossy(body).replace(
                "</Types>",
                r#"<Default Extension="fntdata" ContentType="application/x-font-ttf"/></Types>"#,
            )),
            "ppt/presentation.xml" => Some(
                String::from_utf8_lossy(body)
                    .replace("</p:presentation>", &format!("{list}</p:presentation>")),
            ),
            "ppt/_rels/presentation.xml.rels" => Some(
                String::from_utf8_lossy(body)
                    .replace("</Relationships>", &format!("{rels}</Relationships>")),
            ),
            _ => None,
        };
        if let Some(text) = patch {
            *body = text.into_bytes();
        }
    }
    parts.extend(binary);
    stored_zip(&parts)
}

/// `embeddedFonts` of the open package, as `(fontName, style, partPath,
/// contentType)` tuples in projection order.
fn bootstrap_embedded_fonts(bytes: Vec<u8>) -> Vec<(String, String, String, String)> {
    let mut archive =
        pptx_parser::PptxArchive::new(bytes, None, None, None).expect("package opens");
    let bootstrap = archive.presentation_bootstrap().expect("bootstrap projects");
    let json: serde_json::Value = serde_json::from_slice(&bootstrap).expect("bootstrap JSON");
    json["embeddedFonts"]
        .as_array()
        .expect("embeddedFonts array")
        .iter()
        .map(|entry| {
            let field = |name: &str| entry[name].as_str().expect("string field").to_string();
            (
                field("fontName"),
                field("style"),
                field("partPath"),
                field("contentType"),
            )
        })
        .collect()
}

/// ref embedded-fonts.test.ts `maps all four PresentationML slots to CSS weight
/// and style`. The reference maps the slots onto `FontFace` descriptors
/// (bold/italic × normal/normal); the port keeps the same four slots as
/// `EmbeddedFontStyle` refs, so the assertion is on slot order and part paths.
#[test]
fn maps_all_four_embedded_font_slots() {
    const BYTES: &[u8] = &[0, 1, 0, 0, 1];
    let deck = embedded_deck(&[
        EmbeddedSlot("Deck Sans", "regular", "ppt/fonts/font1.fntdata", Some(BYTES)),
        EmbeddedSlot("Deck Sans", "bold", "ppt/fonts/font2.fntdata", Some(BYTES)),
        EmbeddedSlot("Deck Sans", "italic", "ppt/fonts/font3.fntdata", Some(BYTES)),
        EmbeddedSlot("Deck Sans", "boldItalic", "ppt/fonts/font4.fntdata", Some(BYTES)),
    ]);
    let refs = bootstrap_embedded_fonts(deck);
    let tuples: Vec<(String, String)> = refs
        .iter()
        .map(|(family, style, _, _)| (family.clone(), style.clone()))
        .collect();
    assert_eq!(
        tuples,
        vec![
            ("Deck Sans".to_string(), "regular".to_string()),
            ("Deck Sans".to_string(), "bold".to_string()),
            ("Deck Sans".to_string(), "italic".to_string()),
            ("Deck Sans".to_string(), "boldItalic".to_string()),
        ]
    );
    let paths: Vec<&str> = refs.iter().map(|(_, _, path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "ppt/fonts/font1.fntdata",
            "ppt/fonts/font2.fntdata",
            "ppt/fonts/font3.fntdata",
            "ppt/fonts/font4.fntdata",
        ]
    );
    assert!(refs
        .iter()
        .all(|(_, _, _, content_type)| content_type == "application/x-font-ttf"));
    // All four slots name one family: the reference asserts `Set(family).size == 1`.
    let families: std::collections::BTreeSet<&String> =
        refs.iter().map(|(family, _, _, _)| family).collect();
    assert_eq!(families.len(), 1);
}

/// ref embedded-fonts.test.ts `keeps raw PPTX bytes and skips an unreadable part
/// without aborting siblings`.
///
/// The port keeps the Font part as raw bytes (`extract_font`) and drops a ref
/// whose part is missing from the package instead of failing the whole list, so
/// both halves of the contract are observable without a `FontFace` host.
#[test]
fn keeps_raw_font_bytes_and_skips_an_unreadable_part() {
    const RAW: &[u8] = &[0x00, 0x01, 0x00, 0x00, 0xFF, 0x80];
    let deck = embedded_deck(&[
        EmbeddedSlot("Good", "regular", "ppt/fonts/good.fntdata", Some(RAW)),
        EmbeddedSlot("Missing", "regular", "ppt/fonts/missing.fntdata", None),
    ]);
    let refs = bootstrap_embedded_fonts(deck.clone());
    assert_eq!(
        refs.iter()
            .map(|(family, _, path, _)| (family.as_str(), path.as_str()))
            .collect::<Vec<_>>(),
        vec![("Good", "ppt/fonts/good.fntdata")]
    );
    // The retained bytes are the part's own bytes, never a text transcoding.
    let extracted = pptx_parser::extract_font(&deck, "ppt/fonts/good.fntdata", None, None)
        .expect("readable font part");
    assert_eq!(extracted, RAW);
}
