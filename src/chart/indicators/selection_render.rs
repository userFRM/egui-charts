//! Selection rendering for indicators.
//!
//! Draws selection dots on the selected indicator line.

use crate::chart::coords::ChartMapping;
use crate::studies::Indicator;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Stroke};

/// Configuration for selection dots
#[derive(Clone, Debug)]
pub struct SelectionDotConfig {
    /// Radius of the selection dot
    pub radius: f32,
    /// Fill color of the dot
    pub fill_color: Color32,
    /// Border color of the dot
    pub border_color: Color32,
    /// Border width
    pub border_width: f32,
    /// Interval between dots (every N bars)
    pub dot_interval: usize,
}

impl Default for SelectionDotConfig {
    fn default() -> Self {
        Self {
            radius: 4.0,
            fill_color: Color32::WHITE,
            border_color: DESIGN_TOKENS.semantic.brand.accent, // accent blue
            border_width: 2.0,
            dot_interval: 5,
        }
    }
}

/// Render selection dots on an indicator line
pub fn render_indicator_selection_dots<F>(
    painter: &Painter,
    indicator: &dyn Indicator,
    line_idx: usize,
    visible_range: std::ops::Range<usize>,
    coords: &ChartMapping,
    price_to_y: F,
    config: &SelectionDotConfig,
) where
    F: Fn(f64) -> f32,
{
    if !indicator.plot_style(line_idx).visible {
        return;
    }

    // Draw dots at regular intervals
    for i in visible_range.start
        ..visible_range
            .end
            .max(coords.x_to_idx(coords.rect.right()).saturating_add(1))
    {
        // Only draw at intervals
        if i % config.dot_interval != 0 {
            continue;
        }

        let value = indicator.plot_value(line_idx, i);

        let Some(value) = value else { continue };

        let x = coords.idx_to_x(i);
        let y = price_to_y(value);

        // Skip if outside visible area
        if x < coords.rect.min.x || x > coords.rect.max.x {
            continue;
        }
        if y < coords.rect.min.y || y > coords.rect.max.y {
            continue;
        }

        let center = Pos2::new(x, y);

        // Draw dot with border
        painter.circle(
            center,
            config.radius + config.border_width,
            config.border_color,
            Stroke::NONE,
        );
        painter.circle(center, config.radius, config.fill_color, Stroke::NONE);
    }
}
