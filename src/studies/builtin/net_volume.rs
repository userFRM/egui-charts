use crate::model::Bar;
/// Net Volume
/// Shows volume as positive when close > open, negative when close < open
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct NetVolume {
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl NetVolume {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
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
    fn name(&self) -> &str {
        "Net Volume"
    }

    fn desc(&self) -> &str {
        "Net Volume - Positive on up bars, negative on down bars"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let net = if bar.close > bar.open {
                bar.volume
            } else if bar.close < bar.open {
                -bar.volume
            } else {
                0.0
            };
            self.values.push(IndicatorValue::Single(net));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
