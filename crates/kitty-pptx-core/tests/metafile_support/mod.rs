//! Fixture builders and harness helpers for the metafile playback tests.
//!
//! The EMF/WMF encoders mirror the record layouts documented by [MS-EMF] /
//! [MS-WMF] and exercised by the engine's own test-suite; keeping them here
//! lets M3–M5 build text, gradient and container fixtures without checking in
//! opaque binaries.

#![allow(dead_code)]

use crossbeam_channel::Sender;
use kitty_pptx_core::contracts::{RectF32, VectorCommand, VectorSlide, WarningEvent};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::AssetError;
use kitty_pptx_core::{DefaultSlideCompiler, SlideCompiler};

pub const EMF_SIGNATURE: u32 = 0x464D_4520; // " EMF"

/// Smallest well-formed EMF: EMR_HEADER + EMR_EOF.
pub fn minimal_emf() -> Vec<u8> {
    emf(&[])
}

/// EMF record with an 8-byte header; `payload` must be 4-byte aligned.
pub fn record(kind: u32, payload: &[u8]) -> Vec<u8> {
    let size = 8 + payload.len();
    assert_eq!(size % 4, 0, "record {kind} payload must be 4-byte aligned");
    let mut output = Vec::with_capacity(size);
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(&(size as u32).to_le_bytes());
    output.extend_from_slice(payload);
    output
}

pub fn i32s(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

pub fn transform(values: [f32; 6]) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

pub fn emf_plus_record(kind: u16, flags: u16, data: &[u8]) -> Vec<u8> {
    assert_eq!(data.len() % 4, 0);
    let mut output = Vec::with_capacity(12 + data.len());
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(&flags.to_le_bytes());
    output.extend_from_slice(&((12 + data.len()) as u32).to_le_bytes());
    output.extend_from_slice(&(data.len() as u32).to_le_bytes());
    output.extend_from_slice(data);
    output
}

/// `EMR_GDICOMMENT` carrying an EMF+ stream (Dual when `dual`).
pub fn emf_plus_comment(dual: bool, extra: &[Vec<u8>]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&0xdbc0_1002u32.to_le_bytes());
    header.extend_from_slice(&u32::from(dual).to_le_bytes());
    header.extend_from_slice(&96u32.to_le_bytes());
    header.extend_from_slice(&96u32.to_le_bytes());
    let mut stream = emf_plus_record(0x4001, u16::from(dual), &header);
    for item in extra {
        stream.extend_from_slice(item);
    }
    stream.extend_from_slice(&emf_plus_record(0x4002, 0, &[]));
    let mut comment = ((stream.len() + 4) as u32).to_le_bytes().to_vec();
    comment.extend_from_slice(b"EMF+");
    comment.extend_from_slice(&stream);
    record(70, &comment)
}

/// Minimal EMF: 88-byte header, `records`, then `EMR_EOF`.
pub fn emf(records: &[Vec<u8>]) -> Vec<u8> {
    let mut output = vec![0; 88];
    output[0..4].copy_from_slice(&1u32.to_le_bytes());
    output[4..8].copy_from_slice(&88u32.to_le_bytes());
    output[8..24].copy_from_slice(&i32s(&[0, 0, 200, 100]));
    output[24..40].copy_from_slice(&i32s(&[0, 0, 5292, 2646]));
    output[40..44].copy_from_slice(&EMF_SIGNATURE.to_le_bytes());
    output[44..48].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    output[56..58].copy_from_slice(&32u16.to_le_bytes());
    output[72..80].copy_from_slice(&i32s(&[1920, 1080]));
    output[80..88].copy_from_slice(&i32s(&[508, 286]));
    for item in records {
        output.extend_from_slice(item);
    }
    output.extend_from_slice(&record(14, &[0; 12]));
    let length = output.len() as u32;
    output[48..52].copy_from_slice(&length.to_le_bytes());
    output[52..56].copy_from_slice(&((records.len() + 2) as u32).to_le_bytes());
    output
}

