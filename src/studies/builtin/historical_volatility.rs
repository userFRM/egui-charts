use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Historical Volatility (HV)
/// Measures the annualized standard deviation of log returns
/// Used for options pricing and risk assessment
#[derive(Clone)]
pub struct HistoricalVolatility {
    /// Lookback period (typically 20 or 30)
    period: usize,
    /// Trading days per year (252 for stocks, 365 for crypto)
    trading_days: f64,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl HistoricalVolatility {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            trading_days: 252.0, // Default for stocks
            values: IndicatorValues::new(),
            color: palette::BLUE, // Deep Orange
            visible: true,
        }
    }

    /// Set the bars per year the volatility is annualized over, clamped into 0 to
    /// `f64::MAX`, NaN to 0.
    pub fn with_trading_days(mut self, days: f64) -> Self {
        self.trading_days = clamp_param(days, 0.0, f64::MAX);
        self
    }

    /// Annualize over 365 bars instead of the default 252.
    pub fn for_crypto(mut self) -> Self {
        self.trading_days = 365.0;
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for HistoricalVolatility {
    fn default() -> Self {
        Self::new(20)
    }
}

impl HistoricalVolatility {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate log returns
        let mut log_returns = Vec::with_capacity(data.len());
        log_returns.push(f64::NAN); // First bar has no return

        for i in 1..data.len() {
            if data[i - 1].close <= 0.0 || data[i].close <= 0.0 {
                log_returns.push(f64::NAN);
            } else {
                let ret = (data[i].close / data[i - 1].close).ln();
                log_returns.push(ret);
            }
        }

        // Calculate rolling standard deviation
        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
            } else {
                let window = &log_returns[i + 1 - self.period..=i];

                // Check for NaN values
                let valid_returns: Vec<f64> =
                    window.iter().filter(|&&x| !x.is_nan()).copied().collect();

                if valid_returns.len() < 2 {
                    values.push(IndicatorValue::None);
                    continue;
                }

                // Calculate mean
                let mean = valid_returns.iter().sum::<f64>() / valid_returns.len() as f64;

                // Calculate variance
                let variance = valid_returns
                    .iter()
                    .map(|&x| (x - mean).powi(2))
                    .sum::<f64>()
                    / (valid_returns.len() - 1) as f64;

                // Standard deviation
                let std_dev = variance.sqrt();

                // Annualize: multiply by sqrt(trading_days) and convert to percentage
                let annualized_hv = std_dev * self.trading_days.sqrt() * 100.0;

                values.push(IndicatorValue::Single(annualized_hv));
            }
        }

        values
    }
}

impl Indicator for HistoricalVolatility {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        trading_days: number("Trading days", 0.0, f64::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "HV"
    }

    fn desc(&self) -> &str {
        "Historical Volatility - Annualized standard deviation of returns"
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
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
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
        vec![format!("HV({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_hv_calculation() {
        let mut hv = HistoricalVolatility::new(5);

        // Prices with some volatility
        let data = vec![
            make_bar(100.0),
            make_bar(102.0),
            make_bar(99.0),
            make_bar(103.0),
            make_bar(98.0),
            make_bar(105.0),
            make_bar(101.0),
        ];

        hv.calculate(&data);

        assert_eq!(hv.values.len(), 7);

        // First values should be None
        for i in 0..5 {
            assert!(matches!(hv.values.to_rows()[i], IndicatorValue::None));
        }

        // Should have positive volatility
        let IndicatorValue::Single(v) = hv.values.to_rows()[6] else {
            panic!("no value: {:?}", hv.values.to_rows()[6]);
        };
        assert!(v > 0.0, "HV should be positive");
    }

    #[test]
    fn test_hv_no_volatility() {
        let mut hv = HistoricalVolatility::new(3);

        // Constant prices = no volatility
        let data = vec![
            make_bar(100.0),
            make_bar(100.0),
            make_bar(100.0),
            make_bar(100.0),
            make_bar(100.0),
        ];

        hv.calculate(&data);

        // With constant prices, HV should be 0
        let rows = hv.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", hv.values.to_rows().last().unwrap());
        };
        assert!(v.abs() < 0.01, "HV should be ~0 for constant prices");
    }

    #[test]
    fn test_hv_high_volatility() {
        let mut hv = HistoricalVolatility::new(3);

        // Large price swings
        let data = vec![
            make_bar(100.0),
            make_bar(120.0),
            make_bar(80.0),
            make_bar(130.0),
            make_bar(70.0),
        ];

        hv.calculate(&data);

        // High volatility should produce large HV values
        let rows = hv.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", hv.values.to_rows().last().unwrap());
        };
        assert!(*v > 100.0, "HV should be high for volatile prices, got {v}");
    }
}
