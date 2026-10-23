//! Volume rendering helpers

use crate::chart::renderers::{self, BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;

/// Render volume bars for chart types that show volume
pub(super) fn render_volume_bars(
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    _bar_width: f32,
    max_volume: f64,
    colors: &StyleColors,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let pitch = (idx_to_coord(start_idx + i + 1, chart_rect_min_x) - x).abs();
        let volume_params = BarRenderParams::new(x, pitch * 0.8, 0.0);
        renderers::render_volume_bar(volume_ctx, bar, max_volume, colors, &volume_params);
    }
}
