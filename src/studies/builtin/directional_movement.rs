use super::ADX;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Directional Movement Index (DMI / DI+ / DI-)
/// Shows the positive and negative directional indicators and the ADX
/// of the same length (Wilder 1978)
#[derive(Clone)]
pub struct DirectionalMovement {
    /// Period for smoothing (typically 14)
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    adx: ADX,
    /// Colors: [+DI, -DI, ADX]
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    index: usize,
    previous: Option<[f64; 3]>,
    count: usize,
    plus: f64,
    minus: f64,
    tr: f64,
}

impl DirectionalMovement {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            adx: ADX::new(period),
            colors: vec![
                palette::BLUE,                 // +DI
                palette::ORANGE,               // -DI
                Color32::from_rgb(245, 0, 87), // ADX
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for DirectionalMovement {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for DirectionalMovement {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "DMI"
    }

    fn desc(&self) -> &str {
        "Directional Movement Index - +DI, -DI and ADX lines"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.adx = ADX::new(self.period);
        }
        self.adx.update(data, from);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.index;
                state.index += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                let Some(previous) = state.previous.replace([bar.high, bar.low, bar.close]) else {
                    return IndicatorValue::None;
                };
                state.count += 1;
                let up = bar.high - previous[0];
                let down = previous[1] - bar.low;
                let plus = if up > down && up > 0.0 { up } else { 0.0 };
                let minus = if down > up && down > 0.0 { down } else { 0.0 };
                let tr = (bar.high - bar.low)
                    .max((bar.high - previous[2]).abs())
                    .max((bar.low - previous[2]).abs());
                if state.count <= self.period {
                    state.plus += plus;
                    state.minus += minus;
                    state.tr += tr;
                    if state.count < self.period {
                        return IndicatorValue::None;
                    }
                } else {
                    state.plus = state.plus - state.plus / self.period as f64 + plus;
                    state.minus = state.minus - state.minus / self.period as f64 + minus;
                    state.tr = state.tr - state.tr / self.period as f64 + tr;
                }
                let adx = self.adx.values().value(0, index).unwrap_or(f64::NAN);
                if state.tr.is_nan() || state.tr == 0.0 {
                    if adx.is_nan() {
                        IndicatorValue::None
                    } else {
                        IndicatorValue::Multiple(vec![f64::NAN, f64::NAN, adx])
                    }
                } else {
                    IndicatorValue::Multiple(vec![
                        100.0 * state.plus / state.tr,
                        100.0 * state.minus / state.tr,
                        adx,
                    ])
                }
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
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
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
            format!("+DI({})", self.period),
            format!("-DI({})", self.period),
            format!("ADX({})", self.period),
        ]
    }
}
