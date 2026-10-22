//! Overlay rendering (OHLC info, crosshair, box zoom, realtime button)

mod box_zoom;
mod realtime_btn;

pub use box_zoom::render_box_zoom;
pub use realtime_btn::render_realtime_btn;

// Re-export directly from renderers (no wrapper needed)
pub use crate::chart::renderers::{render_legend, render_ohlc_info};

use crate::chart::renderers::{ChartMapping, RenderContext};
use crate::config::CrosshairOptions;
use crate::scales::{PriceMap, TimeFormatter};
use egui::{Pos2, Rect};

/// Renders crosshair with full customization from CrosshairOptions.
///
/// Adapts the `CrosshairOptions` struct, with its colours resolved against the active theme,
/// into individual parameters for the underlying `render_crosshair_full` implementation, and
/// returns the rects of its price and time labels.
pub fn render_crosshair_with_options(
    price_ctx: &RenderContext,
    hover_pos: Pos2,
    visible_data: &[crate::model::Bar],
    price_scale: &PriceMap,
    coords: &ChartMapping,
    options: &CrosshairOptions,
    chart_bottom: f32,
    axis_right: f32,
    last_price_tag: Option<Rect>,
    formatter: &dyn TimeFormatter,
) -> [Option<Rect>; 2] {
    let options = options.resolved(price_ctx.painter.ctx());
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
        options.label_background_color,
        options.label_text_color,
        chart_bottom,
        axis_right,
        last_price_tag,
        formatter,
    )
}
