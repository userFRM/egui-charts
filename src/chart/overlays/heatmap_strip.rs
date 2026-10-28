//! Order Flow Heatmap Strip
//!
//! A horizontal strip visualization below the chart showing volume flow
//! over time, color-coded by delta (buy/sell pressure).

use egui::{Color32, Pos2, Rect, Ui, Vec2};

use crate::theme::ThemeExt;
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
            self.intensity = ((normalized + 1.0) / 2.0).clamp(0.0, 1.0); // Map -1..1 to 0..1
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
    /// Minimum painted cell width; bar positions still follow the caller's mapping.
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
    /// Raw cells ordered by bar index; intensities are normalized when read or rendered.
    cells: Vec<HeatmapCell>,
    max_abs_delta: f64,
    max_volume: f64,
    /// Configuration
    pub config: HeatmapStripConfig,
}

impl HeatmapStrip {
    /// Create a new empty heatmap strip
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom configuration
    pub fn with_config(config: HeatmapStripConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    /// Set the cells from delta/volume data, sorting them by bar index.
    pub fn set_data(&mut self, data: impl IntoIterator<Item = (usize, f64, f64)>) {
        self.clear();
        for (idx, delta, volume) in data {
            self.max_abs_delta = self.max_abs_delta.max(delta.abs());
            self.max_volume = self.max_volume.max(volume);
            self.cells.push(HeatmapCell::new(idx, delta, volume));
        }
        self.cells.sort_by_key(|cell| cell.bar_idx);
    }

    /// Add a single cell, keeping the cells ordered by bar index. Appending in bar-index
    /// order takes amortized constant time.
    pub fn add_cell(&mut self, bar_idx: usize, delta: f64, volume: f64) {
        self.max_abs_delta = self.max_abs_delta.max(delta.abs());
        self.max_volume = self.max_volume.max(volume);
        let at = self.cells.partition_point(|cell| cell.bar_idx <= bar_idx);
        self.cells
            .insert(at, HeatmapCell::new(bar_idx, delta, volume));
    }

    /// Cells in bar-index order, with intensities normalized against all held deltas.
    pub fn cells(&self) -> impl ExactSizeIterator<Item = HeatmapCell> + '_ {
        self.cells
            .iter()
            .map(|cell| cell.with_normalized_intensity(self.max_abs_delta))
    }

