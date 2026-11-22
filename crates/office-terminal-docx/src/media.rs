//! Embedded media extraction and the DOCX asset store (issue #40 §7.9).
//!
//! Images arrive inline in the positioned elements (raw `data`, a
//! `content_type` and a stable `MediaId`), so the store is built once at
//! open by walking the pages. No ZIP re-reading and no filesystem writes
//! happen here, and no archive path ever reaches a decoder: keys are
//! content-addressed `docx-media-<id>` strings, so path traversal is
//! structurally impossible.

use std::collections::HashMap;
use std::sync::Mutex;
#[cfg(any(feature = "rdocx-backend", test))]
use std::sync::Arc;

use office_terminal_core::loader::{
    decode_raster, decode_svg, AssetStore, AssetStoreStats, DecodedImage, SizeHint,
};
use office_terminal_core::AssetError;
#[cfg(any(feature = "rdocx-backend", test))]
use oxml_layout::{PageFrame, PositionedElement};
use oxml_layout::MediaId;

use crate::engine::MediaBytes;

/// `rel_id` for one media id: content-addressed, never an archive path.
pub fn media_rel_id(id: MediaId) -> String {
    format!("docx-media-{:016x}", id.0)
}

/// Collect every positioned image's bytes by media id (page walk, groups
/// included). Duplicate references share one entry.
#[cfg(any(feature = "rdocx-backend", test))]
pub(crate) fn collect_page_media(pages: &[Arc<PageFrame>]) -> HashMap<MediaId, MediaBytes> {
    let mut media: HashMap<MediaId, MediaBytes> = HashMap::new();
    for page in pages {
        collect_elements(&page.elements, &mut media);
    }
    media
}

#[cfg(any(feature = "rdocx-backend", test))]
fn collect_elements(elements: &[PositionedElement], media: &mut HashMap<MediaId, MediaBytes>) {
    for element in elements {
        match element {
            PositionedElement::Image {
                data,
                content_type,
                media_id,
                ..
            } => {
                media.entry(*media_id).or_insert_with(|| MediaBytes {
                    data: Arc::from(data.as_slice()),
                    content_type: content_type.clone(),
                });
            }
            PositionedElement::Group(group) => {
                collect_elements(&group.children, media);
            }
            PositionedElement::MarkedContent { children, .. } => {
                collect_elements(children, media);
            }
            _ => {}
        }
    }
}

struct CacheEntry {
    image: DecodedImage,
    access_counter: u64,
}

/// In-memory asset store over the document's extracted media with a bounded
/// decoded-image LRU (issue #40 §7.9).
pub struct DocxAssetStore {
    entries: HashMap<String, MediaBytes>,
    cache: Mutex<HashMap<String, CacheEntry>>,
    access_clock: Mutex<u64>,
    max_cache_bytes: usize,
    raw_bytes_served: Mutex<usize>,
}

impl DocxAssetStore {
    /// Build the store from the engine's media inventory.
    pub fn from_engine(engine: &dyn crate::engine::DocxEngine) -> Self {
        let mut entries = HashMap::new();
        for id in engine.media_ids() {
            if let Some(bytes) = engine.media(id) {
                entries.insert(media_rel_id(id), bytes);
            }
        }
        Self {
            entries,
            cache: Mutex::new(HashMap::new()),
            access_clock: Mutex::new(0),
            max_cache_bytes: 64 * 1024 * 1024,
            raw_bytes_served: Mutex::new(0),
        }
    }

    /// Number of distinct media entries (feeds `ot info`).
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Whether `rel_id` names a registered media entry.
    pub fn contains(&self, rel_id: &str) -> bool {
        self.entries.contains_key(rel_id)
    }

    /// Byte-accurate raw media size (feeds `ot info`).
    pub fn raw_bytes_total(&self) -> usize {
        self.entries.values().map(|entry| entry.data.len()).sum()
    }

    fn decode(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let entry = self
            .entries
            .get(rel_id)
            .ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        if entry.content_type.contains("svg") || entry.data.starts_with(b"<svg")
            || entry.data.starts_with(b"<?xml")
        {
            decode_svg(&entry.data, &hint)
        } else {
            decode_raster(&entry.data, &hint)
        }
    }
}

impl AssetStore for DocxAssetStore {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let cache_key = match hint {
            SizeHint::Native => format!("native:{rel_id}"),
            SizeHint::AtMost { width, height } => format!("at_most_{width}x{height}:{rel_id}"),
        };

        {
            let mut cache = self
                .cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut clock = self
                .access_clock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *clock += 1;
            if let Some(entry) = cache.get_mut(&cache_key) {
                entry.access_counter = *clock;
                return Ok(entry.image.clone());
            }
        }

        let decoded = self.decode(rel_id, hint)?;

