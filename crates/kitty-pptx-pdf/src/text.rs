use hayro_interpret::font::GlyphRun;
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::kurbo::{BezPath, Rect};
use hayro_interpret::util::TransformExt;
use hayro_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image,
    ImageDrawProps, InterpreterCache, InterpreterSettings, SoftMask,
};
use hayro_syntax::page::Page;
use kitty_pptx_core::{EmuRect, SearchCorpus, TextLine};

/// Device that intercepts glyph runs during page interpretation to extract text and bounds.
struct TextExtractionDevice {
    page_height_pt: f32,
    lines: Vec<TextLine>,
    current_line: String,
    line_min_x: f32,
    line_max_x: f32,
    line_min_y: f32,
    line_max_y: f32,
    last_y: Option<f32>,
}

impl TextExtractionDevice {
    fn new(page_height_pt: f32) -> Self {
        Self {
            page_height_pt,
            lines: Vec::new(),
            current_line: String::new(),
            line_min_x: f32::MAX,
            line_max_x: f32::MIN,
            line_min_y: f32::MAX,
            line_max_y: f32::MIN,
            last_y: None,
        }
    }

    fn flush_line(&mut self) {
        let trimmed = self.current_line.trim();
        if !trimmed.is_empty() && self.line_max_x > self.line_min_x {
            self.lines.push(TextLine {
                text: trimmed.to_string(),
                rect: EmuRect::new(
                    (self.line_min_x * 12_700.0).max(0.0),
                    (self.line_min_y * 12_700.0).max(0.0),
                    ((self.line_max_x - self.line_min_x) * 12_700.0).max(1.0),
                    ((self.line_max_y - self.line_min_y) * 12_700.0).max(1.0),
                ),
            });
        }
        self.current_line.clear();
        self.line_min_x = f32::MAX;
        self.line_max_x = f32::MIN;
        self.line_min_y = f32::MAX;
        self.line_max_y = f32::MIN;
        self.last_y = None;
    }

    fn finish(mut self) -> Vec<TextLine> {
        self.flush_line();
        self.lines
    }
}

impl<'a> Device<'a> for TextExtractionDevice {
    fn draw_path(&mut self, _path: &BezPath, _props: DrawProps<'a>, _draw_mode: &DrawMode) {}

    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}

    fn push_transparency_group(
        &mut self,
        _opacity: f32,
        _mask: Option<SoftMask<'a>>,
        _blend_mode: BlendMode,
    ) {
    }

    fn pop_clip(&mut self) {}

    fn pop_transparency_group(&mut self) {}

    fn draw_image(&mut self, _image: Image<'a, '_>, _props: ImageDrawProps<'a>) {}

    fn draw_glyph_run(
        &mut self,
        glyph_run: &GlyphRun<'_, 'a>,
        _props: DrawProps<'a>,
        _draw_mode: &DrawMode,
    ) {
        for glyph in glyph_run.glyphs() {
            let Some(unicode) = glyph.as_unicode() else {
                continue;
            };

            let transform = glyph.transform();
            let coeffs = transform.as_coeffs();
            let x = coeffs[4] as f32;
            let y_pdf = coeffs[5] as f32;
            // PDF user points y is bottom-up; convert to y-down top-left
            let y = (self.page_height_pt - y_pdf).max(0.0);

            // If vertical offset jumped by more than 8 points, break to a new line
            if let Some(prev_y) = self.last_y {
                if (y - prev_y).abs() > 8.0 {
                    self.flush_line();
                }
            }
            self.last_y = Some(y);

            match unicode {
                BfString::Char(c) => self.current_line.push(c),
                BfString::String(s) => self.current_line.push_str(&s),
            }

            let est_w = 8.0f32;
            let est_h = 10.0f32;
            self.line_min_x = self.line_min_x.min(x);
            self.line_max_x = self.line_max_x.max(x + est_w);
            self.line_min_y = self.line_min_y.min(y);
            self.line_max_y = self.line_max_y.max(y + est_h);
        }
    }
}

/// Extracts text lines and bounding boxes from a PDF page.
pub fn extract_page_text(page: &Page<'_>) -> Vec<TextLine> {
    let (w_pt, h_pt) = page.render_dimensions();
    let cache = InterpreterCache::default();
    let settings = InterpreterSettings::default();

    let mut state = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, w_pt as f64, h_pt as f64),
        &cache,
        page.xref(),
        settings,
    );

    let mut device = TextExtractionDevice::new(h_pt);
    interpret_page(page, &mut state, &mut device);
    device.finish()
}

/// In-memory search corpus for a PDF document.
#[derive(Clone, Debug, Default)]
pub struct PdfSearchCorpus {
    pages: Vec<Vec<TextLine>>,
}

impl PdfSearchCorpus {
    pub fn new(pages: Vec<Vec<TextLine>>) -> Self {
        Self { pages }
    }
}

impl SearchCorpus for PdfSearchCorpus {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page_text(&self, page: usize) -> &[TextLine] {
        self.pages.get(page).map(|v| v.as_slice()).unwrap_or(&[])
    }
}
