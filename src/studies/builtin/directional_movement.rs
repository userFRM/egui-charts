use super::ADX;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Directional Movement Index (DMI / DI+ / DI-)
/// Shows the positive and negative directional indicators and the ADX, Wilder's average of DX
/// over the ADX smoothing period (Wilder 1978)
///
/// The `period` and `adx_smoothing` inputs are independent; setting either leaves the other.
/// A template saved before the smoothing had an input of its own restores it equal to `period`.
/// Values read to four decimals.
#[derive(Clone, Debug)]
pub struct DirectionalMovement {
    /// Period of the directional indicators (typically 14)
    period: usize,
    /// Length of Wilder's average of DX that forms the ADX line.
    adx_smoothing: usize,
    values: IndicatorValues,
    stepper: Stepper<usize>,
    adx: ADX,
    /// Colors: [+DI, -DI, ADX]
    colors: Vec<Color32>,
    visible: bool,
}

impl DirectionalMovement {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            adx_smoothing: period.max(1),
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

    /// Set the length of Wilder's average of DX that forms the ADX line, clamped to at least one.
    /// `new` sets it to the DI period.
    pub fn with_smoothing(mut self, smoothing: usize) -> Self {
        self.adx_smoothing = smoothing.max(1);
        self.adx = ADX::new(self.period).with_smoothing(self.adx_smoothing);
        self
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
        period: length("DI period", 1, usize::MAX),
        adx_smoothing: length("ADX smoothing period", 1, usize::MAX)
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
            self.adx = ADX::new(self.period).with_smoothing(self.adx_smoothing);
        }
        self.adx.update(data, from);
        self.stepper.run(data, from, &mut self.values, |index, _| {
            let at = *index;
            *index += 1;
            // The inner ADX draws ADX, +DI and -DI.
            let [plus, minus, adx] =
                [1, 2, 0].map(|line| self.adx.values().value(line, at).unwrap_or(f64::NAN));
            if plus.is_nan() {
                IndicatorValue::None
            } else {
                IndicatorValue::Multiple(vec![plus, minus, adx])
            }
        });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(4)
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

    fn draws_only_plots(&self) -> bool {
        true
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
            if self.adx_smoothing == self.period {
                format!("ADX({})", self.period)
            } else {
                format!("ADX({},{})", self.period, self.adx_smoothing)
            },
        ]
    }
}
