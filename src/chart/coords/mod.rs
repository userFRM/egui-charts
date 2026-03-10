//! Unified chart coordinate system.
//!
//! This module provides the single source of truth for all coordinate
//! conversions between bar indices, prices, and screen positions.
//!
//! # Key Types
//!
//! - [`ChartMapping`]: The unified coordinate mapper for idx ↔ x and price ↔ y
//!
//! # Coordinate Formula
//!
//! ```text
//! delta_from_right = base_idx + right_offset - bar_idx
//! x = rect.min.x + rect.width() - (delta_from_right + 0.5) * bar_spacing - 1.0
//! ```

use egui::{Pos2, Rect};

/// Unified coordinate mapping for charts and indicator panes.
///
/// This is the single source of truth for converting between:
/// - Bar indices ↔ X screen coordinates
/// - Prices ↔ Y screen coordinates
///
/// All chart components (main chart, indicator panes, hit testing, drawings)
/// should use this struct for coordinate conversions.
#[derive(Clone, Copy, Debug)]
pub struct ChartMapping {
    /// The chart rectangle (rendering area)
    pub rect: Rect,
    /// Spacing between bars in pixels
    pub bar_spacing: f32,
    /// Index of the first visible bar
    pub start_idx: usize,
    /// Index of the last visible bar (anchor point for x conversion)
    pub base_idx: usize,
    /// Right-side offset for chart scrolling (in bar units)
    pub right_offset: f32,
    /// Minimum price in the visible range
    pub price_min: f64,
    /// Maximum price in the visible range
    pub price_max: f64,
}

impl ChartMapping {
    /// Create a new ChartMapping with all parameters.
    pub fn new(
        rect: Rect,
        bar_spacing: f32,
        start_idx: usize,
        base_idx: usize,
        right_offset: f32,
        price_min: f64,
        price_max: f64,
    ) -> Self {
        Self {
            rect,
            bar_spacing,
            start_idx,
            base_idx,
            right_offset,
            price_min,
            price_max,
        }
    }

    /// Create a ChartMapping for X-axis only (no price conversion).
    ///
    /// Useful for indicator panes that have their own Y-axis range.
    pub fn x_only(
        rect: Rect,
        bar_spacing: f32,
        start_idx: usize,
        base_idx: usize,
        right_offset: f32,
    ) -> Self {
        Self {
            rect,
            bar_spacing,
            start_idx,
            base_idx,
            right_offset,
            price_min: 0.0,
            price_max: 1.0,
        }
    }

    // =========================================================================
    // X-axis conversions (bar index ↔ screen X)
    // =========================================================================

    /// Convert bar index to X screen coordinate.
    ///
    /// This is the canonical formula used everywhere.
    #[inline]
    pub fn idx_to_x(&self, bar_idx: usize) -> f32 {
        let delta_from_right = self.base_idx as f32 + self.right_offset - bar_idx as f32;
        let relative_x = self.rect.width() - (delta_from_right + 0.5) * self.bar_spacing - 1.0;
        self.rect.min.x + relative_x
    }

    /// Convert X screen coordinate to bar index (inverse of idx_to_x).
    #[inline]
    pub fn x_to_idx(&self, x: f32) -> usize {
        let relative_x = x - self.rect.min.x;
        let delta_from_right = (self.rect.width() - relative_x - 1.0) / self.bar_spacing - 0.5;
        let bar_idx = self.base_idx as f32 + self.right_offset - delta_from_right;
        bar_idx.round() as usize
    }

    /// Convert X screen coordinate to fractional bar index.
    ///
    /// Useful for precise positioning (e.g., crosshair snapping).
    #[inline]
    pub fn x_to_idx_f32(&self, x: f32) -> f32 {
        let relative_x = x - self.rect.min.x;
        let delta_from_right = (self.rect.width() - relative_x - 1.0) / self.bar_spacing - 0.5;
        self.base_idx as f32 + self.right_offset - delta_from_right
    }

