use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Rainbow Oscillator (Mel Widner, "Rainbow Charts", Technical Analysis of Stocks & Commodities,
/// July 1997): the close's distance from a stack of recursive moving averages, relative to the
/// recent range of the close.
///
/// The first average is the `base_period` SMA of the close and each further one the same SMA of
/// the one before, `levels` averages in all:
/// `100 * (close - mean of the averages) / (highest close - lowest close over the last levels bars)`.
/// A bar whose range is zero has no value, so a single level never gives one.
#[derive(Clone)]
pub struct RainbowOscillator {
    /// Number of MA levels (typically 10)
    levels: usize,
    /// Base period for first SMA (typically 2)
    base_period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl RainbowOscillator {
    pub fn new(levels: usize, base_period: usize) -> Self {
        Self {
            levels: levels.max(1),
            base_period: base_period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().sum();
                result.push(sum / period as f64);
            }
        }

        result
    }
}

impl Default for RainbowOscillator {
    fn default() -> Self {
        Self::new(10, 2)
    }
}

impl Indicator for RainbowOscillator {
    crate::studies::inputs::study_inputs!(self;
        levels: length("Levels", 1, usize::MAX),
        base_period: length("Base period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Rainbow"
    }

    fn desc(&self) -> &str {
        "Rainbow Oscillator - Multi-MA distance"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Sum the recursive SMAs (each applied to the previous) bar by bar, keeping only
        // the latest level, so memory stays a few series whatever the level count.
        let mut level = Self::calculate_sma(&closes, self.base_period);
        let mut sums = level.clone();
        let mut missing: Vec<bool> = level.iter().map(|v| v.is_nan()).collect();
        let mut level_cnt = 1;
        // With a base period of 1 every level equals the closes, and once every bar misses a
        // level no deeper level can add a value: either way deeper levels change nothing.
        while level_cnt < self.levels && self.base_period > 1 && missing.contains(&false) {
            level = Self::calculate_sma(&level, self.base_period);
            for ((sum, m), v) in sums.iter_mut().zip(missing.iter_mut()).zip(&level) {
                *sum += v;
                *m |= v.is_nan();
            }
            level_cnt += 1;
        }

        // 100 * (close - mean of the averages) / (highest - lowest close over the last `levels` bars)
        for i in 0..data.len() {
            if missing[i] || i + 1 < self.levels {
                self.values.push(IndicatorValue::None);
                continue;
            }
            let window = &closes[i + 1 - self.levels..=i];
            let highest = window.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let lowest = window.iter().copied().fold(f64::INFINITY, f64::min);
            let range = highest - lowest;
            if range > 0.0 {
                let osc = 100.0 * (closes[i] - sums[i] / level_cnt as f64) / range;
                self.values.push(IndicatorValue::Single(osc));
            } else {
                self.values.push(IndicatorValue::None);
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
        vec![format!("Rainbow({},{})", self.levels, self.base_period)]
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
            colors: None,
            payload: None,
        }
    }

    /// Reference values from Mel Widner's published definition ("Rainbow Charts", Technical
    /// Analysis of Stocks & Commodities, July 1997), computed independently in double precision:
    /// `100 * (C - mean of ten recursive 2-bar SMAs) / (HHV(C, 10) - LLV(C, 10))`.
    #[test]
    fn rainbow_matches_published_definition() {
        let mut closes = vec![10.0, 11.0, 13.0, 12.0, 14.0, 15.0, 13.0, 16.0, 17.0, 15.0];
        closes.extend([18.0; 11]);
        closes.extend([20.0, 19.0, 21.0]);
        let expected: [Option<f64>; 24] = [
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(36.67689732142857),
            Some(30.81705729166669),
            Some(20.88704427083332),
            Some(15.666015624999972),
            Some(8.736328124999986),
            Some(4.130859375),
            Some(2.5781250000000235),
            Some(0.6998697916666666),
            Some(0.12044270833335702),
            // Ten equal closes: the range is zero, so there is no value.
            None,
            None,
            Some(90.00976562500007),
            Some(25.1220703125),
            Some(70.48502604166664),
        ];

        let mut rainbow = RainbowOscillator::default();
        rainbow.calculate(&closes.iter().map(|&c| make_bar(c)).collect::<Vec<_>>());

        assert_eq!(rainbow.values.len(), expected.len());
        for (i, (got, want)) in rainbow.values.to_rows().iter().zip(expected).enumerate() {
            match (got, want) {
                (IndicatorValue::None, None) => {}
                (IndicatorValue::Single(g), Some(w)) => {
                    assert!((g - w).abs() < 1e-12, "bar {i}: got {g}, want {w}")
                }
                _ => panic!("bar {i}: got {got:?}, want {want:?}"),
            }
        }
    }
}
