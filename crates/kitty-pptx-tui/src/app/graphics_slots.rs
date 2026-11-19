//! Shared slot bookkeeping engine for the three graphics syncers.
//!
//! Every view — the full-screen slide pair, the reader panes, the sorter
//! cards — follows the same terminal-slot discipline: compare the staged
//! bitmap identity, transmit on mismatch and take a fresh epoch, compare
//! the on-screen placement, present or reposition on change, and release
//! what went stale. That discipline used to be hand-rolled three times (and
//! had already diverged once), so it lives here exactly once.
//!
//! The engine is pure bookkeeping: it never touches the driver. Each syncer
//! keeps its own POLICY — which slots exist, when to keep presenting an
//! already staged bitmap, present-versus-reposition, release order — and
//! makes the driver calls itself, guided by these primitives.

use kitty_pptx_display::{CellRect, ImageId, PixelRect};

/// Identity of the bitmap a slot must hold.
///
/// `page` is the slide index for the full-screen views and the page index
/// for reader panes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotBitmap {
    pub id: ImageId,
    pub page: usize,
    pub width: u32,
    pub height: u32,
}

/// A placement's identity: cell rect plus, for reader panes, the source crop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlacedKey {
    Cells(CellRect),
    Subrect { rect: CellRect, src: PixelRect },
}

/// A recorded placement: the bitmap id on screen, its key, and the staged
/// epoch it was placed against.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlacedRecord {
    pub id: ImageId,
    pub key: PlacedKey,
    pub epoch: u64,
}

/// What the engine recorded this frame for one slot.
///
/// Returned by the record methods so a syncer (or test) can observe what
/// changed; the syncers decide their policy before recording, so today the
/// flags are asserted by the engine's own unit tests.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct SlotOutcome {
    /// Fresh pixels were transmitted for this slot.
    pub staged_now: bool,
    /// The slot's placement was (re)recorded.
    pub placed_now: bool,
}

/// Bookkeeping for one terminal image slot: what it holds and what's on screen.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct SlotState {
    staged: Option<SlotBitmap>,
    placed: Option<PlacedRecord>,
    /// Staging epoch of the current bitmap: the owning syncer's
    /// [`SlotEpochs`] value at transmit time. A new epoch means new pixels
    /// even when the image id and geometry stayed the same (e.g. slide N and
    /// slide N+8 share an id).
    staged_epoch: u64,
}

impl SlotState {
    /// None-or-different compare: must `want` be transmitted?
    pub fn needs_stage(&self, want: &SlotBitmap) -> bool {
        self.staged.as_ref() != Some(want)
    }

    /// Must the slot be (re)placed? True when nothing is on screen, a
    /// different bitmap id is placed, the placement key moved, or — when
    /// `compare_epoch` is set — the staged pixels changed since the last
    /// placement.
    ///
    /// The slide pair and reader panes pass `true`: they compare the placed
    /// epoch against the staged epoch, so a same-id re-stage re-presents.
    /// Sorter cards pass `false`: their move-don't-restage policy
    /// deliberately ignores content changes at a fixed rect.
    pub fn needs_place(&self, id: ImageId, key: PlacedKey, compare_epoch: bool) -> bool {
        match self.placed {
            None => true,
            Some(placed) => {
                placed.id != id
                    || placed.key != key
                    || (compare_epoch && placed.epoch != self.staged_epoch)
            }
        }
    }

    /// Records that `want` was transmitted under `epoch`.
    pub fn record_staged(&mut self, want: SlotBitmap, epoch: u64) -> SlotOutcome {
        self.staged = Some(want);
        self.staged_epoch = epoch;
        SlotOutcome {
            staged_now: true,
            placed_now: false,
        }
    }

    /// Records that `id` is placed at `key`, placed against `epoch`.
    pub fn record_placed(&mut self, id: ImageId, key: PlacedKey, epoch: u64) -> SlotOutcome {
        self.placed = Some(PlacedRecord { id, key, epoch });
        SlotOutcome {
            staged_now: false,
            placed_now: true,
        }
    }

