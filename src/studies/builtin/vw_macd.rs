//! Volume Weighted MACD (VW-MACD) indicator.
//!
//! A variation of MACD built from volume-weighted moving averages of the close,
//! giving more weight to price movements backed by higher volume.
//! This can reduce false signals during low-volume price moves.
//!
//! # Components
//!
//! - **VW-MACD line** = VWMA(close, fast) - VWMA(close, slow), where
//!   `VWMA(n) = sum(close * volume, n) / sum(volume, n)`.
//! - **Signal line** = EMA(VW-MACD line, signal).
//! - **Histogram** = VW-MACD line - signal line.
//!
//! The line starts on bar `max(fast, slow) - 1`; earlier bars have no value.
//! A window whose volume sums to 0 takes the simple average of its closes, as
//! [`VWMA`](super::VWMA) does.
//!
//! # Default parameters
//!
//! `VolumeWeightedMACD::new(12, 26, 9)` -- same periods as classic MACD.
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{VolumeWeightedMACD, Indicator};
//!
//! let mut vw = VolumeWeightedMACD::new(12, 26, 9);
//! vw.calculate(&bars);
//! ```

use super::VWMA;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Volume Weighted MACD indicator
///
/// The difference of a fast and a slow volume-weighted moving average of the
/// close, with an EMA signal line seeded with the first line value, as the
/// signal of [`MACD`](super::MACD) is.
#[derive(Clone)]
pub struct VolumeWeightedMACD {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl Default for VolumeWeightedMACD {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl VolumeWeightedMACD {
    /// Create a new VW-MACD with specified periods
    pub fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        Self {
            fast_period: fast_period.max(1),
            slow_period: slow_period.max(1),
            signal_period: signal_period.max(1),
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.macd_line, // MACD line
                DESIGN_TOKENS.semantic.indicators.macd_signal, // Signal line
                DESIGN_TOKENS.semantic.extended.success,     // Histogram positive
            ],
            visible: true,
        }
    }
}

impl Indicator for VolumeWeightedMACD {
    fn name(&self) -> &str {
        "VW-MACD"
    }

    fn desc(&self) -> &str {
        "Volume Weighted MACD - MACD using volume-weighted prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        let mut fast = VWMA::new(self.fast_period);
        let mut slow = VWMA::new(self.slow_period);
        fast.calculate(data);
        slow.calculate(data);

        let signal_mult = 2.0 / (self.signal_period as f64 + 1.0);
        let mut signal: Option<f64> = None;
        self.values = fast
            .values()
            .iter()
            .zip(slow.values())
            .map(|pair| match pair {
                (IndicatorValue::Single(fast), IndicatorValue::Single(slow)) => {
                    let line = fast - slow;
                    let sig = signal.map_or(line, |prev| (line - prev) * signal_mult + prev);
                    signal = Some(sig);
                    IndicatorValue::Multiple(vec![line, sig, line - sig])
                }
                _ => IndicatorValue::None,
            })
            .collect();
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            self.colors = vec![colors[0], colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        false // VW-MACD is drawn in separate pane
    }

    fn line_cnt(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!(
                "VW-MACD({},{},{})",
                self.fast_period, self.slow_period, self.signal_period
            ),
            "Signal".to_string(),
            "Histogram".to_string(),
        ]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(closes: &[f64], volumes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .zip(volumes)
            .map(|(&c, &v)| Bar::new(chrono::Utc::now(), c, c, c, c, v))
            .collect()
    }

    fn lines(study: &VolumeWeightedMACD, bar: usize) -> Option<&[f64]> {
        match &study.values()[bar] {
            IndicatorValue::Multiple(v) => Some(v),
            _ => None,
        }
    }

    /// Reference values from the published definition (Buff Pelz Dormeier, "Investing with
    /// Volume Analysis", 2011; John Bollinger, "Bollinger on Bollinger Bands", 2002):
    /// VWMA(close, fast) - VWMA(close, slow), a signal EMA seeded with the first line value,
    /// and their difference, computed independently in Python.
    #[test]
    fn matches_published_definition() {
        let closes = [
            102.0, 107.0, 108.0, 110.0, 114.0, 112.0, 117.0, 119.0, 121.0, 124.0,
        ];
        let volumes = [
            1000.0, 1500.0, 1200.0, 2000.0, 1800.0, 1400.0, 2500.0, 1600.0, 1300.0, 1700.0,
        ];
        let expected: [Option<[f64; 3]>; 10] = [
            None,
            None,
            None,
            None,
            Some([1.9866666666666646, 1.9866666666666646, 0.0]),
            Some([1.5306718597857838, 1.7586692632262242, -0.2279974034404404]),
            Some([2.0043366844076473, 1.8815029738169358, 0.12283371059071158]),
            Some([
                1.8037145650048956,
                1.8426087694109157,
                -0.038894204406020094,
            ]),
            Some([2.0206718346253183, 1.931640302018117, 0.08903153260720131]),
            Some([2.8483375959079353, 2.389988948963026, 0.4583486469449092]),
        ];
        let mut study = VolumeWeightedMACD::new(3, 5, 3);
        study.calculate(&bars(&closes, &volumes));
        assert_eq!(study.values().len(), expected.len());
        for (bar, want) in expected.iter().enumerate() {
            let got = lines(&study, bar);
            match want {
                None => assert!(got.is_none(), "bar {bar}: {got:?}"),
                Some(want) => {
                    let got = got.unwrap_or_else(|| panic!("bar {bar}: no value"));
                    for (g, w) in got.iter().zip(want) {
                        assert!((g - w).abs() < 1e-12, "bar {bar}: {got:?} != {want:?}");
                    }
                }
            }
        }

        // With constant volume the line is SMA(12) - SMA(26), 7 on a trend rising by 1 per bar,
        // whatever the length of the history.
        let closes: Vec<f64> = (0..200).map(|i| 100.0 + i as f64).collect();
        let mut study = VolumeWeightedMACD::default();
        study.calculate(&bars(&closes, &[1000.0; 200]));
        assert!(lines(&study, 24).is_none());
        for bar in 25..200 {
            let line = lines(&study, bar).map(|v| v[0]);
            assert!(
                line.is_some_and(|l| (l - 7.0).abs() < 1e-9),
                "bar {bar}: {line:?}"
            );
        }
    }
}
