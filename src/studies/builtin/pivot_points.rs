//! Pivot Points indicator.
//!
//! Pivot Points calculate support and resistance levels from the previous
//! period's high, low, and close prices. Five calculation methods are
//! supported: Standard (Classic), Fibonacci, Woodie, Camarilla, and DeMark.
//!
//! # Output
//!
//! Each computed value is a `Multiple` vector containing
//! `[Pivot, S1, R1, S2, R2, S3, R3]` (7 levels).
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{PivotPoints, PivotMethod, Indicator};
//!
//! let mut pp = PivotPoints::new(PivotMethod::Standard, 20);
//! pp.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// The calculation method used by [`PivotPoints`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PivotMethod {
    /// Standard (Classic) pivot points: PP = (H + L + C) / 3.
    #[default]
    Standard,
    /// Fibonacci pivot points with 38.2%, 61.8%, 100% retracement levels.
    Fibonacci,
    /// Woodie pivot points (gives more weight to the close).
    Woodie,
    /// Camarilla pivot points using the 1.1 multiplier series.
    Camarilla,
    /// DeMark pivot points (conditional on open vs close).
    DeMark,
}

/// Pivot Points indicator producing seven support/resistance levels.
///
/// The indicator looks back over `period` bars to determine the highest
/// high, lowest low, and last close, then computes PP, S1-S3, and R1-R3.
/// Overlay indicator.
#[derive(Clone)]
pub struct PivotPoints {
    method: PivotMethod,
    /// Number of bars to use for calculating pivot (typically daily).
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors for: Pivot, S1, S2, S3, R1, R2, R3
    colors: Vec<Color32>,
    visible: bool,
}

impl PivotPoints {
    pub fn new(period: usize) -> Self {
        Self {
            method: PivotMethod::Standard,
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.favorite_gold, // Pivot - Amber
                DESIGN_TOKENS.semantic.extended.success,       // S1 - Green
                DESIGN_TOKENS.semantic.extended.success_dark,  // S2 - Dark Green
                DESIGN_TOKENS.semantic.extended.success_darker, // S3 - Darker Green
                DESIGN_TOKENS.semantic.extended.error,         // R1 - Red
                DESIGN_TOKENS.semantic.extended.error_dark,    // R2 - Dark Red
                DESIGN_TOKENS.semantic.extended.error_darker,  // R3 - Darker Red
            ],
            visible: true,
        }
    }

    pub fn with_method(mut self, method: PivotMethod) -> Self {
        self.method = method;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// Calculate pivot levels from high, low, close
    fn calculate_pivots(&self, high: f64, low: f64, close: f64, open: f64) -> PivotLevels {
        match self.method {
            PivotMethod::Standard => self.standard_pivots(high, low, close),
            PivotMethod::Fibonacci => self.fibonacci_pivots(high, low, close),
            PivotMethod::Woodie => self.woodie_pivots(high, low, close),
            PivotMethod::Camarilla => self.camarilla_pivots(high, low, close),
            PivotMethod::DeMark => self.demark_pivots(high, low, close, open),
        }
    }

    fn standard_pivots(&self, high: f64, low: f64, close: f64) -> PivotLevels {
        let pp = (high + low + close) / 3.0;
        let range = high - low;

        PivotLevels {
            pivot: pp,
            s1: 2.0 * pp - high,
            s2: pp - range,
            s3: low - 2.0 * (high - pp),
            r1: 2.0 * pp - low,
            r2: pp + range,
            r3: high + 2.0 * (pp - low),
        }
    }

    fn fibonacci_pivots(&self, high: f64, low: f64, close: f64) -> PivotLevels {
        let pp = (high + low + close) / 3.0;
        let range = high - low;

        PivotLevels {
            pivot: pp,
            s1: pp - 0.382 * range,
            s2: pp - 0.618 * range,
            s3: pp - 1.0 * range,
            r1: pp + 0.382 * range,
            r2: pp + 0.618 * range,
            r3: pp + 1.0 * range,
        }
    }

    fn woodie_pivots(&self, high: f64, low: f64, close: f64) -> PivotLevels {
        let pp = (high + low + 2.0 * close) / 4.0;
        let range = high - low;

        PivotLevels {
            pivot: pp,
            s1: 2.0 * pp - high,
            s2: pp - range,
            s3: low - 2.0 * (high - pp),
            r1: 2.0 * pp - low,
            r2: pp + range,
            r3: high + 2.0 * (pp - low),
        }
    }

    fn camarilla_pivots(&self, high: f64, low: f64, close: f64) -> PivotLevels {
        let range = high - low;

        PivotLevels {
            pivot: (high + low + close) / 3.0,
            s1: close - range * 1.1 / 12.0,
            s2: close - range * 1.1 / 6.0,
            s3: close - range * 1.1 / 4.0,
            r1: close + range * 1.1 / 12.0,
            r2: close + range * 1.1 / 6.0,
            r3: close + range * 1.1 / 4.0,
        }
    }

    fn demark_pivots(&self, high: f64, low: f64, close: f64, open: f64) -> PivotLevels {
        let x = if close < open {
            high + 2.0 * low + close
        } else if close > open {
            2.0 * high + low + close
        } else {
            high + low + 2.0 * close
        };

        let pp = x / 4.0;

        PivotLevels {
            pivot: pp,
            s1: x / 2.0 - high,
            s2: pp - (high - low),       // Extended
            s3: pp - 2.0 * (high - low), // Extended
            r1: x / 2.0 - low,
            r2: pp + (high - low),       // Extended
            r3: pp + 2.0 * (high - low), // Extended
        }
    }
}

