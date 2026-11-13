//! DrawingML table shared-edge conflict resolver.
//! ref `packages/pptx/src/table-border-conflict.ts:79-96`.
//!
//! SPEC IS SILENT for PresentationML; this is the defined deterministic rule:
//! null loses -> wider wins -> darker wins (Rec.601 luma) -> owner wins.

/// Shared interior edge: pick the single winning stroke from the two facing
/// candidates. `a` is the OWNING (reading-order-first) side.
pub(super) fn resolve_border_conflict<'a>(
    a: Option<&'a pptx_model::Stroke>,
    b: Option<&'a pptx_model::Stroke>,
) -> Option<&'a pptx_model::Stroke> {
    match (a, b) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => {
            if a.width != b.width {
                return Some(if a.width > b.width { a } else { b });
            }
            let (la, lb) = (luminance(a), luminance(b));
            if la != lb {
                return Some(if la < lb { a } else { b });
            }
            Some(a)
        }
    }
}

/// Rec.601 perceived luma; smaller = darker. The 8-hex alpha pair is ignored
/// and a malformed colour compares as black.
/// ref table-border-conflict.ts:53-70
fn luminance(stroke: &pptx_model::Stroke) -> f32 {
    let (r, g, b) = rgb_channels(&stroke.color);
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

fn rgb_channels(color: &str) -> (u8, u8, u8) {
    let clean = color.trim().trim_start_matches('#');
    let bytes = clean.as_bytes();
    if bytes.len() < 6 {
        return (0, 0, 0);
    }
    let channel = |i: usize| {
        std::str::from_utf8(&bytes[i..i + 2])
            .ok()
            .and_then(|s| u8::from_str_radix(s, 16).ok())
    };
    match (channel(0), channel(2), channel(4)) {
        (Some(r), Some(g), Some(b)) => (r, g, b),
        _ => (0, 0, 0),
    }
}
