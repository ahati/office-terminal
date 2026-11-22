//! Hidden-slide policy shared by the viewer state machine.
//!
//! The reference viewer exposes `<p:sld show="0">` slides through three modes
//! (`show`, `skip`, `dim`) and resolves navigation through the pure helpers in
//! `packages/core/src/nav/visible-index.ts`. This module is the Rust port of
//! both: the mode enum (`packages/pptx/src/hidden.ts:6-10`) and the index
//! arithmetic, kept free of any deck/presentation type so the viewer, the
//! search index and the slide sorter can share one implementation.
//!
//! All indices are absolute (0-based, `sldIdLst` order); hidden slides never
//! renumber the deck — they are simply skipped by the `skip` policy.
// ref packages/pptx/src/hidden.ts:6-10, packages/core/src/nav/visible-index.ts:1-41

/// How a viewer presents hidden slides (`<p:sld show="0">`).
// ref packages/pptx/src/hidden.ts:8-9
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HiddenSlideMode {
    /// Hidden slides stay addressable and are drawn normally (default).
    #[default]
    Show,
    /// Sequential navigation and absolute jumps skip hidden slides (#2181).
    Skip,
    /// Hidden slides stay addressable but are drawn under the dim overlay.
    Dim,
}

impl HiddenSlideMode {
    /// Whether this mode jumps over hidden slides.
    pub fn is_skip(self) -> bool {
        matches!(self, Self::Skip)
    }
}

/// Next shown (non-hidden) absolute index in `direction` (+1 forward, -1 back),
/// searching strictly after `from`.
///
/// Returns `from` itself when no other shown index exists in that direction, so
/// the caller simply stays put at a deck edge.
// ref packages/core/src/nav/visible-index.ts:8-18
pub fn next_visible(
    from: usize,
    direction: i8,
    is_hidden: impl Fn(usize) -> bool,
    count: usize,
) -> usize {
    if count == 0 {
        return from;
    }
    let step = direction.signum() as isize;
    if step == 0 {
        return from;
    }
    let mut index = from as isize + step;
    while index >= 0 && (index as usize) < count {
        if !is_hidden(index as usize) {
            return index as usize;
        }
        index += step;
    }
    from
}

/// Resolve which slide to show on initial load or when entering `skip` mode:
/// keep `current` when shown, else the nearest shown slide after it, else the
/// nearest shown slide before it, else `current` (an all-hidden deck degenerates
/// to `show`).
// ref packages/core/src/nav/visible-index.ts:21-29
pub fn resolve_visible(current: usize, is_hidden: impl Fn(usize) -> bool, count: usize) -> usize {
    if count == 0 || !is_hidden(current) {
        return current;
    }
    let forward = next_visible(current, 1, &is_hidden, count);
    if forward != current {
        return forward;
    }
    next_visible(current, -1, &is_hidden, count)
}

/// Count the shown (non-hidden) slides out of `count`.
// ref packages/core/src/nav/visible-index.ts:32-40
pub fn count_visible(is_hidden: impl Fn(usize) -> bool, count: usize) -> usize {
    (0..count).filter(|&index| !is_hidden(index)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_visible_stays_put_when_every_candidate_is_hidden() {
        assert_eq!(next_visible(1, 1, |_| true, 3), 1);
        assert_eq!(next_visible(1, -1, |_| true, 3), 1);
        assert_eq!(next_visible(0, 1, |_| false, 0), 0);
    }

    #[test]
    fn resolve_visible_prefers_forward_then_backward() {
        assert_eq!(resolve_visible(1, |i| i == 1, 4), 2);
        assert_eq!(resolve_visible(3, |i| i >= 3, 4), 2);
        assert_eq!(resolve_visible(0, |i| i == 0, 2), 1);
        assert_eq!(count_visible(|i| i == 1, 4), 3);
    }
}
