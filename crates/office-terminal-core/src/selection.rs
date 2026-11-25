//! Element hit-testing and compact element-context projection (V3).
//!
//! Port of `packages/pptx/src/element-selection.ts` (reference `1f5c6e1d`).
//! The renderer's reverse paint-order hit test, inverse rotation/flip frame,
//! line tolerance and bounded UTF-16 text projection are pure model math.

use std::collections::HashMap;

mod chart_context;

pub use chart_context::bounded_chart_context_text;

use pptx_model::{ChartElement, Fill, ShapeElement, Slide, SlideElement, TextBody, TextRun};

/// Bounds and frame of one slide element, in EMU / degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// A point in slide EMU.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlidePoint {
    pub x: f32,
    pub y: f32,
}

/// Bounded text projection plus its accounting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedText {
    pub text: Option<String>,
    pub truncated: bool,
    pub text_characters: usize,
    pub max_text_characters: usize,
}

/// Compact element context returned by a hit test.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementContext {
    pub format: &'static str,
    pub kind: &'static str,
    pub slide_index: usize,
    pub element_index: usize,
    pub origin: &'static str,
    pub element_type: &'static str,
    pub point: SlidePoint,
    pub bounds: ElementBounds,
    pub shape_id: Option<String>,
    pub name: Option<String>,
    pub geometry: Option<String>,
    pub text: Option<String>,
    pub mime_type: Option<String>,
    pub media_kind: Option<String>,
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub series_count: Option<usize>,
    pub truncated: bool,
    pub truncation_reasons: Vec<&'static str>,
    pub text_characters: usize,
    pub max_text_characters: usize,
}

/// Geometry-only lookup by drawing id (comment anchors / primitive UI).
#[derive(Clone, Debug, PartialEq)]
pub struct ElementBoundsEntry {
    pub element_id: String,
    pub element_index: usize,
    pub origin: &'static str,
    pub element_type: &'static str,
    pub bounds: ElementBounds,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementContextOptions {
    pub tolerance: f32,
    pub max_text_characters: Option<usize>,
}

impl Default for ElementContextOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.0,
            max_text_characters: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SelectionError {
    #[error("PPTX hit-test point must contain finite coordinates")]
    NonFinitePoint,
    #[error("tolerance must be a finite non-negative number")]
    InvalidTolerance,
    #[error("maxTextCharacters must be a finite non-negative number")]
    InvalidMaxTextCharacters,
}

pub const MAX_ELEMENT_TEXT_CHARACTERS: usize = 65_536;
pub const DEFAULT_ELEMENT_TEXT_CHARACTERS: usize = 16_384;

fn origin_name(origin: pptx_model::SlideElementOrigin) -> &'static str {
    match origin {
        pptx_model::SlideElementOrigin::Master => "master",
        pptx_model::SlideElementOrigin::Layout => "layout",
        pptx_model::SlideElementOrigin::Slide => "slide",
    }
}

fn element_type_name(element: &SlideElement) -> &'static str {
    match element {
        SlideElement::Shape(_) => "shape",
        SlideElement::Picture(_) => "picture",
        SlideElement::Table(_) => "table",
        SlideElement::Chart(_) => "chart",
        SlideElement::Media(_) => "media",
    }
}

fn element_frame(element: &SlideElement) -> (i64, i64, i64, i64, f64, bool, bool) {
    match element {
        SlideElement::Shape(shape) => (
            shape.x,
            shape.y,
            shape.width,
            shape.height,
            shape.rotation,
            shape.flip_h,
            shape.flip_v,
        ),
        SlideElement::Picture(picture) => (
            picture.x,
            picture.y,
            picture.width,
            picture.height,
            picture.rotation,
            picture.flip_h,
            picture.flip_v,
        ),
        SlideElement::Table(table) => (
            table.x,
            table.y,
            table.width,
            table.height,
            table.rotation,
            table.flip_h,
            table.flip_v,
        ),
        SlideElement::Chart(chart) => (
            chart.x,
            chart.y,
            chart.width,
            chart.height,
            chart.rotation,
            chart.flip_h,
            chart.flip_v,
        ),
        SlideElement::Media(media) => (
            media.x,
            media.y,
            media.width,
            media.height,
            media.rotation,
            media.flip_h,
            media.flip_v,
        ),
    }
}

