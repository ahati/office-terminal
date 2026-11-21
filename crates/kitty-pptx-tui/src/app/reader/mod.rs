//! [`ReaderState`] implementation continuations.
//!
//! The reader state model itself lives in
//! [`crate::app::reader_state`]; this module tree holds the two cohesive
//! halves that were split out of it to keep that file under the project's
//! 600-line ceiling:
//!
//! * [`anim`] — smoothed scroll display (the `Arc<AtomicU32>` dance: the
//!   immutable render path advances the displayed offset once per frame),
//! * [`views`] — the per-`PageView` map (fit cycling, zoom adjustment,
//!   view reset, apply-zoom-to-all),
//! * [`status`] — the status bar: `·`-joined segments and the key hints.
//!
//! All are plain `impl ReaderState` continuation blocks, following the same
//! pattern as [`crate::app::reader_scroll`].

mod anim;
mod status;
mod views;
