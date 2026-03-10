//! Chart Overlays
//!
//! Overlay visualizations that sit on top of the main chart, including:
//! - Order lines (entry, TP, SL indicators)
//! - Heatmap strips (volume flow visualization)
//!
//! # Example
//!
//! ```ignore
//! use egui_charts::chart::overlays::{OrderLineOverlay, OrderLine, OrderSide, HeatmapStrip};
//!
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
