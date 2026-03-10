//! OHLC bar series renderer.
//!
//! Renders traditional OHLC bars: a vertical line from low to high, with
//! a left tick at the open price and a right tick at the close price.
//! Color is determined by the close-vs-open relationship (bullish/bearish).

use super::types::{Series, SeriesData, SeriesRenderContext, SeriesType, idx_to_x, price_to_y};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Stroke};

/// Visual configuration for a [`BarSeries`].
#[derive(Debug, Clone)]
pub struct BarSeriesOptions {
    /// Bullish bar color (close >= open)
    pub up_color: Color32,

    /// Bearish bar color (close < open)
    pub down_color: Color32,

    /// Bar thickness
    pub thickness: f32,

    /// Length of open/close ticks as fraction of bar spacing
    pub tick_len_fraction: f32,
}

impl Default for BarSeriesOptions {
    fn default() -> Self {
        Self {
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
            thickness: 1.5,
            tick_len_fraction: 0.4,
        }
    }
}

/// An OHLC bar series rendered as vertical lines with open/close ticks.
pub struct BarSeries {
    data: Vec<SeriesData>,
    options: BarSeriesOptions,
    name: String,
}

impl BarSeries {
    pub fn new(name: impl Into<String>, data: Vec<SeriesData>) -> Self {
        Self {
            data,
            options: BarSeriesOptions::default(),
            name: name.into(),
        }
    }

    pub fn with_options(mut self, options: BarSeriesOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_colors(mut self, up_color: Color32, down_color: Color32) -> Self {
        self.options.up_color = up_color;
        self.options.down_color = down_color;
        self
    }
}

impl Series for BarSeries {
    fn series_type(&self) -> SeriesType {
        SeriesType::Bar
    }

    fn data(&self) -> &[SeriesData] {
        &self.data
    }

    fn price_range(&self, start_idx: usize, end_idx: usize) -> Option<(f64, f64)> {
        let visible = &self.data[start_idx.min(self.data.len())..end_idx.min(self.data.len())];

        let mut min = f64::MAX;
        let mut max = f64::MIN;
        let mut found = false;

        for point in visible {
            if let (Some(high), Some(low)) = (point.high, point.low) {
                min = min.min(low);
                max = max.max(high);
                found = true;
            }
        }

        if found { Some((min, max)) } else { None }
    }

    fn render(&self, ctx: &SeriesRenderContext) {
        let visible =
            &self.data[ctx.start_idx.min(self.data.len())..ctx.end_idx.min(self.data.len())];
        if visible.is_empty() {
            return;
        }

        let last_idx = self.data.len().saturating_sub(1);
        let tick_len = ctx.bar_spacing * self.options.tick_len_fraction;

        for (i, point) in visible.iter().enumerate() {
            if let (Some(open), Some(high), Some(low), Some(close)) =
                (point.open, point.high, point.low, point.close)
            {
                let global_idx = ctx.start_idx + i;
                let x = idx_to_x(
                    global_idx,
                    last_idx,
                    ctx.bar_spacing,
                    ctx.right_offset,
                    ctx.rect,
                );

                let y_open = price_to_y(open, ctx.price_min, ctx.price_max, ctx.rect);
                let y_high = price_to_y(high, ctx.price_min, ctx.price_max, ctx.rect);
                let y_low = price_to_y(low, ctx.price_min, ctx.price_max, ctx.rect);
                let y_close = price_to_y(close, ctx.price_min, ctx.price_max, ctx.rect);

                // Determine color
                let color = if let Some(color_override) = point.color {
                    color_override
                } else if close >= open {
                    self.options.up_color
                } else {
                    self.options.down_color
                };

                let stroke = Stroke::new(self.options.thickness, color);

                // Draw vertical line (low to high)
                ctx.painter
                    .line_segment([Pos2::new(x, y_low), Pos2::new(x, y_high)], stroke);

                // Draw open tick (to the left)
                ctx.painter.line_segment(
                    [Pos2::new(x - tick_len / 2.0, y_open), Pos2::new(x, y_open)],
                    stroke,
                );

                // Draw close tick (to the right)
                ctx.painter.line_segment(
                    [
                        Pos2::new(x, y_close),
                        Pos2::new(x + tick_len / 2.0, y_close),
                    ],
                    stroke,
                );
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn color(&self) -> Color32 {
        self.options.up_color
    }
}
