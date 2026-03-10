//! Order Flow Heatmap Strip
//!
//! A horizontal strip visualization below the chart showing volume flow
//! over time, color-coded by delta (buy/sell pressure).

use egui::{Color32, Pos2, Rect, Ui, Vec2};

use crate::ext::HasDesignTokens;
use crate::tokens::DESIGN_TOKENS;

/// A single cell in the heatmap strip
#[derive(Debug, Clone, Copy)]
pub struct HeatmapCell {
    /// Bar index this cell corresponds to
    pub bar_idx: usize,
    /// Delta value (positive = buying, negative = selling)
    pub delta: f64,
    /// Total volume at this bar
    pub volume: f64,
    /// Normalized intensity (0.0-1.0) for color mapping
    pub intensity: f32,
}

impl HeatmapCell {
    /// Create a new heatmap cell
    pub fn new(bar_idx: usize, delta: f64, volume: f64) -> Self {
        Self {
            bar_idx,
            delta,
            volume,
            intensity: 0.5, // Default mid-intensity
        }
    }

    /// Set intensity based on delta relative to max delta
    pub fn with_normalized_intensity(mut self, max_abs_delta: f64) -> Self {
        if max_abs_delta > 0.0 {
            // Normalize to 0.0-1.0 range where 0.5 is neutral
            let normalized = (self.delta / max_abs_delta) as f32;
            self.intensity = (normalized + 1.0) / 2.0; // Map -1..1 to 0..1
        }
        self
    }

    /// Check if this is a buying cell (positive delta)
    pub fn is_buying(&self) -> bool {
        self.delta > 0.0
    }

    /// Check if this is a selling cell (negative delta)
    pub fn is_selling(&self) -> bool {
        self.delta < 0.0
    }
}

/// Configuration for the heatmap strip
#[derive(Debug, Clone)]
pub struct HeatmapStripConfig {
    /// Height of the strip in pixels
    pub height: f32,
    /// Minimum cell width
    pub min_cell_width: f32,
    /// Whether to show volume-based intensity
    pub volume_weighted: bool,
    /// Whether to show gradient or solid colors
    pub use_gradient: bool,
    /// Opacity of the strip (0.0-1.0)
    pub opacity: f32,
}

impl Default for HeatmapStripConfig {
    fn default() -> Self {
        Self {
            height: DESIGN_TOKENS.sizing.charts_ext.heatmap_strip_height,
            min_cell_width: DESIGN_TOKENS.spacing.sm,
            volume_weighted: true,
            use_gradient: true,
            opacity: 0.85,
        }
    }
}

/// Heatmap strip visualization
#[derive(Debug, Clone, Default)]
pub struct HeatmapStrip {
    /// Cells to render
    pub cells: Vec<HeatmapCell>,
    /// Configuration
    pub config: HeatmapStripConfig,
}

