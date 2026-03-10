//! Baseline series renderer.
//!
//! Similar to an area chart, but uses different colors for regions above
//! and below the baseline value. This dual-color approach makes it easy to
//! visualize positive/negative deviations from a reference level (e.g.,
//! profit/loss relative to entry, or temperature above/below average).

use super::types::{Series, SeriesData, SeriesRenderContext, SeriesType, idx_to_x, price_to_y};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Shape, Stroke};

/// Visual configuration for a [`BaselineSeries`].
#[derive(Debug, Clone)]
pub struct BaselineSeriesOptions {
    /// Color above baseline
    pub top_line_color: Color32,

    /// Color below baseline
    pub bottom_line_color: Color32,

    /// Fill color above baseline
    pub top_fill_color: Color32,

    /// Fill color below baseline
    pub bottom_fill_color: Color32,

    /// Line width
    pub line_width: f32,

    /// Baseline value
    pub baseline: f64,
}

impl Default for BaselineSeriesOptions {
    fn default() -> Self {
        Self {
            top_line_color: DESIGN_TOKENS.semantic.extended.bullish,
            bottom_line_color: DESIGN_TOKENS.semantic.extended.bearish,
            top_fill_color: DESIGN_TOKENS
                .semantic
                .extended
                .bullish
                .gamma_multiply(50_f32 / 255.0),
            bottom_fill_color: DESIGN_TOKENS
                .semantic
                .extended
                .bearish
                .gamma_multiply(50_f32 / 255.0),
            line_width: 2.0,
            baseline: 0.0,
        }
    }
}

/// A baseline series with dual-color regions above and below a reference value.
pub struct BaselineSeries {
    data: Vec<SeriesData>,
    options: BaselineSeriesOptions,
    name: String,
}

impl BaselineSeries {
    pub fn new(name: impl Into<String>, data: Vec<SeriesData>) -> Self {
        Self {
            data,
            options: BaselineSeriesOptions::default(),
            name: name.into(),
        }
    }

    pub fn with_options(mut self, options: BaselineSeriesOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_baseline(mut self, baseline: f64) -> Self {
        self.options.baseline = baseline;
        self
    }
}

impl Series for BaselineSeries {
    fn series_type(&self) -> SeriesType {
        SeriesType::Baseline
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

        // Split into segments above/below baseline
        let mut curr_segment: Vec<Pos2> = Vec::new();
        let mut is_above = false;

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
                let point_pos = Pos2::new(x, y);
                let curr_is_above = value >= self.options.baseline;

                if curr_segment.is_empty() {
                    // Start new segment
                    curr_segment.push(point_pos);
                    is_above = curr_is_above;
                } else if curr_is_above == is_above {
                    // Continue current segment
                    curr_segment.push(point_pos);
                } else {
                    // Crossed baseline - finish current segment and start new one
                    self.render_segment(ctx.painter, &curr_segment, baseline_y, is_above);
                    curr_segment.clear();
                    curr_segment.push(point_pos);
                    is_above = curr_is_above;
                }
            }
        }

        // Render last segment
        if !curr_segment.is_empty() {
            self.render_segment(ctx.painter, &curr_segment, baseline_y, is_above);
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn color(&self) -> Color32 {
        self.options.top_line_color
    }
}

impl BaselineSeries {
    fn render_segment(&self, painter: &Painter, points: &[Pos2], baseline_y: f32, is_above: bool) {
        if points.len() < 2 {
            return;
        }

        let (line_color, fill_color) = if is_above {
            (self.options.top_line_color, self.options.top_fill_color)
        } else {
            (
                self.options.bottom_line_color,
                self.options.bottom_fill_color,
            )
        };

        // Create fill polygon
        // Safe: guarded by len() < 2 check above
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return;
        };
        let mut fill_points = points.to_vec();
        let last_x = last.x;
        let first_x = first.x;
        fill_points.push(Pos2::new(last_x, baseline_y));
        fill_points.push(Pos2::new(first_x, baseline_y));

        // Draw filled area
        painter.add(Shape::convex_polygon(fill_points, fill_color, Stroke::NONE));

        // Draw line
        painter.add(Shape::line(
            points.to_vec(),
            Stroke::new(self.options.line_width, line_color),
        ));
    }
}
