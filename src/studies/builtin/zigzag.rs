use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
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

/// One swing extreme in source-bar order. The last pivot is provisional and can move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZigZagPivot {
    /// Index in the bars passed to [`Indicator::calculate`], including skipped bars.
    pub bar_index: usize,
    /// Price of the swing extreme.
    pub price: f64,
    /// Whether this is a high; highs and lows alternate.
    pub is_high: bool,
}

/// Zig Zag: the swings of at least `deviation_percent` (Merrill's filtered waves), drawn
/// as straight lines between consecutive pivots.
///
/// - A bar whose prices in the chosen mode are not finite has no value and is not counted in
///   `depth`, as if it were not in the series. A bar whose prices are not positive is skipped:
///   it is never a pivot and never confirms one, and a leg is drawn across it. A series with no
///   positive prices has no values, because the percentage filter needs positive prices.
///   A deviation of 0 filters nothing, so it gives no values either.
/// - Until the first reversal the study tracks the highest high and the lowest low. A rise
///   of `deviation_percent` from the lowest low confirms that low as the first pivot; a fall
///   from the highest high confirms that high. When one bar confirms both, the earlier
///   extreme is the first pivot, and the low when both are on one bar.
/// - Each bar first extends the current leg (a higher high in an up leg, a lower low in a
///   down leg) and only otherwise is tested for a reversal. A reversal needs at least
///   `depth` bars after the extreme it is measured from, so an outside bar, which both
///   extends the leg and reverses from the old extreme, extends the leg.
/// - Pivots sit on the swing extremes. When a reversal is confirmed, the new leg starts at
///   the lowest low (for a down leg) or the highest high (for an up leg) since the pivot it
///   reverses from, which may be before the confirming bar, and the bars after that extreme
///   up to the confirming bar are judged against the new leg as if it had started there,
///   so a reversal they already confirm is drawn too. `depth` is the minimum number of bars
///   from a pivot to the bar that confirms the next reversal, not the distance between
///   pivots: two pivots can be fewer than `depth` bars apart, and a swing shorter than
///   `depth` bars is drawn when a later bar confirms it.
/// - The last pivot is the extreme of the current leg, which moves while price extends it.
///   Bars before the first pivot and after the last have no value.
#[derive(Clone)]
pub struct ZigZag {
    /// Min percentage change to register a swing
    deviation_percent: f64,
    /// Min number of bars from a pivot to the bar that confirms the next reversal
    depth: usize,
    /// Price mode
    mode: ZigZagMode,
    values: IndicatorValues,
    pivots: Vec<ZigZagPivot>,
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
            values: IndicatorValues::new(),
            pivots: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info,
            visible: true,
        }
    }

    /// Swing extremes used to draw the study, in source-bar order.
    /// All except the last are confirmed; the last follows the current leg.
    pub fn pivots(&self) -> &[ZigZagPivot] {
        &self.pivots
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

    /// The values over `data`, every bar of which has finite prices.
    fn compute(&self, data: &[Bar]) -> (IndicatorValues, Vec<ZigZagPivot>) {
        let mut values = IndicatorValues::missing(data.len());
        let (depth, deviation) = (self.depth, self.deviation_percent);
        if deviation <= 0.0 {
            return (values, Vec::new());
        }
        // (bar, high, low) of the bars with usable prices.
        let bars: Vec<(usize, f64, f64)> = data
            .iter()
            .enumerate()
            .map(|(i, b)| (i, self.get_price(b, true), self.get_price(b, false)))
            .filter(|&(_, high, low)| [high, low].iter().all(|p| p.is_finite() && *p > 0.0))
            .collect();
        // The extremes below are (position in `bars`, price).
        let rises = |from: (usize, f64), p: usize| {
            bars[p].0 - bars[from.0].0 >= depth
                && (bars[p].1 - from.1) / from.1 * 100.0 >= deviation
        };
        let falls = |from: (usize, f64), p: usize| {
            bars[p].0 - bars[from.0].0 >= depth
                && (from.1 - bars[p].2) / from.1 * 100.0 >= deviation
        };
        let Some(&(_, high, low)) = bars.first() else {
            return (values, Vec::new());
        };

        // Before the first pivot: the highest high and the lowest low.
        let (mut top, mut bottom) = ((0, high), (0, low));
        // Pivots in bar order, then the current leg's direction and extreme.
        let mut pivots = Vec::new();
        let mut trend: Option<(bool, (usize, f64))> = None;

        let mut p = 1;
        while p < bars.len() {
            let (_, high, low) = bars[p];
            // The pivot a reversal confirms at this bar, and whether the new leg goes up.
            let mut reversal = None;
            match &mut trend {
                None => {
                    if high > top.1 {
                        top = (p, high);
                    }
                    if low < bottom.1 {
                        bottom = (p, low);
                    }
                    // A bar confirming both reversals confirms the earlier extreme.
                    let (rise, fall) = (rises(bottom, p), falls(top, p));
                    if rise && !(fall && top.0 < bottom.0) {
                        reversal = Some((bottom, true));
                    } else if fall {
                        reversal = Some((top, false));
                    }
                }
                Some((up, extreme)) => {
                    let (extends, reverses) = if *up {
                        (high > extreme.1, falls(*extreme, p))
                    } else {
                        (low < extreme.1, rises(*extreme, p))
                    };
                    if extends {
                        *extreme = (p, if *up { high } else { low });
                    } else if reverses {
                        reversal = Some((*extreme, !*up));
                    }
                }
            }
            if let Some((pivot, up)) = reversal {
                pivots.push((pivot, !up));
                // The new leg starts at the swing's extreme since the pivot, not at the confirming
                // bar; the bars after it are judged again against the leg.
                let extreme = (pivot.0 + 1..=p)
                    .map(|q| (q, if up { bars[q].1 } else { bars[q].2 }))
                    .reduce(|best, c| {
                        if (c.1 > best.1) == up && c.1 != best.1 {
                            c
                        } else {
                            best
                        }
                    })
                    .expect("the pivot comes before this bar");
                trend = Some((up, extreme));
                p = extreme.0;
            }
            p += 1;
        }
        pivots.extend(trend.map(|(up, extreme)| (extreme, up)));
        // Back to bar numbers.
        let pivots: Vec<ZigZagPivot> = pivots
            .into_iter()
            .map(|((p, price), is_high)| ZigZagPivot {
                bar_index: bars[p].0,
                price,
                is_high,
            })
            .collect();

        // Renderers join values on adjacent bars, so fill each leg with a
        // straight line between its pivots. A lone pivot is not a swing.
        for leg in pivots.windows(2) {
            let (start, from) = (leg[0].bar_index, leg[0].price);
            let (end, to) = (leg[1].bar_index, leg[1].price);
            for i in start..=end {
                let t = (i - start) as f64 / (end - start) as f64;
                values.set(i, IndicatorValue::Single(from + (to - from) * t));
            }
        }
        (values, pivots)
    }
}

