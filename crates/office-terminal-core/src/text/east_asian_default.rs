//! PowerPoint's application defaults for an empty East Asian / complex-script
//! slot (issue #1627, #1653).
//!
//! 1:1 port of `packages/pptx/src/east-asian-default.ts` (reference revision
//! 1f5c6e1d). The parser resolves every authored `a:ea`/`a:cs` face, theme
//! script fonts included, so these apply only when that leaves the slot empty:
//! no face anywhere in the chain, an explicit `typeface=""`, or a token naming
//! an empty theme slot.
//!
//! Observed with PowerPoint's reference PDF engine (#1627 controls D, N, X):
//! a Latin face that is itself an East Asian face (OS/2 Far-East code page: Yu
//! Gothic, SimSun, Batang, PMingLiU, MS Mincho) draws the East Asian text;
//! otherwise the Latin face's installed PANOSE serif style picks the tier —
//! serif (2-10) MS Mincho > PMingLiU > Batang, sans MS Gothic > Microsoft
//! JhengHei > Malgun Gothic — and the first tier face whose repertoire covers
//! the whole East Asian text draws it.
//!
//! The reference reads the two predicates from its bundled `office-mac` face
//! catalogue (`findReferenceFontMetrics`, PANOSE + Far-East code page). This
//! port ships no such catalogue, so [`SERIF_FACES`] / [`EAST_ASIAN_FACES`] carry
//! the faces its specs and the bundled Office decks name; an unlisted face is
//! sans, exactly the reference's "unknown faces are sans".

use super::east_asian_coverage;

/// Serif faces (PANOSE family 2, serif style 2-10) the port knows.
const SERIF_FACES: &[&str] = &[
    "Baskerville",
    "Book Antiqua",
    "Cambria",
    "Caladea",
    "Century Schoolbook",
    "Constantia",
    "Garamond",
    "Georgia",
    "Liberation Serif",
    "Palatino",
    "Palatino Linotype",
    "Perpetua",
    "Times",
    "Times New Roman",
    "Nimbus Roman",
];

/// East Asian faces (OS/2 Far-East code page declared) the port knows.
const EAST_ASIAN_FACES: &[&str] = &[
    "Batang",
    "DengXian",
    "Dotum",
    "FangSong",
    "Gulim",
    "Gungsuh",
    "KaiTi",
    "Malgun Gothic",
    "Meiryo",
    "Microsoft JhengHei",
    "Microsoft YaHei",
    "MingLiU",
    "MS Gothic",
    "MS Mincho",
    "MS PGothic",
    "MS UI Gothic",
    "NSimSun",
    "PMingLiU",
    "SimHei",
    "SimSun",
    "Yu Gothic",
    "Yu Mincho",
];

/// Per-glyph complex-script application defaults, in fallback order
/// (`COMPLEX_SCRIPT_DEFAULT_FACES`).
pub const COMPLEX_SCRIPT_DEFAULT_FACES: &[&str] = &["Arial", "Angsana New", "Mangal", "Latha"];

const SERIF_TIERS: [&str; 3] = ["MS Mincho", "PMingLiU", "Batang"];
const SANS_TIERS: [&str; 3] = ["MS Gothic", "Microsoft JhengHei", "Malgun Gothic"];

/// FACENAME → catalogue membership is case-insensitive, like a font lookup.
fn named(face: &str, list: &[&str]) -> bool {
    list.iter().any(|known| known.eq_ignore_ascii_case(face))
}

/// True when the face declares an OS/2 Far-East code page
/// (`isEastAsianFace`).
pub fn is_east_asian_face(face: &str) -> bool {
    named(face, EAST_ASIAN_FACES)
}

/// PANOSE serif class of the face; unknown faces are sans
/// (`isSerifLatinFace`).
pub fn is_serif_latin_face(face: &str) -> bool {
    named(face, SERIF_FACES)
}

/// `classifyCjkFont(face) === 'jp'`: a Japanese-face follow-up chain.
fn is_japanese_face(face: &str) -> bool {
    let lower = face.to_lowercase();
    ["mincho", "gothic", "meiryo", "yu ", "hiragino", "osaka", "jp"]
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Index into [`COMPLEX_SCRIPT_DEFAULT_FACES`] of the application default for
/// one character (`complexScriptDefaultFace`): Thai in Angsana New, Devanagari
/// in Mangal, Tamil in Latha, everything else in Arial.
pub fn complex_script_default_index(ch: char) -> usize {
    let cp = ch as u32;
    if (0x0e00..=0x0e7f).contains(&cp) {
        1
    } else if (0x0900..=0x097f).contains(&cp) || (0xa8e0..=0xa8ff).contains(&cp) {
        2
    } else if (0x0b80..=0x0bff).contains(&cp) {
        3
    } else {
        0
    }
}

/// The complex-script application default face for one character.
pub fn complex_script_default_face(ch: char) -> &'static str {
    COMPLEX_SCRIPT_DEFAULT_FACES[complex_script_default_index(ch)]
}