/// Pivot point levels
#[derive(Clone, Copy, Debug)]
struct PivotLevels {
    pivot: f64,
    s1: f64,
    s2: f64,
    s3: f64,
    r1: f64,
    r2: f64,
    r3: f64,
}

impl Indicator for PivotPoints {
    fn name(&self) -> &str {
        "Pivot Points"
    }

    fn desc(&self) -> &str {
        "Pivot Points - Support and resistance levels from previous period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Find period high, low, close
        let mut i = 0;
        while i < data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
                i += 1;
                continue;
            }

            // Calculate from previous period
            let period_data = &data[i - self.period..i];
            let high = period_data
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let low = period_data
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);
            let close = period_data.last().unwrap().close;
            let open = period_data.first().unwrap().open;

            let levels = self.calculate_pivots(high, low, close, open);

            // Store all 7 levels as Multiple
            self.values.push(IndicatorValue::Multiple(vec![
                levels.pivot,
                levels.s1,
                levels.s2,
                levels.s3,
                levels.r1,
                levels.r2,
                levels.r3,
            ]));

            i += 1;
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 7 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true // Drawn on price chart
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
            format!("P({})", self.period),
            "S1".to_string(),
            "S2".to_string(),
            "S3".to_string(),
            "R1".to_string(),
            "R2".to_string(),
            "R3".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_pivots() {
        let pp = PivotPoints::new(1);

        // High = 110, Low = 90, Close = 100
        // PP = (110 + 90 + 100) / 3 = 100
        // R1 = 2 * 100 - 90 = 110
        // S1 = 2 * 100 - 110 = 90
        let levels = pp.standard_pivots(110.0, 90.0, 100.0);

        assert!((levels.pivot - 100.0).abs() < 0.01);
        assert!((levels.r1 - 110.0).abs() < 0.01);
        assert!((levels.s1 - 90.0).abs() < 0.01);
    }

    #[test]
    fn test_fibonacci_pivots() {
        let pp = PivotPoints::new(1).with_method(PivotMethod::Fibonacci);

        let levels = pp.fibonacci_pivots(110.0, 90.0, 100.0);

        // Range = 20
        // S1 = 100 - 0.382 * 20 = 92.36
        // R1 = 100 + 0.382 * 20 = 107.64
        assert!((levels.pivot - 100.0).abs() < 0.01);
        assert!((levels.s1 - 92.36).abs() < 0.01);
        assert!((levels.r1 - 107.64).abs() < 0.01);
    }
}
