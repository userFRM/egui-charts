//! Technical Ratings.
//!
//! Votes of moving averages and oscillators, averaged into a moving-average score, an
//! oscillator score and a summary score between -1 (all sell) and +1 (all buy), each with the
//! share of its components that had a vote.
//!
//! # Rule
//!
//! Every enabled component votes +1 (buy), -1 (sell) or 0 (neutral) on a bar:
//!
//! ```text
//! Moving average (SMA and EMA of three lengths):  sign(close - average)
//! RSI:                  +1 below 30, -1 above 70
//! Stochastic %K:        +1 below 20, -1 above 80
//! CCI:                  +1 below -100, -1 above 100
//! MACD:                 sign(MACD line - signal line)
//! Momentum:             sign(momentum)
//! ```
//!
//! Each component counts once, whatever its group. A group's score is the mean of the votes it
//! has, the summary is the mean of all votes, and a score without a single vote has no value.
//! A component without a value, in its warm-up (for the MACD the first max(slow, signal) bars)
//! or on a bar with a non-finite high, low or close, has no vote: it leaves the mean and lowers
//! the coverage, the votes it has over the components enabled. A rating is derived from a score:
//!
//! ```text
//! Strong Buy   score >= 0.5
//! Buy          0.1 <= score < 0.5
//! Neutral      -0.1 < score < 0.1
//! Sell         -0.5 < score <= -0.1
//! Strong Sell  score <= -0.5
//! ```
//!
//! The six outputs are the moving-average, oscillator and summary scores followed by their
//! coverages; the coverages are hidden by default. The rating basis decides which groups the
//! summary averages: both by default, or only the moving averages or only the oscillators.
//!
//! The table shows the latest bar's score, rating and coverage of each group the basis uses,
//! each row on the background of its rating. With higher intervals shown, the study requests the
//! chart symbol's 1-minute, 5-minute, 15-minute, 30-minute, 1-hour, 4-hour, daily, weekly and
//! monthly bars longer than the chart interval, rates the latest bar of each with the same
//! components and adds a row with its summary score, rating and coverage.

use crate::model::{Bar, Timeframe};
use crate::studies::palette;
use crate::studies::{
    CCI, EMA, Indicator, IndicatorValue, IndicatorValues, MACD, Momentum, RSI, SMA, Stochastic,
    StudyCell, StudyData, StudyDataRequest, StudyTable, TablePosition, TableTextSize,
};
use egui::Color32;

/// The intervals rated in the table with higher intervals shown, where longer than the chart's.
const INTERVALS: [Timeframe; 9] = [
    Timeframe::Min1,
    Timeframe::Min5,
    Timeframe::Min15,
    Timeframe::Min30,
    Timeframe::Hour1,
    Timeframe::Hour4,
    Timeframe::Day1,
    Timeframe::Week1,
    Timeframe::Month1,
];

/// The groups whose votes the summary score averages. The `Debug` names are the choices its
/// input lists.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RatingBasis {
    /// Every vote of the moving averages and the oscillators.
    #[default]
    All,
    /// The moving averages' votes only.
    MovingAverages,
    /// The oscillators' votes only.
    Oscillators,
}

impl std::fmt::Debug for RatingBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All",
            Self::MovingAverages => "Moving averages",
            Self::Oscillators => "Oscillators",
        })
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
}

/// Group of a component, in the order of the score outputs.
#[derive(Clone, Copy, PartialEq)]
enum Group {
    MovingAverage,
    Oscillator,
}

#[derive(Clone, Copy)]
enum Rule {
    Average,
    Rsi,
    Stochastic,
    Cci,
    Macd,
    Momentum,
}

