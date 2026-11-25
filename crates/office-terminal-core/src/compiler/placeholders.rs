use super::transform::compute_shape_transform;
use crate::contracts::{
    FillRule, LineCap, LineJoin, NormalizedRect, PathSegment, PositionedGlyph, PositionedGlyphRun, RectF32,
    VectorCommand, VectorPath, VectorStroke, EMU_PER_PT,
};

pub fn compile_chart_placeholder(chart_elem: &pptx_model::ChartElement) -> Vec<VectorCommand> {
    let transform = compute_shape_transform(
        chart_elem.x,
        chart_elem.y,
        chart_elem.width,
        chart_elem.height,
        chart_elem.rotation,
        chart_elem.flip_h,
        chart_elem.flip_v,
    );

    let w = chart_elem.width as f32;
    let h = chart_elem.height as f32;

    let mut commands = Vec::new();

    // 1. Dashed frame
    let frame_path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    let frame_stroke = VectorStroke {
        color: [120, 120, 120, 180],
        width_emu: 19050, // 1.5 pt
        dash_segments: Some(vec![4.0, 3.0]),
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        paint: None,
        ..VectorStroke::default()
    };
    commands.push(VectorCommand::DrawPath {
        path: frame_path,
        fill: None,
        stroke: Some(frame_stroke),
        transform,
        effects: Default::default(),
    });

    // 2. Kind Label + Title Text
    let chart = &chart_elem.chart;
    let title = chart.title.as_deref().unwrap_or("Chart");
    let chart_type = chart.chart_type.clone();

    let header_text = format!("◇ CHART ({})", chart_type);
    let title_text = format!("\"{}\"", title);

    let mut runs = Vec::new();

    // Header run
    runs.push(make_simple_run(
        &header_text,
        14.0,
        [80, 80, 80, 255],
        0.05 * w,
        0.35 * h,
    ));

    // Title run
    runs.push(make_simple_run(
        &title_text,
        12.0,
        [100, 100, 100, 255],
        0.05 * w,
        0.55 * h,
    ));

    // Series summaries
    let mut cur_y = 0.72 * h;
    for (i, series) in chart.series.iter().take(3).enumerate() {
        let series_name = if series.name.is_empty() {
            "Series"
        } else {
            series.name.as_str()
        };
        let series_line = format!("▮ {} ({} points)", series_name, series.values.len());
        let series_color = if i == 0 {
            [40, 100, 200, 255]
        } else if i == 1 {
            [200, 80, 40, 255]
        } else {
            [80, 160, 60, 255]
        };
        runs.push(make_simple_run(&series_line, 10.0, series_color, 0.08 * w, cur_y));
        cur_y += 10.0 * EMU_PER_PT * 1.3;
    }

    commands.push(VectorCommand::DrawText { runs, transform });

    commands
}

pub fn compile_media_placeholder(media_elem: &pptx_model::MediaElement) -> Vec<VectorCommand> {
    let transform = compute_shape_transform(
        media_elem.x,
        media_elem.y,
        media_elem.width,
        media_elem.height,
        media_elem.rotation,
        media_elem.flip_h,
        media_elem.flip_v,
    );

    let w = media_elem.width as f32;
    let h = media_elem.height as f32;

    let mut commands = Vec::new();

    // Decoded poster frame (P4): the bitmap planner sees this DrawImage and
    // the raster/SVG backends paint the poster under the media chrome. The
    // compiler itself never decodes bytes.
    if !media_elem.poster_path.is_empty() {
        commands.push(VectorCommand::DrawImage {
            rel_id: media_elem.poster_path.clone(),
            src_rect: NormalizedRect::default(),
            dest_rect: RectF32::new(0.0, 0.0, w, h),
            opacity: 1.0,
            transform,
            effects: Vec::new(),
        });
    }

    // Dashed frame
    let frame_path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    let frame_stroke = VectorStroke {
        color: [100, 100, 100, 180],
        width_emu: 19050,
        dash_segments: Some(vec![4.0, 3.0]),
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        paint: None,
        ..VectorStroke::default()
    };
    commands.push(VectorCommand::DrawPath {
        path: frame_path,
        fill: None,
        stroke: Some(frame_stroke),
        transform,
        effects: Default::default(),
    });

    let header_text = format!("▶ MEDIA ({})", media_elem.media_kind);
    let runs = vec![make_simple_run(&header_text, 14.0, [80, 80, 80, 255], 0.1 * w, 0.5 * h)];
    commands.push(VectorCommand::DrawText { runs, transform });

    commands
}

/// §3 metafile placeholder: dashed frame, `◇ METAFILE` header and a detail
/// line. Used both by the compiler (unplayable blips) and by
/// `metafile::expand_metafiles` (engine failure/limits).
pub fn compile_metafile_placeholder(
    dest_rect: RectF32,
    transform: crate::contracts::AffineTransform,
    detail: &str,
) -> Vec<VectorCommand> {
    let w = dest_rect.w.max(1.0);
    let h = dest_rect.h.max(1.0);
    let local = crate::contracts::AffineTransform {
        tx: dest_rect.x,
        ty: dest_rect.y,
        ..crate::contracts::AffineTransform::default()
    };
    let placed = transform.post_concat(&local);

    let frame_path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    let frame_stroke = VectorStroke {
        color: [120, 120, 120, 180],
        width_emu: 19050, // 1.5 pt
        dash_segments: Some(vec![4.0, 3.0]),
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        ..VectorStroke::default()
    };
    let mut commands = vec![VectorCommand::DrawPath {
        path: frame_path,
        fill: None,
        stroke: Some(frame_stroke),
        transform: placed,
        effects: Box::default(),
    }];

    let header_pt = (h * 0.06 / EMU_PER_PT).clamp(8.0, 18.0);
    commands.push(VectorCommand::DrawText {
        runs: vec![make_simple_run(
            "◇ METAFILE",
            header_pt,
            [90, 90, 90, 255],
            w * 0.08,
            0.42 * h,
        )],
        transform: placed,
    });

    let detail: String = detail.chars().take(64).collect();
    if !detail.is_empty() {
        let detail_pt = (h * 0.045 / EMU_PER_PT).clamp(7.0, 14.0);
        commands.push(VectorCommand::DrawText {
            runs: vec![make_simple_run(
                &detail,
                detail_pt,
                [130, 130, 130, 255],
                w * 0.08,
                0.62 * h,
            )],
            transform: placed,
        });
    }

    commands
}

fn make_simple_run(text: &str, sz_pt: f32, color: [u8; 4], x_start: f32, y: f32) -> PositionedGlyphRun {
    let sz_emu = sz_pt * EMU_PER_PT;
    let mut glyphs = Vec::new();
    let mut cur_x = x_start;
    for (i, _ch) in text.chars().enumerate() {
        let gw = sz_emu * 0.55;
        glyphs.push(PositionedGlyph {
            glyph_id: i as u16,
            x: cur_x,
            y,
            advance: gw,
            warp_u: None,
        });
        cur_x += gw;
    }
    PositionedGlyphRun {
        text: text.to_string(),
        font_family: "Carlito".to_string(),
        font_size_pt: sz_pt,
        color,
        bold: false,
        italic: false,
        glyphs,
        decorations: Default::default(),
        single_face: false,
        face: None,
        warp: None,
        reflection: None,
    }
}
