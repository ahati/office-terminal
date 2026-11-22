use std::sync::Arc;

use office_terminal_core::{
    DisplayError, HyperlinkTarget, RectF32, RgbaSlide, SlideLink, TransmitEncoder,
};
use office_terminal_display::probe::env::kitty_from_env_with;
use office_terminal_display::*;

fn create_test_slide(w: u32, h: u32, color: [u8; 4]) -> RgbaSlide {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        pixels.extend_from_slice(&color);
    }
    RgbaSlide {
        width: w,
        height: h,
        pixels: Arc::from(pixels.into_boxed_slice()),
    }
}

// -----------------------------------------------------------------------------
// Kitty Driver Tests
// -----------------------------------------------------------------------------

#[test]
fn test_kitty_driver_caps() {
    let driver = KittyDriver::new(PlacementMode::Interactive);
    let caps = driver.caps();
    assert!(caps.alpha);
    assert!(caps.terminal_side_scaling);
    assert_eq!(caps.resident_ids, RESIDENT_IMAGE_IDS);
    assert!(
        caps.resident_ids >= THUMBNAIL_IMAGE_ID_BASE + MAX_THUMBNAIL_IMAGES - 1,
        "resident pool must cover the thumbnail id range"
    );
    assert_eq!(driver.placement_mode(), PlacementMode::Interactive);
}

#[test]
fn test_image_id_partition_is_disjoint() {
    // Machine-checks the documented id partition in `driver.rs`: current
    // slide `1..=8`, preview `9..=16`, thumbnails `17..=80`, reader `81..=92`.
    let ranges: [(u32, u32); 4] = [
        (CURRENT_IMAGE_ID_BASE, CURRENT_IMAGE_ID_SLOTS),
        (PREVIEW_IMAGE_ID_BASE, PREVIEW_IMAGE_ID_SLOTS),
        (THUMBNAIL_IMAGE_ID_BASE, MAX_THUMBNAIL_IMAGES),
        (READER_IMAGE_ID_BASE, READER_IMAGE_ID_SLOTS),
    ];
    for (i, &(base_i, len_i)) in ranges.iter().enumerate() {
        assert!(len_i > 0, "partition range at index {i} must be non-empty");
        for &(base_j, len_j) in ranges.iter().skip(i + 1) {
            assert!(
                base_i.saturating_add(len_i) <= base_j
                    || base_j.saturating_add(len_j) <= base_i,
                "id ranges ({base_i}, {len_i}) and ({base_j}, {len_j}) must not overlap"
            );
        }
    }
    let resident_ids = RESIDENT_IMAGE_IDS;
    let reader_end = READER_IMAGE_ID_BASE + READER_IMAGE_ID_SLOTS;
    assert!(
        resident_ids + 1 >= reader_end,
        "resident pool must cover the reader range"
    );
}

#[test]
fn test_tty_write_seq_batch_smoke() {
    // A batch — including the empty batch — must complete one buffered
    // write+flush without error; output is captured by the test harness.
    let tty = TtyWriter::new();
    assert!(tty.write_seq_batch(&[]).is_ok());
    assert!(tty
        .write_seq_batch(&["\x1b_Ga=d,d=a,q=2\x1b\\", "\x1b_Gm=0;QUJD\x1b\\"])
        .is_ok());
}

#[test]
fn test_kitty_transmission_chunking_single() {
    let payload = "ABCD1234EFGH5678";
    let chunks = KittyDriver::format_transmission_chunks(ImageId(42), 100, 100, payload);
    assert_eq!(chunks.len(), 1);
    assert!(
        chunks[0].starts_with("\x1b_Ga=t,f=100,i=42,s=100,v=100,q=2,m=0;ABCD1234EFGH5678\x1b\\")
    );
}

#[test]
fn test_kitty_transmission_chunking_multi() {
    let payload = "A".repeat(4096 * 2 + 10);
    let chunks = KittyDriver::format_transmission_chunks(ImageId(7), 800, 600, &payload);
    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].starts_with("\x1b_Ga=t,f=100,i=7,s=800,v=600,q=2,m=1;"));
    assert!(chunks[1].starts_with("\x1b_Gm=1;"));
    assert!(chunks[2].starts_with("\x1b_Gm=0;"));
}

#[test]
fn test_kitty_present_sequence_uses_direct_placement() {
    let rect = CellRect {
        row: 2,
        col: 3,
        rows: 4,
        cols: 5,
    };
    let sequences = KittyDriver::present_sequences(ImageId(7), Placement::Cells(rect));
    assert_eq!(sequences.len(), 1);
    assert_eq!(
        sequences[0],
        "\x1b[3;4H\x1b_Ga=p,i=7,p=1,c=5,r=4,z=-1073741825,q=2\x1b\\"
    );
    assert!(!sequences[0].contains('\u{10EEEE}'));
}

