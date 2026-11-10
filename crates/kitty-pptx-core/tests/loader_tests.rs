use std::sync::Arc;
use kitty_pptx_core::*;

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
        img1.write_to(
            &mut std::io::Cursor::new(&mut png1),
            image::ImageFormat::Png,
        )
        .unwrap();

        zip.start_file("ppt/media/image1.png", options).unwrap();
        std::io::Write::write_all(&mut zip, &png1).unwrap();

        // 2. Create another 4x4 PNG
        let img2 = image::RgbaImage::from_fn(4, 4, |_, _| image::Rgba([40, 50, 60, 255]));
        let mut png2 = Vec::new();
        img2.write_to(
            &mut std::io::Cursor::new(&mut png2),
            image::ImageFormat::Png,
        )
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
