//! Awesome Oscillator (AO) Indicator
//!
//! Developed by Bill Williams, the Awesome Oscillator measures market momentum
//! by comparing recent market momentum to a larger time frame.
//!
//! # Formula
//! AO = SMA(median_price, 5) - SMA(median_price, 34)
//! Where median_price = (high + low) / 2
//!
//! # Interpretation
//! - AO > 0: Bullish momentum
//! - AO < 0: Bearish momentum
//! - Zero line crossovers signal momentum shifts
//! - Twin peaks (divergence) signals
//! - Saucer signals (3-bar pattern)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AwesomeOscillator, Indicator};
//!
//! let mut ao = AwesomeOscillator::new(5, 34);
//! ao.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Awesome Oscillator indicator
#[derive(Clone)]
pub struct AwesomeOscillator {
    fast_period: usize,
    slow_period: usize,
    values: IndicatorValues,
    positive_color: Color32,
    negative_color: Color32,
    visible: bool,
}

impl AwesomeOscillator {
    /// Create a new Awesome Oscillator
    ///
    /// # Arguments
    /// * `fast_period` - Fast SMA period (typically 5)
    /// * `slow_period` - Slow SMA period (typically 34)
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        Self {
            fast_period: fast_period.max(1),
            slow_period: slow_period.max(1),
            values: IndicatorValues::new(),
            positive_color: Color32::from_rgb(0, 150, 136), // Green
            negative_color: Color32::from_rgb(244, 67, 54), // Red
            visible: true,
        }
    }

    /// Create with default params (5, 34)
    pub fn default_params() -> Self {
        Self::new(5, 34)
    }

    /// Set colors for positive and negative values
    pub fn with_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.positive_color = positive;
        self.negative_color = negative;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for AwesomeOscillator {
    fn default() -> Self {
        Self::new(5, 34)
    }
}

impl AwesomeOscillator {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.is_empty() {
            return values;
        }

        let min_period = self.slow_period.max(self.fast_period);

        // Calculate median prices (HL/2)
        let median_prices: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();

        for i in 0..data.len() {
            if i < min_period - 1 {
                values.push(IndicatorValue::None);
                continue;
            }

            // Calculate fast SMA of median prices
            let fast_start = i + 1 - self.fast_period;
            let fast_sma: f64 =
                median_prices[fast_start..=i].iter().sum::<f64>() / self.fast_period as f64;

            // Calculate slow SMA of median prices
            let slow_start = i + 1 - self.slow_period;
            let slow_sma: f64 =
                median_prices[slow_start..=i].iter().sum::<f64>() / self.slow_period as f64;

            // AO = Fast SMA - Slow SMA
            let ao = fast_sma - slow_sma;
            values.push(IndicatorValue::Single(ao));
        }
        values
    }
}

impl Indicator for AwesomeOscillator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HL2,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        fast_period: length("Fast period", 1, usize::MAX),
        slow_period: length("Slow period", 1, usize::MAX)
    ; {});

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.positive_color,
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        let value = self.values.value(0, bar)?;
        let previous = bar.checked_sub(1).and_then(|bar| self.values.value(0, bar));
        Some(if previous.is_some_and(|previous| value <= previous) {
            self.negative_color
        } else {
            self.positive_color
        })
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "AO"
    }

    fn desc(&self) -> &str {
        "Awesome Oscillator - Market momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            |_data, from| from.saturating_sub(self.fast_period.max(self.slow_period) - 1),
            |data| self.compute(data),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.positive_color, self.negative_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.positive_color = colors[0];
        }
        if colors.len() > 1 {
            self.negative_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false // AO is a separate histogram pane
    }

    fn line_count(&self) -> usize {
        1 // Single histogram
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
        vec![format!("AO({}, {})", self.fast_period, self.slow_period)]
    }
}
