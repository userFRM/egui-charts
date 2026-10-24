use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Volume signed by the close against the previous close: positive, negative or zero.
/// The first bar has no value.
#[derive(Clone)]
pub struct NetVolume {
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl NetVolume {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(38, 166, 154)], // Teal
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
    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Volume
    }

    fn name(&self) -> &str {
        "Net Volume"
    }

    fn desc(&self) -> &str {
        "Net Volume - Volume signed by the change from the previous close"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.close.is_finite() && b.volume.is_finite(),
            |data| {
                let mut previous = None;
                data.iter()
                    .map(|bar| {
                        let value = match previous {
                            None => IndicatorValue::None,
                            Some(close) => IndicatorValue::Single(if bar.close > close {
                                bar.volume
                            } else if bar.close < close {
                                -bar.volume
                            } else {
                                0.0
                            }),
                        };
                        previous = Some(bar.close);
                        value
                    })
                    .collect()
            },
        );
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
