use crate::model::Bar;
/// Price Volume Trend (Enhanced) with Signal Line
/// PVT = prev_PVT + volume * (close - prev_close) / prev_close
/// Signal = EMA(PVT, signal_period)
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct PriceVolumeTrendStudy {
    signal_period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceVolumeTrendStudy {
    pub fn new(signal_period: usize) -> Self {
        Self {
            signal_period,
            values: Vec::new(),
            colors: vec![
                Color32::from_rgb(38, 166, 154), // PVT line (teal)
                Color32::from_rgb(255, 152, 0),  // Signal line (orange)
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
    fn name(&self) -> &str {
        "PVT Enhanced"
    }

    fn desc(&self) -> &str {
        "Price Volume Trend Enhanced - PVT with EMA signal line"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Calculate PVT series
        let mut pvt_values = Vec::with_capacity(data.len());
        let mut pvt = 0.0_f64;
        pvt_values.push(pvt);

        for i in 1..data.len() {
            let prev_close = data[i - 1].close;
            if prev_close != 0.0 {
                pvt += data[i].volume * (data[i].close - prev_close) / prev_close;
            }
            pvt_values.push(pvt);
        }

        // Calculate signal line (EMA of PVT)
        let mult = 2.0 / (self.signal_period as f64 + 1.0);
        let mut signal = pvt_values[0];
        let mut signal_values = Vec::with_capacity(data.len());
        signal_values.push(signal);

        for i in 1..pvt_values.len() {
            signal = (pvt_values[i] - signal) * mult + signal;
            signal_values.push(signal);
        }

        // Output both lines
        for i in 0..data.len() {
            self.values.push(IndicatorValue::Multiple(vec![
                pvt_values[i],
                signal_values[i],
            ]));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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

    fn line_cnt(&self) -> usize {
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
