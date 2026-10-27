use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Williams Accumulation/Distribution
/// Developed by Larry Williams
/// Measures buying and selling pressure
#[derive(Clone)]
pub struct WilliamsAD {
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    bars: usize,
    count: usize,
    first: Option<usize>,
    previous: Option<Bar>,
    total: f64,
}

impl WilliamsAD {
    /// Create with the default plot colors.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::UP, // Teal
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for WilliamsAD {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for WilliamsAD {
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "WAD"
    }

    fn desc(&self) -> &str {
        "Williams Accumulation/Distribution - Buying/selling pressure"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.bars;
                state.bars += 1;
                if !([bar.high, bar.low, bar.close].iter().all(|x| x.is_finite())) {
                    return IndicatorValue::None;
                }
                state.count += 1;
                if let Some(previous) = &state.previous {
                    state.total += if bar.close > previous.close {
                        bar.close - bar.low.min(previous.close)
                    } else if bar.close < previous.close {
                        bar.close - bar.high.max(previous.close)
                    } else {
                        0.0
                    };
                }
                state.first.get_or_insert(index);
                state.previous = Some(bar.clone());
                IndicatorValue::Single(state.total)
            });
        // The batch definition reveals the initial zero only once a second usable bar exists.
        if let Some(first) = self.stepper.state().first {
            self.values.set(
                first,
                if self.stepper.state().count < 2 {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(0.0)
                },
            );
        }
    }

    fn is_incremental(&self) -> bool {
        true
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
        false
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
        vec!["WAD".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_williams_ad() {
        let mut wad = WilliamsAD::new();

        let data = vec![
            make_bar(100.0, 102.0, 98.0, 101.0, 1000.0),
            make_bar(101.0, 104.0, 100.0, 103.0, 1200.0), // Up
            make_bar(103.0, 105.0, 101.0, 102.0, 800.0),  // Down
        ];

        wad.calculate(&data);

        assert_eq!(wad.values.len(), 3);
    }
}
