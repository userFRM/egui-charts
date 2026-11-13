use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// OHLC Volatility (Garman-Klass)
/// Uses OHLC data for more efficient volatility estimation
/// GK = 0.5 * ln(H/L)^2 - (2*ln(2)-1) * ln(C/O)^2
#[derive(Clone)]
pub struct OhlcVolatility {
    period: usize,
    /// Trading days per year the result is annualized over, one bar per day (252 for stocks, 365
    /// for crypto); bars shorter than a day need the bars in a year instead
    trading_days: f64,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl OhlcVolatility {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            trading_days: 252.0,
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(255, 82, 82)], // Violet
            visible: true,
        }
    }

    /// Set the bars per year the volatility is annualized over, clamped into 0 to
    /// `f64::MAX`, NaN to 0.
    pub fn with_trading_days(mut self, days: f64) -> Self {
        self.trading_days = super::clamp_param(days, 0.0, f64::MAX);
        self
    }
}

impl Default for OhlcVolatility {
    fn default() -> Self {
        Self::new(20)
    }
}

impl OhlcVolatility {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        let ln2 = 2.0_f64.ln();
        let coeff = 2.0 * ln2 - 1.0;

        // Calculate per-bar Garman-Klass values
        let mut gk_values = Vec::with_capacity(data.len());

        for bar in data {
            if bar.low > 0.0 && bar.open > 0.0 {
                let hl = (bar.high / bar.low).ln();
                let co = (bar.close / bar.open).ln();
                let gk = 0.5 * hl * hl - coeff * co * co;
                gk_values.push(gk);
            } else {
                gk_values.push(f64::NAN);
            }
        }

        // Rolling average over period, then sqrt and annualize
        for i in 0..data.len() {
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            let window = &gk_values[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.is_empty() {
                values.push(IndicatorValue::None);
                continue;
            }

            let avg = valid.iter().sum::<f64>() / valid.len() as f64;
            // sqrt of average GK, annualized, as percentage
            let vol = if avg >= 0.0 {
                avg.sqrt() * self.trading_days.sqrt() * 100.0
            } else {
                0.0
            };

            values.push(IndicatorValue::Single(vol));
        }

        values
    }
}

impl Indicator for OhlcVolatility {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        trading_days: number("Trading days per year", 0.0, f64::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "GK Vol"
    }

    fn desc(&self) -> &str {
        "Garman-Klass OHLC Volatility - Efficient volatility estimator using OHLC"
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
                b.open.is_finite() && b.high.is_finite() && b.low.is_finite() && b.close.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| {
                        b.open.is_finite()
                            && b.high.is_finite()
                            && b.low.is_finite()
                            && b.close.is_finite()
                    },
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
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.colors = colors;
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
        vec![format!("GK Vol({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four usable bars with open, high, low and close apart and a bar that closes at its open, an
    /// unusable bar between the second and the third, then the second bar's high and close
    /// corrected. Expected values come from the Garman-Klass definition over the last three
    /// usable bars, evaluated independently, annualized over 252 and 365 bars.
    #[test]
    fn ohlc_annualizes_over_the_bars_per_year_across_gaps_ties_and_corrections() {
        let bars = |second: (f64, f64, f64, f64)| -> Vec<Bar> {
            let bar = |(o, h, l, c)| Bar::new(chrono::DateTime::UNIX_EPOCH, o, h, l, c, 1.0);
            vec![
                bar((100.0, 104.0, 98.0, 103.0)),
                bar(second),
                bar((f64::NAN, 105.0, 99.0, 102.0)),
                bar((102.0, 105.0, 99.0, 102.0)),
                bar((102.0, 107.0, 100.0, 106.0)),
            ]
        };
        for (days, as_saved, corrected) in [
            (252.0, 68.93275599968585, 77.34860413688467),
            (365.0, 82.96058468746685, 93.08905948841814),
        ] {
            let mut study = OhlcVolatility::new(3).with_trading_days(days);
            study.calculate(&bars((103.0, 108.0, 101.0, 102.0)));
            assert_eq!(study.values().value(0, 2), None, "{days}: unusable bar");
            let got = study.values().value(0, 4).unwrap();
            assert!((got - as_saved).abs() < 1e-9, "{days}: {got}");
            study.update(&bars((103.0, 110.0, 100.0, 108.0)), 1);
            let got = study.values().value(0, 4).unwrap();
            assert!((got - corrected).abs() < 1e-9, "{days} corrected: {got}");
        }
    }
}
