use std::sync::Arc;

use kitty_pptx_core::{HyperlinkTarget, RectF32, RgbaSlide, SlideLink};
use kitty_pptx_display::probe::env::kitty_from_env_with;
use kitty_pptx_display::*;

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
        "\x1b[3;4H\x1b_Ga=p,i=7,p=1,c=5,r=4,z=-1,q=2\x1b\\"
    );
    assert!(!sequences[0].contains('\u{10EEEE}'));
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
        "\x1b[3;4H\x1b_Ga=p,i=7,p=1,x=10,y=20,w=100,h=200,c=5,r=4,z=-1,C=1,q=2\x1b\\"
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
