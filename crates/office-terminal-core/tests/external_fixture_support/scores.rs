//! Recorded diagnostic metrics for the external corpus (L0).
//!
//! Strict decks already enforce their authored thresholds. Diagnostic decks
//! only ratchet: a run fails when `min_cell_pass_ratio` drops by more than 0.05
//! or `max_global_mean_abs_diff` rises by more than 2.0 against this file.
//! The baseline records the comparison width; changing `COMPARE_WIDTH` makes
//! the old metrics incomparable, so the ratchet is skipped until the file is
//! regenerated with `UPDATE_EXTERNAL_SCORES=1`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::metrics::DiffMetrics;

pub const RELATIVE: &str = "tests/parity/scores/external.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlideScore {
    pub cell_pass_ratio: f64,
    pub global_mean_abs_diff: f64,
    pub worst_cell_diff: f64,
    pub ink_fraction_delta: f64,
}

impl From<&DiffMetrics> for SlideScore {
    fn from(metrics: &DiffMetrics) -> Self {
        Self {
            cell_pass_ratio: metrics.cell_pass_ratio,
            global_mean_abs_diff: metrics.global_mean_abs_diff,
            worst_cell_diff: metrics.worst_cell_diff,
            ink_fraction_delta: metrics.ink_fraction_delta,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalScores {
    pub commit: String,
    /// Width the metrics were recorded at; absent in pre-thumbnail baselines.
    #[serde(default)]
    pub compare_width: u32,
    pub slides: BTreeMap<String, SlideScore>,
}

pub fn key(deck: &str, slide: usize) -> String {
    format!("{deck}/slide-{slide}")
}

pub fn load() -> Result<Option<ExternalScores>, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(RELATIVE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("parse {}: {error}", path.display()))
}

pub fn write(scores: &ExternalScores) -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(RELATIVE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(scores)
        .map_err(|error| format!("serialize external scores: {error}"))?;
    std::fs::write(&path, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}
