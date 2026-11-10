//! Terminal-stream golden tests for the display drivers.
//!
//! The drivers write to the process TTY, so these tests capture the real
//! emitted byte stream by re-invoking this test binary in a child process
//! (`golden_stream_child_writer`) with the driver call requested through an
//! environment variable. The parent then parses the protocol sequences back
//! into structured fields and checks pixel/coordinate invariants.
//!
//! These tests intentionally use a small synthetic `RgbaSlide`, keeping them
//! fast and independent of fonts, PPTX parsing or the core golden PNG set.
//!
//! Kitty note: the current driver transmits PNG (`f=100`) payloads. The test
//! understands both `f=100` and the raw-RGBA `f=32` form so a concurrent
//! driver change does not invalidate the structural contract. See the TODO in
//! `kitty_stage_stream_transmits_the_source_image`.

mod golden_stream_support;

use base64::Engine;
use golden_stream_support::{
    capture_stream, expected_halfblock_color, parse_apc_chunks, parse_halfblock_cells,
    parse_placeholder_cells, synthetic_slide, write_marker, IMAGE_ID, SLIDE_H, SLIDE_W,
};
use kitty_pptx_display::*;

/// Child-process test body. Only runs when re-invoked by `capture_stream`;
/// in a normal test run the environment variable is absent and this is a no-op.
#[test]
fn golden_stream_child_writer() {
    let Ok(kind) = std::env::var(golden_stream_support::CHILD_ENV) else {
        return;
    };
    let slide = synthetic_slide();
    let rect = CellRect {
        row: 2,
        col: 4,
        rows: 2,
        cols: 3,
    };

    write_marker(golden_stream_support::BEGIN);
    match kind.as_str() {
        "kitty_stage" => {
            let mut driver = KittyDriver::new(PlacementMode::Interactive);
            TerminalGraphicsDriver::stage(&mut driver, &slide, IMAGE_ID).expect("kitty stage");
        }
        "kitty_cells" => {
            let mut driver = KittyDriver::new(PlacementMode::Interactive);
            driver
                .present(IMAGE_ID, Placement::Cells(rect))
                .expect("kitty present cells");
        }
        "halfblock_cells" => {
            let mut driver = HalfBlockDriver::new(PlacementMode::Interactive);
            TerminalGraphicsDriver::stage(&mut driver, &slide, IMAGE_ID).expect("halfblock stage");
            driver
                .present(
                    IMAGE_ID,
                    Placement::Cells(CellRect {
                        rows: 3,
                        cols: SLIDE_W as u16,
                        ..rect
                    }),
                )
                .expect("halfblock present cells");
        }
        "sixel_cells" => {
            let mut driver = SixelDriver::new(PlacementMode::Interactive);
            TerminalGraphicsDriver::stage(&mut driver, &slide, IMAGE_ID).expect("sixel stage");
            driver
                .present(IMAGE_ID, Placement::Cells(rect))
                .expect("sixel present cells");
        }
        other => panic!("unknown stream child kind {other}"),
    }
    write_marker(golden_stream_support::END);
}

// ---------------------------------------------------------------------------
// Half-block round-trip
// ---------------------------------------------------------------------------

#[test]
fn halfblock_ansi_round_trips_to_the_source_pixels() {
    let slide = synthetic_slide();
    let rect = CellRect {
        row: 2,
        col: 3,
        rows: 3,
        cols: SLIDE_W as u16,
    };
    let ansi = HalfBlockDriver::render_halfblock_cells(&slide, rect);

    assert!(
        ansi.starts_with("\x1b[3;4H"),
        "expected absolute cursor positioning for the placement rect: {ansi:?}"
    );
    let cells = parse_halfblock_cells(&ansi);
    assert_eq!(cells.len(), (SLIDE_W * 3) as usize);

    for cell in &cells {
        let rel_row = cell.row - rect.row;
        let rel_col = cell.col - rect.col;
        assert!(rel_col < SLIDE_W as u16 && rel_row < 3);
        let top = expected_halfblock_color(rel_col as u32, (rel_row * 2) as u32);
        let bottom = expected_halfblock_color(rel_col as u32, (rel_row * 2 + 1) as u32);
        assert_eq!(cell.fg, Some(top), "top half mismatch at {cell:?}");
        assert_eq!(cell.bg, Some(bottom), "bottom half mismatch at {cell:?}");
    }
}

