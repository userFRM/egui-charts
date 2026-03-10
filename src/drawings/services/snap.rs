//! Snap Service - magnet snapping for drawing tools
//!
//! Provides snap-to-price, snap-to-time, and magnet mode (snap to existing drawing points).
//!
//! # Features
//!
//! - Snap to OHLC prices of visible candles
//! - Snap to candle open times
//! - Magnet mode: snap to nearby existing drawing points

use egui::Pos2;

/// Options for snap behavior
#[derive(Debug, Clone)]
pub struct SnapOptions {
    /// Enable snap to OHLC prices
    pub snap_to_price: bool,
    /// Enable snap to candle times
    pub snap_to_time: bool,
    /// Snap distance threshold in pixels
    pub snap_distance: f32,
    /// Enable magnet mode (snap to existing drawing points)
    pub magnet_mode: bool,
    /// Magnet distance threshold in pixels
    pub magnet_distance: f32,
}

impl Default for SnapOptions {
    fn default() -> Self {
        Self {
            snap_to_price: true,
            snap_to_time: true,
            snap_distance: 10.0,
            magnet_mode: false,
            magnet_distance: 15.0,
        }
    }
}

/// Snap targets - prices and times to snap to
#[derive(Debug, Clone, Default)]
pub struct SnapTargets {
    /// Y-coordinates of OHLC prices in screen space
    pub prices: Vec<f32>,
    /// X-coordinates of candle times in screen space
    pub times: Vec<f32>,
    /// Points from existing drawings (for magnet mode)
    pub drawing_points: Vec<Pos2>,
}

impl SnapTargets {
    /// Create empty snap targets
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with price and time targets
    pub fn with_price_time(prices: Vec<f32>, times: Vec<f32>) -> Self {
        Self {
            prices,
            times,
            drawing_points: Vec::new(),
        }
    }

    /// Add drawing points for magnet mode
    pub fn with_drawing_points(mut self, points: Vec<Pos2>) -> Self {
        self.drawing_points = points;
        self
    }

    /// Update price targets
    pub fn set_prices(&mut self, prices: Vec<f32>) {
        self.prices = prices;
    }

    /// Update time targets
    pub fn set_times(&mut self, times: Vec<f32>) {
        self.times = times;
    }

    /// Update drawing points for magnet mode
    pub fn set_drawing_points(&mut self, points: Vec<Pos2>) {
        self.drawing_points = points;
    }

    /// Clear all targets
    pub fn clear(&mut self) {
        self.prices.clear();
        self.times.clear();
        self.drawing_points.clear();
    }
}

/// Snap service for applying snap behavior to points
#[derive(Debug, Clone)]
pub struct SnapService {
    options: SnapOptions,
}

impl Default for SnapService {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapService {
    /// Create a new snap service with default options
    pub fn new() -> Self {
        Self {
            options: SnapOptions::default(),
        }
    }

    /// Create with custom options
    pub fn with_options(options: SnapOptions) -> Self {
        Self { options }
    }

    /// Get the current options
    pub fn options(&self) -> &SnapOptions {
        &self.options
    }

    /// Get mutable options
    pub fn options_mut(&mut self) -> &mut SnapOptions {
        &mut self.options
    }

    /// Apply snap to a point
    ///
    /// Applies in order:
    /// 1. Snap to price (Y axis)
    /// 2. Snap to time (X axis)
    /// 3. Magnet mode (snap to nearest drawing point)
    pub fn snap_point(&self, point: Pos2, targets: &SnapTargets) -> Pos2 {
        let mut result = point;

        // Snap to price (Y axis)
        if self.options.snap_to_price
            && !targets.prices.is_empty()
            && let Some(snapped_y) = self.find_closest(&targets.prices, point.y)
        {
            result.y = snapped_y;
        }

        // Snap to time (X axis)
        if self.options.snap_to_time
            && !targets.times.is_empty()
            && let Some(snapped_x) = self.find_closest(&targets.times, point.x)
        {
            result.x = snapped_x;
        }

        // Magnet mode - snap to nearby drawing points
        if self.options.magnet_mode
            && !targets.drawing_points.is_empty()
            && let Some(closest_point) = self.find_closest_point(&targets.drawing_points, point)
        {
            result = closest_point;
        }

        result
    }

    /// Find closest value within snap distance
    fn find_closest(&self, values: &[f32], target: f32) -> Option<f32> {
        values
            .iter()
            .min_by(|a, b| {
                (target - **a)
                    .abs()
                    .partial_cmp(&(target - **b).abs())
                    .unwrap()
            })
            .filter(|&&closest| (target - closest).abs() < self.options.snap_distance)
            .copied()
    }

    /// Find closest point within magnet distance
    fn find_closest_point(&self, points: &[Pos2], target: Pos2) -> Option<Pos2> {
        let mut min_dist = self.options.magnet_distance;
        let mut closest = None;

        for &p in points {
            let dist = ((target.x - p.x).powi(2) + (target.y - p.y).powi(2)).sqrt();
            if dist < min_dist {
                min_dist = dist;
                closest = Some(p);
            }
        }

        closest
    }

