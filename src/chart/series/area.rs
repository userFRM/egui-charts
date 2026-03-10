//! Area series renderer.
//!
//! Renders an area chart with a line on top and a filled polygon extending
//! down to a configurable baseline. Commonly used for market-cap, equity
//! curves, or any metric where the filled area conveys magnitude.

use super::types::{Series, SeriesData, SeriesRenderContext, SeriesType, idx_to_x, price_to_y};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Shape, Stroke};

/// Visual configuration for an [`AreaSeries`].
#[derive(Debug, Clone)]
pub struct AreaSeriesOptions {
    /// Line color
    pub line_color: Color32,

    /// Fill color (with transparency)
    pub fill_color: Color32,

    /// Line width
    pub line_width: f32,

    /// Baseline value (fill from this value)
    pub baseline: f64,
}

impl Default for AreaSeriesOptions {
    fn default() -> Self {
        Self {
            line_color: DESIGN_TOKENS.semantic.extended.info,
            fill_color: Color32::from_rgba_unmultiplied(
                DESIGN_TOKENS.semantic.extended.info.r(),
                DESIGN_TOKENS.semantic.extended.info.g(),
                DESIGN_TOKENS.semantic.extended.info.b(),
                50,
            ),
            line_width: 2.0,
            baseline: 0.0,
        }
    }
}

/// An area series that renders a line with a filled region down to a baseline.
pub struct AreaSeries {
    data: Vec<SeriesData>,
    options: AreaSeriesOptions,
    name: String,
}

impl AreaSeries {
    pub fn new(name: impl Into<String>, data: Vec<SeriesData>) -> Self {
        Self {
            data,
            options: AreaSeriesOptions::default(),
            name: name.into(),
        }
    }

    pub fn with_options(mut self, options: AreaSeriesOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_colors(mut self, line_color: Color32, fill_color: Color32) -> Self {
        self.options.line_color = line_color;
        self.options.fill_color = fill_color;
        self
    }
}

impl Series for AreaSeries {
    fn series_type(&self) -> SeriesType {
        SeriesType::Area
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

        // Collect points for line
        let mut line_points = Vec::new();
        let mut fill_points = Vec::new();

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
                line_points.push(Pos2::new(x, y));
            }
        }

        if line_points.len() < 2 {
            return;
        }

        // Create fill polygon: line points + baseline points (reversed)
        fill_points.extend_from_slice(&line_points);

        // Add baseline points in reverse order
        // Safe: guarded by len() < 2 check above
        let (Some(first), Some(last)) = (line_points.first(), line_points.last()) else {
            return;
        };
        let first_x = first.x;
        let last_x = last.x;
        fill_points.push(Pos2::new(last_x, baseline_y));
        fill_points.push(Pos2::new(first_x, baseline_y));

        // Draw filled area
        ctx.painter.add(Shape::convex_polygon(
            fill_points,
            self.options.fill_color,
            Stroke::NONE,
        ));

        // Draw line on top
        ctx.painter.add(Shape::line(
            line_points,
            Stroke::new(self.options.line_width, self.options.line_color),
        ));
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn color(&self) -> Color32 {
        self.options.line_color
    }
}
