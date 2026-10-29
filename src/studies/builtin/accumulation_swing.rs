use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Accumulation Swing Index (ASI)
/// Developed by Welles Wilder
/// Cumulative sum of swing index values
#[derive(Clone)]
pub struct AccumulationSwingIndex {
    /// Limit move (typically high value for stocks, actual limit for futures)
    limit_move: f64,
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

impl AccumulationSwingIndex {
    /// `limit_move` is clamped into `f64::EPSILON` to `f64::MAX`, NaN to `f64::EPSILON`; a
    /// smaller limit move only scales the index up.
    pub fn new(limit_move: f64) -> Self {
        Self {
            limit_move: clamp_param(limit_move, f64::EPSILON, f64::MAX),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::LIGHT_BLUE, // Teal
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_swing_idx(current: &Bar, prev: &Bar, limit: f64) -> f64 {
        let c = current.close;
        let cy = prev.close;
        let h = current.high;
        let l = current.low;
        let o = current.open;
        let oy = prev.open;

        let k = (h - cy).abs().max((l - cy).abs());

        let h_cy = (h - cy).abs();
        let l_cy = (l - cy).abs();
        let h_l = h - l;
        let cy_oy = (cy - oy).abs();

        let r = if h_cy >= l_cy && h_cy >= h_l {
            h_cy - 0.5 * l_cy + 0.25 * cy_oy
        } else if l_cy >= h_cy && l_cy >= h_l {
            l_cy - 0.5 * h_cy + 0.25 * cy_oy
        } else {
            h_l + 0.25 * cy_oy
        };

        if r == 0.0 {
            return 0.0;
        }

        50.0 * ((c - cy) + 0.5 * (c - o) + 0.25 * (cy - oy)) / r * (k / limit)
    }
}

impl Default for AccumulationSwingIndex {
    fn default() -> Self {
        Self::new(1000.0)
    }
}

impl Indicator for AccumulationSwingIndex {
    crate::studies::inputs::study_inputs!(self;
        limit_move: number("Limit move", f64::EPSILON, f64::MAX)
    ; {});

    fn name(&self) -> &str {
        "ASI"
    }

    fn desc(&self) -> &str {
        "Accumulation Swing Index - Cumulative swing index"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.bars;
                state.bars += 1;
                if !([bar.open, bar.high, bar.low, bar.close]
                    .iter()
                    .all(|x| x.is_finite()))
                {
                    return IndicatorValue::None;
                }
                state.count += 1;
                if let Some(previous) = &state.previous {
                    state.total += Self::calculate_swing_idx(bar, previous, self.limit_move);
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
        vec!["ASI".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_asi_calculation() {
        let mut asi = AccumulationSwingIndex::new(100.0);

        let data = vec![
            make_bar(100.0, 102.0, 98.0, 101.0),
            make_bar(101.0, 104.0, 100.0, 103.0),
            make_bar(103.0, 106.0, 102.0, 105.0),
        ];

        asi.calculate(&data);

        assert_eq!(asi.values.len(), 3);

        // In uptrend, ASI should increase
        let (IndicatorValue::Single(v1), IndicatorValue::Single(v2)) =
            (&asi.values.to_rows()[1], &asi.values.to_rows()[2])
        else {
            panic!("no values: {:?}", &asi.values.to_rows()[1..]);
        };
        assert!(v2 > v1, "ASI should increase in uptrend");
    }
}
