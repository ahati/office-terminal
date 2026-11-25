//! PowerPoint's pinned face-metrics catalogue (`powerpoint-line-metrics.ts`).
//!
//! The reference renderer does not ask the host for a line box: PowerPoint's
//! #1610 model is a static table of the faces Office itself ships, plus the
//! macOS Supplemental copies it embeds. Its test controls pin numbers that no
//! installed-face probe reproduces (Arial is 1854 / 2288, not the host `hhea`
//! split), so layout must read the same table.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts:39-49` `OBSERVED_SUBSTITUTES`
//! - `:51-141` `powerPointAscentShare` + the bounded share LRU
//!   (`SHARE_CACHE_LIMIT`)
//! - `:96-141` `chosenProfiles` / `resolveShare` (prefer the
//!   `macos-supplemental` copy, `typoMetrics` when `USE_TYPO_METRICS`)
//! - `:143-221` the glyph box, the Excel box and the decoration geometry
//! - `:223-279` the interned `PowerPointFaceMetrics` + its bounded LRU
//! - `packages/core/src/fonts/reference-font-metrics-data.json` — the data,
//!   copied verbatim to `assets/fonts/office-face-metrics.json`
//! - `packages/core/src/fonts/office-auto-line.ts:56-120`
//!   `excelDrawingMlLineRatios` and the 1.3× Far-East Windows box
//! - `packages/pptx/src/east-asian-default.ts:136-150`
//!   `isEastAsianFace` / `isSerifLatinFace`
//!
//! `None` from the public lookups means "uncatalogued face": the caller keeps
//! the ordinary line model, which is the reference's own fallback
//! (`metricOk = false`, `renderer.ts:225`).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// `SHARE_CACHE_LIMIT` (`powerpoint-line-metrics.ts:56`): the number of
/// distinct face/weight/style keys both caches keep.
pub const SHARE_CACHE_LIMIT: usize = 256;

/// `OFFICE_FAR_EAST_SINGLE_LINE_FACTOR` (`office-auto-line.ts:15`).
const FAR_EAST_SINGLE_LINE_FACTOR: f64 = 1.3;

/// `OBSERVED_SUBSTITUTES` (`powerpoint-line-metrics.ts:39-43`). Only the two
/// pinned mappings; every other system-only face stays uncatalogued.
const OBSERVED_SUBSTITUTES: &[(&str, &str)] = &[
    ("palatino", "Palatino Linotype"),
    ("helvetica", "Arial"),
    ("helvetica neue", "Arial"),
];

/// A face's natural line box as em ratios (`ExcelLineBox`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExcelLineBox {
    pub ascent: f32,
    pub descent: f32,
}

/// The face's own underline / strikethrough geometry (`FontDecorationGeometry`),
/// as em ratios; the positions are the rule's top edge above the baseline
/// (negative = below).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationGeometry {
    pub underline_top_em: f32,
    pub underline_thickness_em: f32,
    pub strike_top_em: f32,
    pub strike_thickness_em: f32,
}

/// `PowerPointFaceMetrics` (`powerpoint-line-metrics.ts:223-235`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetricsFull {
    /// #1610 ascent share of the 1.2 × size line box.
    pub share: f32,
    /// The glyph box the share is taken from (`resolveGlyphBox`), em ratios.
    pub glyph: Option<ExcelLineBox>,
    /// #1604 natural box, selected by an explicit `compatLnSpc="0"`.
    pub excel: Option<ExcelLineBox>,
    /// The face's own underline / strikethrough geometry.
    pub decoration: Option<DecorationGeometry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    OfficeMac,
    MacosSystem,
    MacosSupplemental,
    Other,
}

impl Source {
    fn of(value: &str) -> Self {
        match value {
            "office-mac" => Source::OfficeMac,
            "macos-system" => Source::MacosSystem,
            "macos-supplemental" => Source::MacosSupplemental,
            _ => Source::Other,
        }
    }
}

