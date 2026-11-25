//! Owned, lazily-decoded view of the ECMA-376 preset-shape table.
//!
//! `presets.json` is a verbatim copy of `presetShapeDefinitions.xml`, matching
//! the reference renderer's table (`packages/core/src/shape/preset-geometry/
//! geometry.ts:14`, `PRESETS`). It ships 186 `<a:prstGeom>` definitions; each
//! entry holds the adjust defaults, the guide formulas, and the path command
//! tokens. Parsing runs once per process behind a [`OnceLock`], so evaluation
//! borrows one immutable table.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// One `<a:prstGeom>` definition: adjust defaults, guides, and body paths.
///
/// `adj` and `gd` are `(name, formula)` pairs in declaration order. A formula
/// is a postfix expression consumed by
/// [`super::preset_eval::eval_formula`]; the reference tokenises it once per
/// preset (`preset-geometry/evaluator.ts:53-58`) but the strings are immutable
/// and short, so splitting them per evaluation is equivalent.
#[derive(Debug, Clone, Deserialize)]
pub struct PresetDef {
    /// `<a:avLst>` defaults, e.g. `("adj", "val 16667")`.
    pub adj: Vec<(String, String)>,
    /// `<a:gdLst>` formulas, evaluated in this order.
    pub gd: Vec<(String, String)>,
    /// `<a:pathLst>` body paths.
    pub paths: Vec<PresetPathDef>,
}

/// One `<a:path>` of a preset (ref `preset-geometry/path-executor.ts`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetPathDef {
    /// Authoring width: when set, coordinates are scaled by `w / path.w`.
    /// ref `geometry.ts:283-284` (`toAbsX` / `toAbsY`).
    pub w: Option<f64>,
    /// Authoring height: see [`Self::w`].
    pub h: Option<f64>,
    /// `<a:path fill>` (ST_PathFillMode, ECMA-376 §20.1.10.37). `None` is the
    /// default "norm" fill; `"none"` marks a decorative stroke-only overlay.
    pub fill: Option<String>,
    pub stroke: bool,
    /// `<a:path extrusionOk>`.
    pub extrusion_ok: bool,
    /// Raw command tokens: `["m", x, y]`, `["l", x, y]`, `["c"]`,
    /// `["a", wR, hR, stAng, swAng]`, `["C", x1, y1, x2, y2, x, y]`,
    /// `["Q", x1, y1, x, y]`. Every coordinate is a name or a numeric literal.
    pub cmds: Vec<Vec<String>>,
}

static PRESETS: OnceLock<BTreeMap<String, PresetDef>> = OnceLock::new();

/// The immutable preset table.
///
/// A decode failure would mean the embedded data was corrupted at build time;
/// the unit tests assert it parses, and an empty table degrades to "unknown
/// preset" rather than panicking.
fn presets() -> &'static BTreeMap<String, PresetDef> {
    PRESETS.get_or_init(|| serde_json::from_str(include_str!("presets.json")).unwrap_or_default())
}

/// Look up a preset definition by name.
///
/// Matching is case-insensitive: ref `geometry.ts:hasPreset` lowercases the
/// geometry name before consulting the table.
pub fn preset_def(name: &str) -> Option<&'static PresetDef> {
    let table = presets();
    table.get(name).or_else(|| table.get(&name.to_ascii_lowercase()))
}

/// All preset names in sorted order.
pub fn preset_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| presets().keys().map(String::as_str).collect())
}

/// Whether the table knows a (case-insensitive) preset name.
/// ref `geometry.ts:hasPreset`.
pub fn has_preset(name: &str) -> bool {
    preset_def(name).is_some()
}