    /// The staged bitmap identity, if any.
    pub fn staged(&self) -> Option<SlotBitmap> {
        self.staged
    }

    /// Epoch of the staged bitmap; meaningful while a stage is recorded
    /// (a placement always implies a stage).
    pub fn staged_epoch(&self) -> u64 {
        self.staged_epoch
    }

    /// Whether the staged record holds `id` (releasing `id` invalidates it;
    /// a different staged bitmap stays valid on the terminal).
    pub fn staged_holds(&self, id: ImageId) -> bool {
        self.staged.is_some_and(|staged| staged.id == id)
    }

    /// The id currently placed on screen, if any.
    pub fn placed_id(&self) -> Option<ImageId> {
        self.placed.map(|placed| placed.id)
    }

    /// Whether the slot has nothing on screen.
    pub fn is_clear(&self) -> bool {
        self.placed.is_none()
    }

    /// Moves the staged record out: the caller forgets the terminal bitmap.
    pub fn take_staged(&mut self) -> Option<SlotBitmap> {
        self.staged.take()
    }

    /// Moves the placed record out: the caller releases the placement.
    pub fn take_placed(&mut self) -> Option<PlacedRecord> {
        self.placed.take()
    }

    /// Release-when-inactive bookkeeping: an inactive slot is off-screen —
    /// forget both records and hand the placement back so the caller can
    /// release it on the driver. An active slot is untouched.
    pub fn release_stale(&mut self, active: bool) -> Option<PlacedRecord> {
        if active {
            return None;
        }
        self.staged = None;
        self.placed.take()
    }
}

/// Monotonic staging-epoch counter shared by every syncer in the frame
/// loop, so all three views follow one epoch discipline: whichever syncer
/// transmits first after a content change takes the next epoch, and every
/// slot's placement compare sees that the pixels moved.
#[derive(Default, Debug)]
pub struct SlotEpochs {
    counter: u64,
}