/// Faces, in fallback order, for East Asian text drawn with an empty East Asian
/// slot (`eastAsianDefaultFaces`).
pub fn east_asian_default_faces(latin_face: Option<&str>, text: &str) -> Vec<String> {
    if let Some(latin) = latin_face {
        if is_east_asian_face(latin) {
            let tail: [&str; 2] = if is_japanese_face(latin) {
                ["Microsoft JhengHei", "Malgun Gothic"]
            } else {
                ["PMingLiU", "Batang"]
            };
            return vec![latin.to_string(), tail[0].to_string(), tail[1].to_string()];
        }
    }
    let tiers = if latin_face.is_some_and(is_serif_latin_face) {
        SERIF_TIERS
    } else {
        SANS_TIERS
    };
    let first = tiers[..2].iter().copied().find(|face| covers(face, text));
    match first {
        Some(face) => std::iter::once(face)
            .chain(tiers.iter().copied().filter(|other| *other != face))
            .map(str::to_string)
            .collect(),
        None => vec![tiers[2].to_string(), tiers[0].to_string(), tiers[1].to_string()],
    }
}

/// Whether `face` possesses a glyph for every non-whitespace character of
/// `text` (`covers`). Only the two default tiers carry repertoire data.
fn covers(face: &str, text: &str) -> bool {
    text.chars().all(|ch| {
        let cp = ch as u32;
        if ch.is_whitespace() {
            return true;
        }
        match face {
            "MS Mincho" | "MS Gothic" => east_asian_coverage::mincho_covers(cp),
            "PMingLiU" => east_asian_coverage::pmingliu_covers(cp),
            "Microsoft JhengHei" => east_asian_coverage::jhenghei_covers(cp),
            _ => false,
        }
    })
}

/// True when `cp` is a complex-script code point (`isComplexScriptCodePoint`):
/// every RTL script block, joining/Arabic presentation forms included.
pub fn is_complex_script_code_point(cp: u32) -> bool {
    matches!(
        cp,
        0x0590..=0x05ff
            | 0x0600..=0x06ff
            | 0x0700..=0x074f
            | 0x0750..=0x077f
            | 0x0780..=0x07bf
            | 0x07c0..=0x07ff
            | 0x0800..=0x083f
            | 0x0840..=0x085f
            | 0x0860..=0x08ff
            | 0xfb1d..=0xfb4f
            | 0xfb50..=0xfdff
            | 0xfe70..=0xfeff
            | 0x10800..=0x10fff
            | 0x1e800..=0x1efff
    )
}

/// `INDIC_CS_GLYPH_RE` (`renderer.ts:1548`): a complex-script (Indic / SEA)
/// cluster takes the CS slot even where `isComplexScriptCodePoint` says no.
pub fn has_indic_cs_glyph(cluster: &str) -> bool {
    cluster.chars().any(|ch| {
        matches!(
            ch as u32,
            0x0900..=0x097f
                | 0x0980..=0x09ff
                | 0x0a00..=0x0a7f
                | 0x0a80..=0x0aff
                | 0x0b00..=0x0b7f
                | 0x0b80..=0x0bff
                | 0x0c00..=0x0c7f
                | 0x0c80..=0x0cff
                | 0x0d00..=0x0d7f
                | 0x0d80..=0x0dff
                | 0x0e00..=0x0e7f
                | 0x0e80..=0x0eff
                | 0x0f00..=0x0fff
                | 0x1000..=0x109f
                | 0x1780..=0x17ff
                | 0x19e0..=0x19ff
                | 0xa8e0..=0xa8ff
                | 0xa9e0..=0xa9ff
                | 0xaa60..=0xaa7f
        )
    })
}

/// Non-CJK characters PowerPoint draws in a resolved East Asian face
/// (`isEastAsianSlotSymbol`, #1653): `§ ¨ ° ± ´` in every measured language;
/// Enclosed Alphanumerics, Geometric Shapes, Miscellaneous Symbols, Dingbats and
/// CJK Compatibility Forms as blocks; `× ÷ ⁇ ⁈` except in `ar`/`he`/`hi`/`th`
/// runs; curly quotes U+2018-201E in `ja`/`ko`/`zh` runs only (U+201F stays
/// Latin).
pub fn is_east_asian_slot_symbol(cp: u32, lang: Option<&str>) -> bool {
    const EA_SLOT_SYMBOLS: [u32; 5] = [0xa7, 0xa8, 0xb0, 0xb1, 0xb4];
    const EA_SLOT_OPERATORS: [u32; 4] = [0xd7, 0xf7, 0x2047, 0x2048];
    if EA_SLOT_SYMBOLS.contains(&cp)
        || (0x2460..=0x24ff).contains(&cp)
        || (0x25a0..=0x27bf).contains(&cp)
        || (0xfe30..=0xfe4f).contains(&cp)
    {
        return true;
    }
    let language = lang.unwrap_or("");
    if EA_SLOT_OPERATORS.contains(&cp) {
        return !has_lang_prefix(language, &["ar", "he", "hi", "th"]);
    }
    (0x2018..=0x201e).contains(&cp) && has_lang_prefix(language, &["ja", "ko", "zh"])
}

/// `^(?:ja|ko|zh)(?:[-_]|$)` style language-tag prefix test.
fn has_lang_prefix(lang: &str, prefixes: &[&str]) -> bool {
    let lower = lang.to_lowercase();
    prefixes.iter().any(|prefix| {
        lower
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-') || rest.starts_with('_'))
    })
}
