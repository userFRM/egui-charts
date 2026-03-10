//! Separate-pane indicator rendering with direct egui painting.
//!
//! This module renders oscillator and multi-line indicators (RSI, MACD, Stochastic,
//! etc.) in dedicated panels below the main chart. It uses direct [`egui::Painter`]
//! calls rather than `egui_plot` to ensure pixel-perfect x-axis alignment with
//! the main candlestick chart.
//!
//! # Usage
//!
//! Indicator panes are typically managed by [`super::Chart::show_with_indicators`], but
//! they can also be used standalone:
//!
//! ```rust,ignore
//! use egui_charts::widget::indicator_pane::{IndicatorPane, IndicatorPaneConfig};
//!
//! let config = IndicatorPaneConfig::rsi();
//! let mut pane = IndicatorPane::with_config(egui::Id::new("chart_x"), config);
//! pane.show_aligned(ui, &rsi_indicator, &bars, visible_range, coords);
//! ```
//!
//! # Coordinate Alignment
//!
//! The [`IndicatorCoordParams`] struct captures the main chart's bar spacing,
//! right offset, and base index so that indicator panes draw their data points
//! at exactly the same x-coordinates as the corresponding candlesticks above.

use crate::ext::HasDesignTokens;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui};
use std::ops::Range;

/// Chart layout constants matching main chart (from ChartConfig defaults)
mod layout {
    /// Left padding (matches ChartConfig::padding default)
    pub const LEFT_PADDING: f32 = 40.0;
    /// Right padding (matches ChartConfig::padding * 2.0)
    pub const RIGHT_PADDING: f32 = 80.0;
    /// Minimum pixel spacing between vertical grid lines
    pub const MIN_GRID_SPACING: f32 = 80.0;
}

/// Indicator panel sizing constants
mod indicator_sizing {
    /// Standard panel height for oscillators (RSI, Stochastic)
    pub const OSCILLATOR_HEIGHT: f32 = 120.0;
    /// Standard panel height for multi-line indicators (MACD)
    pub const MULTI_LINE_HEIGHT: f32 = 150.0;
}

/// Configuration for an indicator panel's visual appearance and behavior.
///
/// Controls the panel height, y-axis range, zone lines (overbought/oversold),
/// legend display, and grid visibility. Pre-built configurations are available
/// for common indicators via [`IndicatorPaneConfig::rsi`],
/// [`IndicatorPaneConfig::macd`], and [`IndicatorPaneConfig::stochastic`].
#[derive(Debug, Clone)]
pub struct IndicatorPaneConfig {
    /// Panel height in pixels
    pub height: f32,
    /// Fixed y-axis range (None = auto-scale)
    pub y_range: Option<(f64, f64)>,
    /// Show horizontal zone lines (e.g., RSI 70/30)
    pub show_zones: bool,
    /// Zone line levels and colors
    pub zones: Vec<(f64, Color32, &'static str)>,
    /// Show legend
    pub show_legend: bool,
    /// Show grid
    pub show_grid: bool,
}

impl Default for IndicatorPaneConfig {
    fn default() -> Self {
        Self {
            height: DESIGN_TOKENS.sizing.panel.bottom_default_height,
            y_range: None,
            show_zones: false,
            zones: Vec::new(),
            show_legend: true,
            show_grid: true,
        }
    }
}

impl IndicatorPaneConfig {
    /// Create config for RSI indicator
    pub fn rsi() -> Self {
        Self {
            height: indicator_sizing::OSCILLATOR_HEIGHT,
            y_range: Some((0.0, 100.0)),
            show_zones: true,
            zones: vec![
                (70.0, DESIGN_TOKENS.semantic.extended.bearish, "Overbought"),
                (50.0, DESIGN_TOKENS.semantic.extended.chart_text, "Neutral"),
                (30.0, DESIGN_TOKENS.semantic.extended.bullish, "Oversold"),
            ],
            show_legend: true,
            show_grid: true,
        }
    }

    /// Create config for MACD indicator
    pub fn macd() -> Self {
        Self {
            height: indicator_sizing::MULTI_LINE_HEIGHT,
            y_range: None, // Auto-scale
            show_zones: true,
            zones: vec![(0.0, DESIGN_TOKENS.semantic.extended.chart_text, "Zero Line")],
            show_legend: true,
            show_grid: true,
        }
    }

