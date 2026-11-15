//! Deterministic robustness smoke for arbitrary metafile bytes (PRD 13 §8.3).
//!
//! The blocking fuzz gate is `fuzz/fuzz_targets/fuzz_metafile.rs`; this test
//! replays a small deterministic corpus (random buffers plus mutated valid
//! EMFs) through classify → playback → recorder so the panic/termination
//! invariants run on every `cargo test`, not only under cargo-fuzz.

#![cfg(feature = "metafile")]


mod metafile_support;

use std::time::Instant;

use kitty_pptx_core::contracts::{AffineTransform, VectorCommand};
use kitty_pptx_core::metafile::engine_metafile_rs::MetafileRsEngine;
use kitty_pptx_core::metafile::recorder::MetafileRecorder;
use kitty_pptx_core::metafile::{self, MetafileEngine, MetafileLimits};
use metafile_support as support;

/// xorshift64*: deterministic, no external crates.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn assert_sane(commands: &[VectorCommand]) {
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
            | VectorCommand::DrawMetafile { transform, .. } => assert!(finite(transform)),
            VectorCommand::DrawBitmap { placement, .. } => assert!(finite(placement)),
        }
    }
    assert_eq!(depth, 0, "unbalanced clip stack");
}

fn finite(transform: &AffineTransform) -> bool {
    [
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    ]
    .iter()
    .all(|value| value.is_finite())
}

fn play_guarded(bytes: &[u8]) {
    let limits = MetafileLimits {
        max_input_bytes: 256 * 1024,
        max_records: 20_000,
        ..MetafileLimits::default()
    };
    let engine = MetafileRsEngine;
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let started = Instant::now();
    let _ = engine.play(bytes, &limits, &mut recorder);
    let (commands, _) = recorder.finish();
    assert!(
        started.elapsed().as_secs() < 2,
        "one input took more than 2s"
    );
    assert_sane(&commands);
}

#[test]
fn arbitrary_and_mutated_bytes_never_panic_or_unbalance_clips() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    // Random containers.
    for _ in 0..64 {
        let len = 1 + (rng.next() as usize % 4096);
        let mut bytes = vec![0u8; len];
        for byte in &mut bytes {
            *byte = rng.next() as u8;
        }
        let _ = metafile::classify_bytes(&bytes);
        play_guarded(&bytes);
    }

    // Mutated valid EMFs (header + geometry + text) to reach deeper records.
    let valid = support::emf(&[
        support::create_font(1, 20, 700, 0, false, false, "Carlito"),
        support::select_object(1),
        support::set_text_align(0x0018),
        support::ext_text_out((20, 40), "Hi", 0, None, None),
        support::rectangle([10, 10, 120, 80]),
    ]);
    for _ in 0..128 {
        let mut bytes = valid.clone();
        let flips = 1 + (rng.next() as usize % 8);
        for _ in 0..flips {
            let index = rng.next() as usize % bytes.len();
            bytes[index] = rng.next() as u8;
        }
        play_guarded(&bytes);
    }

    // Truncations keep the header (which dispatches to the engine) but cut the
    // record stream at every interesting boundary.
    for len in [8, 44, 64, 88, 96, 128, valid.len().saturating_sub(1)] {
        if len <= valid.len() {
            play_guarded(&valid[..len]);
        }
    }
}
