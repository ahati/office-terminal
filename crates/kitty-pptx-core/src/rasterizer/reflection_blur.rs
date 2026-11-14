//! Distance-aware reflection blur (S8).
//!
//! PowerPoint keeps a floor reflection sharp where it meets the source and
//! increases the authored blur toward the far edge. The reference approximates
//! that with narrow monotonic bands, each drawing the sharp source with a
//! larger filter radius. Port of `packages/pptx/src/reflection-blur.ts`.

/// One vertical band of the reflection source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionBlurBand {
    pub y: f32,
    pub h: f32,
    pub radius: f32,
}

/// Half-device-pixel radius step; finer sampling only repeats bitmap copies.
const TARGET_RADIUS_STEP_PX: f32 = 0.5;
/// Hard cap on band passes (one slide may have many reflections).
const MAX_BANDS: usize = 24;

/// Split `bounds` (`x`, `y`, `w`, `h`) into equal-radius-increment bands whose
/// radius grows quadratically away from the bottom edge. The first band is
/// sharp; the last reaches `max_blur`.
pub fn reflection_blur_bands(bounds: (f32, f32, f32, f32), max_blur: f32) -> Vec<ReflectionBlurBand> {
    let (_, y, _, h) = bounds;
    if max_blur <= 0.0 || h <= 0.0 {
        return vec![ReflectionBlurBand {
            y,
            h: h.max(0.0),
            radius: 0.0,
        }];
    }
    let count = (max_blur / TARGET_RADIUS_STEP_PX).ceil() as usize + 1;
    let count = count.clamp(4, MAX_BANDS);
    let bottom = y + h;
    let denominator = (count - 1) as f32;
    let mut bands = Vec::with_capacity(count);
    for index in 0..count {
        let sample = (index as f32 / denominator).sqrt();
        let previous = if index == 0 {
            0.0
        } else {
            ((index - 1) as f32 / denominator).sqrt()
        };
        let next = if index + 1 == count {
            1.0
        } else {
            ((index + 1) as f32 / denominator).sqrt()
        };
        let near = if index == 0 { 0.0 } else { (previous + sample) * 0.5 };
        let far = if index + 1 == count { 1.0 } else { (sample + next) * 0.5 };
        bands.push(ReflectionBlurBand {
            y: bottom - far * h,
            h: (far - near) * h,
            radius: max_blur * index as f32 / denominator,
        });
    }
    bands
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radii(bands: &[ReflectionBlurBand]) -> Vec<f32> {
        bands.iter().map(|band| band.radius).collect()
    }

    #[test]
    fn contact_band_is_sharp_and_far_edge_reaches_the_authored_blur() {
        let bands = reflection_blur_bands((10.0, 20.0, 100.0, 40.0), 2.0);
        assert_eq!(bands.len(), 5);
        assert_eq!(bands[0].radius, 0.0);
        assert!((bands.last().unwrap().radius - 2.0).abs() < 1e-6);
        assert!(radii(&bands).windows(2).all(|pair| pair[1] >= pair[0]));
        // Radius increments stay uniform even though the spatial bands do not.
        let steps: Vec<f32> = radii(&bands).windows(2).map(|pair| pair[1] - pair[0]).collect();
        let min = steps.iter().copied().fold(f32::MAX, f32::min);
        let max = steps.iter().copied().fold(f32::MIN, f32::max);
        assert!(max - min < 1e-6);
        assert!(max <= 0.5 + 1e-6);
        // Bands stack upward from the source bottom; the first is the narrow
        // contact band and the last reaches the top edge.
        assert!(bands[0].y > 20.0);
        assert!(bands.last().unwrap().y <= 20.0 + 1e-4);
        assert!(bands[0].h > bands.last().unwrap().h);
    }

    #[test]
    fn pass_cap_bounds_the_number_of_bands() {
        let bands = reflection_blur_bands((0.0, 0.0, 10.0, 80.0), 100.0);
        assert_eq!(bands.len(), 24);
        let steps: Vec<f32> = radii(&bands).windows(2).map(|pair| pair[1] - pair[0]).collect();
        for step in steps {
            assert!((step - 100.0 / 23.0).abs() < 1e-4);
        }
    }

    #[test]
    fn no_blur_is_one_sharp_pass() {
        let bands = reflection_blur_bands((0.0, 5.0, 20.0, 10.0), 0.0);
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].radius, 0.0);
        assert_eq!(bands[0].y, 5.0);
    }
}
