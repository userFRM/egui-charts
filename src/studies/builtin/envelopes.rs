use super::clamp_param;
use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// The moving average in the middle of [`Envelopes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeAverage {
    /// Arithmetic mean of a complete window, named `SMA`.
    Simple,
    /// Exponential smoothing with alpha 2/(period+1), seeded by the first price, named `EMA`.
    Exponential,
}

impl EnvelopeAverage {
    /// The abbreviation in the middle line's name.
    fn label(self) -> &'static str {
        match self {
            Self::Simple => "SMA",
            Self::Exponential => "EMA",
        }
    }
}

/// Moving Average Envelopes. The band between the upper and lower lines is filled.
#[derive(Clone)]
pub struct Envelopes {
    period: usize,
    percent: f64,
    ma_type: EnvelopeAverage,
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
    /// Bands `percent` above and below a `ma_type` average of `period` closes. `percent` is
    /// clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, percent: f64, ma_type: EnvelopeAverage) -> Self {
        Self {
            period: period.max(1),
            percent: clamp_param(percent, 0.0, f64::MAX),
            ma_type,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,                       // Upper
                palette::ORANGE,                     // Middle
                palette::BLUE,                       // Lower
                palette::BLUE.gamma_multiply_u8(13), // Fill
            ],
            visible: true,
        }
    }
}

impl Default for Envelopes {
    fn default() -> Self {
        Self::new(20, 10.0, EnvelopeAverage::Simple)
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
        ma_type: choice("Average", [EnvelopeAverage::Simple, EnvelopeAverage::Exponential])
    ; {});

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Plot(2),
            color: self.colors[3],
            inverted_color: None,
        }]
    }

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
                let ma = if self.ma_type == EnvelopeAverage::Exponential {
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
            let n = colors.len().min(self.colors.len());
            self.colors[..n].copy_from_slice(&colors[..n]);
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
            format!("{}({})", self.ma_type.label(), self.period),
            format!("Lower(-{}%)", self.percent),
        ]
    }
}
