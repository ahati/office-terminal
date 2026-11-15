//! Stacked strike/underline and paragraph-mark cases (T9).

use super::*;

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct Bar {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 4],
}

fn bars(commands: &[VectorCommand]) -> Vec<Bar> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path,
                fill: Some(kitty_pptx_core::contracts::VectorPaint::Solid(color)),
                stroke: None,
                ..
            } => {
                let pts: Vec<(f32, f32)> = path
                    .segments
                    .iter()
                    .filter_map(|segment| match segment {
                        kitty_pptx_core::contracts::PathSegment::MoveTo { x, y }
                        | kitty_pptx_core::contracts::PathSegment::LineTo { x, y } => {
                            Some((*x, *y))
                        }
                        _ => None,
                    })
                    .collect();
                if pts.len() != 4 {
                    return None;
                }
                let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
                let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
                let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
                let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
                Some(Bar { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y, color: *color })
            }
            _ => None,
        })
        .collect()
}

#[test]
fn stacked_paints_glyph_sized_strikes_and_a_mark() {
    let mut text_body = body("AB", "wordArtVert");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.strikethrough = true;
    }
    let commands = compile(&text_body, 200.0, 200.0);
    let bars = bars(&commands);
    assert_eq!(bars.len(), 3, "two glyphs + the paragraph mark: {bars:?}");
    assert!(bars[0].w > 1.0, "glyph strike has ink width");
    assert!((bars[2].w - 8.0 * PT).abs() < 1.0, "mark is 8 px wide");
}

#[test]
fn stacked_unstruck_paragraph_mark_style_is_honoured() {
    let mut text_body = body("A", "wordArtVert");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.strikethrough = true;
    }
    let mut end = match text_body.paragraphs[0].runs[0].clone() {
        pptx_model::TextRun::Text(run) => run,
        _ => unreachable!(),
    };
    end.strikethrough = false;
    text_body.paragraphs[0].end_run_properties = Some(Box::new(end));
    let commands = compile(&text_body, 200.0, 200.0);
    assert_eq!(bars(&commands).len(), 1, "only the visible glyph is struck");
}

#[test]
fn stacked_underline_uses_one_frame_per_column() {
    let mut text_body = body("A", "wordArtVert");
    let first = match text_body.paragraphs[0].runs[0].clone() {
        pptx_model::TextRun::Text(run) => run,
        _ => unreachable!(),
    };
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.underline = true;
        run.underline_color = Some("FF0000".to_string());
    }
    let mut small = first.clone();
    small.text = "CD".to_string();
    small.font_size = Some(18.0);
    small.underline = true;
    small.underline_color = Some("FF0000".to_string());
    let mut large = first;
    large.text = "B".to_string();
    large.font_size = Some(32.0);
    large.underline = true;
    large.underline_color = Some("FF0000".to_string());
    text_body.paragraphs[0]
        .runs
        .extend([pptx_model::TextRun::Text(large), pptx_model::TextRun::Text(small)]);

    let commands = compile(&text_body, 200.0, 110.0);
    let bars = bars(&commands);
    assert_eq!(bars.len(), 2, "one underline frame per column: {bars:?}");
    let mut widths: Vec<f32> = bars.iter().map(|bar| bar.w / PT).collect();
    widths.sort_by(f32::total_cmp);
    assert!((widths[0] - 1.0).abs() < 0.01);
    assert!((widths[1] - 32.0 * 0.05).abs() < 0.01);
    assert!(bars.iter().all(|bar| bar.color == [255, 0, 0, 255]));
}

#[test]
fn stacked_strikes_follow_the_glyph_frame() {
    let mut text_body = body("(A「", "wordArtVert");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.strikethrough = true;
    }
    let commands = compile(&text_body, 200.0, 200.0);
    let rotated = commands.iter().filter(|command| match command {
        VectorCommand::DrawPath { transform, .. } => {
            transform.kx.abs() > 0.1 || transform.ky.abs() > 0.1
        }
        _ => false,
    });
    assert!(rotated.count() >= 1, "the ASCII bracket strike turns with its frame");
}


#[test]
fn stacked_rtl_paints_glyph_sized_strikes() {
    let mut text_body = body("AB", "wordArtVertRtl");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.strikethrough = true;
    }
    let bars = bars(&compile(&text_body, 200.0, 200.0));
    assert_eq!(bars.len(), 3, "two glyphs + the paragraph mark: {bars:?}");
    assert!(bars.iter().all(|bar| bar.color == [0, 0, 0, 255]));
}

#[test]
fn stacked_rtl_underline_uses_one_frame_per_column() {
    let mut text_body = body("A", "wordArtVertRtl");
    let first = match text_body.paragraphs[0].runs[0].clone() {
        pptx_model::TextRun::Text(run) => run,
        _ => unreachable!(),
    };
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.underline = true;
        run.underline_color = Some("FF0000".to_string());
    }
    let mut small = first.clone();
    small.text = "CD".to_string();
    small.font_size = Some(18.0);
    small.underline = true;
    small.underline_color = Some("FF0000".to_string());
    let mut large = first;
    large.text = "B".to_string();
    large.font_size = Some(32.0);
    large.underline = true;
    large.underline_color = Some("FF0000".to_string());
    text_body.paragraphs[0]
        .runs
        .extend([pptx_model::TextRun::Text(large), pptx_model::TextRun::Text(small)]);

    let bars = bars(&compile(&text_body, 200.0, 110.0));
    assert_eq!(bars.len(), 2, "one underline frame per column: {bars:?}");
    let mut widths: Vec<f32> = bars.iter().map(|bar| bar.w / PT).collect();
    widths.sort_by(f32::total_cmp);
    assert!((widths[0] - 1.0).abs() < 0.01);
    assert!((widths[1] - 32.0 * 0.05).abs() < 0.01);
    assert!(bars.iter().all(|bar| bar.color == [255, 0, 0, 255]));
}
