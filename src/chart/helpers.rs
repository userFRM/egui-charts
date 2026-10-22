//! Low-level coordinate conversion and zoom helpers.
//!
//! These utility functions are used by [`pan_zoom`](super::pan_zoom) and the
//! rendering pipeline to convert between screen-space Y coordinates and price
//! values, and to apply price-axis zoom transformations anchored at a given
//! price level.

use crate::scales::auto_scale_span;
use egui::Rect;

/// Convert a Y pixel coordinate to its corresponding price value.
///
/// The conversion is linear within the `price_rect`, where the bottom edge
/// maps to `min` and the top edge maps to `max`. Values outside the rect
/// are clamped to the `[min, max]` range.
///
/// # Arguments
///
/// * `y` - Screen-space Y coordinate
/// * `min` - Minimum price at the bottom of `price_rect`
/// * `max` - Maximum price at the top of `price_rect`
/// * `price_rect` - The chart area rectangle used for the conversion
pub fn y_to_price(y: f32, min: f64, max: f64, price_rect: Rect) -> f64 {
    let ratio = ((price_rect.max.y - y) / price_rect.height()).clamp(0.0, 1.0) as f64;
    min + ratio * (max - min)
}

/// Apply a zoom transformation to a price range, anchored at a specific price.
///
/// The zoom uses an exponential response so that equal mouse-wheel deltas produce
/// proportional zoom regardless of the current range. The `anchor` price stays
/// fixed on screen while the range expands or contracts around it.
///
/// # Arguments
///
/// * `bounds` - Current `(min, max)` price range
/// * `anchor` - Price to keep fixed on screen (typically under the cursor)
/// * `delta_y` - Mouse wheel delta (positive = zoom out, negative = zoom in)
/// * `height` - Chart height in pixels (used to normalize delta)
///
/// # Returns
///
/// New `(min, max)` price range after zooming, clamped to 5%..2000% of the original range.
/// Zooming in stops at a span of 1e-6 of the larger bound's magnitude (1e-6 when both
/// bounds are 0). If the zoomed range would not be finite, `bounds` is returned.
pub fn apply_price_zoom(bounds: (f64, f64), anchor: f64, delta_y: f32, height: f32) -> (f64, f64) {
    let (min, max) = bounds;
    let range = auto_scale_span(min, max);
    // 1e-6 of the price: far above f64 rounding (~1e-16), at or below common tick sizes.
    let level = min.abs().max(max.abs());
    let floor = if level > 0.0 { level * 1e-6 } else { 1e-6 };
    // Never widen a range that is already narrower than the floor.
    let min_range = floor.min(range);
    // scale factor using exponential response; positive dy -> zoom out
    let scale = (-delta_y as f64 / height as f64).exp();
    let new_range = (range / scale)
        .clamp(range * 0.05, range * 20.0)
        .max(min_range);
    let t = ((anchor - min) / range).clamp(0.0, 1.0);
    let new_min = anchor - t * new_range;
    let new_max = new_min + new_range;
    if (new_max - new_min).is_finite() {
        (new_min, new_max)
    } else {
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual zoom must narrow ordinary intraday ranges, stop at a span tied to the
    /// price level, and never overflow to infinite or NaN bounds when zooming out.
    #[test]
    fn price_zoom_stays_within_sane_span() {
        // EURUSD, BTC and a series crossing zero, each zoomed in at its anchor.
        for (lo, hi, anchor) in [
            (1.0845, 1.0855, 1.085),
            (99_500.0, 100_000.0, 99_750.0),
            (-0.5, 0.5, 0.0),
        ] {
            let (min, max) = apply_price_zoom((lo, hi), anchor, -50.0, 500.0);
            assert!(max - min < hi - lo, "({lo}, {hi}) -> ({min}, {max})");
        }

        let mut bounds = (90.0, 110.0);
        for _ in 0..200 {
            bounds = apply_price_zoom(bounds, 100.0, -500.0, 500.0);
        }
        // 1e-6 of a price of 100.
        assert!(bounds.1 - bounds.0 >= 1e-4, "{bounds:?}");

        for _ in 0..1000 {
            bounds = apply_price_zoom(bounds, 100.0, 500.0, 500.0);
        }
        assert!(bounds.0.is_finite() && bounds.1.is_finite(), "{bounds:?}");
        assert!(bounds.0 < bounds.1, "{bounds:?}");
    }
}
