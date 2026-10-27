use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Short group periods: 3, 5, 8, 10, 12, 15
/// Long group periods: 30, 35, 40, 45, 50, 60
const SHORT_PERIODS: [usize; 6] = [3, 5, 8, 10, 12, 15];
const LONG_PERIODS: [usize; 6] = [30, 35, 40, 45, 50, 60];

/// Guppy Multiple Moving Average (GMMA)
#[derive(Clone)]
pub struct GuppyMA {
    values: IndicatorValues,
    stepper: Stepper<Option<[f64; 12]>>,
    colors: Vec<Color32>,
    visible: bool,
}

impl GuppyMA {
    /// Create the six short and six long exponential moving averages.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
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

impl Indicator for GuppyMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    fn name(&self) -> &str {
        "GMMA"
    }

    fn desc(&self) -> &str {
        "Guppy Multiple Moving Average - 12 EMAs in short and long groups"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let averages = state.get_or_insert([bar.close; 12]);
                for (average, period) in averages
                    .iter_mut()
                    .zip(SHORT_PERIODS.into_iter().chain(LONG_PERIODS))
                {
                    *average = (bar.close - *average) * (2.0 / (period as f64 + 1.0)) + *average;
                }
                IndicatorValue::Multiple(averages.to_vec())
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
        if colors.len() >= 12 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_count(&self) -> usize {
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
