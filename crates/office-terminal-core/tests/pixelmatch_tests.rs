//! Unit tests for the `pixelmatch@7.1.0` port used by the VRT harness.
//!
//! Everything here is synthetic: raw RGBA buffers built in-memory, no image
//! codecs and no fixture decks. Expected values are either derived from the
//! reference `colorDelta` formula (transcribed below) or hand-computed from the
//! `visual.spec.ts:182-183` percentage arithmetic.
//!
//! Only `parity_support/pixelmatch.rs` is included (not the shared `mod.rs`)
//! so this target compiles independently of the other parity workstreams.

#[path = "parity_support/pixelmatch.rs"]
mod pixelmatch;

use pixelmatch::{compare_rgba, pad_to_white, DiffOptions, DiffResult};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for _ in 0..(w as usize) * (h as usize) {
        out.extend_from_slice(&rgba);
    }
    out
}

fn put(buf: &mut [u8], w: u32, x: u32, y: u32, rgba: [u8; 4]) {
    let pos = ((y * w + x) as usize) * 4;
    buf[pos..pos + 4].copy_from_slice(&rgba);
}

fn counted(r: &DiffResult) -> usize {
    r.diff_mask.iter().filter(|b| **b != 0).count()
}

fn approx(actual: f64, expected: f64, eps: f64) {
    assert!(
        (actual - expected).abs() <= eps,
        "expected {expected} +/- {eps}, got {actual}"
    );
}

/// Transcription of `colorDelta` from `pixelmatch@7.1.0/index.js:209-246`,
/// including the `48/159` alpha checkerboard branch. Used to derive the
/// threshold boundaries in the tests without hard-coding magic numbers.
fn ref_delta(p1: [u8; 4], p2: [u8; 4], byte_offset: usize, y_only: bool) -> f64 {
    // Reference literal `1.618033988749895`.
    const PHI: f64 = 1.618_033_988_749_895;
    const PHI2: f64 = 2.618_033_988_749_895;

    let [r1, g1, b1, a1] = p1.map(f64::from);
    let [r2, g2, b2, a2] = p2.map(f64::from);
    let mut dr = r1 - r2;
    let mut dg = g1 - g2;
    let mut db = b1 - b2;
    let da = a1 - a2;
    if dr == 0.0 && dg == 0.0 && db == 0.0 && da == 0.0 {
        return 0.0;
    }
    if a1 < 255.0 || a2 < 255.0 {
        let rb = 48.0 + 159.0 * (byte_offset % 2) as f64;
        let gb = 48.0 + 159.0 * (((byte_offset as f64 / PHI).trunc() as i64) % 2) as f64;
        let bb = 48.0 + 159.0 * (((byte_offset as f64 / PHI2).trunc() as i64) % 2) as f64;
        dr = (r1 * a1 - r2 * a2 - rb * da) / 255.0;
        dg = (g1 * a1 - g2 * a2 - gb * da) / 255.0;
        db = (b1 * a1 - b2 * a2 - bb * da) / 255.0;
    }
    let y = dr * 0.29889531 + dg * 0.58662247 + db * 0.11448223;
    if y_only {
        return y;
    }
    let i = dr * 0.59597799 - dg * 0.27417610 - db * 0.32180189;
    let q = dr * 0.21147017 - dg * 0.52261711 + db * 0.31114694;
    let delta = 0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q;
    if y > 0.0 {
        -delta
    } else {
        delta
    }
}

// ── Defaults ─────────────────────────────────────────────────────────────────

#[test]
fn defaults_are_the_vrt_options() {
    let o = DiffOptions::default();
    assert_eq!(o.threshold, 0.20);
    assert!(o.include_aa);
}

// ── Identical images ─────────────────────────────────────────────────────────

#[test]
fn identical_images_are_a_perfect_match() {
    let a = solid(4, 3, [10, 20, 30, 255]);
    let r = compare_rgba(&a, &a, 4, 3, &DiffOptions::default());

    assert_eq!(r.diff_pixels, 0);
    assert_eq!(r.total_pixels, 12);
    assert_eq!(r.diff_mask.len(), 12);
    assert_eq!(counted(&r), 0);
    assert_eq!(r.diff_pct, 0.0);
    assert_eq!(r.match_pct, 100.0);
    assert_eq!(r.mean_abs_diff, 0.0);
}

#[test]
fn identical_transparent_images_match() {
    // The identical-32-bit fast path (ref index.js:47-50) short-circuits to 0
    // before the alpha branch runs.
    let a = solid(3, 2, [10, 20, 30, 0]);
    let r = compare_rgba(&a, &a, 3, 2, &DiffOptions::default());

    assert_eq!(r.diff_pixels, 0);
    assert_eq!(r.match_pct, 100.0);
    assert_eq!(r.mean_abs_diff, 0.0);
}

