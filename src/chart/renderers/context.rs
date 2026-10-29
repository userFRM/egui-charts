use egui::{Color32, Painter, Rect};

// Re-export ChartMapping as the primary coordinate type
pub use crate::chart::coords::ChartMapping;

/// Core rendering context containing painter and drawing area
pub struct RenderContext<'a> {
    /// Painter used to draw the overlay.
    pub painter: &'a Painter,
    /// Screen rectangle available for rendering.
    pub rect: Rect,
}

impl std::fmt::Debug for RenderContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderContext")
            .field("rect", &self.rect)
            .finish_non_exhaustive()
    }
}

impl<'a> RenderContext<'a> {
    /// Bind a painter to the rectangle available for rendering.
    pub fn new(painter: &'a Painter, rect: Rect) -> Self {
        Self { painter, rect }
    }
}

/// Color scheme for rendering chart elements
#[derive(Debug, Copy, Clone)]
pub struct StyleColors {
    pub bullish: Color32,
    pub bearish: Color32,
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
