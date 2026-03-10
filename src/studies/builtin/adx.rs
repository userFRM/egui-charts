//! Avg Directional Index (ADX) Indicator
//!
//! ADX measures trend strength regardless of direction. It's derived from the
//! Directional Movement Index (DMI) system which includes +DI and -DI.
//!
//! # Components
//! - ADX: Trend strength (0-100)
//! - +DI: Positive directional indicator
//! - -DI: Negative directional indicator
//!
//! # Interpretation
//! - ADX > 25: Strong trend
//! - ADX < 20: Weak or no trend
//! - +DI > -DI: Bullish trend
//! - -DI > +DI: Bearish trend
//!
//! # Example
//! ```ignore
//! use egui_charts::ADX;
//!
//! let mut adx = ADX::new(14);
//! adx.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Avg Directional Index indicator
#[derive(Clone)]
pub struct ADX {
    period: usize,
    values: Vec<IndicatorValue>,
    adx_color: Color32,
    plus_di_color: Color32,
    minus_di_color: Color32,
    visible: bool,
}

impl ADX {
    /// Create a new ADX indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            adx_color: DESIGN_TOKENS.semantic.extended.favorite_gold, // Amber (ADX)
            plus_di_color: DESIGN_TOKENS.semantic.extended.success,   // Green (+DI)
            minus_di_color: DESIGN_TOKENS.semantic.extended.error,    // Red (-DI)
            visible: true,
        }
    }

    /// Set colors for all three lines
    pub fn with_colors(mut self, adx: Color32, plus_di: Color32, minus_di: Color32) -> Self {
        self.adx_color = adx;
        self.plus_di_color = plus_di;
        self.minus_di_color = minus_di;
        self
    }

    /// Calculate True Range
    #[inline]
    fn true_range(bar: &Bar, prev_close: f64) -> f64 {
        let hl = bar.high - bar.low;
        let hc = (bar.high - prev_close).abs();
        let lc = (bar.low - prev_close).abs();
        hl.max(hc).max(lc)
    }

    /// Wilder's smoothing
    #[inline]
    fn wilders_smooth(prev: f64, current: f64, period: usize) -> f64 {
        prev - (prev / period as f64) + current
    }
}

impl Indicator for ADX {
    fn name(&self) -> &str {
        "ADX"
    }

    fn desc(&self) -> &str {
        "Avg Directional Index - Trend strength indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period * 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate +DM, -DM, and TR for each bar
        let mut plus_dm = Vec::with_capacity(data.len());
        let mut minus_dm = Vec::with_capacity(data.len());
        let mut tr = Vec::with_capacity(data.len());

        plus_dm.push(0.0);
        minus_dm.push(0.0);
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            let up_move = data[i].high - data[i - 1].high;
            let down_move = data[i - 1].low - data[i].low;

            // +DM
            if up_move > down_move && up_move > 0.0 {
                plus_dm.push(up_move);
            } else {
                plus_dm.push(0.0);
            }

            // -DM
            if down_move > up_move && down_move > 0.0 {
                minus_dm.push(down_move);
            } else {
                minus_dm.push(0.0);
            }

            // True Range
            tr.push(Self::true_range(&data[i], data[i - 1].close));
        }

        // Calculate smoothed values using Wilder's method
        let mut smoothed_plus_dm = vec![0.0; data.len()];
        let mut smoothed_minus_dm = vec![0.0; data.len()];
        let mut smoothed_tr = vec![0.0; data.len()];

        // Initial sums for first period
        let sum_plus_dm: f64 = plus_dm[1..=self.period].iter().sum();
        let sum_minus_dm: f64 = minus_dm[1..=self.period].iter().sum();
        let sum_tr: f64 = tr[1..=self.period].iter().sum();

        smoothed_plus_dm[self.period] = sum_plus_dm;
        smoothed_minus_dm[self.period] = sum_minus_dm;
        smoothed_tr[self.period] = sum_tr;