// ── Single changed pixel ─────────────────────────────────────────────────────

#[test]
fn single_changed_pixel() {
    let w = 8;
    let h = 8;
    let white = solid(w, h, [255, 255, 255, 255]);
    let mut changed = white.clone();
    put(&mut changed, w, 5, 2, [0, 0, 0, 255]);

    let r = compare_rgba(&white, &changed, w, h, &DiffOptions::default());

    assert_eq!(r.diff_pixels, 1);
    assert_eq!(r.total_pixels, 64);
    // ref visual.spec.ts:182-183.
    approx(r.diff_pct, 100.0 / 64.0, 1e-12);
    approx(r.match_pct, 100.0 - 100.0 / 64.0, 1e-12);

    // One pixel changed by 255 on R,G,B only: 765 / (64 pixels * 4 ch * 255).
    approx(r.mean_abs_diff, 765.0 / (64.0 * 4.0 * 255.0), 1e-15);

    let mut expected = vec![0u8; 64];
    expected[2 * 8 + 5] = 255;
    assert_eq!(r.diff_mask, expected);
}

#[test]
fn diff_mask_is_row_major() {
    let w = 3;
    let h = 2;
    let base = solid(w, h, [255, 255, 255, 255]);
    let mut changed = base.clone();
    put(&mut changed, w, 0, 0, [0, 0, 0, 255]);
    put(&mut changed, w, 1, 1, [0, 0, 0, 255]);

    let r = compare_rgba(&base, &changed, w, h, &DiffOptions::default());

    assert_eq!(r.diff_pixels, 2);
    assert_eq!(r.diff_mask, vec![255, 0, 0, 0, 255, 0]);
}

// ── Threshold boundary ───────────────────────────────────────────────────────

#[test]
fn reference_delta_values_are_pinned() {
    // Values recomputed from the published `colorDelta` constants.
    approx(
        ref_delta([0, 0, 0, 255], [255, 255, 255, 255], 0, false),
        32857.13315714265,
        1e-9,
    );
    approx(
        ref_delta([0, 0, 0, 0], [255, 255, 255, 255], 0, false),
        21651.600133032,
        1e-9,
    );
    approx(
        ref_delta([0, 0, 0, 0], [0, 0, 0, 255], 0, false),
        -1164.2112232842244,
        1e-9,
    );
    approx(
        ref_delta([0, 0, 0, 0], [0, 0, 0, 255], 4, false),
        -3476.389758473968,
        1e-9,
    );
}

#[test]
fn threshold_boundary_is_strict_greater_than() {
    // `|delta| > 35215 * threshold^2` (ref index.js:56,73). Bracketing the
    // ratio pins the production delta to the reference formula.
    let a = [0u8, 0, 0, 255];
    let b = [255u8, 255, 255, 255];
    let delta = ref_delta(a, b, 0, false).abs();
    let t = (delta / 35215.0).sqrt();
    assert!(t < 1.0, "sanity: {t}");

    let lower = DiffOptions {
        threshold: t * (1.0 - 1e-6),
        include_aa: true,
    };
    let upper = DiffOptions {
        threshold: t * (1.0 + 1e-6),
        include_aa: true,
    };

    let counted_r = compare_rgba(&a, &b, 1, 1, &lower);
    let skipped_r = compare_rgba(&a, &b, 1, 1, &upper);

    assert_eq!(counted_r.diff_pixels, 1, "delta just above max_delta counts");
    assert_eq!(skipped_r.diff_pixels, 0, "delta just below max_delta is ignored");
    assert_eq!(counted_r.diff_pct, 100.0);
    assert_eq!(skipped_r.match_pct, 100.0);
}

#[test]
fn threshold_zero_counts_any_colour_difference() {
    let a = [255u8, 255, 255, 255];
    let b = [254u8, 254, 254, 255];
    let delta = ref_delta(a, b, 0, false);
    assert!(delta != 0.0 && delta.abs() < 1.0, "subtle delta {delta}");

    let exact = DiffOptions {
        threshold: 0.0,
        include_aa: true,
    };
    assert_eq!(compare_rgba(&a, &b, 1, 1, &exact).diff_pixels, 1);
    assert_eq!(compare_rgba(&a, &b, 1, 1, &DiffOptions::default()).diff_pixels, 0);
}

#[test]
fn threshold_one_is_never_exceeded() {
    // 35215 is the maximum possible YIQ delta, so `threshold = 1` counts
    // nothing even for black vs white (the most extreme pair).
    let a = [0u8, 0, 0, 255];
    let b = [255u8, 255, 255, 255];
    let lenient = DiffOptions {
        threshold: 1.0,
        include_aa: true,
    };
    let r = compare_rgba(&a, &b, 1, 1, &lenient);
    assert_eq!(r.diff_pixels, 0);
    assert_eq!(r.match_pct, 100.0);
}

