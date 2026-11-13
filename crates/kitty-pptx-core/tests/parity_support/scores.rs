//! Per-slide fidelity score storage and the ratchet check.
//!
//! Port of the score bookkeeping in
//! `/tmp/oooxml/packages/pptx/tests/visual/visual.spec.ts:68-86` (`1f5c6e1d`):
//!
//! - scores live in one JSON object keyed by item id (e.g. `"slide-3"`),
//! - values are match percentages rounded to 2 decimal places,
//! - keys are written in sorted order, pretty-printed, with a trailing newline
//!   (`JSON.stringify(ordered, null, 2) + '\n'`, `visual.spec.ts:85`),
//! - read-modify-write keeps the scores of slides that were not re-scored.
//!
//! Deliberate deviations from the reference:
//! - a corrupt/unparseable scores file is a hard error instead of being
//!   silently treated as empty (`readScores`, `visual.spec.ts:71-78`, swallows
//!   the exception). A malformed committed ratchet baseline should be loud;
//! - [`ratchet`] is a pure decision function returning the failure message, so
//!   the driver controls whether the ratchet applies (the reference folds the
//!   check into the per-slide test body at `visual.spec.ts:199-215`).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Fidelity-score ratchet: fail when a slide's match-% drops more than this
/// below the committed score. ref `visual.spec.ts:47` (`RATCHET_DROP_PCT`).
pub const RATCHET_DROP_PCT: f64 = 0.5;

/// Item id → match percentage, kept ordered like the reference's sorted object.
pub type Scores = BTreeMap<String, f64>;

/// `Math.round(pct * 100) / 100`. ref `visual.spec.ts:83` (`writeScore`).
pub fn round2(pct: f64) -> f64 {
    (pct * 100.0).round() / 100.0
}

/// Reads the score map at `path`.
///
/// A missing or empty file is an empty map, matching `readScores`
/// (ref `visual.spec.ts:71-78`). Any other I/O or JSON failure is reported with
/// the offending path; unlike the reference it is never silently ignored.
pub fn load(path: &Path) -> Result<Scores, String> {
    if !path.is_file() {
        return Ok(Scores::new());
    }
    let text = fs::read_to_string(path)
        .map_err(|e| format!("cannot read fidelity scores {}: {e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Scores::new());
    }
    serde_json::from_str(&text)
        .map_err(|e| format!("cannot parse fidelity scores {}: {e}", path.display()))
}

/// Serialises `scores` exactly like `writeScore`
/// (ref `visual.spec.ts:80-85`): keys sorted, 2-space indent, trailing newline.
///
/// `Scores` is a [`BTreeMap`], so iteration (and therefore serialisation) is
/// already in ascending key order, matching `Object.entries(...).sort(...)`.
pub fn to_json(scores: &Scores) -> String {
    // The map only contains finite `f64`s produced by [`round2`]; serialising
    // it cannot fail. Keep a defensive fallback rather than `unwrap` in shared
    // harness code.
    let mut out = serde_json::to_string_pretty(scores).unwrap_or_else(|_| "{}".to_string());
    out.push('\n');
    out
}

/// Writes `scores`, creating the parent directory. ref `visual.spec.ts:84-85`.
pub fn save(path: &Path, scores: &Scores) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create scores dir {}: {e}", parent.display()))?;
    }
    fs::write(path, to_json(scores))
        .map_err(|e| format!("cannot write fidelity scores {}: {e}", path.display()))
}

/// Read-modify-writes one entry and returns the stored (rounded) value.
/// ref `writeScore` (ref `visual.spec.ts:80-85`).
pub fn record(path: &Path, key: &str, match_pct: f64) -> Result<f64, String> {
    let mut scores = load(path)?;
    let stored = round2(match_pct);
    scores.insert(key.to_string(), stored);
    save(path, &scores)?;
    Ok(stored)
}

/// The ratchet verdict for one slide: `Ok(())` when the score is acceptable,
/// otherwise a failure message. ref `visual.spec.ts:205-215`.
///
/// A missing prior score is a failure, mirroring the reference's
/// `has no recorded fidelity score` throw (`visual.spec.ts:206-208`).
/// `prior` is passed in so this stays a pure decision function.
pub fn ratchet(key: &str, match_pct: f64, prior: Option<f64>) -> Result<(), String> {
    let Some(prior) = prior else {
        return Err(format!(
            "{key} has no recorded fidelity score; run UPDATE_SCORES=1 to record one"
        ));
    };
    if match_pct < prior - RATCHET_DROP_PCT {
        return Err(format!(
            "{key} fidelity regressed: match {match_pct:.2}% is more than \
             {RATCHET_DROP_PCT}pt below the recorded {prior:.2}%"
        ));
    }
    Ok(())
}
