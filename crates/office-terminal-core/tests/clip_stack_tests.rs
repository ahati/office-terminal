//! Clip-stack parity tests.
//!
//! The reference renderer clips a picture to its preset/custom silhouette
//! (`/tmp/oooxml/packages/pptx/src/renderer.ts:6985-7036`, `target.clip()`), and
//! the compiler mirrors that with `VectorCommand::PushClip`/`PopClip`
//! (`compiler/pictures.rs:88,132`). These tests pin the behaviour of both
//! backends: rasterizer clip compositing and SVG `<clipPath>` emission.

use std::sync::Arc;

use office_terminal_core::*;

const SLIDE_EMU: i64 = 1_000_000;
const TARGET_PX: u32 = 100;

const BLUE: [u8; 4] = [0, 0, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 200, 0, 255];

/// Asset store serving a fully opaque 4x4 red bitmap.
struct SolidAssetStore;

impl AssetStore for SolidAssetStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        if rel_id == "red.png" {
            Ok(DecodedImage {
                width: 4,
                height: 4,
                pixels: Arc::from([255u8, 0, 0, 255].repeat(16).into_boxed_slice()),
            })
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 1,
            cached_bytes: 64,
        }
    }
}

fn empty_slide() -> VectorSlide {
    let mut slide = VectorSlide::new(SLIDE_EMU, SLIDE_EMU);
    slide.background = Some(VectorBackground::Solid(BLUE));
    slide
}

fn rect_path(x0: f32, y0: f32, x1: f32, y1: f32) -> VectorPath {
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x: x0, y: y0 },
            PathSegment::LineTo { x: x1, y: y0 },
            PathSegment::LineTo { x: x1, y: y1 },
            PathSegment::LineTo { x: x0, y: y1 },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    )
}

fn circle_path(cx: f32, cy: f32, radius: f32) -> VectorPath {
    const STEPS: usize = 128;
    let mut segments = Vec::with_capacity(STEPS + 2);
    for step in 0..STEPS {
        let angle = step as f32 / STEPS as f32 * std::f32::consts::TAU;
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();
        if step == 0 {
            segments.push(PathSegment::MoveTo { x, y });
        } else {
            segments.push(PathSegment::LineTo { x, y });
        }
    }
    segments.push(PathSegment::Close);
    VectorPath::new(segments, FillRule::NonZero)
}

/// Full-canvas red bitmap, the "shaped picture" body.
fn full_image() -> VectorCommand {
    VectorCommand::draw_image(
        "red.png".to_string(),
        NormalizedRect::default(),
        RectF32::new(0.0, 0.0, SLIDE_EMU as f32, SLIDE_EMU as f32),
        1.0,
        AffineTransform::default(),
    )
}

fn solid_rect(path: VectorPath, color: [u8; 4]) -> VectorCommand {
    VectorCommand::draw_path(path, Some(VectorPaint::Solid(color)), None, AffineTransform::default())
}

fn render(slide: &VectorSlide) -> RgbaSlide {
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: TARGET_PX,
        height_px: TARGET_PX,
    };
    let assets = SolidAssetStore;
    rasterizer
        .rasterize(slide, &target, &assets)
        .expect("rasterization failed")
}

fn pixel(slide: &RgbaSlide, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * slide.width + x) * 4) as usize;
    [
        slide.pixels[idx],
        slide.pixels[idx + 1],
        slide.pixels[idx + 2],
        slide.pixels[idx + 3],
    ]
}

fn svg_of(slide: &VectorSlide) -> String {
    let serializer = DefaultSlideSvgSerializer::new();
    let assets = SolidAssetStore;
    serializer
        .serialize_svg(slide, &assets, SvgTextMode::VectorGlyphs)
        .expect("svg serialization failed")
}

// ---------------------------------------------------------------------------
// Rasterizer
// ---------------------------------------------------------------------------