    /// Check if a point would snap
    ///
    /// Returns true if the point would be modified by snapping.
    pub fn would_snap(&self, point: Pos2, targets: &SnapTargets) -> bool {
        let snapped = self.snap_point(point, targets);
        (snapped.x - point.x).abs() > 0.001 || (snapped.y - point.y).abs() > 0.001
    }

    /// Get snap indicator position if snapping is active
    ///
    /// Returns the snapped position only if it differs from input,
    /// useful for showing snap indicators in the UI.
    pub fn get_snap_indicator(&self, point: Pos2, targets: &SnapTargets) -> Option<Pos2> {
        let snapped = self.snap_point(point, targets);
        if (snapped.x - point.x).abs() > 0.001 || (snapped.y - point.y).abs() > 0.001 {
            Some(snapped)
        } else {
            None
        }
    }

    /// Enable/disable snap to price
    pub fn set_snap_to_price(&mut self, enabled: bool) {
        self.options.snap_to_price = enabled;
    }

    /// Enable/disable snap to time
    pub fn set_snap_to_time(&mut self, enabled: bool) {
        self.options.snap_to_time = enabled;
    }

    /// Enable/disable magnet mode
    pub fn set_magnet_mode(&mut self, enabled: bool) {
        self.options.magnet_mode = enabled;
    }

    /// Set snap distance
    pub fn set_snap_distance(&mut self, distance: f32) {
        self.options.snap_distance = distance;
    }

    /// Set magnet distance
    pub fn set_magnet_distance(&mut self, distance: f32) {
        self.options.magnet_distance = distance;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snap_to_price() {
        let service = SnapService::new();
        let targets = SnapTargets::with_price_time(vec![100.0, 150.0, 200.0], vec![]);

        // Point near 150 should snap
        let point = Pos2::new(50.0, 152.0);
        let snapped = service.snap_point(point, &targets);
        assert_eq!(snapped.y, 150.0);
        assert_eq!(snapped.x, 50.0); // X unchanged
    }

    #[test]
    fn test_snap_to_time() {
        let service = SnapService::new();
        let targets = SnapTargets::with_price_time(vec![], vec![100.0, 200.0, 300.0]);

        // Point near 200 should snap
        let point = Pos2::new(203.0, 50.0);
        let snapped = service.snap_point(point, &targets);
        assert_eq!(snapped.x, 200.0);
        assert_eq!(snapped.y, 50.0); // Y unchanged
    }

    #[test]
    fn test_no_snap_outside_distance() {
        let service = SnapService::new(); // default snap_distance = 10
        let targets = SnapTargets::with_price_time(vec![100.0], vec![200.0]);

        // Point too far from targets
        let point = Pos2::new(250.0, 150.0);
        let snapped = service.snap_point(point, &targets);
        assert_eq!(snapped, point); // Unchanged
    }

    #[test]
    fn test_magnet_mode() {
        let mut service = SnapService::new();
        service.set_magnet_mode(true);

        let targets = SnapTargets::with_price_time(vec![], vec![])
            .with_drawing_points(vec![Pos2::new(100.0, 100.0), Pos2::new(200.0, 200.0)]);

        // Point near first drawing point
        let point = Pos2::new(105.0, 103.0);
        let snapped = service.snap_point(point, &targets);
        assert_eq!(snapped, Pos2::new(100.0, 100.0));
    }

    #[test]
    fn test_magnet_outside_distance() {
        let mut service = SnapService::new();
        service.set_magnet_mode(true);
        service.set_magnet_distance(10.0);

        let targets = SnapTargets::with_price_time(vec![], vec![])
            .with_drawing_points(vec![Pos2::new(100.0, 100.0)]);

        // Point too far
        let point = Pos2::new(150.0, 150.0);
        let snapped = service.snap_point(point, &targets);
        assert_eq!(snapped, point);
    }

    #[test]
    fn test_disabled_snap() {
        let mut service = SnapService::new();
        service.set_snap_to_price(false);
        service.set_snap_to_time(false);

        let targets = SnapTargets::with_price_time(vec![100.0], vec![100.0]);

        let point = Pos2::new(101.0, 101.0);
        let snapped = service.snap_point(point, &targets);
        assert_eq!(snapped, point); // No snapping
    }

    #[test]
    fn test_would_snap() {
        let service = SnapService::new();
        let targets = SnapTargets::with_price_time(vec![100.0], vec![]);

        assert!(service.would_snap(Pos2::new(50.0, 102.0), &targets));
        assert!(!service.would_snap(Pos2::new(50.0, 150.0), &targets));
    }

    #[test]
    fn test_snap_indicator() {
        let service = SnapService::new();
        let targets = SnapTargets::with_price_time(vec![100.0], vec![]);

        // Should show indicator when snapping
        let indicator = service.get_snap_indicator(Pos2::new(50.0, 102.0), &targets);
        assert!(indicator.is_some());
        assert_eq!(indicator.unwrap().y, 100.0);

        // No indicator when not snapping
        let indicator = service.get_snap_indicator(Pos2::new(50.0, 150.0), &targets);
        assert!(indicator.is_none());
    }
}