impl SlotEpochs {
    /// Bumps the counter and returns the fresh epoch (saturating, so a
    /// `u64` overflow can never wedge the discipline).
    pub fn bump(&mut self) -> u64 {
        self.counter = self.counter.saturating_add(1);
        self.counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(id: u32, page: usize) -> SlotBitmap {
        SlotBitmap { id: ImageId(id), page, width: 4, height: 2 }
    }

    fn cells(col: u16) -> PlacedKey {
        PlacedKey::Cells(CellRect { row: 0, col, rows: 2, cols: 4 })
    }

    #[test]
    fn an_empty_slot_needs_stage_and_place() {
        let mut slot = SlotState::default();
        assert!(slot.needs_stage(&bitmap(1, 0)));
        assert!(slot.needs_place(ImageId(1), cells(0), true));
        assert!(slot.is_clear());
        assert_eq!(slot.staged(), None);
        assert_eq!(slot.placed_id(), None);
        assert_eq!(slot.take_staged(), None);
        assert_eq!(slot.take_placed(), None);
    }

    #[test]
    fn record_staged_satisfies_needs_stage_until_identity_changes() {
        let mut slot = SlotState::default();
        let want = bitmap(1, 0);
        assert!(slot.needs_stage(&want));
        let outcome = slot.record_staged(want, 7);
        assert_eq!(
            outcome,
            SlotOutcome {
                staged_now: true,
                placed_now: false
            }
        );
        assert!(!slot.needs_stage(&want));

        // A different id, page, or bitmap size all demand a re-stage.
        assert!(slot.needs_stage(&bitmap(2, 0)));
        assert!(slot.needs_stage(&bitmap(1, 1)));
        assert!(slot.needs_stage(&SlotBitmap { id: ImageId(1), page: 0, width: 8, height: 2 }));
        assert_eq!(slot.staged_epoch(), 7);
    }

    #[test]
    fn record_placed_satisfies_needs_place_until_id_key_or_epoch_change() {
        let mut slot = SlotState::default();
        slot.record_staged(bitmap(1, 0), 7);
        let key = cells(0);
        assert!(slot.needs_place(ImageId(1), key, true));
        let outcome = slot.record_placed(ImageId(1), key, 7);
        assert_eq!(
            outcome,
            SlotOutcome {
                staged_now: false,
                placed_now: true
            }
        );
        assert!(!slot.needs_place(ImageId(1), key, true));
        assert!(!slot.is_clear());
        assert_eq!(slot.placed_id(), Some(ImageId(1)));

        // A different bitmap id or a moved rect needs a new placement.
        assert!(slot.needs_place(ImageId(2), key, true));
        assert!(slot.needs_place(ImageId(1), cells(4), true));

        // A same-id re-stage bumps the epoch: slides and reader panes must
        // re-present fresh pixels under identical geometry.
        slot.record_staged(bitmap(1, 0), 8);
        assert!(slot.needs_place(ImageId(1), key, true));
        // Sorter cards deliberately ignore the epoch at a fixed rect.
        assert!(!slot.needs_place(ImageId(1), key, false));
    }

    #[test]
    fn subrect_keys_compare_the_source_crop() {
        let mut slot = SlotState::default();
        slot.record_staged(bitmap(81, 3), 1);
        let key = PlacedKey::Subrect {
            rect: CellRect {
                row: 0,
                col: 0,
                rows: 40,
                cols: 68,
            },
            src: PixelRect {
                x: 0,
                y: 16,
                width: 2560,
                height: 512,
            },
        };
        slot.record_placed(ImageId(81), key, 1);
        assert!(!slot.needs_place(ImageId(81), key, true));

        // Same cells but a scrolled source crop must re-place the pane.
        let scrolled = PlacedKey::Subrect {
            rect: CellRect {
                row: 0,
                col: 0,
                rows: 40,
                cols: 68,
            },
            src: PixelRect {
                x: 0,
                y: 32,
                width: 2560,
                height: 512,
            },
        };
        assert!(slot.needs_place(ImageId(81), scrolled, true));
    }

    #[test]
    fn take_and_staged_holds_drive_release_bookkeeping() {
        let mut slot = SlotState::default();
        slot.record_staged(bitmap(1, 0), 3);
        slot.record_placed(ImageId(1), cells(0), 3);
        assert!(slot.staged_holds(ImageId(1)));
        assert!(!slot.staged_holds(ImageId(2)));

        // Releasing a placement forgets the staged key only when the
        // released id is the staged one (a different staged bitmap stays
        // valid on the terminal).
        let previous = slot.take_placed().expect("placement was recorded");
        assert_eq!(previous.id, ImageId(1));
        assert_eq!(previous.key, cells(0));
        assert_eq!(previous.epoch, 3);
        assert!(slot.staged_holds(ImageId(1)));
        assert_eq!(slot.take_staged(), Some(bitmap(1, 0)));
        assert!(!slot.staged_holds(ImageId(1)));
        assert!(slot.is_clear());
    }

    #[test]
    fn release_stale_clears_inactive_slots_only() {
        let mut slot = SlotState::default();
        slot.record_staged(bitmap(81, 2), 4);
        slot.record_placed(ImageId(81), cells(0), 4);

        // An active slot keeps its bookkeeping and releases nothing.
        assert!(slot.release_stale(true).is_none());
        assert!(!slot.is_clear());
        assert!(slot.staged().is_some());

        // An inactive slot hands its placement back and forgets both records.
        let released = slot.release_stale(false).expect("inactive slot releases");
        assert_eq!(released.id, ImageId(81));
        assert_eq!(released.epoch, 4);
        assert_eq!(slot.staged(), None);
        assert!(slot.is_clear());
        assert_eq!(slot.release_stale(false), None);
    }

    #[test]
    fn epochs_bump_monotonically() {
        let mut epochs = SlotEpochs::default();
        assert_eq!(epochs.bump(), 1);
        assert_eq!(epochs.bump(), 2);
        assert_eq!(epochs.bump(), 3);
    }
}
