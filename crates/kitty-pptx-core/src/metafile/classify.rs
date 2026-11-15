//! Metafile container classification.

use super::MetafileFormat;

/// Classify a part path by extension (case-insensitive).
pub fn classify_path(path: &str) -> Option<MetafileFormat> {
    let (_, ext) = path.rsplit_once('.')?;
    match ext.to_ascii_lowercase().as_str() {
        "emf" => Some(MetafileFormat::Emf),
        "wmf" => Some(MetafileFormat::Wmf),
        "emz" => Some(MetafileFormat::Emz),
        "wmz" => Some(MetafileFormat::Wmz),
        _ => None,
    }
}

/// Whether the path names a metafile the compiler must route to expansion.
pub fn is_metafile_path(path: &str) -> bool {
    classify_path(path).is_some()
}

/// Sniff the container from its bytes.
///
/// gzip-wrapped parts (`.emz`/`.wmz`) cannot be distinguished without
/// inflating, so [`classify_path`] remains authoritative for them.
pub fn classify_bytes(bytes: &[u8]) -> Option<MetafileFormat> {
    if bytes.len() >= 44 && bytes[0..4] == [0x01, 0x00, 0x00, 0x00] && &bytes[40..44] == b" EMF" {
        return Some(MetafileFormat::Emf);
    }
    // Placeable WMF: D7 CD C6 9A.
    if bytes.len() >= 4 && bytes[0..4] == [0xD7, 0xCD, 0xC6, 0x9A] {
        return Some(MetafileFormat::Wmf);
    }
    // Standard METAHEADER: type 1, header size 9 words.
    if bytes.len() >= 4 && bytes[0..4] == [0x01, 0x00, 0x09, 0x00] {
        return Some(MetafileFormat::Wmf);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_extensions_case_insensitively() {
        assert_eq!(classify_path("ppt/media/image1.EMF"), Some(MetafileFormat::Emf));
        assert_eq!(classify_path("ppt/media/image1.Wmf"), Some(MetafileFormat::Wmf));
        assert_eq!(classify_path("ppt/media/image1.emz"), Some(MetafileFormat::Emz));
        assert_eq!(classify_path("ppt/media/image1.wmz"), Some(MetafileFormat::Wmz));
        assert_eq!(classify_path("ppt/media/image1.png"), None);
        assert_eq!(classify_path("emf"), None);
    }

    #[test]
    fn sniffs_emf_and_wmf_headers() {
        let mut emf = vec![0u8; 44];
        emf[0] = 1;
        emf[40..44].copy_from_slice(b" EMF");
        assert_eq!(classify_bytes(&emf), Some(MetafileFormat::Emf));

        assert_eq!(
            classify_bytes(&[0xD7, 0xCD, 0xC6, 0x9A, 0, 0]),
            Some(MetafileFormat::Wmf)
        );
        assert_eq!(
            classify_bytes(&[0x01, 0x00, 0x09, 0x00, 0, 0]),
            Some(MetafileFormat::Wmf)
        );
        assert_eq!(classify_bytes(b"\x89PNG\r\n"), None);
    }
}
