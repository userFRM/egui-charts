//! Separate-pane indicator rendering with direct egui painting.
//!
//! This module renders oscillator and multi-line indicators (RSI, MACD, Stochastic,
//! etc.) in dedicated panels below the main chart. It paints directly through
//! [`egui::Painter`] to ensure pixel-perfect x-axis alignment with the main
//! candlestick chart.
//!
//! # Usage
//!
//! Indicator panes are typically drawn by [`super::Chart::show`], but
//! they can also be used standalone:
//!
//! ```rust
//! use egui_charts::widget::indicator_pane::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig};
//!
//! # fn example(
//! #     ui: &mut egui::Ui,
//! #     rsi_indicator: &egui_charts::studies::RSI,
//! #     bars: &[egui_charts::model::Bar],
//! #     visible_range: std::ops::Range<usize>,
//! #     coords: IndicatorCoordParams,
//! # ) {
//! let config = IndicatorPaneConfig::rsi();
//! let mut pane = IndicatorPane::with_config(config);
//! pane.show_aligned(ui, rsi_indicator, bars, visible_range, coords);
//! # }
//! ```
//!
//! # Coordinate Alignment
//!
//! The [`IndicatorCoordParams`] struct captures the main chart's bar spacing,
//! right offset, and base index so that indicator panes draw their data points
//! at exactly the same x-coordinates as the corresponding candlesticks above.

use crate::chart::rendering::draw_dashed_line;
use crate::model::Bar;
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::{Indicator, IndicatorValue};
use crate::styles::typography;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui};
use std::ops::Range;

