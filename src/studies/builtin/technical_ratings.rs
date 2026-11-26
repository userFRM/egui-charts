//! Technical Ratings.
//!
//! Votes of moving averages and oscillators, averaged into a moving-average score, an
//! oscillator score and a summary score between -1 (all sell) and +1 (all buy), each with the
//! share of its components that had a vote.
//!
//! # Rules
//!
//! The moving averages are SMA and EMA at 10, 20, 30, 50, 100 and 200 bars, HMA 9,
//! VWMA 20 and Ichimoku (9, 26, 52). The oscillators are RSI 14, Stochastic (14, 3, 3),
//! CCI 20, ADX (14, 14), Awesome Oscillator (5, 34), Momentum 10, MACD (12, 26, 9),
//! Stochastic RSI (14, 14, 3, 3), Williams %R 14, Bull Bear Power 13 and Ultimate (7, 14, 28).
//!
//! Averages buy below the close and sell above it. Ichimoku buys when close, conversion,
//! base, span A and span B strictly descend, and sells when they ascend. The spans are those
//! drawn at the bar, computed 25 bars earlier. RSI, CCI and Williams %R buy below 30, -100
//! and -80 respectively while rising, and sell above 70, 100 and -20 while falling.
//! Stochastic needs both lines below 20 with K above D to buy, or above 80 with K below D
//! to sell. ADX buys with +DI above -DI, ADX above 20 and rising; it sells with -DI above
//! +DI, ADX above 20 and falling. Awesome buys a positive zero cross or positive upturn
//! and sells a negative cross or negative downturn. Momentum votes by its change, MACD
//! by its line against its signal. Stochastic RSI uses Stochastic's conditions in a downtrend
//! to buy and an uptrend to sell. Bull Bear Power buys negative, rising Bear Power in an
//! uptrend and sells positive, falling Bull Power in a downtrend. Both take trend from the
//! direction of Elder Ray's 13-bar EMA. Ultimate buys above 70 and sells below 30.
//! All other conditions are neutral; comparisons are strict.
//!
//! A group's score is its mean vote. The summary averages the chosen groups' scores,
//! ignoring groups without a vote. A component missing any value its rule reads has no
//! vote; MACD votes from bar 25. A bar without finite high, low and close has no values.
//! Coverage is votes divided by components, across the chosen groups for the summary.
//! A score without a vote has no value. Categories use these thresholds:
//!
//! ```text
//! Strong Buy   score > 0.5
//! Buy          0.1 < score <= 0.5
//! Neutral      -0.1 <= score <= 0.1
//! Sell         -0.5 <= score < -0.1
//! Strong Sell  score < -0.5
//! ```
//!
//! The six outputs are the moving-average, oscillator and summary scores followed by their
//! coverages; the coverages are hidden by default. The rating basis decides which groups the
//! summary averages: both by default, or only the moving averages or only the oscillators.
//!
//! The pane plots only the summary, which follows the rating basis. The table lists its
//! interval first and each chosen additional interval once, with the rating of each group
//! the basis uses, each cell on its rating colour, followed by coverage. Other intervals
//! default to 1h, 4h, 1D, 1W and 1M; a slot left at Chart interval adds no row. Host-supplied
//! histories are requested from 2000 periods before the latest rated bar. The plot can wait
//! for interval close while the table's first row shows the latest rating.

use crate::model::{Bar, Timeframe};
use crate::studies::palette;
use crate::studies::{
    ADX, AwesomeOscillator, CCI, EMA, ElderRay, HMA, IchimokuCloud, Indicator, IndicatorValue,
    IndicatorValues, MACD, Momentum, RSI, SMA, Stochastic, StochasticRSI, StudyCell, StudyData,
    StudyDataRequest, StudyTable, TablePosition, TableTextSize, UltimateOscillator, VWMA,
    WilliamsR,
};
use chrono::{DateTime, TimeDelta, Utc};
use egui::Color32;
use std::cmp::Ordering;

/// Bars by which `IchimokuCloud::new(9, 26, 52)` draws its spans ahead: 26 periods, counting the
/// bar itself.
const CLOUD_SHIFT: usize = 25;

/// The intervals the table rates by default with other intervals shown.
const TABLE_TIMEFRAMES: [Option<Timeframe>; 5] = [
    Some(Timeframe::Hour1),
    Some(Timeframe::Hour4),
    Some(Timeframe::Day1),
    Some(Timeframe::Week1),
    Some(Timeframe::Month1),
];

/// Periods of a table interval before the latest bar from which its bars are requested: the
/// 200 bars of the slowest average where the market trades a tenth of the time.
const TABLE_HISTORY: i64 = 2000;

/// The groups whose ratings the summary averages. The `Debug` names are the choices its
/// input lists.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RatingBasis {
    /// The mean of the moving-average and oscillator ratings.
    #[default]
    All,
    /// The moving-average rating alone.
    MovingAverages,
    /// The oscillator rating alone.
    Oscillators,
}

impl std::fmt::Debug for RatingBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl RatingBasis {
    /// The groups it averages, as a range of group indices.
    fn groups(self) -> std::ops::Range<usize> {
        match self {
            Self::All => 0..2,
            Self::MovingAverages => 0..1,
            Self::Oscillators => 1..2,
        }
    }

    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::MovingAverages => "Moving averages",
            Self::Oscillators => "Oscillators",
        }
    }
}

/// Group of a component, in the order of the score outputs.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Group {
    MovingAverage,
    Oscillator,
}

#[derive(Clone, Copy, Debug)]
enum Rule {
    Average,
    Ichimoku,
    Rsi,
    Stochastic,
    Cci,
    Adx,
    Awesome,
    Momentum,
    Macd,
    StochasticRsi,
    WilliamsR,
    BullBearPower,
    Ultimate,
}

