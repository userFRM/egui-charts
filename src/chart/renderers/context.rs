use egui::{Color32, Painter, Rect};

// Re-export ChartMapping as the primary coordinate type
pub use crate::chart::coords::ChartMapping;

/// Core rendering context containing painter and drawing area
pub struct RenderContext<'a> {
    pub painter: &'a Painter,
    pub rect: Rect,
}

impl<'a> RenderContext<'a> {
    pub fn new(painter: &'a Painter, rect: Rect) -> Self {
        Self { painter, rect }
    }
}

/// Price scaling information for converting prices to screen coords
#[derive(Debug, Copy, Clone)]
pub struct PriceScale {
    pub min_price: f64,
    pub max_price: f64,
    pub price_range: f64,
}

impl PriceScale {
    pub fn new(min_price: f64, max_price: f64) -> Self {
        Self {
            min_price,
            max_price,
            price_range: max_price - min_price,
        }
    }

    /// Convert price to Y coord
    pub fn price_to_y(&self, price: f64, rect: Rect) -> f32 {
        let ratio = ((price - self.min_price) / self.price_range) as f32;
        rect.max.y - ratio * rect.height()
    }
}

/// Color scheme for rendering chart elements
#[derive(Debug, Copy, Clone)]
pub struct StyleColors {
    pub bullish: Color32,
    pub bearish: Color32,
    pub grid: Color32,
    pub text: Color32,
    /// Border color for bullish candles (None = same as body)
    pub bullish_border: Option<Color32>,
    /// Border color for bearish candles (None = same as body)
    pub bearish_border: Option<Color32>,
    /// Wick color for bullish candles (None = same as border/body)
    pub bullish_wick: Option<Color32>,
    /// Wick color for bearish candles (None = same as border/body)
    pub bearish_wick: Option<Color32>,
    /// Border width for candles (0 = no border)
    pub candle_border_width: f32,
}

impl StyleColors {
    pub fn new(bullish: Color32, bearish: Color32, grid: Color32, text: Color32) -> Self {
        Self {
            bullish,
            bearish,
            grid,
            text,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: 0.0,
        }
    }

    /// Get color based on bar direction
    pub fn bar_color(&self, is_bullish: bool) -> Color32 {
        if is_bullish {
            self.bullish
        } else {
            self.bearish
        }
    }

    /// Get wick color based on bar direction
    /// Falls back to border color, then body color
    pub fn wick_color(&self, is_bullish: bool) -> Color32 {
        if is_bullish {
            self.bullish_wick
                .or(self.bullish_border)
                .unwrap_or(self.bullish)
        } else {
            self.bearish_wick
                .or(self.bearish_border)
                .unwrap_or(self.bearish)
        }
    }

    /// Get border color based on bar direction
    /// Falls back to body color if not set
    pub fn border_color(&self, is_bullish: bool) -> Color32 {
        if is_bullish {
            self.bullish_border.unwrap_or(self.bullish)
        } else {
            self.bearish_border.unwrap_or(self.bearish)
        }
    }

    /// Check if candle borders should be drawn
    pub fn has_border(&self) -> bool {
        self.candle_border_width > 0.0
    }
}

/// Params for rendering a single bar
#[derive(Debug, Copy, Clone)]
pub struct BarRenderParams {
    pub x: f32,
    pub width: f32,
    pub wick_width: f32,
}

impl BarRenderParams {
    pub fn new(x: f32, width: f32, wick_width: f32) -> Self {
        Self {
            x,
            width,
            wick_width,
        }
    }
}
