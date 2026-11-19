//! Geometry and clip conversion helpers for the renderer adapter.
//!
//! The helpers are grouped by responsibility: [`paths`] builds contract paths
//! from engine geometry, [`clip`] maps clip regions into clip paths, and
//! [`bounds`] measures clip regions, point lists and engine paths.

mod bounds;
mod clip;
mod paths;

pub(super) use bounds::path_bounds;
pub(super) use clip::map_clip;
pub(super) use paths::{
    arc_path, closed_polygon, ellipse_path, engine_path, open_path, rounded_rect,
};
