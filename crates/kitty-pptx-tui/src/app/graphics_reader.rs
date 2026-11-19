//! Terminal graphics synchronization for Reader mode (PRD 14a §7).
//!
//! Staging and direct subrect placement (`Placement::CellsSubrect`) for
//! focusable document panes.

use std::collections::{HashSet, VecDeque};

use kitty_pptx_core::{
    DisplayError, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
};
use kitty_pptx_display::{
    ImageId, Placement, TerminalGraphicsDriver, READER_IMAGE_ID_BASE, READER_IMAGE_ID_SLOTS,
};

use crate::app::bindings::FrameBindings;
use crate::app::graphics_slots::{PlacedKey, PlacedRecord, SlotBitmap, SlotEpochs, SlotState};
use crate::app::reader_state::ReaderSlotPlacement;
use crate::app::state::PresentationState;

/// Number of resident reader image ids (mirrors the display-layer pool).
pub const READER_SLOTS: usize = READER_IMAGE_ID_SLOTS as usize;

/// Reader raster cache budget: pages stay resident while scrolling so row
/// transitions and small jumps do not re-rasterize.
const READER_CACHE_BUDGET_BYTES: usize = 512 * 1024 * 1024;

/// Graphics syncer state for document reader viewports.
#[derive(Default)]
pub(crate) struct ReaderGraphicsSyncer {
    /// One engine slot per resident reader image id, indexed by
    /// `id - READER_IMAGE_ID_BASE` (stable per-page ids, PRD 14a).
    slots: [SlotState; READER_SLOTS],
    /// Prefetch guard keyed on the exact visible target set: re-issues when
    /// the visible window changes, never per frame.
    last_prefetch: Option<Vec<(usize, RenderTargetSpec)>>,
}

impl ReaderGraphicsSyncer {
    /// Index of the pool slot an id belongs to.
    fn slot_index(id: ImageId) -> usize {
        (id.0.saturating_sub(READER_IMAGE_ID_BASE) as usize).min(READER_SLOTS - 1)
    }

    /// The id of a slot bitmap that already holds exactly these pixels
    /// (possibly the spare id of a completed swap).
    fn staged_equivalent(&self, want: &SlotBitmap) -> Option<ImageId> {
        self.slots.iter().find_map(|slot| {
            slot.staged().and_then(|staged| {
                (staged.page == want.page
                    && staged.width == want.width
                    && staged.height == want.height)
                    .then_some(staged.id)
            })
        })
    }

    /// Whether any slot currently displays `id`.
    fn displays(&self, id: ImageId) -> bool {
        self.slots.iter().any(|slot| slot.placed_id() == Some(id))
    }

    /// Id to transmit fresh pixels under.
    ///
    /// Re-transmitting image data for an id deletes that image *and every
    /// placement of it* until a new placement arrives, so the id a pane is
    /// displaying is never reused: a page whose stable id is currently on
    /// screen (an id collision `page % READER_IMAGE_ID_SLOTS`, or a
    /// re-rendered page) stages into a spare id of the reader pool instead,
    /// is presented, and only then is the outgoing id released. The spare
    /// avoids every id another pane owns, stages or displays this frame.
    /// When the pool is exhausted the preferred id degrades to the old
    /// destructive behaviour (more visible panes than ids is already
    /// broken beyond repair).
    fn stage_id(&self, preferred: ImageId, pane_ids: &HashSet<u32>) -> ImageId {
        if !self.displays(preferred) {
            return preferred;
        }
        let busy: HashSet<u32> = self
            .slots
            .iter()
            .filter_map(|slot| slot.staged().map(|staged| staged.id.0))
            .chain(self.slots.iter().filter_map(|slot| slot.placed_id().map(|id| id.0)))
            .chain(pane_ids.iter().copied())
            .collect();
        (0..READER_SLOTS as u32)
            .map(|offset| ImageId(READER_IMAGE_ID_BASE + offset))
            .find(|candidate| !busy.contains(&candidate.0))
            .unwrap_or(preferred)
    }

