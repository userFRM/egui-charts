use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Volume Weighted Moving Average (VWMA): the closes of the last `period` bars weighted by their
/// volume. A window without volume has no value.
#[derive(Clone, Debug)]
pub struct VWMA {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl VWMA {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for VWMA {
    fn default() -> Self {
        Self::new(20)
    }
}

impl VWMA {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // VWMA = Sum(Price * Volume) / Sum(Volume)
                let price_volume_sum: f64 = window.iter().map(|bar| bar.close * bar.volume).sum();

                let volume_sum: f64 = window.iter().map(|bar| bar.volume).sum();

                // A window without volume has no volume-weighted average.
                values.push(if volume_sum > 0.0 {
                    IndicatorValue::Single(price_volume_sum / volume_sum)
                } else {
                    IndicatorValue::None
                });
            }
        }

        values
    }
}

impl Indicator for VWMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    fn offset_range(&self) -> std::ops::RangeInclusive<i32> {
        -500..=500
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "VWMA"
    }

    fn desc(&self) -> &str {
        "Volume Weighted Moving Average - MA weighted by volume"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period - 1, |b| {
                b.close.is_finite() && b.volume.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.close.is_finite() && b.volume.is_finite(),
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
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
        vec![format!("VWMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Length 3 over closes 10 to 90 with volumes 1, 1, 2, then four bars without volume,
    /// then 3 and 1: (10 + 20 + 60) / 4, (20 + 60) / 3, 60 / 2, no value while the window
    /// holds only bars without volume, then 240 / 3 and (240 + 90) / 4.
    #[test]
    fn a_window_without_volume_has_no_value() {
        let bars: Vec<_> = [1.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 1.0]
            .into_iter()
            .zip(1_i32..)
            .map(|(volume, i)| {
                let close = 10.0 * f64::from(i);
                let time = chrono::DateTime::from_timestamp(60 * i64::from(i), 0).unwrap();
                Bar::new(time, close, close, close, close, volume)
            })
            .collect();
        let mut study = VWMA::new(3);
        study.calculate(&bars);
        let want = [
            None,
            None,
            Some(22.5),
            Some(80.0 / 3.0),
            Some(30.0),
            None,
            None,
            Some(80.0),
            Some(82.5),
        ];
        assert_eq!(study.values().len(), want.len());
        for (bar, want) in want.into_iter().enumerate() {
            let got = study.values().value(0, bar);
            let close = match (got, want) {
                (Some(got), Some(want)) => (got - want).abs() < 1e-12,
                (got, want) => got.is_none() && want.is_none(),
            };
            assert!(close, "bar {bar}: {got:?} != {want:?}");
        }
    }
}
