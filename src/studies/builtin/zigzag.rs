use super::clamp_param;
use crate::model::Bar;
use crate::scales::StudyFormat;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, LabelPlacement, StudyLabel};
use egui::Color32;

/// ZigZag mode - what price to use
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ZigZagMode {
    /// Use high/low for zigzag points
    #[default]
    HighLow,
    /// Use close price only
    Close,
    /// Use typical price (HLC/3)
    Typical,
}

impl ZigZagMode {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::HighLow => "High/Low",
            Self::Close => "Close",
            Self::Typical => "Typical",
        }
    }
}

/// How a Zig Zag label writes the change from the previous pivot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ZigZagChange {
    /// The price difference, signed.
    #[default]
    Price,
    /// The percentage of the previous pivot's price, signed.
    Percent,
}

impl ZigZagChange {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Price => "Price",
            Self::Percent => "Percent",
        }
    }
}

/// One swing extreme in source-bar order. The last pivot is provisional and can move.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
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
///   Bars before the first pivot have no value. With the extension to the last bar, on by
///   default, a last segment runs from the last pivot to the close of the latest bar whose prices
///   in the chosen mode are finite, straight across the bars between as a leg is. It is no pivot
///   and has no label, and there is none when that bar is the last pivot's or its close is not
///   finite. Without it the bars after the last pivot have no value.
/// - Labels show each pivot's price, its change from the previous pivot as a price difference or
///   a percentage, and the volume of the leg that ends at it, written with the precision chosen
///   in the study's settings or else the instrument's volume decimals. The last leg and the
///   extension, whose ends still move, are drawn dashed.
#[derive(Clone, Debug)]
pub struct ZigZag {
    /// Min percentage change to register a swing
    deviation_percent: f64,
    /// Min number of bars from a pivot to the bar that confirms the next reversal
    depth: usize,
    /// Price mode
    mode: ZigZagMode,
    values: IndicatorValues,
    pivots: Vec<ZigZagPivot>,
    source_len: usize,
    finite: Vec<usize>,
    prices: Vec<(usize, f64, f64)>,
    completed: Vec<((usize, f64), bool)>,
    detector: Detector,
    before_last: Checkpoint,
    color: Color32,
    visible: bool,
    price_labels: bool,
    change_labels: bool,
    change_mode: ZigZagChange,
    volume_labels: bool,
    /// Whether a last segment runs from the last pivot to the latest close.
    extend_to_last_bar: bool,
    /// The instrument's price decimals and those of the study's presentation settings.
    precision: Option<usize>,
    label_precision: Option<usize>,
    /// The instrument's volume decimals, for leg volumes without a chosen precision.
    volume_decimals: Option<usize>,
    labels: Vec<StudyLabel>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Detector {
    extremes: Option<((usize, f64), (usize, f64))>,
    trend: Option<(bool, (usize, f64))>,
}

#[derive(Clone, Debug, Default)]
struct Checkpoint {
    detector: Detector,
    finite: usize,
    prices: usize,
    completed: usize,
}

impl ZigZag {
    /// The smallest deviation in percent that [`new`](Self::new) and the input accept.
    pub(crate) const MIN_DEVIATION: f64 = 0.000_01;
    /// The largest deviation in percent.
    pub(crate) const MAX_DEVIATION: f64 = 100.0;

    /// `deviation_percent` is clamped into 0.00001 to 100, NaN to 0.00001. Pivots are labelled
    /// with their price, change and leg volume, and the line extends to the last bar.
    pub fn new(deviation_percent: f64, depth: usize) -> Self {
        Self {
            deviation_percent: clamp_param(
                deviation_percent,
                Self::MIN_DEVIATION,
                Self::MAX_DEVIATION,
            ),
            depth: depth.max(1),
            mode: ZigZagMode::HighLow,
            values: IndicatorValues::new(),
            pivots: Vec::new(),
            source_len: 0,
            finite: Vec::new(),
            prices: Vec::new(),
            completed: Vec::new(),
            detector: Detector::default(),
            before_last: Checkpoint::default(),
            color: palette::BLUE,
            visible: true,
            price_labels: true,
            change_labels: true,
            change_mode: ZigZagChange::Price,
            volume_labels: true,
            extend_to_last_bar: true,
            precision: None,
            label_precision: None,
            volume_decimals: None,
            labels: Vec::new(),
        }
    }