fn element_id(element: &SlideElement) -> Option<&str> {
    match element {
        SlideElement::Shape(shape) => shape.id.as_deref(),
        SlideElement::Picture(picture) => picture.id.as_deref(),
        SlideElement::Table(table) => table.id.as_deref(),
        SlideElement::Chart(chart) => chart.id.as_deref(),
        SlideElement::Media(media) => media.id.as_deref(),
    }
}

fn bounds_of(frame: (i64, i64, i64, i64, f64, bool, bool)) -> ElementBounds {
    let (x, y, width, height, rotation, flip_h, flip_v) = frame;
    ElementBounds {
        x: x as f32,
        y: y as f32,
        width: width as f32,
        height: height as f32,
        rotation: rotation as f32,
        flip_h,
        flip_v,
    }
}

fn inverse_frame_point(frame: (i64, i64, i64, i64, f64, bool, bool), point: SlidePoint) -> SlidePoint {
    let (x, y, width, height, rotation, flip_h, flip_v) = frame;
    let cx = x as f64 + width as f64 / 2.0;
    let cy = y as f64 + height as f64 / 2.0;
    let radians = (-rotation * std::f64::consts::PI) / 180.0;
    let (sin, cos) = radians.sin_cos();
    let dx = point.x as f64 - cx;
    let dy = point.y as f64 - cy;
    let mut x = cx + cos * dx - sin * dy;
    let mut y = cy + sin * dx + cos * dy;
    if flip_h {
        x = 2.0 * cx - x;
    }
    if flip_v {
        y = 2.0 * cy - y;
    }
    SlidePoint {
        x: x as f32,
        y: y as f32,
    }
}

fn distance_to_segment(point: SlidePoint, start: SlidePoint, end: SlidePoint) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return ((point.x - start.x).powi(2) + (point.y - start.y).powi(2)).sqrt();
    }
    let projection = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    ((point.x - (start.x + projection * dx)).powi(2) + (point.y - (start.y + projection * dy)).powi(2)).sqrt()
}

fn is_line_like(frame: (i64, i64, i64, i64, f64, bool, bool), element: &SlideElement) -> bool {
    let (_, _, width, height, _, _, _) = frame;
    if width == 0 || height == 0 {
        return true;
    }
    match element {
        SlideElement::Shape(shape) => {
            let geometry = shape.geometry.to_ascii_lowercase();
            geometry == "line" || geometry == "straightconnector1"
        }
        _ => false,
    }
}

/// Whether `point` hits `element` in slide EMU. `tolerance` only widens
/// line-like shapes.
pub fn hit_test_slide_element(element: &SlideElement, point: SlidePoint, tolerance: f32) -> bool {
    if !point.x.is_finite() || !point.y.is_finite() {
        return false;
    }
    let frame = element_frame(element);
    let local = inverse_frame_point(frame, point);
    let safe_tolerance = if tolerance.is_finite() && tolerance > 0.0 {
        tolerance
    } else {
        0.0
    };
    if is_line_like(frame, element) {
        let (x, y, width, height, _, _, _) = frame;
        return distance_to_segment(
            local,
            SlidePoint {
                x: x as f32,
                y: y as f32,
            },
            SlidePoint {
                x: (x + width) as f32,
                y: (y + height) as f32,
            },
        ) <= safe_tolerance;
    }
    let (x, y, width, height, _, _, _) = frame;
    let min_x = (x as f32).min((x + width) as f32);
    let max_x = (x as f32).max((x + width) as f32);
    let min_y = (y as f32).min((y + height) as f32);
    let max_y = (y as f32).max((y + height) as f32);
    local.x >= min_x && local.x <= max_x && local.y >= min_y && local.y <= max_y
}

