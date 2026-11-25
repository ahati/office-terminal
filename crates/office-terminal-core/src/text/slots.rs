//! Per-grapheme-cluster font-slot dispatch (`renderer.ts:1930-1985`).
//!
//! A DrawingML run carries three faces: `a:latin` (the run's own face), `a:ea`
//! and `a:cs`. PowerPoint chooses the slot per BASE CHARACTER of a grapheme
//! cluster, so a combining mark, variation selector, ZWJ or other extender stays
//! in its base's font segment and a cluster never straddles two segments (one
//! shaped horizontal glyph). An empty `a:ea`/`a:cs` slot draws in PowerPoint's
//! application default (issue #1627), never in the Latin face.
//!
//! The reference splits its input runs only where `font`, the line-metric share
//! or a kinsoku quote seam changes; the port's `StyleId` interning gives the same
//! coalescing because two clusters with an identical resolved face intern to one
//! `StyleDef`.

use crossbeam_channel::Sender;

use crate::compiler::text::resolve_font_family;
use crate::contracts::WarningEvent;
use crate::text::east_asian_default;
use crate::text::linebreak::grapheme::grapheme_spans;
use crate::text::linebreak::rules::is_cjk_break_char;

use super::emit::{Measurer, StyleDef};
use crate::text::linebreak::BreakItem;

/// The three resolved faces of one authored run.
pub(super) struct SlotFaces {
    /// `a:latin`: the run's own resolved face.
    pub(super) latin: String,
    /// `a:ea`, or the application-default tier for this run's East Asian text.
    pub(super) east_asian: Option<String>,
    /// `a:cs`; `None` keeps the per-character script default.
    pub(super) complex_script: Option<String>,
    /// `COMPLEX_SCRIPT_DEFAULT_FACES`, resolved, in the reference's order.
    cs_defaults: [String; 4],
    /// The run authored (or inherited) an `a:ea` face: only then does an East
    /// Asian-slot symbol leave the Latin face (`renderer.ts:1934`, `#1653`).
    east_asian_authored: bool,
    /// `rPr@lang`, for the East Asian slot-symbol language rules (#1653).
    pub(super) lang: Option<String>,
}

impl SlotFaces {
    /// Resolve the slots of one run (`renderer.ts:1866-1905`).
    ///
    /// `authored_ea` / `authored_cs` are the parser's resolved slot faces;
    /// `latin_authored` reports whether the run or its paragraph named a Latin
    /// face, which the reference passes to `buildFont` as `hasNamedFontFamily`.
    pub(super) fn resolve(
        latin: &str,
        authored_ea: Option<&str>,
        authored_cs: Option<&str>,
        lang: Option<&str>,
        text: &str,
        warning_sink: &Sender<WarningEvent>,
    ) -> Self {
        // An empty ea slot takes PowerPoint's application default, chosen from
        // the run's whole East Asian text before any leading cluster joins the
        // previous seam (`renderer.ts:1868-1875`).
        let east_asian = match authored_ea {
            Some(face) => Some(resolve_font_family(face, warning_sink)),
            None => {
                let ea_text: String = text.chars().filter(|ch| is_cjk_break_char(*ch as u32)).collect();
                east_asian_default::east_asian_default_faces(Some(latin), &ea_text)
                    .first()
                    .map(|face| resolve_font_family(face, warning_sink))
            }
        };
        let mut cs_defaults: [String; 4] =
            std::array::from_fn(|index| east_asian_default::COMPLEX_SCRIPT_DEFAULT_FACES[index].to_string());
        for face in &mut cs_defaults {
            *face = resolve_font_family(face, warning_sink);
        }
        Self {
            latin: latin.to_string(),
            east_asian,
            complex_script: authored_cs.map(|face| resolve_font_family(face, warning_sink)),
            cs_defaults,
            east_asian_authored: authored_ea.is_some(),
            lang: lang.map(str::to_string),
        }
    }

    /// The face one cluster draws in (`renderer.ts:1930-1945`).
    fn face_for(&self, cluster: &str) -> String {
        let Some(ch) = cluster.chars().next() else {
            return self.latin.clone();
        };
        let cp = ch as u32;
        // An East Asian-slot symbol resolves its stack from its own cluster, so
        // the run's other scripts never pick its fallback face.
        let ea_symbol = !is_cjk_break_char(cp)
            && self.east_asian_authored
            && east_asian_default::is_east_asian_slot_symbol(cp, self.lang.as_deref());
        let ea_glyph = ea_symbol || is_cjk_break_char(cp);
        let cs_glyph =
            east_asian_default::is_complex_script_code_point(cp) || east_asian_default::has_indic_cs_glyph(cluster);
        if ea_glyph {
            return self.east_asian.clone().unwrap_or_else(|| self.latin.clone());
        }
        if cs_glyph {
            return self
                .complex_script
                .clone()
                .unwrap_or_else(|| self.cs_defaults[east_asian_default::complex_script_default_index(ch)].clone());
        }
        self.latin.clone()
    }
}

/// Push the run's text as [`BreakItem`]s, split at every tab and at every
/// font-slot boundary. Clusters with the same resolved face stay one item (the
/// reference's `group`, `renderer.ts:1946-1985`).
pub(super) fn push_run_items(
    items: &mut Vec<BreakItem>,
    text: &str,
    base: &StyleDef,
    faces: &SlotFaces,
    measurer: &mut Measurer,
) {
    for (index, part) in text.split('\t').enumerate() {
        if index > 0 {
            items.push(BreakItem::Tab);
        }
        if part.is_empty() {
            continue;
        }
        let mut groups: Vec<(String, String)> = Vec::new();
        for (from, to) in grapheme_spans(part) {
            let cluster = &part[from..to];
            let face = faces.face_for(cluster);
            match groups.last_mut() {
                Some((group, family)) if *family == face => group.push_str(cluster),
                _ => groups.push((cluster.to_string(), face)),
            }
        }
        for (group, family) in groups {
            let mut style = base.clone();
            style.family = family;
            let id = measurer.push(style);
            items.push(BreakItem::Text { text: group, style: id });
        }
    }
}