// ── Alpha / checkerboard branch ──────────────────────────────────────────────

#[test]
fn transparent_rgb_differences_are_invisible() {
    // ref index.js:226-233: with a1 = a2 = 0 and da = 0 every blended
    // component is 0, so two fully transparent pixels are equal regardless of
    // their stored RGB (the 32-bit fast path does not apply: words differ).
    let a = [10u8, 20, 30, 0];
    let b = [200u8, 100, 5, 0];
    assert_ne!(a, b);
    assert_eq!(ref_delta(a, b, 0, false), 0.0);

    let exact = DiffOptions {
        threshold: 0.0,
        include_aa: true,
    };
    let r = compare_rgba(&a, &b, 1, 1, &exact);
    assert_eq!(r.diff_pixels, 0);
    // The raw channels still differ, so the continuous score is non-zero.
    approx(r.mean_abs_diff, (190.0 + 80.0 + 25.0) / (4.0 * 255.0), 1e-15);
}

#[test]
fn alpha_checker_depends_on_byte_offset() {
    // Transparent black vs opaque black: only the checkerboard background
    // differs per position, and the parity strides with the golden ratio
    // (ref index.js:227-229), so the same colour pair is counted on some
    // pixels and not on others at threshold 0.20.
    let w = 4;
    let a = solid(w, 1, [0, 0, 0, 0]);
    let b = solid(w, 1, [0, 0, 0, 255]);

    let o = DiffOptions::default();
    let r = compare_rgba(&a, &b, w, 1, &o);
    let max_delta = 35215.0 * o.threshold * o.threshold;

    let mut expected = Vec::new();
    for i in 0..w as usize {
        let delta = ref_delta([0, 0, 0, 0], [0, 0, 0, 255], i * 4, false).abs();
        expected.push(delta > max_delta);
    }
    assert_eq!(expected, vec![false, true, true, true], "checker parity sanity");
    assert_eq!(r.diff_pixels, 3);
    assert_eq!(r.diff_mask, vec![0, 255, 255, 255]);
}

// ── Anti-aliasing ────────────────────────────────────────────────────────────

#[test]
fn anti_aliasing_counts_with_include_aa() {
    // 5x5 image whose centre (2,2) is 128 grey over 200 grey, with a single
    // 60 grey neighbour. (2,2) has both brighter and darker siblings, and the
    // brightest sibling (1,1) sits in a flat 200-grey region, so the reference
    // heuristic reports it as anti-aliasing.
    let w = 5;
    let h = 5;
    let mut base = solid(w, h, [200, 200, 200, 255]);
    put(&mut base, w, 2, 2, [128, 128, 128, 255]);
    put(&mut base, w, 3, 3, [60, 60, 60, 255]);

    let mut changed = base.clone();
    put(&mut changed, w, 2, 2, [240, 240, 240, 255]);

    // include_aa: true (VRT default) -> the AA pixel counts (ref index.js:73-85).
    let strict = compare_rgba(&base, &changed, w, h, &DiffOptions::default());
    assert_eq!(strict.diff_pixels, 1);
    assert_eq!(strict.diff_mask[2 * 5 + 2], 255);

    // include_aa: false -> detected AA is drawn yellow and not counted.
    let lenient = DiffOptions {
        include_aa: false,
        ..DiffOptions::default()
    };
    let r = compare_rgba(&base, &changed, w, h, &lenient);
    assert_eq!(r.diff_pixels, 0);
    assert_eq!(counted(&r), 0);
    assert_eq!(r.match_pct, 100.0);
}

#[test]
fn flat_colour_change_is_never_treated_as_anti_aliasing() {
    // Same size change but on a uniform image: no brighter/darker siblings
    // (ref index.js:165), so include_aa does not matter.
    let w = 4;
    let h = 4;
    let base = solid(w, h, [128, 128, 128, 255]);
    let mut changed = base.clone();
    put(&mut changed, w, 1, 1, [200, 200, 200, 255]);

    for include_aa in [true, false] {
        let o = DiffOptions {
            include_aa,
            ..DiffOptions::default()
        };
        let r = compare_rgba(&base, &changed, w, h, &o);
        assert_eq!(r.diff_pixels, 1, "include_aa={include_aa}");
    }
}