/// Chart layout constants matching main chart (from ChartConfig defaults)
mod layout {
    /// Left padding (matches ChartConfig::padding default)
    pub const LEFT_PADDING: f32 = crate::styles::sizing::chart::PADDING;
    /// Right price axis width for standalone panes
    pub const RIGHT_PADDING: f32 = crate::styles::sizing::chart::RIGHT_AXIS_WIDTH;
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
    /// Decimals for the pane ticks and readouts. `None` uses two decimals.
    pub price_precision: Option<u8>,
    /// Fixed y-axis range (None = auto-scale)
    pub y_range: Option<(f64, f64)>,
    /// Show horizontal zone lines (e.g., RSI 70/30)
    pub show_zones: bool,
    /// Zone line levels and colors
    pub zones: Vec<(f64, Color32, &'static str)>,
    /// Show legend
    pub show_legend: bool,
    /// Show the study name in its legend.
    pub show_name: bool,
    /// Host-supplied input caption; empty omits it.
    pub inputs: String,
    /// Show the study output values in its legend.
    pub show_values: bool,
    /// Show grid
    pub show_grid: bool,
}

impl Default for IndicatorPaneConfig {
    fn default() -> Self {
        Self {
            height: DESIGN_TOKENS.sizing.panel.bottom_default_height,
            price_precision: None,
            y_range: None,
            show_zones: false,
            zones: Vec::new(),
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }
}

impl IndicatorPaneConfig {
    /// Create config for RSI indicator
    pub fn rsi() -> Self {
        Self {
            height: indicator_sizing::OSCILLATOR_HEIGHT,
            price_precision: None,
            y_range: Some((0.0, 100.0)),
            show_zones: true,
            zones: vec![
                (70.0, DESIGN_TOKENS.semantic.extended.bearish, "Overbought"),
                (50.0, DESIGN_TOKENS.semantic.extended.chart_text, "Neutral"),
                (30.0, DESIGN_TOKENS.semantic.extended.bullish, "Oversold"),
            ],
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }

    /// Create config for MACD indicator
    pub fn macd() -> Self {
        Self {
            height: indicator_sizing::MULTI_LINE_HEIGHT,
            price_precision: None,
            y_range: None, // Auto-scale
            show_zones: true,
            zones: vec![(0.0, DESIGN_TOKENS.semantic.extended.chart_text, "Zero Line")],
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }

    /// Create config for Stochastic indicator
    pub fn stochastic() -> Self {
        Self {
            height: indicator_sizing::OSCILLATOR_HEIGHT,
            price_precision: None,
            y_range: Some((0.0, 100.0)),
            show_zones: true,
            zones: vec![
                (80.0, DESIGN_TOKENS.semantic.extended.bearish, "Overbought"),
                (20.0, DESIGN_TOKENS.semantic.extended.bullish, "Oversold"),
            ],
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
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
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(y_min, y_max),
                rect.min.y,
                rect.max.y,
                false,
            ),
        )
    }
}

/// Layout and interaction results from rendering one interactive indicator pane.
///
/// Returned by [`IndicatorPane::show_aligned_interactive`]. Besides the rects
/// and y-axis range used for click hit-testing, it carries the [`Response`] for
/// the legend's close "x" so the host can offer one-click pane removal.
pub struct PaneInteraction {
    /// Full panel rect, including the y-axis label gutter.
    pub panel_rect: Rect,
    /// Chart drawing area rect, excluding the y-axis label gutter.
    pub chart_rect: Rect,
    /// Y-axis minimum value used for this frame.
    pub y_min: f64,
    /// Y-axis maximum value used for this frame.
    pub y_max: f64,
    /// Interaction response for the whole pane (line selection, hover).
    pub response: Response,
    /// Interaction response for the legend's close "x", or `None` when the pane
    /// shows no legend and therefore exposes no remove affordance.
    pub close_response: Option<Response>,
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
}

impl IndicatorPane {
    /// Create a new indicator panel with custom configuration
    pub fn with_config(config: IndicatorPaneConfig) -> Self {
        Self { config }
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
        let colors = ui.active_theme().chart;
        let chart_bg = colors.bg;
        let text_color = ui.style().visuals.text_color();
        let grid_color = colors.grid_line;

        // Get live theme colors for zones
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;

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
        self.draw_y_axis_labels(painter, rect, chart_rect, y_min, y_max, indicator);

        // Draw indicator legend at top-left. The non-interactive path cannot
        // host a clickable close affordance, so no remove "x" is drawn here.
        self.draw_legend(painter, chart_rect, indicator, text_color, None, false);
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
        let colors = ui.active_theme().chart;
        let chart_bg = colors.bg;
        let text_color = ui.style().visuals.text_color();
        let grid_color = colors.grid_line;

        // Get live theme colors for zones
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;

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
        self.draw_y_axis_labels(painter, rect, chart_rect, y_min, y_max, indicator);

        // Draw indicator legend at top-left. The non-interactive path cannot
        // host a clickable close affordance, so no remove "x" is drawn here.
        self.draw_legend(painter, chart_rect, indicator, text_color, None, false);
    }

    /// Render with coordinate alignment and return layout info for hit testing.
    ///
    /// Like [`IndicatorPane::show_aligned`], but returns the panel and chart
    /// rects, y-axis range, and an interactive [`egui::Response`]. This allows
    /// the caller to implement click-on-line selection, tooltip display, or
    /// other interactive features on indicator panes.
    ///
    /// Returns `None` if the indicator is not visible.
    ///
    /// The returned [`PaneInteraction`] also carries the [`Response`] for the
    /// legend's close "x" (when the pane shows a legend), so the host can offer
    /// one-click pane removal.
    pub fn show_aligned_interactive(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        _bars: &[Bar],
        visible_range: Range<usize>,
        coords: IndicatorCoordParams,
    ) -> Option<PaneInteraction> {
        self.show_aligned_interactive_in(ui, indicator, visible_range, coords, None, false)
    }

    pub(super) fn show_aligned_interactive_in(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        visible_range: Range<usize>,
        coords: IndicatorCoordParams,
        plot_x: Option<egui::emath::Rangef>,
        collapsed: bool,
    ) -> Option<PaneInteraction> {
        if !indicator.is_visible() {
            return None;
        }

        // Get chart background and text colors from theme
        let colors = ui.active_theme().chart;
        let chart_bg = colors.bg;
        let text_color = ui.style().visuals.text_color();
        let grid_color = colors.grid_line;

        // Get live theme colors for zones
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;

        // Allocate the panel area with click sense
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, response) = ui.allocate_exact_size(panel_size, Sense::click());

