//! List markers: `<a:buChar>` / `<a:buAutoNum>` / `<a:buBlip>`.
//!
//! Ported 1:1 from the reference renderer, which resolves a paragraph's marker
//! content (label, face, size, colour, picture) once per paragraph in
//! `paragraphBullet` and mutates a per-level auto-number counter while it walks
//! the body:
//!
//! - `renderer.ts:4694-4710` — `paragraphHasRenderableContent`
//! - `renderer.ts:4712-4755` — `resolveBulletLabel` (counter reset semantics)
//! - `renderer.ts:4777-4838` — `paragraphBullet` (size / colour / face)
//! - `renderer.ts:1515-1537` — `pictureBulletSizePt`
//! - `packages/pptx/src/autonum-label.ts` — `formatAutoNum`
//!
//! Placement is owned by the layout (`text::emit` / `text::paint`): the marker
//! hangs in the paragraph's gutter and never joins the line's text run.

use std::collections::HashMap;

use crossbeam_channel::Sender;

use crate::compiler::parse_hex_color;
use crate::compiler::text::{map_bullet_char, resolve_font_family};
use crate::contracts::WarningEvent;

/// The marker a paragraph paints in its gutter.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerLabel {
    /// Marker text (a char bullet's symbol, or the formatted auto-number).
    pub text: String,
    pub family: String,
    /// Marker size in points, after `a:normAutofit` font scale.
    pub size_pt: f32,
    pub color: [u8; 4],
}

/// An `<a:buBlip>` picture marker (§21.1.2.4.2).
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerImage {
    /// Embedded zip path of the bitmap (`b.imagePath`).
    pub rel_id: String,
    /// Marker HEIGHT in points, after the auto-fit font scale. Paint derives
    /// the width from the decoded bitmap's aspect ratio (`renderer.ts:5866`).
    pub size_pt: f32,
}

/// A resolved marker: a glyph label or a picture, never both
/// (`renderer.ts:4839`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marker {
    pub label: Option<MarkerLabel>,
    pub image: Option<MarkerImage>,
}

impl Marker {
    pub fn is_empty(&self) -> bool {
        self.label.is_none() && self.image.is_none()
    }
}

/// Decoded source size of a picture-bullet bitmap, the renderer's
/// `peekCachedBitmap` seam (`renderer.ts:5812-5816`). `None` means the bitmap is
/// not decoded yet, so the marker paints nothing rather than blocking.
pub trait MarkerBitmap {
    /// `(width, height)` in source pixels of the bitmap at `rel_id`.
    fn source_size(&self, rel_id: &str) -> Option<(u32, u32)>;
}

/// Body-level inputs the marker resolver needs.
pub struct MarkerContext<'a> {
    /// `body.defaultFontSize` in points (before the auto-fit scale).
    pub body_default_size_pt: f32,
    /// Fallback face for a label without an authored `buFont`.
    pub body_default_family: &'a str,
    /// The paragraph's default text colour (`paraDefaultColor`).
    pub para_default_color: [u8; 4],
    /// `a:normAutofit@fontScale`.
    pub font_scale: f32,
}

/// True when a paragraph has renderable content — at least one non-empty text
/// run, or an equation run. Line breaks alone are not content
/// (`renderer.ts:4707-4713`). PowerPoint draws no marker for an empty
/// paragraph and does not advance its auto-number counter.
pub fn paragraph_has_renderable_content(para: &pptx_model::Paragraph) -> bool {
    para.runs.iter().any(|run| match run {
        pptx_model::TextRun::Text(data) => !data.text.is_empty(),
        pptx_model::TextRun::Math { .. } => true,
        pptx_model::TextRun::Break { .. } => false,
    })
}

/// Per-level `<a:buAutoNum>` counters (`autoNumCounters`,
/// `renderer.ts:4876`), walked body-sequentially so the same map drives both
/// the horizontal and the stacked path.
#[derive(Debug, Default)]
pub struct AutoNumCounters {
    per_level: HashMap<u32, u32>,
}

