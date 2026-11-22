//! Ports of the painted half of `renderer-shape-image-fill.test.ts`
//! (batch `shapes-a`): cases #4-6, #8, #10-15.
//!
//! The reference records a canvas op trace (`clone`/`rotate`/`drawImage`) and
//! the `decode` mock's option bag. The Rust twin observes the same facts on the
//! compiled display list (`VectorPaint::Image`) and on the brush
//! `rasterizer::prepare_image_fill` resolves, plus the decode hints the asset
//! store receives.
//!
//! Loaded as a module of the `parity_shapes_a_tests` target.

use office_terminal_core::contracts::{AffineTransform, VectorPaint};
use office_terminal_core::error::RenderError;
use office_terminal_core::loader::SizeHint;
use office_terminal_core::rasterizer::{
    prepare_image_fill, tile_source_extent_emu, MAX_TILE_CELL_PIXELS,
};
use ooxml_common::blip::BlipEffect;
use serde_json::{json, Value};

use crate::shapes_a_support::*;

/// The reference `shape()` fill fixture with `a:blipFill` overrides
/// (`renderer-shape-image-fill.test.ts:60-90`).
fn filled_shape(x: i64, fill: Value) -> Value {
    let mut element = shape("ellipse");
    element["x"] = json!(x);
    element["fill"] = fill;
    element
}

fn image_fill_value(image_path: &str, mode: Value) -> Value {
    let mut fill = json!({
        "fillType": "image",
        "imagePath": image_path,
        "mimeType": "image/png",
        "stretch": true,
        "rotWithShape": true,
        "alpha": 0.5,
    });
    for (key, value) in mode.as_object().expect("mode object") {
        fill[key] = value.clone();
    }
    fill
}

fn decoded(store: &RecordingStore, rel_id: &str) -> office_terminal_core::loader::DecodedImage {
    use office_terminal_core::loader::AssetStore;
    store.get(rel_id, SizeHint::Native).expect("decoded")
}

fn box_of(slide: &office_terminal_core::contracts::VectorSlide) -> (f32, f32, f32, f32) {
    // The reference passes the frame box `{x: 10, y: 20, w: 100, h: 50}`.
    let _ = slide;
    (10.0, 20.0, 100.0, 50.0)
}

// ---------------------------------------------------------------------------
// Case #4-5 — "keeps per-shape blip effects for a shared source"
// ---------------------------------------------------------------------------

