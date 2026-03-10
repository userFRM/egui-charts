//! Low-level coordinate conversion and zoom helpers.
//!
//! These utility functions are used by [`pan_zoom`](super::pan_zoom) and the
//! rendering pipeline to convert between screen-space Y coordinates and price
//! values, and to apply price-axis zoom transformations anchored at a given
//! price level.

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
pub fn apply_price_zoom(bounds: (f64, f64), anchor: f64, delta_y: f32, height: f32) -> (f64, f64) {
    let (min, max) = bounds;
    let range = (max - min).max(1e-12);
    // scale factor using exponential response; positive dy -> zoom out
    let scale = (-delta_y as f64 / height as f64).exp();
    let new_range = (range / scale).clamp(range * 0.05, range * 20.0);
    let t = ((anchor - min) / range).clamp(0.0, 1.0);
    let new_min = anchor - t * new_range;
    let new_max = new_min + new_range;
    (new_min, new_max)
}