#[test]
fn raster_clip_restricts_picture_and_pop_restores() {
    let mut slide = empty_slide();
    slide
        .commands
        .push(VectorCommand::PushClip(rect_path(0.0, 0.0, 500_000.0, 500_000.0)));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);
    // Drawn after PopClip: must not be clipped by the 0..50px square.
    slide
        .commands
        .push(solid_rect(rect_path(800_000.0, 800_000.0, 950_000.0, 950_000.0), GREEN));

    let rendered = render(&slide);

    // Inside the clip: the bitmap painted.
    assert_eq!(pixel(&rendered, 25, 25), RED, "inside clip must be painted");
    assert_eq!(pixel(&rendered, 49, 49), RED, "clip interior near edge");
    // Outside the clip: background untouched.
    assert_eq!(pixel(&rendered, 75, 25), BLUE, "right of clip untouched");
    assert_eq!(pixel(&rendered, 25, 75), BLUE, "below clip untouched");
    assert_eq!(pixel(&rendered, 75, 75), BLUE, "corner untouched");
    // After PopClip the clip stack is empty again.
    assert_eq!(pixel(&rendered, 85, 85), GREEN, "PopClip restores drawing");
}

#[test]
fn raster_nested_clips_intersect() {
    let mut slide = empty_slide();
    // Left 60% and right 60%: the intersection is only 40..60px.
    slide
        .commands
        .push(VectorCommand::PushClip(rect_path(0.0, 0.0, 600_000.0, 1_000_000.0)));
    slide.commands.push(VectorCommand::PushClip(rect_path(
        400_000.0,
        0.0,
        1_000_000.0,
        1_000_000.0,
    )));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);
    slide.commands.push(VectorCommand::PopClip);

    let rendered = render(&slide);

    assert_eq!(pixel(&rendered, 50, 50), RED, "intersection is painted");
    assert_eq!(pixel(&rendered, 20, 50), BLUE, "left of intersection");
    assert_eq!(pixel(&rendered, 80, 50), BLUE, "right of intersection");
    // Both clips popped: a later draw covers the whole canvas again.
    slide
        .commands
        .push(solid_rect(rect_path(0.0, 0.0, 1_000_000.0, 1_000_000.0), GREEN));
    let rendered = render(&slide);
    assert_eq!(pixel(&rendered, 5, 5), GREEN, "stack fully unwound");
}

#[test]
fn raster_circle_silhouette_clips_corners() {
    let mut slide = empty_slide();
    slide
        .commands
        .push(VectorCommand::PushClip(circle_path(500_000.0, 500_000.0, 400_000.0)));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);

    let rendered = render(&slide);

    assert_eq!(pixel(&rendered, 50, 50), RED, "circle centre painted");
    assert_eq!(pixel(&rendered, 50, 15), RED, "inside the radius");
    assert_eq!(pixel(&rendered, 85, 50), RED, "inside the radius (right)");
    assert_eq!(pixel(&rendered, 5, 5), BLUE, "corner outside the circle");
    assert_eq!(pixel(&rendered, 50, 5), BLUE, "top outside the circle");
    assert_eq!(pixel(&rendered, 95, 95), BLUE, "far corner outside");
}

#[test]
fn raster_empty_clip_path_is_noop() {
    let mut slide = empty_slide();
    slide.commands.push(VectorCommand::PushClip(VectorPath::empty()));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);

    let rendered = render(&slide);

    assert_eq!(pixel(&rendered, 5, 5), RED, "empty clip does not clip");
    assert_eq!(pixel(&rendered, 95, 95), RED, "empty clip does not clip");
}

#[test]
fn raster_unbalanced_pop_does_not_panic() {
    let mut slide = empty_slide();
    slide.commands.push(VectorCommand::PopClip);
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);

    let rendered = render(&slide);
    assert_eq!(pixel(&rendered, 50, 50), RED);
}

#[test]
fn raster_clip_without_pop_stays_active() {
    let mut slide = empty_slide();
    slide
        .commands
        .push(VectorCommand::PushClip(rect_path(0.0, 0.0, 300_000.0, 300_000.0)));
    slide.commands.push(full_image());
    // No PopClip: the clip remains in force for the rest of the slide.

    let rendered = render(&slide);
    assert_eq!(pixel(&rendered, 20, 20), RED);
    assert_eq!(pixel(&rendered, 80, 80), BLUE);
}

