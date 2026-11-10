use std::sync::Arc;
use kitty_pptx_core::{HyperlinkTarget, RgbaSlide, RectF32, SlideLink};
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
    // `new` is the conservative Compat default: raw RGBA, no placeholders.
    let driver = KittyDriver::new(PlacementMode::Interactive);
    let caps = driver.caps();
    assert!(caps.alpha);
    assert!(!caps.placeholders);
    assert!(!driver.supports_placeholders());
    assert!(caps.terminal_side_scaling);
    assert_eq!(caps.resident_ids, 8);
    assert_eq!(driver.placement_mode(), PlacementMode::Interactive);
    assert_eq!(driver.flavor(), KittyFlavor::Compat);
    assert_eq!(driver.transmission_format(), TransmissionFormat::RawRgba);
}

#[test]
fn test_kitty_full_driver_caps() {
    let driver = KittyDriver::new_full(PlacementMode::Interactive);
    let caps = driver.caps();
    assert!(caps.alpha);
    // Placeholder mode is opt-in even on genuine kitty.
    assert!(!caps.placeholders);
    assert!(!driver.supports_placeholders());
    assert_eq!(driver.flavor(), KittyFlavor::Full);
    assert_eq!(driver.transmission_format(), TransmissionFormat::Png);
}

#[test]
fn test_kitty_compat_flavor_never_uses_placeholders() {
    let compat = KittyDriver::new_compat(PlacementMode::Interactive);
    assert_eq!(compat.flavor(), KittyFlavor::Compat);
    assert_eq!(compat.transmission_format(), TransmissionFormat::RawRgba);
    assert!(!compat.caps().placeholders);
    assert!(!compat.supports_placeholders());

    // Even when explicitly requested, a Compat terminal must not emit
    // U+10EEEE placeholder cells.
    let forced = KittyDriver::new_compat(PlacementMode::Interactive)
        .with_placeholder_mode(true);
    assert!(forced.is_placeholder_mode());
    assert!(!forced.placeholders_active());
    assert!(!forced.caps().placeholders);
    assert!(!forced.supports_placeholders());
}

#[test]
fn test_kitty_placeholder_mode_requires_full_flavor() {
    let full = KittyDriver::new_full(PlacementMode::Interactive).with_placeholder_mode(true);
    assert!(full.placeholders_active());
    assert!(full.caps().placeholders);
    assert!(full.supports_placeholders());
}

#[test]
fn test_kitty_transmission_uses_raw_rgba_for_compat() {
    let payload = "ABCD1234EFGH5678";
    let chunks = KittyDriver::format_transmission_chunks_raw(ImageId(42), 100, 100, payload);
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].starts_with(
        "\x1b_Ga=t,f=32,i=42,s=100,v=100,q=2,m=0;ABCD1234EFGH5678\x1b\\"
    ));
    assert!(!chunks[0].contains("f=100"));
}

#[test]
fn test_kitty_compat_present_sequence_has_no_placeholders() {
    let driver = KittyDriver::new_compat(PlacementMode::Interactive)
        .with_placeholder_mode(true);
    let rect = CellRect { row: 2, col: 3, rows: 4, cols: 5 };
    let sequences = driver.present_sequences(
        ImageId(7),
        Placement::Cells(rect),
        &[],
    );
    assert_eq!(sequences.len(), 1);
    assert!(sequences[0].starts_with("\x1b[3;4H\x1b_Ga=p,i=7,p=1,c=5,r=4,q=2\x1b\\"));
    assert!(!sequences[0].contains('\u{10EEEE}'));
}

#[test]
fn test_kitty_full_placeholder_present_creates_virtual_placement() {
    let driver = KittyDriver::new_full(PlacementMode::Interactive).with_placeholder_mode(true);
    let rect = CellRect { row: 0, col: 0, rows: 2, cols: 2 };
    let sequences = driver.present_sequences(ImageId(4), Placement::Cells(rect), &[]);
    assert_eq!(sequences.len(), 2);
    // Virtual placement first, placeholders afterwards.
    assert!(sequences[0].contains("a=p,U=1,i=4,p=1,c=2,r=2"));
    assert!(sequences[1].contains('\u{10EEEE}'));
}

#[test]
fn test_kitty_transmission_chunking_single() {
    let payload = "ABCD1234EFGH5678";
    let chunks = KittyDriver::format_transmission_chunks(ImageId(42), 100, 100, payload);
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].starts_with("\x1b_Ga=t,f=100,i=42,s=100,v=100,q=2,m=0;ABCD1234EFGH5678\x1b\\"));
}

