//! Pen → `VectorStroke`.

use crate::contracts::VectorStroke;

use crate::metafile::MetafilePen;

/// Maps an engine pen into a display-list stroke.
pub fn map_pen(pen: &MetafilePen, emu_per_unit: f32) -> VectorStroke {
    let width_emu = if pen.cosmetic || pen.width <= 0.0 {
        emu_per_unit.max(1.0)
    } else {
        pen.width * emu_per_unit
    };
    VectorStroke {
        color: pen.color,
        width_emu: width_emu.round().max(1.0) as i64,
        dash_segments: if pen.dash.is_empty() {
            None
        } else {
            Some(pen.dash.clone())
        },
        line_cap: pen.line_cap,
        line_join: pen.line_join,
        miter_limit: pen.miter_limit,
        ..VectorStroke::default()
    }
}
