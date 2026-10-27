use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{rolling_mean, update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Relative Vigor Index (RVI)
/// Measures conviction of price movement
/// Uses the relationship between close-open vs high-low
#[derive(Clone)]
pub struct RelativeVigorIndex {
    /// Period for smoothing (typically 10)
    period: usize,
    values: IndicatorValues,
    /// Colors: [RVI, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl RelativeVigorIndex {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![
                palette::GREEN, // Green - RVI
                palette::RED,   // Red - Signal
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    fn swma(data: &[f64]) -> Vec<f64> {
        // Symmetrically weighted moving avg (weights: 1, 2, 2, 1)
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < 3 {
                result.push(f64::NAN);
            } else {
                let swma = (data[i - 3] + 2.0 * data[i - 2] + 2.0 * data[i - 1] + data[i]) / 6.0;
                result.push(swma);
            }
        }

        result
    }
}

impl Default for RelativeVigorIndex {
    fn default() -> Self {
        Self::new(10)
    }
}

impl RelativeVigorIndex {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        // Calculate (Close - Open) for numerator
        let close_open: Vec<f64> = data.iter().map(|b| b.close - b.open).collect();

        // Calculate (High - Low) for denominator
        let high_low: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();

        // Apply SWMA smoothing
        let smoothed_num = Self::swma(&close_open);
        let smoothed_den = Self::swma(&high_low);

        // Calculate running sums over period
        let num_sum = rolling_mean(&smoothed_num, self.period);
        let den_sum = rolling_mean(&smoothed_den, self.period);

        // Calculate RVI
        let mut rvi = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if num_sum[i].is_nan() || den_sum[i].is_nan() || den_sum[i] == 0.0 {
                rvi.push(f64::NAN);
            } else {
                rvi.push(num_sum[i] / den_sum[i]);
            }
        }

        // Calculate signal line (SWMA of RVI)
        let signal = Self::swma(&rvi);

        // Store results
        for i in 0..data.len() {
            if rvi[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[rvi[i], signal[i]]);
            }
        }

        values
    }
}

impl Indicator for RelativeVigorIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "RVI"
    }

    fn desc(&self) -> &str {
        "Relative Vigor Index - Price movement conviction"
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
            usable_behind(self.period.saturating_add(5), |b| {
                [b.open, b.high, b.low, b.close]
                    .iter()
                    .all(|x| x.is_finite())
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| {
                        [b.open, b.high, b.low, b.close]
                            .iter()
                            .all(|x| x.is_finite())
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
        if colors.len() >= 2 {
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
        vec![format!("RVI({})", self.period), "Signal".to_string()]
    }
}
