//! Reverse glyph-to-Unicode mapping via OpenType `cmap` tables.
//!
//! When metafiles supply raw glyph indices (e.g. `ETO_GLYPH_INDEX` in EMF or
//! `EmfPlusDrawDriverString` without `CmapLookup`), the glyph indices must be
//! reverse-mapped to Unicode code points so that text can be reshaped, searched,
//! and exported as selectable SVG text.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::text::shape;

type CmapCache = HashMap<(String, bool, bool), Arc<HashMap<u16, char>>>;

static CMAP_CACHE: OnceLock<Mutex<CmapCache>> = OnceLock::new();

fn get_cache() -> &'static Mutex<CmapCache> {
    CMAP_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reverse-maps an array of glyph IDs to a Unicode string using the face's cmap.
///
/// If a glyph ID has no reverse mapping, a fallback character (or `'\u{FFFD}'`)
/// is emitted, guaranteeing that the run retains its character count and advance slots.
pub fn reverse_map_glyphs(
    family: &str,
    bold: bool,
    italic: bool,
    glyph_ids: &[u16],
    fallback_chars: Option<&[char]>,
) -> (String, bool) {
    let map = reverse_cmap_for_face(family, bold, italic);
    let mut mapped_any = false;
    let mut s = String::with_capacity(glyph_ids.len());

    for (index, &gid) in glyph_ids.iter().enumerate() {
        if let Some(&ch) = map.get(&gid) {
            s.push(ch);
            mapped_any = true;
        } else if let Some(fb) = fallback_chars.and_then(|f| f.get(index)).copied() {
            s.push(fb);
        } else {
            s.push('\u{FFFD}');
        }
    }

    (s, mapped_any)
}

/// Retrieves or builds the reverse `GlyphId -> char` map for a given font face.
pub fn reverse_cmap_for_face(family: &str, bold: bool, italic: bool) -> Arc<HashMap<u16, char>> {
    let key = (family.to_string(), bold, italic);
    {
        let cache = get_cache().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(map) = cache.get(&key) {
            return Arc::clone(map);
        }
    }

    let map = shape::with_face_data(family, bold, italic, |data, index| {
        let mut map = HashMap::new();
        let Ok(face) = rustybuzz::ttf_parser::Face::parse(data, index) else {
            return map;
        };
        if let Some(cmap) = face.tables().cmap {
            for subtable in cmap.subtables {
                if subtable.is_unicode() {
                    subtable.codepoints(|cp| {
                        if let Some(ch) = char::from_u32(cp) {
                            if let Some(gid) = subtable.glyph_index(cp) {
                                map.entry(gid.0).or_insert(ch);
                            }
                        }
                    });
                }
            }
        }
        map
    })
    .unwrap_or_default();

    let arc = Arc::new(map);
    let mut cache = get_cache().lock().unwrap_or_else(|p| p.into_inner());
    cache.insert(key, Arc::clone(&arc));
    arc
}
