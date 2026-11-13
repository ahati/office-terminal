//! SVG reflection parity with the raster compositor.
//!
//! The mirrored `<use>` carries `R(dir - 90°) · diag(sx, -sy)` about the `algn`
//! pivot (ref packages/core/src/shape/effects.ts:626-639); `dist` is an
//! `<feOffset>` along `dir`, and the mask's alpha table rebuilds the
//! `stA`/`stPos` → `endA`/`endPos` band (ref effects.ts:612-622).

use kitty_pptx_core::*;

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn slide_with(reflection: VectorReflection) -> VectorSlide {
    let mut slide = VectorSlide::new(2_000_000, 1_000_000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));
    slide.commands.push(VectorCommand::DrawPath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 100_000.0, y: 100_000.0 },
                PathSegment::LineTo { x: 900_000.0, y: 100_000.0 },
                PathSegment::LineTo { x: 900_000.0, y: 500_000.0 },
                PathSegment::LineTo { x: 100_000.0, y: 500_000.0 },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])),
        stroke: None,
        transform: AffineTransform::default(),
        effects: Box::new(ShapeEffects {
            reflection: Some(reflection),
            ..Default::default()
        }),
    });
    slide
}

fn svg_for(reflection: VectorReflection) -> String {
    DefaultSlideSvgSerializer::new()
        .serialize_svg(
            &slide_with(reflection),
            &NoAssets,
            SvgTextMode::SelectableText,
        )
        .expect("serialize svg")
}

fn attr(tag: &str, key: &str) -> String {
    let needle = format!("{key}=\"");
    let start = tag.find(&needle).unwrap_or_else(|| panic!("missing {key} in {tag}")) + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"').expect("unterminated attribute");
    rest[..end].to_string()
}

/// Start tag of the `<filter>`/`<feOffset>`/`<g>` whose id/name starts at the
/// first occurrence of `needle`.
fn tag_containing<'a>(svg: &'a str, needle: &str) -> &'a str {
    let start = svg.find(needle).unwrap_or_else(|| panic!("missing {needle}"));
    let end = svg[start..].find('>').expect("unterminated tag") + start;
    &svg[start..=end]
}

