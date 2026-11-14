use crossbeam_channel::Sender;
use pptx_model::{Slide, SlideElement, TableElement, TextBody, TextRun, TextRunData};

use super::slide_nav::{looks_like_slide_part, resolve_internal_slide};
use crate::contracts::{
    sanitize_hyperlink_url, FidelityClass, HyperlinkTarget, RectF32, SecurityError, SlideAction,
    SlideLink, WarningEvent,
};

/// Extract every hyperlink reachable from a slide model into vector IR links.
///
/// `deck` supplies the slide order needed to turn internal part-name jumps into
/// 0-based slide indices. Pass an empty slice when no deck context exists: all
/// external links are still extracted, internal jumps are dropped.
pub fn extract_slide_hyperlinks(
    slide: &Slide,
    deck: &[Slide],
    warning_sink: &Sender<WarningEvent>,
) -> Vec<SlideLink> {
    let source_part = slide
        .part_name
        .as_deref()
        .or_else(|| deck.get(slide.index).and_then(|s| s.part_name.as_deref()))
        .unwrap_or("ppt/slides/slide1.xml");

    let mut links = Vec::new();
    for element in &slide.elements {
        match element {
            SlideElement::Shape(shape) => {
                let bounds = RectF32::new(
                    shape.x as f32,
                    shape.y as f32,
                    shape.width as f32,
                    shape.height as f32,
                );
                let element_id = shape
                    .id
                    .clone()
                    .or_else(|| shape.name.clone())
                    .unwrap_or_else(|| "shape".to_string());

                if let Some(target) = resolve_hyperlink(
                    shape.hyperlink.as_deref(),
                    shape.hyperlink_action.as_deref(),
                    source_part,
                    deck,
                    &element_id,
                    warning_sink,
                ) {
                    links.push(SlideLink::new(target, bounds, None));
                }

                if let Some(ref text_body) = shape.text_body {
                    let text_rect = shape.text_rect.as_ref().map_or(bounds, |tr| {
                        RectF32::new(
                            tr.x as f32,
                            tr.y as f32,
                            tr.width as f32,
                            tr.height as f32,
                        )
                    });
                    collect_text_links(
                        text_body,
                        text_rect,
                        &element_id,
                        source_part,
                        deck,
                        warning_sink,
                        &mut links,
                    );
                }
            }
            SlideElement::Table(table) => {
                collect_table_links(table, source_part, deck, warning_sink, &mut links);
            }
            SlideElement::Picture(_) | SlideElement::Chart(_) | SlideElement::Media(_) => {}
        }
    }

    dedupe_links(links)
}

/// Extract hyperlinks for every slide in presentation order.
pub fn extract_deck_hyperlinks(deck: &[Slide]) -> Vec<Vec<SlideLink>> {
    let (tx, _rx) = crossbeam_channel::unbounded();
    deck.iter()
        .map(|slide| extract_slide_hyperlinks(slide, deck, &tx))
        .collect()
}

fn collect_table_links(
    table: &TableElement,
    source_part: &str,
    deck: &[Slide],
    warning_sink: &Sender<WarningEvent>,
    out: &mut Vec<SlideLink>,
) {
    let element_id = table.id.clone().unwrap_or_else(|| "table".to_string());
    let mut y = table.y as f32;
    let total_width = table.width.max(1) as f32;
    for row in &table.rows {
        let row_height = row.height.max(1) as f32;
        let mut x = table.x as f32;
        let default_width = total_width / row.cells.len().max(1) as f32;
        for (column, cell) in row.cells.iter().enumerate() {
            let cell_width = table
                .cols
                .get(column)
                .copied()
                .map(|width| width as f32)
                .unwrap_or(default_width);
            let cell_rect = RectF32::new(x, y, cell_width.max(1.0), row_height);
            if let Some(ref text_body) = cell.text_body {
                collect_text_links(
                    text_body,
                    cell_rect,
                    &element_id,
                    source_part,
                    deck,
                    warning_sink,
                    out,
                );
            }
            x += cell_width;
        }
        y += row_height;
    }
}

