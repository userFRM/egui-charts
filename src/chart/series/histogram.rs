//! Histogram series renderer.
//!
//! Renders vertical bars extending from a baseline to each data value.
//! Commonly used for volume, MACD histogram, or any indicator that
//! benefits from a bar-chart visualization. Supports dual coloring
//! for positive/negative values.

use super::types::{Series, SeriesData, SeriesRenderContext, SeriesType, idx_to_x, price_to_y};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Vec2};

/// Visual configuration for a [`HistogramSeries`].
#[derive(Debug, Clone)]
pub struct HistogramSeriesOptions {
    /// Bar color
    pub color: Color32,

    /// Baseline value (bars extend from this value)
    pub baseline: f64,

    /// Bar width as fraction of bar spacing (0.0 to 1.0)
    pub bar_width_fraction: f32,

    /// Use different colors for positive/negative values
    pub use_dual_colors: bool,

    /// Positive value color
    pub positive_color: Color32,

    /// Negative value color
    pub negative_color: Color32,
}

impl Default for HistogramSeriesOptions {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.extended.success,
            baseline: 0.0,
            bar_width_fraction: 0.8,
            use_dual_colors: false,
            positive_color: DESIGN_TOKENS.semantic.extended.bullish,
            negative_color: DESIGN_TOKENS.semantic.extended.bearish,
        }
    }
}

/// A histogram series that renders vertical bars from a baseline to each data point.
pub struct HistogramSeries {
    data: Vec<SeriesData>,
    options: HistogramSeriesOptions,
    name: String,
}

impl HistogramSeries {
    pub fn new(name: impl Into<String>, data: Vec<SeriesData>) -> Self {
        Self {
            data,
            options: HistogramSeriesOptions::default(),
            name: name.into(),
        }
    }

    pub fn with_options(mut self, options: HistogramSeriesOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.options.color = color;
        self
    }

    pub fn with_dual_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.options.use_dual_colors = true;
        self.options.positive_color = positive;
        self.options.negative_color = negative;
        self
    }
}

impl Series for HistogramSeries {
    fn series_type(&self) -> SeriesType {
        SeriesType::Histogram
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
            if let Some(value) = point.main_val() {
                min = min.min(value).min(self.options.baseline);
                max = max.max(value).max(self.options.baseline);
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
        let baseline_y = price_to_y(
            self.options.baseline,
            ctx.price_min,
            ctx.price_max,
            ctx.rect,
        );
        let bar_width = ctx.bar_spacing * self.options.bar_width_fraction;

        for (i, point) in visible.iter().enumerate() {
            if let Some(value) = point.main_val() {
                let global_idx = ctx.start_idx + i;
                let x = idx_to_x(
                    global_idx,
                    last_idx,
                    ctx.bar_spacing,
                    ctx.right_offset,
                    ctx.rect,
                );
                let y = price_to_y(value, ctx.price_min, ctx.price_max, ctx.rect);

                // Determine bar color
                let bar_color = if let Some(color_override) = point.color {
                    color_override
                } else if self.options.use_dual_colors {
                    if value >= self.options.baseline {
                        self.options.positive_color
                    } else {
                        self.options.negative_color
                    }
                } else {
                    self.options.color
                };

                // Draw bar
                let bar_rect = Rect::from_min_size(
                    Pos2::new(x - bar_width / 2.0, y.min(baseline_y)),
                    Vec2::new(bar_width, (y - baseline_y).abs()),
                );

                ctx.painter.rect_filled(bar_rect, 0.0, bar_color);
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn color(&self) -> Color32 {
        self.options.color
    }
}
