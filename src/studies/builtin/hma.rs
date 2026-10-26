//! Hull Moving Avg (HMA) Indicator
//!
//! HMA is a fast, smooth moving avg developed by Alan Hull that
//! significantly reduces lag while maintaining smoothness.
//!
//! # Formula
//! HMA = WMA(2 * WMA(price, n/2) - WMA(price, n), sqrt(n))
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{HMA, Indicator};
//!
//! let mut hma = HMA::new(9);
//! hma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Hull Moving Avg indicator
#[derive(Clone)]
pub struct HMA {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl HMA {
    /// Create a new HMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation (typically 9, 14, or 21)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple, // Deep Purple
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for HMA {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for HMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 2, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "HMA"
    }

    fn desc(&self) -> &str {
        "Hull Moving Avg - Fast and smooth moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let half_period = (self.period / 2).max(1);
        let sqrt_period = (self.period as f64).sqrt() as usize;
        let sqrt_period = sqrt_period.max(1);

        // Extract close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Step 1: Calculate WMA(close, period/2)
        let wma_half = helpers::wma(&closes, half_period);

        // Step 2: Calculate WMA(close, period)
        let wma_full = helpers::wma(&closes, self.period);

        // Step 3: Calculate 2 * WMA(half) - WMA(full)
        let mut raw_hma = vec![f64::NAN; data.len()];
        for i in 0..data.len() {
            if !wma_half[i].is_nan() && !wma_full[i].is_nan() {
                raw_hma[i] = 2.0 * wma_half[i] - wma_full[i];
            }
        }

        // Step 4: Calculate WMA(raw_hma, sqrt(period))
        let hma = helpers::wma(&raw_hma, sqrt_period);

        // Build output
        let min_period = self.period.saturating_add(sqrt_period) - 1;
        for i in 0..data.len() {
            if i < min_period - 1 || hma[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(hma[i]));
            }
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        1
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
        vec![format!("HMA({})", self.period)]
    }
}