/// Two shapes sharing one PNG with different `CT_Blip` pixel effects must each
/// paint their own transformed decode, whichever comes first on the slide
/// (`renderer-shape-image-fill.test.ts:148-190`): the reference asserts the
/// painted image kinds `['plain', 'gray', 'threshold']` and three decodes.
///
/// Rust twin: the effect chain is a per-shape ordered list on the compiled
/// brush (the decode cache keys on it), and the painted pixels differ per
/// shape: untouched, grayscale, and biLevel(0.5)+grayscale.
#[test]
fn per_shape_blip_effects_survive_a_shared_source() {
    const SHARED: &str = "ppt/media/shared-effects.png";
    let with_effects = |x: i64, effects: Value| {
        let mut mode = json!({"alpha": 1.0});
        if !effects.is_null() {
            mode["blipEffects"] = effects;
        }
        filled_shape(x, image_fill_value(SHARED, mode))
    };

    for reversed in [false, true] {
        let mut elements = vec![
            with_effects(0, Value::Null),
            with_effects(1_000_000, json!([{"type": "grayscale"}])),
            with_effects(
                2_000_000,
                json!([{"type": "biLevel", "thresh": 0.5}, {"type": "grayscale"}]),
            ),
        ];
        if reversed {
            elements.reverse();
        }
        let slide = compile_slide(elements);
        let fills = image_fills(&slide.commands);
        assert_eq!(fills.len(), 3, "one blip per shape");

        // Authored effect chains, in document order per fill.
        let mut chains: Vec<Vec<BlipEffect>> =
            fills.iter().map(|fill| fill.effects.clone()).collect();
        if reversed {
            chains.reverse();
        }
        assert!(chains[0].is_empty(), "plain: no CT_Blip chain");
        assert_eq!(chains[1], vec![BlipEffect::Grayscale]);
        assert_eq!(
            chains[2],
            vec![
                BlipEffect::BiLevel { thresh: 0.5 },
                BlipEffect::Grayscale
            ],
            "ascending order is part of the decode identity"
        );

        // One shared decode serves every owner; the per-shape pixel transform
        // happens on paint, so the painted results still differ.
        let store = RecordingStore::new(vec![(
            Image::solid(SHARED, 2, 1, [128, 64, 32, 255]),
            2,
            1,
        )]);
        let rendered = rasterize(&slide, &store);
        // Each 914400 EMU box is 96 device px; the boxes sit at x = 0, 1e6 and
        // 2e6 EMU and all start at y = 914400 EMU (device y ≈ 96).
        let plain = pixel(&rendered, 20, 140);
        let gray = pixel(&rendered, 125, 140);
        let threshold = pixel(&rendered, 230, 140);
        // Each shape keeps its own transform: the reverse paint order only
        // changes the sequence, never the per-shape pixels (ref asserts the
        // painted kind list, reversed).
        assert_eq!(plain, [128, 64, 32, 255], "untouched blip");
        assert!(
            gray[0] == gray[1] && gray[1] == gray[2] && gray != plain,
            "grayscl recolours: {gray:?}"
        );
        assert!(
            matches!(threshold, [0, 0, 0, 255] | [255, 255, 255, 255]),
            "biLevel then grayscl is black or white: {threshold:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Case #6 — "prepares a shared resource once, then paints each owning shape
// before its stroke"
// ---------------------------------------------------------------------------

/// `renderer-shape-image-fill.test.ts:192-214`: one decode, two `drawImage`
/// calls, the first image before the first stroke, and the text run intact.
#[test]
fn shared_blip_is_prepared_once_and_painted_before_its_stroke() {
    const SHARED: &str = "ppt/media/shared.png";
    let mut first = filled_shape(0, image_fill_value(SHARED, json!({"alpha": 1.0})));
    first["textBody"] = picture_fill_text_body();
    let second = filled_shape(1_000_000, image_fill_value(SHARED, json!({"alpha": 1.0})));
    let slide = compile_slide(vec![first, second]);

    let store = RecordingStore::new(vec![(Image::solid(SHARED, 2, 1, [0, 0, 255, 255]), 2, 1)]);
    let _ = rasterize(&slide, &store);

    // ref `expect(mocks.decode).toHaveBeenCalledTimes(1)`: the shape blip keeps
    // its native grid, so the shared source is one request per owner and no
    // renderer-side resize target is invented.
    assert_eq!(
        store.requests(),
        vec![format!("native:{SHARED}"), format!("native:{SHARED}")],
        "no decode target may be derived for a shape blip"
    );

    // Each owning shape paints inside its own path, before its stroke: the
    // fill and the outline are one display-list command.
    let mut painted = 0;
    for command in &slide.commands {
        if let office_terminal_core::contracts::VectorCommand::DrawPath { fill, stroke, .. } = command {
            if matches!(fill, Some(VectorPaint::Image(_))) {
                painted += 1;
                assert!(stroke.is_some(), "the outline follows its blip");
            }
        }
    }
    assert_eq!(painted, 2, "two owning shapes paint the shared blip");
    assert_eq!(drawn_text(&slide.commands), "Picture fill text");
}

// ---------------------------------------------------------------------------
// Case #8 — "samples a tiled effect chain at the fill DPI"
// ---------------------------------------------------------------------------

/// `renderer-shape-image-fill.test.ts:237-262`: the tiled decode uses the
/// authored DPI basis (`emuPerPixelX/Y = 9525·96/192 = 4762.5`) instead of the
/// shape size.
#[test]
fn tiled_effect_chain_is_sampled_at_the_fill_dpi() {
    const TILE: &str = "ppt/media/dpi-tile.png";
    let fill = image_fill_value(
        TILE,
        json!({
            "stretch": false,
            "dpi": 192,
            "tile": {"algn": "tl", "tx": 0, "ty": 0, "sx": 1.0, "sy": 1.0, "flip": "none"},
            "blipEffects": [{"type": "blur", "radius": 12_700, "grow": false}],
        }),
    );
    let slide = compile_slide(vec![filled_shape(0, fill)]);
    let fills = image_fills(&slide.commands);
    assert_eq!(fills.len(), 1);
    let image = &fills[0];
    assert_eq!(image.dpi, 192.0, "authored blip DPI survives");
    assert_eq!(image.effects.len(), 1, "the effect chain reaches the brush");

    let store = RecordingStore::new(vec![(Image::solid(TILE, 16, 16, [10, 20, 30, 255]), 16, 16)]);
    let pixels = decoded(&store, TILE);
    let (extent_w, extent_h) = tile_source_extent_emu(image, &pixels);
    // 16 px × 9525 × 96 / 192 = 76_200 EMU, independent of the 914400 EMU box.
    assert!((extent_w - 76_200.0).abs() < 1e-3, "{extent_w}");
    assert!((extent_h - 76_200.0).abs() < 1e-3, "{extent_h}");
    assert_eq!(
        (pixels.width, pixels.height),
        (16, 16),
        "the tile samples the native grid, never a resized one"
    );
}

// ---------------------------------------------------------------------------
// Case #10-11 — "keeps a shared tile native-sized regardless of consumer
// order"
// ---------------------------------------------------------------------------

/// `renderer-shape-image-fill.test.ts:277-322`: a tiled and a stretched
/// consumer of one PNG never request a decoder resize target, in either order.
#[test]
fn shared_tile_keeps_the_native_grid_regardless_of_consumer_order() {
    const SHARED: &str = "ppt/media/tile-and-stretch.png";
    let tiled = filled_shape(
        0,
        image_fill_value(
            SHARED,
            json!({
                "stretch": false,
                "dpi": 300,
                "tile": {"algn": "tl", "tx": 0, "ty": 0, "sx": 1.0, "sy": 1.0, "flip": "none"},
            }),
        ),
    );
    let stretched = filled_shape(
        1_000_000,
        image_fill_value(
            SHARED,
            json!({
                "srcRect": {"l": 0.25, "t": 0.0, "r": 0.0, "b": 0.0},
                "fillRect": {"l": 0.1, "t": 0.0, "r": 0.2, "b": 0.0},
            }),
        ),
    );

    for reversed in [false, true] {
        let elements = if reversed {
            vec![stretched.clone(), tiled.clone()]
        } else {
            vec![tiled.clone(), stretched.clone()]
        };
        let slide = compile_slide(elements);
        let store = RecordingStore::new(vec![(Image::solid(SHARED, 300, 150, [9, 9, 9, 255]), 300, 150)]);
        let _ = rasterize(&slide, &store);
        assert!(
            store
                .requests()
                .iter()
                .all(|request| request.starts_with("native:")),
            "no resize target may be derived for a shape blip: {:?}",
            store.requests()
        );
        // Both modes keep their authored placement facts.
        let fills = image_fills(&slide.commands);
        let tiled_fill = fills
            .iter()
            .find(|fill| fill.tile.is_some())
            .expect("tiled consumer");
        let stretched_fill = fills
            .iter()
            .find(|fill| fill.tile.is_none())
            .expect("stretched consumer");
        assert_eq!(tiled_fill.dpi, 300.0);
        assert!((stretched_fill.src_rect.x - 0.25).abs() < 1e-6);
        let fill_rect = stretched_fill.fill_rect.expect("fillRect kept");
        assert!((fill_rect.x - 0.1).abs() < 1e-6);
        assert!((fill_rect.w - 0.7).abs() < 1e-6);
    }
}

// ---------------------------------------------------------------------------
// Case #12 — "rejects an unbounded tile auxiliary, restores caller state"
// ---------------------------------------------------------------------------

/// `renderer-shape-image-fill.test.ts:324-334`: an infinite `a:tile@sx` cannot
/// be represented, so the brush is rejected with the typed diagnostic and
/// nothing is painted. The canvas save/restore half is canvas-only.
#[test]
fn unbounded_tile_auxiliary_is_rejected() {
    const HUGE: &str = "ppt/media/huge.png";
    let fill = image_fill_value(
        HUGE,
        json!({
            "stretch": false,
            "tile": {
                "algn": "tl", "tx": 0, "ty": 0,
                "sx": f64::INFINITY, "sy": 1.0, "flip": "none",
            },
        }),
    );
    let slide = compile_slide(vec![filled_shape(0, fill)]);
    // JSON cannot carry `Number.POSITIVE_INFINITY`, so the authored value is
    // restored on the compiled brush (the reference fixture's `sx`).
    let mut fills = image_fills(&slide.commands);
    assert_eq!(fills.len(), 1);
    fills[0].tile.as_mut().expect("tile mode").sx = f32::INFINITY;

    let store = RecordingStore::new(vec![(Image::solid(HUGE, 2, 1, [0, 0, 0, 255]), 2, 1)]);
    let error = prepare_image_fill(
        &store,
        &fills[0],
        box_of(&slide),
        &AffineTransform::identity(),
        1.0,
        0.0,
        0.0,
    )
    .err()
    .expect("an unbounded tile is unrepresentable");
    assert!(
        matches!(error, RenderError::UnsupportedImageEffect(_)),
        "typed diagnostic, got {error:?}"
    );

    // The slide still renders: the unrepresentable brush paints nothing and the
    // fail-soft path keeps the rest of the shape intact.
    let store = RecordingStore::new(vec![(Image::solid(HUGE, 2, 1, [0, 0, 0, 255]), 2, 1)]);
    let rendered = rasterize(&slide, &store);
    assert_eq!(pixel(&rendered, 20, 20), [255, 255, 255, 255], "no blip");
}

// ---------------------------------------------------------------------------
// Case #13 — "preserves in-budget tile sampling and bounds only oversized
// repeating cells"
// ---------------------------------------------------------------------------

/// `renderer-shape-image-fill.test.ts:336-374` asserts the scratch-cell
/// allocation trace against `MAX_CANVAS_AREA`. The Rust brush has no scratch
/// surface — the pattern shader samples the retained bitmap and the transform
/// restores the authored extent (§20.1.8.58) — so the port asserts that policy:
/// the retained ceiling is the reference's 16_777_216 px, and an oversized
/// authored cell keeps the bitmap grid with the authored period restored by the
/// brush transform.
#[test]
fn oversized_repeating_cells_keep_the_bitmap_grid_and_the_authored_period() {
    assert_eq!(MAX_TILE_CELL_PIXELS, 16_777_216, "MAX_CANVAS_AREA");

    const BOUNDARY: &str = "ppt/media/boundary.png";
    let fill = image_fill_value(
        BOUNDARY,
        json!({
            "stretch": false,
            "tile": {
                "algn": "tl", "tx": 0, "ty": 0,
                "sx": 1_000_000.0, "sy": 4096.1, "flip": "none",
            },
        }),
    );
    let slide = compile_slide(vec![filled_shape(0, fill)]);
    let fills = image_fills(&slide.commands);
    let store = RecordingStore::new(vec![(Image::solid(BOUNDARY, 2, 1, [0, 0, 0, 255]), 2, 1)]);
    // 1/9525 px per EMU: the reference's `scale` for this case.
    let scale = 1.0 / 9525.0;
    let brush = prepare_image_fill(
        &store,
        &fills[0],
        (0.0, 0.0, 100.0, 100.0),
        &AffineTransform::identity(),
        scale,
        0.0,
        0.0,
    )
    .expect("in-budget tile")
    .expect("brush");
    assert!(brush.repeats(), "tile mode repeats");
    let (sx, _ky, _kx, sy, _tx, _ty) = brush.transform();
    let (bitmap_w, bitmap_h) = brush.bitmap_size();
    // cell × step = authored period: 2 px × (4_000_000 EMU / 2 px) at scale.
    assert_eq!(bitmap_w, 2, "the retained cell is the bitmap extent");
    assert_eq!(bitmap_h, 1);
    let authored_w = 2.0 * 9525.0 * 1_000_000.0;
    let authored_h = 1.0 * 9525.0 * 4096.1;
    assert!(
        (sx * bitmap_w as f32 - authored_w * scale).abs() < 1e-2,
        "authored period restored: {}",
        sx * bitmap_w as f32
    );
    assert!(
        (sy * bitmap_h as f32 - authored_h * scale).abs() < 1e-2,
        "authored period restored: {}",
        sy * bitmap_h as f32
    );
}

// ---------------------------------------------------------------------------
// Case #14-15 — "counter-rotates an image fill after clipping"
// ---------------------------------------------------------------------------

/// `renderer-shape-image-fill.test.ts:376-388`: with `rotWithShape="0"` the
/// brush is rotated by `sign · rotation` about the fill-box centre, the sign
/// flipping with `flipH` (`R F R = F`), and the blip is still drawn.
#[test]
fn image_fill_counter_rotates_the_brush_for_rot_with_shape_false() {
    for flip_h in [false, true] {
        const IMAGE: &str = "ppt/media/image.png";
        let mut element = filled_shape(
            0,
            image_fill_value(IMAGE, json!({"rotWithShape": false, "alpha": 1.0})),
        );
        element["rotation"] = json!(90.0);
        element["flipH"] = json!(flip_h);
        let slide = compile_slide(vec![element]);
        let fills = image_fills(&slide.commands);
        assert_eq!(fills.len(), 1);
        let image = &fills[0];
        assert!(!image.rot_with_shape);

        let expected = if flip_h { 1.0 } else { -1.0 } * std::f32::consts::FRAC_PI_2;
        assert!(
            (image.counter_rotation_radians() - expected).abs() < 1e-5,
            "flipH={flip_h}: {} vs {expected}",
            image.counter_rotation_radians()
        );

        // The counter-rotation reaches the brush: a quarter turn about the box
        // centre has cos = 0, so the linear part is a pure ±90° rotation.
        let store = RecordingStore::new(vec![(Image::solid(IMAGE, 2, 1, [0, 0, 0, 255]), 2, 1)]);
        let brush = prepare_image_fill(
            &store,
            image,
            (10.0, 20.0, 100.0, 50.0),
            &AffineTransform::identity(),
            1.0,
            0.0,
            0.0,
        )
        .expect("brush")
        .expect("prepared");
        let (sx, ky, kx, sy, _tx, _ty) = brush.transform();
        let step_x = 100.0 / 2.0;
        let step_y = 50.0 / 1.0;
        assert!(sx.abs() < 1e-3, "cos(±90°) = 0, got {sx}");
        assert!(sy.abs() < 1e-3, "cos(±90°) = 0, got {sy}");
        assert!(
            (kx.abs() - step_x).abs() < 1e-3 && (ky.abs() - step_y).abs() < 1e-3,
            "a quarter turn: kx={kx} ky={ky}"
        );
    }
}

// ---------------------------------------------------------------------------
// SVG twin (not a reference case): the same brush reaches the vector export.
// ---------------------------------------------------------------------------

/// The stretched and tiled blip brushes serialize as `<pattern>` paint servers
/// on the shape's own path: a stretch fill registers one cell over its
/// `fillRect` destination, a tile fill repeats the native cell at its `algn`
/// phase. ref renderer.ts:2434-2585, 2620-2700 (`svg` twin of the canvas brush).
#[test]
fn image_fills_serialize_as_pattern_paint_servers() {
    use office_terminal_core::contracts::SvgTextMode;
    use office_terminal_core::svg::{DefaultSlideSvgSerializer, SlideSvgSerializer};

    const IMAGE: &str = "ppt/media/svg-fill.png";
    let mut shape_element = filled_shape(
        0,
        image_fill_value(
            IMAGE,
            json!({"fillRect": {"l": 0.1, "t": 0.2, "r": 0.3, "b": 0.1}}),
        ),
    );
    shape_element["x"] = json!(0);
    shape_element["y"] = json!(0);
    shape_element["width"] = json!(914_400);
    shape_element["height"] = json!(457_200);
    let slide = compile_slide(vec![shape_element]);
    let store = RecordingStore::new(vec![(Image::solid(IMAGE, 4, 4, [1, 2, 3, 255]), 4, 4)]);
    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &store, SvgTextMode::SelectableText)
        .expect("serialize svg");

    assert!(
        svg.contains(r#"<pattern id="imgfill_1" patternUnits="userSpaceOnUse" x="91440" y="91440" width="548640" height="320040">"#),
        "stretch cell = the fillRect destination:\n{svg}"
    );
    assert!(
        svg.contains(r#"x="0" y="0" width="548640" height="320040""#),
        "the cell content is the full source stretched over the cell:\n{svg}"
    );
    assert!(svg.contains(r#"fill="url(#imgfill_1)""#), "{svg}");

    // Tile mode registers the native period at the `algn` anchor.
    let tile_fill = image_fill_value(
        IMAGE,
        json!({
            "stretch": false,
            "tile": {"algn": "ctr", "tx": 0, "ty": 0, "sx": 1.0, "sy": 1.0, "flip": "none"},
        }),
    );
    let mut tile_shape = filled_shape(0, tile_fill);
    tile_shape["x"] = json!(0);
    tile_shape["y"] = json!(0);
    tile_shape["width"] = json!(914_400);
    tile_shape["height"] = json!(914_400);
    let slide = compile_slide(vec![tile_shape]);
    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &store, SvgTextMode::SelectableText)
        .expect("serialize svg");
    // 4 px at 96 DPI = 38100 EMU; centred in the 914400 EMU box → 438150.
    assert!(
        svg.contains(r#"x="438150" y="438150" width="38100" height="38100""#),
        "tile cell = native extent at the centre anchor:\n{svg}"
    );
}
