//! Chart Overlays
//!
//! Overlay visualizations that sit on top of the main chart, including:
//! - Order lines (entry, TP, SL indicators)
//! - Heatmap strips (volume flow visualization)
//!
//! # Example
//!
//! ```
//! use egui_charts::chart::overlays::{HeatmapStrip, OrderLine, OrderLineOverlay, OrderSide};
//!
//! # fn example(ui: &mut egui::Ui, chart_rect: egui::Rect, strip_rect: egui::Rect, price_to_y: impl Fn(f64) -> f32, deltas: &[f64], volumes: &[f64], visible_range: (usize, usize), bar_width: f32) {
//! // Create order lines
//! let mut orders = OrderLineOverlay::new();
//! orders.add_line(OrderLine::entry("order1", 100.0, OrderSide::Buy, 1.0, "BUY LMT"));
//! orders.add_line(OrderLine::stop_loss("sl1", 95.0, OrderSide::Buy, 1.0, "Stop Loss"));
//!
//! // Render in your chart UI
//! let action = orders.render(ui, chart_rect, |price| price_to_y(price));
//!
//! // Create heatmap strip
//! let mut heatmap = HeatmapStrip::new();
//! heatmap.set_data(deltas.iter().enumerate().map(|(i, d)| (i, *d, volumes[i])));
//! heatmap.render(ui, strip_rect, visible_range, bar_width);
//! # }
//! ```

mod heatmap_strip;
mod order_lines;
mod volume_bubbles;

pub use heatmap_strip::{HeatmapCell, HeatmapStrip, HeatmapStripConfig};
pub use order_lines::{
    OrderId, OrderLine, OrderLineAction, OrderLineOverlay, OrderLineType, OrderSide,
};
pub use volume_bubbles::{
    AggregatedBubble, BubbleScalingMode, TradeBubble, VolumeBubbles, VolumeBubblesConfig,
};
