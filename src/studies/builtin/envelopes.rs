use super::clamp_param;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Moving Average Envelopes
#[derive(Clone)]
pub struct Envelopes {
    period: usize,
    percent: f64,
    ma_type: String,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl Envelopes {
    /// `percent` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, percent: f64, ma_type: &str) -> Self {
        Self {
            period: period.max(1),
            percent: clamp_param(percent, 0.0, f64::MAX),
            ma_type: ma_type.to_string(),
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Upper
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Middle
                DESIGN_TOKENS.semantic.extended.error,   // Red - Lower
            ],
            visible: true,
        }
    }
}

impl Default for Envelopes {
    fn default() -> Self {
        Self::new(20, 2.5, "SMA")
    }
}

impl Indicator for Envelopes {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        percent: number("Percent", 0.0, f64::MAX),
        ma_type: choice("Average", ["SMA".to_owned(), "EMA".to_owned()])
    ; {});

    fn name(&self) -> &str {
        "Envelopes"
    }

    fn desc(&self) -> &str {
        "Moving Average Envelopes - Percentage bands around a moving average"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let closes: Vec<_> = data.iter().map(|bar| bar.close).collect();
        let ma_values = match self.ma_type.as_str() {
            "EMA" => crate::studies::helpers::ema(&closes, self.period),
            _ => crate::studies::helpers::sma(&closes, self.period),
        };

        let multiplier = self.percent / 100.0;

        for ma in ma_values {
            self.values
                .push_lines(&[ma * (1.0 + multiplier), ma, ma * (1.0 - multiplier)]);
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        3
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
        vec![
            format!("Upper(+{}%)", self.percent),
            format!("{}({})", self.ma_type, self.period),
            format!("Lower(-{}%)", self.percent),
        ]
    }
}
