use super::clamp_param;
use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Moving Average Envelopes
#[derive(Clone)]
pub struct Envelopes {
    period: usize,
    percent: f64,
    ma_type: String,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    ema: Smoothing,
    window: std::collections::VecDeque<f64>,
}

impl Envelopes {
    /// `percent` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, percent: f64, ma_type: &str) -> Self {
        Self {
            period: period.max(1),
            percent: clamp_param(percent, 0.0, f64::MAX),
            ma_type: ma_type.to_string(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::GREEN, // Green - Upper
                palette::BLUE,  // Blue - Middle
                palette::RED,   // Red - Lower
            ],
            visible: true,
        }
    }
}

impl Default for Envelopes {
    fn default() -> Self {
        Self::new(20, 2.5, "SMA")
    }
}

impl Indicator for Envelopes {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        percent: number("Percent", 0.0, f64::MAX),
        ma_type: choice("Average", ["SMA".to_owned(), "EMA".to_owned()])
    ; {});

    fn name(&self) -> &str {
        "Envelopes"
    }

    fn desc(&self) -> &str {
        "Moving Average Envelopes - Percentage bands around a moving average"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let ma = if self.ma_type == "EMA" {
                    state
                        .ema
                        .next(bar.close, 1, 2.0 / (self.period as f64 + 1.0))
                } else {
                    state.window.push_back(bar.close);
                    if state.window.len() > self.period {
                        state.window.pop_front();
                    }
                    if state.window.len() < self.period
                        || state.window.iter().any(|v| !v.is_finite())
                    {
                        f64::NAN
                    } else {
                        state.window.iter().sum::<f64>() / self.period as f64
                    }
                };
                let multiplier = self.percent / 100.0;
                IndicatorValue::Multiple(vec![ma * (1.0 + multiplier), ma, ma * (1.0 - multiplier)])
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
        if colors.len() >= 3 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_count(&self) -> usize {
        3
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
            format!("Upper(+{}%)", self.percent),
            format!("{}({})", self.ma_type, self.period),
            format!("Lower(-{}%)", self.percent),
        ]
    }
}
