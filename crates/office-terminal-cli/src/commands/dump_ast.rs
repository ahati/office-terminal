//! JSON projection of a compiled `VectorSlide` for `dump --json`.
//!
//! Bitmaps are represented by their SHA-256 by default; `--embed-bitmaps`
//! additionally carries the PNG as base64 (PRD 13 §6.5). The projection is
//! intentionally lossy for renderer-only fields (warp/effects details are only
//! passed through where serde already exists) but keeps geometry, paint,
//! transforms and bitmap metadata deterministic.

use base64::prelude::*;
use office_terminal_core::contracts::{
    DecorationLine, PathSegment, PositionedGlyphRun, RectF32, VectorCommand, VectorImageFill, VectorPaint, VectorPath,
    VectorSlide, VectorStroke,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// One expanded slide in the dump envelope.
pub fn slide_json(scene: &VectorSlide, embed_bitmaps: bool) -> Value {
    json!({
        "widthEmu": scene.width_emu,
        "heightEmu": scene.height_emu,
        "commands": scene
            .commands
            .iter()
            .map(|command| command_json(command, embed_bitmaps))
            .collect::<Vec<_>>(),
    })
}

fn command_json(command: &VectorCommand, embed_bitmaps: bool) -> Value {
    match command {
        VectorCommand::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects,
        } => json!({
            "type": "drawPath",
            "path": path_json(path),
            "fill": fill.as_ref().map(paint_json),
            "stroke": stroke.as_ref().map(stroke_json),
            "transform": transform_json(transform),
            "effects": serde_json::to_value(effects).unwrap_or(Value::Null),
        }),
        VectorCommand::DrawLineEnd {
            path,
            fill,
            stroke,
            transform,
        } => json!({
            "type": "drawLineEnd",
            "path": path_json(path),
            "fill": fill.as_ref().map(paint_json),
            "stroke": stroke.as_ref().map(stroke_json),
            "transform": transform_json(transform),
        }),
        VectorCommand::DrawText { runs, transform } => json!({
            "type": "drawText",
            "runs": runs.iter().map(run_json).collect::<Vec<_>>(),
            "transform": transform_json(transform),
        }),
        VectorCommand::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            effects,
        } => json!({
            "type": "drawImage",
            "relId": rel_id,
            "srcRect": [src_rect.x, src_rect.y, src_rect.w, src_rect.h],
            "destRect": rect_json(dest_rect),
            "opacity": opacity,
            "transform": transform_json(transform),
            "effects": serde_json::to_value(effects).unwrap_or(Value::Null),
        }),
        VectorCommand::DrawMetafile {
            rel_id,
            transform,
            src_rect,
            dest_rect,
            opacity,
            effects,
        } => json!({
            "type": "drawMetafile",
            "relId": rel_id,
            "transform": transform_json(transform),
            "srcRect": [src_rect.x, src_rect.y, src_rect.w, src_rect.h],
            "destRect": rect_json(dest_rect),
            "opacity": opacity,
            "effects": serde_json::to_value(effects).unwrap_or(Value::Null),
        }),
        VectorCommand::DrawBitmap {
            png,
            width,
            height,
            placement,
            opacity,
            sampling,
        } => {
            let mut value = json!({
                "type": "drawBitmap",
                "width": width,
                "height": height,
                "pngSha256": sha256_hex(png),
                "placement": transform_json(placement),
                "opacity": opacity,
                "sampling": sampling_json(*sampling),
            });
            if embed_bitmaps {
                value["pngBase64"] = Value::String(BASE64_STANDARD.encode(png.as_ref()));
            }
            value
        }
        VectorCommand::PushClip(path) => json!({
            "type": "pushClip",
            "path": path_json(path),
        }),
        VectorCommand::PopClip => json!({ "type": "popClip" }),
    }
}

fn path_json(path: &VectorPath) -> Value {
    json!({
        "fillRule": match path.fill_rule {
            office_terminal_core::contracts::FillRule::NonZero => "nonzero",
            office_terminal_core::contracts::FillRule::EvenOdd => "evenOdd",
        },
        "segments": path.segments.iter().map(segment_json).collect::<Vec<_>>(),
    })
}

fn segment_json(segment: &PathSegment) -> Value {
    match *segment {
        PathSegment::MoveTo { x, y } => json!({ "type": "moveTo", "x": x, "y": y }),
        PathSegment::LineTo { x, y } => json!({ "type": "lineTo", "x": x, "y": y }),
        PathSegment::QuadTo { x1, y1, x, y } => {
            json!({ "type": "quadTo", "x1": x1, "y1": y1, "x": x, "y": y })
        }
        PathSegment::CubicTo { x1, y1, x2, y2, x, y } => json!({
            "type": "cubicTo",
            "x1": x1,
            "y1": y1,
            "x2": x2,
            "y2": y2,
            "x": x,
            "y": y,
        }),
        PathSegment::Close => json!({ "type": "close" }),
    }
}

