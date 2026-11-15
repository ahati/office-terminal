//! Bounded container unpacking for `.emz` / `.wmz` parts (PRD 13 §8.2).
//!
//! gzip-wrapped metafiles are inflated with a hard output cap: a lying ISIZE or
//! a decompression bomb degrades to the placeholder instead of exhausting
//! memory, because the reader stops one byte past the limit.

use std::io::Read;

use flate2::read::GzDecoder;

use super::{classify_path, MetafileError, MetafileFormat, MetafileLimits};

/// gzip magic (`1f 8b`). `.emz`/`.wmz` parts always carry it.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

/// Inflates `bytes` when it is gzip-wrapped, capped at `limit` output bytes.
///
/// The part extension is authoritative for `.emz`/`.wmz`; a gzip stream under
/// any other name is inflated too so mislabelled media still plays.
pub fn unpack(bytes: &[u8], path: &str, limits: &MetafileLimits) -> Result<Vec<u8>, MetafileError> {
    let limit = limits.max_input_bytes;
    // A recognised EMF/WMF header wins over the extension: Office decks carry
    // mislabelled parts, and inflating a plain metafile would fail.
    let already_metafile = super::classify_bytes(bytes).is_some();
    let wrapped = is_gzip(bytes)
        || (!already_metafile
            && matches!(
                classify_path(path),
                Some(MetafileFormat::Emz | MetafileFormat::Wmz)
            ));
    if !wrapped {
        return Ok(bytes.to_vec());
    }
    if bytes.len() > limit {
        return Err(MetafileError::Limit(format!(
            "{} compressed bytes exceeds {limit}",
            bytes.len()
        )));
    }
    let mut decoder = GzDecoder::new(bytes).take(limit as u64 + 1);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|error| MetafileError::Invalid(format!("gzip stream: {error}")))?;
    if out.len() > limit {
        return Err(MetafileError::Limit(format!(
            "gzip stream inflates past {limit} bytes"
        )));
    }
    if out.is_empty() {
        return Err(MetafileError::Invalid("gzip stream is empty".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).expect("gzip write");
        encoder.finish().expect("gzip finish")
    }

    #[test]
    fn passes_plain_metafiles_through() {
        let mut bytes = vec![0u8; 44];
        bytes[40..44].copy_from_slice(b" EMF");
        let limits = MetafileLimits::default();
        assert_eq!(unpack(&bytes, "a.emf", &limits).unwrap(), bytes);
    }

    #[test]
    fn inflates_emz_and_wmz_parts() {
        let limits = MetafileLimits::default();
        let plain = vec![7u8; 1024];
        let packed = gzip(&plain);
        assert_eq!(unpack(&packed, "image1.emz", &limits).unwrap(), plain);
        assert_eq!(unpack(&packed, "image1.wmz", &limits).unwrap(), plain);
    }

    #[test]
    fn a_bomb_hits_the_limit_instead_of_allocating() {
        let limits = MetafileLimits {
            max_input_bytes: 4096,
            ..MetafileLimits::default()
        };
        let packed = gzip(&vec![0u8; 1 << 20]);
        assert!(matches!(
            unpack(&packed, "bomb.emz", &limits),
            Err(MetafileError::Limit(_))
        ));
    }

    #[test]
    fn truncated_gzip_is_invalid() {
        let limits = MetafileLimits::default();
        let mut packed = gzip(&[1, 2, 3, 4]);
        packed.truncate(packed.len() - 4);
        assert!(matches!(
            unpack(&packed, "image1.emz", &limits),
            Err(MetafileError::Invalid(_))
        ));
    }
}
