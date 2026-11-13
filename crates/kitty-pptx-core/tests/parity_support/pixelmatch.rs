//! Bit-exact port of `pixelmatch@7.1.0` (ISC, Mapbox) as used by the reference
//! visual-regression harness.
//!
//! ref `packages/pptx/tests/visual/visual.spec.ts:170-183`:
//!
//! ```text
//! const diffPixels = pixelmatch(refPadded.data, actualPadded.data, diff.data, w, h,
//!                               { threshold: PIXEL_THRESHOLD, includeAA: true });
//! const diffPct  = (diffPixels / totalPx) * 100;
//! const matchPct = 100 - diffPct;
//! ```
//!
//! with `PIXEL_THRESHOLD = 0.20` (`visual.spec.ts:39`). The reference pins
//! `pixelmatch@^7.1.0` (`packages/pptx/package.json:52`), so the arithmetic
//! below is transcribed verbatim from `pixelmatch@7.1.0/index.js`
//! (`colorDelta`, `antialiased`, `hasManySiblings`). Every intermediate value
//! is an IEEE-754 double, exactly like JavaScript's `Number`, and rustc does
//! not contract `a * b + c` into an FMA without `fast-math`, so the emitted
//! deltas are bit-identical to the JS implementation.
//!
//! Deliberate differences from the reference, all output-neutral:
//! - No diff-image painting (`output`), no `alpha`/`aaColor`/`diffColor`
//!   options: the VRT harness only consumes the mismatch count. `diff_mask`
//!   is the same information in boolean-per-pixel form.
//! - Anti-aliasing detection is only evaluated when `include_aa == false`
//!   (the reference evaluates it eagerly but discards it when `includeAA` is
//!   set, which is the VRT default).
//! - Uneven image sizes are handled by `pad_to_white`, mirroring the reference
//!   `pad()` helper (`visual.spec.ts:159-168`), instead of throwing.
#![allow(dead_code)]

/// `maxDelta` base: the maximum possible squared YIQ distance.
/// ref `pixelmatch@7.1.0/index.js:56` ("35215 is the maximum possible value
/// for the YIQ difference metric").
const MAX_YIQ_DELTA: f64 = 35215.0;

/// Alpha checkerboard stride (`colorDelta`, `pixelmatch@7.1.0/index.js:227`).
/// The reference literal is `1.618033988749895`, which is bit-identical to
/// `std::f64::consts::GOLDEN_RATIO`.
const PHI: f64 = std::f64::consts::GOLDEN_RATIO;
/// Second checkerboard stride; `phi^2` truncated to the reference literal.
const PHI2: f64 = 2.618_033_988_749_895;

/// Per-pixel comparison options.
///
/// The VRT harness always uses the defaults below
/// (`{ threshold: 0.20, includeAA: true }`, `visual.spec.ts:176`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffOptions {
    /// Matching threshold in `0.0..=1.0`; smaller is more sensitive.
    /// ref `pixelmatch@7.1.0/index.js:23` (`threshold = 0.1` default,
    /// overridden to `0.20` by the VRT).
    pub threshold: f64,
    /// When `true`, anti-aliased pixels are counted as differences (the
    /// reference's `includeAA: true` for VRT, which is *stricter* than the
    /// library default of `false`).
    pub include_aa: bool,
}

impl Default for DiffOptions {
    /// VRT defaults: `{ threshold: 0.20, includeAA: true }`.
    /// ref `packages/pptx/tests/visual/visual.spec.ts:39,176`.
    fn default() -> Self {
        Self {
            threshold: 0.20,
            include_aa: true,
        }
    }
}

/// Result of comparing two RGBA buffers.
#[derive(Clone, Debug)]
pub struct DiffResult {
    /// Number of pixels counted as different (the reference's return value).
    pub diff_pixels: usize,
    /// `width * height`.
    pub total_pixels: usize,
    /// `100 - diff_pct`; `100.0` for identical images.
    /// ref `visual.spec.ts:183`.
    pub match_pct: f64,
    /// `diff_pixels / total_pixels * 100`.
    /// ref `visual.spec.ts:182`.
    pub diff_pct: f64,
    /// Mean absolute per-channel difference over all RGBA channels of all
    /// pixels, normalised to `0.0..=1.0` (i.e. divided by `255`). Computed on
    /// the *raw* channels, before the alpha checkerboard blend, so it is a
    /// continuous magnitude signal alongside the thresholded percentages.
    pub mean_abs_diff: f64,
    /// One byte per pixel in row-major order: `255` where the pixel counted as
    /// a difference, `0` elsewhere (anti-aliased pixels excluded when
    /// `include_aa == false`, matching the reference's AA-mask note at
    /// `pixelmatch@7.1.0/index.js:78`).
    pub diff_mask: Vec<u8>,
}

