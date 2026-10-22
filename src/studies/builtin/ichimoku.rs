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
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{IchimokuCloud, Indicator};
//!
//! let mut ichimoku = IchimokuCloud::new(9, 26, 52);
//! ichimoku.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Ichimoku Cloud indicator
///
/// Each value is `[tenkan, kijun, senkou_a, senkou_b, chikou]`, with NaN for a
/// line that has no value at that bar, and `None` where no line has one. The
/// value at bar `i` holds Senkou Span A and B computed on bar `i - displacement`
/// and the close of bar `i + displacement` as the Chikou Span, so the spans are
/// already displaced. There is one value per bar, so the leading spans computed
/// on the last `displacement` bars, which fall after the last bar, are not
/// output, and the Chikou Span is NaN on the last `displacement` bars.
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
        let kijun_period = kijun_period.max(1);
        Self {
            tenkan_period: tenkan_period.max(1),
            kijun_period,
            senkou_b_period: senkou_b_period.max(1),
            displacement: kijun_period,
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

    /// Set the number of bars Senkou Span A and B are plotted ahead and the
    /// Chikou Span behind (the Kijun period by default; 0 plots them unshifted)
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

/// Construct with the conventional default parameters.
impl Default for IchimokuCloud {
    fn default() -> Self {
        Self::new(9, 26, 52)
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

        let mid = |end: usize, period: usize| {
            (end + 1 >= period).then(|| Self::donchian_mid(data, end, period))
        };
        let d = self.displacement;
        for i in 0..data.len() {
            let tenkan = mid(i, self.tenkan_period);
            let kijun = mid(i, self.kijun_period);
            // Leading spans: computed on bar i - d, plotted on bar i
            let base = i.checked_sub(d);
            let span_a = base.and_then(|j| {
                Some((mid(j, self.tenkan_period)? + mid(j, self.kijun_period)?) / 2.0)
            });
            let span_b = base.and_then(|j| mid(j, self.senkou_b_period));
            // Lagging span: the close of bar i + d, plotted on bar i
            let chikou = i.checked_add(d).and_then(|j| data.get(j)).map(|b| b.close);

            let row = [tenkan, kijun, span_a, span_b, chikou];
            self.values.push(if row.iter().all(Option::is_none) {
                IndicatorValue::None
            } else {
                IndicatorValue::Multiple(row.map(|v| v.unwrap_or(f64::NAN)).to_vec())
            });
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

    /// Tenkan(3), Kijun(5), Senkou B(8) and a displacement of 5 on 20 bars,
    /// against values computed by hand from the textbook definitions: Senkou A
    /// and B are plotted 5 bars after the bar they are computed on, and Chikou
    /// is the close of the bar 5 bars later.
    #[test]
    fn spans_are_displaced() {
        let highs = [
            12., 14., 13., 15., 17., 16., 18., 20., 19., 21., 23., 22., 20., 19., 21., 24., 25.,
            23., 22., 26.,
        ];
        let lows = [
            10., 11., 11., 12., 14., 14., 15., 17., 17., 18., 20., 19., 18., 17., 18., 21., 22.,
            21., 20., 23.,
        ];
        let closes = [
            11., 13., 12., 14., 16., 15., 17., 19., 18., 20., 22., 20., 19., 18., 20., 23., 24.,
            22., 21., 25.,
        ];
        let start = Utc::now();
        let bars: Vec<Bar> = (0..20)
            .map(|i| Bar {
                time: start + Duration::minutes(i as i64),
                open: closes[i],
                high: highs[i],
                low: lows[i],
                close: closes[i],
                volume: 1.0,
            })
            .collect();
        let n = f64::NAN;
        // [tenkan, kijun, senkou A, senkou B, chikou] per bar
        let expected = [
            [n, n, n, n, 15.0],
            [n, n, n, n, 17.0],
            [12.0, n, n, n, 19.0],
            [13.0, n, n, n, 18.0],
            [14.0, 13.5, n, n, 20.0],
            [14.5, 14.0, n, n, 22.0],
            [16.0, 14.5, n, n, 20.0],
            [17.0, 16.0, n, n, 19.0],
            [17.5, 17.0, n, n, 18.0],
            [19.0, 17.5, 13.75, n, 20.0],
            [20.0, 19.0, 14.25, n, 23.0],
            [20.5, 20.0, 15.25, n, 24.0],
            [20.5, 20.0, 16.5, 15.0, 22.0],
            [19.5, 20.0, 17.25, 15.5, 21.0],
            [19.0, 20.0, 18.25, 16.0, 25.0],
            [20.5, 20.5, 19.5, 17.5, n],
            [21.5, 21.0, 20.25, 18.5, n],
            [23.0, 21.0, 20.25, 18.5, n],
            [22.5, 21.5, 19.75, 19.0, n],
            [23.0, 23.0, 19.5, 20.0, n],
        ];

        let mut ichimoku = IchimokuCloud::new(3, 5, 8);
        ichimoku.calculate(&bars);

        assert_eq!(ichimoku.values().len(), bars.len());
        for (i, (value, row)) in ichimoku.values().iter().zip(expected).enumerate() {
            let IndicatorValue::Multiple(got) = value else {
                panic!("bar {i}: {value:?}");
            };
            let same = got.len() == 5
                && got
                    .iter()
                    .zip(row)
                    .all(|(g, e)| (g.is_nan() && e.is_nan()) || *g == e);
            assert!(same, "bar {i}: {got:?} != {row:?}");
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
    fn test_ichimoku_empty_data() {
        let mut ichimoku = IchimokuCloud::new(9, 26, 52);
        ichimoku.calculate(&[]);
        assert!(ichimoku.values().is_empty());
    }
}