#[test]
fn test_kitty_present_z_sits_below_cell_backgrounds() {
    // Kitty and Ghostty draw `INT32_MIN/2 <= z < 0` above non-default cell
    // backgrounds. Overlay fills must stay opaque, so the slide has to go
    // strictly below that band; the Alacritty fork treats every negative z
    // the same way, so one value covers both layer models.
    let rect = CellRect {
        row: 2,
        col: 3,
        rows: 4,
        cols: 5,
    };
    let sequences = KittyDriver::present_sequences(ImageId(7), Placement::Cells(rect));
    let z = sequences[0]
        .split("z=")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|raw| raw.parse::<i32>().ok())
        .expect("present sequence must carry a z-index");
    assert_eq!(z, BEHIND_CELL_BACKGROUNDS_Z);
    assert!(
        z < i32::MIN / 2,
        "slide placements must sit under the cell-background layer (z={z})"
    );
}

#[test]
fn test_kitty_present_sequence_subrect() {
    let rect = CellRect {
        row: 2,
        col: 3,
        rows: 4,
        cols: 5,
    };
    let src = PixelRect {
        x: 10,
        y: 20,
        width: 100,
        height: 200,
    };
    let sequences = KittyDriver::present_sequences(ImageId(7), Placement::CellsSubrect { rect, src });
    assert_eq!(sequences.len(), 1);
    assert_eq!(
        sequences[0],
        format!("\x1b[3;4H\x1b_Ga=p,i=7,p=1,x=10,y=20,w=100,h=200,c=5,r=4,z={BEHIND_CELL_BACKGROUNDS_Z},C=1,q=2\x1b\\")
    );
}

#[test]
fn test_cell_pixel_metrics() {
    let default_metric = CellPixelSize::default();
    assert_eq!(default_metric.width_px, 10.0);
    assert_eq!(default_metric.height_px, 20.0);
    assert_eq!(default_metric.aspect_ratio(), 0.5);
}

#[test]
fn test_kitty_stage_and_sink() {
    let slide = create_test_slide(16, 16, [255, 0, 0, 255]);
    let driver = KittyDriver::new(PlacementMode::Interactive);
    assert_eq!(driver.resident_capacity(), RESIDENT_IMAGE_IDS);
    assert!(driver.stage(ImageId(10), &slide).is_ok());
    assert!(driver.release(ImageId(10)).is_ok());
}

#[test]
fn test_kitty_transmit_prebuilt_reuses_payload_without_reencode() {
    let slide = create_test_slide(8, 8, [0, 255, 0, 255]);

    // The encoder produces a base64 wire payload the driver can slice as-is.
    let payload = KittyTransmitEncoder
        .encode(&slide)
        .expect("transmit encode must succeed");
    assert!(!payload.base64.is_empty());
    assert_eq!(payload.base64_len, payload.base64.len());

    // Framing from the prebuilt payload matches the documented chunk format.
    let chunks = KittyDriver::format_transmission_chunks(ImageId(10), 8, 8, &payload.base64);
    assert!(!chunks.is_empty());
    assert!(chunks[0].starts_with("\x1b_Ga=t,f=100,i=10,s=8,v=8,q=2,m="));

    // The driver accepts the prebuilt payload without re-encoding it.
    let mut driver = KittyDriver::new(PlacementMode::Interactive);
    assert!(driver.transmit_prebuilt(ImageId(10), 8, 8, &payload).is_ok());
    assert!(driver.release(ImageId(10)).is_ok());
}

#[test]
fn test_default_transmit_prebuilt_reports_unsupported() {
    // Drivers that never pre-encode (e.g. test mocks) inherit the trait's
    // default, which reports the capability gap instead of panicking — this
    // is what keeps the TUI driver mocks free of per-method stubs.
    struct NoPrebuiltDriver;

    impl TerminalGraphicsDriver for NoPrebuiltDriver {
        fn name(&self) -> &'static str {
            "stub"
        }
        fn caps(&self) -> DriverCaps {
            DriverCaps {
                alpha: false,
                terminal_side_scaling: false,
                resident_ids: 1,
            }
        }
        fn placement_mode(&self) -> PlacementMode {
            PlacementMode::Interactive
        }
        fn stage(&mut self, _bmp: &RgbaSlide, _id: ImageId) -> Result<(), DisplayError> {
            Ok(())
        }
        fn present(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
            Ok(())
        }
        fn reposition(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
            Ok(())
        }
        fn release(&mut self, _id: ImageId) -> Result<(), DisplayError> {
            Ok(())
        }
        fn release_all(&mut self) -> Result<(), DisplayError> {
            Ok(())
        }
    }

    let payload = office_terminal_core::TransmitPayload {
        base64: "".into(),
        base64_len: 0,
    };
    let mut driver = NoPrebuiltDriver;
    let result = driver.transmit_prebuilt(ImageId(1), 1, 1, &payload);
    assert!(matches!(result, Err(DisplayError::Io(_))));
}

