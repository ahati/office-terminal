//! Persistent reference thumbnails for the external comparison.
//!
//! The pinned references are 1600 px PNGs, but the metric only compares at
//! [`COMPARE_WIDTH`](super::metrics::COMPARE_WIDTH). Decoding, alpha-flattening
//! and box-downscaling every full-resolution reference on every run is wasted
//! work, so the first comparison stores a flattened thumbnail next to the
//! reference cache (under the git-ignored `target/external`). Later runs decode
//! only the thumbnail.
//!
//! The cache key includes the format version, compare width, deck/slide name,
//! and the source file's length plus mtime. Replacing a reference (for example
//! `run.py --force-render`) therefore regenerates just that thumbnail; changing
//! [`COMPARE_WIDTH`](super::metrics::COMPARE_WIDTH) starts a separate width
//! directory. Bump [`THUMBNAIL_VERSION`] if the flatten/downscale path changes.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::metrics::{load_reference, resize_box, scaled_height, RgbImage};

/// Bump when the thumbnail pixel format or downscale path changes.
const THUMBNAIL_VERSION: u32 = 1;

/// Lazily populated cache of flattened, comparison-width reference images.
pub struct ThumbnailCache {
    dir: PathBuf,
    width: u32,
}

impl ThumbnailCache {
    /// Thumbnails live under `thumbnails/<commit>/` next to the commit's
    /// golden deck directories, i.e. inside the ignored `target/external`
    /// cache by default.
    pub fn new(goldens_dir: &Path, commit: &str, width: u32) -> Self {
        let root = goldens_dir.parent().unwrap_or(goldens_dir);
        Self {
            dir: root
                .join("thumbnails")
                .join(commit)
                .join(format!("v{THUMBNAIL_VERSION}-w{width}")),
            width,
        }
    }

    /// Return the flattened reference at the comparison width, generating and
    /// persisting it on first use. A corrupt cached file is regenerated from
    /// the pinned PNG.
    pub fn load(&self, source: &Path) -> Result<RgbImage, String> {
        let entry = self.entry_path(source)?;
        if let Ok(image) = load_rgb(&entry) {
            return Ok(image);
        }

        let full = load_reference(source)?;
        let height = scaled_height(self.width, full.width, full.height);
        let thumbnail = resize_box(&full, self.width, height);
        // A read-only cache is not fatal: the comparison still runs, it just
        // pays the full-resolution decode again next time.
        let _ = save_rgb(&thumbnail, &entry);
        Ok(thumbnail)
    }

    /// Content-addressed by source metadata so a regenerated reference never
    /// reuses a stale thumbnail.
    fn entry_path(&self, source: &Path) -> Result<PathBuf, String> {
        let metadata = fs::metadata(source).map_err(|error| format!("failed to stat {}: {error}", source.display()))?;
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());
        let deck = source
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("deck");
        let stem = source
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("{} has no file stem", source.display()))?;
        Ok(self
            .dir
            .join(deck)
            .join(format!("{stem}.{}-{mtime}.png", metadata.len())))
    }
}

fn load_rgb(path: &Path) -> Result<RgbImage, String> {
    let image = image::open(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?
        .to_rgb8();
    Ok(RgbImage {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    })
}

fn save_rgb(image: &RgbImage, path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

    // Write beside the target so the rename is atomic within one filesystem.
    let temporary = path.with_extension(format!("{}.tmp.png", std::process::id()));
    image::save_buffer(
        &temporary,
        &image.pixels,
        image.width,
        image.height,
        image::ColorType::Rgb8,
    )
    .map_err(|error| format!("failed to write {}: {error}", temporary.display()))?;
    // Unix rename replaces atomically; Windows needs the old entry gone first.
    let _ = fs::remove_file(path);
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("failed to publish {}: {error}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_and_reuses_a_thumbnail() {
        let root = tempfile::tempdir().expect("temp dir");
        let goldens = root.path().join("goldens").join("commit");
        let deck = goldens.join("deck");
        fs::create_dir_all(&deck).expect("deck dir");
        let source = deck.join("slide_001.png");
        image::save_buffer(&source, &[200u8; 16 * 9 * 3], 16, 9, image::ColorType::Rgb8).expect("write source");

        let cache = ThumbnailCache::new(&goldens, "commit", 8);
        let first = cache.load(&source).expect("first load");
        assert_eq!((first.width, first.height), (8, 5));
        let entry = cache.entry_path(&source).expect("entry path");
        assert!(entry.is_file(), "thumbnail should be persisted");

        let second = cache.load(&source).expect("second load");
        assert_eq!(second.pixels, first.pixels);
    }
}
