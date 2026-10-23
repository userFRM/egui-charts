use super::over_finite_bars;
use crate::model::Bar;
/// McGinley Dynamic
/// Adaptive moving avg designed to track price better than traditional MAs
/// Self-adjusts based on price volatility
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct McGinleyDynamic {
    /// Period (typically 10 or 14)
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl McGinleyDynamic {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.pink, // Pink
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.is_empty() {
            return values;
        }

        // Start with first close
        let mut md = data[0].close;
        values.push(IndicatorValue::Single(md));

        let k = 0.6; // Constant factor
        let n = self.period as f64;

        for i in 1..data.len() {
            let close = data[i].close;

            // McGinley Dynamic formula:
            // MD = MD_prev + (Close - MD_prev) / (k * N * (Close / MD_prev)^4)
            if md != 0.0 {
                let ratio = close / md;
                let divisor = k * n * ratio.powi(4);
                if divisor != 0.0 {
                    md = md + (close - md) / divisor;
                }
            }

            values.push(IndicatorValue::Single(md));
        }

        values
    }
}

impl Default for McGinleyDynamic {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for McGinleyDynamic {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "MD"
    }

    fn desc(&self) -> &str {
        "McGinley Dynamic - Self-adjusting moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
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
        true // Overlay on price chart
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
        vec![format!("MD({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_mcginley() {
        let mut md = McGinleyDynamic::new(14);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        md.calculate(&data);

        assert_eq!(md.values.len(), 20);

        // MD should track price but smoother
        let rows = md.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", md.values.to_rows().last().unwrap());
        };
        assert!(*v > 100.0 && *v < 120.0);
    }
}