#[test]
fn alpha_aa_checker_phase_follows_the_centre_pixel() {
    // ref `pixelmatch@7.1.0/index.js:166`:
    //   colorDelta(img, img, pos * 4, (y * width + x) * 4, true)
    // `k` — the *centre* byte offset — drives the 48/159 checkerboard phase,
    // not the neighbour offset `m`. This 3x3 pair is scored 8 by the
    // reference at threshold 0.20 with `includeAA: false`; keying the phase to
    // the neighbour instead loses two pixels and yields 6.
    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const GREY: [u8; 4] = [128, 128, 128, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];
    const GREY_CLEAR: [u8; 4] = [128, 128, 128, 0];

    let mut a = Vec::new();
    for p in [WHITE, WHITE, WHITE, WHITE, GREY, CLEAR, GREY, GREY, GREY] {
        a.extend_from_slice(&p);
    }
    let mut b = Vec::new();
    for p in [GREY_CLEAR, GREY, GREY, GREY, WHITE, GREY, GREY, GREY_CLEAR, BLACK] {
        b.extend_from_slice(&p);
    }

    let lenient = DiffOptions {
        include_aa: false,
        ..DiffOptions::default()
    };
    assert_eq!(compare_rgba(&a, &b, 3, 3, &lenient).diff_pixels, 8);
    // Without AA detection the count is the same for this pair.
    assert_eq!(compare_rgba(&a, &b, 3, 3, &DiffOptions::default()).diff_pixels, 8);
}

// ── Padding / size mismatch ──────────────────────────────────────────────────

#[test]
fn pad_to_white_fills_background() {
    let src = vec![255u8, 0, 0, 255];
    let padded = pad_to_white(&src, 1, 1, 3, 2);

    assert_eq!(padded.len(), 3 * 2 * 4);
    assert_eq!(&padded[0..4], &[255, 0, 0, 255]);
    assert!(padded[4..].iter().all(|b| *b == 255));
}

#[test]
fn pad_to_white_is_a_noop_when_sizes_match() {
    let src = solid(2, 2, [1, 2, 3, 4]);
    assert_eq!(pad_to_white(&src, 2, 2, 2, 2), src);
}

#[test]
fn pad_to_white_clips_oversized_source() {
    let mut src = solid(4, 2, [9, 9, 9, 255]);
    put(&mut src, 4, 0, 0, [1, 1, 1, 255]);
    put(&mut src, 4, 1, 0, [2, 2, 2, 255]);

    let padded = pad_to_white(&src, 4, 2, 2, 1);
    assert_eq!(padded, vec![1, 1, 1, 255, 2, 2, 2, 255]);
}

#[test]
fn size_mismatch_via_pad_to_white() {
    // ref visual.spec.ts:149-168: both images are padded to the max extent,
    // then compared over `max_w * max_h` pixels.
    let small = solid(4, 4, [255, 255, 255, 255]);
    let mut large = solid(8, 8, [0, 0, 0, 255]);
    let white_patch = solid(4, 4, [255, 255, 255, 255]);
    for y in 0..4usize {
        for x in 0..4usize {
            let src = (y * 4 + x) * 4;
            let dst = (y * 8 + x) * 4;
            large[dst..dst + 4].copy_from_slice(&white_patch[src..src + 4]);
        }
    }

    let padded_small = pad_to_white(&small, 4, 4, 8, 8);
    assert_eq!(padded_small.len(), 8 * 8 * 4);

    let r = compare_rgba(&padded_small, &large, 8, 8, &DiffOptions::default());
    assert_eq!(r.total_pixels, 64);
    assert_eq!(r.diff_pixels, 48, "the 6x8 black surround");
    approx(r.diff_pct, 75.0, 1e-12);
    approx(r.match_pct, 25.0, 1e-12);
    assert_eq!(counted(&r), 48);
}

// ── Robustness ───────────────────────────────────────────────────────────────

#[test]
fn short_buffers_read_as_white() {
    // Missing bytes are treated as opaque white (same fill as `pad_to_white`),
    // so an empty buffer equals a white image.
    let white = solid(2, 1, [255, 255, 255, 255]);
    let r = compare_rgba(&[], &white, 2, 1, &DiffOptions::default());
    assert_eq!(r.diff_pixels, 0);
    assert_eq!(r.mean_abs_diff, 0.0);

    // A 3-byte buffer yields pixel 0 = (1, 2, 3, 255) and pixel 1 = white.
    let partial = [1u8, 2, 3];
    let r = compare_rgba(&partial, &white, 2, 1, &DiffOptions::default());
    assert_eq!(r.diff_pixels, 1);
    assert_eq!(r.diff_mask, vec![255, 0]);
    approx(r.mean_abs_diff, 759.0 / (2.0 * 4.0 * 255.0), 1e-15);
}

#[test]
fn empty_image_yields_no_diffs() {
    let r = compare_rgba(&[], &[], 0, 0, &DiffOptions::default());
    assert_eq!(r.diff_pixels, 0);
    assert_eq!(r.total_pixels, 0);
    assert!(r.diff_mask.is_empty());
    assert_eq!(r.match_pct, 100.0);
    assert_eq!(r.diff_pct, 0.0);
    assert_eq!(r.mean_abs_diff, 0.0);
    assert!(!r.match_pct.is_nan());
}
