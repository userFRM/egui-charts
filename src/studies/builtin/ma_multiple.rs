use crate::model::Bar;
/// Multiple Moving Averages (SMA 20, 50, 100, 200)
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

const PERIODS: [usize; 4] = [20, 50, 100, 200];

#[derive(Clone)]
pub struct MultipleMA {
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl MultipleMA {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            colors: vec![
                Color32::from_rgb(41, 98, 255),  // Blue - SMA 20
                Color32::from_rgb(255, 152, 0),  // Orange - SMA 50
                Color32::from_rgb(239, 83, 80),  // Red - SMA 100
                Color32::from_rgb(156, 39, 176), // Purple - SMA 200
            ],
            visible: true,
        }
    }
}

impl Default for MultipleMA {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for MultipleMA {
    fn name(&self) -> &str {
        "Multi MA"
    }

    fn desc(&self) -> &str {
        "Multiple Moving Averages - SMA 20, 50, 100, 200"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            let mut vals = Vec::with_capacity(4);

            for &period in &PERIODS {
                if i + 1 >= period {
                    let start = i + 1 - period;
                    let sma: f64 =
                        data[start..=i].iter().map(|b| b.close).sum::<f64>() / period as f64;
                    vals.push(sma);
                } else {
                    vals.push(f64::NAN);
                }
            }

            // If all are NaN, push None; otherwise push Multiple
            if vals.iter().all(|v| v.is_nan()) {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Multiple(vals));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 4 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        4
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
        PERIODS.iter().map(|p| format!("SMA({})", p)).collect()
    }
}