#[test]
fn test_kitty_transmission_chunking_multi() {
    // 4096 * 2 + 10 bytes payload
    let payload = "A".repeat(4096 * 2 + 10);
    let chunks = KittyDriver::format_transmission_chunks(ImageId(7), 800, 600, &payload);
    assert_eq!(chunks.len(), 3);

    // First chunk has a=t, f=100, i=7, s=800, v=600, q=2, m=1
    assert!(chunks[0].starts_with("\x1b_Ga=t,f=100,i=7,s=800,v=600,q=2,m=1;"));
    assert!(chunks[0].ends_with("\x1b\\"));

    // Second chunk has m=1
    assert!(chunks[1].starts_with("\x1b_Gm=1;"));
    assert!(chunks[1].ends_with("\x1b\\"));

    // Third chunk has m=0 (terminal chunk)
    assert!(chunks[2].starts_with("\x1b_Gm=0;"));
    assert!(chunks[2].ends_with("\x1b\\"));
}

#[test]
fn test_kitty_unicode_placeholder_generation() {
    let id = ImageId(0x010203); // fg: (3, 2, 1)
    let rect = CellRect { row: 2, col: 4, rows: 2, cols: 3 };
    let placeholders = KittyDriver::generate_placeholder_string(id, rect);

    // Check row 1 positioning: \x1b[3;5H
    assert!(placeholders.contains("\x1b[3;5H"));
    // Check FG encoding ImageId: 0x010203 -> R=3, G=2, B=1
    assert!(placeholders.contains("\x1b[38;2;3;2;1m"));
    // Check BG encoding (col 0, row 0): R=0, G=0, B=0
    assert!(placeholders.contains("\x1b[48;2;0;0;0m\u{10EEEE}"));
    // Check BG encoding (col 1, row 0): R=1, G=0, B=0
    assert!(placeholders.contains("\x1b[48;2;1;0;0m\u{10EEEE}"));
    // Check row 2 positioning: \x1b[4;5H
    assert!(placeholders.contains("\x1b[4;5H"));
    // Check BG encoding (col 0, row 1): R=0, G=0, B=1
    assert!(placeholders.contains("\x1b[48;2;0;0;1m\u{10EEEE}"));
}

#[test]
fn test_kitty_stage_and_sink() {
    let slide = create_test_slide(16, 16, [255, 0, 0, 255]);
    let driver = KittyDriver::new(PlacementMode::Interactive);
    assert_eq!(driver.resident_capacity(), 8);
    // StageSink::stage
    let stage_res = driver.stage(ImageId(10), &slide);
    assert!(stage_res.is_ok());
    // StageSink::release
    let rel_res = driver.release(ImageId(10));
    assert!(rel_res.is_ok());
}

// -----------------------------------------------------------------------------
// HalfBlock Driver Tests
// -----------------------------------------------------------------------------

#[test]
fn test_halfblock_driver_caps() {
    let driver = HalfBlockDriver::new(PlacementMode::Interactive);
    let caps = driver.caps();
    assert!(caps.alpha);
    assert!(!caps.placeholders);
    assert!(!caps.terminal_side_scaling);
    assert_eq!(caps.resident_ids, 16);
    assert_eq!(driver.name(), "halfblock");
}

#[test]
fn test_halfblock_ansi_rendering() {
    // 2x2 slide: top row red, bottom row blue
    let mut pixels = Vec::new();
    // Top-left: red, Top-right: red
    pixels.extend_from_slice(&[255, 0, 0, 255]);
    pixels.extend_from_slice(&[255, 0, 0, 255]);
    // Bottom-left: blue, Bottom-right: blue
    pixels.extend_from_slice(&[0, 0, 255, 255]);
    pixels.extend_from_slice(&[0, 0, 255, 255]);

    let slide = RgbaSlide {
        width: 2,
        height: 2,
        pixels: Arc::from(pixels.into_boxed_slice()),
    };

    // Render to 2 cols x 1 row (2 vertical pixels per cell)
    let ansi = HalfBlockDriver::render_halfblock_to_ansi(&slide, 2, 1);

    // Foreground should be red
    assert!(ansi.contains("\x1b[38;2;255;0;0m"));
    // Background should be blue
    assert!(ansi.contains("\x1b[48;2;0;0;255m"));
    // Character should be half block
    assert!(ansi.contains('▀'));
}

#[test]
fn test_halfblock_staging_lifecycle() {
    let mut driver = HalfBlockDriver::new(PlacementMode::Interactive);
    let slide = create_test_slide(10, 10, [100, 150, 200, 255]);

    // Present before staging should fail
    let err = driver.present(ImageId(5), Placement::ScrollbackBlock);
    assert!(err.is_err());

    // Stage then present
    assert!(TerminalGraphicsDriver::stage(&mut driver, &slide, ImageId(5)).is_ok());
    assert!(driver.present(ImageId(5), Placement::ScrollbackBlock).is_ok());

    // Release
    assert!(driver.release(ImageId(5)).is_ok());
    assert!(driver.present(ImageId(5), Placement::ScrollbackBlock).is_err());
}

