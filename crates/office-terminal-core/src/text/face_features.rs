//! OpenType `vert` and `vrt2` feature probing and catalogue lookup.
//!
//! ECMA-376 §20.1.10.83 `eaVert` and stacked WordArt text rely on font-level
//! `vert` and `vrt2` feature coverage. For catalogued Office faces, coverage
//! is resolved from `office-face-metrics.json` (`stackedVerticalGlyphs` and
//! `stackedVrt2OnlyGlyphs`). For uncatalogued faces, coverage is probed
//! dynamically through `rustybuzz` shaping.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::text::shape;

/// Parsed feature profile for one face cut.
#[derive(Clone, Debug, Default)]
struct FaceFeatureProfile {
    weight: u16,
    style: String,
    source: String,
    vert_glyphs: Option<String>,
    vrt2_only_glyphs: Option<String>,
}

struct FeatureCatalogue {
    profiles: Vec<FaceFeatureProfile>,
    alias_index: HashMap<String, Vec<usize>>,
}

static FEATURE_CATALOGUE: OnceLock<FeatureCatalogue> = OnceLock::new();

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
    #[serde(rename = "stackedVerticalGlyphs", default)]
    vert_glyphs: Option<String>,
    #[serde(rename = "stackedVrt2OnlyGlyphs", default)]
    vrt2_only_glyphs: Option<String>,
}

fn normalize_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn parse_catalogue() -> FeatureCatalogue {
    let raw: RawCatalogue = match serde_json::from_str(include_str!("../../assets/fonts/office-face-metrics.json")) {
        Ok(raw) => raw,
        Err(_) => {
            return FeatureCatalogue {
                profiles: Vec::new(),
                alias_index: HashMap::new(),
            }
        }
    };
    let mut profiles = Vec::with_capacity(raw.profiles.len());
    let mut alias_index: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, profile) in raw.profiles.into_iter().enumerate() {
        for alias in &profile.aliases {
            let key = normalize_name(alias);
            let bucket = alias_index.entry(key).or_default();
            if !bucket.contains(&index) {
                bucket.push(index);
            }
        }
        profiles.push(FaceFeatureProfile {
            weight: profile.weight,
            style: profile.style,
            source: profile.source,
            vert_glyphs: profile.vert_glyphs,
            vrt2_only_glyphs: profile.vrt2_only_glyphs,
        });
    }
    FeatureCatalogue { profiles, alias_index }
}

fn catalogue() -> &'static FeatureCatalogue {
    FEATURE_CATALOGUE.get_or_init(parse_catalogue)
}

fn find_profile(family: &str, bold: bool, italic: bool) -> Option<&'static FaceFeatureProfile> {
    let cat = catalogue();
    let key = normalize_name(family);
    let indices = cat.alias_index.get(&key)?;
    let target_weight = if bold { 700 } else { 400 };
    let target_style = if italic { "italic" } else { "normal" };

    // Exact match: weight + style.
    let exact = indices.iter().filter_map(|&i| {
        let p = &cat.profiles[i];
        if p.weight == target_weight && p.style == target_style {
            Some(p)
        } else {
            None
        }
    });
    if let Some(p) = pick_source(exact) {
        return Some(p);
    }

    // Relaxed style: match weight.
    let weight_match = indices.iter().filter_map(|&i| {
        let p = &cat.profiles[i];
        if p.weight == target_weight {
            Some(p)
        } else {
            None
        }
    });
    if let Some(p) = pick_source(weight_match) {
        return Some(p);
    }

    // Any cut for named cuts / aliases.
    let any = indices.iter().map(|&i| &cat.profiles[i]);
    pick_source(any)
}

fn pick_source<'a>(profiles: impl Iterator<Item = &'a FaceFeatureProfile>) -> Option<&'a FaceFeatureProfile> {
    let list: Vec<&'a FaceFeatureProfile> = profiles.collect();
    if list.is_empty() {
        return None;
    }
    list.iter()
        .find(|p| p.source == "macos-supplemental")
        .or_else(|| list.iter().find(|p| p.source == "office-mac"))
        .copied()
        .or_else(|| list.first().copied())
}

type FeatureCacheKey = (String, bool, bool, &'static str, char);
type FeatureCacheMap = HashMap<FeatureCacheKey, bool>;

thread_local! {
    static FEATURE_CACHE: RefCell<FeatureCacheMap> = RefCell::new(HashMap::new());
}

/// Query whether `family` at the given weight/style has `vert` feature coverage for `ch`.
pub fn has_vert_feature(family: &str, bold: bool, italic: bool, ch: char) -> bool {
    let key = (family.to_string(), bold, italic, "vert", ch);
    if let Some(cached) = FEATURE_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return cached;
    }
    let covered = if let Some(profile) = find_profile(family, bold, italic) {
        profile.vert_glyphs.as_ref().is_some_and(|glyphs| glyphs.contains(ch))
    } else {
        probe_shaping_feature(family, bold, italic, "vert", ch)
    };
    FEATURE_CACHE.with(|c| {
        c.borrow_mut().insert(key, covered);
    });
    covered
}

/// Query whether `family` at the given weight/style has `vrt2` feature coverage for `ch`.
pub fn has_vrt2_feature(family: &str, bold: bool, italic: bool, ch: char) -> bool {
    let key = (family.to_string(), bold, italic, "vrt2", ch);
    if let Some(cached) = FEATURE_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return cached;
    }
    let covered = if let Some(profile) = find_profile(family, bold, italic) {
        profile
            .vrt2_only_glyphs
            .as_ref()
            .is_some_and(|glyphs| glyphs.contains(ch))
    } else {
        probe_shaping_feature(family, bold, italic, "vrt2", ch)
    };
    FEATURE_CACHE.with(|c| {
        c.borrow_mut().insert(key, covered);
    });
    covered
}

/// Probe a non-catalogued font on the host through `rustybuzz`.
fn probe_shaping_feature(family: &str, bold: bool, italic: bool, feat: &str, ch: char) -> bool {
    use std::str::FromStr;
    shape::with_face_data(family, bold, italic, |data, index| {
        let Some(face) = rustybuzz::Face::from_slice(data, index) else {
            return false;
        };
        let mut b1 = rustybuzz::UnicodeBuffer::new();
        b1.push_str(&ch.to_string());
        let g1 = rustybuzz::shape(&face, &[], b1);
        let id1 = g1.glyph_infos().first().map(|g| g.glyph_id).unwrap_or(0);
        if id1 == 0 {
            return false;
        }
        let Ok(feature) = rustybuzz::Feature::from_str(feat) else {
            return false;
        };
        let mut b2 = rustybuzz::UnicodeBuffer::new();
        b2.push_str(&ch.to_string());
        let g2 = rustybuzz::shape(&face, &[feature], b2);
        let id2 = g2.glyph_infos().first().map(|g| g.glyph_id).unwrap_or(0);
        id2 != 0 && id2 != id1
    })
    .unwrap_or(false)
}
