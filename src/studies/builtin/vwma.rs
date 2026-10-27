use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Volume Weighted Moving Avg (VWMA)
/// Moving avg weighted by volume - gives more importance to high-volume periods
#[derive(Clone)]
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
            color: palette::GREEN, // Green
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

                let vwma = if volume_sum > 0.0 {
                    price_volume_sum / volume_sum
                } else {
                    // Fall back to simple avg if no volume
                    window.iter().map(|bar| bar.close).sum::<f64>() / self.period as f64
                };

                values.push(IndicatorValue::Single(vwma));
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

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "VWMA"
    }

    fn desc(&self) -> &str {
        "Volume Weighted Moving Avg - MA weighted by volume"
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