    pub(crate) fn release_all(
        &mut self,
        driver: &mut dyn TerminalGraphicsDriver,
    ) -> Result<(), DisplayError> {
        for slot in &mut self.slots {
            if let Some(placed) = slot.release_stale(false) {
                let _ = driver.release(placed.id);
            }
        }
        Ok(())
    }

    pub(crate) fn sync(
        &mut self,
        state: &mut PresentationState,
        pipeline: &dyn RenderPipeline,
        driver: &mut dyn TerminalGraphicsDriver,
        _canvas_target: &RenderTargetSpec,
        epochs: &mut SlotEpochs,
        bindings: &FrameBindings,
    ) -> Result<(), DisplayError> {
        let placements: &[ReaderSlotPlacement] = bindings.reader.as_slice();

        // Per-page prefetch targets (P1-1): every visible pane stages at its
        // own aspect-correct target — mixed-size documents included — and the
        // next row is pre-staged so panes scrolling into view never flash
        // blank.
        if let Some(reader) = state.reader_state() {
            let center = reader.focused_page();
            let mut targets: Vec<(usize, RenderTargetSpec)> = Vec::new();
            for placement in placements {
                if !targets.iter().any(|&(page, _)| page == placement.page) {
                    targets.push((placement.page, placement.page_target));
                }
            }
            // Column count of the drawn frame; fall back to the published
            // mirror when this frame laid out no reader panes (e.g. a
            // viewport too small for a pane).
            let cols = if bindings.reader_columns > 0 {
                bindings.reader_columns
            } else {
                state.reader_num_columns()
            }
            .max(1);
            let next_anchor = if reader.cover_alone && reader.anchor == 0 {
                1
            } else {
                reader.anchor + cols
            };
            for i in 0..cols {
                let page = next_anchor + i;
                if page < reader.total_pages() && !targets.iter().any(|&(p, _)| p == page) {
                    targets.push((page, reader.page_render_target(page)));
                }
            }

            if self.last_prefetch.as_ref() != Some(&targets) {
                let center_target = reader.page_render_target(center);
                let page_bytes =
                    (center_target.width_px as usize) * (center_target.height_px as usize) * 4;
                let max_cached_pages = (READER_CACHE_BUDGET_BYTES / page_bytes.max(1)).clamp(4, 32);
                let behind = (cols + 1).min(max_cached_pages / 4);
                let visible_count = reader.visible_pages().len();
                let max_ahead = max_cached_pages
                    .saturating_sub(visible_count + behind)
                    .max(1);
                let ahead = (cols * 2 + 1).min(max_ahead);
                let plan = PrefetchPlan {
                    center,
                    behind,
                    ahead,
                    // Reader pages are pre-encoded on the workers so the syncer
                    // can later write prebuilt bytes instead of encoding.
                    stage: true,
                };
                pipeline.prefetch_window(&plan, &targets);
                self.last_prefetch = Some(targets);
            }
        }

        // Ids the panes own this frame; a swap candidate must avoid them.
        let pane_ids: HashSet<u32> = placements.iter().map(|p| p.id.0).collect();
        let mut active_slots = [false; READER_SLOTS];

        for placement in placements {
            // Stable per-page ids (reader image pool base 81): the pane's
            // nominal slot is derived from the id, not from a row-parity
            // scheme. An id swap can move the pane's bitmap to a spare slot
            // for a frame; both slots stay active so the cleanup below
            // keeps whatever the pane is showing.
            let preferred = placement.id;
            let preferred_idx = Self::slot_index(preferred);
            let page = placement.page;
            active_slots[preferred_idx] = true;

            match pipeline.entry_at(page, &placement.page_target) {
                PipelineEntry::Ready { bitmap, .. } => {
                    let want = SlotBitmap {
                        id: preferred,
                        page,
                        width: bitmap.width,
                        height: bitmap.height,
                    };
                    // The same pixels may already be resident under the
                    // spare id of a completed swap: keep them instead of
                    // re-sending.
                    let id = match self.staged_equivalent(&want) {
                        Some(resident) => resident,
                        None => {
                            // Never transmit over an id a pane is displaying:
                            // kitty deletes the image and its placements
                            // until a new placement arrives, which blanks the
                            // pane for the whole transmit + decode window.
                            let id = self.stage_id(preferred, &pane_ids);
                            let idx = Self::slot_index(id);
                            let want = SlotBitmap { id, ..want };
                            if self.slots[idx].needs_stage(&want) {
                                // Pre-encoded payloads from the raster workers
                                // keep PNG encoding off the UI thread; fall
                                // back to staging the bitmap directly when no
                                // payload is cached (or the driver cannot take
                                // prebuilt bytes).
                                let prebuilt_sent =
                                    match pipeline.staged_payload(page, &placement.page_target) {
                                        Some(payload) => driver
                                            .transmit_prebuilt(
                                                id,
                                                bitmap.width,
                                                bitmap.height,
                                                &payload,
                                            )
                                            .is_ok(),
                                        None => false,
                                    };
                                if !prebuilt_sent {
                                    driver.stage(&bitmap, id)?;
                                }
                                self.slots[idx].record_staged(want, epochs.bump());
                            }
                            id
                        }
                    };
                    let idx = Self::slot_index(id);
                    active_slots[idx] = true;

                    let key =
                        PlacedKey::Subrect { rect: placement.cell_rect, src: placement.pixel_src };
                    let placed_epoch = self.slots[idx].staged_epoch();
                    if self.slots[idx].needs_place(id, key, true) {
                        // The pane's previous placement may live in its
                        // nominal slot (pre-swap) or in this one; collect
                        // both, present first, release afterwards.
                        let mut outgoing: VecDeque<PlacedRecord> =
                            self.slots[idx].take_placed().into_iter().collect();
                        if preferred_idx != idx {
                            outgoing.extend(self.slots[preferred_idx].take_placed());
                        }
                        let direct = Placement::CellsSubrect {
                            rect: placement.cell_rect,
                            src: placement.pixel_src,
                        };
                        driver.present(id, direct)?;
                        self.slots[idx].record_placed(id, key, placed_epoch);
                        for previous in outgoing {
                            if previous.id != id {
                                let _ = driver.release(previous.id);
                                for slot in &mut self.slots {
                                    if slot.staged_holds(previous.id) {
                                        slot.take_staged();
                                    }
                                }
                            }
                        }
                    }

                    // A page that renders cleanly clears any stale failure
                    // note, mirroring the slide syncer.
                    state.clear_render_failure_for(page);
                }
                PipelineEntry::Rendering | PipelineEntry::NotLoaded => {
                    // If this pane still shows another page, release it so
                    // stale pages are never shown during page turns (P1-2).
                    // After an id swap the pane's bitmap can live in a slot
                    // other than its nominal one, so both the nominal slot
                    // and any placement sitting on this pane's rect go.
                    let mut stale = Vec::new();
                    for (idx, slot) in self.slots.iter_mut().enumerate() {
                        if !slot.staged().is_some_and(|staged| staged.page != page) {
                            continue;
                        }
                        let on_pane_rect = slot.placed().is_some_and(|placed| {
                            matches!(
                                placed.key,
                                PlacedKey::Subrect { rect, .. }
                                    if rect == placement.cell_rect
                            )
                        });
                        if idx == preferred_idx || on_pane_rect {
                            if let Some(placed) = slot.take_placed() {
                                stale.push(placed.id);
                            }
                        }
                    }
                    for id in stale {
                        let _ = driver.release(id);
                    }
                }
                PipelineEntry::Failed { error } => {
                    state.note_render_failure(page, error.to_string());
                }
            }
        }

        // Release any slots no longer active (e.g. going from Spread to Single)
        for (slot, &active) in self.slots.iter_mut().zip(active_slots.iter()) {
            if let Some(placed) = slot.release_stale(active) {
                let _ = driver.release(placed.id);
            }
        }

        Ok(())
    }
}