fn collect_text_links(
    text_body: &TextBody,
    clip: RectF32,
    element_id: &str,
    source_part: &str,
    deck: &[Slide],
    warning_sink: &Sender<WarningEvent>,
    out: &mut Vec<SlideLink>,
) {
    let scale = text_body.font_scale.unwrap_or(1.0).max(0.01) as f32;
    let left = clip.x + text_body.l_ins.max(0) as f32;
    let top = clip.y + text_body.t_ins.max(0) as f32;
    let mut y = top;

    for paragraph in &text_body.paragraphs {
        let default_size = paragraph
            .def_font_size
            .or(text_body.default_font_size)
            .unwrap_or(18.0) as f32;
        let line_height = default_size * 12700.0 * 1.2 * scale;
        let space_before = paragraph
            .space_before
            .map(|points| (points as f32 / 100.0) * 12700.0)
            .unwrap_or(0.0);
        let mut line_top = y + space_before;
        let mut x = left + paragraph.mar_l.max(0) as f32;
        let mut line_height = line_height;

        for run in &paragraph.runs {
            match run {
                TextRun::Text(data) => {
                    let size_pt = data
                        .font_size
                        .map(|size| size as f32)
                        .unwrap_or(default_size)
                        * scale;
                    let size_emu = size_pt * 12700.0;
                    line_height = line_height.max(size_emu * 1.2);
                    let width = estimate_text_width(&data.text, size_emu);

                    if let Some(target) = run_hyperlink(
                        data,
                        source_part,
                        deck,
                        element_id,
                        warning_sink,
                    ) {
                        let rect = clamp_rect(
                            RectF32::new(x, line_top, width.max(1.0), line_height),
                            &clip,
                        );
                        out.push(SlideLink::new(target, rect, None));
                    }
                    x += width;
                }
                TextRun::Break { .. } => {
                    line_top += line_height;
                    x = left + paragraph.mar_l.max(0) as f32;
                    line_height = default_size * 12700.0 * 1.2 * scale;
                }
                _ => {}
            }
        }
        y = line_top + line_height;
    }
}

fn run_hyperlink(
    run: &TextRunData,
    source_part: &str,
    deck: &[Slide],
    element_id: &str,
    warning_sink: &Sender<WarningEvent>,
) -> Option<HyperlinkTarget> {
    resolve_hyperlink(
        run.hyperlink.as_deref(),
        run.hyperlink_action.as_deref(),
        source_part,
        deck,
        element_id,
        warning_sink,
    )
}

/// Resolve a model hyperlink string + action verb into a semantic target.
pub fn resolve_hyperlink(
    raw_target: Option<&str>,
    raw_action: Option<&str>,
    source_part: &str,
    deck: &[Slide],
    element_id: &str,
    warning_sink: &Sender<WarningEvent>,
) -> Option<HyperlinkTarget> {
    if let Some(action) = raw_action.filter(|action| !action.is_empty()) {
        if action.starts_with("ppaction://") {
            return resolve_action(action, raw_target, source_part, deck, element_id, warning_sink);
        }
    }

    let raw_target = raw_target.filter(|target| !target.trim().is_empty())?;
    if let Some(index) = resolve_internal_slide(raw_target, source_part, deck) {
        return Some(HyperlinkTarget::SlideIndex(index));
    }
    // Without deck context a slide-part-looking target cannot be resolved;
    // drop it silently instead of reporting it as an invalid URL.
    if deck.is_empty() && looks_like_slide_part(raw_target) {
        return None;
    }

    match classify_external_target(raw_target) {
        Ok(Some(url)) => Some(HyperlinkTarget::ExternalUri(url)),
        // A non-navigable scheme (or a schemeless string) is INTERNAL in the
        // reference classifier; it names no slide part here, so the click is a
        // no-op rather than a rejected URL. ref hyperlink.ts:75-90
        Ok(None) => None,
        Err(error) => {
            warn(warning_sink, element_id, "hyperlink_url_rejected");
            let _ = error;
            None
        }
    }
}