    /// Clear all cells and their normalization maxima.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.max_abs_delta = 0.0;
        self.max_volume = 0.0;
    }

    /// Render the heatmap strip
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `strip_rect` - The rectangle where the strip should be drawn
    /// * `visible_range` - The inclusive visible bar index range (start, end)
    /// * `bar_width` - Spacing between bar centres in logical points
    /// * `x` - The screen x coordinate of each absolute bar index's centre
    pub fn render(
        &self,
        ui: &mut Ui,
        strip_rect: Rect,
        visible_range: (usize, usize),
        bar_width: f32,
        x: impl Fn(usize) -> f32,
    ) {
        if self.cells.is_empty()
            || visible_range.0 > visible_range.1
            || !bar_width.is_finite()
            || bar_width <= 0.0
        {
            return;
        }

        let painter = ui
            .painter()
            .with_clip_rect(strip_rect.intersect(ui.clip_rect()));

        // Get theme colors
        let colors = ui.active_theme().chart;
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;
        let neutral = colors.bg;

        // Draw background
        painter.rect_filled(strip_rect, 0.0, neutral);

        // Calculate cell dimensions
        let cell_width = bar_width.max(self.config.min_cell_width);

        let max_volume = self.max_volume;
        let first = self
            .cells
            .partition_point(|cell| cell.bar_idx < visible_range.0);
        let end = self
            .cells
            .partition_point(|cell| cell.bar_idx <= visible_range.1);
        for cell in &self.cells[first..end] {
            // Widen the paint to the right without changing the next bar's position.
            let left = x(cell.bar_idx) - bar_width * 0.5;
            let cell_rect = Rect::from_min_size(
                Pos2::new(left, strip_rect.top()),
                Vec2::new(cell_width, strip_rect.height()),
            )
            .intersect(strip_rect);
            if !left.is_finite() || !cell_rect.is_positive() {
                continue;
            }

            // Calculate color based on delta and intensity
            let color = if self.config.use_gradient {
                let mut shown = cell.with_normalized_intensity(self.max_abs_delta);
                if self.config.volume_weighted {
                    shown.intensity = if max_volume > 0.0 {
                        (0.5 + 0.5 * cell.delta.signum() * cell.volume / max_volume).clamp(0.0, 1.0)
                            as f32
                    } else {
                        0.5
                    };
                }
                self.gradient_color(&shown, bullish, bearish, neutral)
            } else {
                self.solid_color(cell, bullish, bearish)
            };

            // Apply opacity
            let weight = if self.config.volume_weighted && !self.config.use_gradient {
                if max_volume > 0.0 {
                    (cell.volume / max_volume).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                }
            } else {
                1.0
            };
            let final_color = color.gamma_multiply(self.config.opacity * weight);

            painter.rect_filled(cell_rect, 0.0, final_color);
        }

        // Draw border
        painter.rect_stroke(
            strip_rect,
            0.0,
            egui::Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            ),
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

        assert_eq!(strip.cells().len(), 3);
        assert!(strip.cells().next().unwrap().is_buying());
        assert!(strip.cells().nth(1).unwrap().is_selling());
        strip.add_cell(3, -200.0, 600.0);
        assert_eq!(strip.cells().nth(3).unwrap().intensity, 0.0);
        assert_eq!(strip.cells().next().unwrap().intensity, 0.75);

        let data = [
            (0, 0.0, 5.0),
            (1, 100.0, 500.0),
            (2, -50.0, 300.0),
            (3, 75.0, 400.0),
            (4, -200.0, 600.0),
        ];
        let mut appended = HeatmapStrip::new();
        for (n, &(idx, delta, volume)) in data.iter().enumerate() {
            appended.add_cell(idx, delta, volume);
            strip.set_data(data[..=n].iter().copied().rev());
            let expected = data[..=n]
                .iter()
                .map(|(_, delta, _)| delta.abs())
                .fold(0.0, f64::max);
            let reference: Vec<_> = data[..=n]
                .iter()
                .map(|&(idx, delta, volume)| {
                    let cell =
                        HeatmapCell::new(idx, delta, volume).with_normalized_intensity(expected);
                    (cell.bar_idx, cell.intensity)
                })
                .collect();
            for built in [&strip, &appended] {
                assert_eq!(
                    built
                        .cells()
                        .map(|c| (c.bar_idx, c.intensity))
                        .collect::<Vec<_>>(),
                    reference
                );
            }
        }
        appended.clear();
        appended.add_cell(0, 1.0, 1.0);
        assert_eq!(appended.cells().next().unwrap().intensity, 1.0);
        appended.add_cell(5, 1.0, 1.0);
        appended.add_cell(2, 1.0, 1.0);
        assert_eq!(
            appended.cells().map(|c| c.bar_idx).collect::<Vec<_>>(),
            [0, 2, 5]
        );

        strip.set_data([(0, 10.0, 1.0), (1, -10.0, 10.0), (2, 0.0, 5.0)]);
        strip.config.use_gradient = false;
        strip.config.opacity = 1.0;
        let ctx = egui::Context::default();
        for (weighted, expected) in [(false, [255, 255, 0]), (true, [26, 255, 0])] {
            strip.config.volume_weighted = weighted;
            let mut output = ctx.run_ui(Default::default(), |ui| {
                strip.render(
                    ui,
                    Rect::from_min_size(Pos2::ZERO, Vec2::new(150.0, 24.0)),
                    (0, 2),
                    50.0,
                    |i| 25.0 + i as f32 * 50.0,
                );
            });
            let alphas: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.rect.width() == 50.0 => Some(rect.fill.a()),
                    _ => None,
                })
                .collect();
            output.textures_delta.clear();
            assert_eq!(alphas, expected, "solid volume weighting: {weighted}");
        }
    }

    /// At two points per bar, x = 401 is the centre of bar 200, and 300 bars
    /// occupy exactly 600 points even when the minimum painted width is four.
    #[test]
    fn dense_cells_follow_the_bars_and_stay_inside_the_strip() {
        let mut strip = HeatmapStrip::new();
        strip.set_data((0..300).map(|i| (i, if i < 150 { 1.0 } else { -1.0 }, 1.0)));
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 24.0));
        for (range, offset, probe, expected) in [
            ((0, 299), 0.0, 401.0, 200),
            ((0, 299), -0.7, 400.3, 200),
            ((100, 299), 20.0, 221.0, 100),
        ] {
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 100.0))),
                    ..Default::default()
                },
                |ui| {
                    strip.render(ui, rect, range, 2.0, |i| {
                        1.0 + 2.0 * (i - range.0) as f32 + offset
                    })
                },
            );
            output.textures_delta.clear();
            let cells: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.rect.width() <= 4.01 && r.stroke.width == 0.0 => {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                cells.len(),
                range.1 - range.0 + 1,
                "range {range:?}, offset {offset}"
            );
            assert_eq!(
                cells
                    .iter()
                    .rposition(|r| r.left() <= probe && r.right() > probe),
                Some(expected)
            );
            assert!(cells.iter().all(|r| rect.contains_rect(*r)));
        }
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
