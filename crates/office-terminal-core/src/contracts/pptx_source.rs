use crossbeam_channel::Sender;
use std::sync::Arc;

use crate::compiler::SlideCompiler;
use crate::contracts::deck::{EmuRect, PageSource, SearchCorpus, TextLine, TocEntry};
use crate::contracts::pipeline::WarningEvent;
use crate::contracts::vector_slide::VectorSlide;
use crate::error::RenderError;
use crate::loader::AssetStore;

/// Search corpus built from PPTX presentation slides.
#[derive(Clone, Debug, Default)]
pub struct PptxSearchCorpus {
    pages: Vec<Vec<TextLine>>,
}

impl PptxSearchCorpus {
    pub fn from_presentation(presentation: &pptx_model::Presentation) -> Self {
        let mut pages = Vec::with_capacity(presentation.slides.len());
        for slide in &presentation.slides {
            let mut lines = Vec::new();
            for element in &slide.elements {
                if let pptx_model::SlideElement::Shape(shape) = element {
                    if let Some(ref body) = shape.text_body {
                        for para in &body.paragraphs {
                            let mut para_text = String::new();
                            for run in &para.runs {
                                if let pptx_model::TextRun::Text(ref data) = run {
                                    para_text.push_str(&data.text);
                                }
                            }
                            let trimmed = para_text.trim();
                            if !trimmed.is_empty() {
                                lines.push(TextLine {
                                    text: trimmed.to_string(),
                                    rect: EmuRect::new(
                                        shape.x as f32,
                                        shape.y as f32,
                                        shape.width as f32,
                                        shape.height as f32,
                                    ),
                                });
                            }
                        }
                    }
                }
            }
            pages.push(lines);
        }
        Self { pages }
    }
}

impl SearchCorpus for PptxSearchCorpus {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page_text(&self, page: usize) -> &[TextLine] {
        self.pages.get(page).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Adapter implementing [`PageSource`] over an OOXML PPTX presentation model.
pub struct PptxPageSource {
    presentation: Arc<pptx_model::Presentation>,
    asset_store: Arc<dyn AssetStore>,
    compiler: Arc<dyn SlideCompiler>,
    corpus: Arc<PptxSearchCorpus>,
}

impl PptxPageSource {
    pub fn new(
        presentation: Arc<pptx_model::Presentation>,
        asset_store: Arc<dyn AssetStore>,
        compiler: Arc<dyn SlideCompiler>,
    ) -> Self {
        let corpus = Arc::new(PptxSearchCorpus::from_presentation(&presentation));
        Self {
            presentation,
            asset_store,
            compiler,
            corpus,
        }
    }

    pub fn presentation(&self) -> &Arc<pptx_model::Presentation> {
        &self.presentation
    }

    pub fn compiler(&self) -> &Arc<dyn SlideCompiler> {
        &self.compiler
    }
}

impl PageSource for PptxPageSource {
    fn len(&self) -> usize {
        self.presentation.slides.len()
    }

    fn page_size_emu(&self, _index: usize) -> (i64, i64) {
        (self.presentation.slide_width, self.presentation.slide_height)
    }

    fn compile_page(&self, index: usize, warnings: &Sender<WarningEvent>) -> Result<VectorSlide, RenderError> {
        if index >= self.presentation.slides.len() {
            return Err(RenderError::CompilationFailed(format!(
                "slide index {} out of bounds (total slides: {})",
                index,
                self.presentation.slides.len()
            )));
        }

        let slide = &self.presentation.slides[index];
        let mut vector_slide = self.compiler.compile_in_deck(
            slide,
            &self.presentation.slides,
            self.presentation.slide_width,
            self.presentation.slide_height,
            warnings,
        )?;

        crate::metafile::expand_metafiles(&mut vector_slide, self.asset_store.as_ref(), warnings);

        Ok(vector_slide)
    }

    fn notes(&self, index: usize) -> Option<String> {
        self.presentation.slides.get(index).and_then(|s| s.notes.clone())
    }

    fn toc(&self) -> Vec<TocEntry> {
        self.presentation
            .slides
            .iter()
            .enumerate()
            .map(|(i, slide)| {
                let mut title = None;
                for element in &slide.elements {
                    if let pptx_model::SlideElement::Shape(shape) = element {
                        if let Some(ref ph) = shape.placeholder_type {
                            if ph == "title" || ph == "ctrTitle" {
                                if let Some(ref body) = shape.text_body {
                                    let text: String = body
                                        .paragraphs
                                        .iter()
                                        .flat_map(|p| p.runs.iter())
                                        .filter_map(|r| match r {
                                            pptx_model::TextRun::Text(t) => Some(t.text.as_str()),
                                            _ => None,
                                        })
                                        .collect();
                                    let trimmed = text.trim();
                                    if !trimmed.is_empty() {
                                        title = Some(trimmed.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                TocEntry {
                    title: title.unwrap_or_else(|| format!("Slide {}", i + 1)),
                    page_index: i,
                    level: 0,
                }
            })
            .collect()
    }

    fn search_corpus(&self) -> Arc<dyn SearchCorpus> {
        Arc::clone(&self.corpus) as Arc<dyn SearchCorpus>
    }

    fn asset_store(&self) -> Option<Arc<dyn AssetStore>> {
        Some(Arc::clone(&self.asset_store))
    }
}