/// One catalogued face cut (`ReferenceFontMetricProfile`).
struct Profile {
    source: Source,
    weight: u16,
    italic: bool,
    units_per_em: f64,
    /// `[ascender, descender, lineGap]` in design units (descender negative).
    hhea: [f64; 3],
    /// OS/2 `[usWinAscent, usWinDescent]`; `None` when the face has no OS/2.
    win: Option<[f64; 2]>,
    /// OS/2 `[sTypoAscender, sTypoDescender, sTypoLineGap]`, present only when
    /// `fsSelection` sets `USE_TYPO_METRICS`.
    typo_metrics: Option<[f64; 3]>,
    /// `None` = the source did not record the field (`farEastCodePage` null).
    far_east_code_page: Option<bool>,
    /// PANOSE `[bFamilyType, bSerifStyle]`.
    panose: Option<[i64; 2]>,
    /// post/OS/2 `[underlinePosition, underlineThickness, strikeoutPosition,
    /// strikeoutSize]`.
    decoration: Option<[f64; 4]>,
}

struct Catalogue {
    profiles: Vec<Profile>,
    /// Normalized family name / alias -> profile indices, in file order.
    alias_index: HashMap<String, Vec<usize>>,
}

static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();

fn catalogue() -> &'static Catalogue {
    CATALOGUE.get_or_init(parse_catalogue)
}

#[derive(serde::Deserialize)]
struct RawCatalogue {
    profiles: Vec<RawProfile>,
}

#[derive(serde::Deserialize)]
struct RawProfile {
    source: String,
    aliases: Vec<String>,
    weight: u16,
    style: String,
    #[serde(rename = "unitsPerEm")]
    units_per_em: f64,
    hhea: [f64; 3],
    #[serde(default)]
    win: Option<[f64; 2]>,
    #[serde(rename = "typoMetrics", default)]
    typo_metrics: Option<[f64; 3]>,
    #[serde(rename = "farEastCodePage", default)]
    far_east_code_page: Option<bool>,
    #[serde(default)]
    panose: Option<[i64; 2]>,
    #[serde(default)]
    decoration: Option<[f64; 4]>,
}

/// Parse the pinned data once. A corrupt asset degrades to an empty catalogue
/// (every face then takes the ordinary line model) rather than panicking.
fn parse_catalogue() -> Catalogue {
    let raw: RawCatalogue = match serde_json::from_str(include_str!("../../assets/fonts/office-face-metrics.json")) {
        Ok(raw) => raw,
        Err(_) => {
            return Catalogue {
                profiles: Vec::new(),
                alias_index: HashMap::new(),
            }
        }
    };
    let mut profiles = Vec::with_capacity(raw.profiles.len());
    let mut alias_index: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, profile) in raw.profiles.into_iter().enumerate() {
        for alias in &profile.aliases {
            let key = normalize_family_name(alias);
            let bucket = alias_index.entry(key).or_default();
            if !bucket.contains(&index) {
                bucket.push(index);
            }
        }
        profiles.push(Profile {
            source: Source::of(&profile.source),
            weight: profile.weight,
            italic: profile.style == "italic",
            units_per_em: profile.units_per_em,
            hhea: profile.hhea,
            win: profile.win,
            typo_metrics: profile.typo_metrics,
            far_east_code_page: profile.far_east_code_page,
            panose: profile.panose,
            decoration: profile.decoration,
        });
    }
    Catalogue { profiles, alias_index }
}

