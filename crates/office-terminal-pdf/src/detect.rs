use crate::error::PdfError;

/// Classification decision between interactive slide deck and document reader (PRD 14a §2).
#[derive(Clone, Debug, PartialEq)]
pub enum ReaderModeDecision {
    Slides {
        landscape_ratio: f32,
        median_aspect: f32,
        inlier_ratio: f32,
    },
    Document {
        portrait_ratio: f32,
        paper_size: Option<&'static str>,
    },
}

impl ReaderModeDecision {
    /// Returns true if the classification determined Reader/Document mode.
    pub fn is_reader(&self) -> bool {
        matches!(self, Self::Document { .. })
    }
}

/// Detects if bytes represent a PDF container by inspecting magic bytes in first 1 KiB.
pub fn is_pdf(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(1024);
    bytes[..check_len].windows(5).any(|w| w == b"%PDF-")
}

/// Extracts PDF version string from header, e.g. "PDF 1.7".
pub fn parse_version(bytes: &[u8]) -> Option<String> {
    let check_len = bytes.len().min(1024);
    let pos = bytes[..check_len].windows(5).position(|w| w == b"%PDF-")?;
    let rest = &bytes[pos + 5..check_len];
    let end = rest.iter().position(|&b| b == b'\r' || b == b'\n' || b == b' ')?;
    let ver_str = std::str::from_utf8(&rest[..end]).ok()?;
    Some(format!("PDF {}", ver_str.trim()))
}

/// Fast pre-check whether the PDF specifies document encryption.
///
/// Returns true iff the byte pattern `b"/Encrypt"` occurs anywhere in the
/// file. Most trailers place `/Encrypt` near the end, so the last 4 KiB
/// is scanned first with an early exit on match; only if that misses does
/// a first-byte-prefilter scan cover the whole buffer.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    // In unencrypted files, /Encrypt dictionary is absent from trailer.
    // Check for b"/Encrypt" within trailer or xref streams (last 4 KiB or whole document).
    let tail_len = bytes.len().min(4096);
    let tail = &bytes[bytes.len() - tail_len..];
    if contains_pattern(tail, b"/Encrypt") {
        return true;
    }

    // The tail scan already covered the whole buffer.
    if tail_len == bytes.len() {
        return false;
    }

    contains_pattern(bytes, b"/Encrypt")
}

/// Returns true if `needle` occurs in `haystack` (an empty needle never
/// matches).
///
/// Skips to each occurrence of the needle's first byte instead of running
/// a full-width windowed comparison at every offset, keeping the common
/// (pattern-absent) path cheap without extra dependencies.
fn contains_pattern(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }

    let first = needle[0];
    let mut window = haystack;
    while window.len() >= needle.len() {
        // Jump to the next occurrence of the first byte.
        let Some(offset) = window.iter().position(|&b| b == first) else {
            return false;
        };
        window = &window[offset..];
        if window.len() < needle.len() {
            return false;
        }
        if &window[..needle.len()] == needle {
            return true;
        }
        window = &window[1..];
    }

    false
}

/// Classifies a PDF document into Slides vs Reader mode per PRD 14a §2.
pub fn classify_mode(page_sizes_pt: &[(f32, f32)]) -> Result<ReaderModeDecision, PdfError> {
    let n = page_sizes_pt.len();
    if n == 0 {
        return Err(PdfError::EmptyDocument);
    }

    let mut landscape_count = 0usize;
    let mut portrait_count = 0usize;
    let mut landscape_aspects = Vec::new();

    for &(w, h) in page_sizes_pt {
        if w > 1.02 * h {
            landscape_count += 1;
            landscape_aspects.push(w / h);
        } else if h > 1.02 * w {
            portrait_count += 1;
        }
        // Near-square: counts as outlier for both
    }

    let landscape_ratio = landscape_count as f32 / n as f32;
    let portrait_ratio = portrait_count as f32 / n as f32;

    if !landscape_aspects.is_empty() {
        landscape_aspects.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = landscape_aspects.len() / 2;
        let median_aspect = if landscape_aspects.len() % 2 == 0 {
            (landscape_aspects[mid - 1] + landscape_aspects[mid]) / 2.0
        } else {
            landscape_aspects[mid]
        };

        // Aspect inlier: |a_i - m| <= 0.02 * m
        let inliers_count = page_sizes_pt
            .iter()
            .filter(|&&(w, h)| {
                let a = w / h;
                (a - median_aspect).abs() <= 0.02 * median_aspect
            })
            .count();

        let inlier_ratio = inliers_count as f32 / n as f32;

        // Rule D2: landscape >= 90% AND inliers >= 90% AND median aspect >= 1.25
        if landscape_ratio >= 0.90 && inlier_ratio >= 0.90 && median_aspect >= 1.25 {
            return Ok(ReaderModeDecision::Slides {
                landscape_ratio,
                median_aspect,
                inlier_ratio,
            });
        }
    }

    // Rule D3: everything else -> Document
    let paper_size = identify_paper_size(page_sizes_pt);
    Ok(ReaderModeDecision::Document {
        portrait_ratio,
        paper_size,
    })
}

/// Identifies standard paper formats (within 0.5 mm tolerance).
fn identify_paper_size(page_sizes_pt: &[(f32, f32)]) -> Option<&'static str> {
    if page_sizes_pt.is_empty() {
        return None;
    }
    let (w, h) = page_sizes_pt[0];
    let (min_dim, max_dim) = if w < h { (w, h) } else { (h, w) };

    // 1 pt = 25.4 / 72 mm = 0.352778 mm. 0.5 mm ~= 1.417 pt.
    let matches = |target_w_pt: f32, target_h_pt: f32| {
        (min_dim - target_w_pt).abs() <= 1.5 && (max_dim - target_h_pt).abs() <= 1.5
    };

    if matches(595.28, 841.89) {
        Some("A4")
    } else if matches(612.0, 792.0) {
        Some("Letter")
    } else if matches(612.0, 1008.0) {
        Some("Legal")
    } else if matches(419.53, 595.28) {
        Some("A5")
    } else if matches(841.89, 1190.55) {
        Some("A3")
    } else if matches(792.0, 1224.0) {
        Some("Tabloid")
    } else {
        None
    }
}
