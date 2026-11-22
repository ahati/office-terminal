//! Resource bounds for untrusted DOCX packages (PRD 15 §8.1, issue #40 §7.14).
//!
//! Every limit is a guard against hostile input, never a behavior knob for
//! ordinary documents: the defaults sit far above anything a real Word file
//! produces, and crossing one surfaces as a typed
//! [`DocxError::ResourceLimit`](crate::DocxError) rather than an OOM or a hang.

use std::time::Duration;

/// Resource limits applied while opening a DOCX document.
#[derive(Clone, Copy, Debug)]
pub struct DocxLimits {
    /// Maximum OPC ZIP entries.
    pub max_entries: usize,
    /// Maximum decompressed bytes for a single package part.
    pub max_part_uncompressed_bytes: u64,
    /// Maximum total decompressed bytes across the package (ZIP-bomb guard).
    pub max_total_uncompressed_bytes: u64,
    /// Maximum paginated page count.
    pub max_pages: usize,
    /// Search-corpus line cap per page.
    pub max_corpus_lines_per_page: usize,
    /// Search-corpus text-byte cap per page.
    pub max_corpus_bytes_per_page: usize,
    /// Decoded-image LRU byte budget in the asset store.
    pub image_cache_bytes: usize,
    /// Compiled `VectorSlide` cache entry cap.
    pub ir_cache_entries: usize,
    /// Compiled `VectorSlide` cache byte budget (rough estimate).
    pub ir_cache_bytes: usize,
    /// Soft layout deadline. Crossing it records a diagnostic warning but does
    /// not abort a running pagination (issue #40 §7.14).
    pub layout_deadline: Duration,
}

impl Default for DocxLimits {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            max_part_uncompressed_bytes: 64 * 1024 * 1024,
            max_total_uncompressed_bytes: 512 * 1024 * 1024,
            max_pages: 10_000,
            max_corpus_lines_per_page: 10_000,
            max_corpus_bytes_per_page: 8 * 1024 * 1024,
            image_cache_bytes: 64 * 1024 * 1024,
            ir_cache_entries: 16,
            ir_cache_bytes: 32 * 1024 * 1024,
            layout_deadline: Duration::from_secs(30),
        }
    }
}

impl DocxLimits {
    /// The package-read limits handed to the layout engine.
    #[cfg(feature = "rdocx-backend")]
    pub(crate) fn package_limits(&self) -> oxml_opc::PackageReadLimits {
        oxml_opc::PackageReadLimits {
            max_entries: self.max_entries,
            max_part_uncompressed_bytes: self.max_part_uncompressed_bytes,
            max_total_uncompressed_bytes: self.max_total_uncompressed_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_are_generous_but_bounded() {
        let limits = DocxLimits::default();
        assert_eq!(limits.max_entries, 10_000);
        assert_eq!(limits.max_pages, 10_000);
        assert_eq!(limits.max_part_uncompressed_bytes, 64 * 1024 * 1024);
        assert_eq!(limits.max_total_uncompressed_bytes, 512 * 1024 * 1024);
        assert_eq!(limits.layout_deadline, Duration::from_secs(30));
    }

    #[cfg(feature = "rdocx-backend")]
    #[test]
    fn package_limits_forward_the_bounds_verbatim() {
        let limits = DocxLimits {
            max_entries: 7,
            max_part_uncompressed_bytes: 9,
            max_total_uncompressed_bytes: 11,
            ..DocxLimits::default()
        };
        let package = limits.package_limits();
        assert_eq!(package.max_entries, 7);
        assert_eq!(package.max_part_uncompressed_bytes, 9);
        assert_eq!(package.max_total_uncompressed_bytes, 11);
    }
}
