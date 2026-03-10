//! Line series renderer.
//!
//! Renders a simple line chart by connecting consecutive data points with
//! straight segments. Supports solid, dashed, and dotted line styles,
//! configurable width and color, and optional data-point markers.

use super::types::{Series, SeriesData, SeriesRenderContext, SeriesType, idx_to_x, price_to_y};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Shape, Stroke};

/// Visual configuration for a [`LineSeries`].
#[derive(Debug, Clone)]
pub struct LineSeriesOptions {
    /// Line color
    pub color: Color32,

    /// Line width
    pub line_width: f32,

    /// Line style (solid/dashed)
    pub line_style: LineStyle,

    /// Show points at data values
    pub show_points: bool,

    /// Point radius
    pub point_radius: f32,
}

/// Stroke style for a line series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    /// Continuous solid line.
    Solid,
    /// Dashed line (equal dash and gap lengths).
    Dashed,
    /// Dotted line (short dots with gaps).
    Dotted,
}

impl Default for LineSeriesOptions {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.extended.info,
            line_width: 2.0,
            line_style: LineStyle::Solid,
            show_points: false,
            point_radius: 3.0,
        }
    }
}

/// A line series that plots data points connected by straight line segments.
pub struct LineSeries {
    data: Vec<SeriesData>,
    options: LineSeriesOptions,
    name: String,
}

impl LineSeries {
    /// Create a new line series with the given name and data, using default options.
    pub fn new(name: impl Into<String>, data: Vec<SeriesData>) -> Self {
        Self {
            data,
            options: LineSeriesOptions::default(),
            name: name.into(),
        }
    }

    /// Override all visual options at once.
    pub fn with_options(mut self, options: LineSeriesOptions) -> Self {
        self.options = options;
        self
    }

    /// Set the line color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.options.color = color;
        self
    }

    /// Set the line stroke width in pixels.
    pub fn with_line_width(mut self, width: f32) -> Self {
        self.options.line_width = width;
        self
    }
}

impl Series for LineSeries {
    fn series_type(&self) -> SeriesType {
        SeriesType::Line
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
                min = min.min(value);
                max = max.max(value);
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

        // Collect points
        let mut points = Vec::new();
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
                points.push(Pos2::new(x, y));
            }
        }

        if points.len() < 2 {
            return;
        }

        // Draw line
        let stroke = match self.options.line_style {
            LineStyle::Solid => Stroke::new(self.options.line_width, self.options.color),
            LineStyle::Dashed => Stroke::new(self.options.line_width, self.options.color),
            LineStyle::Dotted => Stroke::new(self.options.line_width, self.options.color),
        };

        ctx.painter.add(Shape::line(points.clone(), stroke));

        // Draw points if enabled
        if self.options.show_points {
            for point in points {
                ctx.painter
                    .circle_filled(point, self.options.point_radius, self.options.color);
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
