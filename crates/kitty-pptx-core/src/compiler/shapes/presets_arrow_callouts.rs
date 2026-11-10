//! Arrow callouts: an arrow hull combined with a callout pointer wedge.
//!
//! The arrow silhouette is built with default arrow adjustments so that the
//! pointer handles (`adj1`/`adj2`) keep their callout meaning, then the wedge
//! is appended as a second subpath rooted inside the arrow body.

use crate::contracts::VectorPath;

use super::presets_callouts::push_arrow_callout_pointer;
use super::{presets_arrows, presets_t1};

pub fn build_arrow_callout_preset(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) -> Option<VectorPath> {
    let base = match geometry {
        "leftArrowCallout" => presets_t1::build_t1_preset("leftArrow", w, h, &[]),
        "rightArrowCallout" => presets_t1::build_t1_preset("rightArrow", w, h, &[]),
        "upArrowCallout" => presets_t1::build_t1_preset("upArrow", w, h, &[]),
        "downArrowCallout" => presets_t1::build_t1_preset("downArrow", w, h, &[]),
        "leftRightArrowCallout" => presets_t1::build_t1_preset("leftRightArrow", w, h, &[]),
        "upDownArrowCallout" => presets_arrows::build_arrow_preset("upDownArrow", w, h, &[]),
        "quadArrowCallout" => presets_arrows::build_arrow_preset("quadArrow", w, h, &[]),
        _ => return None,
    };
    let mut path = base?;
    push_arrow_callout_pointer(&mut path.segments, w, h, adj);
    Some(path)
}
