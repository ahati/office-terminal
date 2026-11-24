//! ODF shrink-to-fit (`TextBody.shrink_on_overflow`): derive the largest font
//! scale at which the wrapped body fits its frame, as LibreOffice Impress
//! does (ODF 1.3 Part 3 §20.358 `style:shrink-to-fit`).
//!
//! DrawingML sources never set the flag — PowerPoint only ever applies a
//! *stored* `normAutofit@fontScale` (`TextBody.font_scale`), which the
//! emission pass already honours — so this path runs only for model sources
//! that explicitly ask for it and carry no stored scale.

use crossbeam_channel::Sender;

use crate::contracts::{FidelityClass, WarningEvent};

use super::body_layout::break_body_lines;
use super::emit::{push_default_style, Measurer};
use super::paragraphs;

/// Full size in half-percent units (100 %). Half a percent is the finest
/// step worth converging to: PowerPoint's own autofit walks integer
/// percents, and a finer grain is imperceptible at terminal cell sizes.
const MAX_HALF_PERCENTS: i32 = 200;
/// Search floor: a tenth of the authored size. Below it the text is
/// unreadable at every cell resolution, so the body keeps overflowing like
/// an unsatisfiable fit and the reader is warned instead.
const MIN_HALF_PERCENTS: i32 = 20;

/// Largest half-percent font scale (≤ 100 %) whose wrapped layout fits the
/// frame's inner height.
///
/// Returns 1.0 when the body already fits (no shrink), and the floor with a
/// `shrink_on_overflow_floor` fidelity warning when even a tenth overflows.
/// The emission pass runs once more at the returned scale with the real
/// warning sink, so measurement passes lay out against a scratch channel and
/// never duplicate substitution warnings.
#[allow(clippy::too_many_arguments)]
pub(super) fn derive_shrink_scale(
    text_body: &pptx_model::TextBody,
    default_color: [u8; 4],
    avail_w: f32,
    t_ins: f32,
    b_ins: f32,
    layout_h: f32,
    reduction: f32,
    warning_sink: &Sender<WarningEvent>,
) -> f32 {
    let capacity = (layout_h - t_ins - b_ins).max(0.0);
    // A degenerate frame cannot fit anything at any scale; shrinking would
    // only blur the overflow away.
    if capacity <= 0.0 {
        return 1.0;
    }
    let do_wrap = text_body.wrap != "none";
    let num_col = text_body.num_col.max(1) as usize;
    let (scratch_sink, _scratch_rx) = crossbeam_channel::unbounded::<WarningEvent>();

    let fits = |half_percent: i32| -> bool {
        let scale = half_percent as f32 / MAX_HALF_PERCENTS as f32;
        let mut measurer = Measurer::new();
        let default_style = push_default_style(&mut measurer, text_body, scale, default_color);
        let paragraphs = paragraphs::build_paragraphs(
            text_body,
            default_style,
            scale,
            default_color,
            &mut measurer,
            &scratch_sink,
        );
        let broken = break_body_lines(
            text_body,
            &paragraphs,
            &measurer,
            avail_w,
            do_wrap,
            reduction,
            t_ins,
            b_ins,
            layout_h,
            num_col,
        );
        // `plan_columns` accepts a column as fitting within half an EMU.
        broken.anchor_h <= capacity + 0.5
    };

    if fits(MAX_HALF_PERCENTS) {
        return 1.0;
    }
    if !fits(MIN_HALF_PERCENTS) {
        let _ = warning_sink.send(WarningEvent::FidelityDegraded {
            element_id: "text-body".to_string(),
            feature: "shrink_on_overflow_floor",
            class: FidelityClass::Degraded,
        });
        return MIN_HALF_PERCENTS as f32 / MAX_HALF_PERCENTS as f32;
    }
    // The candidates bracket a crossing (fits at the floor, overflows at
    // full size), so a binary search converges in ceil(log2 180) ≈ 8
    // layouts — the only bodies that pay for it are the flagged ones.
    let (mut lo, mut hi) = (MIN_HALF_PERCENTS, MAX_HALF_PERCENTS);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo as f32 / MAX_HALF_PERCENTS as f32
}
