//! Milestone playback tests for PRD 13: text (M3), EMF+ (M4) and WMF/EMZ (M5).
//!
//! The milestones live in sibling submodules of `tests/metafile_milestone_tests/`
//! and share the fixtures in [`helpers`]: `text` (M3), `emfplus` (M4) and
//! `containers` (M5).

#![cfg(feature = "metafile")]

mod metafile_support;

#[path = "metafile_milestone_tests/containers.rs"]
mod containers;
#[path = "metafile_milestone_tests/emfplus.rs"]
mod emfplus;
#[path = "metafile_milestone_tests/helpers.rs"]
mod helpers;
#[path = "metafile_milestone_tests/text.rs"]
mod text;
