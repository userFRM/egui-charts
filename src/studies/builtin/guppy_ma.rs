use crate::model::Bar;
/// Guppy Multiple Moving Average (GMMA)
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

/// Short group periods: 3, 5, 8, 10, 12, 15
/// Long group periods: 30, 35, 40, 45, 50, 60
const SHORT_PERIODS: [usize; 6] = [3, 5, 8, 10, 12, 15];
const LONG_PERIODS: [usize; 6] = [30, 35, 40, 45, 50, 60];

#[derive(Clone)]
pub struct GuppyMA {
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl GuppyMA {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            colors: vec![
                // Short group - shades of green
                Color32::from_rgb(0, 200, 83),
                Color32::from_rgb(0, 180, 75),
                Color32::from_rgb(0, 160, 67),
                Color32::from_rgb(0, 140, 59),
                Color32::from_rgb(0, 120, 51),
                Color32::from_rgb(0, 100, 43),
                // Long group - shades of red
                Color32::from_rgb(239, 83, 80),
                Color32::from_rgb(220, 75, 72),
                Color32::from_rgb(200, 67, 64),
                Color32::from_rgb(180, 59, 56),
                Color32::from_rgb(160, 51, 48),
                Color32::from_rgb(140, 43, 40),
            ],
            visible: true,
        }
    }
}

impl Default for GuppyMA {
    fn default() -> Self {
        Self::new()
    }
}

fn calc_ema_series(data: &[Bar], period: usize) -> Vec<f64> {
    let mut result = Vec::with_capacity(data.len());
    if data.is_empty() {
        return result;
    }

    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = data[0].close;

    for bar in data {
        ema = (bar.close - ema) * multiplier + ema;
        result.push(ema);
    }
    result
}

impl Indicator for GuppyMA {
    fn name(&self) -> &str {
        "GMMA"
    }

    fn desc(&self) -> &str {
        "Guppy Multiple Moving Average - 12 EMAs in short and long groups"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Calculate all 12 EMA series
        let short_emas: Vec<Vec<f64>> = SHORT_PERIODS
            .iter()
            .map(|&p| calc_ema_series(data, p))
            .collect();
        let long_emas: Vec<Vec<f64>> = LONG_PERIODS
            .iter()
            .map(|&p| calc_ema_series(data, p))
            .collect();

        for i in 0..data.len() {
            let mut vals = Vec::with_capacity(12);
            for series in &short_emas {
                vals.push(series[i]);
            }
            for series in &long_emas {
                vals.push(series[i]);
            }
            self.values.push(IndicatorValue::Multiple(vals));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 12 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        12
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
        let mut names = Vec::with_capacity(12);
        for &p in &SHORT_PERIODS {
            names.push(format!("EMA({p})"));
        }
        for &p in &LONG_PERIODS {
            names.push(format!("EMA({p})"));
        }
        names
    }
}