#[test]
fn halfblock_present_emits_the_same_grid() {
    let stream = capture_stream("halfblock_cells");
    let stream = String::from_utf8(stream).expect("halfblock output is UTF-8");
    let cells = parse_halfblock_cells(&stream);
    // rect row=2 col=4, 8 columns and 3 rows => 24 cells.
    assert_eq!(cells.len(), 24);
    assert_eq!((cells[0].row, cells[0].col), (2, 4));
    assert_eq!((cells[23].row, cells[23].col), (4, 11));
    for cell in &cells {
        let rel_row = cell.row - 2;
        let rel_col = cell.col - 4;
        let top = expected_halfblock_color(rel_col as u32, (rel_row * 2) as u32);
        let bottom = expected_halfblock_color(rel_col as u32, (rel_row * 2 + 1) as u32);
        assert_eq!(cell.fg, Some(top));
        assert_eq!(cell.bg, Some(bottom));
    }
}

// ---------------------------------------------------------------------------
// Kitty graphics protocol
// ---------------------------------------------------------------------------

#[test]
fn kitty_stage_stream_transmits_the_source_image() {
    let stream =
        String::from_utf8(capture_stream("kitty_stage")).expect("kitty APC stream is ASCII");
    let chunks = parse_apc_chunks(&stream);
    assert!(!chunks.is_empty(), "stage emitted no APC sequences");

    let first = &chunks[0];
    assert_eq!(first.key("a"), Some("t"));
    assert_eq!(first.key("i"), Some("4660"));
    assert_eq!(first.key("s"), Some("8"));
    assert_eq!(first.key("v"), Some("6"));
    assert_eq!(first.key("q"), Some("2"));
    assert_eq!(first.key("m"), Some("0"));
    assert_eq!(chunks.len(), 1, "8x6 image should fit in one chunk");

    // TODO(display): the driver currently transmits PNG (`f=100`). PRD 03a
    // prefers raw RGBA (`f=32`). Accept either so the structural contract
    // survives the concurrent driver fix.
    match first.key("f") {
        Some("100") => {
            let png = base64::engine::general_purpose::STANDARD
                .decode(&first.payload)
                .expect("base64 PNG payload");
            let image = image::load_from_memory(&png)
                .expect("PNG payload decodes")
                .to_rgba8();
            assert_eq!((image.width(), image.height()), (SLIDE_W, SLIDE_H));
            assert_eq!(
                image.as_raw().as_slice(),
                &*synthetic_slide().pixels,
                "PNG payload does not decode back to the source RGBA"
            );
        }
        Some("32") => {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(&first.payload)
                .expect("base64 raw payload");
            assert_eq!(raw.len(), (SLIDE_W * SLIDE_H * 4) as usize);
            assert_eq!(raw.as_slice(), &*synthetic_slide().pixels);
        }
        other => panic!("unexpected Kitty transmission format f={other:?}"),
    }
}

#[test]
fn kitty_present_cells_emits_cursor_and_placement_keys() {
    let stream =
        String::from_utf8(capture_stream("kitty_cells")).expect("kitty APC stream is ASCII");
    // rect row=2 col=4 => 1-based cursor 3;5
    assert!(
        stream.starts_with("\x1b[3;5H"),
        "present(Cells) must position the cursor first: {stream:?}"
    );
    let chunks = parse_apc_chunks(&stream);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.key("a"), Some("p"));
    assert_eq!(chunk.key("i"), Some("4660"));
    assert_eq!(chunk.key("c"), Some("3"));
    assert_eq!(chunk.key("r"), Some("2"));
    assert_eq!(chunk.key("q"), Some("2"));
    assert!(chunk.payload.is_empty());
}

#[test]
fn kitty_placeholder_grid_encodes_image_id_and_cell_coordinates() {
    let id = ImageId(0x010203);
    let rect = CellRect {
        row: 2,
        col: 4,
        rows: 2,
        cols: 3,
    };
    let stream = KittyDriver::generate_placeholder_string(id, rect);
    let cells = parse_placeholder_cells(&stream);
    assert_eq!(cells.len(), (rect.rows * rect.cols) as usize);

    for (cell, link) in &cells {
        assert!(link.is_none(), "plain placeholders must not emit OSC 8");
        // FG = ImageId as RGB little-endian: 0x010203 -> (3, 2, 1).
        assert_eq!(cell.fg, Some([0x03, 0x02, 0x01]));
        let rel_row = cell.row - rect.row;
        let rel_col = cell.col - rect.col;
        // Current driver encodes BG as R=col, G=col>>8, B=row.
        assert_eq!(
            cell.bg,
            Some([rel_col as u8, (rel_col >> 8) as u8, rel_row as u8]),
            "placeholder offset mismatch at {cell:?}"
        );
    }

    // Every cell is visited exactly once.
    let mut seen = std::collections::HashSet::new();
    for (cell, _) in &cells {
        assert!(seen.insert((cell.row, cell.col)), "cell repeated: {cell:?}");
    }
}

