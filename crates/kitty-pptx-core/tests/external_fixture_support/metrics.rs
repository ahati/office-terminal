//! Perceptual comparison primitives: RGBA normalisation, box downscale, and
//! cell-based difference metrics.

use std::path::Path;

use kitty_pptx_core::RgbaSlide;

/// Both images are scaled to this width before the cell comparison.
pub const COMPARE_WIDTH: u32 = 400;
const CELL: u32 = 10;
/// Mean per-channel difference at which a cell counts as passing.
pub const CELL_TOLERANCE: f64 = 40.0;

#[derive(Clone, Debug)]
pub struct RgbImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbImage {
    pub fn from_rgba(rgba: &RgbaSlide) -> Self {
        Self::from_raw_rgba(rgba.width, rgba.height, &rgba.pixels)
    }

    fn from_raw_rgba(width: u32, height: u32, data: &[u8]) -> Self {
        let pixels = data
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| {
                let alpha = f64::from(pixel[3]) / 255.0;
                pixel[..3].iter().map(move |channel| {
                    (f64::from(*channel) * alpha + 255.0 * (1.0 - alpha)).round() as u8
                })
            })
            .collect();
        Self {
            width,
            height,
            pixels,
        }
    }

    fn ink_fraction(&self) -> f64 {
        let ink = self
            .pixels
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|pixel| pixel.iter().any(|channel| *channel < 250))
            .count();
        ink as f64 / (self.width as f64 * self.height as f64)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DiffMetrics {
    pub cell_pass_ratio: f64,
    pub global_mean_abs_diff: f64,
    pub worst_cell_diff: f64,
    pub ink_fraction_delta: f64,
    pub cells_x: u32,
    pub cells_y: u32,
}

/// Compare one rendered slide against a reference thumbnail.
///
/// The thumbnail comes from [`super::thumbs::ThumbnailCache`] and is already
/// flattened over white and downscaled to [`COMPARE_WIDTH`]. The freshly
/// rendered slide is the only image that still needs normalisation, so the
/// common path does no 1600 px work here.
pub fn compare_slide(actual: &RgbaSlide, reference: RgbImage) -> DiffMetrics {
    let actual = RgbImage::from_rgba(actual);
    let height = scaled_height(COMPARE_WIDTH, actual.width, actual.height);
    let actual = fit_width(actual, COMPARE_WIDTH, height);
    let reference = fit_width(reference, COMPARE_WIDTH, height);
    compare(&actual, &reference, CELL_TOLERANCE)
}

/// Height that preserves `source_height / source_width` at `width` px.
pub(crate) fn scaled_height(width: u32, source_width: u32, source_height: u32) -> u32 {
    ((f64::from(width) * f64::from(source_height) / f64::from(source_width.max(1))).round()
        as u32)
        .max(1)
}

fn fit_width(image: RgbImage, width: u32, height: u32) -> RgbImage {
    if image.width == width && image.height == height {
        image
    } else {
        resize_box(&image, width, height)
    }
}

pub fn load_reference(path: &Path) -> Result<RgbImage, String> {
    let image = image::open(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?
        .to_rgba8();
    Ok(RgbImage::from_raw_rgba(
        image.width(),
        image.height(),
        image.as_raw(),
    ))
}

/// Box-average `input` into `width` x `height`, so CowG files and dense
/// anti-aliasing are averaged rather than point-sampled.
pub(crate) fn resize_box(input: &RgbImage, width: u32, height: u32) -> RgbImage {
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    let scale_x = input.width as f64 / width as f64;
    let scale_y = input.height as f64 / height as f64;

    for y in 0..height {
        let y0 = (y as f64 * scale_y).floor() as u32;
        let y1 = (((y + 1) as f64 * scale_y).floor() as u32)
            .max(y0 + 1)
            .min(input.height);
        for x in 0..width {
            let x0 = (x as f64 * scale_x).floor() as u32;
            let x1 = (((x + 1) as f64 * scale_x).floor() as u32)
                .max(x0 + 1)
                .min(input.width);
            let mut sums = [0u64; 3];
            let mut count = 0u64;
            for source_y in y0..y1 {
                for source_x in x0..x1 {
                    let offset = ((source_y * input.width + source_x) * 3) as usize;
                    for (channel, sum) in sums.iter_mut().enumerate() {
                        *sum += u64::from(input.pixels[offset + channel]);
                    }
                    count += 1;
                }
            }
            let target = ((y * width + x) * 3) as usize;
            for (channel, value) in pixels[target..target + 3].iter_mut().enumerate() {
                *value = (sums[channel] / count) as u8;
            }
        }
    }
    RgbImage {
        width,
        height,
        pixels,
    }
}

fn compare(actual: &RgbImage, reference: &RgbImage, tolerance: f64) -> DiffMetrics {
    let columns = actual.width.div_ceil(CELL);
    let rows = actual.height.div_ceil(CELL);
    let mut sum = 0.0;
    let mut cells = 0u64;
    let mut passing = 0u64;
    let mut worst: f64 = 0.0;

    for cell_y in 0..rows {
        for cell_x in 0..columns {
            let mut actual_sum = [0u64; 3];
            let mut reference_sum = [0u64; 3];
            let mut count = 0u64;
            for y in cell_y * CELL..((cell_y + 1) * CELL).min(actual.height) {
                for x in cell_x * CELL..((cell_x + 1) * CELL).min(actual.width) {
                    let offset = ((y * actual.width + x) * 3) as usize;
                    for (channel, (actual_sum, reference_sum)) in actual_sum
                        .iter_mut()
                        .zip(reference_sum.iter_mut())
                        .enumerate()
                    {
                        *actual_sum += u64::from(actual.pixels[offset + channel]);
                        *reference_sum += u64::from(reference.pixels[offset + channel]);
                    }
                    count += 1;
                }
            }
            let delta = actual_sum
                .iter()
                .zip(reference_sum.iter())
                .map(|(a, r)| (*a as f64 / count as f64 - *r as f64 / count as f64).abs())
                .sum::<f64>()
                / 3.0;
            sum += delta;
            cells += 1;
            if delta <= tolerance {
                passing += 1;
            }
            worst = worst.max(delta);
        }
    }

    let ink_actual = actual.ink_fraction();
    let ink_reference = reference.ink_fraction();
    DiffMetrics {
        cell_pass_ratio: passing as f64 / cells as f64,
        global_mean_abs_diff: sum / cells as f64,
        worst_cell_diff: worst,
        ink_fraction_delta: (ink_actual - ink_reference).abs(),
        cells_x: columns,
        cells_y: rows,
    }
}
