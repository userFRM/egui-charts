use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Price Volume Trend (Enhanced) with Signal Line
/// PVT = prev_PVT + volume * (close - prev_close) / prev_close
/// Signal = EMA(PVT, signal_period)
#[derive(Clone)]
pub struct PriceVolumeTrendStudy {
    signal_period: usize,
    values: IndicatorValues,
    stepper: Stepper<(Option<f64>, f64, f64)>,
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceVolumeTrendStudy {
    /// Create with `signal_period` in bars, each clamped to at least one.
    pub fn new(signal_period: usize) -> Self {
        Self {
            signal_period: signal_period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::TEAL,   // PVT line
                palette::ORANGE, // Signal line
            ],
            visible: true,
        }
    }
}

impl Default for PriceVolumeTrendStudy {
    fn default() -> Self {
        Self::new(9)
    }
}

impl Indicator for PriceVolumeTrendStudy {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Volume
    }

    fn name(&self) -> &str {
        "PVT Enhanced"
    }

    fn desc(&self) -> &str {
        "Price Volume Trend Enhanced - PVT with EMA signal line"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() || !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                if let Some(previous) = state.0 {
                    if previous != 0.0 {
                        state.1 += bar.volume * (bar.close - previous) / previous;
                    }
                    state.2 =
                        (state.1 - state.2) * (2.0 / (self.signal_period as f64 + 1.0)) + state.2;
                }
                state.0 = Some(bar.close);
                IndicatorValue::Multiple(vec![state.1, state.2])
            });
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
        } else if !colors.is_empty() {
            self.colors = vec![colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn line_count(&self) -> usize {
        2
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
        vec![format!("PVT({})", self.signal_period), "Signal".to_string()]
    }
}
