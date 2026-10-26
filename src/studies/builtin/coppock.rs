use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Coppock Curve
/// A momentum indicator originally designed to identify buying opportunities in stock indices
/// Uses a weighted moving avg of the sum of two ROC periods
#[derive(Clone)]
pub struct CoppockCurve {
    /// Long ROC period (typically 14)
    long_roc: usize,
    /// Short ROC period (typically 11)
    short_roc: usize,
    /// WMA period (typically 10)
    wma_period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl CoppockCurve {
    /// Create with rate-of-change periods 14 and 11 and weighted-average period 10.
    pub fn new() -> Self {
        Self {
            long_roc: 14,
            short_roc: 11,
            wma_period: 10,
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    /// Set the long and short rate-of-change and weighted-average periods, each at least one.
    pub fn with_periods(mut self, long_roc: usize, short_roc: usize, wma: usize) -> Self {
        self.long_roc = long_roc.max(1);
        self.short_roc = short_roc.max(1);
        self.wma_period = wma.max(1);
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate Rate of Change
    fn calculate_roc(prices: &[f64], period: usize) -> Vec<f64> {
        let mut roc = Vec::with_capacity(prices.len());

        for i in 0..prices.len() {
            if i < period || prices[i - period] == 0.0 {
                roc.push(f64::NAN);
            } else {
                let change = (prices[i] - prices[i - period]) / prices[i - period] * 100.0;
                roc.push(change);
            }
        }

        roc
    }
}

impl Default for CoppockCurve {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for CoppockCurve {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        long_roc: length("Long roc", 1, usize::MAX),
        short_roc: length("Short roc", 1, usize::MAX),
        wma_period: length("Wma period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Coppock"
    }

    fn desc(&self) -> &str {
        "Coppock Curve - Long-term momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let required = self
            .long_roc
            .max(self.short_roc)
            .saturating_add(self.wma_period);
        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate both ROCs
        let long_roc = Self::calculate_roc(&closes, self.long_roc);
        let short_roc = Self::calculate_roc(&closes, self.short_roc);

        // Sum of ROCs
        let roc_sum: Vec<f64> = long_roc
            .iter()
            .zip(short_roc.iter())
            .map(|(l, s)| {
                if l.is_nan() || s.is_nan() {
                    f64::NAN
                } else {
                    l + s
                }
            })
            .collect();

        // Apply WMA to the sum
        let coppock = helpers::wma(&roc_sum, self.wma_period);

        // Store results
        for value in coppock {
            if value.is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(value));
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
        false // Separate pane
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
        vec![format!(
            "Coppock({},{},{})",
            self.long_roc, self.short_roc, self.wma_period
        )]
    }
}
