//! `MetafileSink` implementation that records display-list commands.

use crate::contracts::{AffineTransform, ShapeEffects, VectorCommand};

use crate::metafile::{
    mapping, MetafileBitmap, MetafileClip, MetafileDiagnostic, MetafileError, MetafilePaint, MetafilePath, MetafilePen,
    MetafileSink, MetafileTextRun,
};

/// Accumulates commands in element-local EMU for one metafile playback.
pub struct MetafileRecorder {
    unit_to_emu: AffineTransform,
    emu_per_unit: f32,
    commands: Vec<VectorCommand>,
    diagnostics: Vec<MetafileDiagnostic>,
}

impl MetafileRecorder {
    /// `unit_to_emu` maps engine output units into picture-local EMU;
    /// `emu_per_unit` is its average scale, used for pen widths and font sizes.
    pub fn new(unit_to_emu: AffineTransform, emu_per_unit: f32) -> Self {
        Self {
            unit_to_emu,
            emu_per_unit,
            commands: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Returns `(commands, diagnostics)` once playback completes.
    pub fn finish(self) -> (Vec<VectorCommand>, Vec<MetafileDiagnostic>) {
        (self.commands, self.diagnostics)
    }
}

impl MetafileSink for MetafileRecorder {
    fn push_clip(&mut self, clip: &MetafileClip) -> Result<(), MetafileError> {
        if let Some(path) = mapping::clip::map_clip(clip, &self.unit_to_emu) {
            self.commands.push(VectorCommand::PushClip(path));
        }
        Ok(())
    }

    fn pop_clip(&mut self) {
        self.commands.push(VectorCommand::PopClip);
    }

    fn draw_path(
        &mut self,
        path: &MetafilePath,
        fill: Option<&MetafilePaint>,
        stroke: Option<&MetafilePen>,
    ) -> Result<(), MetafileError> {
        let fill = fill.and_then(|p| mapping::paint::map_paint(p, &self.unit_to_emu));
        let stroke = stroke.map(|pen| mapping::pen::map_pen(pen, self.emu_per_unit));
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        self.commands.push(VectorCommand::DrawPath {
            path: mapping::path::map_path(path, &self.unit_to_emu),
            fill,
            stroke,
            transform: AffineTransform::identity(),
            effects: Box::new(ShapeEffects::default()),
        });
        Ok(())
    }

    fn draw_text(&mut self, run: &MetafileTextRun) -> Result<(), MetafileError> {
        let Some(mapped) = mapping::text::map_text_run(run, &self.unit_to_emu) else {
            return Ok(());
        };
        if let Some((path, color)) = mapped.background {
            self.commands.push(VectorCommand::DrawPath {
                path,
                fill: Some(crate::contracts::VectorPaint::Solid(color)),
                stroke: None,
                transform: AffineTransform::identity(),
                effects: Box::new(ShapeEffects::default()),
            });
        }
        if !mapped.runs.is_empty() {
            self.commands.push(VectorCommand::DrawText {
                runs: mapped.runs,
                transform: mapped.transform,
            });
        }
        self.diagnostics.extend(mapped.diagnostics);
        Ok(())
    }

    fn draw_bitmap(&mut self, bitmap: &MetafileBitmap) -> Result<(), MetafileError> {
        match mapping::bitmap::encode_png(bitmap) {
            Some(png) => {
                let placement = mapping::bitmap::map_placement(bitmap, &self.unit_to_emu);
                self.commands.push(VectorCommand::DrawBitmap {
                    png,
                    width: bitmap.width,
                    height: bitmap.height,
                    placement,
                    opacity: bitmap.opacity,
                    sampling: bitmap.sampling,
                });
            }
            None => self.diagnostics.push(MetafileDiagnostic::degraded(
                "metafile_bitmap",
                "embedded bitmap could not be encoded",
            )),
        }
        Ok(())
    }

    fn diagnostic(&mut self, diagnostic: MetafileDiagnostic) {
        self.diagnostics.push(diagnostic);
    }
}
