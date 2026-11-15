//! Normalized AST serializer and comparison for metafile tests (PRD 13a §5.1).

use kitty_pptx_core::contracts::{
    AffineTransform, BitmapSampling, DecorationLine, FillRule, PathSegment, PositionedGlyphRun,
    VectorCommand, VectorPaint, VectorPath, VectorSlide, VectorStroke,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn r4(val: f32) -> f64 {
    if !val.is_finite() {
        return 0.0;
    }
    ((val as f64) * 10000.0).round() / 10000.0
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn command_to_ast(cmd: &VectorCommand) -> Value {
    match cmd {
        VectorCommand::DrawPath {
            path,
            fill,
            stroke,
            transform,
            ..
        } => json!({
            "type": "drawPath",
            "path": path_to_ast(path),
            "fill": fill.as_ref().map(paint_to_ast),
            "stroke": stroke.as_ref().map(stroke_to_ast),
            "transform": transform_to_ast(transform),
        }),
        VectorCommand::DrawLineEnd {
            path,
            fill,
            stroke,
            transform,
        } => json!({
            "type": "drawLineEnd",
            "path": path_to_ast(path),
            "fill": fill.as_ref().map(paint_to_ast),
            "stroke": stroke.as_ref().map(stroke_to_ast),
            "transform": transform_to_ast(transform),
        }),
        VectorCommand::DrawText { runs, transform } => json!({
            "type": "drawText",
            "runs": runs.iter().map(run_to_ast).collect::<Vec<_>>(),
            "transform": transform_to_ast(transform),
        }),
        VectorCommand::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            ..
        } => json!({
            "type": "drawImage",
            "relId": rel_id,
            "srcRect": [r4(src_rect.x), r4(src_rect.y), r4(src_rect.w), r4(src_rect.h)],
            "destRect": [r4(dest_rect.x), r4(dest_rect.y), r4(dest_rect.w), r4(dest_rect.h)],
            "opacity": r4(*opacity),
            "transform": transform_to_ast(transform),
        }),
        VectorCommand::DrawMetafile {
            rel_id,
            transform,
            src_rect,
            dest_rect,
            opacity,
            ..
        } => json!({
            "type": "drawMetafile",
            "relId": rel_id,
            "srcRect": [r4(src_rect.x), r4(src_rect.y), r4(src_rect.w), r4(src_rect.h)],
            "destRect": [r4(dest_rect.x), r4(dest_rect.y), r4(dest_rect.w), r4(dest_rect.h)],
            "opacity": r4(*opacity),
            "transform": transform_to_ast(transform),
        }),
        VectorCommand::DrawBitmap {
            png,
            width,
            height,
            placement,
            opacity,
            sampling,
        } => json!({
            "type": "drawBitmap",
            "width": width,
            "height": height,
            "pngSha256": sha256_hex(png),
            "placement": transform_to_ast(placement),
            "opacity": r4(*opacity),
            "sampling": match sampling {
                BitmapSampling::Nearest => "nearest",
                BitmapSampling::Linear => "linear",
            },
        }),
        VectorCommand::PushClip(path) => json!({
            "type": "pushClip",
            "path": path_to_ast(path),
        }),
        VectorCommand::PopClip => json!({ "type": "popClip" }),
    }
}

pub fn path_to_ast(path: &VectorPath) -> Value {
    json!({
        "fillRule": match path.fill_rule {
            FillRule::NonZero => "nonzero",
            FillRule::EvenOdd => "evenOdd",
        },
        "segments": path.segments.iter().map(segment_to_ast).collect::<Vec<_>>(),
    })
}

pub fn segment_to_ast(seg: &PathSegment) -> Value {
    match *seg {
        PathSegment::MoveTo { x, y } => json!({ "type": "moveTo", "x": r4(x), "y": r4(y) }),
        PathSegment::LineTo { x, y } => json!({ "type": "lineTo", "x": r4(x), "y": r4(y) }),
        PathSegment::QuadTo { x1, y1, x, y } => {
            json!({ "type": "quadTo", "x1": r4(x1), "y1": r4(y1), "x": r4(x), "y": r4(y) })
        }
        PathSegment::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => json!({
            "type": "cubicTo",
            "x1": r4(x1), "y1": r4(y1),
            "x2": r4(x2), "y2": r4(y2),
            "x": r4(x), "y": r4(y),
        }),
        PathSegment::Close => json!({ "type": "close" }),
    }
}

pub fn paint_to_ast(paint: &VectorPaint) -> Value {
    match paint {
        VectorPaint::Solid(color) => json!({ "type": "solid", "color": color }),
        VectorPaint::LinearGradient(g) => json!({
            "type": "linearGradient",
            "angleDeg": r4(g.angle_deg),
            "stops": g.stops.iter().map(|(o, c)| json!([r4(*o), c])).collect::<Vec<_>>(),
        }),
        VectorPaint::RadialGradient(g) => json!({
            "type": "radialGradient",
            "angleDeg": r4(g.angle_deg),
            "stops": g.stops.iter().map(|(o, c)| json!([r4(*o), c])).collect::<Vec<_>>(),
        }),
        VectorPaint::Image(fill) => json!({
            "type": "image",
            "relId": fill.rel_id,
            "srcRect": [r4(fill.src_rect.x), r4(fill.src_rect.y), r4(fill.src_rect.w), r4(fill.src_rect.h)],
            "alpha": r4(fill.alpha),
        }),
        VectorPaint::Pattern(p) => json!({
            "type": "pattern",
            "preset": p.preset,
            "fg": p.fg,
            "bg": p.bg,
        }),
    }
}

pub fn stroke_to_ast(stroke: &VectorStroke) -> Value {
    json!({
        "color": stroke.color,
        "widthEmu": stroke.width_emu,
        "dashSegments": stroke.dash_segments.as_ref().map(|segs| segs.iter().map(|v| r4(*v)).collect::<Vec<_>>()),
        "lineCap": format!("{:?}", stroke.line_cap).to_lowercase(),
        "lineJoin": format!("{:?}", stroke.line_join).to_lowercase(),
        "miterLimit": r4(stroke.miter_limit),
        "paint": stroke.paint.as_ref().map(|p| paint_to_ast(p.as_ref())),
    })
}

pub fn run_to_ast(run: &PositionedGlyphRun) -> Value {
    json!({
        "text": run.text,
        "fontFamily": run.font_family,
        "fontSizePt": r4(run.font_size_pt),
        "color": run.color,
        "bold": run.bold,
        "italic": run.italic,
        "glyphs": run.glyphs.iter().map(|g| json!({
            "glyphId": g.glyph_id,
            "x": r4(g.x),
            "y": r4(g.y),
            "advance": r4(g.advance),
        })).collect::<Vec<_>>(),
        "underline": run.decorations.underline.as_ref().map(decoration_to_ast),
        "strike": run.decorations.strike.as_ref().map(decoration_to_ast),
    })
}

pub fn decoration_to_ast(line: &DecorationLine) -> Value {
    json!({
        "color": line.color,
        "topEmu": line.top_emu,
        "thicknessEmu": line.thickness_emu,
        "gapEmu": line.gap_emu,
        "style": format!("{:?}", line.style).to_lowercase(),
    })
}

pub fn transform_to_ast(t: &AffineTransform) -> Value {
    json!([r4(t.sx), r4(t.ky), r4(t.kx), r4(t.sy), r4(t.tx), r4(t.ty)])
}

pub fn scene_to_ast(scene: &VectorSlide) -> Value {
    json!({
        "widthEmu": scene.width_emu,
        "heightEmu": scene.height_emu,
        "commands": scene.commands.iter().map(command_to_ast).collect::<Vec<_>>(),
    })
}

pub fn compare_ast_commands(
    actual: &Value,
    expected: &Value,
    fixture_id: &str,
) -> Result<(), String> {
    let act_cmds = actual["commands"]
        .as_array()
        .ok_or_else(|| "Actual AST missing commands array".to_string())?;
    let exp_cmds = expected["commands"]
        .as_array()
        .ok_or_else(|| "Expected AST missing commands array".to_string())?;

    let max_len = act_cmds.len().max(exp_cmds.len());
    for i in 0..max_len {
        let a = act_cmds.get(i);
        let e = exp_cmds.get(i);
        if a != e {
            return Err(format!(
                "Fixture '{fixture_id}' differs at command #{i}:\n  actual:   {}\n  expected: {}",
                serde_json::to_string(a.unwrap_or(&Value::Null)).unwrap_or_default(),
                serde_json::to_string(e.unwrap_or(&Value::Null)).unwrap_or_default()
            ));
        }
    }
    Ok(())
}