impl HeatmapStrip {
    /// Create a new empty heatmap strip
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            config: HeatmapStripConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: HeatmapStripConfig) -> Self {
        Self {
            cells: Vec::new(),
            config,
        }
    }

    /// Set the cells from delta/volume data
    pub fn set_data(&mut self, data: impl IntoIterator<Item = (usize, f64, f64)>) {
        self.cells.clear();

        let cells: Vec<HeatmapCell> = data
            .into_iter()
            .map(|(idx, delta, volume)| HeatmapCell::new(idx, delta, volume))
            .collect();

        // Find max absolute delta for normalization
        let max_abs_delta = cells.iter().map(|c| c.delta.abs()).fold(0.0f64, f64::max);

        // Normalize intensities
        self.cells = cells
            .into_iter()
            .map(|c| c.with_normalized_intensity(max_abs_delta))
            .collect();
    }

    /// Add a single cell
    pub fn add_cell(&mut self, bar_idx: usize, delta: f64, volume: f64) {
        self.cells.push(HeatmapCell::new(bar_idx, delta, volume));
    }

    /// Clear all cells
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// Render the heatmap strip
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `strip_rect` - The rectangle where the strip should be drawn
    /// * `visible_range` - The visible bar index range (start, end)
    /// * `bar_width` - Width of each bar in pixels
    pub fn render(
        &self,
        ui: &mut Ui,
        strip_rect: Rect,
        visible_range: (usize, usize),
        bar_width: f32,
    ) {
        if self.cells.is_empty() {
            return;
        }

        let painter = ui.painter();

        // Get theme colors
        let bullish = ui.bullish_color();
        let bearish = ui.bearish_color();
        let neutral = ui.chart_bg();

        // Draw background
        painter.rect_filled(strip_rect, 0.0, neutral);

        // Calculate cell dimensions
        let cell_width = bar_width.max(self.config.min_cell_width);

        // Filter and render visible cells
        for cell in &self.cells {
            if cell.bar_idx < visible_range.0 || cell.bar_idx > visible_range.1 {
                continue;
            }

            // Calculate x position relative to visible range
            let relative_idx = cell.bar_idx - visible_range.0;
            let x = strip_rect.left() + (relative_idx as f32 * cell_width);

            // Skip if outside strip rect
            if x + cell_width < strip_rect.left() || x > strip_rect.right() {
                continue;
            }

            let cell_rect = Rect::from_min_size(
                Pos2::new(x, strip_rect.top()),
                Vec2::new(cell_width, strip_rect.height()),
            );

            // Calculate color based on delta and intensity
            let color = if self.config.use_gradient {
                self.gradient_color(cell, bullish, bearish, neutral)
            } else {
                self.solid_color(cell, bullish, bearish)
            };

            // Apply opacity
            let [r, g, b, _] = color.to_array();
            let alpha = (255.0 * self.config.opacity) as u8;
            let final_color = Color32::from_rgba_unmultiplied(r, g, b, alpha);

            painter.rect_filled(cell_rect, 0.0, final_color);
        }

        // Draw border
        painter.rect_stroke(
            strip_rect,
            0.0,
            egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_color()),
            egui::StrokeKind::Inside,
        );
    }

    /// Calculate gradient color based on intensity
    fn gradient_color(
        &self,
        cell: &HeatmapCell,
        bullish: Color32,
        bearish: Color32,
        neutral: Color32,
    ) -> Color32 {
        let intensity = cell.intensity;

        if intensity > 0.5 {
            // Bullish (green) gradient
            let t = (intensity - 0.5) * 2.0; // 0.0 to 1.0
            lerp_color(neutral, bullish, t)
        } else {
            // Bearish (red) gradient
            let t = (0.5 - intensity) * 2.0; // 0.0 to 1.0
            lerp_color(neutral, bearish, t)
        }
    }

    /// Calculate solid color (no gradient)
    fn solid_color(&self, cell: &HeatmapCell, bullish: Color32, bearish: Color32) -> Color32 {
        if cell.is_buying() {
            bullish
        } else if cell.is_selling() {
            bearish
        } else {
            Color32::TRANSPARENT
        }
    }
}

/// Linear interpolation between two colors
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_array();
    let [br, bg, bb, ba] = b.to_array();

    Color32::from_rgba_unmultiplied(
        lerp_u8(ar, br, t),
        lerp_u8(ag, bg, t),
        lerp_u8(ab, bb, t),
        lerp_u8(aa, ba, t),
    )
}

/// Linear interpolation between two u8 values
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heatmap_cell_creation() {
        let cell = HeatmapCell::new(0, 100.0, 500.0);
        assert!(cell.is_buying());
        assert!(!cell.is_selling());

        let cell = HeatmapCell::new(1, -50.0, 300.0);
        assert!(!cell.is_buying());
        assert!(cell.is_selling());
    }

    #[test]
    fn test_intensity_normalization() {
        let cell = HeatmapCell::new(0, 100.0, 500.0).with_normalized_intensity(200.0);
        // 100/200 = 0.5, then (0.5 + 1) / 2 = 0.75
        assert!((cell.intensity - 0.75).abs() < 0.01);

        let cell = HeatmapCell::new(0, -100.0, 500.0).with_normalized_intensity(200.0);
        // -100/200 = -0.5, then (-0.5 + 1) / 2 = 0.25
        assert!((cell.intensity - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_strip_data_setting() {
        let mut strip = HeatmapStrip::new();
        strip.set_data(vec![(0, 100.0, 500.0), (1, -50.0, 300.0), (2, 75.0, 400.0)]);

        assert_eq!(strip.cells.len(), 3);
        assert!(strip.cells[0].is_buying());
        assert!(strip.cells[1].is_selling());
    }

    #[test]
    fn test_color_lerp() {
        let white = Color32::WHITE;
        let black = Color32::BLACK;

        let mid = lerp_color(white, black, 0.5);
        assert_eq!(mid.r(), 128);
        assert_eq!(mid.g(), 128);
        assert_eq!(mid.b(), 128);
    }
}