    /// Swing extremes used to draw the study, in source-bar order.
    /// All except the last are confirmed; the last follows the current leg.
    pub fn pivots(&self) -> &[ZigZagPivot] {
        &self.pivots
    }

    /// The minimum number of bars from a pivot to the bar that confirms the next reversal.
    #[cfg(feature = "drawings")]
    pub(super) fn depth(&self) -> usize {
        self.depth
    }

    /// Select high/low, close or typical prices for reversal detection.
    pub fn with_mode(mut self, mode: ZigZagMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Labels each pivot, above highs and below lows, with its price, its change from the
    /// previous pivot (`None` leaves it out) and the volume of the leg that ends at it, from the
    /// bar after the previous pivot through the pivot's bar. A new study shows all three,
    /// the change as a price difference.
    pub fn with_labels(mut self, price: bool, change: Option<ZigZagChange>, volume: bool) -> Self {
        self.price_labels = price;
        self.change_labels = change.is_some();
        self.change_mode = change.unwrap_or(self.change_mode);
        self.volume_labels = volume;
        self
    }

    /// Whether a last segment runs from the last pivot to the close of the latest bar whose prices
    /// in the chosen mode are finite. It is on by default and changes neither
    /// [`pivots`](Self::pivots) nor the labels.
    pub fn with_extend_to_last_bar(mut self, extend: bool) -> Self {
        self.extend_to_last_bar = extend;
        self
    }

    /// Rewrites the labels of the pivots from `fixed` on: the earlier ones and their legs are
    /// final, and the last one moves.
    fn refresh_labels(&mut self, data: &[Bar], fixed: usize) {
        if !(self.price_labels || self.change_labels || self.volume_labels) {
            self.labels.clear();
            return;
        }
        let kept = fixed.checked_sub(1).map_or(0, |last| {
            let bar = self.pivots[last].bar_index;
            self.labels.partition_point(|label| label.bar <= bar)
        });
        self.labels.truncate(kept);
        let precision = self.label_precision.or(self.precision);
        let volume_precision = self.label_precision.or(self.volume_decimals);
        for (k, pivot) in self.pivots.iter().enumerate().skip(fixed) {
            let mut lines = Vec::new();
            if self.price_labels {
                lines.push(StudyFormat::Price.format(pivot.price, precision));
            }
            if let Some(previous) = k.checked_sub(1).map(|k| self.pivots[k]) {
                let change = pivot.price - previous.price;
                let sign = if change > 0.0 { "+" } else { "" };
                if self.change_labels {
                    lines.push(match self.change_mode {
                        ZigZagChange::Price => {
                            format!("{sign}{}", StudyFormat::Price.format(change, precision))
                        }
                        ZigZagChange::Percent => {
                            let percent = change / previous.price * 100.0;
                            format!("{sign}{}", StudyFormat::Percent.format(percent, Some(2)))
                        }
                    });
                }
                if self.volume_labels {
                    let first = self.finite.binary_search(&previous.bar_index).unwrap() + 1;
                    let last = self.finite.binary_search(&pivot.bar_index).unwrap();
                    let volume: f64 = self.finite[first..=last]
                        .iter()
                        .map(|&bar| data[bar].volume)
                        .filter(|volume| volume.is_finite())
                        .sum();
                    lines.push(match (self.label_precision, self.volume_decimals) {
                        // Volume decimals stand below a thousand; K, M and B values keep two.
                        (None, Some(decimals)) if volume.is_finite() => {
                            crate::scales::VolumeFormatter {
                                precision: decimals.min(12),
                            }
                            .format_with_min_unit_decimals(volume, 2)
                        }
                        _ => StudyFormat::Volume.format(volume, volume_precision),
                    });
                }
            }
            if !lines.is_empty() {
                self.labels.push(StudyLabel {
                    bar: pivot.bar_index,
                    price: pivot.price,
                    text: lines.join("\n"),
                    placement: if pivot.is_high {
                        LabelPlacement::Above
                    } else {
                        LabelPlacement::Below
                    },
                    plot: 0,
                });
            }
        }
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

    fn tail_from(&self, data: &[Bar], from: usize) -> usize {
        if self.source_len > 0 && from >= self.source_len - 1 && data.len() >= self.source_len {
            from.min(self.source_len)
        } else {
            0
        }
    }

    /// Resume swing detection without interpolating lines that automatic drawings do not use.
    /// `threshold` gives, for a source bar index, the percentage a reversal confirmed on that
    /// bar must reach, in place of the fixed deviation; a threshold that is not a positive
    /// number confirms no reversal.
    pub(super) fn update_swings(
        &mut self,
        data: &[Bar],
        from: usize,
        threshold: Option<&dyn Fn(usize) -> f64>,
    ) -> usize {
        let start = self.tail_from(data, from);
        if start == 0 {
            self.finite.clear();
            self.prices.clear();
            self.completed.clear();
            self.detector = Detector::default();
            self.before_last = Checkpoint::default();
        } else if start < self.source_len {
            self.finite.truncate(self.before_last.finite);
            self.prices.truncate(self.before_last.prices);
            self.completed.truncate(self.before_last.completed);
            self.detector = self.before_last.detector;
        }
        let fixed = self.completed.len();
        self.pivots.truncate(fixed);
        let deviation = self.deviation_percent;
        let fixed_threshold = move |_: usize| deviation;
        let threshold = threshold.unwrap_or(&fixed_threshold);
        for (source, bar) in data.iter().enumerate().skip(start) {
            if source + 1 == data.len() {
                self.before_last = Checkpoint {
                    detector: self.detector,
                    finite: self.finite.len(),
                    prices: self.prices.len(),
                    completed: self.completed.len(),
                };
            }
            let (high, low) = (self.get_price(bar, true), self.get_price(bar, false));
            if !high.is_finite() || !low.is_finite() {
                continue;
            }
            let sequence = self.finite.len();
            self.finite.push(source);
            if high <= 0.0 || low <= 0.0 {
                continue;
            }
            self.prices.push((sequence, high, low));
            self.process(self.prices.len() - 1, threshold);
        }
        self.pivots.extend(
            self.completed[fixed..]
                .iter()
                .map(|&((point, price), is_high)| ZigZagPivot {
                    bar_index: self.finite[self.prices[point].0],
                    price,
                    is_high,
                }),
        );
        if let Some((up, (point, price))) = self.detector.trend {
            self.pivots.push(ZigZagPivot {
                bar_index: self.finite[self.prices[point].0],
                price,
                is_high: up,
            });
        }
        self.source_len = data.len();
        fixed
    }

    fn process(&mut self, mut p: usize, threshold: &dyn Fn(usize) -> f64) {
        let bars = &self.prices;
        let Some((mut top, mut bottom)) = self.detector.extremes else {
            self.detector.extremes = Some(((p, bars[p].1), (p, bars[p].2)));
            return;
        };
        let depth = self.depth;
        let finite = &self.finite;
        let reaches = |p: usize, change: f64| {
            let threshold = threshold(finite[bars[p].0]);
            threshold > 0.0 && change >= threshold
        };
        let rises = |from: (usize, f64), p: usize| {
            bars[p].0 - bars[from.0].0 >= depth && reaches(p, (bars[p].1 - from.1) / from.1 * 100.0)
        };
        let falls = |from: (usize, f64), p: usize| {
            bars[p].0 - bars[from.0].0 >= depth && reaches(p, (from.1 - bars[p].2) / from.1 * 100.0)
        };
        while p < bars.len() {
            let (_, high, low) = bars[p];
            // The pivot a reversal confirms at this bar, and whether the new leg goes up.
            let mut reversal = None;
            match &mut self.detector.trend {
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
                self.completed.push((pivot, !up));
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
                self.detector.trend = Some((up, extreme));
                p = extreme.0;
            }
            p += 1;
        }
        self.detector.extremes = Some((top, bottom));
    }
}

/// Construct with the conventional default parameters.
impl Default for ZigZag {
    fn default() -> Self {
        Self::new(5.0, 10)
    }
}

impl Indicator for ZigZag {
    fn line_style(&self, _line: usize) -> crate::studies::IndicatorLineStyle {
        crate::studies::IndicatorLineStyle {
            width: 2.0,
            ..Default::default()
        }
    }

    crate::studies::inputs::study_inputs!(self;
        deviation_percent: number("Deviation percent", Self::MIN_DEVIATION, Self::MAX_DEVIATION),
        depth: length("Depth", 1, usize::MAX),
        mode: choice("Mode", [ZigZagMode::HighLow, ZigZagMode::Close, ZigZagMode::Typical]),
        extend_to_last_bar: toggle("Extend to last bar"),
        price_labels: toggle("Show price"),
        change_labels: toggle("Show change"),
        change_mode: choice("Change as", [ZigZagChange::Price, ZigZagChange::Percent]),
        volume_labels: toggle("Show leg volume")
    ; {});

    fn name(&self) -> &str {
        "ZigZag"
    }

    fn desc(&self) -> &str {
        "ZigZag - Filters out minor price movements to show significant swings"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let start = self.tail_from(data, from);
        if start == 0 {
            self.values = IndicatorValues::missing(data.len());
        } else {
            let fixed = if start < self.source_len {
                self.before_last.completed
            } else {
                self.completed.len()
            };
            if let (Some(first), Some(last)) =
                (self.pivots.get(fixed.saturating_sub(1)), self.pivots.last())
                && first.bar_index < last.bar_index
            {
                let first = self.finite.partition_point(|&bar| bar < first.bar_index);
                // Up to the latest bar, which an extension may have reached.
                for &bar in &self.finite[first..] {
                    self.values.set(bar, IndicatorValue::None);
                }
            }
            self.values.resize(data.len(), IndicatorValue::None);
        }
        let fixed = self.update_swings(data, from, None);
        for leg in self.pivots[fixed.saturating_sub(1)..].windows(2) {
            let first = self.finite.binary_search(&leg[0].bar_index).unwrap();
            let end = self.finite.binary_search(&leg[1].bar_index).unwrap();
            for position in first..=end {
                let t = (position - first) as f64 / (end - first) as f64;
                self.values.set(
                    self.finite[position],
                    IndicatorValue::Single(leg[0].price + (leg[1].price - leg[0].price) * t),
                );
            }
        }
        if self.extend_to_last_bar
            && let (Some(&pivot), Some(&last)) = (self.pivots.last(), self.finite.last())
            && last > pivot.bar_index
            && data[last].close.is_finite()
        {
            let first = self.finite.binary_search(&pivot.bar_index).unwrap();
            let end = self.finite.len() - 1;
            let close = data[last].close;
            for position in first + 1..=end {
                let t = (position - first) as f64 / (end - first) as f64;
                self.values.set(
                    self.finite[position],
                    IndicatorValue::Single(pivot.price + (close - pivot.price) * t),
                );
            }
        }
        self.refresh_labels(data, fixed);
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_instrument(&mut self, instrument: Option<&crate::model::Instrument>) -> bool {
        let precision = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map(usize::from);
        let volume_decimals = instrument
            .and_then(|instrument| instrument.volume_decimals)
            .map(usize::from);
        let changed = (precision, volume_decimals) != (self.precision, self.volume_decimals);
        self.precision = precision;
        self.volume_decimals = volume_decimals;
        changed
    }

    fn labels(&self) -> &[StudyLabel] {
        &self.labels
    }

    fn set_label_precision(&mut self, precision: Option<usize>) {
        self.label_precision = precision;
    }

    /// The last pivot before the leg that still moves.
    fn provisional_after(&self, line: usize) -> Option<usize> {
        let before_last = self.pivots.len().checked_sub(2)?;
        (line == 0).then(|| self.pivots[before_last].bar_index)
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

    // Its labels sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec![format!(
            "ZigZag({}%, {})",
            self.deviation_percent, self.depth
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instrument;
    use crate::studies::test_support::make_range_bar as make_bar;

    /// Pivots of the "rises from the first bar" series: a low of 100 on bar 0, a high of 110 on
    /// bar 3 and a low of 103 on bar 4, the last. The leg to bar 3 holds volumes 1200.25, 300.25
    /// and 500.25, 2000.75 in all, the leg to bar 4 holds 750.375, and (103 - 110) / 110 = -6.36%.
    /// Without a chosen precision or volume decimals the volumes read 2.00K and 750; three volume
    /// decimals give 2.001K and 750.375, and a chosen precision of 1 gives 2.0K and 750.4, with
    /// prices such as 110.0, whatever the volume decimals.
    #[test]
    fn pivots_are_labelled_and_the_last_leg_is_provisional() {
        let data: Vec<Bar> = [
            (101.0, 100.0, 100.0),
            (103.0, 102.0, 1200.25),
            (106.0, 105.0, 300.25),
            (110.0, 109.0, 500.25),
            (104.0, 103.0, 750.375),
            (105.0, 104.0, 40.0),
        ]
        .into_iter()
        .map(|(high, low, volume)| Bar {
            volume,
            ..make_bar(high, low, low)
        })
        .collect();
        let study = || ZigZag::new(5.0, 1);
        // `None` keeps the labels a new study opens with.
        for (shown, want) in [
            (
                None,
                &[
                    (0, "100.00"),
                    (3, "110.00\n+10.00\n2.00K"),
                    (4, "103.00\n-7.00\n750"),
                ][..],
            ),
            (
                Some((true, Some(ZigZagChange::Price), true)),
                &[
                    (0, "100.00"),
                    (3, "110.00\n+10.00\n2.00K"),
                    (4, "103.00\n-7.00\n750"),
                ][..],
            ),
            (
                Some((true, Some(ZigZagChange::Percent), true)),
                &[
                    (0, "100.00"),
                    (3, "110.00\n+10.00%\n2.00K"),
                    (4, "103.00\n-6.36%\n750"),
                ][..],
            ),
            (
                Some((false, Some(ZigZagChange::Percent), false)),
                &[(3, "+10.00%"), (4, "-6.36%")][..],
            ),
            (Some((false, None, false)), &[][..]),
        ] {
            let mut zigzag = match shown {
                Some((price, change, volume)) => study().with_labels(price, change, volume),
                None => study(),
            };
            zigzag.calculate(&data);
            let got: Vec<_> = zigzag
                .labels()
                .iter()
                .map(|label| (label.bar, label.text.as_str()))
                .collect();
            assert_eq!(got, want);
            for label in zigzag.labels() {
                let placement = if label.bar == 3 {
                    LabelPlacement::Above
                } else {
                    LabelPlacement::Below
                };
                assert_eq!(label.placement, placement);
            }
        }

        // A chosen precision sets the prices and the volume; without one the volume takes the
        // instrument's volume decimals, and the prices keep two decimals, as COIN sets none.
        let coin = Instrument::new("COIN").with_volume_decimals(3);
        for (name, instrument, chosen, want) in [
            (
                "volume decimals",
                Some(&coin),
                None,
                ["100.00", "110.00\n+10.00\n2.001K", "103.00\n-7.00\n750.375"],
            ),
            (
                "chosen precision",
                Some(&coin),
                Some(1),
                ["100.0", "110.0\n+10.0\n2.0K", "103.0\n-7.0\n750.4"],
            ),
            (
                "chosen precision without volume decimals",
                None,
                Some(1),
                ["100.0", "110.0\n+10.0\n2.0K", "103.0\n-7.0\n750.4"],
            ),
        ] {
            let mut zigzag = study();
            zigzag.set_instrument(instrument);
            zigzag.set_label_precision(chosen);
            zigzag.calculate(&data);
            let got: Vec<_> = zigzag
                .labels()
                .iter()
                .map(|label| label.text.as_str())
                .collect();
            assert_eq!(got, want, "{name}");
        }
        // Volume decimals alone change the labels, so the chart must calculate again; the same
        // metadata again must not.
        let mut zigzag = study();
        assert_eq!(
            [
                zigzag.set_instrument(Some(&coin)),
                zigzag.set_instrument(Some(&coin))
            ],
            [true, false]
        );

        let mut zigzag = study();
        zigzag.calculate(&data[..4]);
        assert_eq!(zigzag.provisional_after(0), Some(0));
        zigzag.update(&data[..5], 4);
        assert_eq!(zigzag.provisional_after(0), Some(3));
        zigzag.update(&data, 5);
        assert_eq!(zigzag.provisional_after(0), Some(3));
        assert_eq!(zigzag.provisional_after(1), None);
    }

    /// The extension follows every tick without changing pivots or labels. At 5% and depth 1,
    /// the initial pivots are 100 at bar 0, 110 at bar 3 and 103 at bar 4. A low of 102 at bar 7
    /// extends the down leg; the high of 108 at bar 9 reverses it by 5.88%.
    #[test]
    fn the_extension_reaches_the_latest_close_on_every_tick() {
        let nan = f64::NAN;
        let bar = |(high, low, close)| make_bar(high, low, close);
        let mut data: Vec<Bar> = [
            (101.0, 100.0, 100.5),
            (103.0, 102.0, 102.5),
            (106.0, 105.0, 105.5),
            (110.0, 109.0, 109.5),
            (104.0, 103.0, 103.5),
        ]
        .into_iter()
        .map(bar)
        .collect();
        let mut on = ZigZag::new(5.0, 1);
        let mut off = ZigZag::new(5.0, 1).with_extend_to_last_bar(false);
        on.calculate(&data);
        off.calculate(&data);
        // (the bar written at `from`, values with the extension from bar 3 on)
        type Step<'a> = (Option<(usize, (f64, f64, f64))>, &'a [Option<f64>]);
        let steps: &[Step] = &[
            (None, &[Some(110.0), Some(103.0)]),
            (
                Some((5, (105.0, 104.0, 104.5))),
                &[Some(110.0), Some(103.0), Some(104.5)],
            ),
            (
                Some((5, (106.0, 104.5, 105.8))),
                &[Some(110.0), Some(103.0), Some(105.8)],
            ),
            (
                Some((6, (107.0, 106.0, 106.5))),
                &[Some(110.0), Some(103.0), Some(104.75), Some(106.5)],
            ),
            (
                Some((7, (nan, nan, nan))),
                &[Some(110.0), Some(103.0), Some(104.75), Some(106.5), None],
            ),
            (
                Some((7, (102.5, 102.0, 102.2))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                ],
            ),
            (
                Some((8, (103.0, 102.5, 102.9))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(102.9),
                ],
            ),
            (
                Some((9, (108.0, 107.0, 107.5))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(105.0),
                    Some(108.0),
                ],
            ),
            (
                Some((10, (107.5, 106.0, 106.2))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(105.0),
                    Some(108.0),
                    Some(106.2),
                ],
            ),
            // A finite high/low with no close withdraws the extension on this tick.
            (
                Some((10, (107.5, 106.0, nan))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(105.0),
                    Some(108.0),
                    None,
                ],
            ),
            (
                Some((10, (107.5, 106.0, 106.2))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(105.0),
                    Some(108.0),
                    Some(106.2),
                ],
            ),
            (
                Some((11, (nan, nan, nan))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(105.0),
                    Some(108.0),
                    Some(106.2),
                    None,
                ],
            ),
            // A non-positive high/low cannot be a pivot, but its finite close ends the extension.
            // Between 108 and 104 the one finite intermediate bar is 106; whitespace stays empty.
            (
                Some((12, (0.0, 0.0, 104.0))),
                &[
                    Some(110.0),
                    Some(108.0),
                    Some(106.0),
                    Some(104.0),
                    Some(102.0),
                    Some(105.0),
                    Some(108.0),
                    Some(106.0),
                    None,
                    Some(104.0),
                ],
            ),
        ];
        for (step, &(write, want)) in steps.iter().enumerate() {
            if let Some((from, prices)) = write {
                if from == data.len() {
                    data.push(bar(prices));
                } else {
                    data[from] = bar(prices);
                }
                on.update(&data, from);
                off.update(&data, from);
            }
            assert_eq!(on.pivots(), off.pivots(), "step {step}");
            assert_eq!(on.labels(), off.labels(), "step {step}");
            assert_eq!(
                on.provisional_after(0),
                off.provisional_after(0),
                "step {step}"
            );
            let last = on.pivots().last().unwrap().bar_index;
            let got: Vec<_> = (3..data.len())
                .map(|bar| on.values().value(0, bar))
                .collect();
            assert_eq!(got.len(), want.len(), "step {step}");
            for (bar, (got, want)) in (3..).zip(got.iter().zip(want)) {
                match (got, want) {
                    (Some(got), Some(want)) => {
                        assert!(
                            (got - want).abs() < 1e-9,
                            "step {step} bar {bar}: {got} != {want}"
                        )
                    }
                    _ => assert_eq!(got, want, "step {step} bar {bar}"),
                }
                let without = off.values().value(0, bar);
                let expected = if bar <= last { *got } else { None };
                assert_eq!(
                    without, expected,
                    "step {step} bar {bar} without the extension"
                );
            }
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
                // Closes are the lows here; the highs must not set the pivots.
                "close mode",
                ZigZag::new(5.0, 1).with_mode(ZigZagMode::Close),
                &[(120.0, 100.0), (130.0, 110.0), (120.0, 100.0)],
                &[Some(100.0), Some(110.0), Some(100.0)],
            ),
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
                // A deviation of 0 is raised to 0.00001, so the 3% rise from bar 0 confirms its low.
                "deviation 0",
                ZigZag::new(0.0, 1),
                &[
                    (101.0, 100.0),
                    (103.0, 102.0),
                    (106.0, 105.0),
                    (110.0, 109.0),
                ],
                &[
                    Some(100.0),
                    Some(103.333_333_333_333_33),
                    Some(106.666_666_666_666_67),
                    Some(110.0),
                ],
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
            let mut zz = zz.clone().with_extend_to_last_bar(false);
            zz.calculate(&data);
            let extremes: &[(usize, bool)] = match *name {
                "close mode" => &[(0, false), (1, true), (2, false)],
                "swings" => &[(1, true), (4, false), (8, true), (10, false)],
                "both reversals at once" => &[(0, true), (1, false), (2, true)],
                "swing extreme before the confirming bar" => {
                    &[(0, false), (3, true), (5, false), (9, true)]
                }
                "rises from the first bar" => &[(0, false), (3, true), (4, false)],
                "deviation 0" => &[(0, false), (3, true)],
                "outside bar" => &[(0, false), (4, true)],
                "high before the low" => &[(1, false), (2, true)],
                "touches zero" => &[(0, true), (3, false)],
                "NaN bar inside a leg" => &[(0, false), (1, true)],
                "NaN first bar" => &[(2, true), (7, false), (11, true), (16, false), (19, true)],
                "flat" | "negative prices" => &[],
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
            let before = format!("{:?}", (zz.inputs(), zz.pivots(), zz.values().to_rows()));
            for (deviation, accepted) in [
                (0.0, false),
                (0.000_01, true),
                (100.0, true),
                (150.0, false),
            ] {
                let mut edited = zz.clone();
                let result = edited.set_input(
                    "deviation_percent",
                    crate::studies::InputValue::Number(deviation),
                );
                assert_eq!(result.is_ok(), accepted, "{name} {deviation}");
                if !accepted {
                    assert!(
                        matches!(result, Err(crate::studies::StudyError::InvalidInput { .. })),
                        "{name} {deviation}"
                    );
                    assert_eq!(
                        format!(
                            "{:?}",
                            (edited.inputs(), edited.pivots(), edited.values().to_rows())
                        ),
                        before,
                        "{name} {deviation}"
                    );
                }
            }
        }
    }
}