/// Compares two RGBA8 buffers of `w * h` pixels.
///
/// Semantics match `pixelmatch@7.1.0` with `output = null`:
/// - `max_delta = 35215 * threshold^2` (`index.js:56`).
/// - Pixel pairs whose 32-bit RGBA words are equal short-circuit to delta `0`
///   (`index.js:70`).
/// - Otherwise the signed YIQ delta is compared with `|delta| > max_delta`
///   (strictly greater, `index.js:73`).
/// - With `include_aa == false`, pixels detected as anti-aliasing by the
///   8-neighbour luma-slope heuristic are not counted (`index.js:73-85`).
///
/// Buffers shorter than `w * h * 4` are read as opaque white (`255`) past the
/// end rather than panicking; this mirrors `pad_to_white` / the reference
/// `pad()` helper filling with `255` and keeps the harness total.
pub fn compare_rgba(a: &[u8], b: &[u8], w: u32, h: u32, o: &DiffOptions) -> DiffResult {
    let width = w as usize;
    let height = h as usize;
    let total = width.saturating_mul(height);
    if total == 0 {
        return DiffResult {
            diff_pixels: 0,
            total_pixels: 0,
            match_pct: 100.0,
            diff_pct: 0.0,
            mean_abs_diff: 0.0,
            diff_mask: Vec::new(),
        };
    }

    // ref `pixelmatch@7.1.0/index.js:56`.
    let max_delta = MAX_YIQ_DELTA * o.threshold * o.threshold;
    let mut diff_mask = vec![0u8; total];
    let mut diff_pixels = 0usize;
    let mut abs_sum = 0u64;

    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let pos = i * 4;
            let pa = px4(a, pos);
            let pb = px4(b, pos);

            for (ca, cb) in pa.iter().zip(pb.iter()) {
                abs_sum += u64::from(ca.abs_diff(*cb));
            }

            // ref `index.js:70`: identical 32-bit words contribute delta 0.
            let delta = if pa == pb {
                0.0
            } else {
                color_delta(pa, pb, pos, false)
            };

            if delta.abs() > max_delta {
                // ref `index.js:73-76`: `isAA` is only consumed when `includeAA`
                // is false, so it is evaluated lazily here.
                let is_aa = !o.include_aa
                    && (antialiased(a, x, y, width, height, b)
                        || antialiased(b, x, y, width, height, a));
                if !is_aa {
                    diff_mask[i] = 255;
                    diff_pixels += 1;
                }
            }
        }
    }

    // ref `visual.spec.ts:182-183`.
    let diff_pct = diff_pixels as f64 / total as f64 * 100.0;
    let match_pct = 100.0 - diff_pct;
    let mean_abs_diff = abs_sum as f64 / (total as f64 * 4.0 * 255.0);

    DiffResult {
        diff_pixels,
        total_pixels: total,
        match_pct,
        diff_pct,
        mean_abs_diff,
        diff_mask,
    }
}

/// Copies `px` (`w * h`) into a white `tw * th` RGBA buffer, top-left aligned.
///
/// ref `packages/pptx/tests/visual/visual.spec.ts:159-168`: `pad()` allocates
/// a `tw * th` PNG, fills it with `255` and `bitblt`s the source at `(0, 0)`.
/// A target smaller than the source clips (the reference never does that
/// because it pads to `max(actual, ref)` on both axes).
pub fn pad_to_white(px: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let tw = tw as usize;
    let th = th as usize;
    let mut out = vec![255u8; tw.saturating_mul(th).saturating_mul(4)];

    let cols = (w as usize).min(tw);
    let rows = (h as usize).min(th);
    let copy_len = cols * 4;

    for y in 0..rows {
        let src = y * w as usize * 4;
        if src >= px.len() {
            break;
        }
        let n = copy_len.min(px.len() - src);
        out[y * tw * 4..y * tw * 4 + n].copy_from_slice(&px[src..src + n]);
    }

    out
}

/// Reads `pos .. pos + 4` as RGBA; missing bytes read as opaque white.
#[inline]
fn px4(buf: &[u8], pos: usize) -> [u8; 4] {
    [
        ch(buf, pos),
        ch(buf, pos + 1),
        ch(buf, pos + 2),
        ch(buf, pos + 3),
    ]
}

#[inline]
fn ch(buf: &[u8], i: usize) -> u8 {
    buf.get(i).copied().unwrap_or(255)
}