// -----------------------------------------------------------------------------
// Sixel Driver Tests
// -----------------------------------------------------------------------------

#[test]
fn test_sixel_driver_caps() {
    let driver = SixelDriver::new(PlacementMode::Interactive);
    let caps = driver.caps();
    assert!(!caps.alpha);
    assert!(!caps.placeholders);
    assert_eq!(caps.resident_ids, 16);
    assert_eq!(driver.name(), "sixel");
}

#[test]
fn test_sixel_encoding_format() {
    let slide = create_test_slide(8, 6, [0, 255, 0, 255]);
    let sixel = SixelDriver::encode_sixel(&slide.pixels, slide.width, slide.height);

    // DCS Introducer
    assert!(sixel.starts_with("\x1bPq"));
    // Palette definition (green 100% -> #0;2;0;100;0)
    assert!(sixel.contains(";2;0;100;0"));
    // Terminator
    assert!(sixel.ends_with("\x1b\\"));
    // Band terminator '-'
    assert!(sixel.contains('-'));
}

#[test]
fn test_sixel_staging_lifecycle() {
    let mut driver = SixelDriver::new(PlacementMode::Interactive);
    let slide = create_test_slide(8, 8, [20, 40, 60, 255]);

    assert!(driver.present(ImageId(1), Placement::ScrollbackBlock).is_err());
    assert!(TerminalGraphicsDriver::stage(&mut driver, &slide, ImageId(1)).is_ok());
    assert!(driver.present(ImageId(1), Placement::ScrollbackBlock).is_ok());
    assert!(driver.release_all().is_ok());
    assert!(driver.present(ImageId(1), Placement::ScrollbackBlock).is_err());
}

// -----------------------------------------------------------------------------
// iTerm2 Driver Tests
// -----------------------------------------------------------------------------

#[test]
fn test_iterm2_formatting() {
    let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ";
    let seq = Iterm2Driver::format_iterm2_sequence(b64, Some(80), Some(24));
    assert_eq!(
        seq,
        format!("\x1b]1337;File=inline=1;width=80;height=24:{b64}\x07")
    );
}

// -----------------------------------------------------------------------------
// Capability Probe Tests
// -----------------------------------------------------------------------------

#[test]
fn test_capability_probe_env_detection() {
    use std::collections::HashMap;

    let detect = |pairs: &[(&str, &str)]| {
        let vars: HashMap<&str, &str> = pairs.iter().copied().collect();
        detect_protocol_with(|key| vars.get(key).map(|v| (*v).to_string()))
    };

    // 1. Kitty
    assert_eq!(detect(&[("KITTY_WINDOW_ID", "1")]), Some(DetectedProtocol::Kitty));

    // 2. Ghostty (kitty graphics, no placeholders)
    assert_eq!(
        detect(&[("TERM_PROGRAM", "ghostty")]),
        Some(DetectedProtocol::KittyCompat)
    );

    // 3. WezTerm (kitty graphics, no placeholders)
    assert_eq!(
        detect(&[("TERM_PROGRAM", "WezTerm")]),
        Some(DetectedProtocol::KittyCompat)
    );

    // 4. iTerm2
    assert_eq!(
        detect(&[("TERM_PROGRAM", "iTerm.app")]),
        Some(DetectedProtocol::Iterm2)
    );

    // 5. Foot
    assert_eq!(detect(&[("FOOT_TERMINAL", "1")]), Some(DetectedProtocol::Sixel));

    // 6. mintty
    assert_eq!(detect(&[("MINTTY_SHORTCUT", "1")]), Some(DetectedProtocol::Sixel));

    // 7. Contour supports sixel, not kitty graphics
    assert_eq!(detect(&[("CONTOUR_VERSION", "0.3")]), Some(DetectedProtocol::Sixel));

    // 8. Alacritty
    assert_eq!(
        detect(&[("ALACRITTY_WINDOW_ID", "1")]),
        Some(DetectedProtocol::HalfBlock)
    );
}

#[test]
fn test_da1_has_sixel() {
    assert!(parse_da1_has_sixel("\x1b[?62;1;4;6;7;8;9c"));
    assert!(parse_da1_has_sixel("\x1b[?63;4c"));
    assert!(!parse_da1_has_sixel("\x1b[?62;1;2;6;7;8;9c"));
    assert!(!parse_da1_has_sixel("\x1b[?1;2c"));
}