/// Classify a target that is not a slide part.
///
/// `Ok(Some(url))` is an external URI whose scheme is in the navigable
/// allowlist (`http`, `https`, `mailto`, `tel`); `Ok(None)` is an internal
/// reference (bare part name or non-navigable scheme), matching
/// `classifyPptxHyperlink`'s external-vs-internal boundary exactly.
/// ref packages/pptx/src/hyperlink.ts:60-90
fn classify_external_target(raw_target: &str) -> Result<Option<String>, SecurityError> {
    match sanitize_hyperlink_url(raw_target) {
        Ok(url) => Ok(Some(url)),
        // `tel:` is navigable in the reference allowlist
        // (`DEFAULT_ALLOWED_HYPERLINK_SCHEMES`). `sanitize_hyperlink_url` has
        // already enforced the shared length / control-character / scheme
        // guards before it reaches this arm.
        Err(SecurityError::UnsupportedScheme(scheme)) if scheme.eq_ignore_ascii_case("tel") => {
            let rest = raw_target
                .split_once(':')
                .map(|(_, rest)| rest)
                .unwrap_or("");
            if rest.trim().is_empty() {
                return Err(SecurityError::InvalidUri(
                    "tel URI has no number".to_string(),
                ));
            }
            Ok(Some(raw_target.trim().to_string()))
        }
        // Everything else that is not a navigable scheme is internal: the
        // reference never turns it into an external URL and never rejects it.
        Err(SecurityError::UnsupportedScheme(_))
        | Err(SecurityError::ForbiddenScheme(_))
        | Err(SecurityError::InvalidUri(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn resolve_action(
    action: &str,
    raw_target: Option<&str>,
    source_part: &str,
    deck: &[Slide],
    element_id: &str,
    warning_sink: &Sender<WarningEvent>,
) -> Option<HyperlinkTarget> {
    let verb = action
        .trim_start_matches("ppaction://")
        .split('?')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let query = action.split_once('?').map(|(_, query)| query).unwrap_or("");

    match verb.as_str() {
        "hlinksldjump" => raw_target
            .and_then(|target| resolve_internal_slide(target, source_part, deck))
            .map(HyperlinkTarget::SlideIndex)
            .or_else(|| {
                if !deck.is_empty() {
                    warn(warning_sink, element_id, "hyperlink_slide_jump_unresolved");
                }
                None
            }),
        "hlinkshowjump" => {
            let jump = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("jump="))
                .unwrap_or("")
                .to_ascii_lowercase();
            let action = match jump.as_str() {
                "nextslide" => Some(SlideAction::NextSlide),
                "prevslide" | "previousslide" => Some(SlideAction::PreviousSlide),
                "firstslide" => Some(SlideAction::FirstSlide),
                "lastslide" => Some(SlideAction::LastSlide),
                "endshow" => Some(SlideAction::EndShow),
                _ => None,
            };
            if action.is_none() {
                warn(warning_sink, element_id, "hyperlink_action_unsupported");
            }
            action.map(HyperlinkTarget::Action)
        }
        "customshow" => {
            warn(warning_sink, element_id, "hyperlink_customshow_unsupported");
            None
        }
        _ => {
            warn(warning_sink, element_id, "hyperlink_action_unsupported");
            None
        }
    }
}

fn warn(sink: &Sender<WarningEvent>, element_id: &str, feature: &'static str) {
    let _ = sink.send(WarningEvent::FidelityDegraded {
        element_id: element_id.to_string(),
        feature,
        class: FidelityClass::Degraded,
    });
}

fn clamp_rect(rect: RectF32, clip: &RectF32) -> RectF32 {
    let x = rect.x.max(clip.x);
    let y = rect.y.max(clip.y);
    let right = rect.right().min(clip.right());
    let bottom = rect.bottom().min(clip.bottom());
    if right > x && bottom > y {
        RectF32::new(x, y, right - x, bottom - y)
    } else {
        *clip
    }
}

fn dedupe_links(links: Vec<SlideLink>) -> Vec<SlideLink> {
    let mut unique: Vec<SlideLink> = Vec::with_capacity(links.len());
    for link in links {
        if !unique.iter().any(|existing| {
            existing.target == link.target
                && existing.tooltip == link.tooltip
                && existing.bounds_emu == link.bounds_emu
        }) {
            unique.push(link);
        }
    }
    unique
}

fn estimate_text_width(text: &str, font_size_emu: f32) -> f32 {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_punctuation() || ch == ' ' {
                font_size_emu * 0.3
            } else if ch.is_ascii_digit() || ch.is_ascii_uppercase() {
                font_size_emu * 0.6
            } else if ch.is_ascii_lowercase() {
                font_size_emu * 0.5
            } else {
                font_size_emu
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    fn slide_json(index: usize, part: &str, hyperlink: Option<&str>, action: Option<&str>) -> Slide {
        let mut shape = serde_json::json!({
            "type": "shape",
            "x": 100,
            "y": 200,
            "width": 400,
            "height": 300,
            "rotation": 0.0,
            "flipH": false,
            "flipV": false,
            "geometry": "rect"
        });
        if let Some(link) = hyperlink {
            shape["hyperlink"] = serde_json::Value::String(link.to_string());
        }
        if let Some(action) = action {
            shape["hyperlinkAction"] = serde_json::Value::String(action.to_string());
        }
        serde_json::from_value(serde_json::json!({
            "index": index,
            "slideNumber": index + 1,
            "partName": part,
            "elements": [shape],
            "elementSources": [],
            "comments": []
        }))
        .expect("slide deserializes")
    }

    #[test]
    fn external_link_is_sanitized_and_bounded() {
        let (tx, rx) = unbounded();
        let deck = vec![slide_json(0, "ppt/slides/slide1.xml", Some(" https://example.com "), None)];
        let links = extract_slide_hyperlinks(&deck[0], &deck, &tx);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].target,
            HyperlinkTarget::ExternalUri("https://example.com".into())
        );
        assert_eq!(links[0].bounds_emu, RectF32::new(100.0, 200.0, 400.0, 300.0));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn forbidden_scheme_is_classified_internal_and_dropped() {
        let (tx, rx) = unbounded();
        let deck = vec![slide_json(0, "ppt/slides/slide1.xml", Some("javascript:alert(1)"), None)];
        let links = extract_slide_hyperlinks(&deck[0], &deck, &tx);
        // ref hyperlink.test.ts:46-52: a non-navigable scheme is INTERNAL, not
        // an external URL and not a rejection; no slide part matches it, so no
        // link is emitted and nothing is reported.
        assert!(links.is_empty());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn internal_part_name_maps_to_slide_index() {
        let (tx, _rx) = unbounded();
        let deck = vec![
            slide_json(
                0,
                "ppt/slides/slide1.xml",
                Some("../slides/slide3.xml"),
                Some("ppaction://hlinksldjump"),
            ),
            slide_json(1, "ppt/slides/slide2.xml", None, None),
            slide_json(2, "ppt/slides/slide3.xml", None, None),
        ];
        let links = extract_slide_hyperlinks(&deck[0], &deck, &tx);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, HyperlinkTarget::SlideIndex(2));
    }

    #[test]
    fn show_jump_actions_map_to_semantics() {
        let (tx, _rx) = unbounded();
        let deck = vec![slide_json(
            0,
            "ppt/slides/slide1.xml",
            None,
            Some("ppaction://hlinkshowjump?jump=endshow"),
        )];
        let links = extract_slide_hyperlinks(&deck[0], &deck, &tx);
        assert_eq!(
            links[0].target,
            HyperlinkTarget::Action(SlideAction::EndShow)
        );
    }

    #[test]
    fn text_run_links_use_run_level_bounds_and_dedupe_exact_duplicates() {
        let (tx, _rx) = unbounded();
        let slide: Slide = serde_json::from_str(
            r#"{
                "index": 0,
                "slideNumber": 1,
                "partName": "ppt/slides/slide1.xml",
                "elements": [{
                    "type": "shape",
                    "x": 0, "y": 0, "width": 1000000, "height": 500000,
                    "rotation": 0.0, "flipH": false, "flipV": false,
                    "geometry": "rect",
                    "textBody": {
                        "verticalAnchor": "top", "wrap": "square", "vert": "horz",
                        "autoFit": "none", "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
                        "paragraphs": [{
                            "alignment": "left", "marL": 0, "marR": 0, "indent": 0,
                            "lvl": 0, "bullet": {"type": "none"}, "rtl": false,
                            "eaLnBrk": false, "tabStops": [],
                            "runs": [
                                {"type": "text", "text": "One", "bold": false,
                                 "italic": false, "underline": true, "strikethrough": false,
                                 "strikeDouble": false, "hyperlinkUsesTextFill": false,
                                 "fieldType": null, "hyperlink": "mailto:a@example.com"},
                                {"type": "text", "text": "Two", "bold": false,
                                 "italic": false, "underline": false, "strikethrough": false,
                                 "strikeDouble": false, "hyperlinkUsesTextFill": false,
                                 "fieldType": null, "hyperlink": "mailto:a@example.com"}
                            ]
                        }]
                    }
                }],
                "elementSources": [],
                "comments": []
            }"#,
        )
        .expect("slide deserializes");
        let links = extract_slide_hyperlinks(&slide, std::slice::from_ref(&slide), &tx);
        assert_eq!(links.len(), 2, "different run bounds stay distinct");
        assert!(links[0].bounds_emu.x < links[1].bounds_emu.x);
        assert_eq!(links[0].target, HyperlinkTarget::ExternalUri("mailto:a@example.com".into()));
    }
}
