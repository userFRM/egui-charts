use super::clamp_param;
use crate::model::Bar;
/// ZigZag Indicator
/// Identifies significant price swings by filtering out smaller movements
/// Useful for identifying trends, support/resistance, and pattern recognition
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// ZigZag mode - what price to use
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ZigZagMode {
    /// Use high/low for zigzag points
    #[default]
    HighLow,
    /// Use close price only
    Close,
    /// Use typical price (HLC/3)
    Typical,
}

#[derive(Clone)]
pub struct ZigZag {
    /// Min percentage change to register a swing
    deviation_percent: f64,
    /// Min number of bars between pivots
    depth: usize,
    /// Price mode
    mode: ZigZagMode,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ZigZag {
    /// `deviation_percent` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(deviation_percent: f64, depth: usize) -> Self {
        Self {
            deviation_percent: clamp_param(deviation_percent, 0.0, f64::MAX),
            depth: depth.max(1),
            mode: ZigZagMode::HighLow,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info,
            visible: true,
        }
    }

    pub fn with_mode(mut self, mode: ZigZagMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Get price value based on mode
    fn get_price(&self, bar: &Bar, is_high: bool) -> f64 {
        match self.mode {
            ZigZagMode::HighLow => {
                if is_high {
                    bar.high
                } else {
                    bar.low
                }
            }
            ZigZagMode::Close => bar.close,
            ZigZagMode::Typical => (bar.high + bar.low + bar.close) / 3.0,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for ZigZag {
    fn default() -> Self {
        Self::new(5.0, 10)
    }
}

impl Indicator for ZigZag {
    fn name(&self) -> &str {
        "ZigZag"
    }

    fn desc(&self) -> &str {
        "ZigZag - Filters out minor price movements to show significant swings"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.depth.saturating_add(1) {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Initialize all values as None
        for _ in 0..data.len() {
            self.values.push(IndicatorValue::None);
        }

        // Find initial high and low
        let mut last_pivot_idx = 0;
        let mut last_pivot_price = self.get_price(&data[0], true);
        let mut is_looking_for_high = true;

        // First pass: find initial direction
        let initial_high = data
            .iter()
            .take(self.depth)
            .map(|b| self.get_price(b, true))
            .fold(f64::NEG_INFINITY, f64::max);
        let initial_low = data
            .iter()
            .take(self.depth)
            .map(|b| self.get_price(b, false))
            .fold(f64::INFINITY, f64::min);

        // Start from the extreme point
        for i in 0..self.depth.min(data.len()) {
            let high = self.get_price(&data[i], true);
            let low = self.get_price(&data[i], false);

            if (high - initial_high).abs() < 1e-10 {
                last_pivot_idx = i;
                last_pivot_price = high;
                is_looking_for_high = false; // Found high, now look for low
                break;
            } else if (low - initial_low).abs() < 1e-10 {
                last_pivot_idx = i;
                last_pivot_price = low;
                is_looking_for_high = true; // Found low, now look for high
                break;
            }
        }

        // Pivots in bar order; the last one is the extreme of the current leg.
        let mut pivots = vec![(last_pivot_idx, last_pivot_price)];

        // Main calculation loop
        for i in last_pivot_idx + 1..data.len() {
            let high = self.get_price(&data[i], true);
            let low = self.get_price(&data[i], false);

            if is_looking_for_high {
                // Looking for high after low
                let change = (high - last_pivot_price) / last_pivot_price * 100.0;

                if change >= self.deviation_percent && i - last_pivot_idx >= self.depth {
                    // Found significant high
                    pivots.push((i, high));
                    last_pivot_idx = i;
                    last_pivot_price = high;
                    is_looking_for_high = false;
                } else if low < last_pivot_price {
                    // New lower low, update the last pivot
                    pivots.pop();
                    pivots.push((i, low));
                    last_pivot_idx = i;
                    last_pivot_price = low;
                }
            } else {
                // Looking for low after high
                let change = (last_pivot_price - low) / last_pivot_price * 100.0;

                if change >= self.deviation_percent && i - last_pivot_idx >= self.depth {
                    // Found significant low
                    pivots.push((i, low));
                    last_pivot_idx = i;
                    last_pivot_price = low;
                    is_looking_for_high = true;
                } else if high > last_pivot_price {
                    // New higher high, update the last pivot
                    pivots.pop();
                    pivots.push((i, high));
                    last_pivot_idx = i;
                    last_pivot_price = high;
                }
            }
        }

        // Renderers join values on adjacent bars, so fill each leg with a
        // straight line between its pivots. A lone pivot is not a swing.
        for leg in pivots.windows(2) {
            let ((start, from), (end, to)) = (leg[0], leg[1]);
            for i in start..=end {
                let t = (i - start) as f64 / (end - start) as f64;
                self.values[i] = IndicatorValue::Single(from + (to - from) * t);
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        true // Drawn on price chart
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
            "ZigZag({:.1}%, {})",
            self.deviation_percent, self.depth
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    /// Expected values from an independent percentage-filter reference
    /// (Merrill's filtered waves, drawn as straight lines between pivots).
    #[test]
    fn zigzag_draws_lines_between_pivots() {
        // (name, (high, low) per bar, expected value per bar)
        type Case<'a> = (&'a str, &'a [(f64, f64)], &'a [Option<f64>]);
        let nan = f64::NAN;
        let cases: &[Case] = &[
            (
                // Warm-up bar, NaN bar inside a leg, bars after the last pivot.
                "swings",
                &[
                    (99.0, 98.5),
                    (100.0, 98.0),
                    (99.0, 96.0),
                    (97.0, 94.0),
                    (96.0, 93.0),
                    (nan, nan),
                    (97.0, 94.0),
                    (99.0, 95.0),
                    (101.0, 97.0),
                    (100.0, 96.0),
                    (98.0, 95.0),
                    (97.0, 95.5),
                    (96.0, 95.2),
                ],
                &[
                    None,
                    Some(100.0),
                    Some(97.666_666_666_666_67),
                    Some(95.333_333_333_333_33),
                    Some(93.0),
                    Some(95.0),
                    Some(97.0),
                    Some(99.0),
                    Some(101.0),
                    Some(98.0),
                    Some(95.0),
                    None,
                    None,
                ],
            ),
            ("flat", &[(100.0, 100.0); 6], &[None; 6]),
        ];

        for (name, bars, expected) in cases {
            let data: Vec<Bar> = bars.iter().map(|&(h, l)| make_bar(h, l, l)).collect();
            let mut zz = ZigZag::new(5.0, 1);
            zz.calculate(&data);
            let got: Vec<Option<f64>> = zz
                .values()
                .iter()
                .map(|v| match v {
                    IndicatorValue::Single(x) => Some(*x),
                    _ => None,
                })
                .collect();
            assert_eq!(got.len(), expected.len(), "{name}");
            for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
                match (g, e) {
                    (Some(g), Some(e)) => {
                        assert!((g - e).abs() < 1e-9, "{name} bar {i}: {g} != {e}")
                    }
                    (None, None) => {}
                    _ => panic!("{name} bar {i}: got {g:?}, expected {e:?}\n  got {got:?}"),
                }
            }
        }
    }

    #[test]
    fn test_zigzag_modes() {
        let zz_hl = ZigZag::new(5.0, 1).with_mode(ZigZagMode::HighLow);
        let zz_close = ZigZag::new(5.0, 1).with_mode(ZigZagMode::Close);

        let bar = make_bar(110.0, 90.0, 105.0);

        assert_eq!(zz_hl.get_price(&bar, true), 110.0);
        assert_eq!(zz_hl.get_price(&bar, false), 90.0);
        assert_eq!(zz_close.get_price(&bar, true), 105.0);
        assert_eq!(zz_close.get_price(&bar, false), 105.0);
    }
}
