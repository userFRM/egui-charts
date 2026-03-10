//! Series settings for customizing chart series appearance.
//!
//! Provides configurable colors and options for candlestick/bar chart series.

use crate::model::PriceSource;
use crate::scales::PriceScaleMode;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Settings for chart series appearance
#[derive(Clone, Debug)]
pub struct SeriesSettings {
    // === Symbol Tab: Colors ===
    /// Bullish candle body color
    pub bullish_color: Color32,
    /// Bearish candle body color
    pub bearish_color: Color32,
    /// Bullish candle border color (None = same as fill)
    pub bullish_border_color: Option<Color32>,
    /// Bearish candle border color (None = same as fill)
    pub bearish_border_color: Option<Color32>,
    /// Bullish wick color (None = same as fill)
    pub bullish_wick_color: Option<Color32>,
    /// Bearish wick color (None = same as fill)
    pub bearish_wick_color: Option<Color32>,
    /// Price source for line calculations
    pub price_source: PriceSource,

    // === Symbol Tab: Visibility ===
    /// Whether to show on all timeframes
    pub visible_all_timeframes: bool,
    /// Specific visible timeframes (when not all)
    pub visible_timeframes: Vec<String>,

    // === Status Line Tab ===
    /// Show OHLC values in status line
    pub show_ohlc_values: bool,
    /// Show price change in status line
    pub show_change: bool,
    /// Show volume in status line
    pub show_volume: bool,
    /// Show bar change (open to close) in status line
    pub show_bar_change: bool,
    /// Decimal places for price display
    pub decimal_places: u8,

    // === Scales Tab ===
    /// Price scale mode (Normal, Log, Percentage, IndexedTo100)
    pub price_scale_mode: PriceScaleMode,
    /// Show last price horizontal line
    pub show_price_line: bool,
    /// Show previous close line
    pub show_prev_close_line: bool,
    /// Invert the price scale
    pub invert_scale: bool,

    // === Canvas Tab ===
    /// Background type: 0 = Solid, 1 = Gradient
    pub background_type: u8,
    /// Show vertical grid lines
    pub show_vertical_grid: bool,
    /// Show horizontal grid lines
    pub show_horizontal_grid: bool,
    /// Crosshair mode: 0 = Full, 1 = Cross, 2 = None
    pub crosshair_mode: u8,
}

impl Default for SeriesSettings {
    fn default() -> Self {
        Self {
            // Symbol Tab: Colors
            bullish_color: DESIGN_TOKENS.semantic.extended.bullish,
            bearish_color: DESIGN_TOKENS.semantic.extended.bearish,
            bullish_border_color: None,
            bearish_border_color: None,
            bullish_wick_color: None,
            bearish_wick_color: None,
            price_source: PriceSource::default(),
            // Symbol Tab: Visibility
            visible_all_timeframes: true,
            visible_timeframes: Vec::new(),
            // Status Line Tab
            show_ohlc_values: true,
            show_change: true,
            show_volume: true,
            show_bar_change: false,
            decimal_places: 2,
            // Scales Tab
            price_scale_mode: PriceScaleMode::Normal,
            show_price_line: true,
            show_prev_close_line: false,
            invert_scale: false,
            // Canvas Tab
            background_type: 0, // Solid
            show_vertical_grid: true,
            show_horizontal_grid: true,
            crosshair_mode: 0, // Full
        }
    }
}

impl SeriesSettings {
    /// Create new settings with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the effective bullish border color
    pub fn effective_bullish_border(&self) -> Color32 {
        self.bullish_border_color.unwrap_or(self.bullish_color)
    }

    /// Get the effective bearish border color
    pub fn effective_bearish_border(&self) -> Color32 {
        self.bearish_border_color.unwrap_or(self.bearish_color)
    }

    /// Get the effective bullish wick color
    pub fn effective_bullish_wick(&self) -> Color32 {
        self.bullish_wick_color.unwrap_or(self.bullish_color)
    }

    /// Get the effective bearish wick color
    pub fn effective_bearish_wick(&self) -> Color32 {
        self.bearish_wick_color.unwrap_or(self.bearish_color)
    }

    /// Reset to default colors
    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }

    /// Compute the line value for a bar based on the configured price source.
    ///
    /// Use this when rendering line charts to respect the user's price source selection.
    /// For example: `settings.compute_line_value(bar.open, bar.high, bar.low, bar.close)`
    pub fn compute_line_value(&self, open: f64, high: f64, low: f64, close: f64) -> f64 {
        self.price_source.compute(open, high, low, close)
    }

    /// Compute line values for a slice of bars based on price source.
    ///
    /// Returns a Vec of computed values matching the input bars.
    pub fn compute_line_values(&self, bars: &[crate::model::Bar]) -> Vec<f64> {
        bars.iter()
            .map(|bar| self.compute_line_value(bar.open, bar.high, bar.low, bar.close))
            .collect()
    }
}