#[test]
fn test_instantiate_driver_is_kitty() {
    let driver = CapabilityProbe::instantiate_driver(PlacementMode::Interactive);
    assert_eq!(driver.name(), "kitty");
    let scrollback = CapabilityProbe::instantiate_driver(PlacementMode::Scrollback);
    assert_eq!(scrollback.name(), "kitty");
}

// -----------------------------------------------------------------------------
// Capability Probe Tests
// -----------------------------------------------------------------------------

#[test]
fn test_kitty_env_detection() {
    let detect = |pairs: &[(&str, &str)]| {
        kitty_from_env_with(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        })
    };

    assert!(detect(&[("KITTY_WINDOW_ID", "1")]));
    assert!(detect(&[("KITTY_PID", "42")]));
    assert!(detect(&[("TERM", "xterm-kitty")]));
    assert!(detect(&[("TERM_PROGRAM", "kitty")]));

    // Alacritty is left to the active query so forks with Kitty graphics work.
    assert!(!detect(&[("ALACRITTY_WINDOW_ID", "1")]));
    assert!(!detect(&[("TERM", "alacritty")]));
    assert!(!detect(&[("TERM_PROGRAM", "iTerm.app")]));
    assert!(!detect(&[("FOOT_TERMINAL", "1")]));
    assert!(!detect(&[]));
}

// -----------------------------------------------------------------------------
// Aspect Ratio & Geometry Tests
// -----------------------------------------------------------------------------

#[test]
fn test_aspect_ratio_fitting() {
    let (cols, rows) = fit_aspect_ratio(1920, 1080, 80, 40, DEFAULT_CELL_ASPECT_RATIO);
    assert_eq!(cols, 80);
    assert_eq!(rows, 23);

    let container = CellRect {
        row: 0,
        col: 0,
        rows: 40,
        cols: 80,
    };
    let centered = calculate_centered_placement(1920, 1080, container, DEFAULT_CELL_ASPECT_RATIO);
    assert_eq!(centered.cols, 80);
    assert_eq!(centered.rows, 23);
    assert_eq!(centered.col, 0);
    assert_eq!(centered.row, (40 - 23) / 2);
}

// -----------------------------------------------------------------------------
// Hyperlink Mapping Tests
// -----------------------------------------------------------------------------

#[test]
fn test_map_slide_links_to_cells_and_sanitize() {
    let links = vec![
        SlideLink::new(
            HyperlinkTarget::ExternalUri("https://example.com".into()),
            RectF32::new(100.0, 200.0, 300.0, 400.0),
            None,
        ),
        SlideLink::new(
            HyperlinkTarget::SlideIndex(3),
            RectF32::new(0.0, 0.0, 1000.0, 1000.0),
            None,
        ),
        SlideLink::new(
            HyperlinkTarget::ExternalUri("file:///etc/passwd".into()),
            RectF32::new(0.0, 0.0, 1000.0, 1000.0),
            None,
        ),
    ];
    let placement = CellRect {
        row: 3,
        col: 2,
        rows: 10,
        cols: 10,
    };
    let mapped = map_slide_links_to_cells(&links, 1000, 1000, placement, 1);
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].url, "https://example.com");
    assert_eq!(mapped[0].id, 1);
    assert_eq!(
        mapped[0].rect,
        CellRect {
            row: 5,
            col: 3,
            rows: 5,
            cols: 4
        }
    );
}

/// Transmission payloads must actually deflate: the pty write is the
/// bottleneck, so a near-raw PNG stalls the UI thread for the whole drain
/// whenever a page scrolls into view (the reader's boundary flicker).
/// `Fast` + `NoFilter` encoded a text-page raster at ~44 % of its raw size
/// (~34 MB for a 2560×3312 reader page → ~45 MB of base64 on the wire);
/// the current settings must stay an order of magnitude below that.
#[test]
fn test_transmit_payload_deflates_text_pages() {
    // Text-page-ish raster: white with dark "glyph" bars.
    let (w, h) = (640u32, 830u32);
    let mut pixels = vec![255u8; (w * h * 4) as usize];
    for row in 0..h {
        if row % 9 < 3 {
            for col in 0..w {
                if col % 7 < 2 {
                    let i = ((row * w + col) * 4) as usize;
                    pixels[i] = 30;
                    pixels[i + 1] = 30;
                    pixels[i + 2] = 30;
                }
            }
        }
    }
    let page = RgbaSlide {
        width: w,
        height: h,
        pixels: Arc::from(pixels.into_boxed_slice()),
    };
    let raw = page.pixels.len();
    let payload = encode_transmit_payload(&page).expect("payload encodes");

    // Base64 inflates by 4/3; even so the wire form must stay tiny against
    // the raw raster. Fast+NoFilter landed at ~59 % (44 % PNG + base64).
    assert!(
        payload.base64_len < raw / 10,
        "transmit payload {}/{} bytes ({:.1} % of raw) is not deflating",
        payload.base64_len,
        raw,
        payload.base64_len as f32 / raw as f32 * 100.0
    );
}