pub(super) fn normalize_text_limit(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(DEFAULT_ELEMENT_TEXT_CHARACTERS)
        .min(MAX_ELEMENT_TEXT_CHARACTERS)
}

/// UTF-16-safe prefix and its code-unit length (`safeUtf16Prefix`).
pub(super) fn utf16_prefix(text: &str, max_units: usize) -> (String, usize) {
    let mut units = 0usize;
    let mut end = 0usize;
    for (index, ch) in text.char_indices() {
        let width = ch.len_utf16();
        if units + width > max_units {
            break;
        }
        units += width;
        end = index + ch.len_utf8();
    }
    (text[..end].to_string(), units)
}

fn text_parts(body: Option<&TextBody>) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    let Some(body) = body else {
        return parts;
    };
    for paragraph in &body.paragraphs {
        let mut begins_part = true;
        for run in &paragraph.runs {
            let text = match run {
                TextRun::Text(data) => data.text.clone(),
                TextRun::Break { .. } => "\n".to_string(),
                TextRun::Math { .. } => "[equation]".to_string(),
            };
            if text.is_empty() {
                continue;
            }
            parts.push((text, begins_part));
            begins_part = false;
        }
    }
    parts
}

fn element_text_parts(element: &SlideElement) -> Vec<(String, bool)> {
    match element {
        SlideElement::Shape(shape) => text_parts(shape.text_body.as_ref()),
        SlideElement::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| text_parts(cell.text_body.as_ref()))
            .collect(),
        _ => Vec::new(),
    }
}

fn bounded_joined_text(parts: Vec<(String, bool)>, max_units: usize) -> BoundedText {
    let mut chunks = Vec::new();
    let mut length = 0usize;
    let mut truncated = false;
    let mut found = false;
    for (part, begins_part) in parts {
        found = true;
        if begins_part && !chunks.is_empty() {
            if length >= max_units {
                truncated = true;
                break;
            }
            chunks.push("\n".to_string());
            length += 1;
        }
        let allowed = max_units.saturating_sub(length);
        let (chunk, units) = utf16_prefix(&part, allowed);
        length += units;
        let complete = chunk.len() == part.len();
        chunks.push(chunk);
        if !complete {
            truncated = true;
            break;
        }
    }
    BoundedText {
        text: found.then(|| chunks.concat()),
        truncated,
        text_characters: length,
        max_text_characters: max_units,
    }
}

/// Topmost element context at `point`; reverse paint order wins.
pub fn hit_test_slide_context(
    slide_index: usize,
    slide: &Slide,
    point: SlidePoint,
    options: ElementContextOptions,
) -> Result<Option<ElementContext>, SelectionError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(SelectionError::NonFinitePoint);
    }
    if !options.tolerance.is_finite() || options.tolerance < 0.0 {
        return Err(SelectionError::InvalidTolerance);
    }
    let max_text = normalize_text_limit(options.max_text_characters);
    for element_index in (0..slide.elements.len()).rev() {
        let element = &slide.elements[element_index];
        if !hit_test_slide_element(element, point, options.tolerance) {
            continue;
        }
        let bounded = match element {
            SlideElement::Chart(chart) => bounded_chart_context_text(&chart.chart, Some(max_text)),
            _ => bounded_joined_text(element_text_parts(element), max_text),
        };
        let frame = element_frame(element);
        let origin = slide
            .element_sources
            .get(element_index)
            .map(|source| origin_name(source.origin))
            .unwrap_or("unknown");
        let (shape_id, name, geometry) = match element {
            SlideElement::Shape(shape) => (shape.id.clone(), shape.name.clone(), Some(shape.geometry.clone())),
            _ => (None, None, None),
        };
        let (mime_type, media_kind) = match element {
            SlideElement::Picture(picture) => (Some(picture.mime_type.clone()), None),
            SlideElement::Media(media) => (Some(media.mime_type.clone()), Some(media.media_kind.clone())),
            _ => (None, None),
        };
        let (row_count, column_count) = match element {
            SlideElement::Table(table) => (Some(table.rows.len()), Some(table.cols.len())),
            _ => (None, None),
        };
        let series_count = match element {
            SlideElement::Chart(chart) => Some(chart.chart.series.len()),
            _ => None,
        };
        return Ok(Some(ElementContext {
            format: "pptx",
            kind: "element",
            slide_index,
            element_index,
            origin,
            element_type: element_type_name(element),
            point,
            bounds: bounds_of(frame),
            shape_id,
            name,
            geometry,
            text: bounded.text,
            mime_type,
            media_kind,
            row_count,
            column_count,
            series_count,
            truncated: bounded.truncated,
            truncation_reasons: if bounded.truncated { vec!["text"] } else { Vec::new() },
            text_characters: bounded.text_characters,
            max_text_characters: max_text,
        }));
    }
    Ok(None)
}