/// `LOGFONTW` (92 bytes) + handle for `EMR_EXTCREATEFONTINDIRECTW`.
pub fn create_font(
    handle: u32,
    height: i32,
    weight: i32,
    escapement_tenths: i32,
    italic: bool,
    underline: bool,
    family: &str,
) -> Vec<u8> {
    let mut payload = handle.to_le_bytes().to_vec();
    let mut font = vec![0u8; 92];
    font[0..4].copy_from_slice(&height.to_le_bytes());
    font[8..12].copy_from_slice(&escapement_tenths.to_le_bytes());
    font[16..20].copy_from_slice(&weight.to_le_bytes());
    font[20] = u8::from(italic);
    font[21] = u8::from(underline);
    font[23] = 0;
    for (index, unit) in family.encode_utf16().take(31).enumerate() {
        font[28 + index * 2..30 + index * 2].copy_from_slice(&unit.to_le_bytes());
    }
    payload.extend_from_slice(&font);
    record(82, &payload)
}

/// `EMR_EXTTEXTOUTW` with optional `ETO_OPAQUE` rect and per-char advances.
pub fn ext_text_out(
    reference: (i32, i32),
    text: &str,
    options: u32,
    opaque_rect: Option<[i32; 4]>,
    dx: Option<&[i32]>,
) -> Vec<u8> {
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let count = text.encode_utf16().count() as u32;
    let mut payload = vec![0u8; 68];
    payload[28..36].copy_from_slice(&i32s(&[reference.0, reference.1]));
    payload[36..40].copy_from_slice(&count.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload[44..48].copy_from_slice(&options.to_le_bytes());
    if let Some(rect) = opaque_rect {
        payload[48..64].copy_from_slice(&i32s(&rect));
    }
    payload.extend_from_slice(&utf16);
    // Records are 4-byte aligned (`record` asserts it); the string offset is
    // fixed but the `dx` array must follow the padding.
    while !payload.len().is_multiple_of(4) {
        payload.push(0);
    }
    if let Some(values) = dx {
        let dx_offset = (8 + payload.len()) as u32;
        payload[64..68].copy_from_slice(&dx_offset.to_le_bytes());
        payload.extend_from_slice(&i32s(values));
    }
    record(84, &payload)
}

pub fn select_object(handle: u32) -> Vec<u8> {
    record(37, &handle.to_le_bytes())
}

pub fn set_text_align(align: u32) -> Vec<u8> {
    record(22, &align.to_le_bytes())
}

pub fn set_text_color(color: u32) -> Vec<u8> {
    record(24, &color.to_le_bytes())
}

pub fn set_bk_mode(mode: u32) -> Vec<u8> {
    record(18, &mode.to_le_bytes())
}

pub fn set_bk_color(color: u32) -> Vec<u8> {
    record(25, &color.to_le_bytes())
}

pub fn rectangle(rect: [i32; 4]) -> Vec<u8> {
    record(43, &i32s(&rect))
}

// ── EMF+ records ───────────────────────────────────────────────────────────

const EMF_PLUS_MAGIC: u32 = 0xdbc0_1002;

/// `EmfPlusObject` with an object id and type.
pub fn emf_plus_object(id: u8, kind: u8, data: &[u8]) -> Vec<u8> {
    emf_plus_record(
        0x4008,
        u16::from(id) | (u16::from(kind) << 8),
        data,
    )
}

/// EMF+ font object (type 6).
pub fn emf_plus_font(id: u8, size: f32, unit: u32, style: u32, family: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&EMF_PLUS_MAGIC.to_le_bytes());
    data.extend_from_slice(&size.to_le_bytes());
    data.extend_from_slice(&unit.to_le_bytes());
    data.extend_from_slice(&style.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    let units: Vec<u16> = family.encode_utf16().collect();
    data.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for unit in units {
        data.extend_from_slice(&unit.to_le_bytes());
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    emf_plus_object(id, 6, &data)
}

/// EMF+ solid brush object (type 1).
pub fn emf_plus_solid_brush(id: u8, color: u32) -> Vec<u8> {
    let mut data = EMF_PLUS_MAGIC.to_le_bytes().to_vec();
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&color.to_le_bytes());
    emf_plus_object(id, 1, &data)
}

/// EMF+ pen object (type 2) with explicit start/end caps.
pub fn emf_plus_pen(
    id: u8,
    color: u32,
    width: f32,
    start_cap: u32,
    end_cap: u32,
) -> Vec<u8> {
    let mut data = EMF_PLUS_MAGIC.to_le_bytes().to_vec();
    data.extend_from_slice(&0u32.to_le_bytes());
    let flags: u32 = if start_cap == end_cap { 0 } else { 0x6 };
    data.extend_from_slice(&flags.to_le_bytes());
    data.extend_from_slice(&2u32.to_le_bytes()); // UnitPixel
    data.extend_from_slice(&width.to_le_bytes());
    if flags != 0 {
        data.extend_from_slice(&start_cap.to_le_bytes());
        data.extend_from_slice(&end_cap.to_le_bytes());
    }
    data.extend_from_slice(&EMF_PLUS_MAGIC.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&color.to_le_bytes());
    emf_plus_object(id, 2, &data)
}

/// EMF+ path object (type 3) from compressed points.
pub fn emf_plus_path(id: u8, points: &[(i16, i16)], closed: bool) -> Vec<u8> {
    let mut data = EMF_PLUS_MAGIC.to_le_bytes().to_vec();
    data.extend_from_slice(&(points.len() as u32).to_le_bytes());
    data.extend_from_slice(&0x4000u32.to_le_bytes()); // compressed points
    for (x, y) in points {
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
    }
    data.push(if closed { 0x80 } else { 0 });
    data.resize(data.len() + points.len().saturating_sub(1), 1);
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    emf_plus_object(id, 3, &data)
}

/// `EmfPlusDrawString`; `format_id == u32::MAX` selects left/top defaults.
pub fn emf_plus_draw_string(
    font_id: u8,
    brush_id: u8,
    format_id: u32,
    text: &str,
    rect: [f32; 4],
) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut data = Vec::new();
    data.extend_from_slice(&u32::from(brush_id).to_le_bytes());
    data.extend_from_slice(&format_id.to_le_bytes());
    data.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for value in rect {
        data.extend_from_slice(&value.to_le_bytes());
    }
    for unit in units {
        data.extend_from_slice(&unit.to_le_bytes());
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    emf_plus_record(0x401c, u16::from(font_id), &data)
}

/// `EmfPlusDrawPath` selecting a path and pen object.
pub fn emf_plus_draw_path(path_id: u8, pen_id: u8) -> Vec<u8> {
    emf_plus_record(
        0x4015,
        u16::from(path_id),
        &u32::from(pen_id).to_le_bytes(),
    )
}

/// `EmfPlusSetWorldTransform` (m11, m12, m21, m22, dx, dy).
pub fn emf_plus_world_transform(matrix: [f32; 6]) -> Vec<u8> {
    emf_plus_record(0x402a, 0, &transform(matrix))
}

/// An EMF+ Only container around `records` inside a classic EMF wrapper.
pub fn emf_plus_only_emf(records: &[Vec<u8>]) -> Vec<u8> {
    emf(&[emf_plus_comment(false, records)])
}

// ── WMF ────────────────────────────────────────────────────────────────────

const PLACEABLE_MAGIC: u32 = 0x9AC6_CDD7;
const META_EXTTEXTOUT: u16 = 0x0A32;
const META_EOF: u16 = 0x0000;

/// WMF record: 3-word header + even-length parameters.
pub fn wmf_rec(function: u16, params: &[u8]) -> Vec<u8> {
    assert_eq!(params.len() % 2, 0);
    let words = (params.len() / 2 + 3) as u32;
    let mut output = Vec::new();
    output.extend_from_slice(&words.to_le_bytes());
    output.extend_from_slice(&function.to_le_bytes());
    output.extend_from_slice(params);
    output
}

fn wmf_i16s(values: &[i16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// Standard `METAHEADER` + `records`; appends `META_EOF`.
pub fn wmf_standard(records: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = records.iter().flatten().copied().collect();
    let words = ((18 + body.len()) / 2) as u32;
    let max = records
        .iter()
        .map(|record| (record.len() / 2) as u32)
        .max()
        .unwrap_or(3);
    let mut output = Vec::new();
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&9u16.to_le_bytes());
    output.extend_from_slice(&0x0300u16.to_le_bytes());
    output.extend_from_slice(&words.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes());
    output.extend_from_slice(&max.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes());
    output.extend_from_slice(&body);
    output.extend_from_slice(&wmf_rec(META_EOF, &[]));
    output
}

/// Placeable WMF wrapper (`PlaceableMetaHeader`, 22 bytes, checksum valid).
pub fn wmf_placeable(records: &[Vec<u8>]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&PLACEABLE_MAGIC.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0i16.to_le_bytes());
    header.extend_from_slice(&0i16.to_le_bytes());
    header.extend_from_slice(&100i16.to_le_bytes());
    header.extend_from_slice(&100i16.to_le_bytes());
    header.extend_from_slice(&1440u16.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    let checksum = header.chunks(2).fold(0u16, |acc, pair| {
        acc ^ u16::from_le_bytes([pair[0], pair[1]])
    });
    header.extend_from_slice(&checksum.to_le_bytes());
    header.extend_from_slice(&wmf_standard(records));
    header
}

/// `META_RECTANGLE` with 16-bit coordinates.
pub fn wmf_rect(rect: [i16; 4]) -> Vec<u8> {
    wmf_rec(0x041B, &wmf_i16s(&rect))
}

/// `META_EXTTEXTOUT`: position, count, options, text, per-char advances.
pub fn wmf_ext_text(position: [i16; 2], text: &[u8], options: u16, dx: &[i16]) -> Vec<u8> {
    let mut params = Vec::new();
    params.extend_from_slice(&wmf_i16s(&position));
    params.extend_from_slice(&(text.len() as u16).to_le_bytes());
    params.extend_from_slice(&options.to_le_bytes());
    params.extend_from_slice(text);
    if text.len() & 1 == 1 {
        params.push(0);
    }
    params.extend_from_slice(&wmf_i16s(dx));
    wmf_rec(META_EXTTEXTOUT, &params)
}

// ── Harness ────────────────────────────────────────────────────────────────

pub struct FixtureStore {
    pub rel_id: String,
    pub bytes: Vec<u8>,
}

impl FixtureStore {
    pub fn new(rel_id: &str, bytes: Vec<u8>) -> Self {
        Self {
            rel_id: rel_id.to_string(),
            bytes,
        }
    }
}

impl AssetStore for FixtureStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::UnsupportedFormat("metafile".into()))
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        if rel_id == self.rel_id {
            Ok(self.bytes.clone())
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

pub fn picture_slide(part: &str, mime: &str) -> pptx_model::Slide {
    let picture: pptx_model::PictureElement = serde_json::from_value(serde_json::json!({
        "x": 0, "y": 0, "width": 9_144_000, "height": 6_858_000,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "imagePath": part,
        "mimeType": mime
    }))
    .expect("picture element");
    pptx_model::Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: None,
        background: None,
        elements: vec![pptx_model::SlideElement::Picture(picture)],
        element_sources: vec![pptx_model::SlideElementSource {
            origin: pptx_model::SlideElementOrigin::Slide,
        }],
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    }
}

pub fn compile_slide(slide: &pptx_model::Slide, warnings: &Sender<WarningEvent>) -> VectorSlide {
    DefaultSlideCompiler::new()
        .compile(slide, 9_144_000, 6_858_000, warnings)
        .expect("compile")
}

pub fn markers(scene: &VectorSlide) -> usize {
    scene
        .commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawMetafile { .. }))
        .count()
}

pub fn text_commands(scene: &VectorSlide) -> Vec<(String, String)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(
                runs.iter()
                    .map(|run| (run.font_family.clone(), run.text.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

pub fn dest_rect() -> RectF32 {
    RectF32 {
        x: 0.0,
        y: 0.0,
        w: 9_144_000.0,
        h: 6_858_000.0,
    }
}
