use super::clamp_param;
use crate::model::Bar;
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

/// Zig Zag: the swings of at least `deviation_percent` (Merrill's filtered waves), drawn
/// as straight lines between consecutive pivots.
///
/// - A bar whose prices in the chosen mode are not finite or not positive is skipped: it is
///   never a pivot and never confirms one, and a leg is drawn across it. A series with no
///   positive prices has no values, because the percentage filter needs positive prices.
///   A deviation of 0 filters nothing, so it gives no values either.
/// - Until the first reversal the study tracks the highest high and the lowest low. A rise
///   of `deviation_percent` from the lowest low confirms that low as the first pivot and
///   starts an up leg at the bar that confirms it; a fall from the highest high confirms
///   that high and starts a down leg. When one bar confirms both, the earlier extreme is
///   the first pivot, and the low when both are on one bar.
/// - Each bar first extends the current leg (a higher high in an up leg, a lower low in a
///   down leg) and only otherwise is tested for a reversal. A reversal needs at least
///   `depth` bars after the extreme it is measured from, so an outside bar, which both
///   extends the leg and reverses from the old extreme, extends the leg. Pivots are at
///   least `depth` bars apart, so a swing shorter than `depth` bars is not drawn.
/// - The last pivot is the extreme of the current leg, which moves while price extends it.
///   Bars before the first pivot and after the last have no value.
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
    /// `deviation_percent` is clamped into 0 to `f64::MAX`, NaN to 0; at 0 there are no values.
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
        self.values = vec![IndicatorValue::None; data.len()];
        let (depth, deviation) = (self.depth, self.deviation_percent);
        if deviation <= 0.0 {
            return;
        }
        let rises = |from: (usize, f64), i: usize, high: f64| {
            i - from.0 >= depth && (high - from.1) / from.1 * 100.0 >= deviation
        };
        let falls = |from: (usize, f64), i: usize, low: f64| {
            i - from.0 >= depth && (from.1 - low) / from.1 * 100.0 >= deviation
        };
        let mut bars = data
            .iter()
            .map(|b| (self.get_price(b, true), self.get_price(b, false)))
            .enumerate()
            .filter(|&(_, (high, low))| [high, low].iter().all(|p| p.is_finite() && *p > 0.0));
        let Some((first, (high, low))) = bars.next() else {
            return;
        };

        // Before the first pivot: the highest high and the lowest low.
        let (mut top, mut bottom) = ((first, high), (first, low));
        // Pivots in bar order, then the current leg's direction and extreme.
        let mut pivots = Vec::new();
        let mut trend: Option<(bool, (usize, f64))> = None;

        for (i, (high, low)) in bars {
            match &mut trend {
                None => {
                    if high > top.1 {
                        top = (i, high);
                    }
                    if low < bottom.1 {
                        bottom = (i, low);
                    }
                    // A bar confirming both reversals confirms the earlier extreme.
                    let (rise, fall) = (rises(bottom, i, high), falls(top, i, low));
                    if rise && !(fall && top.0 < bottom.0) {
                        pivots.push(bottom);
                        trend = Some((true, (i, high)));
                    } else if fall {
                        pivots.push(top);
                        trend = Some((false, (i, low)));
                    }
                }
                Some((up, extreme)) => {
                    let (extends, reverses) = if *up {
                        (high > extreme.1, falls(*extreme, i, low))
                    } else {
                        (low < extreme.1, rises(*extreme, i, high))
                    };
                    if extends {
                        *extreme = (i, if *up { high } else { low });
                    } else if reverses {
                        pivots.push(*extreme);
                        *up = !*up;
                        *extreme = (i, if *up { high } else { low });
                    }
                }
            }
        }
        pivots.extend(trend.map(|(_, extreme)| extreme));

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
        // (name, study, (high, low) per bar, expected value per bar)
        type Case<'a> = (&'a str, ZigZag, &'a [(f64, f64)], &'a [Option<f64>]);
        let nan = f64::NAN;
        let sine: Vec<(f64, f64)> = (0..20)
            .map(|i| match i {
                0 => (nan, nan),
                _ => {
                    let c = 100.0 + 5.0 * (0.7 * f64::from(i)).sin();
                    (c + 1.0, c - 1.0)
                }
            })
            .collect();
        let cases: &[Case] = &[
            (
                // Warm-up bar, NaN bar inside a leg, bars after the last pivot.
                "swings",
                ZigZag::new(5.0, 1),
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
            (
                "flat",
                ZigZag::new(5.0, 1),
                &[(100.0, 100.0); 6],
                &[None; 6],
            ),
            (
                // Bar 3 confirms both the fall from 110 and the rise from 90: the earlier
                // extreme is the first pivot, and the next pivot is `depth` bars after it.
                "both reversals at once",
                ZigZag::new(5.0, 2),
                &[(110.0, 109.0), (95.0, 90.0), (108.0, 105.0), (106.0, 104.0)],
                &[Some(110.0), Some(108.0), Some(106.0), Some(104.0)],
            ),
            (
                // A deviation of 0 filters nothing, so there are no swings to draw.
                "deviation 0",
                ZigZag::new(0.0, 1),
                &[
                    (101.0, 100.0),
                    (103.0, 102.0),
                    (106.0, 105.0),
                    (110.0, 109.0),
                ],
                &[None; 4],
            ),
            (
                // The first leg starts at the low of bar 0, not at the first high.
                "rises from the first bar",
                ZigZag::new(5.0, 1),
                &[
                    (101.0, 100.0),
                    (103.0, 102.0),
                    (106.0, 105.0),
                    (110.0, 109.0),
                    (104.0, 103.0),
                    (105.0, 104.0),
                ],
                &[
                    Some(100.0),
                    Some(103.333_333_333_333_33),
                    Some(106.666_666_666_666_67),
                    Some(110.0),
                    Some(103.0),
                    None,
                ],
            ),
            (
                // Bar 2 makes a new high and falls 5% from the previous one: it extends the leg.
                "outside bar",
                ZigZag::new(5.0, 1),
                &[
                    (101.0, 100.0),
                    (110.0, 108.0),
                    (112.0, 104.0),
                    (111.0, 109.0),
                    (118.0, 116.0),
                ],
                &[
                    Some(100.0),
                    Some(104.5),
                    Some(109.0),
                    Some(113.5),
                    Some(118.0),
                ],
            ),
            (
                // The highest high, on bar 0, comes before the low that the first reversal confirms.
                "high before the low",
                ZigZag::new(5.0, 1),
                &[(100.0, 99.0), (96.0, 95.1), (99.9, 99.0)],
                &[None, Some(95.1), Some(99.9)],
            ),
            (
                // Bar 2 has a low of 0, so it is skipped and the leg is drawn across it.
                "touches zero",
                ZigZag::new(5.0, 1),
                &[(5.0, 4.0), (3.0, 2.0), (1.0, 0.0), (1.02, 0.5)],
                &[Some(5.0), Some(3.5), Some(2.0), Some(0.5)],
            ),
            (
                "negative prices",
                ZigZag::new(5.0, 1),
                &[
                    (-1.0, -2.0),
                    (-3.0, -4.0),
                    (-5.0, -6.0),
                    (-1.0, -2.0),
                    (3.0, 2.0),
                    (-4.0, -5.0),
                ],
                &[None; 6],
            ),
            (
                // High and low 1 above and below 100 + 5 sin(0.7 i), NaN on bar 0.
                "NaN first bar",
                ZigZag::new(5.0, 1),
                &sine,
                &[
                    None,
                    None,
                    Some(105.9272486499423),
                    Some(103.5593463073295),
                    Some(101.19144396471671),
                    Some(98.82354162210393),
                    Some(96.45563927949114),
                    Some(94.08773693687834),
                    Some(97.05101299500501),
                    Some(100.01428905313168),
                    Some(102.97756511125833),
                    Some(105.940841169385),
                    Some(103.57349520635668),
                    Some(101.20614924332837),
                    Some(98.83880328030004),
                    Some(96.47145731727173),
                    Some(94.10411135424341),
                    Some(97.51869050648995),
                    Some(100.93326965873648),
                    Some(104.34784881098301),
                ],
            ),
        ];

        for (name, zz, bars, expected) in cases {
            let data: Vec<Bar> = bars.iter().map(|&(h, l)| make_bar(h, l, l)).collect();
            let mut zz = zz.clone();
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