impl Rule {
    /// The vote of a component with `values` at clean bar `at` with close `close`, the market's
    /// direction being `trend`, or `None` without every value the rule reads.
    fn vote(
        self,
        values: &IndicatorValues,
        at: usize,
        close: f64,
        trend: Option<Ordering>,
    ) -> Option<f64> {
        let now = |line| values.value(line, at);
        let back = |line, bars| values.value(line, at.checked_sub(bars)?);
        // The buy and sell conditions never both hold.
        let side = |buy: bool, sell: bool| {
            if buy {
                1.0
            } else if sell {
                -1.0
            } else {
                0.0
            }
        };
        // Oversold and rising buys, overbought and falling sells.
        let zone = |low: f64, high: f64| {
            let (value, previous) = (now(0)?, back(0, 1)?);
            Some(side(
                value < low && value > previous,
                value > high && value < previous,
            ))
        };
        match self {
            Self::Average => {
                let average = now(0)?;
                Some(side(close > average, close < average))
            }
            Self::Ichimoku => {
                let (tenkan, kijun) = (now(0)?, now(1)?);
                let (a, b) = (back(2, CLOUD_SHIFT)?, back(3, CLOUD_SHIFT)?);
                Some(side(
                    close > tenkan && tenkan > kijun && kijun > a && a > b,
                    close < tenkan && tenkan < kijun && kijun < a && a < b,
                ))
            }
            Self::Rsi => zone(30.0, 70.0),
            Self::Cci => zone(-100.0, 100.0),
            Self::WilliamsR => zone(-80.0, -20.0),
            Self::Stochastic => {
                let (k, d) = (now(0)?, now(1)?);
                Some(side(
                    k < 20.0 && d < 20.0 && k > d,
                    k > 80.0 && d > 80.0 && k < d,
                ))
            }
            Self::StochasticRsi => {
                let (k, d, trend) = (now(0)?, now(1)?, trend?);
                Some(side(
                    trend == Ordering::Less && k < 20.0 && d < 20.0 && k > d,
                    trend == Ordering::Greater && k > 80.0 && d > 80.0 && k < d,
                ))
            }
            Self::Adx => {
                let (adx, plus, minus, previous) = (now(0)?, now(1)?, now(2)?, back(0, 1)?);
                Some(side(
                    plus > minus && adx > 20.0 && adx > previous,
                    plus < minus && adx > 20.0 && adx < previous,
                ))
            }
            Self::Awesome => {
                let (value, previous, earlier) = (now(0)?, back(0, 1)?, back(0, 2)?);
                Some(side(
                    value > 0.0 && (previous <= 0.0 || (value > previous && previous < earlier)),
                    value < 0.0 && (previous >= 0.0 || (value < previous && previous > earlier)),
                ))
            }
            Self::Momentum => {
                let (value, previous) = (now(0)?, back(0, 1)?);
                Some(side(value > previous, value < previous))
            }
            Self::Macd => {
                let (macd, signal) = (now(0)?, now(1)?);
                Some(side(macd > signal, macd < signal))
            }
            Self::BullBearPower => {
                let (bull, bear) = (now(0)?, now(1)?);
                let (bull_before, bear_before, trend) = (back(0, 1)?, back(1, 1)?, trend?);
                Some(side(
                    trend == Ordering::Greater && bear < 0.0 && bear > bear_before,
                    trend == Ordering::Less && bull > 0.0 && bull < bull_before,
                ))
            }
            Self::Ultimate => {
                let value = now(0)?;
                Some(side(value > 70.0, value < 30.0))
            }
        }
    }
}

#[derive(Clone)]
struct Component {
    group: Group,
    rule: Rule,
    /// Usable bars before the first vote.
    warmup: usize,
    study: Box<dyn Indicator>,
}

impl std::fmt::Debug for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Component")
            .field("group", &self.group)
            .field("rule", &self.rule)
            .field("warmup", &self.warmup)
            .finish_non_exhaustive()
    }
}

/// Moving-average, oscillator and summary ratings from the votes of their components.
#[derive(Clone, Debug)]
pub struct TechnicalRatings {
    basis: RatingBasis,
    table_intervals: bool,
    /// The intervals the table rates besides the rating's own, `None` for none.
    table_timeframes: [Option<Timeframe>; 5],
    table_position: TablePosition,
    table_size: TableTextSize,
    table_strong_buy_color: Color32,
    table_buy_color: Color32,
    table_neutral_color: Color32,
    table_sell_color: Color32,
    table_strong_sell_color: Color32,
    table_header_color: Color32,
    table_text_color: Color32,
    /// The interval the rating is calculated on: the chart's, or its Interval's.
    timeframe: Option<Timeframe>,
    /// The latest rated bar's time, from which table histories are requested.
    latest_time: Option<DateTime<Utc>>,
    /// The latest scores and coverages of each requested interval, `None` without its bars.
    intervals: Vec<(Timeframe, Option<[f64; 6]>)>,
    components: Option<Vec<Component>>,
    /// The bars with a finite high, low and close, which every component reads, and their
    /// positions in the input.
    bars: Vec<Bar>,
    sources: Vec<usize>,
    values: IndicatorValues,
    table: StudyTable,
    colors: Vec<Color32>,
    visible: bool,
}

impl TechnicalRatings {
    /// Ratings of the fixed components, with the table showing the rating's interval alone.
    ///
    /// ```
    /// use egui_charts::studies::{Indicator, RatingBasis, TechnicalRatings};
    ///
    /// let mut ratings = TechnicalRatings::new().with_basis(RatingBasis::Oscillators);
    /// ratings.calculate(&[]);
    /// assert_eq!(ratings.line_names()[2], "Summary");
    /// ```
    pub fn new() -> Self {
        Self {
            basis: RatingBasis::All,
            table_intervals: false,
            table_timeframes: TABLE_TIMEFRAMES,
            table_position: TablePosition::BottomRight,
            table_size: TableTextSize::Normal,
            table_strong_buy_color: palette::BLUE,
            table_buy_color: Color32::from_rgb(91, 156, 246),
            table_neutral_color: palette::DISABLED,
            table_sell_color: Color32::from_rgb(239, 154, 154),
            table_strong_sell_color: palette::BRIGHT_RED,
            table_header_color: Color32::from_rgb(41, 91, 121),
            table_text_color: Color32::WHITE,
            timeframe: None,
            latest_time: None,
            intervals: Vec::new(),
            components: None,
            bars: Vec::new(),
            sources: Vec::new(),
            values: IndicatorValues::new(),
            table: StudyTable {
                title: "Technical Ratings".into(),
                columns: [
                    "Interval",
                    "Moving averages",
                    "Oscillators",
                    "Summary",
                    "Coverage",
                ]
                .map(String::from)
                .to_vec(),
                ..Default::default()
            },
            colors: vec![
                palette::DISABLED,
                palette::DISABLED,
                palette::DISABLED,
                palette::BLUE,
                palette::BLUE,
                palette::BLUE,
                palette::BRIGHT_RED,
                palette::BRIGHT_RED,
                palette::BRIGHT_RED,
            ],
            visible: true,
        }
    }

    /// The summary averages the ratings of the groups `basis` chooses.
    pub fn with_basis(mut self, basis: RatingBasis) -> Self {
        self.basis = basis;
        self.components = None;
        self
    }