/// Apply a smaller on-demand text budget to a retained context.
pub fn limit_element_context(
    context: ElementContext,
    requested_max_text_characters: usize,
) -> Result<ElementContext, SelectionError> {
    let max_text = requested_max_text_characters.min(MAX_ELEMENT_TEXT_CHARACTERS);
    let (text, newly_truncated) = match context.text.as_deref() {
        Some(source) => {
            let (prefix, units) = utf16_prefix(source, max_text);
            let truncated = units < source.encode_utf16().count();
            (Some(prefix), truncated)
        }
        None => (None, false),
    };
    let text_characters = text.as_deref().map(|value| value.encode_utf16().count()).unwrap_or(0);
    Ok(ElementContext {
        text,
        truncated: context.truncated || newly_truncated,
        truncation_reasons: if context.truncated || newly_truncated {
            vec!["text"]
        } else {
            Vec::new()
        },
        text_characters,
        max_text_characters: max_text,
        ..context
    })
}

/// Geometry-only lookup by drawing id, in request order.
pub fn find_element_bounds_by_ids(slide: &Slide, element_ids: &[String]) -> Vec<ElementBoundsEntry> {
    let requested: std::collections::HashSet<&str> = element_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.is_empty())
        .collect();
    if requested.is_empty() {
        return Vec::new();
    }
    let rank = |origin: &str| match origin {
        "slide" => 3,
        "layout" => 2,
        "master" => 1,
        _ => 0,
    };
    let mut found: HashMap<&str, (usize, ElementBoundsEntry)> = HashMap::new();
    for (element_index, element) in slide.elements.iter().enumerate() {
        let Some(id) = element_id(element) else {
            continue;
        };
        if !requested.contains(id) {
            continue;
        }
        let origin = slide
            .element_sources
            .get(element_index)
            .map(|source| origin_name(source.origin))
            .unwrap_or("unknown");
        let entry = ElementBoundsEntry {
            element_id: id.to_string(),
            element_index,
            origin,
            element_type: element_type_name(element),
            bounds: bounds_of(element_frame(element)),
        };
        let replace = found
            .get(id)
            .is_none_or(|(_, previous)| rank(origin) >= rank(previous.origin));
        if replace {
            found.insert(id, (element_index, entry));
        }
    }
    element_ids
        .iter()
        .filter_map(|id| found.get(id.as_str()).map(|(_, entry)| entry.clone()))
        .collect()
}

/// Chart element convenience used by the test harness.
pub fn chart_context_text(chart: &ChartElement) -> BoundedText {
    bounded_chart_context_text(&chart.chart, None)
}

/// Shape element text convenience used by the test harness.
pub fn shape_text(shape: &ShapeElement) -> Option<String> {
    bounded_joined_text(text_parts(shape.text_body.as_ref()), MAX_ELEMENT_TEXT_CHARACTERS).text
}

/// Background fill of a slide, used by SmartArt contrast helpers.
pub fn slide_background(slide: &Slide) -> Option<&Fill> {
    slide.background.as_ref()
}