impl AutoNumCounters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve the marker *label string* and advance the counter map as a side
    /// effect — the reference's `resolveBulletLabel` (`renderer.ts:4728-4755`).
    ///
    /// `None` = no marker (`none`/`inherit`, or an empty paragraph).
    pub fn resolve_label(&mut self, para: &pptx_model::Paragraph) -> Option<String> {
        let has_content = paragraph_has_renderable_content(para);
        match &para.bullet {
            pptx_model::Bullet::Char { ch, font_family, .. } => {
                // Switching to char bullets resets the auto-number counters.
                self.per_level.clear();
                has_content.then(|| map_bullet_char(ch, font_family.as_deref()))
            }
            pptx_model::Bullet::AutoNum { num_type, start_at, .. } => {
                // An empty paragraph draws no marker and does not advance the
                // counter, so a blank line between items keeps the sequence.
                if !has_content {
                    return None;
                }
                let lvl = para.lvl;
                let counter = match self.per_level.get(&lvl) {
                    Some(previous) => {
                        let next = previous + 1;
                        self.per_level.insert(lvl, next);
                        next
                    }
                    None => {
                        let first = start_at.unwrap_or(1);
                        self.per_level.insert(lvl, first);
                        first
                    }
                };
                Some(format_auto_num(counter, num_type))
            }
            // none / inherit — not a list paragraph; reset the counters.
            _ => {
                self.per_level.clear();
                None
            }
        }
    }
}

/// `formatAutoNum` (`autonum-label.ts:14-60`): format an auto-number label
/// from the counter and the OOXML `ST_TextAutonumberScheme` (§20.1.10.61).
/// Unsupported schemes fall through to `arabicPeriod`.
pub fn format_auto_num(counter: u32, num_type: &str) -> String {
    let arabic = counter.to_string();
    let alpha_lc = if (1..=26).contains(&counter) {
        char::from_u32(0x60 + counter)
            .map(String::from)
            .unwrap_or_else(|| arabic.clone())
    } else {
        arabic.clone()
    };
    let alpha_uc = if (1..=26).contains(&counter) {
        char::from_u32(0x40 + counter)
            .map(String::from)
            .unwrap_or_else(|| arabic.clone())
    } else {
        arabic.clone()
    };
    let roman_lc = to_roman(counter).to_lowercase();
    let roman_uc = to_roman(counter);
    // §20.1.10.61 `arabicDb*` — full-width Arabic digits (U+FF10–U+FF19).
    let arabic_db: String = arabic
        .chars()
        .map(|ch| match ch.to_digit(10) {
            Some(digit) => char::from_u32(0xFF10 + digit).unwrap_or(ch),
            None => ch,
        })
        .collect();

    match num_type {
        "arabicPlain" => arabic,
        "arabicPeriod" => format!("{arabic}."),
        "arabicParenR" => format!("{arabic})"),
        "arabicParenBoth" => format!("({arabic})"),
        "arabicDbPlain" => arabic_db,
        "arabicDbPeriod" => format!("{arabic_db}."),
        "alphaLcPlain" => alpha_lc,
        "alphaLcPeriod" => format!("{alpha_lc}."),
        "alphaLcParenR" => format!("{alpha_lc})"),
        "alphaLcParenBoth" => format!("({alpha_lc})"),
        "alphaUcPlain" => alpha_uc,
        "alphaUcPeriod" => format!("{alpha_uc}."),
        "alphaUcParenR" => format!("{alpha_uc})"),
        "alphaUcParenBoth" => format!("({alpha_uc})"),
        "romanLcPlain" => roman_lc,
        "romanLcPeriod" => format!("{roman_lc}."),
        "romanLcParenR" => format!("{roman_lc})"),
        "romanLcParenBoth" => format!("({roman_lc})"),
        "romanUcPlain" => roman_uc,
        "romanUcPeriod" => format!("{roman_uc}."),
        "romanUcParenR" => format!("{roman_uc})"),
        "romanUcParenBoth" => format!("({roman_uc})"),
        _ => format!("{arabic}."),
    }
}

/// `toRoman` (`autonum-label.ts:62-70`).
fn to_roman(value: u32) -> String {
    const VALUES: [u32; 13] = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
    const SYMBOLS: [&str; 13] = ["M", "CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I"];
    let mut remaining = value;
    let mut out = String::new();
    for (value, symbol) in VALUES.iter().zip(SYMBOLS.iter()) {
        while remaining >= *value {
            out.push_str(symbol);
            remaining -= value;
        }
    }
    out
}