        if !ui.is_rect_visible(rect) {
            return None;
        }

        let painter = &ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));

        // Fill background
        painter.rect_filled(rect, 0.0, chart_bg);

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let plot_x = plot_x.unwrap_or_else(|| {
            egui::emath::Rangef::new(
                rect.left() + left_padding,
                (rect.right() - right_padding).max(rect.left() + left_padding),
            )
        });
        let chart_rect = Rect::from_min_max(
            Pos2::new(plot_x.min, rect.top()),
            Pos2::new(plot_x.max, rect.bottom()),
        );

        // Determine y-axis range
        let (y_min, y_max) = self.calculate_y_range(indicator, &visible_range);

        if !collapsed {
            let plot_painter = &painter.with_clip_rect(chart_rect.intersect(painter.clip_rect()));
            // Draw zone fills (colored bands)
            if self.config.show_zones && self.config.zones.len() >= 2 {
                self.draw_zone_fills(plot_painter, chart_rect, y_min, y_max, bullish, bearish);
            }

            // Draw horizontal grid lines and zone lines
            self.draw_horizontal_lines(
                plot_painter,
                chart_rect,
                y_min,
                y_max,
                grid_color,
                bullish,
                bearish,
            );

            // Draw vertical grid lines (aligned with main chart)
            if self.config.show_grid {
                self.draw_vertical_grid_aligned(plot_painter, chart_rect, &coords, grid_color);
            }

            // Draw indicator lines with aligned coordinates
            self.draw_indicator_lines_aligned(
                plot_painter,
                chart_rect,
                indicator,
                &visible_range,
                y_min,
                y_max,
                &coords,
            );

            // Draw y-axis labels
            self.draw_y_axis_labels(painter, rect, chart_rect, y_min, y_max, indicator);
        }

        // Reserve the legend's close "x" and make it independently clickable so
        // it can surface a remove request without stealing pane-line clicks.
        let close_rect = self
            .legend_close_rect(painter, chart_rect, indicator)
            .map(|close| close.intersect(chart_rect));
        let close_response = close_rect
            .map(|cr| ui.interact(cr, response.id.with("indicator_pane_close"), Sense::click()));
        let close_hovered = close_response.as_ref().is_some_and(|r| r.hovered());
        if close_hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // Draw indicator legend (and the close glyph) at top-left.
        self.draw_legend(
            &painter.with_clip_rect(chart_rect.intersect(ui.clip_rect())),
            chart_rect,
            indicator,
            text_color,
            close_rect,
            close_hovered,
        );

