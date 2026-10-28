use crate::model::math::Moments;
use crate::model::{Bar, PriceSource};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Trend Strength Index: the Pearson correlation of the source price with bar position over
/// the last `period` bars, from -1 for a straight fall to 1 for a straight rise.
///
/// A bar has no value until `period` bars exist, when the window holds a non-finite price, or
/// when the prices of the window are all equal, as the correlation is then undefined. A bar
/// with a non-finite price still takes its place in the window, so the correlation is always
/// taken against true bar positions. The latest value equals
/// [`LinearRegressionChannel::correlation`](super::LinearRegressionChannel::correlation) of
/// the same period.
#[derive(Clone)]
pub struct TrendStrengthIndex {
    period: usize,
    source: PriceSource,
    values: IndicatorValues,
    stepper: Stepper<VecDeque<Bar>>,
    color: Color32,
    visible: bool,
}

impl TrendStrengthIndex {
    /// Create with a window of `period` bars, clamped to at least 2.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            source: PriceSource::Close,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// Select the input price. Defaults to close.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for TrendStrengthIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for TrendStrengthIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(self.source))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 2, usize::MAX),
        source: choice("Source", PriceSource::all())
    ; {});

    fn name(&self) -> &str {
        "Trend Strength Index"
    }

    fn desc(&self) -> &str {
        "Trend Strength Index - Rolling correlation of price with bar position (-1 to 1)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, source) = (self.period, self.source);
        self.stepper
            .run(data, from, &mut self.values, |window, bar| {
                let price = source.compute(bar.open, bar.high, bar.low, bar.close);
                window.push_back(Bar::new(bar.time, price, price, price, price, 0.0));
                if window.len() > period {
                    window.pop_front();
                }
                if window.len() < period || window.iter().any(|bar| !bar.close.is_finite()) {
                    return IndicatorValue::None;
                }
                Moments::of(window.make_contiguous())
                    .fit()
                    .and_then(|fit| fit.correlation)
                    .map_or(IndicatorValue::None, IndicatorValue::Single)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(3)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            range: Some((-1.0, 1.0)),
            levels: vec![(0.0, Color32::from_gray(128), "Zero")],
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false
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
        vec![format!("Trend Strength({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    fn rolling(closes: &[f64], period: usize) -> Vec<Option<f64>> {
        let mut tsi = TrendStrengthIndex::new(period);
        tsi.calculate(&bars(closes));
        tsi.values()
            .to_rows()
            .into_iter()
            .map(|v| match v {
                IndicatorValue::Single(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Period 3 over 1, 2, 4, 3, 3, 3 against positions 0, 1, 2, from the centred sums
    /// r = sxy / sqrt(sxx * syy), with sxx = 2: 1, 2, 4 has sxy 3 and syy 14/3; 2, 4, 3 has
    /// sxy 1 and syy 2; 4, 3, 3 has sxy -1 and syy 2/3; 3, 3, 3 is flat. A straight fall is -1.
    #[test]
    fn rolling_correlation_matches_hand_values() {
        let got = rolling(&[1.0, 2.0, 4.0, 3.0, 3.0, 3.0], 3);
        let want = [
            None,
            None,
            Some(3.0 / (2.0 * 14.0 / 3.0_f64).sqrt()),
            Some(1.0 / (2.0 * 2.0_f64).sqrt()),
            Some(-1.0 / (2.0 * 2.0 / 3.0_f64).sqrt()),
            None,
        ];
        for (got, want) in got.iter().zip(want) {
            match (got, want) {
                (Some(got), Some(want)) => assert!((got - want).abs() < 1e-12, "{got} {want}"),
                _ => assert_eq!(*got, want),
            }
        }
        assert_eq!(rolling(&[5.0, 4.0, 3.0, 2.0], 4)[3], Some(-1.0));
    }

    /// A missing price blanks every window that holds it and keeps later bar positions true.
    #[test]
    fn missing_price_blanks_its_windows() {
        let got = rolling(&[1.0, 2.0, f64::NAN, 4.0, 5.0, 6.0], 3);
        assert_eq!(got[..5], [None; 5]);
        assert!((got[5].unwrap() - 1.0).abs() < 1e-12);
    }
}
