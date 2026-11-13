use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Close-to-Close Volatility
/// Standard deviation of log returns, annualized by the square root of the bars per year (252
/// by default)
#[derive(Clone)]
pub struct CloseToCloseVol {
    period: usize,
    /// Trading days per year the result is annualized over, one bar per day (252 for stocks, 365
    /// for crypto); bars shorter than a day need the bars in a year instead
    trading_days: f64,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl CloseToCloseVol {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            trading_days: 252.0,
            values: IndicatorValues::new(),
            colors: vec![palette::LIGHT_BLUE], // Purple
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

impl Default for CloseToCloseVol {
    fn default() -> Self {
        Self::new(20)
    }
}

impl CloseToCloseVol {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < 2 {
            for _ in data {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate log returns
        let mut log_returns = Vec::with_capacity(data.len());
        log_returns.push(f64::NAN); // No return for first bar

        for i in 1..data.len() {
            if data[i - 1].close > 0.0 && data[i].close > 0.0 {
                log_returns.push((data[i].close / data[i - 1].close).ln());
            } else {
                log_returns.push(f64::NAN);
            }
        }

        // Rolling standard deviation of log returns
        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            let window = &log_returns[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.len() < 2 {
                values.push(IndicatorValue::None);
                continue;
            }

            let mean = valid.iter().sum::<f64>() / valid.len() as f64;
            let variance =
                valid.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (valid.len() - 1) as f64;
            let std_dev = variance.sqrt();

            // Annualize: std_dev * sqrt(trading_days) * 100 (as percentage)
            let annualized = std_dev * self.trading_days.sqrt() * 100.0;
            values.push(IndicatorValue::Single(annualized));
        }

        values
    }
}

impl Indicator for CloseToCloseVol {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        trading_days: number("Trading days per year", 0.0, f64::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "C2C Vol"
    }

    fn desc(&self) -> &str {
        "Close-to-Close Volatility - Annualized standard deviation of log returns"
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
            usable_behind(self.period, |b| b.close.is_finite()),
            |data| over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data)),
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
        vec![format!("C2C Vol({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_bar;

    /// Closes 100, 110, a bar without a close, 110 and 99 (a tie in the middle of the window), then
    /// the second close corrected to 105. Expected values come from the definition over the three
    /// returns of the usable bars, evaluated independently, annualized over 252 and 365 bars.
    #[test]
    fn c2c_annualizes_over_the_bars_per_year_across_gaps_ties_and_corrections() {
        let bars = |second: f64| -> Vec<Bar> {
            [100.0, second, f64::NAN, 110.0, 99.0]
                .map(make_bar)
                .to_vec()
        };
        for (days, as_saved, corrected) in [
            (252.0, 159.34400080811727, 140.25269412585286),
            (365.0, 191.7705346575994, 168.79414350887444),
        ] {
            let mut study = CloseToCloseVol::new(3).with_trading_days(days);
            let mut data = bars(110.0);
            study.calculate(&data);
            assert_eq!(study.values().value(0, 2), None, "{days}: bar without a close");
            let got = study.values().value(0, 4).unwrap();
            assert!((got - as_saved).abs() < 1e-9, "{days}: {got}");
            data = bars(105.0);
            study.update(&data, 1);
            let got = study.values().value(0, 4).unwrap();
            assert!((got - corrected).abs() < 1e-9, "{days} corrected: {got}");
        }
    }
}
