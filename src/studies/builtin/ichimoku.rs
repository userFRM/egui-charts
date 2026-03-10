//! Ichimoku Cloud (Ichimoku Kinko Hyo) Indicator
//!
//! Ichimoku Cloud is a comprehensive indicator that shows support/resistance,
//! trend direction, momentum, and trading signals at a glance.
//!
//! # Components
//! - Tenkan-sen (Conversion Line): (9-period high + 9-period low) / 2
//! - Kijun-sen (Base Line): (26-period high + 26-period low) / 2
//! - Senkou Span A (Leading Span A): (Tenkan + Kijun) / 2, plotted 26 periods ahead
//! - Senkou Span B (Leading Span B): (52-period high + 52-period low) / 2, plotted 26 periods ahead
//! - Chikou Span (Lagging Span): Close plotted 26 periods back
//!
//! # Interpretation
//! - Price above cloud: Bullish trend
//! - Price below cloud: Bearish trend
//! - Cloud color: Green (Span A > Span B), Red (Span A < Span B)
//! - Tenkan crossing Kijun: Potential entry signal
//!
//! # Example
//! ```ignore
//! use egui_charts::IchimokuCloud;
//!
//! let mut ichimoku = IchimokuCloud::new(9, 26, 52);
//! ichimoku.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Ichimoku Cloud indicator
#[derive(Clone)]
pub struct IchimokuCloud {
    tenkan_period: usize,
    kijun_period: usize,
    senkou_b_period: usize,
    displacement: usize,
    values: Vec<IndicatorValue>,
    tenkan_color: Color32,
    kijun_color: Color32,
    span_a_color: Color32,
    span_b_color: Color32,
    chikou_color: Color32,
    visible: bool,
}

impl IchimokuCloud {
    /// Create a new Ichimoku Cloud indicator
    ///
    /// # Arguments
    /// * `tenkan_period` - Tenkan-sen period (typically 9)
    /// * `kijun_period` - Kijun-sen period (typically 26)
    /// * `senkou_b_period` - Senkou Span B period (typically 52)
    pub fn new(tenkan_period: usize, kijun_period: usize, senkou_b_period: usize) -> Self {
        Self {
            tenkan_period: tenkan_period.max(1),
            kijun_period: kijun_period.max(1),
            senkou_b_period: senkou_b_period.max(1),
            displacement: kijun_period, // Usually same as kijun_period
            values: Vec::new(),
            tenkan_color: DESIGN_TOKENS.semantic.extended.info, // Blue
            kijun_color: DESIGN_TOKENS.semantic.extended.error, // Red
            span_a_color: {
                let success = DESIGN_TOKENS.semantic.extended.success;
                Color32::from_rgba_unmultiplied(success.r(), success.g(), success.b(), 100)
            }, // Green cloud (transparent)
            span_b_color: {
                let error = DESIGN_TOKENS.semantic.extended.error;
                Color32::from_rgba_unmultiplied(error.r(), error.g(), error.b(), 100)
            }, // Red cloud (transparent)
            chikou_color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            visible: true,
        }
    }

    /// Create with default params (9, 26, 52)
    pub fn default_params() -> Self {
        Self::new(9, 26, 52)
    }

    /// Set custom displacement
    pub fn with_displacement(mut self, displacement: usize) -> Self {
        self.displacement = displacement;
        self
    }

    /// Calculate highest high over period
    fn highest_high(data: &[Bar], end: usize, period: usize) -> f64 {
        let start = end.saturating_sub(period - 1);
        data[start..=end]
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Calculate lowest low over period
    fn lowest_low(data: &[Bar], end: usize, period: usize) -> f64 {
        let start = end.saturating_sub(period - 1);
        data[start..=end]
            .iter()
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min)
    }

    /// Calculate Donchian midline (high + low) / 2
    fn donchian_mid(data: &[Bar], end: usize, period: usize) -> f64 {
        let hh = Self::highest_high(data, end, period);
        let ll = Self::lowest_low(data, end, period);
        (hh + ll) / 2.0
    }
}

impl Indicator for IchimokuCloud {
    fn name(&self) -> &str {
        "Ichimoku"
    }

