//! # Volume Bubbles Overlay
//!
//! Shows individual trades as bubbles on the chart.
//! Bubble size represents trade size, color represents side.

use crate::model::TradeSide;
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
use egui::{Color32, Pos2, Rect, Ui};

/// Configuration for volume bubbles overlay
#[derive(Debug, Clone)]
pub struct VolumeBubblesConfig {
    /// Minimum bubble diameter
    pub min_diameter: f32,
    /// Maximum bubble diameter
    pub max_diameter: f32,
    /// Opacity for bubbles
    pub opacity: u8,
    /// Color for buy trades
    pub buy_color: Color32,
    /// Color for sell trades
    pub sell_color: Color32,
    /// Color for unknown side trades
    pub neutral_color: Color32,
    /// Size scaling mode
    pub scaling_mode: BubbleScalingMode,
    /// Show border around bubbles
    pub show_border: bool,
    /// Border color
    pub border_color: Color32,
    /// Filter trades by minimum size
    pub min_size_filter: Option<f64>,
    /// Aggregate nearby trades
    pub aggregate_nearby: bool,
    /// Aggregation threshold (pixels)
    pub aggregate_threshold: f32,
}

impl Default for VolumeBubblesConfig {
    fn default() -> Self {
        Self {
            min_diameter: DESIGN_TOKENS.spacing.sm,
            max_diameter: DESIGN_TOKENS.sizing.charts_ext.volume_bubbles_max_diameter,
            opacity: 180,
            buy_color: DESIGN_TOKENS.semantic.chart.bullish,
            sell_color: DESIGN_TOKENS.semantic.chart.bearish,
            neutral_color: DESIGN_TOKENS.semantic.ui.text_muted_dark,
            scaling_mode: BubbleScalingMode::Logarithmic,
            show_border: true,
            border_color: Color32::from_rgba_unmultiplied(255, 255, 255, 100),
            min_size_filter: None,
            aggregate_nearby: true,
            aggregate_threshold: DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
        }
    }
}

/// How to scale bubble sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleScalingMode {
    /// Linear scaling
    Linear,
    /// Logarithmic scaling (better for large range)
    Logarithmic,
    /// Square root scaling
    SquareRoot,
}

/// A trade to display as a bubble
#[derive(Debug, Clone)]
pub struct TradeBubble {
    /// Trade timestamp
    pub ts: DateTime<Utc>,
    /// Trade price
    pub price: f64,
    /// Trade size
    pub size: f64,
    /// Trade side
    pub side: TradeSide,
    /// Optional trade ID
    pub id: Option<String>,
}

impl TradeBubble {
    /// Create a new trade bubble
    pub fn new(ts: DateTime<Utc>, price: f64, size: f64, side: TradeSide) -> Self {
        Self {
            ts,
            price,
            size,
            side,
            id: None,
        }
    }