impl Rule {
    /// The vote of a component with `values` at clean bar `at`, or `None` without one.
    fn vote(self, values: &IndicatorValues, at: usize, close: f64) -> Option<f64> {
        let signed = |difference: f64| {
            if difference > 0.0 {
                1.0
            } else if difference < 0.0 {
                -1.0
            } else {
                0.0
            }
        };
        let level = |low: f64, high: f64| {
            let value = values.value(0, at)?;
            Some(if value < low {
                1.0
            } else if value > high {
                -1.0
            } else {
                0.0
            })
        };
        match self {
            Self::Average => Some(signed(close - values.value(0, at)?)),
            Self::Rsi => level(30.0, 70.0),
            Self::Stochastic => level(20.0, 80.0),
            Self::Cci => level(-100.0, 100.0),
            Self::Macd => Some(signed(values.value(0, at)? - values.value(1, at)?)),
            Self::Momentum => Some(signed(values.value(0, at)?)),
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

/// Moving-average, oscillator and summary ratings from the votes of their components.
#[derive(Clone)]
pub struct TechnicalRatings {
    fast: usize,
    medium: usize,
    slow: usize,
    simple: bool,
    exponential: bool,
    rsi: bool,
    rsi_period: usize,
    stochastic: bool,
    stochastic_period: usize,
    cci: bool,
    cci_period: usize,
    macd: bool,
    macd_fast: usize,
    macd_slow: usize,
    macd_signal: usize,
    momentum: bool,
    momentum_period: usize,
    basis: RatingBasis,
    table_intervals: bool,
    table_position: TablePosition,
    table_size: TableTextSize,
    table_strong_buy_color: Color32,
    table_buy_color: Color32,
    table_neutral_color: Color32,
    table_sell_color: Color32,
    table_strong_sell_color: Color32,
    table_header_color: Color32,
    table_text_color: Color32,
    /// The chart interval, which decides the higher intervals requested.
    timeframe: Option<Timeframe>,
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
    /// Moving averages of the three lengths, each raised to at least 1, and the conventional
    /// oscillators.
    pub fn new(fast: usize, medium: usize, slow: usize) -> Self {
        Self {
            fast: fast.max(1),
            medium: medium.max(1),
            slow: slow.max(1),
            simple: true,
            exponential: true,
            rsi: true,
            rsi_period: 14,
            stochastic: true,
            stochastic_period: 14,
            cci: true,
            cci_period: 20,
            macd: true,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
            momentum: true,
            momentum_period: 10,
            basis: RatingBasis::All,
            table_intervals: false,
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
            intervals: Vec::new(),
            components: None,
            bars: Vec::new(),
            sources: Vec::new(),
            values: IndicatorValues::new(),
            table: StudyTable {
                title: "Technical Ratings".into(),
                columns: ["Group", "Score", "Rating", "Coverage"]
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

    /// The summary score averages the votes of the groups `basis` chooses.
    pub fn with_basis(mut self, basis: RatingBasis) -> Self {
        self.basis = basis;
        self.components = None;
        self
    }

    fn build(&self) -> Vec<Component> {
        let mut components = Vec::new();
        let mut add = |group, rule, warmup, study: Box<dyn Indicator>| {
            components.push(Component {
                group,
                rule,
                warmup,
                study,
            });
        };
        for length in [self.fast, self.medium, self.slow] {
            if self.simple {
                add(
                    Group::MovingAverage,
                    Rule::Average,
                    0,
                    Box::new(SMA::new(length)),
                );
            }
            if self.exponential {
                add(
                    Group::MovingAverage,
                    Rule::Average,
                    0,
                    Box::new(EMA::new(length)),
                );
            }
        }
        let oscillators: [(bool, Rule, usize, Box<dyn Indicator>); 5] = [
            (self.rsi, Rule::Rsi, 0, Box::new(RSI::new(self.rsi_period))),
            (
                self.stochastic,
                Rule::Stochastic,
                0,
                Box::new(Stochastic::new(self.stochastic_period, 3, 3)),
            ),
            (self.cci, Rule::Cci, 0, Box::new(CCI::new(self.cci_period))),
            // The MACD shows no value before its slow and signal averages are seeded, then
            // shows all of its history; the vote starts at that bar so that it never changes.
            (
                self.macd,
                Rule::Macd,
                self.macd_slow.max(self.macd_signal) - 1,
                Box::new(MACD::new(self.macd_fast, self.macd_slow, self.macd_signal)),
            ),
            (
                self.momentum,
                Rule::Momentum,
                0,
                Box::new(Momentum::new(self.momentum_period)),
            ),
        ];
        for (enabled, rule, warmup, study) in oscillators {
            if enabled {
                add(Group::Oscillator, rule, warmup, study);
            }
        }
        components
    }

    /// The scores and coverages on clean bar `at`, the summary of the groups `basis` chooses.
    fn row(components: &[Component], at: usize, close: f64, basis: RatingBasis) -> [f64; 6] {
        // Votes and enabled components of the moving averages, the oscillators and in total.
        let mut sum = [0.0; 2];
        let mut votes = [0usize; 2];
        let mut enabled = [0usize; 2];
        for component in components {
            let group = usize::from(component.group == Group::Oscillator);
            enabled[group] += 1;
            if let Some(vote) = (at >= component.warmup)
                .then(|| component.rule.vote(component.study.values(), at, close))
                .flatten()
            {
                sum[group] += vote;
                votes[group] += 1;
            }
        }
        let score = |sum: f64, votes: usize| {
            if votes == 0 {
                f64::NAN
            } else {
                sum / votes as f64
            }
        };
        let coverage = |votes: usize, enabled: usize| {
            if enabled == 0 {
                f64::NAN
            } else {
                votes as f64 / enabled as f64
            }
        };
        let chosen = basis.groups();
        [
            score(sum[0], votes[0]),
            score(sum[1], votes[1]),
            score(
                sum[chosen.clone()].iter().sum(),
                votes[chosen.clone()].iter().sum(),
            ),
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
        let bars: Vec<_> = bars.iter().filter(|bar| usable(bar)).cloned().collect();
        let at = bars.len().checked_sub(1)?;
        let mut components = self.build();
        for component in &mut components {
            component.study.calculate(&bars);
        }
        Some(Self::row(&components, at, bars[at].close, self.basis))
    }

    /// The table rows of the latest bar and of the requested intervals, on their ratings'
    /// backgrounds.
    fn fill_table(&mut self) {
        let latest = self.values.len().checked_sub(1);
        let at = |line: usize| latest.and_then(|bar| self.values.value(line, bar));
        let groups: &[usize] = match self.basis {
            RatingBasis::All => &[0, 1, 2],
            RatingBasis::MovingAverages => &[0],
            RatingBasis::Oscillators => &[1],
        };
        let labels = ["Moving averages", "Oscillators", "Summary"];
        let mut rows: Vec<_> = groups
            .iter()
            .map(|&group| (labels[group].to_string(), at(group), at(group + 3)))
            .collect();
        if self.table_intervals {
            let finite = |value: f64| value.is_finite().then_some(value);
            rows.extend(self.intervals.iter().map(|(timeframe, latest)| {
                (
                    timeframe.as_str().into_owned(),
                    latest.and_then(|row| finite(row[2])),
                    latest.and_then(|row| finite(row[5])),
                )
            }));
        }
        self.table.cell_colors = rows
            .iter()
            .map(|(_, score, _)| match score.map_or("Neutral", rating) {
                "Strong Buy" => self.table_strong_buy_color,
                "Buy" => self.table_buy_color,
                "Sell" => self.table_sell_color,
                "Strong Sell" => self.table_strong_sell_color,
                _ => self.table_neutral_color,
            })
            .collect();
        self.table.rows = rows
            .into_iter()
            .map(|(label, score, coverage)| {
                vec![
                    StudyCell::Text(label),
                    StudyCell::Text(score.map_or("N/A".into(), |score| format!("{score:+.2}"))),
                    StudyCell::Text(score.map_or("N/A", rating).into()),
                    StudyCell::Text(
                        coverage
                            .map_or("N/A".into(), |coverage| format!("{:.0}%", coverage * 100.0)),
                    ),
                ]
            })
            .collect();
        self.table.header_color = Some(self.table_header_color);
        self.table.text_color = Some(self.table_text_color);
        self.table.position = self.table_position;
        self.table.text_size = self.table_size;
    }
}

/// Whether every component can read `bar`.
fn usable(bar: &Bar) -> bool {
    bar.high.is_finite() && bar.low.is_finite() && bar.close.is_finite()
}

/// Construct with the conventional default parameters.
impl Default for TechnicalRatings {
    fn default() -> Self {
        Self::new(10, 50, 200)
    }
}

/// The displayed rating of a score between -1 and 1.
fn rating(score: f64) -> &'static str {
    if score >= 0.5 {
        "Strong Buy"
    } else if score >= 0.1 {
        "Buy"
    } else if score > -0.1 {
        "Neutral"
    } else if score > -0.5 {
        "Sell"
    } else {
        "Strong Sell"
    }
}

impl Indicator for TechnicalRatings {
    crate::studies::inputs::study_inputs!(self;
        fast: length("Fast moving average", 1, usize::MAX),
        medium: length("Medium moving average", 1, usize::MAX),
        slow: length("Slow moving average", 1, usize::MAX),
        simple: toggle("Simple moving averages"),
        exponential: toggle("Exponential moving averages"),
        rsi: toggle("RSI"),
        rsi_period: length("RSI period", 1, usize::MAX),
        stochastic: toggle("Stochastic"),
        stochastic_period: length("Stochastic period", 1, usize::MAX),
        cci: toggle("CCI"),
        cci_period: length("CCI period", 1, usize::MAX),
        macd: toggle("MACD"),
        macd_fast: length("MACD fast period", 1, usize::MAX),
        macd_slow: length("MACD slow period", 1, usize::MAX),
        macd_signal: length("MACD signal period", 1, usize::MAX),
        momentum: toggle("Momentum"),
        momentum_period: length("Momentum period", 1, usize::MAX),
        basis: choice("Rating basis", [RatingBasis::All, RatingBasis::MovingAverages, RatingBasis::Oscillators]),
        table_intervals: toggle("Table shows higher intervals"),
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

    /// The chart symbol's bars of each higher interval, with higher intervals shown in the table.
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        if !self.table_intervals {
            return Vec::new();
        }
        let chart = self
            .timeframe
            .map_or(0, |timeframe| timeframe.duration_ms());
        INTERVALS
            .into_iter()
            .filter(|timeframe| timeframe.duration_ms() > chart)
            .map(|timeframe| StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(timeframe),
                metric: None,
            })
            .collect()
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
        let mut from = from.min(self.values.len()).min(data.len());
        if self.components.is_none() {
            self.components = Some(self.build());
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
                    .push_lines(&Self::row(components, at, bar.close, self.basis));
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
            visible: line < 3,
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
    use chrono::{DateTime, Duration};

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

    /// Ratings of three SMA(2), three EMA(2) and Momentum(1), the other oscillators off.
    fn study(basis: usize) -> TechnicalRatings {
        let mut study = TechnicalRatings::default();
        for (id, value) in [
            ("fast", InputValue::Length(2)),
            ("medium", InputValue::Length(2)),
            ("slow", InputValue::Length(2)),
            ("momentum_period", InputValue::Length(1)),
            ("rsi", InputValue::Toggle(false)),
            ("stochastic", InputValue::Toggle(false)),
            ("cci", InputValue::Toggle(false)),
            ("macd", InputValue::Toggle(false)),
            ("basis", InputValue::Choice(basis)),
            ("table_strong_sell_color", InputValue::Color(Color32::GOLD)),
        ] {
            study.set_input(id, value).unwrap();
        }
        study
    }

    /// The label, score, rating and coverage of each table row, with its background.
    fn table(study: &TechnicalRatings) -> Vec<([String; 4], Color32)> {
        let table = study.table().unwrap();
        assert_eq!(table.cell_colors.len(), table.rows.len());
        table
            .rows
            .iter()
            .zip(&table.cell_colors)
            .map(|(row, &fill)| {
                let text = |cell: &StudyCell| match cell {
                    StudyCell::Text(text) => text.clone(),
                    _ => panic!("text cells"),
                };
                (std::array::from_fn(|column| text(&row[column])), fill)
            })
            .collect()
    }

    /// Scores, coverages, ratings and row backgrounds of each rating basis, by hand.
    ///
    /// Closes 10, 12, a bar without a close and 11.4. Every component skips the bar without a
    /// close, which has no value. SMA(2) is 11 and 11.7 on the last two rated bars; EMA(2) seeds
    /// with 10 and moves 2/3 of the way: 34/3 and 34/9 + 22.8/3 = 11.3778. Momentum(1) is +2 and
    /// -0.6. The first bar has no SMA or momentum vote, which lowers its coverage, and the
    /// three EMAs equal the close there and vote neutral. On the second bar all seven components
    /// buy. On the last the SMAs sell and the EMAs buy, a tie, and momentum sells.
    #[test]
    fn scores_coverage_and_ratings_follow_the_votes() {
        let bars = bars(&[10.0, 12.0, f64::NAN, 11.4]);
        let nan = f64::NAN;
        let row = |label: &str, score: &str, rating: &str, fill| {
            ([label, score, rating, "100%"].map(String::from), fill)
        };
        let sell = Color32::from_rgb(239, 154, 154);
        for (basis, expected, rows) in [
            (
                RatingBasis::All,
                [
                    [0.0, nan, 0.0, 0.5, 0.0, 3.0 / 7.0],
                    [1.0; 6],
                    [nan; 6],
                    [0.0, -1.0, -1.0 / 7.0, 1.0, 1.0, 1.0],
                ],
                vec![
                    row("Moving averages", "+0.00", "Neutral", palette::DISABLED),
                    row("Oscillators", "-1.00", "Strong Sell", Color32::GOLD),
                    row("Summary", "-0.14", "Sell", sell),
                ],
            ),
            (
                RatingBasis::MovingAverages,
                [
                    [0.0, nan, 0.0, 0.5, 0.0, 0.5],
                    [1.0; 6],
                    [nan; 6],
                    [0.0, -1.0, 0.0, 1.0, 1.0, 1.0],
                ],
                vec![row(
                    "Moving averages",
                    "+0.00",
                    "Neutral",
                    palette::DISABLED,
                )],
            ),
            (
                RatingBasis::Oscillators,
                [
                    [0.0, nan, nan, 0.5, 0.0, 0.0],
                    [1.0; 6],
                    [nan; 6],
                    [0.0, -1.0, -1.0, 1.0, 1.0, 1.0],
                ],
                vec![row("Oscillators", "-1.00", "Strong Sell", Color32::GOLD)],
            ),
        ] {
            let mut study = study(basis as usize);
            study.calculate(&bars);
            for (bar, row) in expected.iter().enumerate() {
                for (line, want) in row.iter().enumerate() {
                    match study.values().value(line, bar) {
                        Some(got) => {
                            assert!(
                                (got - want).abs() < 1e-12,
                                "{basis:?} line {line} bar {bar}"
                            )
                        }
                        None => assert!(want.is_nan(), "{basis:?} line {line} bar {bar}"),
                    }
                }
            }
            assert_eq!(table(&study), rows, "{basis:?}");
            let header = study.table().unwrap();
            assert_eq!(
                (header.header_color, header.text_color),
                (Some(Color32::from_rgb(41, 91, 121)), Some(Color32::WHITE))
            );
        }
        // A correction of the latest close to 9: SMA(2) 10.5 and EMA(2) 34/3 - 2/3 * 7/3 =
        // 9.7778 are above it and momentum is -3, so every component sells.
        let mut corrected = study(0);
        corrected.calculate(&bars);
        let mut bars = bars;
        bars[3].close = 9.0;
        corrected.update(&bars, 3);
        for line in 0..6 {
            let want = if line < 3 { -1.0 } else { 1.0 };
            assert_eq!(corrected.values().value(line, 3), Some(want), "line {line}");
        }
        assert_eq!(
            table(&corrected)[2],
            row("Summary", "-1.00", "Strong Sell", Color32::GOLD)
        );
        for (score, label) in [
            (0.5, "Strong Buy"),
            (0.1, "Buy"),
            (0.09, "Neutral"),
            (-0.09, "Neutral"),
            (-0.1, "Sell"),
            (-0.49, "Sell"),
            (-0.5, "Strong Sell"),
        ] {
            assert_eq!(rating(score), label, "{score}");
        }
    }

    /// Higher intervals add one row each, rated from their own bars, and never repeat the chart
    /// interval. Closes 10 and 12 make every component buy; 10, 12 and 11.4 rate as above.
    #[test]
    fn higher_intervals_add_rated_rows() {
        let mut study = study(0);
        study.set_timeframe(Timeframe::Hour1);
        assert!(study.data_requests().is_empty());
        study
            .set_input("table_intervals", InputValue::Toggle(true))
            .unwrap();
        let request = |timeframe| StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(timeframe),
            metric: None,
        };
        let requests = [
            Timeframe::Hour4,
            Timeframe::Day1,
            Timeframe::Week1,
            Timeframe::Month1,
        ]
        .map(request);
        assert_eq!(study.data_requests(), requests);
        let (rising, turning) = (bars(&[10.0, 12.0]), bars(&[10.0, 12.0, 11.4]));
        let source = |request: &StudyDataRequest, bars| StudyData {
            request: request.clone(),
            bars,
            clock: BarClock::new(request.timeframe.unwrap()),
            instrument: None,
            timeframe: request.timeframe.unwrap(),
        };
        let sources = [
            source(&requests[0], &rising[..]),
            source(&requests[1], &turning[..]),
            source(&requests[2], &[][..]),
        ];
        study.update_data(&rising, 0, &sources);
        let missing = |label: &str| {
            (
                [label, "N/A", "N/A", "N/A"].map(String::from),
                palette::DISABLED,
            )
        };
        assert_eq!(
            table(&study)[3..],
            [
                (
                    ["4h", "+1.00", "Strong Buy", "100%"].map(String::from),
                    palette::BLUE
                ),
                (
                    ["1D", "-0.14", "Sell", "100%"].map(String::from),
                    Color32::from_rgb(239, 154, 154)
                ),
                missing("1W"),
                missing("1M"),
            ]
        );
    }
}