    /// The components in rating order: the moving averages, then the oscillators.
    fn build() -> Vec<Component> {
        let component = |group, rule, warmup, study: Box<dyn Indicator>| Component {
            group,
            rule,
            warmup,
            study,
        };
        let (average, oscillator) = (Group::MovingAverage, Group::Oscillator);
        let mut components = Vec::with_capacity(26);
        for length in [10, 20, 30, 50, 100, 200] {
            components.push(component(
                average,
                Rule::Average,
                0,
                Box::new(SMA::new(length)),
            ));
            components.push(component(
                average,
                Rule::Average,
                0,
                Box::new(EMA::new(length)),
            ));
        }
        components.extend([
            component(average, Rule::Average, 0, Box::new(HMA::new(9))),
            component(average, Rule::Average, 0, Box::new(VWMA::new(20))),
            component(
                average,
                Rule::Ichimoku,
                0,
                Box::new(IchimokuCloud::new(9, 26, 52)),
            ),
            component(oscillator, Rule::Rsi, 0, Box::new(RSI::new(14))),
            component(
                oscillator,
                Rule::Stochastic,
                0,
                Box::new(Stochastic::new(14, 3, 3)),
            ),
            component(oscillator, Rule::Cci, 0, Box::new(CCI::new(20))),
            component(oscillator, Rule::Adx, 0, Box::new(ADX::new(14))),
            component(
                oscillator,
                Rule::Awesome,
                0,
                Box::new(AwesomeOscillator::new(5, 34)),
            ),
            component(oscillator, Rule::Momentum, 0, Box::new(Momentum::new(10))),
            // The MACD shows no value before its slow and signal averages are seeded, then shows
            // all of its history; the vote starts at that bar so that it never changes.
            component(oscillator, Rule::Macd, 25, Box::new(MACD::new(12, 26, 9))),
            component(
                oscillator,
                Rule::StochasticRsi,
                0,
                Box::new(StochasticRSI::new(14, 14, 3, 3)),
            ),
            component(oscillator, Rule::WilliamsR, 0, Box::new(WilliamsR::new(14))),
            component(
                oscillator,
                Rule::BullBearPower,
                0,
                Box::new(ElderRay::new(13)),
            ),
            component(
                oscillator,
                Rule::Ultimate,
                0,
                Box::new(UltimateOscillator::new(7, 14, 28)),
            ),
        ]);
        components
    }

    /// Each component's group and vote on clean bar `at` of `bars`, in build order.
    fn votes<'a>(
        components: &'a [Component],
        bars: &'a [Bar],
        at: usize,
    ) -> impl Iterator<Item = (Group, Option<f64>)> + 'a {
        let trend = components
            .iter()
            .find(|component| matches!(component.rule, Rule::BullBearPower))
            .and_then(|component| trend(component.study.values(), bars, at));
        components.iter().map(move |component| {
            let vote = (at >= component.warmup)
                .then(|| {
                    component
                        .rule
                        .vote(component.study.values(), at, bars[at].close, trend)
                })
                .flatten();
            (component.group, vote)
        })
    }

    /// The scores and coverages on clean bar `at` of `bars`, the summary of the groups `basis`
    /// chooses.
    fn row(components: &[Component], bars: &[Bar], at: usize, basis: RatingBasis) -> [f64; 6] {
        let (mut sum, mut votes, mut enabled) = ([0.0; 2], [0usize; 2], [0usize; 2]);
        for (group, vote) in Self::votes(components, bars, at) {
            let group = usize::from(group == Group::Oscillator);
            enabled[group] += 1;
            if let Some(vote) = vote {
                sum[group] += vote;
                votes[group] += 1;
            }
        }
        Self::scores(sum, votes, enabled, basis)
    }

    /// The two group ratings, their mean over the groups `basis` chooses that have one, and the
    /// coverages, from each group's vote sum, votes and components.
    fn scores(
        sum: [f64; 2],
        votes: [usize; 2],
        enabled: [usize; 2],
        basis: RatingBasis,
    ) -> [f64; 6] {
        let score = |group: usize| {
            if votes[group] == 0 {
                f64::NAN
            } else {
                sum[group] / votes[group] as f64
            }
        };
        let coverage = |votes: usize, enabled: usize| {
            if enabled == 0 {
                f64::NAN
            } else {
                votes as f64 / enabled as f64
            }
        };
        let rated: Vec<usize> = basis.groups().filter(|&group| votes[group] > 0).collect();
        // Vote sums are whole numbers: over a common denominator the summary is rounded once,
        // so an exact boundary such as the mean of 0.8 and -0.6 stays 0.1.
        let summary = if rated.is_empty() {
            f64::NAN
        } else {
            let product: usize = rated.iter().map(|&group| votes[group]).product();
            let numerator: f64 = rated
                .iter()
                .map(|&group| sum[group] * (product / votes[group]) as f64)
                .sum();
            numerator / (rated.len() * product) as f64
        };
        let chosen = basis.groups();
        [
            score(0),
            score(1),
            summary,
            coverage(votes[0], enabled[0]),
            coverage(votes[1], enabled[1]),
            coverage(
                votes[chosen.clone()].iter().sum(),
                enabled[chosen].iter().sum(),
            ),
        ]
    }

    /// The scores and coverages of the latest of `bars`, or `None` when it has no finite high,
    /// low and close.
    fn latest(&self, bars: &[Bar]) -> Option<[f64; 6]> {
        bars.last().filter(|bar| usable(bar))?;
        let bars: Vec<_> = bars.iter().filter(|bar| usable(bar)).cloned().collect();
        let at = bars.len().checked_sub(1)?;
        let mut components = Self::build();
        for component in &mut components {
            component.study.calculate(&bars);
        }
        Some(Self::row(&components, &bars, at, self.basis))
    }

    /// The table rows of the rating's interval and of the table intervals: each group's rating the
    /// basis uses on its colour, then the coverage.
    fn fill_table(&mut self) {
        let groups: &[usize] = match self.basis {
            RatingBasis::All => &[0, 1, 2],
            RatingBasis::MovingAverages => &[0],
            RatingBasis::Oscillators => &[1],
        };
        let names = ["Moving averages", "Oscillators", "Summary"];
        self.table.columns = std::iter::once("Interval")
            .chain(groups.iter().map(|&group| names[group]))
            .chain(std::iter::once("Coverage"))
            .map(String::from)
            .collect();
        let latest = self.values.len().checked_sub(1).map(|bar| {
            std::array::from_fn(|line| self.values.value(line, bar).unwrap_or(f64::NAN))
        });
        let own = self.timeframe.map_or_else(
            || "Chart".to_owned(),
            |timeframe| timeframe.as_str().into_owned(),
        );
        let mut rows = vec![(own, latest)];
        // Only intervals still requested: once an input or the rating's interval leaves none to
        // request, `update_data` is no longer called to rebuild the rows.
        let requested = self.data_requests();
        rows.extend(
            self.intervals
                .iter()
                .filter(|(timeframe, _)| {
                    requested
                        .iter()
                        .any(|request| request.timeframe == Some(*timeframe))
                })
                .map(|(timeframe, latest)| (timeframe.as_str().into_owned(), *latest)),
        );
        let fill = |score: Option<f64>| match score.map_or("Neutral", rating) {
            "Strong Buy" => self.table_strong_buy_color,
            "Buy" => self.table_buy_color,
            "Sell" => self.table_sell_color,
            "Strong Sell" => self.table_strong_sell_color,
            _ => self.table_neutral_color,
        };
        (self.table.rows, self.table.cell_colors) = rows
            .into_iter()
            .map(|(label, latest)| {
                let value = |line: usize| {
                    latest.and_then(|row: [f64; 6]| row[line].is_finite().then_some(row[line]))
                };
                let mut cells = vec![StudyCell::Text(label)];
                let mut fills = vec![self.table_header_color];
                for &group in groups {
                    cells.push(StudyCell::Text(value(group).map_or("N/A", rating).into()));
                    fills.push(fill(value(group)));
                }
                cells.push(StudyCell::Text(value(5).map_or("N/A".into(), |coverage| {
                    format!("{:.0}%", coverage * 100.0)
                })));
                (cells, fills)
            })
            .unzip();
        self.table.header_color = Some(self.table_header_color);
        self.table.text_color = Some(self.table_text_color);
        self.table.position = self.table_position;
        self.table.text_size = self.table_size;
    }
}