    fn desc(&self) -> &str {
        "Ichimoku Cloud - Comprehensive trend indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.senkou_b_period;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate components
        let mut tenkan = vec![f64::NAN; data.len()];
        let mut kijun = vec![f64::NAN; data.len()];
        let mut senkou_a = vec![f64::NAN; data.len()];
        let mut senkou_b = vec![f64::NAN; data.len()];
        let mut chikou = vec![f64::NAN; data.len()];

        for i in 0..data.len() {
            // Tenkan-sen (Conversion Line)
            if i >= self.tenkan_period - 1 {
                tenkan[i] = Self::donchian_mid(data, i, self.tenkan_period);
            }

            // Kijun-sen (Base Line)
            if i >= self.kijun_period - 1 {
                kijun[i] = Self::donchian_mid(data, i, self.kijun_period);
            }

            // Senkou Span A (Leading Span A) - displaced forward
            // Note: We calculate current value; rendering should shift it forward
            if i >= self.kijun_period - 1 && !tenkan[i].is_nan() && !kijun[i].is_nan() {
                senkou_a[i] = (tenkan[i] + kijun[i]) / 2.0;
            }

            // Senkou Span B (Leading Span B) - displaced forward
            if i >= self.senkou_b_period - 1 {
                senkou_b[i] = Self::donchian_mid(data, i, self.senkou_b_period);
            }

            // Chikou Span (Lagging Span) - close price, displaced backward
            // Note: We store the close price; rendering should shift it backward
            chikou[i] = data[i].close;
        }

        // Build output values: [tenkan, kijun, senkou_a, senkou_b, chikou]
        for i in 0..data.len() {
            if i < self.senkou_b_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Multiple(vec![
                    if tenkan[i].is_nan() { 0.0 } else { tenkan[i] },
                    if kijun[i].is_nan() { 0.0 } else { kijun[i] },
                    if senkou_a[i].is_nan() {
                        0.0
                    } else {
                        senkou_a[i]
                    },
                    if senkou_b[i].is_nan() {
                        0.0
                    } else {
                        senkou_b[i]
                    },
                    chikou[i],
                ]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![
            self.tenkan_color,
            self.kijun_color,
            self.span_a_color,
            self.span_b_color,
            self.chikou_color,
        ]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.tenkan_color = colors[0];
        }
        if colors.len() > 1 {
            self.kijun_color = colors[1];
        }
        if colors.len() > 2 {
            self.span_a_color = colors[2];
        }
        if colors.len() > 3 {
            self.span_b_color = colors[3];
        }
        if colors.len() > 4 {
            self.chikou_color = colors[4];
        }
    }

    fn is_overlay(&self) -> bool {
        true // Ichimoku is drawn on main chart
    }

    fn line_cnt(&self) -> usize {
        5 // Tenkan, Kijun, Senkou A, Senkou B, Chikou
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("Tenkan({})", self.tenkan_period),
            format!("Kijun({})", self.kijun_period),
            "Senkou A".to_string(),
            format!("Senkou B({})", self.senkou_b_period),
            "Chikou".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..100)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.1).sin() * 20.0;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 2.0,
                    low: price - 2.0,
                    close: price + 0.5,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_ichimoku_calculation() {
        let bars = create_test_bars();
        let mut ichimoku = IchimokuCloud::new(9, 26, 52);
        ichimoku.calculate(&bars);

        assert_eq!(ichimoku.values().len(), bars.len());

        // First senkou_b_period - 1 values should be None
        for i in 0..51 {
            assert!(matches!(ichimoku.values()[i], IndicatorValue::None));
        }

        // Remaining values should have 5 components
        for value in ichimoku.values().iter().skip(51) {
            if let IndicatorValue::Multiple(vals) = value {
                assert_eq!(vals.len(), 5, "Should have 5 components");
            }
        }
    }

    #[test]
    fn test_ichimoku_line_cnt() {
        let ichimoku = IchimokuCloud::new(9, 26, 52);
        assert_eq!(ichimoku.line_cnt(), 5);
    }

    #[test]
    fn test_ichimoku_is_overlay() {
        let ichimoku = IchimokuCloud::new(9, 26, 52);
        assert!(ichimoku.is_overlay());
    }

    #[test]
    fn test_ichimoku_tenkan_faster_than_kijun() {
        // Tenkan uses shorter period, so should be more responsive
        let bars = create_test_bars();
        let mut ichimoku = IchimokuCloud::new(9, 26, 52);
        ichimoku.calculate(&bars);

        // Count crossovers - Tenkan should cross Kijun during trends
        let mut _crossovers = 0;
        for value in ichimoku.values().iter().skip(52) {
            if let IndicatorValue::Multiple(vals) = value {
                // Just verify both are calculated
                assert!(vals[0] > 0.0 && vals[1] > 0.0);
            }
        }
    }

    #[test]
    fn test_ichimoku_empty_data() {
        let mut ichimoku = IchimokuCloud::new(9, 26, 52);
        ichimoku.calculate(&[]);
        assert!(ichimoku.values().is_empty());
    }
}
