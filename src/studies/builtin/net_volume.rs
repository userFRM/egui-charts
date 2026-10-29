use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Volume signed by the close against the previous close: positive, negative or zero.
/// The first bar has no value.
#[derive(Clone)]
pub struct NetVolume {
    values: IndicatorValues,
    stepper: Stepper<Option<f64>>,
    colors: Vec<Color32>,
    visible: bool,
}

impl NetVolume {
    /// Create with the default plot colors.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![palette::BLUE], // Teal
            visible: true,
        }
    }
}

impl Default for NetVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for NetVolume {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[0],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Line,
            visible: true,
        }
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Volume
    }

    fn name(&self) -> &str {
        "Net Volume"
    }

    fn desc(&self) -> &str {
        "Net Volume - Volume signed by the change from the previous close"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |previous, bar| {
                if !bar.close.is_finite() || !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                let value = match *previous {
                    None => IndicatorValue::None,
                    Some(close) => IndicatorValue::Single(if bar.close > close {
                        bar.volume
                    } else if bar.close < close {
                        -bar.volume
                    } else {
                        0.0
                    }),
                };
                *previous = Some(bar.close);
                value
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
        vec!["Net Volume".to_string()]
    }
}