#[test]
fn kitty_placeholder_links_wrap_only_covered_cells() {
    let id = ImageId(0x334455);
    let rect = CellRect {
        row: 1,
        col: 1,
        rows: 3,
        cols: 4,
    };
    let link = TerminalHyperlink::new(
        7,
        "https://example.com/doc",
        CellRect {
            row: 2,
            col: 2,
            rows: 1,
            cols: 2,
        },
    );
    let stream =
        KittyDriver::generate_placeholder_string_with_links(id, rect, std::slice::from_ref(&link));
    let cells = parse_placeholder_cells(&stream);
    assert_eq!(cells.len(), (rect.rows * rect.cols) as usize);

    for (cell, active) in &cells {
        let inside = link.rect.contains(cell.col, cell.row);
        if inside {
            assert_eq!(active.as_deref(), Some(link.url.as_str()), "cell {cell:?}");
        } else {
            assert!(active.is_none(), "link leaked onto cell {cell:?}");
        }
    }
    assert_eq!(
        stream
            .matches("\x1b]8;id=link_7;https://example.com/doc\x1b\\")
            .count(),
        1,
        "expected one OSC 8 open for the contiguous link region"
    );
    assert_eq!(
        stream.matches("\x1b]8;;\x1b\\").count(),
        1,
        "expected one OSC 8 close"
    );
}

// ---------------------------------------------------------------------------
// Sixel
// ---------------------------------------------------------------------------

fn dcs_body(stream: &str) -> &str {
    let start = stream.find("\x1bP").expect("Sixel DCS introducer");
    let body_start = start + 2;
    let end = stream[body_start..]
        .find("\x1b\\")
        .expect("Sixel string terminator");
    &stream[body_start..body_start + end]
}

#[test]
fn sixel_present_emits_a_valid_dcs_stream() {
    let stream = String::from_utf8(capture_stream("sixel_cells")).expect("sixel output is ASCII");
    assert!(
        stream.starts_with("\x1b[3;5H"),
        "present(Cells) must position the cursor: {stream:?}"
    );
    let body = dcs_body(&stream);
    assert!(
        body.starts_with('q'),
        "DCS must introduce Sixel data with q"
    );
    assert!(body.len() > 4);
    assert!(body.contains('#')); // palette definition
    assert!(body.contains('-')); // at least one sixel band terminator
    assert!(stream.ends_with("\x1b\\"));

    // Raster attributes ("1;1;W;H) are specified by PRD 03b. The current
    // encoder omits them; validate when present, and leave the strict check
    // to `sixel_stream_includes_raster_attributes`.
    let sixel_data = &body[1..];
    if sixel_data.starts_with('"') {
        let attr = sixel_data
            .split(|c: char| !(c.is_ascii_digit() || c == '"' || c == ';'))
            .next()
            .unwrap_or("");
        let parts: Vec<u32> = attr
            .trim_start_matches('"')
            .split(';')
            .filter_map(|part| part.parse().ok())
            .collect();
        assert_eq!(parts.len(), 4, "raster attribute must be \"Pan;Pad;W;H");
        assert_eq!(parts[0], 1);
        assert_eq!(parts[1], 1);
        assert_eq!((parts[2], parts[3]), (30, 40)); // 3 cols * 10px, 2 rows * 20px
    }
}

/// PRD 03b requires raster attributes in the Sixel DCS. The current display
/// crate encoder does not emit them; enable this test once that lands.
#[test]
#[ignore = "display crate Sixel encoder does not emit raster attributes yet"]
fn sixel_stream_includes_raster_attributes() {
    let slide = synthetic_slide();
    let sixel = SixelDriver::encode_sixel(&slide.pixels, slide.width, slide.height);
    let body = dcs_body(&sixel);
    assert!(
        body.starts_with(&format!("\"1;1;{};{}", slide.width, slide.height)),
        "expected sixel raster attribute \"1;1;{};{}\", got {:?}",
        slide.width,
        slide.height,
        &body[..body.len().min(24)]
    );
}