fn reflection_filter(svg: &str) -> &str {
    tag_containing(svg, r#"<filter id="refl-fx-0""#)
}

fn reflection_region(svg: &str) -> (f32, f32, f32, f32) {
    let tag = reflection_filter(svg);
    (
        attr(tag, "x").parse().expect("x"),
        attr(tag, "y").parse().expect("y"),
        attr(tag, "width").parse().expect("width"),
        attr(tag, "height").parse().expect("height"),
    )
}

fn fe_offset(svg: &str) -> (f32, f32) {
    let tag = tag_containing(svg, "<feOffset");
    (
        attr(tag, "dx").parse().expect("dx"),
        attr(tag, "dy").parse().expect("dy"),
    )
}

fn table_values(svg: &str) -> Vec<f32> {
    let tag = tag_containing(svg, "<feFuncA");
    attr(tag, "tableValues")
        .split_whitespace()
        .map(|v| v.parse().expect("alpha sample"))
        .collect()
}

fn reflection_transform(svg: &str) -> String {
    let group = svg
        .find(r#"<g transform="translate"#)
        .map(|start| &svg[start..])
        .expect("reflection mirror group");
    attr(group, "transform")
}

fn refl(
    st_a: f32,
    st_pos: f32,
    end_a: f32,
    end_pos: f32,
    dist: i64,
    dir: f64,
) -> VectorReflection {
    VectorReflection {
        start_opacity: st_a,
        start_pos: st_pos,
        end_opacity: end_a,
        end_pos,
        distance_emu: dist,
        direction_deg: dir,
        ..VectorReflection::default()
    }
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.005,
        "{what}: expected {expected}, got {actual}"
    );
}

#[test]
fn bare_reflection_uses_reference_defaults() {
    // ref effects.ts:553-559 / 612-639 — no attributes means dir=90,
    // sx=1, sy=-1 (the mirror), stA=1, endA=0, stPos=0, endPos=1.
    let svg = svg_for(VectorReflection::default());
    let transform = reflection_transform(&svg);

    assert!(transform.contains("rotate(0)"), "dir 90 ⇒ dir-90 = 0: {transform}");
    assert!(transform.contains("scale(1, -1)"), "default sx/sy: {transform}");
    // Bottom-centre `algn` pivot of the 100k..900k × 100k..500k body.
    assert_eq!(transform, "translate(500000, 500000) rotate(0) scale(1, -1) translate(-500000, -500000)");

    let values = table_values(&svg);
    assert_eq!(values.len(), 101);
    close(values[0], 1.0, "stA at band start");
    close(values[50], 0.5, "linear ramp midpoint");
    close(values[100], 0.0, "endA at band end");
}

#[test]
fn direction_90_and_0_emit_rotation_and_mirror_scale() {
    let below = svg_for(refl(1.0, 0.0, 0.0, 1.0, 0, 90.0));
    assert!(reflection_transform(&below).contains("rotate(0)"));

    // dir 0 turns the mirror a quarter turn to the right.
    let right = svg_for(refl(1.0, 0.0, 0.0, 1.0, 0, 0.0));
    let transform = reflection_transform(&right);
    assert!(transform.contains("rotate(-90)"), "dir 0 ⇒ dir-90 = -90: {transform}");
    assert!(transform.contains("scale(1, -1)"));

    // sx/sy are magnitudes; the SVG scale applies the mirror sign to sy.
    let mut scaled = refl(1.0, 0.0, 0.0, 1.0, 0, 90.0);
    scaled.scale_x = 1.5;
    scaled.scale_y = 0.5;
    let transform = reflection_transform(&svg_for(scaled));
    assert!(transform.contains("scale(1.50, -0.50)"), "authored sx/sy: {transform}");
}

#[test]
fn alpha_table_reflects_authored_band() {
    // stA=0.8 → endA=0.2, stA holds to stPos=25 %, endA holds from 75 %.
    let svg = svg_for(refl(0.8, 0.25, 0.2, 0.75, 0, 90.0));
    let values = table_values(&svg);

    close(values[0], 0.8, "stA at t=0");
    close(values[25], 0.8, "stA plateau up to stPos");
    close(values[50], 0.5, "mid-ramp at t=0.5");
    close(values[75], 0.2, "endA plateau from endPos");
    close(values[100], 0.2, "endA at t=1");

    // The mask gradient is a pure position ramp; the table above turns it into
    // the authored alpha band.
    assert!(svg.contains(
        r#"<linearGradient id="refl-grad-0" gradientUnits="userSpaceOnUse" x1="0" y1="1" x2="0" y2="0">"#
    ));
    assert!(svg.contains(r#"stop-opacity="0""#));
    assert!(svg.contains(r#"stop-opacity="1""#));
}

#[test]
fn distance_offsets_the_mirrored_group_along_direction() {
    // dir 90 ⇒ the gap opens downward; dir 0 ⇒ to the right. The offset lives
    // in the outer filter, i.e. in slide space after the mirror transform.
    let down = svg_for(refl(1.0, 0.0, 0.0, 1.0, 200_000, 90.0));
    assert_eq!(fe_offset(&down), (0.0, 200_000.0));

    let right = svg_for(refl(1.0, 0.0, 0.0, 1.0, 200_000, 0.0));
    assert_eq!(fe_offset(&right), (200_000.0, 0.0));

    // A diagonal `dir` offsets both axes with the same magnitude.
    let diagonal = svg_for(refl(1.0, 0.0, 0.0, 1.0, 100_000, 45.0));
    let (dx, dy) = fe_offset(&diagonal);
    close(dx, 70_710.68, "dist·cos(45°)");
    close(dy, 70_710.68, "dist·sin(45°)");
    assert!(reflection_transform(&diagonal).contains("rotate(-45)"));
}

#[test]
fn filter_region_grows_with_blur_and_distance() {
    let base = reflection_region(&svg_for(VectorReflection::default()));

    let blurred = reflection_region(&svg_for(VectorReflection {
        blur_radius_emu: 100_000,
        ..VectorReflection::default()
    }));
    assert!(
        blurred.2 > base.2 + 500_000.0,
        "3σ blur must widen the region: base {} -> {}",
        base.2,
        blurred.2
    );
    assert!(blurred.3 > base.3, "blur must heighten the region");

    let shifted = reflection_region(&svg_for(refl(1.0, 0.0, 0.0, 1.0, 200_000, 90.0)));
    assert!(
        shifted.3 > base.3,
        "dist must extend the region: base {} -> {}",
        base.3,
        shifted.3
    );
    assert!(shifted.1 <= base.1, "downward dist also extends the bottom");
}

#[test]
fn reflection_svg_parses_with_mirror_blur_and_offset() {
    let mut r = refl(0.9, 0.1, 0.0, 0.8, 50_000, 0.0);
    r.blur_radius_emu = 20_000;
    r.scale_x = 1.2;
    r.scale_y = 0.6;
    let svg = svg_for(r);

    assert!(svg.contains(r#"<mask id="refl-mask-0""#));
    assert!(svg.contains(r##"<use href="#shape-0""##));
    assert!(svg.contains(r#"filter="url(#refl-blur-0)""#));

    let options = resvg::usvg::Options::default();
    resvg::usvg::Tree::from_str(&svg, &options).expect("reflection SVG must parse");
}

