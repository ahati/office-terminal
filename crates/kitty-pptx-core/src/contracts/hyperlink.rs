use serde::{Deserialize, Serialize};
use super::geometry::RectF32;

/// Semantic destination of an interactive hyperlink.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HyperlinkTarget {
    /// Validated external web or communication URI (https://, http://, mailto:).
    ExternalUri(String),
    /// 0-indexed chronological slide number in the presentation.
    SlideIndex(usize),
    /// Native PowerPoint presentation control action.
    Action(SlideAction),
}

/// Standard presentation control actions from `ppaction://` verbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlideAction {
    NextSlide,
    PreviousSlide,
    FirstSlide,
    LastSlide,
    EndShow,
}

/// Rich hyperlink annotation anchoring an interactive target to a slide coordinate region.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlideLink {
    pub target: HyperlinkTarget,
    pub bounds_emu: RectF32,
    pub tooltip: Option<String>,
}

impl SlideLink {
    pub fn new(target: HyperlinkTarget, bounds_emu: RectF32, tooltip: Option<String>) -> Self {
        Self {
            target,
            bounds_emu,
            tooltip,
        }
    }
}
