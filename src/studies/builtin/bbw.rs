use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use crate::studies::{helpers, palette};
use egui::Color32;

/// Bollinger Bands Width - measures how wide the Bollinger Bands are.
/// Formula: (upper_band - lower_band) / middle_band * 100
/// where middle = SMA(close, period), upper = middle + std_dev * stdev,
/// lower = middle - std_dev * stdev.
///
/// Outputs are the width, the highest width of the last `expansion` bars (highest
/// expansion) and the lowest width of the last `contraction` bars (lowest contraction),
/// each window ending at the current bar. The width starts at bar `period - 1` and both
/// extrema start with it. A bar with a non-finite close is skipped as if absent: it has no
/// value and is in no window, so later values equal those of the series without it.
#[derive(Clone)]
pub struct BollingerBandsWidth {
    period: usize,
    std_dev: f64,
    expansion: usize,
    contraction: usize,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl BollingerBandsWidth {
    /// `std_dev` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, std_dev: f64) -> Self {
        Self {
            period: period.max(1),
            std_dev: clamp_param(std_dev, 0.0, f64::MAX),
            expansion: 125,
            contraction: 125,
            values: IndicatorValues::new(),
            colors: vec![
                palette::BLUE,
                palette::RED.gamma_multiply_u8(128),
                palette::TEAL.gamma_multiply_u8(128),
            ],
            visible: true,
        }
    }

    /// Set the highest expansion and lowest contraction lookbacks, each at least 1 bar.
    pub fn with_lookbacks(mut self, expansion: usize, contraction: usize) -> Self {
        self.expansion = expansion.max(1);
        self.contraction = contraction.max(1);
        self
    }
}

impl Default for BollingerBandsWidth {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl BollingerBandsWidth {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut widths = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i + 1 < self.period {
                widths.push(f64::NAN);
                continue;
            }

            let start = i + 1 - self.period;
            let prices: Vec<f64> = data[start..=i].iter().map(|b| b.close).collect();

            let sma = prices.iter().sum::<f64>() / self.period as f64;
            let variance =
                prices.iter().map(|p| (p - sma).powi(2)).sum::<f64>() / self.period as f64;
            let stdev = variance.sqrt();

            let upper = sma + self.std_dev * stdev;
            let lower = sma - self.std_dev * stdev;

            widths.push(if sma == 0.0 {
                0.0
            } else {
                (upper - lower) / sma * 100.0
            });
        }
        let highest = helpers::highest(widths.iter().copied(), self.expansion);
        let lowest = helpers::lowest(widths.iter().copied(), self.contraction);
        let mut values = IndicatorValues::new();
        for ((width, highest), lowest) in widths.into_iter().zip(highest).zip(lowest) {
            values.push_lines(&[width, highest, lowest]);
        }
        values
    }
}

impl Indicator for BollingerBandsWidth {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        std_dev: number("Std dev", 0.0, f64::MAX),
        expansion: length("Highest expansion lookback", 1, usize::MAX),
        contraction: length("Lowest contraction lookback", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "BBW"
    }

    fn desc(&self) -> &str {
        "Bollinger Bands Width - Band width as percentage of middle band"
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
            usable_behind(
                (self.period - 1).saturating_add(self.expansion.max(self.contraction) - 1),
                |b| b.close.is_finite(),
            ),
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
        for (slot, color) in self.colors.iter_mut().zip(colors) {
            *slot = color;
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
        vec![
            format!("BBW({}, {})", self.period, self.std_dev),
            format!("Highest Expansion({})", self.expansion),
            format!("Lowest Contraction({})", self.contraction),
        ]
    }
}