/// Signed squared YIQ distance; negative when the first pixel is brighter.
///
/// ref `pixelmatch@7.1.0/index.js:209-246` (`colorDelta`).
fn color_delta(p1: [u8; 4], p2: [u8; 4], byte_offset: usize, y_only: bool) -> f64 {
    let [r1, g1, b1, a1] = p1.map(f64::from);
    let [r2, g2, b2, a2] = p2.map(f64::from);

    let mut dr = r1 - r2;
    let mut dg = g1 - g2;
    let mut db = b1 - b2;
    let da = a1 - a2;

    if dr == 0.0 && dg == 0.0 && db == 0.0 && da == 0.0 {
        // ref `index.js:224`.
        return 0.0;
    }

    if a1 < 255.0 || a2 < 255.0 {
        // ref `index.js:226-233`: blend both pixels over the checkerboard
        // background `48/159` whose parity advances with the golden ratio.
        let rb = 48.0 + 159.0 * (byte_offset % 2) as f64;
        let gb = 48.0 + 159.0 * (trunc_div(byte_offset, PHI) % 2) as f64;
        let bb = 48.0 + 159.0 * (trunc_div(byte_offset, PHI2) % 2) as f64;
        dr = (r1 * a1 - r2 * a2 - rb * da) / 255.0;
        dg = (g1 * a1 - g2 * a2 - gb * da) / 255.0;
        db = (b1 * a1 - b2 * a2 - bb * da) / 255.0;
    }

    // ref `index.js:235`: luma.
    let y = dr * 0.29889531 + dg * 0.58662247 + db * 0.11448223;
    if y_only {
        return y;
    }

    // ref `index.js:239-240`: chroma.
    let i = dr * 0.59597799 - dg * 0.27417610 - db * 0.32180189;
    let q = dr * 0.21147017 - dg * 0.52261711 + db * 0.31114694;
    let delta = 0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q;

    // ref `index.js:242-245`: `y > 0 ? -delta : delta`.
    if y > 0.0 {
        -delta
    } else {
        delta
    }
}

/// JS `(n / divisor) | 0`: truncate toward zero, then twos-complement int.
#[inline]
fn trunc_div(n: usize, divisor: f64) -> i64 {
    (n as f64 / divisor).trunc() as i64
}

/// Anti-aliasing heuristic over the 8 neighbours of `(x1, y1)`.
///
/// ref `pixelmatch@7.1.0/index.js:121-171` (`antialiased`). Neighbour scan
/// order (`x` outer, `y` inner, ascending) is preserved because ties on
/// `min`/`max` keep the first candidate, which decides `has_many_siblings`.
fn antialiased(
    img: &[u8],
    x1: usize,
    y1: usize,
    width: usize,
    height: usize,
    other: &[u8],
) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    let (x0, y0, x2, y2) = neighbourhood(x1, y1, width, height);
    let center_pos = (y1 * width + x1) * 4;
    let center = px4(img, center_pos);

    // ref `index.js:127`: a border pixel already has one "equal sibling".
    let mut zeroes = if x1 == x0 || x1 == x2 || y1 == y0 || y1 == y2 {
        1
    } else {
        0
    };
    let mut min = 0.0f64;
    let mut max = 0.0f64;
    let mut min_xy = (0usize, 0usize);
    let mut max_xy = (0usize, 0usize);

    for x in x0..=x2 {
        for y in y0..=y2 {
            if x == x1 && y == y1 {
                continue;
            }
            // ref `index.js:141`: `colorDelta(img, img, pos * 4, neighbour * 4,
            // true)` — the 48/159 checkerboard phase is keyed to the centre
            // pixel (`k = pos * 4`), not to the neighbour (`m`).
            let delta = color_delta(center, px4(img, (y * width + x) * 4), center_pos, true);
            if delta == 0.0 {
                zeroes += 1;
                // ref `index.js:147`: more than 2 equal siblings is not AA.
                if zeroes > 2 {
                    return false;
                }
            } else if delta < min {
                min = delta;
                min_xy = (x, y);
            } else if delta > max {
                max = delta;
                max_xy = (x, y);
            }
        }
    }

    // ref `index.js:165`: needs both a brighter and a darker sibling.
    if min == 0.0 || max == 0.0 {
        return false;
    }

    // ref `index.js:169-170`: the extreme must sit in a flat region in *both*
    // images at that coordinate.
    let flat_in_both = |(x, y): (usize, usize)| {
        has_many_siblings(img, x, y, width, height)
            && has_many_siblings(other, x, y, width, height)
    };
    flat_in_both(min_xy) || flat_in_both(max_xy)
}

/// `true` when `(x1, y1)` has more than two identical 8-neighbours.
///
/// ref `pixelmatch@7.1.0/index.js:181-206` (`hasManySiblings`). The reference
/// compares 32-bit words, so RGBA bytes are compared as a tuple here.
fn has_many_siblings(img: &[u8], x1: usize, y1: usize, width: usize, height: usize) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    let (x0, y0, x2, y2) = neighbourhood(x1, y1, width, height);
    let val = px4(img, (y1 * width + x1) * 4);
    // ref `index.js:187`: a border pixel already has one "equal sibling".
    let mut zeroes = if x1 == x0 || x1 == x2 || y1 == y0 || y1 == y2 {
        1
    } else {
        0
    };

    for x in x0..=x2 {
        for y in y0..=y2 {
            if x == x1 && y == y1 {
                continue;
            }
            if px4(img, (y * width + x) * 4) == val {
                zeroes += 1;
            }
            if zeroes > 2 {
                return true;
            }
        }
    }

    false
}

/// Clamped `(x0, y0, x2, y2)` 3x3 window around `(x1, y1)`.
#[inline]
fn neighbourhood(x1: usize, y1: usize, width: usize, height: usize) -> (usize, usize, usize, usize) {
    (
        x1.saturating_sub(1),
        y1.saturating_sub(1),
        (x1 + 1).min(width - 1),
        (y1 + 1).min(height - 1),
    )
}
