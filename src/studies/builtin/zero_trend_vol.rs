use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Zero-mean close-to-close volatility, without subtracting the mean log return.
///
/// `sqrt(mean(ln(close / previous_close)^2)) * sqrt(trading_days) * 100`.
/// Annualisation takes the bars per year, 252 by default (daily bars), from the trading days
/// input.
/// A full `period` of returns from positive closes is required before a value exists.
#[derive(Clone)]
pub struct ZeroTrendVol {
    period: usize,
    /// Bars per year the result is annualized over (252 for stocks, 365 for crypto)
    trading_days: f64,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl ZeroTrendVol {
    /// Create a zero-trend study over `period` log returns (at least 1).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            trading_days: 252.0,
            values: IndicatorValues::new(),
            colors: vec![palette::LIGHT_BLUE],
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

impl Default for ZeroTrendVol {
    fn default() -> Self {
        Self::new(20)
    }
}

impl ZeroTrendVol {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let returns: Vec<_> = data
            .windows(2)
            .map(|bars| {
                if bars[0].close > 0.0 && bars[1].close > 0.0 {
                    (bars[1].close / bars[0].close).ln()
                } else {
                    f64::NAN
                }
            })
            .collect();
        (0..data.len())
            .map(|i| {
                if i < self.period {
                    return IndicatorValue::None;
                }
                let window = &returns[i - self.period..i];
                if window.iter().any(|r| !r.is_finite()) {
                    return IndicatorValue::None;
                }
                let mean_squared = window.iter().map(|r| r * r).sum::<f64>() / self.period as f64;
                IndicatorValue::Single(mean_squared.sqrt() * self.trading_days.sqrt() * 100.0)
            })
            .collect()
    }
}

impl Indicator for ZeroTrendVol {
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
        "Zero-Trend Vol"
    }

    fn desc(&self) -> &str {
        "Zero-Trend Close-to-Close Volatility - Annualised percentage"
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
        vec![format!("Zero-Trend Vol({})", self.period)]
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
    fn zero_trend_annualizes_over_the_bars_per_year_across_gaps_ties_and_corrections() {
        let bars = |second: f64| -> Vec<Bar> {
            [100.0, second, f64::NAN, 110.0, 99.0]
                .map(make_bar)
                .to_vec()
        };
        for (days, as_saved, corrected) in [
            (252.0, 130.21247886655595, 114.63926638193587),
            (365.0, 156.7107425738662, 137.96837844741043),
        ] {
            let mut study = ZeroTrendVol::new(3).with_trading_days(days);
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
