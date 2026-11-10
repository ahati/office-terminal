use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};
use crate::error::AssetError;

pub enum SizeHint {
    Native,
    AtMost { width: u32, height: u32 },
}

#[derive(Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // Straight RGBA8
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AssetStoreStats {
    pub cached_entries: usize,
    pub cached_bytes: usize,
}

pub trait AssetStore: Send + Sync {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError>;
    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        let _ = rel_id;
        Err(AssetError::NotFound(rel_id.to_string()))
    }
    fn stats(&self) -> AssetStoreStats;
}

struct CacheEntry {
    image: DecodedImage,
    access_counter: u64,
}

/// In-memory & spooled archive asset store implementation with LRU cache
pub struct DefaultAssetStore {
    archive_bytes: Arc<[u8]>,
    _spooled_temp_file: Option<tempfile::NamedTempFile>,
    cache: Mutex<HashMap<String, CacheEntry>>,
    access_clock: Mutex<u64>,
    max_cache_bytes: usize,
}

impl DefaultAssetStore {
    pub fn new(bytes: Arc<[u8]>, temp_file: Option<tempfile::NamedTempFile>) -> Self {
        Self {
            archive_bytes: bytes,
            _spooled_temp_file: temp_file,
            cache: Mutex::new(HashMap::new()),
            access_clock: Mutex::new(0),
            max_cache_bytes: 64 * 1024 * 1024, // 64 MB LRU cache budget
        }
    }

    pub fn with_cache_budget(
        bytes: Arc<[u8]>,
        temp_file: Option<tempfile::NamedTempFile>,
        max_bytes: usize,
    ) -> Self {
        Self {
            archive_bytes: bytes,
            _spooled_temp_file: temp_file,
            cache: Mutex::new(HashMap::new()),
            access_clock: Mutex::new(0),
            max_cache_bytes: max_bytes,
        }
    }

    fn read_zip_entry(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        let cursor = Cursor::new(&self.archive_bytes[..]);
        let mut zip = zip::ZipArchive::new(cursor)
            .map_err(|e| AssetError::DecodeFailed(e.to_string()))?;

        // Normalize paths: strip leading slashes or resolve common prefixes
        let clean_path = rel_id.trim_start_matches('/');
        let candidates = [
            clean_path.to_string(),
            format!("ppt/{}", clean_path),
            format!("ppt/media/{}", clean_path),
            clean_path.trim_start_matches("../").to_string(),
            format!("ppt/{}", clean_path.trim_start_matches("../")),
        ];

        for path in &candidates {
            if let Ok(mut entry) = zip.by_name(path) {
                let mut data = Vec::with_capacity(entry.size() as usize);
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| AssetError::DecodeFailed(e.to_string()))?;
                return Ok(data);
            }
        }

        Err(AssetError::NotFound(rel_id.to_string()))
    }
}

impl AssetStore for DefaultAssetStore {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let cache_key = match hint {
            SizeHint::Native => format!("native:{}", rel_id),
            SizeHint::AtMost { width, height } => format!("at_most_{}x{}:{}", width, height, rel_id),
        };

        // Check cache
        {
            let mut cache = self.cache.lock().unwrap();
            let mut clock = self.access_clock.lock().unwrap();
            *clock += 1;
            if let Some(entry) = cache.get_mut(&cache_key) {
                entry.access_counter = *clock;
                return Ok(entry.image.clone());
            }
        }

        // Read raw entry from archive
        let data = self.read_zip_entry(rel_id)?;

        // Try decoding as SVG if name ends with .svg or starts with SVG header
        let decoded = if rel_id.ends_with(".svg") || data.starts_with(b"<?xml") || data.starts_with(b"<svg") {
            decode_svg(&data)?
        } else {
            decode_raster(&data, &hint)?
        };

        // Insert into LRU cache
        {
            let mut cache = self.cache.lock().unwrap();
            let mut clock = self.access_clock.lock().unwrap();
            *clock += 1;

            // Evict if cache exceeds max_cache_bytes
            let current_bytes: usize = cache.values().map(|e| e.image.pixels.len()).sum();
            if current_bytes + decoded.pixels.len() > self.max_cache_bytes && !cache.is_empty() {
                // Find oldest entry
                let oldest_key = cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.access_counter)
                    .map(|(k, _)| k.clone());
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
        self.read_zip_entry(rel_id)
    }

    fn stats(&self) -> AssetStoreStats {
        let cache = self.cache.lock().unwrap();
        let cached_entries = cache.len();
        let cached_bytes = cache.values().map(|e| e.image.pixels.len()).sum();
        AssetStoreStats {
            cached_entries,
            cached_bytes,
        }
    }
}

fn decode_raster(data: &[u8], hint: &SizeHint) -> Result<DecodedImage, AssetError> {
    let img = image::load_from_memory(data)
        .map_err(|e| AssetError::DecodeFailed(e.to_string()))?;

    let (orig_w, orig_h) = (img.width(), img.height());
    let resized = match hint {
        SizeHint::AtMost { width, height } if orig_w > *width || orig_h > *height => {
            img.resize(*width, *height, image::imageops::FilterType::Triangle)
        }
        _ => img,
    };

    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();

    Ok(DecodedImage {
        width,
        height,
        pixels: Arc::from(rgba.into_raw().into_boxed_slice()),
    })
}

fn decode_svg(data: &[u8]) -> Result<DecodedImage, AssetError> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &opt)
        .map_err(|e| AssetError::DecodeFailed(format!("SVG parse error: {}", e)))?;

    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| AssetError::DecodeFailed("Failed to allocate SVG pixmap".to_string()))?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    // tiny-skia stores premultiplied RGBA8; convert to straight RGBA8
    let mut pixels = pixmap.take();
    #[allow(clippy::chunks_exact_to_as_chunks)]
    for chunk in pixels.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a > 0 && a < 255 {
            let r = (chunk[0] as u32 * 255 + a / 2) / a;
            let g = (chunk[1] as u32 * 255 + a / 2) / a;
            let b = (chunk[2] as u32 * 255 + a / 2) / a;
            chunk[0] = r.min(255) as u8;
            chunk[1] = g.min(255) as u8;
            chunk[2] = b.min(255) as u8;
        }
    }

    Ok(DecodedImage {
        width: size.width(),
        height: size.height(),
        pixels: Arc::from(pixels.into_boxed_slice()),
    })
}