#[test]
fn test_instantiate_drivers() {
    let k_driver = CapabilityProbe::instantiate_driver(DetectedProtocol::Kitty, PlacementMode::Interactive);
    assert_eq!(k_driver.name(), "kitty");
    assert!(k_driver.supports_placeholders());

    let k_compat = CapabilityProbe::instantiate_driver(DetectedProtocol::KittyCompat, PlacementMode::Interactive);
    assert_eq!(k_compat.name(), "kitty");
    assert!(!k_compat.supports_placeholders());

    // Scrollback placement never needs placeholders.
    let k_scroll = CapabilityProbe::instantiate_driver(DetectedProtocol::Kitty, PlacementMode::Scrollback);
    assert!(!k_scroll.supports_placeholders());

    let s_driver = CapabilityProbe::instantiate_driver(DetectedProtocol::Sixel, PlacementMode::Interactive);
    assert_eq!(s_driver.name(), "sixel");
    assert!(!s_driver.supports_placeholders());

    let h_driver = CapabilityProbe::instantiate_driver(DetectedProtocol::HalfBlock, PlacementMode::Interactive);
    assert_eq!(h_driver.name(), "halfblock");
    assert!(!h_driver.supports_placeholders());

    let i_driver = CapabilityProbe::instantiate_driver(DetectedProtocol::Iterm2, PlacementMode::Interactive);
    assert_eq!(i_driver.name(), "iterm2");
    assert!(!i_driver.supports_placeholders());
}

// -----------------------------------------------------------------------------
// Aspect Ratio & Geometry Tests
// -----------------------------------------------------------------------------

#[test]
fn test_aspect_ratio_fitting() {
    // 16:9 image (1920x1080)
    // cell aspect ratio = 0.5 (1:2 cell width:height)
    // container: 80 cols x 40 rows
    let (cols, rows) = fit_aspect_ratio(1920, 1080, 80, 40, DEFAULT_CELL_ASPECT_RATIO);
    // With K = 0.5, char AR = 16/9 / 0.5 = 32/9 ≈ 3.555
    // For 80 cols: rows = round(80 / 3.555) = 23
    assert_eq!(cols, 80);
    assert_eq!(rows, 23);

    let container = CellRect { row: 0, col: 0, rows: 40, cols: 80 };
    let centered = calculate_centered_placement(1920, 1080, container, DEFAULT_CELL_ASPECT_RATIO);
    assert_eq!(centered.cols, 80);
    assert_eq!(centered.rows, 23);
    assert_eq!(centered.col, 0);
    assert_eq!(centered.row, (40 - 23) / 2);
}

// -----------------------------------------------------------------------------
// OSC 8 Hyperlink Tests
// -----------------------------------------------------------------------------

#[test]
fn test_kitty_placeholder_without_links_is_byte_identical() {
    let id = ImageId(9);
    let rect = CellRect { row: 1, col: 1, rows: 2, cols: 4 };
    let plain = KittyDriver::generate_placeholder_string(id, rect);
    let linked = KittyDriver::generate_placeholder_string_with_links(id, rect, &[]);
    assert_eq!(plain, linked);
    assert!(!plain.contains("\x1b]8;"));
}

#[test]
fn test_kitty_placeholder_wraps_linked_cells_in_osc8() {
    let id = ImageId(3);
    let rect = CellRect { row: 0, col: 0, rows: 1, cols: 6 };
    let link = TerminalHyperlink::new(
        7,
        "https://example.com/doc",
        CellRect { row: 0, col: 2, rows: 1, cols: 2 },
    );
    let output = KittyDriver::generate_placeholder_string_with_links(id, rect, &[link]);
    assert!(output.contains("\x1b]8;id=link_7;https://example.com/doc\x1b\\"));
    assert!(output.contains("\x1b]8;;\x1b\\"));
    let open = output.find("\x1b]8;id=link_7;").expect("open sequence present");
    let close = output.find("\x1b]8;;\x1b\\").expect("close sequence present");
    assert!(open < close);
}

#[test]
fn test_kitty_placeholder_skips_unsanitized_link_urls() {
    let id = ImageId(3);
    let rect = CellRect { row: 0, col: 0, rows: 1, cols: 3 };
    let link = TerminalHyperlink::new(
        1,
        "javascript:alert(1)",
        CellRect { row: 0, col: 0, rows: 1, cols: 3 },
    );
    let output = KittyDriver::generate_placeholder_string_with_links(id, rect, &[link]);
    assert_eq!(
        output,
        KittyDriver::generate_placeholder_string(id, rect)
    );
}

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
    let placement = CellRect { row: 3, col: 2, rows: 10, cols: 10 };
    let mapped = map_slide_links_to_cells(&links, 1000, 1000, placement, 1);
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].url, "https://example.com");
    assert_eq!(mapped[0].id, 1);
    assert_eq!(
        mapped[0].rect,
        CellRect { row: 5, col: 3, rows: 5, cols: 4 }
    );
}