        // Return panel info for hit testing
        Some(PaneInteraction {
            panel_rect: rect,
            chart_rect,
            y_min,
            y_max,
            response,
            close_response,
        })
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
            let padding = crate::scales::auto_scale_span(min_val, max_val) * 0.1;
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
                    draw_dashed_line(
                        painter,
                        Pos2::new(rect.min.x, y),
                        Pos2::new(rect.max.x, y),
                        Stroke::new(1.0_f32, live_color),
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
                }
                .filter(|v| v.is_finite());

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
                }
                .filter(|v| v.is_finite());

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
        indicator: &dyn Indicator,
    ) {
        let text_color = painter.ctx().active_theme().chart.axis_text;
        let mut tags = Vec::new();
        if self.config.show_values {
            let map = crate::scales::PriceMap::new(
                crate::scales::PriceTransform::default(),
                crate::scales::PriceRange::new(y_min, y_max),
                chart_rect.top(),
                chart_rect.bottom(),
                false,
            )
            .with_price_precision(self.config.price_precision);
            crate::chart::rendering::axes::render_study_tags(
                painter,
                chart_rect,
                &map,
                indicator,
                full_rect.right(),
                &mut tags,
            );
        }
        let range = y_max - y_min;
        let step = self.calculate_nice_step(range, 5);
        let label_x = chart_rect.max.x + DESIGN_TOKENS.spacing.sm;

        let mut y_val = (y_min / step).ceil() * step;
        while y_val <= y_max {
            let y = self.value_to_y(y_val, chart_rect, y_min, y_max);
            if y >= full_rect.min.y + 10.0 && y <= full_rect.max.y - 10.0 {
                let label = crate::scales::PriceFormatter::format(
                    &crate::scales::DefaultPriceFormatter::with_precision(
                        self.config.price_precision.map_or(2, usize::from),
                    ),
                    y_val,
                );
                let galley = painter.layout_no_wrap(
                    label,
                    crate::chart::rendering::axes::axis_font(painter.ctx()),
                    text_color,
                );
                let bounds =
                    egui::Align2::LEFT_CENTER.anchor_size(Pos2::new(label_x, y), galley.size());
                if tags.iter().flatten().any(|tag| tag.intersects(bounds)) {
                    y_val += step;
                    continue;
                }
                painter.text(
                    Pos2::new(label_x, y),
                    egui::Align2::LEFT_CENTER,
                    galley.text(),
                    crate::chart::rendering::axes::axis_font(painter.ctx()),
                    text_color,
                );
            }
            y_val += step;
        }
    }

    /// Builds the legend text (indicator name plus its current value) shown at
    /// the pane's top-left.
    fn legend_text(&self, indicator: &dyn Indicator) -> String {
        let values = indicator.values();
        let mut legend_parts = Vec::new();
        let precision = self.config.price_precision.map_or(2, usize::from);
        if self.config.show_name {
            legend_parts.push(indicator.name().to_string());
        }
        if !self.config.inputs.is_empty() {
            legend_parts.push(self.config.inputs.clone());
        }
        if self.config.show_values
            && let Some(last_value) = values.last()
        {
            match last_value {
                IndicatorValue::Single(v) => {
                    legend_parts.push(format!("{v:.precision$}"));
                }
                IndicatorValue::Multiple(vals) => {
                    for v in vals {
                        legend_parts.push(format!("{v:.precision$}"));
                    }
                }
                IndicatorValue::None => {}
            }
        }

        legend_parts.join(" ")
    }

    /// Computes the screen rect of the close "x" trailing the legend text for a
    /// pane drawn into `rect`, or `None` when the legend (and thus the remove
    /// affordance) is hidden. Layout is measured through `painter` so the hit
    /// region matches the painted glyph exactly.
    fn legend_close_rect(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
    ) -> Option<Rect> {
        if !self.config.show_legend {
            return None;
        }

        let galley = painter.layout_no_wrap(
            self.legend_text(indicator),
            egui::FontId::proportional(typography::MD),
            Color32::WHITE,
        );
        let text_left = rect.min.x + DESIGN_TOKENS.spacing.sm;
        let center_y = rect.min.y + 12.0_f32.min(rect.height() / 2.0);
        let glyph_size = typography::MD;
        let center = Pos2::new(
            text_left + galley.size().x + DESIGN_TOKENS.spacing.md,
            center_y,
        );
        let pad = DESIGN_TOKENS.spacing.sm;
        Some(Rect::from_center_size(
            center,
            egui::vec2(glyph_size + pad, glyph_size + pad),
        ))
    }

    /// Draws the indicator legend (name + current value) at the pane's
    /// top-left, plus a subtle close "x" inside
    /// `close_rect` when one was reserved by [`Self::legend_close_rect`]. The
    /// glyph dims at rest and brightens to the text color when `close_hovered`,
    /// so it reads as clickable without competing with the indicator readout.
    fn draw_legend(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        text_color: Color32,
        close_rect: Option<Rect>,
        close_hovered: bool,
    ) {
        if !self.config.show_legend {
            return;
        }

        painter.text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.sm,
                rect.min.y + 12.0_f32.min(rect.height() / 2.0),
            ),
            egui::Align2::LEFT_CENTER,
            self.legend_text(indicator),
            egui::FontId::proportional(typography::MD),
            text_color,
        );

        if let Some(close_rect) = close_rect {
            let glyph_color = if close_hovered {
                text_color
            } else {
                Color32::from_rgba_unmultiplied(text_color.r(), text_color.g(), text_color.b(), 140)
            };
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                "\u{00d7}",
                egui::FontId::proportional(typography::MD),
                glyph_color,
            );
        }
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
}