/// `normalizeFamilyName` (`reference-font-metrics.ts:95-101`): trim, collapse
/// whitespace, case-fold. The reference also applies NFKC; the catalogue's
/// aliases are ASCII apart from a few CJK names, so the crate's plain
/// `to_lowercase` is used and documented here.
fn normalize_family_name(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for word in value.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

/// `findReferenceFontMetrics` with the option keys the catalogue uses.
fn find(name: &str, weight: Option<u16>, italic: Option<bool>, source: Option<Source>) -> Vec<usize> {
    let query = normalize_family_name(name);
    if query.is_empty() {
        return Vec::new();
    }
    let cat = catalogue();
    match cat.alias_index.get(&query) {
        None => Vec::new(),
        Some(bucket) => bucket
            .iter()
            .copied()
            .filter(|&index| {
                let profile = &cat.profiles[index];
                weight.is_none_or(|value| profile.weight == value)
                    && italic.is_none_or(|value| profile.italic == value)
                    && source.is_none_or(|value| profile.source == value)
            })
            .collect(),
    }
}

fn substitute(name: &str) -> String {
    let trimmed = name.trim();
    let lower = trimmed.to_lowercase();
    OBSERVED_SUBSTITUTES
        .iter()
        .find(|(from, _)| *from == lower)
        .map_or_else(|| trimmed.to_string(), |(_, to)| (*to).to_string())
}

/// `chosenProfiles` (`powerpoint-line-metrics.ts:96-118`): the copies
/// PowerPoint lays the face out with. Supplemental first (the embedded copy),
/// then Office-bundled; a face only macOS itself provides is never chosen.
fn chosen_profiles(family: &str, bold: bool, italic: bool) -> Vec<usize> {
    let trimmed = family.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let name = substitute(trimmed);
    let style = Some(italic);
    let mut profiles = find(&name, Some(if bold { 700 } else { 400 }), style, None);
    if profiles.is_empty() {
        // A face name that is itself one cut of a family ("Calibri Light" is
        // weight 300) has no 400/700 profile; use that cut when the name
        // selects a single weight (#1630).
        let named = find(&name, None, style, None);
        if !named.is_empty() {
            let cat = catalogue();
            let weight = cat.profiles[named[0]].weight;
            if named.iter().all(|&index| cat.profiles[index].weight == weight) {
                profiles = named;
            }
        }
    }
    let supplemental: Vec<usize> = profiles
        .iter()
        .copied()
        .filter(|&index| catalogue().profiles[index].source == Source::MacosSupplemental)
        .collect();
    if !supplemental.is_empty() {
        return supplemental;
    }
    profiles
        .into_iter()
        .filter(|&index| catalogue().profiles[index].source == Source::OfficeMac)
        .collect()
}

/// The #1610 share of one profile, or `None` when its tables are unusable.
fn profile_share(profile: &Profile) -> Option<f64> {
    if let Some([ascender, descender, line_gap]) = profile.typo_metrics {
        let above = ascender + line_gap.max(0.0);
        return Some(above / (above - descender));
    }
    let [ascent, descent] = profile.win?;
    Some(ascent / (ascent + descent))
}

fn valid_share(share: f64) -> Option<f64> {
    (share.is_finite() && share > 0.0 && share < 1.0).then_some(share)
}

/// The reference's `value > 0` test: NaN and non-positive values are rejected
/// (`Number.isFinite` + the range check in `resolveShare` /
/// `excelDrawingMlLineRatios`).
fn is_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// The reference's `value >= 0` test, NaN rejected.
fn is_non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

/// `resolveShare` (`powerpoint-line-metrics.ts:120-141`): every chosen copy
/// must agree.
fn resolve_share(family: &str, bold: bool, italic: bool) -> Option<f32> {
    let chosen = chosen_profiles(family, bold, italic);
    if chosen.is_empty() {
        return None;
    }
    let cat = catalogue();
    let mut share: Option<f64> = None;
    for index in chosen {
        let next = valid_share(profile_share(&cat.profiles[index])?)?;
        if share.is_some_and(|known| (known - next).abs() > 1e-12) {
            return None;
        }
        share = Some(next);
    }
    share.map(|value| value as f32)
}

/// `resolveGlyphBox` (`powerpoint-line-metrics.ts:147-170`): the usWin (or typo)
/// extent in em ratios.
fn resolve_glyph_box(family: &str, bold: bool, italic: bool) -> Option<ExcelLineBox> {
    let chosen = chosen_profiles(family, bold, italic);
    if chosen.is_empty() {
        return None;
    }
    let cat = catalogue();
    let mut box_: Option<ExcelLineBox> = None;
    for index in chosen {
        let profile = &cat.profiles[index];
        if !is_positive(profile.units_per_em) {
            return None;
        }
        let next = glyph_box_of(profile)?;
        let share = f64::from(next.ascent) / f64::from(next.ascent + next.descent);
        valid_share(share)?;
        if box_.is_some_and(|known| !same_box(known, next)) {
            return None;
        }
        box_ = Some(next);
    }
    box_
}

fn glyph_box_of(profile: &Profile) -> Option<ExcelLineBox> {
    let em = profile.units_per_em as f32;
    if let Some([ascender, descender, line_gap]) = profile.typo_metrics {
        return Some(ExcelLineBox {
            ascent: (ascender + line_gap.max(0.0)) as f32 / em,
            descent: -descender as f32 / em,
        });
    }
    let [ascent, descent] = profile.win?;
    Some(ExcelLineBox {
        ascent: ascent as f32 / em,
        descent: descent as f32 / em,
    })
}

fn same_box(left: ExcelLineBox, right: ExcelLineBox) -> bool {
    (left.ascent - right.ascent).abs() <= 1e-6 && (left.descent - right.descent).abs() <= 1e-6
}

/// The Far-East 1.3× Windows box (`office-auto-line.ts:17-26`).
fn far_east_windows_line_box(ascent: f64, descent: f64) -> Option<(f64, f64)> {
    if !is_non_negative(ascent) || !is_non_negative(descent) || !is_positive(ascent + descent) {
        return None;
    }
    let half = (FAR_EAST_SINGLE_LINE_FACTOR - 1.0) / 2.0 * (ascent + descent);
    Some((ascent + half, descent + half))
}

/// `excelDrawingMlLineRatios` (`office-auto-line.ts:56-120`), returning
/// `[ascent, descent]` em ratios.
fn excel_ratios(profile: &Profile) -> Option<[f64; 2]> {
    let em = profile.units_per_em;
    if !is_positive(em) {
        return None;
    }
    let [hhea_ascent, hhea_descent, hhea_gap] = profile.hhea;
    let [win_ascent, win_descent] = profile.win?;
    let values = [em, hhea_ascent, hhea_descent, hhea_gap, win_ascent, win_descent];
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let far_east = profile.far_east_code_page?;
    let (ascent, descent) = if profile.source == Source::MacosSupplemental {
        if far_east {
            return None;
        }
        (hhea_ascent + hhea_gap.max(0.0), -hhea_descent)
    } else if far_east {
        far_east_windows_line_box(win_ascent, win_descent)?
    } else if let Some([ascender, descender, line_gap]) = profile.typo_metrics {
        (ascender + line_gap.max(0.0), -descender)
    } else {
        let glyph_box = win_ascent + win_descent;
        let external_leading = (hhea_gap - (glyph_box - (hhea_ascent - hhea_descent))).max(0.0);
        (win_ascent + external_leading, win_descent)
    };
    if !is_non_negative(ascent) || !is_non_negative(descent) || !is_positive(ascent + descent) {
        return None;
    }
    Some([ascent / em, descent / em])
}

/// `resolveExcelBox` (`powerpoint-line-metrics.ts:180-200`): the #1604 natural
/// box, used only under an explicit `compatLnSpc="0"`.
fn resolve_excel_box(family: &str, bold: bool, italic: bool) -> Option<ExcelLineBox> {
    let cat = catalogue();
    let mut box_: Option<ExcelLineBox> = None;
    for index in chosen_profiles(family, bold, italic) {
        let ratios = excel_ratios(&cat.profiles[index])?;
        let next = ExcelLineBox {
            ascent: ratios[0] as f32,
            descent: ratios[1] as f32,
        };
        if box_.is_some_and(|known| !same_box(known, next)) {
            return None;
        }
        box_ = Some(next);
    }
    box_
}

/// `resolveDecoration` (`powerpoint-line-metrics.ts:207-221`).
fn resolve_decoration(family: &str, bold: bool, italic: bool) -> Option<DecorationGeometry> {
    let cat = catalogue();
    let mut geometry: Option<DecorationGeometry> = None;
    for index in chosen_profiles(family, bold, italic) {
        let profile = &cat.profiles[index];
        let [underline_position, underline_thickness, strikeout_position, strikeout_size] = profile.decoration?;
        let em = profile.units_per_em;
        if !is_positive(em)
            || !is_positive(underline_thickness)
            || !is_positive(strikeout_size)
            || !is_positive(strikeout_position)
        {
            return None;
        }
        let next = DecorationGeometry {
            underline_top_em: (-underline_position / em) as f32,
            underline_thickness_em: (underline_thickness / em) as f32,
            strike_top_em: (-strikeout_position / em) as f32,
            strike_thickness_em: (strikeout_size / em) as f32,
        };
        if geometry.is_some_and(|known| !same_decoration(known, next)) {
            return None;
        }
        geometry = Some(next);
    }
    geometry
}

fn same_decoration(left: DecorationGeometry, right: DecorationGeometry) -> bool {
    (left.underline_top_em - right.underline_top_em).abs() <= 1e-6
        && (left.underline_thickness_em - right.underline_thickness_em).abs() <= 1e-6
        && (left.strike_top_em - right.strike_top_em).abs() <= 1e-6
        && (left.strike_thickness_em - right.strike_thickness_em).abs() <= 1e-6
}

// ── Bounded LRUs ───────────────────────────────────────────────────────────
//
// The reference keeps two bounded LRUs (`SHARE_CACHE_LIMIT`, least recently
// used evicted first): `powerPointAscentShare`'s share cache and
// `powerPointFaceMetrics`'s interned-face cache. The implementation lives in
// [`super::face_cache`] so this file stays inside the 600-line budget.

use super::face_cache;

/// `powerPointAscentShare` (`powerpoint-line-metrics.ts:58-81`): the #1610
/// line-box share, or `None` for a face outside the catalogue.
pub fn power_point_ascent_share(family: &str, bold: bool, italic: bool) -> Option<f32> {
    let key = cache_key(family, bold, italic);
    if let Some(hit) = face_cache::shares().get(&key).copied() {
        return hit;
    }
    let share = resolve_share(family, bold, italic);
    face_cache::shares().insert(key, share);
    share
}

/// `powerPointFaceMetrics` (`powerpoint-line-metrics.ts:244-270`): the line
/// metrics of a face, or `None` when its share is unresolvable.
pub fn power_point_face_metrics(family: &str, bold: bool, italic: bool) -> Option<Arc<FaceMetricsFull>> {
    let key = cache_key(family, bold, italic);
    if let Some(hit) = face_cache::faces().get(&key).cloned() {
        return hit;
    }
    let metrics = resolve_share(family, bold, italic).map(|share| {
        Arc::new(FaceMetricsFull {
            share,
            glyph: resolve_glyph_box(family, bold, italic),
            excel: resolve_excel_box(family, bold, italic),
            decoration: resolve_decoration(family, bold, italic),
        })
    });
    face_cache::faces().insert(key, metrics.clone());
    metrics
}

/// `` `${family.trim().toLowerCase()}|${bold ? 700 : 400}|${italic ? 'i' : 'n'}` ``
/// (`powerpoint-line-metrics.ts:62`).
fn cache_key(family: &str, bold: bool, italic: bool) -> String {
    format!(
        "{}|{}|{}",
        family.trim().to_lowercase(),
        if bold { 700 } else { 400 },
        if italic { "i" } else { "n" }
    )
}

/// The live entry count of the share cache — the bound
/// `powerpoint-line-metrics.test.ts:195` pins.
pub fn share_cache_len() -> usize {
    face_cache::shares().len()
}

/// The live entry count of the interned face-metrics cache.
pub fn face_cache_len() -> usize {
    face_cache::faces().len()
}

/// `profilesOf` (`east-asian-default.ts:129-133`): the Office-bundled copy of
/// the face when present, else every catalogue copy.
fn profiles_of(face: &str) -> Vec<usize> {
    let office = find(face, None, None, Some(Source::OfficeMac));
    if office.is_empty() {
        find(face, None, None, None)
    } else {
        office
    }
}

/// `isEastAsianFace` (`east-asian-default.ts:136-138`): the face declares an
/// OS/2 Far-East code page (T2's slot tier).
pub fn is_east_asian_face(face: &str) -> bool {
    let cat = catalogue();
    profiles_of(face)
        .into_iter()
        .any(|index| cat.profiles[index].far_east_code_page == Some(true))
}

/// `isSerifLatinFace` (`east-asian-default.ts:141-147`): PANOSE family type 2
/// with a serif style in 2..=10; unknown faces are sans.
pub fn is_serif_latin_face(face: &str) -> bool {
    let cat = catalogue();
    profiles_of(face).into_iter().any(|index| {
        cat.profiles[index]
            .panose
            .is_some_and(|[family_type, serif_style]| family_type == 2 && (2..=10).contains(&serif_style))
    })
}