fn paint_json(paint: &VectorPaint) -> Value {
    match paint {
        VectorPaint::Solid(color) => json!({ "type": "solid", "color": color }),
        VectorPaint::LinearGradient(gradient) => json!({
            "type": "linearGradient",
            "angleDeg": gradient.angle_deg,
            "stops": gradient
                .stops
                .iter()
                .map(|(offset, color)| json!([offset, color]))
                .collect::<Vec<_>>(),
        }),
        VectorPaint::RadialGradient(gradient) => json!({
            "type": "radialGradient",
            "angleDeg": gradient.angle_deg,
            "stops": gradient
                .stops
                .iter()
                .map(|(offset, color)| json!([offset, color]))
                .collect::<Vec<_>>(),
        }),
        VectorPaint::Image(fill) => image_fill_json(fill),
        VectorPaint::Pattern(pattern) => json!({
            "type": "pattern",
            "preset": pattern.preset,
            "fg": pattern.fg,
            "bg": pattern.bg,
        }),
        VectorPaint::Texture(texture) => json!({
            "type": "texture",
            "width": texture.width,
            "height": texture.height,
            "repeat": texture.repeat,
            "opacity": texture.opacity,
        }),
    }
}

fn image_fill_json(fill: &VectorImageFill) -> Value {
    json!({
        "type": "image",
        "relId": fill.rel_id,
        "srcRect": [fill.src_rect.x, fill.src_rect.y, fill.src_rect.w, fill.src_rect.h],
        "alpha": fill.alpha,
    })
}

fn stroke_json(stroke: &VectorStroke) -> Value {
    json!({
        "color": stroke.color,
        "widthEmu": stroke.width_emu,
        "dashSegments": stroke.dash_segments,
        "lineCap": format!("{:?}", stroke.line_cap).to_lowercase(),
        "lineJoin": format!("{:?}", stroke.line_join).to_lowercase(),
        "miterLimit": stroke.miter_limit,
        "paint": stroke.paint.as_ref().map(|paint| paint_json(paint)),
    })
}

fn run_json(run: &PositionedGlyphRun) -> Value {
    json!({
        "text": run.text,
        "fontFamily": run.font_family,
        "fontSizePt": run.font_size_pt,
        "color": run.color,
        "bold": run.bold,
        "italic": run.italic,
        "glyphs": run
            .glyphs
            .iter()
            .map(|glyph| json!({
                "glyphId": glyph.glyph_id,
                "x": glyph.x,
                "y": glyph.y,
                "advance": glyph.advance,
            }))
            .collect::<Vec<_>>(),
        "underline": run.decorations.underline.as_ref().map(decoration_json),
        "strike": run.decorations.strike.as_ref().map(decoration_json),
        "reflection": run.reflection.as_ref().map(|r| json!({
            "blurRadiusEmu": r.blur_radius_emu,
            "startOpacity": r.start_opacity,
            "startPos": r.start_pos,
            "endOpacity": r.end_opacity,
            "endPos": r.end_pos,
            "distanceEmu": r.distance_emu,
            "directionDeg": r.direction_deg,
            "scaleX": r.scale_x,
            "scaleY": r.scale_y,
            "align": r.align,
        })),
    })
}

fn decoration_json(line: &DecorationLine) -> Value {
    json!({
        "color": line.color,
        "topEmu": line.top_emu,
        "thicknessEmu": line.thickness_emu,
        "gapEmu": line.gap_emu,
        "style": format!("{:?}", line.style).to_lowercase(),
    })
}

fn rect_json(rect: &RectF32) -> Value {
    json!([rect.x, rect.y, rect.w, rect.h])
}

fn transform_json(transform: &office_terminal_core::contracts::AffineTransform) -> Value {
    json!([
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    ])
}

fn sampling_json(sampling: office_terminal_core::contracts::BitmapSampling) -> &'static str {
    match sampling {
        office_terminal_core::contracts::BitmapSampling::Nearest => "nearest",
        office_terminal_core::contracts::BitmapSampling::Linear => "linear",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use office_terminal_core::contracts::{AffineTransform, BitmapSampling, FillRule, VectorSlide};
    use std::sync::Arc;

    fn bitmap_slide(embed: bool) -> Value {
        let mut scene = VectorSlide::new(100, 100);
        scene.commands.push(VectorCommand::DrawBitmap {
            png: Arc::from([1u8, 2, 3, 4].as_slice()),
            width: 1,
            height: 1,
            placement: AffineTransform::scale(100.0, 100.0),
            opacity: 1.0,
            sampling: BitmapSampling::Nearest,
        });
        slide_json(&scene, embed)
    }

    #[test]
    fn bitmaps_default_to_a_hash_and_embed_on_request() {
        let hashed = bitmap_slide(false);
        let command = &hashed["commands"][0];
        assert_eq!(command["type"], "drawBitmap");
        assert!(command["pngSha256"].as_str().is_some());
        assert!(command.get("pngBase64").is_none());

        let embedded = bitmap_slide(true);
        assert_eq!(
            embedded["commands"][0]["pngBase64"],
            BASE64_STANDARD.encode([1u8, 2, 3, 4])
        );
    }

    #[test]
    fn path_geometry_and_fill_rule_are_projected() {
        let mut scene = VectorSlide::new(100, 100);
        scene.commands.push(VectorCommand::DrawPath {
            path: VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: 10.0, y: 0.0 },
                    PathSegment::Close,
                ],
                FillRule::EvenOdd,
            ),
            fill: Some(VectorPaint::Solid([1, 2, 3, 255])),
            stroke: None,
            transform: AffineTransform::identity(),
            effects: Box::default(),
        });
        let slide = slide_json(&scene, false);
        assert_eq!(slide["commands"][0]["path"]["fillRule"], "evenOdd");
        assert_eq!(slide["commands"][0]["path"]["segments"][2]["type"], "close");
        assert_eq!(slide["commands"][0]["fill"]["color"], json!([1, 2, 3, 255]));
    }
}
