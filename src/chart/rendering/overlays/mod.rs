//! Overlay rendering (OHLC info, crosshair, box zoom, realtime button)

mod box_zoom;
mod realtime_btn;

pub use box_zoom::render_box_zoom;
pub use realtime_btn::render_realtime_btn;

// Re-export directly from renderers (no wrapper needed)
pub use crate::chart::renderers::{render_legend, render_ohlc_info};

use crate::chart::renderers::{ChartMapping, PriceScale, RenderContext};
use crate::config::CrosshairOptions;
use egui::Pos2;

/// Renders crosshair with full customization from CrosshairOptions.
///
/// Adapts the `CrosshairOptions` struct into individual parameters for
/// the underlying `render_crosshair_full` implementation.
pub fn render_crosshair_with_options(
    price_ctx: &RenderContext,
    hover_pos: Pos2,
    visible_data: &[crate::model::Bar],
    price_scale: &PriceScale,
    coords: &ChartMapping,
    options: &CrosshairOptions,
) {
    crate::chart::renderers::render_crosshair_full(
        price_ctx,
        hover_pos,
        visible_data,
        price_scale,
        coords,
        options.mode,
        options.style,
        options.vert_line_color,
        options.vert_line_width,
        options.line_style,
    );
}