    /// Set trade ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Aggregated bubble (multiple trades combined)
#[derive(Debug, Clone)]
pub struct AggregatedBubble {
    /// Average timestamp
    pub ts: DateTime<Utc>,
    /// VWAP price
    pub price: f64,
    /// Total size
    pub total_size: f64,
    /// Net side
    pub net_side: TradeSide,
    /// Number of trades aggregated
    pub trade_count: usize,
    /// Buy volume
    pub buy_volume: f64,
    /// Sell volume
    pub sell_volume: f64,
}

/// Renderer that displays individual trades as sized, colored bubbles on the chart.
///
/// Bubble diameter scales with trade size (using configurable linear, logarithmic,
/// or square-root scaling), and color indicates buy/sell side. Nearby trades can
/// be aggregated into a single bubble to reduce visual clutter.
pub struct VolumeBubbles {
    /// Configuration
    pub config: VolumeBubblesConfig,
    /// Trades to render
    trades: Vec<TradeBubble>,
    /// Statistics for scaling
    min_size: f64,
    max_size: f64,
}

impl VolumeBubbles {
    /// Create a new volume bubbles renderer
    pub fn new() -> Self {
        Self {
            config: VolumeBubblesConfig::default(),
            trades: Vec::new(),
            min_size: f64::MAX,
            max_size: f64::MIN,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: VolumeBubblesConfig) -> Self {
        Self {
            config,
            trades: Vec::new(),
            min_size: f64::MAX,
            max_size: f64::MIN,
        }
    }

    /// Add a trade
    pub fn add_trade(&mut self, trade: TradeBubble) {
        self.min_size = self.min_size.min(trade.size);
        self.max_size = self.max_size.max(trade.size);
        self.trades.push(trade);
    }

    /// Add multiple trades
    pub fn add_trades(&mut self, trades: impl IntoIterator<Item = TradeBubble>) {
        for trade in trades {
            self.add_trade(trade);
        }
    }

    /// Clear all trades
    pub fn clear(&mut self) {
        self.trades.clear();
        self.min_size = f64::MAX;
        self.max_size = f64::MIN;
    }

    /// Set trades directly
    pub fn set_trades(&mut self, trades: Vec<TradeBubble>) {
        self.trades = trades;
        self.recalculate_stats();
    }

    fn recalculate_stats(&mut self) {
        self.min_size = f64::MAX;
        self.max_size = f64::MIN;
        for trade in &self.trades {
            self.min_size = self.min_size.min(trade.size);
            self.max_size = self.max_size.max(trade.size);
        }
    }

    /// Calculate bubble diameter for a given size
    pub fn calculate_diameter(&self, size: f64) -> f32 {
        if self.max_size <= self.min_size {
            return (self.config.min_diameter + self.config.max_diameter) / 2.0;
        }

        let normalized = match self.config.scaling_mode {
            BubbleScalingMode::Linear => (size - self.min_size) / (self.max_size - self.min_size),
            BubbleScalingMode::Logarithmic => {
                let log_size = (1.0 + size).ln();
                let log_min = (1.0 + self.min_size).ln();
                let log_max = (1.0 + self.max_size).ln();
                (log_size - log_min) / (log_max - log_min)
            }
            BubbleScalingMode::SquareRoot => {
                let sqrt_size = size.sqrt();
                let sqrt_min = self.min_size.sqrt();
                let sqrt_max = self.max_size.sqrt();
                (sqrt_size - sqrt_min) / (sqrt_max - sqrt_min)
            }
        };

        let clamped = normalized.clamp(0.0, 1.0) as f32;
        self.config.min_diameter + clamped * (self.config.max_diameter - self.config.min_diameter)
    }

    /// Get color for a trade side
    pub fn get_color(&self, side: TradeSide) -> Color32 {
        let base_color = match side {
            TradeSide::Buy => self.config.buy_color,
            TradeSide::Sell => self.config.sell_color,
            TradeSide::Unknown => self.config.neutral_color,
        };

        // Apply opacity
        Color32::from_rgba_unmultiplied(
            base_color.r(),
            base_color.g(),
            base_color.b(),
            self.config.opacity,
        )
    }

    /// Aggregate nearby bubbles for rendering optimization
    pub fn aggregate_trades(
        &self,
        price_to_y: impl Fn(f64) -> f32,
        time_to_x: impl Fn(DateTime<Utc>) -> f32,
    ) -> Vec<AggregatedBubble> {
        if !self.config.aggregate_nearby || self.trades.is_empty() {
            // Convert to aggregated format without aggregation
            return self
                .trades
                .iter()
                .map(|t| AggregatedBubble {
                    ts: t.ts,
                    price: t.price,
                    total_size: t.size,
                    net_side: t.side,
                    trade_count: 1,
                    buy_volume: if t.side == TradeSide::Buy {
                        t.size
                    } else {
                        0.0
                    },
                    sell_volume: if t.side == TradeSide::Sell {
                        t.size
                    } else {
                        0.0
                    },
                })
                .collect();
        }

        let threshold = self.config.aggregate_threshold;
        let mut aggregated: Vec<AggregatedBubble> = Vec::new();

        for trade in &self.trades {
            let x = time_to_x(trade.ts);
            let y = price_to_y(trade.price);

            // Try to find an existing aggregate to merge with
            let merged = aggregated.iter_mut().find(|agg| {
                let agg_x = time_to_x(agg.ts);
                let agg_y = price_to_y(agg.price);
                let dx = (x - agg_x).abs();
                let dy = (y - agg_y).abs();
                dx <= threshold && dy <= threshold
            });

            if let Some(agg) = merged {
                // Merge into existing aggregate
                let total_volume = agg.total_size + trade.size;
                // VWAP
                agg.price = (agg.price * agg.total_size + trade.price * trade.size) / total_volume;
                agg.total_size = total_volume;
                agg.trade_count += 1;

                match trade.side {
                    TradeSide::Buy => agg.buy_volume += trade.size,
                    TradeSide::Sell => agg.sell_volume += trade.size,
                    TradeSide::Unknown => {}
                }

                // Update net side
                if agg.buy_volume > agg.sell_volume {
                    agg.net_side = TradeSide::Buy;
                } else if agg.sell_volume > agg.buy_volume {
                    agg.net_side = TradeSide::Sell;
                } else {
                    agg.net_side = TradeSide::Unknown;
                }
            } else {
                // Create new aggregate
                aggregated.push(AggregatedBubble {
                    ts: trade.ts,
                    price: trade.price,
                    total_size: trade.size,
                    net_side: trade.side,
                    trade_count: 1,
                    buy_volume: if trade.side == TradeSide::Buy {
                        trade.size
                    } else {
                        0.0
                    },
                    sell_volume: if trade.side == TradeSide::Sell {
                        trade.size
                    } else {
                        0.0
                    },
                });
            }
        }

        aggregated
    }

    /// Render bubbles on the chart
    ///
    /// This is a low-level method that renders directly to the painter.
    /// The caller must provide coordinate conversion functions.
    pub fn render(
        &self,
        ui: &mut Ui,
        chart_rect: Rect,
        price_to_y: impl Fn(f64) -> f32,
        time_to_x: impl Fn(DateTime<Utc>) -> f32,
    ) {
        if self.trades.is_empty() {
            return;
        }

        let painter = ui.painter();

        // Get aggregated bubbles
        let bubbles = self.aggregate_trades(&price_to_y, &time_to_x);

        for bubble in bubbles {
            let x = time_to_x(bubble.ts);
            let y = price_to_y(bubble.price);

            // Skip if outside visible area
            if x < chart_rect.left() - 50.0
                || x > chart_rect.right() + 50.0
                || y < chart_rect.top() - 50.0
                || y > chart_rect.bottom() + 50.0
            {
                continue;
            }

            // Apply size filter
            if let Some(min_size) = self.config.min_size_filter
                && bubble.total_size < min_size
            {
                continue;
            }

            let diameter = self.calculate_diameter(bubble.total_size);
            let radius = diameter / 2.0;
            let center = Pos2::new(x, y);
            let color = self.get_color(bubble.net_side);

            // Draw filled circle
            painter.circle_filled(center, radius, color);

            // Draw border if enabled
            if self.config.show_border {
                painter.circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, self.config.border_color),
                );
            }
        }
    }

