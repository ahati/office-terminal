use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use crate::contracts::*;
use crate::compiler::SlideCompiler;
use crate::rasterizer::SlideRasterizer;
use crate::loader::AssetStore;

pub struct DefaultRenderPipeline {
    compiler: Arc<dyn SlideCompiler>,
    rasterizer: Arc<dyn SlideRasterizer>,
    presentation: Arc<pptx_model::Presentation>,
    asset_store: Arc<dyn AssetStore>,
    generation: AtomicU64,
    vector_cache: Mutex<HashMap<usize, VectorSlide>>,
    raster_cache: Mutex<HashMap<(usize, RenderTargetSpec), RgbaSlide>>,
}

impl DefaultRenderPipeline {
    pub fn new(
        presentation: Arc<pptx_model::Presentation>,
        asset_store: Arc<dyn AssetStore>,
        compiler: Arc<dyn SlideCompiler>,
        rasterizer: Arc<dyn SlideRasterizer>,
    ) -> Self {
        Self {
            compiler,
            rasterizer,
            presentation,
            asset_store,
            generation: AtomicU64::new(0),
            vector_cache: Mutex::new(HashMap::new()),
            raster_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

impl RenderPipeline for DefaultRenderPipeline {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec) {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let total_slides = self.presentation.slides.len();
        if total_slides == 0 { return; }

        let center = plan.center;
        let mut indices = Vec::new();
        indices.push(center);
        if center + 1 < total_slides { indices.push(center + 1); }
        if center > 0 { indices.push(center - 1); }
        if center + 2 < total_slides { indices.push(center + 2); }
        if center >= 2 { indices.push(center - 2); }

        for idx in indices {
            // Check vector cache first
            let mut vcache = self.vector_cache.lock().unwrap();
            let vector_slide = vcache.entry(idx).or_insert_with(|| {
                let slide = &self.presentation.slides[idx];
                self.compiler.compile(slide, self.presentation.slide_width, self.presentation.slide_height, &tx).unwrap()
            }).clone();
            drop(vcache);

            // Check raster cache
            let mut rcache = self.raster_cache.lock().unwrap();
            if let std::collections::hash_map::Entry::Vacant(e) = rcache.entry((idx, *target)) {
                if let Ok(bmp) = self.rasterizer.rasterize(&vector_slide, target, self.asset_store.as_ref()) {
                    e.insert(bmp);
                }
            }
        }
    }

    fn entry(&self, index: usize) -> PipelineEntry {
        let dummy_spec = RenderTargetSpec { width_px: 1920, height_px: 1080 };
        let rcache = self.raster_cache.lock().unwrap();
        if let Some(bmp) = rcache.get(&(index, dummy_spec)) {
            PipelineEntry::Ready {
                bitmap: bmp.clone(),
                staged: false,
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }

    fn invalidate(&self, cause: InvalidateCause) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        match cause {
            InvalidateCause::Resize => {}
            InvalidateCause::TargetChanged => {
                let mut rcache = self.raster_cache.lock().unwrap();
                rcache.clear();
            }
            InvalidateCause::DeckReload => {
                let mut vcache = self.vector_cache.lock().unwrap();
                let mut rcache = self.raster_cache.lock().unwrap();
                vcache.clear();
                rcache.clear();
            }
        }
    }
}
