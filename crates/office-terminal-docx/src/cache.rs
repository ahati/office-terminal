//! Bounded LRU cache of compiled `VectorSlide` pages (issue #40 §7.16).
//!
//! The render pipeline may compile the same page for several terminal sizes
//! and surfaces (main view, thumbnails, peek panes); `VectorSlide` is
//! resolution-independent, so a page-index cache with a rough byte budget
//! removes the repeated geometry conversion without holding the document.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use office_terminal_core::contracts::{VectorCommand, VectorSlide};
use office_terminal_core::RenderError;

/// LRU of compiled pages keyed by page index.
pub(crate) struct IrCache {
    inner: Mutex<Inner>,
    max_entries: usize,
    max_bytes: usize,
}

struct Inner {
    entries: HashMap<usize, (Arc<VectorSlide>, u64)>,
    clock: u64,
    bytes: usize,
}

impl IrCache {
    pub(crate) fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                clock: 0,
                bytes: 0,
            }),
            max_entries,
            max_bytes,
        }
    }

    /// Return the cached page or compile it through `compile` and publish.
    pub(crate) fn get_or_compile(
        &self,
        index: usize,
        compile: impl FnOnce() -> Result<VectorSlide, RenderError>,
    ) -> Result<Arc<VectorSlide>, RenderError> {
        {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inner.clock += 1;
            let tick = inner.clock;
            if let Some((slide, clock)) = inner.entries.get_mut(&index) {
                *clock = tick;
                return Ok(Arc::clone(slide));
            }
        }

        let slide = Arc::new(compile()?);

        let cost = estimate_slide_bytes(&slide);
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.clock += 1;
        let tick = inner.clock;
        insert(&mut inner, index, Arc::clone(&slide), tick, cost);
        evict(&mut inner, self.max_entries, self.max_bytes);
        Ok(slide)
    }
}

/// Insert (or replace) a compiled page. Two threads may compile the same
/// index concurrently (no single-flight); when the second insert replaces
/// the first, the replaced entry's cost must leave the ledger before the new
/// one enters, or `bytes` drifts upward and evicts too eagerly.
fn insert(inner: &mut Inner, index: usize, slide: Arc<VectorSlide>, tick: u64, cost: usize) {
    if let Some((replaced, _)) = inner.entries.insert(index, (slide, tick)) {
        inner.bytes = inner.bytes.saturating_sub(estimate_slide_bytes(&replaced));
    }
    inner.bytes += cost;
}

fn evict(inner: &mut Inner, max_entries: usize, max_bytes: usize) {
    while inner.entries.len() > max_entries || inner.bytes > max_bytes {
        let Some(oldest) = inner
            .entries
            .iter()
            .min_by_key(|(_, (_, clock))| *clock)
            .map(|(index, _)| *index)
        else {
            return;
        };
        if let Some((slide, _)) = inner.entries.remove(&oldest) {
            inner.bytes = inner.bytes.saturating_sub(estimate_slide_bytes(&slide));
        }
    }
}

/// Rough byte estimate: enough to bound memory without walking every glyph.
fn estimate_slide_bytes(slide: &VectorSlide) -> usize {
    let mut bytes = 256;
    for command in &slide.commands {
        bytes += match command {
            VectorCommand::DrawPath { path, .. } => 96 + path.segments.len() * 24,
            VectorCommand::DrawLineEnd { path, .. } => 64 + path.segments.len() * 24,
            VectorCommand::DrawText { runs, .. } => runs
                .iter()
                .map(|run| 128 + run.glyphs.len() * 24 + run.text.len())
                .sum(),
            VectorCommand::DrawImage { .. } | VectorCommand::DrawMetafile { .. } => 128,
            VectorCommand::DrawBitmap { png, .. } => 128 + png.len(),
            VectorCommand::PushClip(path) => 32 + path.segments.len() * 16,
            VectorCommand::PopClip => 8,
        };
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(tag: u8) -> VectorSlide {
        let mut slide = VectorSlide::new(612 * 12_700, 792 * 12_700);
        slide.commands.push(VectorCommand::DrawText {
            runs: vec![office_terminal_core::contracts::PositionedGlyphRun {
                text: format!("{tag}"),
                font_family: "Carlito".into(),
                font_size_pt: 12.0,
                color: [0, 0, 0, 255],
                bold: false,
                italic: false,
                glyphs: vec![office_terminal_core::contracts::PositionedGlyph {
                    glyph_id: 1,
                    x: 0.0,
                    y: 0.0,
                    advance: 100.0,
                    warp_u: None,
                }],
                decorations: Default::default(),
                single_face: false,
                face: None,
                warp: None,
                reflection: None,
            }],
            transform: Default::default(),
        });
        slide
    }

    #[test]
    fn compiles_once_and_serves_cached_handles() {
        let cache = IrCache::new(16, 32 * 1024 * 1024);
        let mut compiled = 0;
        for round in 0..2 {
            let slide = cache
                .get_or_compile(0, || {
                    compiled += 1;
                    Ok(slide(round as u8))
                })
                .expect("compile");
            assert_eq!(slide.commands.len(), 1);
        }
        assert_eq!(compiled, 1);
    }

    #[test]
    fn entry_cap_evicts_least_recently_used() {
        let cache = IrCache::new(2, 32 * 1024 * 1024);
        for index in 0..3 {
            cache
                .get_or_compile(index, || Ok(slide(index as u8)))
                .expect("compile");
        }
        // Touch page 2 so page 1 becomes the LRU victim.
        let _ = cache.get_or_compile(2, || unreachable!()).expect("cached");
        // Compile page 3; with a 2-entry cap the victim must be page 1
        // (page 2 was just touched).
        let _ = cache
            .get_or_compile(3, || Ok(slide(3)))
            .expect("compile");
        let missing = cache.get_or_compile(1, || Err(RenderError::CompilationFailed("evicted".into())));
        assert!(missing.is_err());
    }

    #[test]
    fn compile_errors_are_not_cached() {
        let cache = IrCache::new(16, 32 * 1024 * 1024);
        let first = cache.get_or_compile(0, || Err(RenderError::CompilationFailed("boom".into())));
        assert!(first.is_err());
        let second = cache.get_or_compile(0, || Ok(slide(0)));
        assert!(second.is_ok());
    }

    #[test]
    fn replacing_an_entry_keeps_the_byte_accounting_exact() {
        let mut inner = Inner {
            entries: HashMap::new(),
            clock: 0,
            bytes: 0,
        };
        let first = Arc::new(slide(1));
        let cost = estimate_slide_bytes(&first);
        insert(&mut inner, 0, Arc::clone(&first), 1, cost);
        assert_eq!(inner.bytes, cost);

        // A concurrent compile of the same index overwrites the entry: the
        // replaced cost must leave the ledger before the new one enters.
        let second = Arc::new(slide(2));
        let second_cost = estimate_slide_bytes(&second);
        insert(&mut inner, 0, Arc::clone(&second), 2, second_cost);
        assert_eq!(inner.bytes, second_cost);
        assert_eq!(inner.entries.len(), 1);
        assert_eq!(inner.entries[&0].1, 2, "the newest clock wins");
    }
}