    /// Get trades in visible range
    pub fn trades_in_range(
        &self,
        time_start: DateTime<Utc>,
        time_end: DateTime<Utc>,
    ) -> impl Iterator<Item = &TradeBubble> {
        self.trades
            .iter()
            .filter(move |t| t.ts >= time_start && t.ts <= time_end)
    }

    /// Get trade count
    pub fn len(&self) -> usize {
        self.trades.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }
}

impl Default for VolumeBubbles {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_bubbles() -> VolumeBubbles {
        let mut bubbles = VolumeBubbles::new();
        let ts = Utc::now();

        bubbles.add_trade(TradeBubble::new(ts, 100.0, 10.0, TradeSide::Buy));
        bubbles.add_trade(TradeBubble::new(ts, 100.5, 20.0, TradeSide::Sell));
        bubbles.add_trade(TradeBubble::new(ts, 99.5, 50.0, TradeSide::Buy));
        bubbles.add_trade(TradeBubble::new(ts, 101.0, 5.0, TradeSide::Unknown));

        bubbles
    }

    #[test]
    fn test_bubble_creation() {
        let bubbles = create_test_bubbles();

        assert_eq!(bubbles.len(), 4);
        assert_eq!(bubbles.min_size, 5.0);
        assert_eq!(bubbles.max_size, 50.0);
    }

    #[test]
    fn test_diameter_calculation() {
        let bubbles = create_test_bubbles();

        // Smallest trade should get min diameter
        let min_d = bubbles.calculate_diameter(5.0);
        assert!(min_d >= bubbles.config.min_diameter && min_d <= bubbles.config.min_diameter + 5.0);

        // Largest trade should get max diameter
        let max_d = bubbles.calculate_diameter(50.0);
        assert!(max_d <= bubbles.config.max_diameter && max_d >= bubbles.config.max_diameter - 5.0);
    }

    #[test]
    fn test_color_selection() {
        let bubbles = VolumeBubbles::new();

        let buy_color = bubbles.get_color(TradeSide::Buy);
        let sell_color = bubbles.get_color(TradeSide::Sell);
        let neutral_color = bubbles.get_color(TradeSide::Unknown);

        // Colors should be different
        assert_ne!(buy_color.r(), sell_color.r());
        assert_ne!(buy_color, neutral_color);
    }

    #[test]
    fn test_aggregation() {
        let mut bubbles = VolumeBubbles::new();
        let ts = Utc::now();

        // Add trades at same price (should aggregate)
        bubbles.add_trade(TradeBubble::new(ts, 100.0, 10.0, TradeSide::Buy));
        bubbles.add_trade(TradeBubble::new(ts, 100.0, 20.0, TradeSide::Buy));

        // Trivial coordinate functions for testing
        let price_to_y = |_: f64| 0.0;
        let time_to_x = |_: DateTime<Utc>| 0.0;

        let aggregated = bubbles.aggregate_trades(price_to_y, time_to_x);

        // Should aggregate into one bubble
        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].total_size, 30.0);
        assert_eq!(aggregated[0].trade_count, 2);
    }

    #[test]
    fn test_clear() {
        let mut bubbles = create_test_bubbles();
        bubbles.clear();

        assert!(bubbles.is_empty());
    }
}