    /// Create config for Stochastic indicator
    pub fn stochastic() -> Self {
        Self {
            height: indicator_sizing::OSCILLATOR_HEIGHT,
            y_range: Some((0.0, 100.0)),
            show_zones: true,
            zones: vec![
                (80.0, DESIGN_TOKENS.semantic.extended.bearish, "Overbought"),
                (20.0, DESIGN_TOKENS.semantic.extended.bullish, "Oversold"),
            ],
            show_legend: true,
            show_grid: true,
        }
    }
}

// Re-export ChartMapping from coords module for x-axis alignment
pub use crate::chart::coords::ChartMapping;

/// Coordinate parameters for aligning indicator panes with the main chart.
///
/// Wraps the parameters needed to construct a [`ChartMapping`] so that
/// indicator data points are drawn at the same x-positions as their
/// corresponding bars in the main chart. Obtained from the chart's time scale
/// after rendering.
#[derive(Clone, Copy, Debug)]
pub struct IndicatorCoordParams {
    /// Bar spacing in pixels
    pub bar_spacing: f32,
    /// Right offset (bars to the right of last data point)
    pub right_offset: f32,
    /// Base index (index of last data point)
    pub base_idx: usize,
    /// Start index (index of first visible bar)
    pub start_idx: usize,
}

impl IndicatorCoordParams {
    /// Creates coordinate parameters from the main chart's time-scale state.
    ///
    /// These values are typically obtained from the chart after rendering via
    /// [`super::Chart::get_chart_mapping`] or the internal time-scale accessors.
    pub fn new(bar_spacing: f32, right_offset: f32, base_idx: usize, start_idx: usize) -> Self {
        Self {
            bar_spacing,
            right_offset,
            base_idx,
            start_idx,
        }
    }

    /// Creates a [`ChartMapping`] from these params, the panel's screen rect, and y-range.
    ///
    /// The resulting mapping converts bar indices to x-pixel positions and
    /// price/indicator values to y-pixel positions within the given rect.
    pub fn to_mapping(&self, rect: Rect, y_min: f64, y_max: f64) -> ChartMapping {
        ChartMapping::new(
            rect,
            self.bar_spacing,
            self.start_idx,
            self.base_idx,
            self.right_offset,
            y_min,
            y_max,
        )
    }
}

/// A separate-pane indicator panel rendered via direct egui painting.
///
/// Each `IndicatorPane` renders one indicator (RSI, MACD, etc.) in its own
/// rectangular area below the main chart. The pane includes:
/// - Indicator line(s) with proper colors
/// - Horizontal zone lines (e.g., RSI 70/30 overbought/oversold)
/// - Semi-transparent zone fills for overbought/oversold regions
/// - Y-axis labels on the right side
/// - A legend showing the indicator name and current value
/// - Optional vertical grid lines aligned with the main chart
pub struct IndicatorPane {
    config: IndicatorPaneConfig,
    #[allow(dead_code)]
    linked_axis_id: egui::Id,
}

impl IndicatorPane {
    /// Create a new indicator panel with default configuration
    pub fn new(linked_axis_id: egui::Id) -> Self {
        Self {
            config: IndicatorPaneConfig::default(),
            linked_axis_id,
        }
    }

    /// Create a new indicator panel with custom configuration
    pub fn with_config(linked_axis_id: egui::Id, config: IndicatorPaneConfig) -> Self {
        Self {
            config,
            linked_axis_id,
        }
    }

    /// Render the indicator panel with proper x-axis alignment
    pub fn show(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        bars: &[Bar],
        visible_range: Range<usize>,
    ) {
        if !indicator.is_visible() {
            return;
        }

        // Get chart background and text colors from theme
        let chart_bg = ui.chart_bg();
        let text_color = ui.style().visuals.text_color();
        let grid_color = ui.chart_grid();

        // Get live theme colors for zones
        let bullish = ui.bullish_color();
        let bearish = ui.bearish_color();

        // Allocate the panel area
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, _response) = ui.allocate_exact_size(panel_size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter();

        // Fill background
        painter.rect_filled(rect, 0.0, chart_bg);

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let chart_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + left_padding, rect.min.y),
            Pos2::new(rect.max.x - right_padding, rect.max.y),
        );

        // Determine y-axis range
        let (y_min, y_max) = self.calculate_y_range(indicator, &visible_range);

        // Draw zone fills (colored bands)
        if self.config.show_zones && self.config.zones.len() >= 2 {
            self.draw_zone_fills(painter, chart_rect, y_min, y_max, bullish, bearish);
        }

