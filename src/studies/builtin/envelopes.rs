use crate::model::Bar;
/// Moving Average Envelopes
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct Envelopes {
    period: usize,
    percent: f64,
    ma_type: String,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl Envelopes {
    pub fn new(period: usize, percent: f64, ma_type: &str) -> Self {
        Self {
            period,
            percent,
            ma_type: ma_type.to_string(),
            values: Vec::new(),
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

impl Envelopes {
    fn calc_sma(data: &[Bar], period: usize) -> Vec<Option<f64>> {
        let mut result = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i + 1 < period {
                result.push(None);
            } else {
                let start = i + 1 - period;
                let sma: f64 = data[start..=i].iter().map(|b| b.close).sum::<f64>() / period as f64;
                result.push(Some(sma));
            }
        }
        result
    }

    fn calc_ema(data: &[Bar], period: usize) -> Vec<Option<f64>> {
        let mut result = Vec::with_capacity(data.len());
        if data.is_empty() {
            return result;
        }
        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = data[0].close;
        for bar in data {
            ema = (bar.close - ema) * multiplier + ema;
            result.push(Some(ema));
        }
        result
    }
}

impl Indicator for Envelopes {
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

        let ma_values = match self.ma_type.as_str() {
            "EMA" => Self::calc_ema(data, self.period),
            _ => Self::calc_sma(data, self.period),
        };

        let multiplier = self.percent / 100.0;

        for ma_opt in &ma_values {
            match ma_opt {
                Some(ma) => {
                    let upper = ma * (1.0 + multiplier);
                    let lower = ma * (1.0 - multiplier);
                    self.values
                        .push(IndicatorValue::Multiple(vec![upper, *ma, lower]));
                }
                None => {
                    self.values.push(IndicatorValue::None);
                }
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