#[test]
fn raster_clip_outside_canvas_draws_nothing() {
    let mut slide = empty_slide();
    slide.commands.push(VectorCommand::PushClip(rect_path(
        2_000_000.0,
        2_000_000.0,
        3_000_000.0,
        3_000_000.0,
    )));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);

    let rendered = render(&slide);
    assert_eq!(pixel(&rendered, 50, 50), BLUE, "no intersection with canvas");
}

// ---------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------

#[test]
fn svg_emits_clip_path_def_and_group() {
    let mut slide = empty_slide();
    slide
        .commands
        .push(VectorCommand::PushClip(rect_path(0.0, 0.0, 500_000.0, 500_000.0)));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);

    let svg = svg_of(&slide);

    assert!(
        svg.contains(r#"<clipPath id="clip_1" clipPathUnits="userSpaceOnUse">"#),
        "clipPath def missing:\n{svg}"
    );
    assert!(
        svg.contains(r#"<g clip-path="url(#clip_1)">"#),
        "clip group missing:\n{svg}"
    );
    assert_eq!(svg.matches("</g>").count(), 1, "one group must be closed");
    assert_eq!(svg.matches("<image ").count(), 1, "bitmap still drawn");

    let def_at = svg.find("<clipPath").expect("clipPath");
    let image_at = svg.find("<image ").expect("image");
    assert!(def_at < image_at, "clip def must precede its use");
}

#[test]
fn svg_nested_clips_use_nested_groups() {
    let mut slide = empty_slide();
    slide
        .commands
        .push(VectorCommand::PushClip(rect_path(0.0, 0.0, 600_000.0, 1_000_000.0)));
    slide
        .commands
        .push(VectorCommand::PushClip(circle_path(500_000.0, 500_000.0, 300_000.0)));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);
    slide.commands.push(VectorCommand::PopClip);

    let svg = svg_of(&slide);

    assert_eq!(svg.matches("<clipPath id=").count(), 2);
    assert_eq!(svg.matches(r#"<g clip-path="url(#clip_"#).count(), 2);
    assert_eq!(svg.matches("</g>").count(), 2);
    assert!(svg.contains(r#"url(#clip_1)"#) && svg.contains(r#"url(#clip_2)"#));
    // Nested groups: the second group opens inside the first.
    let first = svg.find(r#"url(#clip_1)"#).expect("clip_1");
    let second = svg.find(r#"url(#clip_2)"#).expect("clip_2");
    assert!(first < second, "clip groups must nest in push order");
}

#[test]
fn svg_empty_clip_path_is_noop() {
    let mut slide = empty_slide();
    slide.commands.push(VectorCommand::PushClip(VectorPath::empty()));
    slide.commands.push(full_image());
    slide.commands.push(VectorCommand::PopClip);

    let svg = svg_of(&slide);

    assert!(!svg.contains("<clipPath"), "empty clip emits no def:\n{svg}");
    assert!(!svg.contains("clip-path"), "empty clip emits no group:\n{svg}");
    assert_eq!(svg.matches("</g>").count(), 0);
    assert!(svg.contains("<image "), "bitmap still drawn");
}

#[test]
fn svg_unbalanced_push_clip_is_closed() {
    let mut slide = empty_slide();
    slide
        .commands
        .push(VectorCommand::PushClip(rect_path(0.0, 0.0, 500_000.0, 500_000.0)));
    slide.commands.push(full_image());
    // No PopClip.

    let svg = svg_of(&slide);

    assert_eq!(svg.matches("<g ").count(), 1);
    assert_eq!(svg.matches("</g>").count(), 1);
    assert!(svg.trim_end().ends_with("</svg>"));
}

#[test]
fn svg_pop_clip_without_push_is_noop() {
    let mut slide = empty_slide();
    slide.commands.push(VectorCommand::PopClip);
    slide.commands.push(full_image());

    let svg = svg_of(&slide);
    assert!(!svg.contains("</g>"), "no stray closing group:\n{svg}");
    assert!(svg.contains("<image "));
}