        // Draw horizontal grid lines and zone lines
        self.draw_horizontal_lines(
            painter, chart_rect, y_min, y_max, grid_color, bullish, bearish,
        );

        // Draw indicator lines
        self.draw_indicator_lines(
            painter,
            chart_rect,
            indicator,
            bars,
            &visible_range,
            y_min,
            y_max,
        );

        // Draw y-axis labels
        self.draw_y_axis_labels(painter, rect, chart_rect, y_min, y_max, text_color);

        // Draw indicator legend at top-left
        self.draw_legend(painter, chart_rect, indicator, text_color);
    }

    /// Render with explicit coordinate parameters for perfect x-axis alignment.
    ///
    /// Uses [`IndicatorCoordParams`] from the main chart to ensure that each
    /// indicator data point is drawn at the exact same x-coordinate as its
    /// corresponding bar in the price chart above.
    pub fn show_aligned(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        _bars: &[Bar],
        visible_range: Range<usize>,
        coords: IndicatorCoordParams,
    ) {
        if !indicator.is_visible() {
            return;
        }

        // Get chart background and text colors from theme
        let chart_bg = ui.chart_bg();
        let text_color = ui.style().visuals.text_color();
        let grid_color = ui.chart_grid();

        // Get live theme colors for zones
        let bullish = ui.bullish_color();
        let bearish = ui.bearish_color();

        // Allocate the panel area
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, _response) = ui.allocate_exact_size(panel_size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter();

        // Fill background
        painter.rect_filled(rect, 0.0, chart_bg);

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let chart_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + left_padding, rect.min.y),
            Pos2::new(rect.max.x - right_padding, rect.max.y),
        );

        // Determine y-axis range
        let (y_min, y_max) = self.calculate_y_range(indicator, &visible_range);

        // Draw zone fills (colored bands)
        if self.config.show_zones && self.config.zones.len() >= 2 {
            self.draw_zone_fills(painter, chart_rect, y_min, y_max, bullish, bearish);
        }

        // Draw horizontal grid lines and zone lines
        self.draw_horizontal_lines(
            painter, chart_rect, y_min, y_max, grid_color, bullish, bearish,
        );

        // Draw vertical grid lines (aligned with main chart)
        if self.config.show_grid {
            self.draw_vertical_grid_aligned(painter, chart_rect, &coords, grid_color);
        }

        // Draw indicator lines with aligned coordinates
        self.draw_indicator_lines_aligned(
            painter,
            chart_rect,
            indicator,
            &visible_range,
            y_min,
            y_max,
            &coords,
        );

        // Draw y-axis labels
        self.draw_y_axis_labels(painter, rect, chart_rect, y_min, y_max, text_color);

        // Draw indicator legend at top-left
        self.draw_legend(painter, chart_rect, indicator, text_color);
    }

    /// Render with coordinate alignment and return layout info for hit testing.
    ///
    /// Like [`IndicatorPane::show_aligned`], but returns the panel and chart
    /// rects, y-axis range, and an interactive [`egui::Response`]. This allows
    /// the caller to implement click-on-line selection, tooltip display, or
    /// other interactive features on indicator panes.
    ///
    /// Returns `None` if the indicator is not visible.
    pub fn show_aligned_interactive(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        _bars: &[Bar],
        visible_range: Range<usize>,
        coords: IndicatorCoordParams,
    ) -> Option<(Rect, Rect, f64, f64, Response)> {
        if !indicator.is_visible() {
            return None;
        }

        // Get chart background and text colors from theme
        let chart_bg = ui.chart_bg();
        let text_color = ui.style().visuals.text_color();
        let grid_color = ui.chart_grid();

        // Get live theme colors for zones
        let bullish = ui.bullish_color();
        let bearish = ui.bearish_color();

        // Allocate the panel area with click sense
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, response) = ui.allocate_exact_size(panel_size, Sense::click());

        if !ui.is_rect_visible(rect) {
            return None;
        }

        let painter = ui.painter();

        // Fill background
        painter.rect_filled(rect, 0.0, chart_bg);

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let chart_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + left_padding, rect.min.y),
            Pos2::new(rect.max.x - right_padding, rect.max.y),
        );

        // Determine y-axis range
        let (y_min, y_max) = self.calculate_y_range(indicator, &visible_range);

        // Draw zone fills (colored bands)
        if self.config.show_zones && self.config.zones.len() >= 2 {
            self.draw_zone_fills(painter, chart_rect, y_min, y_max, bullish, bearish);
        }

        // Draw horizontal grid lines and zone lines
        self.draw_horizontal_lines(
            painter, chart_rect, y_min, y_max, grid_color, bullish, bearish,
        );

        // Draw vertical grid lines (aligned with main chart)
        if self.config.show_grid {
            self.draw_vertical_grid_aligned(painter, chart_rect, &coords, grid_color);
        }

        // Draw indicator lines with aligned coordinates
        self.draw_indicator_lines_aligned(
            painter,
            chart_rect,
            indicator,
            &visible_range,
            y_min,
            y_max,
            &coords,
        );

        // Draw y-axis labels
        self.draw_y_axis_labels(painter, rect, chart_rect, y_min, y_max, text_color);

        // Draw indicator legend at top-left
        self.draw_legend(painter, chart_rect, indicator, text_color);

        // Return panel info for hit testing
        Some((rect, chart_rect, y_min, y_max, response))
    }

    fn calculate_y_range(
        &self,
        indicator: &dyn Indicator,
        visible_range: &Range<usize>,
    ) -> (f64, f64) {
        if let Some((min, max)) = self.config.y_range {
            return (min, max);
        }

        // Auto-scale based on visible indicator values
        let values = indicator.values();
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;

        for i in visible_range.clone() {
            if let Some(value) = values.get(i) {
                match value {
                    IndicatorValue::Single(v) => {
                        min_val = min_val.min(*v);
                        max_val = max_val.max(*v);
                    }
                    IndicatorValue::Multiple(vals) => {
                        for v in vals {
                            min_val = min_val.min(*v);
                            max_val = max_val.max(*v);
                        }
                    }
                    IndicatorValue::None => {}
                }
            }
        }

        if min_val == f64::MAX {
            (0.0, 100.0) // Default fallback
        } else {
            // Add 10% padding
            let range = max_val - min_val;
            let padding = range * 0.1;
            (min_val - padding, max_val + padding)
        }
    }

    fn draw_zone_fills(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        y_min: f64,
        y_max: f64,
        bullish: Color32,
        bearish: Color32,
    ) {
        // For RSI-style indicators, fill zones with semi-transparent colors
        // Overbought zone (above 70) - bearish color
        // Oversold zone (below 30) - bullish color

        for (level, color, label) in &self.config.zones {
            let live_color = if *color == DESIGN_TOKENS.semantic.extended.bullish {
                bullish
            } else if *color == DESIGN_TOKENS.semantic.extended.bearish {
                bearish
            } else {
                continue; // Skip neutral zones for fills
            };

            let y = self.value_to_y(*level, rect, y_min, y_max);

            // Create semi-transparent fill color
            let fill_color = Color32::from_rgba_unmultiplied(
                live_color.r(),
                live_color.g(),
                live_color.b(),
                25, // Very transparent
            );

            if label.contains("Overbought") || label.contains("above") {
                // Fill from this level to top
                let fill_rect =
                    Rect::from_min_max(Pos2::new(rect.min.x, rect.min.y), Pos2::new(rect.max.x, y));
                painter.rect_filled(fill_rect, 0.0, fill_color);
            } else if label.contains("Oversold") || label.contains("below") {
                // Fill from this level to bottom
                let fill_rect =
                    Rect::from_min_max(Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, rect.max.y));
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }
        }
    }

    fn draw_horizontal_lines(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        y_min: f64,
        y_max: f64,
        grid_color: Color32,
        bullish: Color32,
        bearish: Color32,
    ) {
        // Draw zone lines with dashed style
        if self.config.show_zones {
            for (level, color, _label) in &self.config.zones {
                let live_color = if *color == DESIGN_TOKENS.semantic.extended.bullish {
                    bullish
                } else if *color == DESIGN_TOKENS.semantic.extended.bearish {
                    bearish
                } else {
                    *color
                };

                let y = self.value_to_y(*level, rect, y_min, y_max);
                if y >= rect.min.y && y <= rect.max.y {
                    // Draw dashed line
                    self.draw_dashed_line(
                        painter,
                        Pos2::new(rect.min.x, y),
                        Pos2::new(rect.max.x, y),
                        live_color,
                        1.0,
                        5.0,
                        3.0,
                    );
                }
            }
        }

        // Draw subtle grid lines
        if self.config.show_grid {
            let range = y_max - y_min;
            let step = self.calculate_nice_step(range, 5);

            let mut y_val = (y_min / step).ceil() * step;
            while y_val <= y_max {
                let y = self.value_to_y(y_val, rect, y_min, y_max);
                if y >= rect.min.y && y <= rect.max.y {
                    painter.line_segment(
                        [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                        Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
                    );
                }
                y_val += step;
            }
        }
    }

    /// Draw vertical grid lines aligned with main chart
    /// Uses the same coordinate system as the main chart for seamless grid appearance
    fn draw_vertical_grid_aligned(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        coords: &IndicatorCoordParams,
        grid_color: Color32,
    ) {
        // Create mapping for coordinate conversion (y range doesn't matter for grid)
        let mapping = coords.to_mapping(rect, 0.0, 1.0);

        // Calculate grid interval based on bar spacing (same algorithm as main chart)
        let min_pixel_spacing = layout::MIN_GRID_SPACING;
        let bars_per_grid = (min_pixel_spacing / coords.bar_spacing).ceil().max(1.0) as usize;

        // Round to nice intervals: 1, 2, 5, 10, 20, 50, 100, etc.
        let bars_per_grid = Self::nice_interval(bars_per_grid);

        // Calculate visible bar range
        let chart_width = rect.width();
        let visible_bars = (chart_width / coords.bar_spacing).ceil() as usize + 2;

        // Use start_idx from coords
        let start_idx = coords.start_idx;

        // Find the first grid line that's visible
        let first_visible_idx = if start_idx > 0 {
            (start_idx / bars_per_grid) * bars_per_grid
        } else {
            0
        };

        // Draw grid lines
        for i in 0..=(visible_bars / bars_per_grid + 2) {
            let grid_idx = first_visible_idx + i * bars_per_grid;

            // Convert bar index to x coord using ChartMapping
            let x = mapping.idx_to_x(grid_idx);

            // Skip if outside visible area
            if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
                continue;
            }

            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );
        }
    }

    /// Round to a nice interval (1, 2, 5, 10, 20, 50, 100, etc.)
    /// Uses the same fixed breakpoints as the main chart for consistent grid alignment
    fn nice_interval(raw: usize) -> usize {
        if raw <= 1 {
            return 1;
        }
        if raw <= 2 {
            return 2;
        }
        if raw <= 5 {
            return 5;
        }
        if raw <= 10 {
            return 10;
        }
        if raw <= 20 {
            return 20;
        }
        if raw <= 50 {
            return 50;
        }
        if raw <= 100 {
            return 100;
        }
        if raw <= 200 {
            return 200;
        }
        if raw <= 500 {
            return 500;
        }
        1000
    }

    fn draw_indicator_lines(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        bars: &[Bar],
        visible_range: &Range<usize>,
        y_min: f64,
        y_max: f64,
    ) {
        let values = indicator.values();
        let colors = indicator.colors();
        let line_cnt = indicator.line_cnt();

        // Calculate bar spacing from rect and visible range
        let visible_bars = visible_range.end.saturating_sub(visible_range.start);
        if visible_bars == 0 {
            return;
        }
        let bar_spacing = rect.width() / visible_bars as f32;

        for line_idx in 0..line_cnt {
            let color = colors.get(line_idx).copied().unwrap_or(Color32::WHITE);
            let mut points = Vec::new();

            for i in visible_range.clone() {
                if i >= bars.len() || i >= values.len() {
                    break;
                }

                let val = match &values[i] {
                    IndicatorValue::Single(v) => Some(*v),
                    IndicatorValue::Multiple(vals) => vals.get(line_idx).copied(),
                    IndicatorValue::None => None,
                };

                if let Some(v) = val {
                    // X position based on position in visible range
                    let local_idx = i - visible_range.start;
                    let x = rect.min.x + (local_idx as f32 + 0.5) * bar_spacing;
                    let y = self.value_to_y(v, rect, y_min, y_max);

                    if x >= rect.min.x && x <= rect.max.x {
                        points.push(Pos2::new(x, y));
                    }
                }
            }

            // Draw connected line segments
            if points.len() > 1 {
                for i in 0..points.len() - 1 {
                    painter.line_segment(
                        [points[i], points[i + 1]],
                        Stroke::new(DESIGN_TOKENS.stroke.medium, color),
                    );
                }
            }
        }
    }

    fn draw_indicator_lines_aligned(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        visible_range: &Range<usize>,
        y_min: f64,
        y_max: f64,
        coords: &IndicatorCoordParams,
    ) {
        let values = indicator.values();
        let colors = indicator.colors();
        let line_cnt = indicator.line_cnt();

        // Create mapping for coordinate conversion
        let mapping = coords.to_mapping(rect, y_min, y_max);

        for line_idx in 0..line_cnt {
            let color = colors.get(line_idx).copied().unwrap_or(Color32::WHITE);
            let mut points = Vec::new();

            for i in visible_range.clone() {
                if i >= values.len() {
                    break;
                }

                let val = match &values[i] {
                    IndicatorValue::Single(v) => Some(*v),
                    IndicatorValue::Multiple(vals) => vals.get(line_idx).copied(),
                    IndicatorValue::None => None,
                };

                if let Some(v) = val {
                    // Use the same coordinate formula as main chart
                    let x = mapping.idx_to_x(i);
                    let y = self.value_to_y(v, rect, y_min, y_max);

                    if x >= rect.min.x && x <= rect.max.x {
                        points.push(Pos2::new(x, y));
                    }
                }
            }

            // Draw connected line segments
            if points.len() > 1 {
                for i in 0..points.len() - 1 {
                    painter.line_segment(
                        [points[i], points[i + 1]],
                        Stroke::new(DESIGN_TOKENS.stroke.medium, color),
                    );
                }
            }
        }
    }

    fn draw_y_axis_labels(
        &self,
        painter: &egui::Painter,
        full_rect: Rect,
        chart_rect: Rect,
        y_min: f64,
        y_max: f64,
        text_color: Color32,
    ) {
        let range = y_max - y_min;
        let step = self.calculate_nice_step(range, 5);
        let label_x = chart_rect.max.x + DESIGN_TOKENS.spacing.sm;

        let mut y_val = (y_min / step).ceil() * step;
        while y_val <= y_max {
            let y = self.value_to_y(y_val, chart_rect, y_min, y_max);
            if y >= full_rect.min.y + 10.0 && y <= full_rect.max.y - 10.0 {
                let label = format!("{:.2}", y_val);
                painter.text(
                    Pos2::new(label_x, y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(typography::XS),
                    text_color,
                );
            }
            y_val += step;
        }
    }

    fn draw_legend(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        text_color: Color32,
    ) {
        if !self.config.show_legend {
            return;
        }

        // Get current values for legend
        let values = indicator.values();
        let colors = indicator.colors();

        let mut legend_parts = vec![indicator.name().to_string()];

        // Add current values if available
        if let Some(last_value) = values.last() {
            match last_value {
                IndicatorValue::Single(v) => {
                    legend_parts.push(format!("{:.2}", v));
                }
                IndicatorValue::Multiple(vals) => {
                    for (i, v) in vals.iter().enumerate() {
                        let color = colors.get(i).copied().unwrap_or(text_color);
                        legend_parts.push(format!("{:.2}", v));
                        // Could draw colored value, but keeping it simple for now
                        let _ = color;
                    }
                }
                IndicatorValue::None => {}
            }
        }

        let legend_text = legend_parts.join(" ");
        painter.text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.sm,
                rect.min.y + DESIGN_TOKENS.spacing.xl,
            ),
            egui::Align2::LEFT_CENTER,
            legend_text,
            egui::FontId::proportional(typography::SM),
            text_color,
        );
    }

    fn value_to_y(&self, value: f64, rect: Rect, y_min: f64, y_max: f64) -> f32 {
        let norm = (value - y_min) / (y_max - y_min);
        rect.max.y - (norm as f32 * rect.height())
    }

    fn calculate_nice_step(&self, range: f64, target_lines: usize) -> f64 {
        let raw_step = range / target_lines as f64;
        let magnitude = 10f64.powf(raw_step.log10().floor());
        let normalized = raw_step / magnitude;

        let nice = if normalized <= 1.0 {
            1.0
        } else if normalized <= 2.0 {
            2.0
        } else if normalized <= 5.0 {
            5.0
        } else {
            10.0
        };

        nice * magnitude
    }

    fn draw_dashed_line(
        &self,
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        color: Color32,
        width: f32,
        dash_length: f32,
        gap_length: f32,
    ) {
        let dir = to - from;
        let len = dir.length();
        if len == 0.0 {
            return;
        }
        let dir = dir / len;

        let mut pos = 0.0;
        let stroke = Stroke::new(width, color);

        while pos < len {
            let start = from + dir * pos;
            let end_pos = (pos + dash_length).min(len);
            let end = from + dir * end_pos;
            painter.line_segment([start, end], stroke);
            pos += dash_length + gap_length;
        }
    }
}