/// Whether the market rises, falls or is flat on clean bar `at` of `bars`: the direction of the
/// EMA under the Elder Ray's `power`, given back as the high less Bull Power.
fn trend(power: &IndicatorValues, bars: &[Bar], at: usize) -> Option<Ordering> {
    let average = |at: usize| Some(bars[at].high - power.value(0, at)?);
    average(at)?.partial_cmp(&average(at.checked_sub(1)?)?)
}

/// Whether every component can read `bar`.
fn usable(bar: &Bar) -> bool {
    bar.high.is_finite() && bar.low.is_finite() && bar.close.is_finite()
}

/// The same as [`new`](Self::new).
impl Default for TechnicalRatings {
    fn default() -> Self {
        Self::new()
    }
}

/// The displayed rating of a score between -1 and 1. A score on a boundary takes the inner
/// rating: 0.5 is Buy, 0.1 and -0.1 are Neutral and -0.5 is Sell.
pub(crate) fn rating(score: f64) -> &'static str {
    if score > 0.5 {
        "Strong Buy"
    } else if score > 0.1 {
        "Buy"
    } else if score < -0.5 {
        "Strong Sell"
    } else if score < -0.1 {
        "Sell"
    } else {
        "Neutral"
    }
}

impl Indicator for TechnicalRatings {
    crate::studies::inputs::study_inputs!(self;
        basis: choice("Rating basis", [RatingBasis::All, RatingBasis::MovingAverages, RatingBasis::Oscillators]),
        table_intervals: toggle("Table shows other intervals"),
        table_timeframes[0]: timeframe("Table interval 1"),
        table_timeframes[1]: timeframe("Table interval 2"),
        table_timeframes[2]: timeframe("Table interval 3"),
        table_timeframes[3]: timeframe("Table interval 4"),
        table_timeframes[4]: timeframe("Table interval 5"),
        table_position: choice("Table position", TablePosition::ALL),
        table_size: choice("Table size", TableTextSize::ALL),
        table_strong_buy_color: color("Table strong buy colour"),
        table_buy_color: color("Table buy colour"),
        table_neutral_color: color("Table neutral colour"),
        table_sell_color: color("Table sell colour"),
        table_strong_sell_color: color("Table strong sell colour"),
        table_header_color: color("Table header colour"),
        table_text_color: color("Table text colour")
    ; {
        self.components = None;
    });

    fn name(&self) -> &str {
        "Technical Ratings"
    }

    fn desc(&self) -> &str {
        "Technical Ratings - Buy and sell votes of moving averages and oscillators, scored from -1 to 1"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
    }

    fn requests_allow_other_intervals(&self) -> bool {
        true
    }