        // Subsequent smoothed values
        for i in (self.period + 1)..data.len() {
            smoothed_plus_dm[i] =
                Self::wilders_smooth(smoothed_plus_dm[i - 1], plus_dm[i], self.period);
            smoothed_minus_dm[i] =
                Self::wilders_smooth(smoothed_minus_dm[i - 1], minus_dm[i], self.period);
            smoothed_tr[i] = Self::wilders_smooth(smoothed_tr[i - 1], tr[i], self.period);
        }

        // Calculate +DI and -DI
        let mut plus_di = vec![0.0; data.len()];
        let mut minus_di = vec![0.0; data.len()];
        let mut dx = vec![0.0; data.len()];

        for i in self.period..data.len() {
            if smoothed_tr[i].abs() > 1e-10 {
                plus_di[i] = 100.0 * smoothed_plus_dm[i] / smoothed_tr[i];
                minus_di[i] = 100.0 * smoothed_minus_dm[i] / smoothed_tr[i];
            }

            let di_sum = plus_di[i] + minus_di[i];
            if di_sum.abs() > 1e-10 {
                dx[i] = 100.0 * (plus_di[i] - minus_di[i]).abs() / di_sum;
            }
        }

        // Calculate ADX (smoothed DX)
        let mut adx = vec![0.0; data.len()];
        let adx_start = self.period * 2 - 1;

        if adx_start < data.len() {
            // Initial ADX is avg of first period DX values
            let initial_sum: f64 = dx[self.period..adx_start].iter().sum();
            adx[adx_start] = initial_sum / self.period as f64;

            // Subsequent ADX values
            for i in adx_start + 1..data.len() {
                adx[i] = (adx[i - 1] * (self.period - 1) as f64 + dx[i]) / self.period as f64;
            }
        }

        // Build output values
        for i in 0..data.len() {
            if i < self.period * 2 - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Multiple(vec![
                    adx[i],
                    plus_di[i],
                    minus_di[i],
                ]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.adx_color, self.plus_di_color, self.minus_di_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.adx_color = colors[0];
        }
        if colors.len() > 1 {
            self.plus_di_color = colors[1];
        }
        if colors.len() > 2 {
            self.minus_di_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        false // ADX is drawn in separate pane
    }

    fn line_cnt(&self) -> usize {
        3 // ADX, +DI, -DI
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
            format!("ADX({})", self.period),
            "+DI".to_string(),
            "-DI".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_trending_bars(count: usize, uptrend: bool) -> Vec<Bar> {
        let start = Utc::now();
        let mut price = 100.0;
        let direction = if uptrend { 1.0 } else { -1.0 };

        (0..count)
            .map(|i| {
                price += direction * 0.5;
                Bar {
                    time: start + Duration::minutes(i as i64 * 5),
                    open: price - direction * 0.2,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_adx_range() {
        let bars = create_trending_bars(50, true);
        let mut adx = ADX::new(14);
        adx.calculate(&bars);

        for value in adx.values() {
            if let IndicatorValue::Multiple(vals) = value {
                for v in vals {
                    assert!(*v >= 0.0 && *v <= 100.0, "ADX/DI should be 0-100");
                }
            }
        }
    }

    #[test]
    fn test_adx_line_cnt() {
        let adx = ADX::new(14);
        assert_eq!(adx.line_cnt(), 3);
    }

    #[test]
    fn test_adx_uptrend() {
        let bars = create_trending_bars(50, true);
        let mut adx = ADX::new(14);
        adx.calculate(&bars);

        // In uptrend, +DI should generally be > -DI
        let mut plus_di_wins = 0;
        let mut total = 0;

        for value in adx.values() {
            if let IndicatorValue::Multiple(vals) = value {
                total += 1;
                if vals[1] > vals[2] {
                    plus_di_wins += 1;
                }
            }
        }

        assert!(
            plus_di_wins as f64 / total as f64 > 0.5,
            "+DI should be > -DI in uptrend"
        );
    }

    #[test]
    fn test_adx_empty_data() {
        let mut adx = ADX::new(14);
        adx.calculate(&[]);
        assert!(adx.values().is_empty());
    }
}