        {
            let mut cache = self
                .cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut clock = self
                .access_clock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *clock += 1;

            let current_bytes: usize = cache.values().map(|e| e.image.pixels.len()).sum();
            if current_bytes + decoded.pixels.len() > self.max_cache_bytes && !cache.is_empty() {
                let oldest_key = cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.access_counter)
                    .map(|(key, _)| key.clone());
                if let Some(key) = oldest_key {
                    cache.remove(&key);
                }
            }

            cache.insert(
                cache_key,
                CacheEntry {
                    image: decoded.clone(),
                    access_counter: *clock,
                },
            );
        }

        Ok(decoded)
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        let entry = self
            .entries
            .get(rel_id)
            .ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        *self
            .raw_bytes_served
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        Ok(entry.data.to_vec())
    }

    fn archive_bytes(&self) -> Option<&[u8]> {
        // DOCX media is not a ZIP surface; nothing serves as a whole archive.
        None
    }

    fn stats(&self) -> AssetStoreStats {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        AssetStoreStats {
            cached_entries: cache.len(),
            cached_bytes: cache.values().map(|e| e.image.pixels.len()).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{DocxEngine, MediaBytes, ResolvedFont};
    use oxml_layout::{DocumentMetadata, FontId, OutlineEntry, Rect};

    struct FixtureEngine {
        media: HashMap<MediaId, MediaBytes>,
    }

    impl DocxEngine for FixtureEngine {
        fn page_count(&self) -> usize {
            1
        }
        fn page(&self, index: usize) -> Option<Arc<PageFrame>> {
            if index != 0 {
                return None;
            }
            let png: Vec<u8> = vec![0x89, b'P', b'N', b'G'];
            Some(Arc::new(PageFrame::new(
                1,
                612.0,
                792.0,
                vec![PositionedElement::Image {
                    rect: Rect {
                        x: 10.0,
                        y: 10.0,
                        width: 100.0,
                        height: 50.0,
                    },
                    data: png,
                    content_type: "image/png".to_string(),
                    media_id: MediaId::from_bytes(b"png"),
                }],
            )))
        }
        fn resolve_font(&self, _id: FontId) -> Option<&ResolvedFont> {
            None
        }
        fn media_ids(&self) -> Vec<MediaId> {
            self.media.keys().copied().collect()
        }
        fn media(&self, id: MediaId) -> Option<MediaBytes> {
            self.media.get(&id).cloned()
        }
        fn outlines(&self) -> &[OutlineEntry] {
            &[]
        }
        fn bookmark_page(&self, _name: &str) -> Option<usize> {
            None
        }
        fn diagnostics(&self) -> &[String] {
            &[]
        }
        fn metadata(&self) -> Option<&DocumentMetadata> {
            None
        }
    }

    #[test]
    fn collect_page_media_deduplicates_by_content_address() {
        let png: Vec<u8> = vec![0x89, b'P', b'N', b'G'];
        let id = MediaId::from_bytes(&png);
        let page = PageFrame::new(
            1,
            100.0,
            100.0,
            vec![
                PositionedElement::Image {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 5.0,
                        height: 5.0,
                    },
                    data: png.clone(),
                    content_type: "image/png".to_string(),
                    media_id: id,
                },
                PositionedElement::Image {
                    rect: Rect {
                        x: 50.0,
                        y: 0.0,
                        width: 5.0,
                        height: 5.0,
                    },
                    data: png,
                    content_type: "image/png".to_string(),
                    media_id: id,
                },
            ],
        );
        let media = collect_page_media(&[Arc::new(page)]);
        assert_eq!(media.len(), 1);
    }

    #[test]
    fn store_serves_raw_bytes_by_rel_id_and_reports_not_found() {
        let png: Vec<u8> = vec![0x89, b'P', b'N', b'G'];
        let id = MediaId::from_bytes(&png);
        let mut media = HashMap::new();
        media.insert(
            id,
            MediaBytes {
                data: Arc::from(png.as_slice()),
                content_type: "image/png".to_string(),
            },
        );
        let engine = FixtureEngine { media };
        let store = DocxAssetStore::from_engine(&engine);

        let rel_id = media_rel_id(id);
        let raw = store.get_raw_bytes(&rel_id).expect("raw bytes");
        assert_eq!(raw, png);
        assert!(store.get_raw_bytes("docx-media-deadbeef").is_err());
        assert_eq!(store.entry_count(), 1);
        assert_eq!(store.raw_bytes_total(), 4);
    }

    #[test]
    fn rel_ids_are_content_addressed_hex_keys() {
        assert_eq!(media_rel_id(MediaId(0)), "docx-media-0000000000000000");
        assert_eq!(media_rel_id(MediaId(u64::MAX)), "docx-media-ffffffffffffffff");
        assert!(media_rel_id(MediaId(1)).starts_with("docx-media-"));
    }
}
