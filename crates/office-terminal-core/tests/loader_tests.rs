use office_terminal_core::*;
use std::sync::Arc;

#[test]
fn test_asset_store_caching_and_lru_eviction() {
    // Create an in-memory zip archive with two sample images
    let mut zip_buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut zip_buffer);
        let options = zip::write::SimpleFileOptions::default();

        // 1. Create a 4x4 PNG
        let img1 = image::RgbaImage::from_fn(4, 4, |_, _| image::Rgba([10, 20, 30, 255]));
        let mut png1 = Vec::new();
        img1.write_to(&mut std::io::Cursor::new(&mut png1), image::ImageFormat::Png)
            .unwrap();

        zip.start_file("ppt/media/image1.png", options).unwrap();
        std::io::Write::write_all(&mut zip, &png1).unwrap();

        // 2. Create another 4x4 PNG
        let img2 = image::RgbaImage::from_fn(4, 4, |_, _| image::Rgba([40, 50, 60, 255]));
        let mut png2 = Vec::new();
        img2.write_to(&mut std::io::Cursor::new(&mut png2), image::ImageFormat::Png)
            .unwrap();

        zip.start_file("ppt/media/image2.png", options).unwrap();
        std::io::Write::write_all(&mut zip, &png2).unwrap();

        zip.finish().unwrap();
    }

    let bytes = Arc::from(zip_buffer.into_inner().into_boxed_slice());

    // Create asset store with a tiny budget (e.g. 100 bytes) to force eviction
    // Each 4x4 RGBA image is 4 * 4 * 4 = 64 bytes.
    // Budget of 100 bytes means only 1 image fits in cache at a time!
    let store = DefaultAssetStore::with_cache_budget(bytes, None, 100);

    let stats0 = store.stats();
    assert_eq!(stats0.cached_entries, 0);

    // Fetch image1
    let img1 = store.get("ppt/media/image1.png", SizeHint::Native).unwrap();
    assert_eq!(img1.width, 4);
    assert_eq!(img1.height, 4);

    let stats1 = store.stats();
    assert_eq!(stats1.cached_entries, 1);
    assert_eq!(stats1.cached_bytes, 64);

    // Fetch image1 again: should hit cache
    let img1_again = store.get("ppt/media/image1.png", SizeHint::Native).unwrap();
    assert_eq!(img1_again.pixels.len(), 64);
    assert_eq!(store.stats().cached_entries, 1);

    // Fetch image2: exceeds budget, so image1 is evicted
    let img2 = store.get("ppt/media/image2.png", SizeHint::Native).unwrap();
    assert_eq!(img2.width, 4);
    assert_eq!(img2.height, 4);

    let stats2 = store.stats();
    assert_eq!(stats2.cached_entries, 1);
    assert_eq!(stats2.cached_bytes, 64);

    // Raw bytes check
    let raw = store.get_raw_bytes("ppt/media/image1.png").unwrap();
    assert!(!raw.is_empty());
}

fn load_bytes(bytes: Vec<u8>) -> Result<pptx_model::Presentation, ParseError> {
    DefaultPresentationLoader
        .load(Source::Bytes(bytes))
        .map(|(presentation, _assets)| presentation)
}

fn cfb_with(stream: &str) -> Vec<u8> {
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    bytes.extend([0u8; 512]);
    bytes.extend(stream.encode_utf16().flat_map(|unit| unit.to_le_bytes()));
    bytes
}

#[test]
fn encrypted_cfb_is_reported_as_password_protected() {
    let error = load_bytes(cfb_with("EncryptedPackage")).expect_err("encrypted package");
    assert!(matches!(error, ParseError::PasswordProtected), "{error:?}");
}

#[test]
fn legacy_powerpoint_cfb_names_the_binary_format() {
    let error = load_bytes(cfb_with("PowerPoint Document")).expect_err("legacy ppt");
    match error {
        ParseError::ArchiveError(message) => {
            assert!(message.contains("legacy"), "{message}");
            assert!(message.contains(".pptx"), "{message}");
        }
        other => panic!("expected ArchiveError, got {other:?}"),
    }
}

#[test]
fn unknown_cfb_is_not_reported_as_corrupt_zip() {
    let error = load_bytes(cfb_with("Some Other Stream")).expect_err("unknown cfb");
    match error {
        ParseError::ArchiveError(message) => assert!(message.contains("not an OOXML"), "{message}"),
        other => panic!("expected ArchiveError, got {other:?}"),
    }
}

#[test]
fn non_zip_bytes_are_rejected_before_the_parser() {
    let error = load_bytes(b"not a package at all".to_vec()).expect_err("plain bytes");
    match error {
        ParseError::ArchiveError(message) => assert!(message.contains("not an OOXML"), "{message}"),
        other => panic!("expected ArchiveError, got {other:?}"),
    }
}