/// Construct with the conventional default parameters.
impl Default for ZigZag {
    fn default() -> Self {
        Self::new(5.0, 10)
    }
}

impl Indicator for ZigZag {
    crate::studies::inputs::study_inputs!(self;
        deviation_percent: number("Deviation percent", 0.0, f64::MAX),
        depth: length("Depth", 1, usize::MAX),
        mode: choice("Mode", [ZigZagMode::HighLow, ZigZagMode::Close, ZigZagMode::Typical])
    ; {});

    fn name(&self) -> &str {
        "ZigZag"
    }

    fn desc(&self) -> &str {
        "ZigZag - Filters out minor price movements to show significant swings"
    }

    fn calculate(&mut self, data: &[Bar]) {
        let finite = |b: &Bar| {
            [true, false]
                .iter()
                .all(|&high| self.get_price(b, high).is_finite())
        };
        let mut pivots = Vec::new();
        let values = over_finite_bars(data, finite, |data| {
            let (values, computed) = self.compute(data);
            pivots = computed;
            values
        });
        if data.iter().any(|b| !finite(b)) {
            let indices: Vec<_> = data
                .iter()
                .enumerate()
                .filter(|(_, b)| finite(b))
                .map(|(i, _)| i)
                .collect();
            for pivot in &mut pivots {
                pivot.bar_index = indices[pivot.bar_index];
            }
        }
        self.values = values;
        self.pivots = pivots;
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
            colors: None,
            payload: None,
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
                // Warm-up bar, NaN bar inside a leg (no value, the leg is drawn as if it were absent),
                // bars after the last pivot.
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
                    None,
                    Some(93.0 + 8.0 / 3.0),
                    Some(93.0 + 16.0 / 3.0),
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
                // extreme is the first pivot. The fall's low is on bar 1, and bar 3 is 2 bars
                // after it, so it confirms the rise too: the pivots are at bars 0, 1 and 2.
                "both reversals at once",
                ZigZag::new(5.0, 2),
                &[(110.0, 109.0), (95.0, 90.0), (108.0, 105.0), (106.0, 104.0)],
                &[Some(110.0), Some(90.0), Some(108.0), None],
            ),
            (
                // The fall from 113 bottoms at 100 on bar 5, which is only 2 bars after the top
                // and so is confirmed on bar 6: the pivot is bar 5, not bar 6.
                "swing extreme before the confirming bar",
                ZigZag::new(5.0, 3),
                &[
                    (101.0, 100.0),
                    (105.0, 104.0),
                    (108.0, 107.0),
                    (113.0, 112.0),
                    (110.0, 104.0),
                    (103.0, 100.0),
                    (107.0, 106.0),
                    (110.0, 109.0),
                    (114.0, 113.0),
                    (118.0, 117.0),
                ],
                &[
                    Some(100.0),
                    Some(313.0 / 3.0),
                    Some(326.0 / 3.0),
                    Some(113.0),
                    Some(106.5),
                    Some(100.0),
                    Some(104.5),
                    Some(109.0),
                    Some(113.5),
                    Some(118.0),
                ],
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
                // The NaN bar is not in the series: it has no value and is not a bar of `depth`,
                // so bar 3 is 1 bar after the top and does not confirm a fall.
                "NaN bar inside a leg",
                ZigZag::new(5.0, 2),
                &[(100.0, 99.0), (110.0, 109.0), (nan, nan), (104.0, 103.0)],
                &[Some(99.0), Some(110.0), None, None],
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
            let extremes: &[(usize, bool)] = match *name {
                "swings" => &[(1, true), (4, false), (8, true), (10, false)],
                "both reversals at once" => &[(0, true), (1, false), (2, true)],
                "swing extreme before the confirming bar" => {
                    &[(0, false), (3, true), (5, false), (9, true)]
                }
                "rises from the first bar" => &[(0, false), (3, true), (4, false)],
                "outside bar" => &[(0, false), (4, true)],
                "high before the low" => &[(1, false), (2, true)],
                "touches zero" => &[(0, true), (3, false)],
                "NaN bar inside a leg" => &[(0, false), (1, true)],
                "NaN first bar" => &[(2, true), (7, false), (11, true), (16, false), (19, true)],
                "flat" | "deviation 0" | "negative prices" => &[],
                _ => unreachable!("case needs its pivot expectations"),
            };
            assert_eq!(zz.pivots().len(), extremes.len(), "{name}");
            for (pivot, &(bar, high)) in zz.pivots().iter().zip(extremes) {
                assert_eq!((pivot.bar_index, pivot.is_high), (bar, high), "{name}");
                assert!(
                    (pivot.price - expected[bar].unwrap()).abs() < 1e-9,
                    "{name}"
                );
            }
            let got: Vec<Option<f64>> = zz
                .values()
                .to_rows()
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
