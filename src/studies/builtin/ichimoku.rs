//! Ichimoku Cloud (Ichimoku Kinko Hyo) Indicator
//!
//! Ichimoku Cloud is a comprehensive indicator that shows support/resistance,
//! trend direction, momentum, and trading signals at a glance.
//!
//! # Components
//! - Tenkan-sen (Conversion Line): (9-period high + 9-period low) / 2
//! - Kijun-sen (Base Line): (26-period high + 26-period low) / 2
//! - Senkou Span A (Leading Span A): (Tenkan + Kijun) / 2, plotted 25 bars ahead with displacement 26
//! - Senkou Span B (Leading Span B): (52-period high + 52-period low) / 2, plotted 25 bars ahead with displacement 26
//! - Chikou Span (Lagging Span): Close plotted 25 bars back with displacement 26
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

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::{rolling_max, rolling_min};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Ichimoku Cloud indicator
///
/// Each row holds `[tenkan, kijun, senkou_a, senkou_b, close]` computed at its
/// source bar, with NaN during warmup. Leading spans use a positive plot offset
/// and Chikou a negative one. Displacement counts the current bar as period one:
/// 26 shifts by 25 bars. Zero and one leave the spans unshifted. All leading
/// values are retained, including those drawn beyond the last price bar.
#[derive(Clone)]
pub struct IchimokuCloud {
    tenkan_period: usize,
    kijun_period: usize,
    senkou_b_period: usize,
    displacement: usize,
    values: IndicatorValues,
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
            values: IndicatorValues::new(),
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

    /// Set displacement periods, counting the current bar as the first.
    /// The Kijun period is the default; 0 and 1 both leave spans unshifted.
    pub fn with_displacement(mut self, displacement: usize) -> Self {
        self.displacement = displacement;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for IchimokuCloud {
    fn default() -> Self {
        Self::new(9, 26, 52)
    }
}

impl IchimokuCloud {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        // Midpoint of the highest high and lowest low of each full window
        let mid = |period: usize| -> Vec<Option<f64>> {
            let highest = rolling_max(data.iter().map(|b| b.high), period);
            let lowest = rolling_min(data.iter().map(|b| b.low), period);
            highest
                .into_iter()
                .zip(lowest)
                .enumerate()
                .map(|(i, (hh, ll))| (i + 1 >= period).then_some((hh + ll) / 2.0))
                .collect()
        };
        let tenkan_line = mid(self.tenkan_period);
        let kijun_line = mid(self.kijun_period);
        let senkou_b_line = mid(self.senkou_b_period);
        for i in 0..data.len() {
            let tenkan = tenkan_line[i];
            let kijun = kijun_line[i];
            let span_a = tenkan
                .zip(kijun)
                .map(|(tenkan, kijun)| (tenkan + kijun) / 2.0);
            let span_b = senkou_b_line[i];
            let chikou = Some(data[i].close);

            let row = [tenkan, kijun, span_a, span_b, chikou];
            values.push(if row.iter().all(Option::is_none) {
                IndicatorValue::None
            } else {
                IndicatorValue::Multiple(row.map(|v| v.unwrap_or(f64::NAN)).to_vec())
            });
        }

        values
    }
}

impl Indicator for IchimokuCloud {
    crate::studies::inputs::study_inputs!(self;
        tenkan_period: length("Tenkan period", 1, usize::MAX),
        kijun_period: length("Kijun period", 1, usize::MAX),
        senkou_b_period: length("Senkou b period", 1, usize::MAX),
        displacement: length("Displacement", 0, usize::MAX)
    ; {});

    fn plot_offset(&self, line: usize) -> isize {
        let shift = self.displacement.saturating_sub(1).min(isize::MAX as usize) as isize;
        match line {
            2 | 3 => shift,
            4 => -shift,
            _ => 0,
        }
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(2),
            second: crate::studies::FillBoundary::Plot(3),
            color: self.span_a_color,
            inverted_color: Some(self.span_b_color),
        }]
    }

    fn name(&self) -> &str {
        "Ichimoku"
    }

    fn desc(&self) -> &str {
        "Ichimoku Cloud - Comprehensive trend indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &IndicatorValues {
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

    fn line_count(&self) -> usize {
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

    /// Source values calculated by hand; displacement includes the current period.
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
                colors: None,
                payload: None,
            })
            .collect();
        let n = f64::NAN;
        // [tenkan, kijun, senkou A, senkou B, chikou] per bar
        let expected = [
            [n, n, n, n, 11.0],
            [n, n, n, n, 13.0],
            [12.0, n, n, n, 12.0],
            [13.0, n, n, n, 14.0],
            [14.0, 13.5, 13.75, n, 16.0],
            [14.5, 14.0, 14.25, n, 15.0],
            [16.0, 14.5, 15.25, n, 17.0],
            [17.0, 16.0, 16.5, 15.0, 19.0],
            [17.5, 17.0, 17.25, 15.5, 18.0],
            [19.0, 17.5, 18.25, 16.0, 20.0],
            [20.0, 19.0, 19.5, 17.5, 22.0],
            [20.5, 20.0, 20.25, 18.5, 20.0],
            [20.5, 20.0, 20.25, 18.5, 19.0],
            [19.5, 20.0, 19.75, 19.0, 18.0],
            [19.0, 20.0, 19.5, 20.0, 20.0],
            [20.5, 20.5, 20.5, 20.5, 23.0],
            [21.5, 21.0, 21.25, 21.0, 24.0],
            [23.0, 21.0, 22.0, 21.0, 22.0],
            [22.5, 21.5, 22.0, 21.0, 21.0],
            [23.0, 23.0, 23.0, 21.5, 25.0],
        ];

        let mut ichimoku = IchimokuCloud::new(3, 5, 8);
        ichimoku.calculate(&bars);

        assert_eq!(ichimoku.values().len(), bars.len());
        assert_eq!(ichimoku.plot_offset(2), 4);
        assert_eq!(ichimoku.plot_offset(4), -4);
        assert_eq!(ichimoku.plot_value(2, 8), Some(13.75));
        assert_eq!(ichimoku.plot_value(2, 23), Some(23.0));
        assert_eq!(ichimoku.plot_value(4, 0), Some(16.0));
        for (periods, offset) in [(0, 0), (1, 0), (26, 25), (usize::MAX, isize::MAX)] {
            let study = IchimokuCloud::default().with_displacement(periods);
            assert_eq!(study.plot_offset(2), offset);
            assert_eq!(study.plot_offset(4), -offset);
        }
        for (i, (value, row)) in ichimoku.values().to_rows().iter().zip(expected).enumerate() {
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
}
