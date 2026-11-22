//! Pass-level scratch pooling for the shape-effect passes (issue #33).
//!
//! One [`EffectScratch`] lives for a whole [`super::SlideRasterizer::rasterize`]
//! pass and is threaded through the command loop into the effect renderers.
//! Every shadowed / glowing shape previously allocated a bbox-sized layer
//! pixmap (capped at [`super::shape_effect::MAX_LAYER_PIXELS`] pixels) plus
//! fresh blur working buffers; the pooled buffers keep that peak allocation
//! once per pass instead of once per shape. Reuse is grow-only: a buffer keeps
//! its capacity after a large shape and is only `resize`d per use, and layer
//! pixel requests round capacity up to 64 KiB buckets so the allocations
//! stabilise after the first few shapes. No algorithm changes — the pixel
//! behaviour is identical, only the allocations are reused.

/// Capacity granularity for pooled buffers, so repeated `resize`s settle.
const BUCKET_BYTES: usize = 64 * 1024;

/// Round a byte length up to the bucket granularity.
fn bucket(len: usize) -> usize {
    len.next_multiple_of(BUCKET_BYTES)
}

/// Pooled working buffers shared by the effect renders of one raster pass.
#[derive(Default)]
pub(crate) struct EffectScratch {
    /// Premultiplied RGBA storage for the bbox-sized effect layer: handed to
    /// `tiny_skia::Pixmap::from_vec` by [`EffectScratch::take_layer`] and
    /// reclaimed with `Pixmap::take` via [`EffectScratch::return_layer`].
    pub(crate) pixels: Vec<u8>,
    /// Single-channel blur ping-pong buffers (1 byte per pixel).
    pub(crate) alpha_a: Vec<u8>,
    pub(crate) alpha_b: Vec<u8>,
    /// RGBA blur ping-pong buffers (4 bytes per pixel).
    pub(crate) rgba_a: Vec<u8>,
    pub(crate) rgba_b: Vec<u8>,
    /// Sliding-window prefix sums shared by the box-blur kernels.
    pub(crate) prefix: Vec<u32>,
}

impl EffectScratch {
    /// Take the pooled layer buffer as a cleared `width x height` pixmap.
    ///
    /// The buffer is zeroed here (not by `from_vec`) so the caller sees the
    /// same fully-transparent layer `Pixmap::new` produced. `None` mirrors
    /// `Pixmap::new`'s failure on absurd dimensions.
    pub(crate) fn take_layer(&mut self, width: u32, height: u32) -> Option<tiny_skia::Pixmap> {
        let size = tiny_skia::IntSize::from_wh(width, height)?;
        let len = width as usize * height as usize * 4;
        reserve_bucketed(&mut self.pixels, len);
        self.pixels.resize(len, 0);
        self.pixels.fill(0);
        tiny_skia::Pixmap::from_vec(std::mem::take(&mut self.pixels), size)
    }

    /// Return a layer pixmap previously produced by [`EffectScratch::take_layer`].
    pub(crate) fn return_layer(&mut self, layer: tiny_skia::Pixmap) {
        self.pixels = layer.take();
    }
}

/// Ensure a buffer's capacity covers `len`, rounding up to a bucket so a run
/// of similar shapes stops reallocating early. Never shrinks.
fn reserve_bucketed(buf: &mut Vec<u8>, len: usize) {
    if buf.capacity() < len {
        buf.reserve(bucket(len) - buf.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_round_trips_and_clears() {
        let mut scratch = EffectScratch::default();
        let mut layer = scratch.take_layer(4, 3).expect("4x3 layer");
        // Dirty the layer like a render pass would, then hand it back.
        layer
            .data_mut()
            .fill(0xFF);
        scratch.return_layer(layer);

        let layer = scratch.take_layer(4, 3).expect("reused 4x3 layer");
        assert!(
            layer.data().iter().all(|&byte| byte == 0),
            "a recycled layer must come back cleared"
        );
        assert_eq!(layer.width(), 4);
        assert_eq!(layer.height(), 3);
    }

    #[test]
    fn reuse_keeps_capacity_across_smaller_shapes() {
        let mut scratch = EffectScratch::default();
        let big = scratch.take_layer(64, 64).expect("big layer");
        scratch.return_layer(big);
        let capacity = scratch.pixels.capacity();
        assert!(capacity >= 64 * 64 * 4);

        // A smaller shape must not reallocate (grow-only reuse).
        let small = scratch.take_layer(4, 4).expect("small layer");
        scratch.return_layer(small);
        assert_eq!(
            scratch.pixels.capacity(),
            capacity,
            "shrinking layer requests must keep the pooled capacity"
        );
    }

    #[test]
    fn rejects_degenerate_layers() {
        let mut scratch = EffectScratch::default();
        assert!(scratch.take_layer(0, 4).is_none());
        assert!(scratch.take_layer(4, 0).is_none());
    }

    #[test]
    fn buckets_round_up() {
        assert_eq!(bucket(1), BUCKET_BYTES);
        assert_eq!(bucket(BUCKET_BYTES), BUCKET_BYTES);
        assert_eq!(bucket(BUCKET_BYTES + 1), BUCKET_BYTES * 2);
    }
}
