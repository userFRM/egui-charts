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

/// Linear price-to-pixel mapping used by the bar/candle render path.
///
/// This is a deliberately thin, render-local mapper: it maps an absolute price
/// onto the price axis assuming a linear scale, which is all the per-bar drawing
/// primitives need. It is distinct from the price-scale engine
/// [`crate::scales::PriceScale`], which additionally models logarithmic,
/// percentage, and indexed-to-100 modes plus margins and inversion. The two
/// types previously shared the name `PriceScale`, which masked the fact that
/// the render path only ever applies the linear mapping.
///
// TODO: route the render path through `crate::scales::PriceScale` so that the
// logarithmic, percentage, and indexed-to-100 modes are honored when drawing
// bars (today only the axis labels respect those modes). Tracked by
// userFRM/egui-charts#8.
#[derive(Debug, Copy, Clone)]
pub struct LinearPriceMap {
    /// Lower bound of the visible price range.
    pub min_price: f64,
    /// Upper bound of the visible price range.
    pub max_price: f64,
}

impl LinearPriceMap {
    pub fn new(min_price: f64, max_price: f64) -> Self {
        Self {
            min_price,
            max_price,
        }
    }

    /// Size of the visible price range (`max - min`).
    ///
    /// Derived from the bounds so it can never disagree with `min_price` /
    /// `max_price`.
    #[inline]
    pub fn price_range(&self) -> f64 {
        self.max_price - self.min_price
    }

    /// Convert price to Y coord.
    ///
    /// A flat price window (`max == min`) has no vertical extent to map onto, so
    /// the price is placed at the vertical center of `rect` rather than dividing
    /// by zero and producing an off-screen infinite/NaN coordinate that would
    /// make every candle vanish.
    pub fn price_to_y(&self, price: f64, rect: Rect) -> f32 {
        let range = self.price_range();
        if range.abs() < f64::EPSILON {
            return rect.center().y;
        }
        let ratio = ((price - self.min_price) / range) as f32;
        rect.max.y - ratio * rect.height()
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Vec2};

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(100.0, 200.0))
    }

    #[test]
    fn price_range_is_derived_from_bounds() {
        let map = LinearPriceMap::new(10.0, 30.0);
        assert_eq!(map.price_range(), 20.0);
    }

    #[test]
    fn price_to_y_is_finite_in_normal_window() {
        let map = LinearPriceMap::new(100.0, 200.0);
        let r = rect();
        let y = map.price_to_y(150.0, r);
        assert!(y.is_finite());
        assert!((r.min.y..=r.max.y).contains(&y));
    }

    #[test]
    fn price_to_y_on_flat_window_is_finite_and_in_rect() {
        // A flat price window (max == min) previously divided by zero and pushed
        // every candle off-screen. The mapper must instead return a finite y
        // inside the rect so candles remain visible.
        let map = LinearPriceMap::new(100.0, 100.0);
        let r = rect();
        let y = map.price_to_y(100.0, r);
        assert!(y.is_finite(), "y must be finite on a flat window");
        assert!(
            (r.min.y..=r.max.y).contains(&y),
            "y={y} must lie within the rect"
        );
        assert_eq!(y, r.center().y);
    }
}