    /// Check if an X coordinate is within the visible chart bounds.
    #[inline]
    pub fn is_x_visible(&self, x: f32) -> bool {
        x >= self.rect.min.x && x <= self.rect.max.x
    }

    /// Calculate the bar width based on spacing.
    #[inline]
    pub fn bar_width(&self) -> f32 {
        (self.bar_spacing * 0.6).max(1.0)
    }

    // =========================================================================
    // Y-axis conversions (price ↔ screen Y)
    // =========================================================================

    /// Convert price to Y screen coordinate.
    #[inline]
    pub fn price_to_y(&self, price: f64) -> f32 {
        let price_range = self.price_max - self.price_min;
        if price_range.abs() < f64::EPSILON {
            return self.rect.center().y;
        }
        let ratio = (price - self.price_min) / price_range;
        self.rect.max.y - (ratio as f32 * self.rect.height())
    }

    /// Convert Y screen coordinate to price (inverse of price_to_y).
    #[inline]
    pub fn y_to_price(&self, y: f32) -> f64 {
        let price_range = self.price_max - self.price_min;
        let ratio = (self.rect.max.y - y) / self.rect.height();
        self.price_min + (ratio as f64 * price_range)
    }

    /// Check if a Y coordinate is within the visible chart bounds.
    #[inline]
    pub fn is_y_visible(&self, y: f32) -> bool {
        y >= self.rect.min.y && y <= self.rect.max.y
    }

    // =========================================================================
    // Combined conversions
    // =========================================================================

    /// Convert bar index and price to screen position.
    #[inline]
    pub fn to_screen(&self, bar_idx: usize, price: f64) -> Pos2 {
        Pos2::new(self.idx_to_x(bar_idx), self.price_to_y(price))
    }

    /// Convert screen position to bar index and price.
    #[inline]
    pub fn from_screen(&self, pos: Pos2) -> (usize, f64) {
        (self.x_to_idx(pos.x), self.y_to_price(pos.y))
    }

    /// Check if a screen position is within the visible chart bounds.
    #[inline]
    pub fn is_visible(&self, pos: Pos2) -> bool {
        self.rect.contains(pos)
    }

    // =========================================================================
    // Utility methods
    // =========================================================================

    /// Create a new ChartMapping with a different price range (for indicator panes).
    pub fn with_price_range(self, price_min: f64, price_max: f64) -> Self {
        Self {
            price_min,
            price_max,
            ..self
        }
    }

    /// Create a new ChartMapping with a different rect (for sub-regions).
    pub fn with_rect(self, rect: Rect) -> Self {
        Self { rect, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idx_to_x_roundtrip() {
        let mapping = ChartMapping::new(
            Rect::from_min_size(Pos2::new(100.0, 50.0), egui::Vec2::new(800.0, 400.0)),
            10.0, // bar_spacing
            50,   // start_idx
            100,  // base_idx
            5.0,  // right_offset
            100.0,
            200.0,
        );

        // Test roundtrip for several indices
        for idx in [50, 75, 100, 105] {
            let x = mapping.idx_to_x(idx);
            let recovered = mapping.x_to_idx(x);
            assert_eq!(
                recovered, idx,
                "idx {} -> x {} -> idx {}",
                idx, x, recovered
            );
        }
    }

    #[test]
    fn test_price_to_y_roundtrip() {
        let mapping = ChartMapping::new(
            Rect::from_min_size(Pos2::new(100.0, 50.0), egui::Vec2::new(800.0, 400.0)),
            10.0,
            50,  // start_idx
            100, // base_idx
            5.0,
            100.0,
            200.0,
        );

        // Test roundtrip for several prices
        for price in [100.0, 125.0, 150.0, 175.0, 200.0] {
            let y = mapping.price_to_y(price);
            let recovered = mapping.y_to_price(y);
            assert!(
                (recovered - price).abs() < 0.001,
                "price {} -> y {} -> price {}",
                price,
                y,
                recovered
            );
        }
    }
}