    /// The chart symbol's bars of each table interval: the chosen ones in order, each once, without
    /// the rating's own interval. Intervals count as the same when they read the same.
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        if !self.table_intervals {
            return Vec::new();
        }
        let mut labels: Vec<_> = self
            .timeframe
            .iter()
            .map(|timeframe| timeframe.as_str())
            .collect();
        let mut requests = Vec::new();
        for timeframe in self.table_timeframes.into_iter().flatten() {
            let label = timeframe.as_str();
            if !labels.contains(&label) {
                labels.push(label);
                requests.push(StudyDataRequest {
                    symbol: String::new(),
                    timeframe: Some(timeframe),
                    metric: None,
                });
            }
        }
        requests
    }

    /// Each table interval from `TABLE_HISTORY` of its periods before the latest bar rated: a
    /// bounded history for intervals shorter than the chart's, enough bars for the 200-bar
    /// averages on longer ones.
    fn data_start(&self, request: &StudyDataRequest) -> Option<DateTime<Utc>> {
        // shortcut: assumes the market trades a tenth of the time; a 1-minute row just after a
        // weekend rates fewer than 200 bars and shows a lower coverage. Upgrade when a request can
        // state a number of bars.
        let timeframe = request
            .timeframe
            .filter(|timeframe| !timeframe.is_trade_based())?;
        if !self.data_requests().contains(request) {
            return None;
        }
        let span =
            TimeDelta::try_milliseconds(timeframe.duration_ms().checked_mul(TABLE_HISTORY)?)?;
        self.latest_time?.checked_sub_signed(span)
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        if from == 0 {
            self.intervals = self
                .data_requests()
                .into_iter()
                .filter_map(|request| {
                    let source = sources.iter().find(|source| source.request == request);
                    Some((
                        request.timeframe?,
                        source.and_then(|source| self.latest(source.bars)),
                    ))
                })
                .collect();
        }
        self.update(data, from);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if let Some(bar) = data.last() {
            self.latest_time = Some(bar.time);
        }
        let mut from = from.min(self.values.len()).min(data.len());
        if self.components.is_none() {
            self.components = Some(Self::build());
            from = 0;
        }
        let components = self.components.as_mut().expect("components built");
        let kept = self.sources.partition_point(|&source| source < from);
        self.sources.truncate(kept);
        self.bars.truncate(kept);
        for (source, bar) in data.iter().enumerate().skip(from) {
            if usable(bar) {
                self.sources.push(source);
                self.bars.push(bar.clone());
            }
        }
        for component in components.iter_mut() {
            component.study.update(&self.bars, kept);
        }
        self.values.truncate(from);
        let mut at = kept;
        for bar in &data[from..] {
            if usable(bar) {
                self.values
                    .push_lines(&Self::row(components, &self.bars, at, self.basis));
                at += 1;
            } else {
                self.values.push(IndicatorValue::None);
            }
        }
        self.fill_table();
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line % 3],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: line == 2,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() < 9 {
            for (line, color) in colors.into_iter().take(3).enumerate() {
                self.set_color(line, color);
            }
        } else {
            for (old, new) in self.colors.iter_mut().zip(colors) {
                *old = new;
            }
        }
    }

    fn set_color(&mut self, line: usize, color: Color32) {
        if line < 3 {
            for slot in [line, line + 3, line + 6] {
                self.colors[slot] = color;
            }
        }
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        if line >= 3 {
            return None;
        }
        let value = self.values.value(line, bar)?;
        let neutral = self.colors[line];
        let endpoint = self.colors[line + if value >= 0.0 { 3 } else { 6 }];
        if neutral == endpoint {
            return Some(neutral);
        }
        let color = neutral.lerp_to_gamma(endpoint, (value.abs() / 0.2).min(1.0) as f32);
        let mut previous = value;
        let mut streak = 0;
        for at in (0..bar).rev().take(5) {
            let Some(earlier) = self.values.value(line, at) else {
                break;
            };
            if (value > 0.0 && previous > earlier) || (value < 0.0 && previous < earlier) {
                streak += 1;
                previous = earlier;
            } else {
                break;
            }
        }
        Some(color.gamma_multiply_u8(((0.5 + streak as f32 * 0.1) * 255.0).round() as u8))
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: Some((-1.1, 1.1)),
            levels: vec![
                (
                    0.5,
                    Color32::from_rgba_unmultiplied(91, 156, 246, 128),
                    "Strong Buy",
                ),
                (
                    -0.5,
                    Color32::from_rgba_unmultiplied(239, 154, 154, 128),
                    "Strong Sell",
                ),
            ],
            fill_between_levels: false,
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
        [
            "Moving Averages",
            "Oscillators",
            "Summary",
            "Moving Averages Coverage",
            "Oscillators Coverage",
            "Summary Coverage",
        ]
        .map(String::from)
        .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarClock;
    use crate::studies::InputValue;
    use chrono::Duration;

    /// Bars of `closes` one day apart, with each price at the close.
    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    DateTime::UNIX_EPOCH + Duration::days(i as i64),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    /// Each row's cell texts and fills, preserving their column order.
    fn table(study: &TechnicalRatings) -> Vec<(Vec<String>, Vec<Color32>)> {
        let table = study.table().unwrap();
        assert_eq!(table.cell_colors.len(), table.rows.len());
        table
            .rows
            .iter()
            .zip(&table.cell_colors)
            .map(|(row, fills)| {
                let text = row
                    .iter()
                    .map(|cell| match cell {
                        StudyCell::Text(text) => text.clone(),
                        _ => panic!("text cells"),
                    })
                    .collect();
                (text, fills.clone())
            })
            .collect()
    }

    /// Every rule's strict thresholds, direction, signal line, history and trend requirements.
    #[test]
    fn votes_follow_the_documented_rules() {
        use Ordering::{Equal, Greater, Less};
        use Rule::*;
        let n = f64::NAN;
        let rows = [
            (Average, vec![vec![10.0]], 11.0, None, Some(1.0)),
            (Average, vec![vec![10.0]], 9.0, None, Some(-1.0)),
            (Average, vec![vec![10.0]], 10.0, None, Some(0.0)),
            (Average, vec![vec![n]], 11.0, None, None),
            (Rsi, vec![vec![20.0], vec![25.0]], n, None, Some(1.0)),
            (Rsi, vec![vec![80.0], vec![75.0]], n, None, Some(-1.0)),
            (Rsi, vec![vec![25.0], vec![20.0]], n, None, Some(0.0)),
            (Rsi, vec![vec![75.0], vec![80.0]], n, None, Some(0.0)),
            (Rsi, vec![vec![29.0], vec![30.0]], n, None, Some(0.0)),
            (Rsi, vec![vec![25.0]], n, None, None),
            (Stochastic, vec![vec![15.0, 10.0]], n, None, Some(1.0)),
            (Stochastic, vec![vec![85.0, 90.0]], n, None, Some(-1.0)),
            (Stochastic, vec![vec![10.0, 15.0]], n, None, Some(0.0)),
            (Stochastic, vec![vec![90.0, 85.0]], n, None, Some(0.0)),
            (Stochastic, vec![vec![15.0, 25.0]], n, None, Some(0.0)),
            (Cci, vec![vec![-120.0], vec![-110.0]], n, None, Some(1.0)),
            (Cci, vec![vec![120.0], vec![110.0]], n, None, Some(-1.0)),
            (Cci, vec![vec![-110.0], vec![-120.0]], n, None, Some(0.0)),
            (Cci, vec![vec![110.0], vec![120.0]], n, None, Some(0.0)),
            (
                Adx,
                vec![vec![25.0, n, n], vec![26.0, 30.0, 20.0]],
                n,
                None,
                Some(1.0),
            ),
            (
                Adx,
                vec![vec![26.0, n, n], vec![25.0, 20.0, 30.0]],
                n,
                None,
                Some(-1.0),
            ),
            (
                Adx,
                vec![vec![26.0, n, n], vec![25.0, 30.0, 20.0]],
                n,
                None,
                Some(0.0),
            ),
            (
                Adx,
                vec![vec![25.0, n, n], vec![26.0, 20.0, 30.0]],
                n,
                None,
                Some(0.0),
            ),
            (
                Adx,
                vec![vec![19.0, n, n], vec![19.5, 30.0, 20.0]],
                n,
                None,
                Some(0.0),
            ),
            (
                Awesome,
                vec![vec![1.0], vec![-1.0], vec![2.0]],
                n,
                None,
                Some(1.0),
            ),
            (
                Awesome,
                vec![vec![3.0], vec![1.0], vec![2.0]],
                n,
                None,
                Some(1.0),
            ),
            (
                Awesome,
                vec![vec![-1.0], vec![1.0], vec![-2.0]],
                n,
                None,
                Some(-1.0),
            ),
            (
                Awesome,
                vec![vec![-3.0], vec![-1.0], vec![-2.0]],
                n,
                None,
                Some(-1.0),
            ),
            (
                Awesome,
                vec![vec![1.0], vec![2.0], vec![3.0]],
                n,
                None,
                Some(0.0),
            ),
            (
                Awesome,
                vec![vec![0.0], vec![0.0], vec![0.0]],
                n,
                None,
                Some(0.0),
            ),
            (Awesome, vec![vec![-1.0], vec![2.0]], n, None, None),
            (Momentum, vec![vec![5.0], vec![6.0]], n, None, Some(1.0)),
            (Momentum, vec![vec![5.0], vec![4.0]], n, None, Some(-1.0)),
            (Momentum, vec![vec![5.0], vec![5.0]], n, None, Some(0.0)),
            (Macd, vec![vec![2.0, 1.0]], n, None, Some(1.0)),
            (Macd, vec![vec![1.0, 2.0]], n, None, Some(-1.0)),
            (Macd, vec![vec![1.0, 1.0]], n, None, Some(0.0)),
            (
                StochasticRsi,
                vec![vec![15.0, 10.0]],
                n,
                Some(Less),
                Some(1.0),
            ),
            (
                StochasticRsi,
                vec![vec![85.0, 90.0]],
                n,
                Some(Greater),
                Some(-1.0),
            ),
            (
                StochasticRsi,
                vec![vec![15.0, 10.0]],
                n,
                Some(Greater),
                Some(0.0),
            ),
            (
                StochasticRsi,
                vec![vec![85.0, 90.0]],
                n,
                Some(Less),
                Some(0.0),
            ),
            (
                StochasticRsi,
                vec![vec![15.0, 10.0]],
                n,
                Some(Equal),
                Some(0.0),
            ),
            (StochasticRsi, vec![vec![15.0, 10.0]], n, None, None),
            (
                WilliamsR,
                vec![vec![-85.0], vec![-82.0]],
                n,
                None,
                Some(1.0),
            ),
            (
                WilliamsR,
                vec![vec![-15.0], vec![-18.0]],
                n,
                None,
                Some(-1.0),
            ),
            (
                WilliamsR,
                vec![vec![-82.0], vec![-85.0]],
                n,
                None,
                Some(0.0),
            ),
            (
                WilliamsR,
                vec![vec![-18.0], vec![-15.0]],
                n,
                None,
                Some(0.0),
            ),
            (
                BullBearPower,
                vec![vec![1.0, -3.0], vec![1.0, -2.0]],
                n,
                Some(Greater),
                Some(1.0),
            ),
            (
                BullBearPower,
                vec![vec![3.0, 1.0], vec![2.0, 1.0]],
                n,
                Some(Less),
                Some(-1.0),
            ),
            (
                BullBearPower,
                vec![vec![1.0, -2.0], vec![1.0, -3.0]],
                n,
                Some(Greater),
                Some(0.0),
            ),
            (
                BullBearPower,
                vec![vec![1.0, -3.0], vec![1.0, -2.0]],
                n,
                Some(Less),
                Some(0.0),
            ),
            (
                BullBearPower,
                vec![vec![1.0, -3.0], vec![1.0, -2.0]],
                n,
                None,
                None,
            ),
            (
                BullBearPower,
                vec![vec![n, -3.0], vec![n, -2.0]],
                n,
                Some(Greater),
                None,
            ),
            (Ultimate, vec![vec![75.0]], n, None, Some(1.0)),
            (Ultimate, vec![vec![25.0]], n, None, Some(-1.0)),
            (Ultimate, vec![vec![70.0]], n, None, Some(0.0)),
        ];
        for (rule, rows, close, trend, expected) in rows {
            let mut values = IndicatorValues::new();
            for row in &rows {
                values.push_lines(row);
            }
            assert_eq!(
                rule.vote(&values, rows.len() - 1, close, trend),
                expected,
                "{rule:?}: {rows:?}, {trend:?}"
            );
        }
        // The cloud drawn at bar 25 comes from bar 0, never from bar 25.
        for (spans, lines, close, expected) in [
            ([3.0, 2.0], [6.0, 5.0, n, n, n], 7.0, Some(1.0)),
            ([5.0, 6.0], [3.0, 4.0, n, n, n], 2.0, Some(-1.0)),
            ([3.0, 4.0], [6.0, 5.0, 4.0, 3.0, n], 7.0, Some(0.0)),
        ] {
            let mut values = IndicatorValues::new();
            values.push_lines(&[n, n, spans[0], spans[1], n]);
            for _ in 1..25 {
                values.push_lines(&[n; 5]);
            }
            values.push_lines(&lines);
            assert_eq!(Rule::Ichimoku.vote(&values, 25, close, None), expected);
            assert_eq!(Rule::Ichimoku.vote(&values, 24, close, None), None);
        }
        let bars = bars(&[10.0, 11.0]);
        for (power, expected) in [(1.5, Greater), (2.0, Equal), (3.0, Less)] {
            let mut values = IndicatorValues::new();
            values.push_lines(&[1.0]);
            values.push_lines(&[power]);
            assert_eq!(trend(&values, &bars, 1), Some(expected));
            assert_eq!(trend(&values, &bars, 0), None);
        }
    }

    /// Hand fixtures: a flat start, 199 unit steps, then a three-unit step. At the last bar,
    /// HMA is 300.6 below close 302, the drawn cloud is 297 > 288.5 > 266.75 > 249.5,
    /// Momentum rises 10 to 12, MACD is 7.1595 > 7.0319 and UO is 82.87. ADX rises
    /// above 20 on both trends, so it buys the rise but does not sell the fall.
    /// A flat stretch has no CCI, and its constant RSI no Stochastic RSI, so their votes are
    /// missing.
    #[test]
    fn scores_coverage_and_ratings_follow_the_votes() {
        let fixture = |direction: f64| {
            let closes: Vec<_> = (0..260)
                .map(|i| {
                    let step = if i < 60 {
                        0.0
                    } else if i == 259 {
                        202.0
                    } else {
                        (i - 59) as f64
                    };
                    if direction < 0.0 {
                        400.0 - step
                    } else {
                        100.0 + direction * step
                    }
                })
                .collect();
            bars(&closes)
                .into_iter()
                .map(|mut bar| {
                    bar.high += 0.25;
                    bar.low -= 0.25;
                    bar
                })
                .collect::<Vec<_>>()
        };
        for (direction, oscillator, last_votes) in [
            (
                1.0,
                4.0 / 11.0,
                [0., 0., 0., 1., 0., 1., 1., 0., 0., 0., 1.],
            ),
            (
                -1.0,
                -3.0 / 11.0,
                [0., 0., 0., 0., 0., -1., -1., 0., 0., 0., -1.],
            ),
            (0.0, 0.0, [0.; 11]),
        ] {
            let mut last_votes = last_votes.map(Some);
            if direction == 0.0 {
                last_votes[2] = None;
                last_votes[7] = None;
            }
            let voted = last_votes.iter().flatten().count() as f64;
            let oscillator_coverage = voted / 11.;
            let bars = fixture(direction);
            for basis in [
                RatingBasis::All,
                RatingBasis::MovingAverages,
                RatingBasis::Oscillators,
            ] {
                let mut study = TechnicalRatings::new().with_basis(basis);
                study.calculate(&bars);
                let expected: Vec<_> = [direction; 15]
                    .into_iter()
                    .map(Some)
                    .chain(last_votes)
                    .collect();
                let got: Vec<_> =
                    TechnicalRatings::votes(study.components.as_ref().unwrap(), &study.bars, 259)
                        .map(|(_, vote)| vote)
                        .collect();
                assert_eq!(got, expected, "direction {direction}, {basis:?}");
                let summary = match basis {
                    RatingBasis::All => (direction + oscillator) / 2.0,
                    RatingBasis::MovingAverages => direction,
                    RatingBasis::Oscillators => oscillator,
                };
                let coverage = match basis {
                    RatingBasis::All => (15. + voted) / 26.,
                    RatingBasis::Oscillators => oscillator_coverage,
                    _ => 1.,
                };
                for (line, expected) in [
                    direction,
                    oscillator,
                    summary,
                    1.,
                    oscillator_coverage,
                    coverage,
                ]
                .into_iter()
                .enumerate()
                {
                    assert!(
                        (study.values.value(line, 259).unwrap() - expected).abs() < 1e-12,
                        "direction {direction}, {basis:?}, line {line}"
                    );
                }
                if basis == RatingBasis::Oscillators && direction != 0.0 {
                    let (label, fill) = if direction > 0.0 {
                        ("Buy", Color32::from_rgb(91, 156, 246))
                    } else {
                        ("Sell", Color32::from_rgb(239, 154, 154))
                    };
                    assert_eq!(
                        table(&study),
                        vec![(
                            ["Chart", label, "100%"].map(String::from).to_vec(),
                            vec![Color32::from_rgb(41, 91, 121), fill]
                        )]
                    );
                    let table = study.table().unwrap();
                    assert_eq!(
                        (table.header_color, table.text_color),
                        (Some(Color32::from_rgb(41, 91, 121)), Some(Color32::WHITE))
                    );
                }
            }
        }
        let bars = fixture(1.0);
        let mut full = TechnicalRatings::new();
        full.calculate(&bars);
        // Stochastic RSI waits for an RSI window with a range, which the flat start has not.
        for (at, expected) in [
            (0, [6. / 15., 0., 6. / 26.]),
            (34, [11. / 15., 8. / 11., 19. / 26.]),
            (35, [11. / 15., 9. / 11., 20. / 26.]),
            (75, [12. / 15., 1., 23. / 26.]),
            (76, [13. / 15., 1., 24. / 26.]),
            (198, [14. / 15., 1., 25. / 26.]),
            (199, [1.; 3]),
        ] {
            for (i, value) in expected.into_iter().enumerate() {
                assert_eq!(
                    full.values.value(i + 3, at),
                    Some(value),
                    "bar {at}, coverage {i}"
                );
            }
        }
        assert_eq!(full.values.value(1, 0), None);
        assert_eq!(full.values.value(2, 0), Some(0.0));
        let mut stream = TechnicalRatings::new();
        stream.calculate(&bars[..1]);
        for n in 2..=bars.len() {
            stream.update(&bars[..n], n - 1);
        }
        let mut missing = bars.clone();
        let mut bad = bars[100].clone();
        bad.close = f64::NAN;
        missing.insert(101, bad);
        let mut skipped = TechnicalRatings::new();
        skipped.calculate(&missing);
        for line in 0..6 {
            for at in 0..bars.len() {
                assert_eq!(
                    stream.values.value(line, at),
                    full.values.value(line, at),
                    "stream line {line}, bar {at}"
                );
            }
            assert_eq!(skipped.values.value(line, 101), None);
            assert_eq!(
                skipped.values.value(line, 260),
                full.values.value(line, 259)
            );
        }
        // Only VWMA reads volume: a missing volume removes its vote, while the other 25
        // components retain their price history. With no volume at all VWMA is the SMA.
        for volume in [f64::NAN, f64::INFINITY] {
            let mut missing_volume = bars.clone();
            missing_volume[259].volume = volume;
            let mut study = TechnicalRatings::new();
            study.calculate(&missing_volume);
            let votes: Vec<_> =
                TechnicalRatings::votes(study.components.as_ref().unwrap(), &study.bars, 259)
                    .map(|(_, vote)| vote)
                    .collect();
            let mut expected: Vec<_> =
                TechnicalRatings::votes(full.components.as_ref().unwrap(), &full.bars, 259)
                    .map(|(_, vote)| vote)
                    .collect();
            expected[13] = None;
            assert_eq!(votes, expected);
            for (line, value) in [
                (0, 1.0),
                (1, 4.0 / 11.0),
                (2, 15.0 / 22.0),
                (3, 14.0 / 15.0),
                (4, 1.0),
                (5, 25.0 / 26.0),
            ] {
                assert!((study.values.value(line, 259).unwrap() - value).abs() < 1e-12);
            }
        }
        let mut no_volume = bars.clone();
        for bar in &mut no_volume {
            bar.volume = 0.0;
        }
        let mut study = TechnicalRatings::new();
        study.calculate(&no_volume);
        for line in 0..6 {
            for at in 0..bars.len() {
                assert_eq!(study.values.value(line, at), full.values.value(line, at));
            }
        }
        let n = f64::NAN;
        for (sum, votes, basis, expected) in [
            (
                [0., 1.],
                [3, 1],
                RatingBasis::All,
                [0., 1., 0.5, 0.2, 1. / 11., 4. / 26.],
            ),
            // 0.8 and -0.6 average to exactly 0.1, rated Neutral.
            (
                [4., -3.],
                [5, 5],
                RatingBasis::All,
                [0.8, -0.6, 0.1, 5. / 15., 5. / 11., 10. / 26.],
            ),
            (
                [0., 1.],
                [3, 1],
                RatingBasis::MovingAverages,
                [0., 1., 0., 0.2, 1. / 11., 0.2],
            ),
            (
                [0., 1.],
                [3, 1],
                RatingBasis::Oscillators,
                [0., 1., 1., 0.2, 1. / 11., 1. / 11.],
            ),
            (
                [0., -2.],
                [0, 4],
                RatingBasis::All,
                [n, -0.5, -0.5, 0., 4. / 11., 4. / 26.],
            ),
            ([0., 0.], [0, 0], RatingBasis::All, [n, n, n, 0., 0., 0.]),
        ] {
            for (got, want) in TechnicalRatings::scores(sum, votes, [15, 11], basis)
                .into_iter()
                .zip(expected)
            {
                assert!(
                    got == want || (got.is_nan() && want.is_nan()),
                    "{basis:?}: {got} != {want}"
                );
            }
        }
        for (score, label) in [
            (0.75, "Strong Buy"),
            (0.5, "Buy"),
            (0.1, "Neutral"),
            (0., "Neutral"),
            (-0.1, "Neutral"),
            (-0.5, "Sell"),
            (-0.75, "Strong Sell"),
        ] {
            assert_eq!(rating(score), label, "{score}");
        }
    }

    /// Table slots choose unique intervals by label, including shorter ones. Two closes
    /// 10 and 12 put all six seeded EMAs below the last close: 6/15 averages, 6/26 overall.
    #[test]
    fn table_rates_the_chosen_intervals() {
        use InputValue::{Choice, Color, Timeframe as Interval, Toggle};
        let mut study = TechnicalRatings::default();
        study.set_timeframe(Timeframe::Hour1);
        assert!(study.data_requests().is_empty());
        for (id, value) in [
            ("table_intervals", Toggle(true)),
            ("table_timeframes [2]", Interval(Some(Timeframe::Hour4))),
            ("table_timeframes [3]", Interval(Some(Timeframe::Min5))),
            ("table_timeframes [4]", Interval(None)),
            ("table_strong_sell_color", Color(Color32::GOLD)),
        ] {
            study.set_input(id, value).unwrap();
        }
        let request = |timeframe| StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(timeframe),
            metric: None,
        };
        let requests = [Timeframe::Hour4, Timeframe::Min5].map(request);
        assert_eq!(study.data_requests(), requests);
        // A custom duration with the same label is the same interval too.
        study
            .set_input(
                "table_timeframes [2]",
                Interval(Some(Timeframe::Custom(
                    std::num::NonZeroU64::new(4 * 3600).unwrap(),
                ))),
            )
            .unwrap();
        assert_eq!(study.data_requests(), requests);
        let (rising, falling) = (bars(&[10., 12.]), bars(&[10., 8.]));
        let source = |request: &StudyDataRequest, bars| StudyData {
            request: request.clone(),
            bars,
            clock: BarClock::new(request.timeframe.unwrap()),
            instrument: None,
            timeframe: request.timeframe.unwrap(),
            chart: None,
        };
        let sources = [
            source(&requests[0], &falling[..]),
            source(&requests[1], &[][..]),
        ];
        study.update_data(&rising, 0, &sources);
        let h = Color32::from_rgb(41, 91, 121);
        let n = palette::DISABLED;
        let b = palette::BLUE;
        let row = |texts: &[&str], fills: &[Color32]| {
            (
                texts
                    .iter()
                    .map(|text| text.to_string())
                    .collect::<Vec<_>>(),
                fills.to_vec(),
            )
        };
        assert_eq!(
            study.table().unwrap().columns,
            [
                "Interval",
                "Moving averages",
                "Oscillators",
                "Summary",
                "Coverage"
            ]
        );
        assert_eq!(
            table(&study),
            vec![
                row(
                    &["1h", "Strong Buy", "N/A", "Strong Buy", "23%"],
                    &[h, b, n, b]
                ),
                row(
                    &["4h", "Strong Sell", "N/A", "Strong Sell", "23%"],
                    &[h, Color32::GOLD, n, Color32::GOLD]
                ),
                row(&["5min", "N/A", "N/A", "N/A", "N/A"], &[h, n, n, n]),
            ]
        );
        assert_eq!(
            study.data_start(&requests[0]),
            Some(rising[1].time - TimeDelta::hours(8000))
        );
        assert_eq!(
            study.data_start(&requests[1]),
            Some(rising[1].time - TimeDelta::minutes(10000))
        );
        assert_eq!(study.data_start(&request(Timeframe::Day1)), None);
        for (basis, label, rating, coverage, fill) in [
            (1, "Moving averages", "Strong Buy", "40%", b),
            (2, "Oscillators", "N/A", "0%", n),
        ] {
            study.set_input("basis", Choice(basis)).unwrap();
            study.update_data(&rising, 0, &sources);
            assert_eq!(
                study.table().unwrap().columns,
                ["Interval", label, "Coverage"]
            );
            let other = if basis == 1 { "Strong Sell" } else { "N/A" };
            assert_eq!(
                table(&study),
                vec![
                    row(&["1h", rating, coverage], &[h, fill]),
                    row(
                        &["4h", other, coverage],
                        &[h, if basis == 1 { Color32::GOLD } else { n }]
                    ),
                    row(&["5min", "N/A", "N/A"], &[h, n]),
                ]
            );
        }
        // The custom duplicate must also be cleared before requests can become empty.
        for slot in [1, 2, 3] {
            study
                .set_input(&format!("table_timeframes [{slot}]"), Interval(None))
                .unwrap();
        }
        assert!(study.data_requests().is_empty());
        study.calculate(&rising);
        assert_eq!(table(&study), vec![row(&["1h", "N/A", "0%"], &[h, n])]);
        // A trailing unusable bar has no latest rating, even if an earlier bar is usable.
        let mut missing = rising.clone();
        missing[1].close = f64::NAN;
        assert!(study.latest(&missing).is_none());
    }
}
