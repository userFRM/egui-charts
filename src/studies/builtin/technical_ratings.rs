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
//! or on a bar with a non-finite high, low or close, has no vote: it leaves the mean and lowers the coverage, the votes it has over the
//! components enabled. A rating is derived from a score:
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
//! coverages; the coverages are hidden by default. The table shows the latest bar's score, rating
//! and coverage of each group.

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{
    CCI, EMA, Indicator, IndicatorValue, IndicatorValues, MACD, Momentum, RSI, SMA, Stochastic,
    StudyCell, StudyTable,
};
use egui::Color32;

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
    components: Option<Vec<Component>>,
    /// The bars with a finite high, low and close, which every component reads, and their
    /// positions in the input.
    bars: Vec<Bar>,
    sources: Vec<usize>,
    values: IndicatorValues,
    table: StudyTable,
    colors: [Color32; 3],
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
            components: None,
            bars: Vec::new(),
            sources: Vec::new(),
            values: IndicatorValues::new(),
            table: StudyTable {
                title: "Technical Ratings".into(),
                columns: ["Group", "Score", "Rating", "Coverage"]
                    .map(String::from)
                    .to_vec(),
                rows: Vec::new(),
            },
            colors: [palette::ACCENT, palette::ORANGE, palette::TEAL],
            visible: true,
        }
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

    /// The scores and coverages on clean bar `at`.
    fn row(components: &[Component], at: usize, close: f64) -> [f64; 6] {
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
        [
            score(sum[0], votes[0]),
            score(sum[1], votes[1]),
            score(sum[0] + sum[1], votes[0] + votes[1]),
            coverage(votes[0], enabled[0]),
            coverage(votes[1], enabled[1]),
            coverage(votes[0] + votes[1], enabled[0] + enabled[1]),
        ]
    }
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
        momentum_period: length("Momentum period", 1, usize::MAX)
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
        let usable =
            |bar: &Bar| bar.high.is_finite() && bar.low.is_finite() && bar.close.is_finite();
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
                    .push_lines(&Self::row(components, at, bar.close));
                at += 1;
            } else {
                self.values.push(IndicatorValue::None);
            }
        }
        let latest = self.values.len().checked_sub(1);
        self.table.rows = ["Moving averages", "Oscillators", "Summary"]
            .into_iter()
            .enumerate()
            .map(|(group, label)| {
                let at = |line: usize| latest.and_then(|bar| self.values.value(line, bar));
                let (score, coverage) = (at(group), at(group + 3));
                vec![
                    StudyCell::Text(label.into()),
                    StudyCell::Text(score.map_or("N/A".into(), |score| format!("{score:+.2}"))),
                    StudyCell::Text(score.map_or("N/A", rating).into()),
                    StudyCell::Text(
                        coverage
                            .map_or("N/A".into(), |coverage| format!("{:.0}%", coverage * 100.0)),
                    ),
                ]
            })
            .collect();
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
            plot_type: crate::studies::PlotType::Line,
            visible: line < 3,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = Color32::from_gray(128);
        crate::studies::StudyPane {
            height: None,
            range: Some((-1.1, 1.1)),
            levels: vec![
                (0.5, gray, "Strong Buy"),
                (0.1, gray, "Buy"),
                (-0.1, gray, "Sell"),
                (-0.5, gray, "Strong Sell"),
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
    use crate::studies::InputValue;
    use chrono::{DateTime, Duration};

    /// Scores, coverages and ratings of two moving-average lengths and momentum, by hand.
    ///
    /// Closes 10, 12 and 11.4. SMA(2) is 11 and 11.7 on the last two bars; EMA(2) seeds with 10 and
    /// moves 2/3 of the way: 34/3 and 34/9 + 22.8/3 = 11.3778. Momentum(1) is +2 and -0.6.
    #[test]
    fn scores_coverage_and_ratings_follow_the_votes() {
        let bars: Vec<_> = [10.0, 12.0, 11.4]
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                Bar::new(
                    DateTime::UNIX_EPOCH + Duration::days(i as i64),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect();
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
        ] {
            study.set_input(id, value).unwrap();
        }
        study.calculate(&bars);
        // Bar 0: no SMA or momentum yet; the three EMAs equal the close and vote neutral.
        // Bar 1: all seven components buy. Bar 2: SMAs sell, EMAs buy, momentum sells.
        let expected = [
            [0.0, f64::NAN, 0.0, 0.5, 0.0, 3.0 / 7.0],
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [0.0, -1.0, -1.0 / 7.0, 1.0, 1.0, 1.0],
        ];
        for (bar, row) in expected.iter().enumerate() {
            for (line, want) in row.iter().enumerate() {
                match study.values().value(line, bar) {
                    Some(got) => assert!((got - want).abs() < 1e-12, "line {line} bar {bar}"),
                    None => assert!(want.is_nan(), "line {line} bar {bar}"),
                }
            }
        }
        let rows: Vec<Vec<_>> = study
            .table()
            .unwrap()
            .rows
            .iter()
            .map(|row| match &row[..] {
                [
                    _,
                    StudyCell::Text(score),
                    StudyCell::Text(rating),
                    StudyCell::Text(coverage),
                ] => {
                    vec![score.clone(), rating.clone(), coverage.clone()]
                }
                _ => panic!("text cells"),
            })
            .collect();
        assert_eq!(
            rows,
            [
                ["+0.00", "Neutral", "100%"],
                ["-1.00", "Strong Sell", "100%"],
                ["-0.14", "Sell", "100%"],
            ]
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
}