/// `paragraphBullet` (`renderer.ts:4777-4838`): resolve the marker content,
/// face, colour and size for one paragraph. `label` is the string
/// [`AutoNumCounters::resolve_label`] produced (already empty-suppressed).
///
/// An empty paragraph yields an empty marker even for a picture bullet: the
/// label stays `None` and the draw site — not this resolver — gates the image.
pub fn resolve_marker(
    para: &pptx_model::Paragraph,
    label: Option<String>,
    context: &MarkerContext<'_>,
    warning_sink: &Sender<WarningEvent>,
) -> Marker {
    // §21.1.2.4.13: with no buSz* the marker takes the first run's size, not
    // the paragraph default (which oversizes bullets and overflows a hanging
    // gutter calibrated against the run).
    let base_pt = first_run_size_pt(para)
        .or(para.def_font_size.map(|size| size as f32))
        .unwrap_or(context.body_default_size_pt);
    // buClrTx (§21.1.2.4.5): without an explicit `<a:buClr>` the marker
    // inherits the FIRST RUN's colour, not the shape-level default.
    let inherited = first_run_color(para).unwrap_or(context.para_default_color);
    let first_run_family = first_run_family(para);

    match &para.bullet {
        pptx_model::Bullet::Char {
            ch,
            color,
            size_pct,
            size_pts,
            font_family,
        } => {
            // No label = no marker: `resolveBulletLabel` suppressed it (an
            // empty paragraph, or a level with no label).
            let Some(text) = label else {
                return Marker::default();
            };
            let authored = ch.clone();
            let mapped = text != authored;
            let size_pt = marker_size_pt(*size_pts, *size_pct, base_pt, context.font_scale);
            // A symbol-mapped char paints in a generic face so the substituted
            // code point resolves (renderer.ts:4804).
            let family = if mapped {
                "sans-serif".to_string()
            } else {
                resolve_font_family(
                    font_family
                        .as_deref()
                        .or(para.def_font_family.as_deref())
                        .unwrap_or(context.body_default_family),
                    warning_sink,
                )
            };
            Marker {
                label: Some(MarkerLabel {
                    text,
                    family,
                    size_pt,
                    color: color.as_deref().map(parse_hex_color).unwrap_or(inherited),
                }),
                image: None,
            }
        }
        pptx_model::Bullet::AutoNum {
            color,
            size_pct,
            size_pts,
            font_family,
            ..
        } => {
            let Some(text) = label else {
                return Marker::default();
            };
            let size_pt = marker_size_pt(*size_pts, *size_pct, base_pt, context.font_scale);
            let family = resolve_font_family(
                font_family
                    .as_deref()
                    .or(first_run_family.as_deref())
                    .or(para.def_font_family.as_deref())
                    .unwrap_or(context.body_default_family),
                warning_sink,
            );
            Marker {
                label: Some(MarkerLabel {
                    text,
                    family,
                    size_pt,
                    color: color.as_deref().map(parse_hex_color).unwrap_or(inherited),
                }),
                image: None,
            }
        }
        pptx_model::Bullet::Blip {
            image_path,
            size_pct,
            size_pts,
            ..
        } => {
            // §21.1.2.4.2: its height follows the text's em box scaled by
            // buSzPct (§21.1.2.4.9), overridden by an absolute buSzPts
            // (§21.1.2.4.10); it is not a glyph, so it has no label.
            let size_pt = match size_pts {
                Some(pts) => *pts as f32 * context.font_scale,
                None => {
                    let pct = size_pct.unwrap_or(100.0) as f32;
                    base_pt * (pct / 100.0) * context.font_scale
                }
            };
            Marker {
                label: None,
                image: Some(MarkerImage {
                    rel_id: image_path.clone(),
                    size_pt,
                }),
            }
        }
        _ => Marker::default(),
    }
}

/// `b.sizePts * PT_TO_EMU * scale * fontScale` else
/// `bulletBaseSizePx * (b.sizePct / 100)` (`renderer.ts:4795-4799`): an
/// absolute `buSzPts` (§21.1.2.4.10) wins over a co-present `buSzPct`
/// (§21.1.2.4.9). Both are one `EG_TextBulletSize` choice.
fn marker_size_pt(size_pts: Option<f64>, size_pct: Option<f64>, base_pt: f32, font_scale: f32) -> f32 {
    match (size_pts, size_pct) {
        (Some(pts), _) => pts as f32 * font_scale,
        (None, Some(pct)) => base_pt * (pct as f32 / 100.0) * font_scale,
        (None, None) => base_pt * font_scale,
    }
}

fn first_run_size_pt(para: &pptx_model::Paragraph) -> Option<f32> {
    para.runs.iter().find_map(|run| match run {
        pptx_model::TextRun::Text(data) => data.font_size.map(|size| size as f32),
        _ => None,
    })
}

fn first_run_color(para: &pptx_model::Paragraph) -> Option<[u8; 4]> {
    para.runs.iter().find_map(|run| match run {
        pptx_model::TextRun::Text(data) => data.color.as_deref().map(parse_hex_color),
        _ => None,
    })
}

fn first_run_family(para: &pptx_model::Paragraph) -> Option<String> {
    para.runs.iter().find_map(|run| match run {
        pptx_model::TextRun::Text(data) => data.font_family.clone(),
        _ => None,
    })
}
