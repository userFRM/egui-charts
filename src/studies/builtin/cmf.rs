//! Chaikin Money Flow (CMF) Indicator
//!
//! CMF measures the amount of Money Flow Volume over a specific period,
//! providing a bounded oscillator between -1 and +1.
//!
//! # Formula
//! CMF = Sum(Money Flow Volume, period) / Sum(Volume, period)
//! Where Money Flow Volume = Money Flow Multiplier * Volume
//! And Money Flow Multiplier = ((Close - Low) - (High - Close)) / (High - Low)
//! A bar whose window holds no volume has no value.
//! While no loaded bar has volume, the pane shows a notice that the symbol's data has no volume.
//!
//! # Interpretation
//! - CMF > 0: Buying pressure (accumulation)
//! - CMF < 0: Selling pressure (distribution)
//! - CMF near +1: Strong accumulation
//! - CMF near -1: Strong distribution
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ChaikinMoneyFlow, Indicator};
//!
//! let mut cmf = ChaikinMoneyFlow::new(20);
//! cmf.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyCell, StudyTable};
use egui::Color32;

/// Chaikin Money Flow indicator
#[derive(Clone, Debug)]
pub struct ChaikinMoneyFlow {
    period: usize,
    values: IndicatorValues,
    first_volume: Option<usize>,
    notice: Option<StudyTable>,
    positive_color: Color32,
    negative_color: Color32,
    visible: bool,
}

impl ChaikinMoneyFlow {
    /// Create a new CMF indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20 or 21)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            first_volume: None,
            notice: None,
            positive_color: palette::DARK_GREEN,
            negative_color: palette::RED,
            visible: true,
        }
    }

    /// Set colors for positive and negative values
    pub fn with_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.positive_color = positive;
        self.negative_color = negative;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ChaikinMoneyFlow {
    fn default() -> Self {
        Self::new(20)
    }
}

impl ChaikinMoneyFlow {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        // Calculate Money Flow Volume for each bar
        let mut mfv: Vec<f64> = Vec::with_capacity(data.len());
        let mut volumes: Vec<f64> = Vec::with_capacity(data.len());

        for bar in data {
            let high_low = bar.high - bar.low;

            if high_low > 0.0 {
                let mfm = (2.0 * bar.close - bar.high - bar.low) / high_low;
                mfv.push(mfm * bar.volume);
            } else {
                mfv.push(0.0);
            }
            volumes.push(bar.volume);
        }

        // Calculate CMF for each period
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let sum_mfv: f64 = mfv[start..=i].iter().sum();
            let sum_vol: f64 = volumes[start..=i].iter().sum();

            // Money flow over no volume is undefined, not balanced.
            values.push(if sum_vol > 0.0 {
                IndicatorValue::Single(sum_mfv / sum_vol)
            } else {
                IndicatorValue::None
            });
        }

        values
    }
}

impl Indicator for ChaikinMoneyFlow {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "CMF"
    }

    fn desc(&self) -> &str {
        "Chaikin Money Flow - Measures buying/selling pressure over period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        // A volume in the unchanged prefix still counts; otherwise search the changed bars.
        if self.first_volume.is_none_or(|at| at >= from) {
            self.first_volume = data[from..]
                .iter()
                .position(|bar| bar.volume.is_finite() && bar.volume > 0.0)
                .map(|at| at + from);
        }
        let wanted = !data.is_empty() && self.first_volume.is_none();
        if wanted != self.notice.is_some() {
            self.notice = wanted.then(|| StudyTable {
                title: "CMF".to_owned(),
                columns: vec!["Volume".to_owned()],
                rows: vec![vec![StudyCell::Text(
                    "This symbol's data has no volume".into(),
                )]],
                ..StudyTable::default()
            });
        }
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period - 1, |b| {
                b.high.is_finite()
                    && b.low.is_finite()
                    && b.close.is_finite()
                    && b.volume.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| {
                        b.high.is_finite()
                            && b.low.is_finite()
                            && b.close.is_finite()
                            && b.volume.is_finite()
                    },
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
    }

    fn table(&self) -> Option<&StudyTable> {
        self.notice.as_ref()
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.positive_color, self.negative_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.positive_color = colors[0];
        }
        if colors.len() > 1 {
            self.negative_color = colors[1];
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![(0.0, palette::GRAY, "Zero")],
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false // CMF is a separate oscillator pane
    }

    fn line_count(&self) -> usize {
        1
    }

    fn draws_only_plots(&self) -> bool {
        true
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
        vec![format!("CMF({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_range_bar;

    /// The notice describes the loaded history, including live corrections, rather than a
    /// single empty rolling window.
    #[test]
    fn history_without_volume_shows_a_notice() {
        use crate::studies::StudyCell;
        let mut data = vec![
            Bar {
                volume: 0.0,
                ..make_range_bar(12.0, 10.0, 11.5)
            };
            200
        ];
        let mut cmf = ChaikinMoneyFlow::default();
        cmf.calculate(&data);
        assert_eq!(
            cmf.table().unwrap().rows,
            [vec![StudyCell::Text(
                "This symbol's data has no volume".into()
            )]]
        );
        data.push(Bar {
            volume: 1000.0,
            ..data[0].clone()
        });
        cmf.update(&data, 200);
        assert_eq!(cmf.table(), None);
        data[200].volume = 0.0;
        cmf.update(&data, 200);
        assert!(cmf.table().is_some());
        cmf.calculate(&data[..200]);
        assert!(cmf.table().is_some());
        for volume in [f64::NAN, -1.0] {
            data[0].volume = volume;
            cmf.calculate(&data);
            assert!(cmf.table().is_some());
        }
        cmf.calculate(&[]);
        assert_eq!(cmf.table(), None);
    }

    /// Money flow over a window without volume is undefined, not balanced: 20 bars with volume, 20 without,
    /// then one with volume again.
    #[test]
    fn a_window_without_volume_has_no_value() {
        let bar = |close: f64, volume: f64| Bar {
            volume,
            ..make_range_bar(12.0, 10.0, close)
        };
        let data: Vec<Bar> = (0..41)
            .map(|i| match i {
                0..20 => bar(11.5, 1000.0),
                20..40 => bar(11.5, 0.0),
                _ => bar(10.5, 1000.0),
            })
            .collect();
        let mut cmf = ChaikinMoneyFlow::new(20);
        cmf.calculate(&data);
        assert_eq!(cmf.table(), None);
        let expected = |i: usize| match i {
            0..19 => None,
            19..39 => Some(0.5),
            39 => None,
            _ => Some(-0.5),
        };
        for i in 0..data.len() {
            assert_eq!(cmf.values().value(0, i), expected(i), "bar {i}");
        }
    }
}
