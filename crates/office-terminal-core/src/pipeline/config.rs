//! Default worker-count resolution for the render pipeline.
//!
//! The automatic count is host-parallelism clamped to 2..=6: every worker
//! transiently holds a full raster (~37 MB for PDF pages at the viewer width),
//! so the cap bounds worst-case in-flight memory, not just scheduling.
//! `OT_WORKERS` overrides it explicitly (clamped to 1..=8) for machines that
//! can afford more parallel rasters or want a single worker. The pre-rebrand
//! `KITTY_PPTX_WORKERS` name remains a fallback.

/// Hard ceiling for an explicit `OT_WORKERS` request.
const MAX_REQUESTED_WORKERS: usize = 8;

/// Resolve the default background worker count.
///
/// `OT_WORKERS` (or legacy `KITTY_PPTX_WORKERS`) is parsed as a whole number
/// and clamped to 1..=8; absent, blank, or unparseable values fall back to host
/// parallelism clamped to 2..=6. Callers that need a fixed pool (tests, the
/// CLI presenter) construct explicit counts and bypass this entirely.
pub fn default_worker_count() -> usize {
    let raw = std::env::var("OT_WORKERS")
        .or_else(|_| std::env::var("KITTY_PPTX_WORKERS"))
        .ok();
    worker_count_from_raw(raw.as_deref())
}

/// Clamp logic, factored over the raw string so tests never mutate the
/// process-global environment.
fn worker_count_from_raw(raw: Option<&str>) -> usize {
    match raw.map(str::trim).and_then(|value| value.parse::<usize>().ok()) {
        Some(requested) => requested.clamp(1, MAX_REQUESTED_WORKERS),
        None => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(2, 6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_parses_and_clamps_into_range() {
        assert_eq!(worker_count_from_raw(Some("1")), 1);
        assert_eq!(worker_count_from_raw(Some("4")), 4);
        assert_eq!(worker_count_from_raw(Some("8")), 8);
        // Out-of-range requests clamp, never error out.
        assert_eq!(worker_count_from_raw(Some("0")), 1);
        assert_eq!(worker_count_from_raw(Some("99")), 8);
        // Surrounding whitespace must not defeat the override.
        assert_eq!(worker_count_from_raw(Some(" 3 ")), 3);
    }

    #[test]
    fn invalid_or_absent_env_falls_back_to_parallelism() {
        for raw in [None, Some(""), Some("  "), Some("abc"), Some("-1"), Some("1.5")] {
            let count = worker_count_from_raw(raw);
            assert!(
                (2..=6).contains(&count),
                "raw={raw:?} resolved to {count}, outside the automatic clamp"
            );
        }
    }
}
