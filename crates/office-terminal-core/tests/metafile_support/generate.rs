//! Record builders and generators for synthetic metafile fixtures (PRD 13a §4).

use std::fs;
use std::io::Write;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;

use super::*;

pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

pub fn synthetic_wmf_placeable() -> Vec<u8> {
    let records = [wmf_rect([10, 10, 120, 80]), wmf_ext_text([20, 60], b"Hi", 0, &[])];
    wmf_placeable(&records)
}

pub fn synthetic_emf_text() -> Vec<u8> {
    emf(&[
        create_font(1, 20, 700, 0, false, true, "Arial"),
        select_object(1),
        set_text_color(0x0000_00FF),
        set_text_align(0x0018),
        ext_text_out((100, 60), "Hi", 0, None, None),
    ])
}

pub fn synthetic_emf_opaque_text() -> Vec<u8> {
    emf(&[
        create_font(1, 20, 400, 0, false, false, "Arial"),
        select_object(1),
        set_bk_mode(2), // OPAQUE
        set_bk_color(0x00FF_FFFF),
        set_text_color(0x0000_0000),
        ext_text_out((100, 60), "Backdrop", 0, None, None),
    ])
}

pub fn synthetic_emf_pen_caps() -> Vec<u8> {
    let path_points = [(10i16, 20i16), (100i16, 20i16)];
    emf_plus_only_emf(&[
        emf_plus_pen(1, 0xFF00_00FF, 8.0, 1, 2),
        emf_plus_path(2, &path_points, false),
        emf_plus_draw_path(2, 1),
    ])
}

pub fn synthetic_emf_hatch() -> Vec<u8> {
    let mut brush = Vec::new();
    brush.extend_from_slice(&2u32.to_le_bytes()); // handle
    brush.extend_from_slice(&2u32.to_le_bytes()); // BS_HATCHED
    brush.extend_from_slice(&0x0000_00FFu32.to_le_bytes()); // red
    brush.extend_from_slice(&0u32.to_le_bytes()); // HS_HORIZONTAL
    let records = [record(39, &brush), select_object(2), rectangle([10, 10, 120, 80])];
    emf(&records)
}

pub fn synthetic_emfplus_dual() -> Vec<u8> {
    let records = [
        rectangle([10, 10, 120, 80]),
        emf_plus_comment(true, &[emf_plus_record(0x4004, 0, &[])]),
    ];
    emf(&records)
}

pub fn synthetic_emfplus_split() -> Vec<u8> {
    let path = emf_plus_path_data(&[(10, 10), (110, 10), (110, 90), (10, 90)], true);
    let total = path.len();
    let split = 16;
    let records = [
        emf_plus_pen(3, 0xff26_2626, 1.0, 0, 0),
        emf_plus_split_object(4, 3, total, &path[..split], true),
        emf_plus_split_object(4, 3, total, &path[split..], true),
        emf_plus_draw_path(4, 3),
    ];
    emf_plus_only_emf(&records)
}

pub fn synthetic_emfplus_linear_gradient() -> Vec<u8> {
    let mut data = EMF_PLUS_MAGIC.to_le_bytes().to_vec();
    data.extend_from_slice(&4u32.to_le_bytes()); // BrushTypeLinearGradient
    data.extend_from_slice(&0u32.to_le_bytes()); // flags
    data.extend_from_slice(&0u32.to_le_bytes()); // wrap mode
    for v in [0.0f32, 0.0, 100.0, 100.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&0xFFFF_0000u32.to_le_bytes()); // start color (red)
    data.extend_from_slice(&0xFF00_00FFu32.to_le_bytes()); // end color (blue)
    data.extend_from_slice(&[0u8; 12]); // reserved / padding
    let records = [
        emf_plus_object(2, 1, &data),
        emf_plus_fill_rects(2, [10.0, 10.0, 110.0, 90.0]),
    ];
    emf_plus_only_emf(&records)
}

pub fn synthetic_emfplus_path_gradient() -> Vec<u8> {
    let records = [
        emf_plus_path_gradient_brush(2, 0xffff_0000, &[0xff00_00ff, 0xff00_ff00], None),
        emf_plus_fill_rects(2, [10.0, 10.0, 110.0, 90.0]),
    ];
    emf_plus_only_emf(&records)
}

pub fn synthetic_emfplus_text_transform() -> Vec<u8> {
    let records = [
        emf_plus_font(1, 12.0, 3, 0, "Carlito"),
        emf_plus_solid_brush(2, 0xff00_0000),
        emf_plus_world_transform([1.0, 0.0, 0.0, 1.0, 100.0, 50.0]),
        emf_plus_draw_string(1, 2, u32::MAX, "A", [0.0, 0.0, 0.0, 0.0]),
        emf_plus_world_transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        emf_plus_draw_string(1, 2, u32::MAX, "A", [100.0, 50.0, 0.0, 0.0]),
    ];
    emf_plus_only_emf(&records)
}

pub fn ensure_synthetic_fixtures(fixture_dir: &Path) -> std::io::Result<()> {
    let real_emf = fixture_dir.join("windows-emf-paths.emf");
    let real_wmf = fixture_dir.join("windows-gdi-vector.wmf");
    let emf_bytes = fs::read(&real_emf)?;
    let wmf_bytes = fs::read(&real_wmf)?;

    let fixtures: Vec<(&str, Vec<u8>)> = vec![
        ("synthetic-wmf-placeable.wmf", synthetic_wmf_placeable()),
        ("synthetic-emf-text.emf", synthetic_emf_text()),
        ("synthetic-emf-opaque-text.emf", synthetic_emf_opaque_text()),
        ("synthetic-emf-pen-caps.emf", synthetic_emf_pen_caps()),
        ("synthetic-emf-hatch.emf", synthetic_emf_hatch()),
        ("synthetic-emfplus-dual.emf", synthetic_emfplus_dual()),
        ("synthetic-emfplus-split.emf", synthetic_emfplus_split()),
        (
            "synthetic-emfplus-linear-gradient.emf",
            synthetic_emfplus_linear_gradient(),
        ),
        ("synthetic-emfplus-path-gradient.emf", synthetic_emfplus_path_gradient()),
        (
            "synthetic-emfplus-text-transform.emf",
            synthetic_emfplus_text_transform(),
        ),
        ("synthetic-emz.emz", gzip(&emf_bytes)),
        ("synthetic-wmz.wmz", gzip(&wmf_bytes)),
        ("synthetic-mislabelled.emz", emf_bytes.clone()),
        ("synthetic-bomb.emz", gzip(&vec![0u8; 10 * 1024 * 1024 + 1])),
    ];

    for (name, bytes) in fixtures {
        let dest = fixture_dir.join(name);
        let update = std::env::var("OT_UPDATE_METAFILE_GOLDENS").is_ok()
            || std::env::var("KITTY_PPTX_UPDATE_METAFILE_GOLDENS").is_ok();
        if !dest.exists() || update {
            fs::write(&dest, bytes)?;
        }
    }

    Ok(())
}
