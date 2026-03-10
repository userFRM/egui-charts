//! Trading and volume tool rendering implementations
//!
//! Split into submodules for maintainability:
//! - `position` - Long/short position tool rendering
//! - `forecast` - Forecast tool rendering
//! - `ghost_feed` - Ghost feed rendering
//! - `projection` - Projection tool rendering
//! - `bars_pattern` - Bars pattern rendering
//! - `volume_profile` - VWAP and volume profile rendering

mod bars_pattern;
mod forecast;
mod ghost_feed;
mod position;
mod projection;
mod volume_profile;

// Helper function shared across modules
use egui::{Painter, Pos2, Stroke};

pub(crate) fn draw_dashed_hline(
    painter: &Painter,
    y: f32,
    x_start: f32,
    x_end: f32,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    super::utils::draw_dashed_line(
        painter,
        Pos2::new(x_start, y),
        Pos2::new(x_end, y),
        stroke,
        dash_len,
        gap_len,
    );
}
