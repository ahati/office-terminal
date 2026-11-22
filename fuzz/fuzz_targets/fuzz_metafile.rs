//! Arbitrary bytes → classify → playback → recorder (PRD 13 §8.3).
//!
//! Invariants: no panic/abort, bounded input via [`MetafileLimits`], balanced
//! clip push/pop, finite transforms. Run with `cargo +nightly fuzz run
//! fuzz_metafile`; the deterministic smoke lives in
//! `crates/office-terminal-core/tests/metafile_robustness_tests.rs`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use office_terminal_core::contracts::{AffineTransform, VectorCommand};
use office_terminal_core::metafile::engine_metafile_rs::MetafileRsEngine;
use office_terminal_core::metafile::recorder::MetafileRecorder;
use office_terminal_core::metafile::{classify_bytes, MetafileEngine, MetafileLimits};

fuzz_target!(|data: &[u8]| {
    let limits = MetafileLimits {
        max_input_bytes: 1 << 20,
        max_records: 50_000,
        ..MetafileLimits::default()
    };
    let _ = classify_bytes(data);
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let _ = MetafileRsEngine.play(data, &limits, &mut recorder);
    let (commands, _) = recorder.finish();

    let mut depth = 0i32;
    for command in commands {
        match command {
            VectorCommand::PushClip(_) => depth += 1,
            VectorCommand::PopClip => {
                depth -= 1;
                assert!(depth >= 0, "clip pop without a push");
            }
            VectorCommand::DrawPath { transform, .. }
            | VectorCommand::DrawLineEnd { transform, .. }
            | VectorCommand::DrawText { transform, .. }
            | VectorCommand::DrawImage { transform, .. }
            | VectorCommand::DrawMetafile { transform, .. } => {
                for value in [
                    transform.sx,
                    transform.ky,
                    transform.kx,
                    transform.sy,
                    transform.tx,
                    transform.ty,
                ] {
                    assert!(value.is_finite(), "non-finite transform");
                }
            }
            VectorCommand::DrawBitmap { placement, .. } => {
                assert!(placement.sx.is_finite() && placement.sy.is_finite());
            }
        }
    }
    assert_eq!(depth, 0, "unbalanced clip stack");
});
