//! All Candlestick Patterns
//!
//! Detects the common Japanese candlestick patterns described by Steve Nison and Thomas
//! Bulkowski, filtered by the prevailing trend, and marks each bar where one completes.
//!
//! # Definitions
//!
//! Each bar has a body from `min(open, close)` to `max(open, close)`, an upper shadow
//! `high - max(open, close)`, a lower shadow `min(open, close) - low` and a range
//! `high - low`. The body average is the EMA of the body over 14 bars, seeded with their
//! SMA. A body is long above the average and small below it. A bar is white when its close
//! is above its open and black when below. A doji body is at most 5% of a nonzero range;
//! a doji has a doji body and shadows within twice each other's length. A shadow is
//! present when it exceeds 5% of the body. Prices are equal when they differ by at most
//! 5% of the body average.
//!
//! The trend rule decides whether a bar is in a downtrend or an uptrend: `Sma50` compares
//! the close with SMA(close, 50) (below is down, above is up), `Sma50Sma200` also requires
//! SMA(close, 50) to be below or above SMA(close, 200), and `NoDetection` treats every bar
//! as both. Before the averages exist no bar is in either trend. A pattern that requires a
//! trend checks it on the bar named below.
//!
//! | Abbr. | Pattern | Side | Rule (`b` the bar, `p1`, `p2`, ... the bars before it) |
//! |---|---|---|---|
//! | AB | Abandoned Baby | bearish | `p2` long white in an uptrend, `p1` a doji body whose low gaps above `p2`'s high, `b` black whose high gaps below `p1`'s low |
//! | AB | Abandoned Baby | bullish | mirror: `p2` long black in a downtrend, `p1` doji body gapping below, `b` white gapping above |
//! | DCC | Dark Cloud Cover | bearish | `p1` long white in an uptrend; `b` black, opening at or above `p1`'s high, closing below the middle of `p1`'s body and above its open |
//! | D | Doji | neutral | a doji that is neither a dragonfly nor a gravestone doji |
//! | DS | Doji Star | bearish | `b` in an uptrend; `p1` long white; `b` a doji body gapping above `p1`'s body |
//! | DS | Doji Star | bullish | `b` in a downtrend; `p1` long black; `b` a doji body gapping below `p1`'s body |
//! | DTG | Downside Tasuki Gap | bearish | `b` in a downtrend; `p2` long black; `p1` small black, its body gapping below `p2`'s; `b` white, opening inside `p1`'s body and closing inside the gap |
//! | DD | Dragonfly Doji | bullish | in a downtrend: a doji body with an upper shadow no longer than the body |
//! | BE | Engulfing | bearish | `b` long black in an uptrend; `p1` small white; `b`'s body covers `p1`'s, extending past at least one end |
//! | BE | Engulfing | bullish | `b` long white in a downtrend; `p1` small black; `b`'s body covers `p1`'s, extending past at least one end |
//! | EDS | Evening Doji Star | bearish | as Evening Star with a doji body for `p1` |
//! | ES | Evening Star | bearish | `b` in an uptrend; `p2` long white; `p1` small, its body gapping above `p2`'s; `b` long black, its body gapping below `p1`'s and closing into the lower half of `p2`'s body |
//! | FTM | Falling Three Methods | bearish | `p4` long black in a downtrend; `p3`, `p2`, `p1` small white, opening and closing within `p4`'s range; `b` long black closing below `p4`'s close |
//! | FW | Falling Window | bearish | `p1` in a downtrend; `b`'s high below `p1`'s low, neither range zero |
//! | GD | Gravestone Doji | bearish | a doji body with a lower shadow no longer than the body |
//! | H | Hammer | bullish | in a downtrend: a small nonzero body above the range midpoint, lower shadow at least twice the body, no upper shadow |
//! | HM | Hanging Man | bearish | the hammer shape in an uptrend |
//! | BH | Harami | bearish | `p1` long white in an uptrend; `b` small black within `p1`'s body, shadows included |
//! | BH | Harami | bullish | `p1` long black in a downtrend; `b` small white within `p1`'s body, shadows included |
//! | HC | Harami Cross | bearish | as the bearish Harami with a doji body for `b` |
//! | HC | Harami Cross | bullish | as the bullish Harami with a doji body for `b` |
//! | IH | Inverted Hammer | bullish | in a downtrend: a small nonzero body below the range midpoint, upper shadow at least twice the body, no lower shadow |
//! | K | Kicking | bearish | `p1` a white marubozu, `b` a black marubozu whose high gaps below `p1`'s low |
//! | K | Kicking | bullish | `p1` a black marubozu, `b` a white marubozu whose low gaps above `p1`'s high |
//! | LLS | Long Lower Shadow | bullish | lower shadow longer than 75% of the range |
//! | LUS | Long Upper Shadow | bearish | upper shadow longer than 75% of the range |
//! | MB | Marubozu Black | bearish | long black body, each shadow at most 5% of the body |
//! | MW | Marubozu White | bullish | long white body, each shadow at most 5% of the body |
//! | MDS | Morning Doji Star | bullish | as Morning Star with a doji body for `p1` |
//! | MS | Morning Star | bullish | `b` in a downtrend; `p2` long black; `p1` small, its body gapping below `p2`'s; `b` long white, its body gapping above `p1`'s and closing into the upper half of `p2`'s body |
//! | ON | On Neck | bearish | `b` in a downtrend; `p1` long black; `b` small white, opening below `p1`'s close and closing equal to `p1`'s low |
//! | P | Piercing | bullish | `p1` long black in a downtrend; `b` white, opening at or below `p1`'s low, closing above the middle of `p1`'s body and below its open |
//! | RTM | Rising Three Methods | bullish | `p4` long white in an uptrend; `p3`, `p2`, `p1` small black, opening and closing within `p4`'s range; `b` long white closing above `p4`'s close |
//! | RW | Rising Window | bullish | `p1` in an uptrend; `b`'s low above `p1`'s high, neither range zero |
//! | SS | Shooting Star | bearish | the inverted hammer shape in an uptrend |
//! | STB | Spinning Top Black | neutral | black, not a doji body, each shadow at least 34% of the range |
//! | STW | Spinning Top White | neutral | white, not a doji body, each shadow at least 34% of the range |
//! | 3BC | Three Black Crows | bearish | three long black bars, each closing lower, opening within the previous body, with a lower shadow under 5% of its range |
//! | 3WS | Three White Soldiers | bullish | three long white bars, each closing higher, opening within the previous body, with an upper shadow under 5% of its range |
//! | 3S | Tri-Star | bearish | three dojis, `p2` in an uptrend, `p1`'s body gapping above both neighbours' bodies |
//! | 3S | Tri-Star | bullish | three dojis, `p2` in a downtrend, `p1`'s body gapping below both neighbours' bodies |
//! | TB | Tweezer Bottom | bullish | `p1` long black in a downtrend; `b` white with a low equal to `p1`'s, not a doji body unless it has both shadows |
//! | TT | Tweezer Top | bearish | `p1` long white in an uptrend; `b` black with a high equal to `p1`'s, not a doji body unless it has both shadows |
//! | UTG | Upside Tasuki Gap | bullish | `b` in an uptrend; `p2` long white; `p1` small white, its body gapping above `p2`'s; `b` black, opening inside `p1`'s body and closing inside the gap |
//!
//! # Outputs
//!
//! Three lines: Bullish at the low of a bar with a bullish pattern (an upward triangle),
//! Bearish at the high of a bar with a bearish one (a downward triangle) and Neutral at the
//! low of a bar with only a doji or spinning top (a circle). These values only place the marks: the
//! legend lists none of them, and the price scale starts without their tags. Each mark is
//! labelled with the abbreviations of the patterns found on the bar, one per line; on a bar with a
//! bearish pattern the neutral ones join the bearish label, and on a bar with only a bullish one
//! the bullish label, so no two labels overlap. Hovering a
//! mark shows the full name of each pattern it labels, such as "Three White Soldiers - Bullish",
//! each followed by its rule in words, in the label's order. Every pattern has a toggle; Abandoned
//! Baby, Doji, Dragonfly Doji, Engulfing and Hammer are on by default. The pattern type marks
//! both sides (the default) or only the bullish or only the bearish patterns; neutral patterns
//! are marked under every type while their toggle is on.
//!
//! Each pattern also has its own study, registered under its label, such as "Doji" or
//! "Engulfing - Bearish". It draws two lines: its marks, named and styled as its side's line
//! above, and Background, which tints every candle of each pattern it finds in the side colour
//! at 90% transparency. A live bar that stops completing a pattern removes its tint. The marks
//! carry only that pattern's abbreviation and hover text. A neutral pattern keeps
//! its own line even when a bearish pattern completes on the same bar. A pattern whose rule
//! reads the trend offers the trend rule as its only input, SMA50 by default; the twelve
//! without a trend condition, such as Doji or Kicking, offer none.
//!
//! Each single study declares "New pattern detected" as an alert condition. The combined study
//! declares one per pattern in the toggle order; disabled patterns and excluded sides read false.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{CandlestickPatterns, Indicator, PatternTrend};
//!
//! let mut patterns = CandlestickPatterns::new(PatternTrend::Sma50Sma200);
//! patterns.calculate(&bars);
//! let labels: Vec<String> = (0..bars.len()).filter_map(|bar| patterns.plot_text(0, bar)).collect();
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, StudyError, StudyInput,
};
use egui::Color32;

/// How [`CandlestickPatterns`] decides whether a bar is in a downtrend or an uptrend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatternTrend {
    /// The close below or above SMA(close, 50).
    #[default]
    Sma50,
    /// The close below SMA(close, 50) below SMA(close, 200), or the reverse.
    Sma50Sma200,
    /// Every bar counts as both, so trend conditions always hold.
    NoDetection,
}

impl PatternTrend {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Sma50 => "SMA50",
            Self::Sma50Sma200 => "SMA50, SMA200",
            Self::NoDetection => "No detection",
        }
    }
}

/// Which signals [`CandlestickPatterns`] marks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatternType {
    /// Bullish, bearish and neutral patterns.
    #[default]
    Both,
    /// Bullish and neutral patterns.
    Bullish,
    /// Bearish and neutral patterns.
    Bearish,
}

impl PatternType {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Both => "Both",
            Self::Bullish => "Bullish",
            Self::Bearish => "Bearish",
        }
    }

    /// Whether this choice marks patterns of `side`; neutral ones are always marked.
    fn marks(self, side: Side) -> bool {
        match self {
            Self::Both => true,
            Self::Bullish => side != Bearish,
            Self::Bearish => side != Bullish,
        }
    }
}

mod rules;

use rules::Side::{Bearish, Bullish, Neutral};
use rules::{BODY_AVERAGE, Candle, PATTERNS, Pattern, Side};

/// Trend rules in input choice order.
const TRENDS: [PatternTrend; 3] = [
    PatternTrend::Sma50,
    PatternTrend::Sma50Sma200,
    PatternTrend::NoDetection,
];
/// Pattern sides in input choice order.
const TYPES: [PatternType; 3] = [
    PatternType::Both,
    PatternType::Bullish,
    PatternType::Bearish,
];

/// The patterns All Candlestick Patterns marks when it is added; the others start off.
const ON_BY_DEFAULT: [&str; 7] = [
    "Abandoned Baby - Bearish",
    "Abandoned Baby - Bullish",
    "Doji",
    "Dragonfly Doji - Bullish",
    "Engulfing - Bearish",
    "Engulfing - Bullish",
    "Hammer - Bullish",
];

/// The line of a single pattern study that tints the pattern's candles.
const BACKGROUND: usize = 1;

/// Whether a source bar supplies a candle to the pattern rules.
fn usable(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close]
        .iter()
        .all(|v| v.is_finite())
}

/// All Candlestick Patterns: the common candlestick patterns, each with a toggle, marked
/// and labelled at the bar that completes them.
///
/// The factory also lists each pattern under its label, such as "Engulfing - Bearish". That
/// study marks only its pattern, tints its candles, and offers the trend rule, SMA50 by
/// default, only when the pattern's rule reads the trend.
#[derive(Clone, Debug)]
pub struct CandlestickPatterns {
    trend: PatternTrend,
    pattern_type: PatternType,
    /// A single study's toggle index, or `None` for the combined study.
    only: Option<usize>,
    patterns: [bool; PATTERNS.len()],
    /// One bit per pattern of [`PATTERNS`] found on each source bar.
    found: Vec<u64>,
    /// For a single study, the first source bar of the pattern found on each source bar.
    spans: Vec<Option<usize>>,
    /// A single study's Background colour.
    background: Color32,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: [Color32; 3],
    visible: bool,
}

#[derive(Clone, Debug, Default)]
struct State {
    /// Source index of the next bar.
    next: usize,
    /// Source index of each candle in `candles`.
    sources: std::collections::VecDeque<usize>,
    closes: std::collections::VecDeque<f64>,
    bodies: std::collections::VecDeque<f64>,
    average: Option<f64>,
    candles: std::collections::VecDeque<Candle>,
}

impl CandlestickPatterns {
    /// Detect every pattern with the given trend rule.
    pub fn new(trend: PatternTrend) -> Self {
        Self {
            trend,
            pattern_type: PatternType::Both,
            only: None,
            patterns: [true; PATTERNS.len()],
            found: Vec::new(),
            spans: Vec::new(),
            background: Color32::TRANSPARENT,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [palette::BLUE, palette::RED, palette::GRAY],
            visible: true,
        }
    }

    /// Number of patterns, each available as a toggle and a separate study.
    pub(crate) const PATTERN_COUNT: usize = PATTERNS.len();

    /// Mark only the pattern at `index`, below [`PATTERN_COUNT`](Self::PATTERN_COUNT),
    /// under its label with the default trend rule.
    pub(crate) fn only(index: usize) -> Self {
        let label = PATTERNS[index].label;
        let color = [palette::BLUE, palette::RED, palette::GRAY][PATTERNS[index].side.line()];
        Self {
            only: Some(index),
            background: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 26),
            ..Self::new(PatternTrend::Sma50).with_patterns(|pattern| pattern == label)
        }
    }

    /// Mark both sides, or only bullish or only bearish patterns; neutral ones are always marked.
    pub fn with_pattern_type(mut self, pattern_type: PatternType) -> Self {
        self.pattern_type = pattern_type;
        self
    }

    /// Detect only the patterns whose label, such as `"Hammer - Bullish"` or `"Doji"`,
    /// `keep` accepts.
    pub fn with_patterns(mut self, keep: impl Fn(&str) -> bool) -> Self {
        for (on, pattern) in self.patterns.iter_mut().zip(&PATTERNS) {
            *on = keep(pattern.label);
        }
        self
    }

    /// The patterns found on a source bar, from the last calculation.
    fn found(&self, bar: usize, side: Side) -> impl Iterator<Item = &'static Pattern> + '_ {
        let bits = self.found.get(bar).copied().unwrap_or(0);
        PATTERNS
            .iter()
            .enumerate()
            .filter(move |(k, p)| bits >> k & 1 == 1 && p.side == side)
            .map(|(_, p)| p)
    }

    /// The patterns one line labels at a source bar: its side's, joined by neutral names on
    /// the bearish line, or the bullish line when there is no bearish pattern.
    fn labelled(&self, line: usize, bar: usize) -> Vec<&'static Pattern> {
        if self.only.is_some() && line == BACKGROUND {
            return Vec::new();
        }
        let Some(side) = [Bullish, Bearish, Neutral]
            .get(self.combined_line(line))
            .copied()
        else {
            return Vec::new();
        };
        let bearish = self.found(bar, Bearish).next().is_some();
        let bullish = self.found(bar, Bullish).next().is_some();
        match side {
            Bearish if bearish => self
                .found(bar, Bearish)
                .chain(self.found(bar, Neutral))
                .collect(),
            Bullish if bullish && !bearish => self
                .found(bar, Bullish)
                .chain(self.found(bar, Neutral))
                .collect(),
            Neutral if bearish || bullish => Vec::new(),
            _ => self.found(bar, side).collect(),
        }
    }

    /// The combined side's line for a mark. Line 0 of a single study stands for its side's
    /// line; its line 1 is the Background.
    fn combined_line(&self, line: usize) -> usize {
        self.only.map_or(line, |k| PATTERNS[k].side.line())
    }

    /// Whether the trend rule can change the bars marked: always for All Candlestick Patterns,
    /// and for a single study when its pattern's rule reads the trend.
    fn reads_trend(&self) -> bool {
        self.only.is_none_or(|k| PATTERNS[k].trend)
    }

    /// The trend rule input, SMA50 by default.
    fn trend_input(&self) -> StudyInput {
        use crate::studies::inputs::input_spec;

        input_spec!("trend", self.trend, choice("Detect trend based on", TRENDS))
            .with_default(InputValue::Choice(0))
    }

    /// The single study registered under `label`, or `None` for another name.
    pub(crate) fn single(label: &str) -> Option<Self> {
        PATTERNS
            .iter()
            .position(|pattern| pattern.label == label)
            .map(Self::only)
    }

    /// The line of All Candlestick Patterns that marks the pattern labelled `label`, which that
    /// pattern's own study drew on while it drew all three lines, or `None` for another name.
    pub(crate) fn pattern_line(label: &str) -> Option<usize> {
        PATTERNS
            .iter()
            .find(|pattern| pattern.label == label)
            .map(|pattern| pattern.side.line())
    }
}

/// Construct with the conventional default trend rule and the usual patterns on: Abandoned
/// Baby, Doji, Dragonfly Doji, Engulfing and Hammer.
impl Default for CandlestickPatterns {
    fn default() -> Self {
        Self::new(PatternTrend::Sma50).with_patterns(|label| ON_BY_DEFAULT.contains(&label))
    }
}

impl Indicator for CandlestickPatterns {
    fn inputs(&self) -> Vec<StudyInput> {
        use crate::studies::inputs::input_spec;

        let mut inputs: Vec<StudyInput> = self
            .reads_trend()
            .then(|| self.trend_input())
            .into_iter()
            .collect();
        // The single study always marks its pattern, whatever its side.
        if self.only.is_none() {
            inputs.push(
                input_spec!(
                    "pattern_type",
                    self.pattern_type,
                    choice("Pattern type", TYPES)
                )
                .with_default(InputValue::Choice(0)),
            );
            // These ids, including the space, are stored in existing charts and templates.
            inputs.extend(PATTERNS.iter().zip(self.patterns).enumerate().map(
                |(k, (pattern, on))| {
                    StudyInput::new(
                        format!("patterns [{k}]"),
                        pattern.label,
                        InputValue::Toggle(on),
                        InputKind::Toggle,
                    )
                    .with_default(InputValue::Toggle(ON_BY_DEFAULT.contains(&pattern.label)))
                },
            ));
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), StudyError> {
        // A single study whose rule has no trend condition accepts a valid trend rule, which
        // charts and templates saved before it dropped the input hold, and ignores it.
        if id == "trend" && !self.reads_trend() {
            return if self.trend_input().accepts(&value) {
                Ok(())
            } else {
                Err(StudyError::invalid_input(id))
            };
        }
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or_else(|| StudyError::UnknownInput(id.into()))?;
        if !schema.accepts(&value) {
            return Err(StudyError::invalid_input(id));
        }
        match (id, value) {
            ("trend", InputValue::Choice(k)) => self.trend = TRENDS[k],
            ("pattern_type", InputValue::Choice(k)) => self.pattern_type = TYPES[k],
            (_, InputValue::Toggle(on)) => {
                let k = id
                    .strip_prefix("patterns [")
                    .and_then(|rest| rest.strip_suffix(']'))
                    .and_then(|k| k.parse::<usize>().ok())
                    .ok_or_else(|| StudyError::UnknownInput(id.into()))?;
                self.patterns[k] = on;
            }
            _ => return Err(StudyError::UnknownInput(id.into())),
        }
        self.calculate(&[]);
        Ok(())
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        if self.only.is_some() && line == BACKGROUND {
            return crate::studies::PlotStyle {
                color: self.background,
                width: 1.0,
                pattern: crate::model::LineStyle::Solid,
                plot_type: crate::studies::PlotType::BackgroundColor,
                visible: true,
            };
        }
        let line = self.combined_line(line);
        crate::studies::PlotStyle {
            color: self.colors[line.min(2)],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 => crate::studies::PlotType::TriangleUp,
                1 => crate::studies::PlotType::TriangleDown,
                _ => crate::studies::PlotType::Circles,
            },
            visible: true,
        }
    }

    fn plot_color_only(&self, line: usize) -> bool {
        self.only.is_some() && line == BACKGROUND
    }

    fn plot_shape_side(&self, line: usize) -> crate::studies::ShapeSide {
        // Bullish and neutral marks hold the bar's low, bearish ones its high.
        if self.combined_line(line) == 1 {
            crate::studies::ShapeSide::Above
        } else {
            crate::studies::ShapeSide::Below
        }
    }

    fn plot_label_colors(&self, line: usize) -> Option<(Color32, Color32)> {
        self.colors
            .get(self.combined_line(line))
            .map(|&color| (color, Color32::WHITE))
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let text: Vec<&str> = self.labelled(line, bar).iter().map(|p| p.abbr).collect();
        (!text.is_empty()).then(|| text.join("\n"))
    }

    fn plot_tooltip(&self, line: usize, bar: usize) -> Option<String> {
        let names: Vec<String> = self
            .labelled(line, bar)
            .iter()
            .map(|p| format!("{}\n{}", p.label, p.rule))
            .collect();
        (!names.is_empty()).then(|| names.join("\n\n"))
    }

    fn name(&self) -> &str {
        self.only
            .map_or("All Candlestick Patterns", |k| PATTERNS[k].label)
    }

    fn desc(&self) -> &str {
        // A label contains " - ", the picker's full-name separator, so use only the rule here.
        self.only.map_or(
            "All Candlestick Patterns - Japanese candlestick patterns with a trend filter",
            |k| PATTERNS[k].rule,
        )
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        self.found.truncate(start);
        let dropped = self
            .spans
            .get(start..)
            .into_iter()
            .flatten()
            .flatten()
            .copied()
            .min();
        self.spans.truncate(start);
        let own = self.only.map(|k| PATTERNS[k].side.line());
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let at = state.next;
                state.next += 1;
                if !usable(bar) {
                    self.spans.push(None);
                    self.found.push(0);
                    return IndicatorValue::None;
                }
                state.closes.push_back(bar.close);
                if state.closes.len() > 200 {
                    state.closes.pop_front();
                }
                let sma = |period| {
                    if state.closes.len() < period {
                        f64::NAN
                    } else {
                        state
                            .closes
                            .iter()
                            .skip(state.closes.len() - period)
                            .sum::<f64>()
                            / period as f64
                    }
                };
                let (down, up) = match self.trend {
                    PatternTrend::Sma50 => {
                        let mean = sma(50);
                        (bar.close < mean, bar.close > mean)
                    }
                    PatternTrend::Sma50Sma200 => {
                        let (fast, slow) = (sma(50), sma(200));
                        (
                            bar.close < fast && fast < slow,
                            bar.close > fast && fast > slow,
                        )
                    }
                    PatternTrend::NoDetection => (true, true),
                };
                let body = (bar.close - bar.open).abs();
                state.bodies.push_back(body);
                if state.bodies.len() > BODY_AVERAGE {
                    state.bodies.pop_front();
                }
                let average = match state.average {
                    Some(previous) => {
                        previous + (2.0 / (BODY_AVERAGE + 1) as f64) * (body - previous)
                    }
                    None if state.bodies.len() == BODY_AVERAGE => {
                        if state.bodies.iter().all(|v| v.is_finite()) {
                            state.bodies.iter().sum::<f64>() / BODY_AVERAGE as f64
                        } else {
                            f64::NAN
                        }
                    }
                    None => f64::NAN,
                };
                if state.bodies.len() == BODY_AVERAGE {
                    state.average = Some(average);
                }
                state.sources.push_back(at);
                state.candles.push_back(Candle {
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    average,
                    down,
                    up,
                });
                let max_bars = PATTERNS.iter().map(|pattern| pattern.bars).max().unwrap();
                if state.candles.len() > max_bars {
                    state.candles.pop_front();
                    state.sources.pop_front();
                }
                let candles = state.candles.make_contiguous();
                let mut bits = 0;
                for (bit, pattern) in PATTERNS.iter().enumerate() {
                    if self.patterns[bit]
                        && self.pattern_type.marks(pattern.side)
                        && candles.len() >= pattern.bars
                        && (pattern.found)(&candles[candles.len() - pattern.bars..])
                    {
                        bits |= 1 << bit;
                    }
                }
                self.found.push(bits);
                self.spans.push(
                    self.only
                        .filter(|&k| bits >> k & 1 == 1)
                        .map(|k| state.sources[state.sources.len() - PATTERNS[k].bars]),
                );
                let found = |side| {
                    PATTERNS
                        .iter()
                        .enumerate()
                        .any(|(bit, pattern)| bits >> bit & 1 == 1 && pattern.side == side)
                };
                let (bull, bear, neutral) = (found(Bullish), found(Bearish), found(Neutral));
                let marks = [
                    if bull { bar.low } else { f64::NAN },
                    if bear { bar.high } else { f64::NAN },
                    if neutral && !bull && !bear {
                        bar.low
                    } else {
                        f64::NAN
                    },
                ];
                // A single study draws only its side's line.
                match own {
                    Some(line) => IndicatorValue::Multiple(vec![marks[line], f64::NAN]),
                    None => IndicatorValue::Multiple(marks.to_vec()),
                }
            });
        if self.only.is_some() {
            // Bars tinted by a pattern found from `start` on, before or after this run.
            let from = self.spans[start..]
                .iter()
                .flatten()
                .copied()
                .chain(dropped)
                .min()
                .map_or(start, |first| first.min(start));
            for bar in from..start {
                self.values.set_value(BACKGROUND, bar, f64::NAN);
            }
            for end in from..self.spans.len() {
                if let Some(first) = self.spans[end] {
                    for bar in (first..=end).filter(|&bar| usable(&data[bar])) {
                        self.values.set_value(BACKGROUND, bar, data[bar].close);
                    }
                }
            }
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn alert_conditions(&self) -> Vec<String> {
        if self.only.is_some() {
            vec!["New pattern detected".into()]
        } else {
            PATTERNS
                .iter()
                .map(|pattern| pattern.label.to_owned())
                .collect()
        }
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        let pattern = match self.only {
            Some(index) if condition == 0 => index,
            Some(_) => return None,
            None => condition,
        };
        PATTERNS.get(pattern)?;
        Some(self.found.get(bar)? >> pattern & 1 == 1)
    }

    // A mark's value only places its shape at the bar's high or low, so it is no reading.
    fn legend_lines(&self) -> Vec<usize> {
        Vec::new()
    }

    fn plot_tagged(&self, _line: usize) -> bool {
        false
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        match self.only {
            Some(k) => vec![self.colors[PATTERNS[k].side.line()], self.background],
            None => self.colors.to_vec(),
        }
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in colors.into_iter().take(self.line_count()).enumerate() {
            if self.only.is_some() && line == BACKGROUND {
                self.background = color;
            } else {
                let slot = self.combined_line(line);
                self.colors[slot] = color;
            }
        }
    }

    fn line_names(&self) -> Vec<String> {
        let names = ["Bullish", "Bearish", "Neutral"];
        match self.only {
            Some(k) => vec![names[PATTERNS[k].side.line()].into(), "Background".into()],
            None => names.map(String::from).into(),
        }
    }

    // A side holds values only while the pattern type marks it and one of its patterns is on.
    fn alert_outputs(&self) -> Vec<(usize, String)> {
        self.line_names()
            .into_iter()
            .enumerate()
            .filter(|(line, _)| {
                if self.only.is_some() && *line == BACKGROUND {
                    return false;
                }
                let side = [Bullish, Bearish, Neutral][self.combined_line(*line)];
                self.pattern_type.marks(side)
                    && PATTERNS
                        .iter()
                        .zip(self.patterns)
                        .any(|(pattern, on)| on && pattern.side == side)
            })
            .collect()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    type Ohlc = (f64, f64, f64, f64);
    /// Label, line, abbreviation, bars that form the pattern and near misses.
    type Case = (
        &'static str,
        usize,
        &'static str,
        &'static [Ohlc],
        &'static [&'static [Ohlc]],
    );

    /// Bars of body 1 through `closes`, each opening one point away from its close.
    fn walk(closes: impl IntoIterator<Item = f64>) -> Vec<Bar> {
        let mut prev: Option<f64> = None;
        closes
            .into_iter()
            .map(|close| {
                let open = prev.unwrap_or(close - 1.0);
                prev = Some(close);
                let (top, bottom) = (open.max(close), open.min(close));
                Bar::new(Utc::now(), open, top + 0.25, bottom - 0.25, close, 1.0)
            })
            .collect()
    }

    /// The last bar's text from the factory study for `label`, or all patterns for an empty
    /// label, under `trend`.
    fn text(
        trend: PatternTrend,
        before: &[Bar],
        bars: &[Ohlc],
        label: &str,
        line: usize,
    ) -> Option<String> {
        use crate::studies::{IndicatorFactory, InputValue};

        let mut data = before.to_vec();
        data.extend(
            bars.iter()
                .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
        );
        let name = if label.is_empty() {
            "All Candlestick Patterns"
        } else {
            label
        };
        let mut study: Box<dyn Indicator> = if label.is_empty() {
            Box::new(CandlestickPatterns::new(trend))
        } else {
            IndicatorFactory::new().create(name).unwrap()
        };
        let rule = TRENDS.iter().position(|&rule| rule == trend).unwrap();
        study.set_input("trend", InputValue::Choice(rule)).unwrap();
        study.calculate(&data);
        let line = if label.is_empty() { line } else { 0 };
        study.plot_text(line, data.len() - 1)
    }

    /// Each pattern's own study, after bars of body 1 that set the body average and without
    /// a trend filter, marks its abbreviation on its own line and rejects each near miss.
    #[test]
    fn patterns_match_their_definitions() {
        let flat = walk((0..14).map(|i| 100.0 + f64::from(i % 2)));
        #[rustfmt::skip]
        let cases: [Case; 44] = [
            ("Abandoned Baby - Bearish", 1, "AB",
                &[(100.0, 103.0, 99.5, 102.5), (104.0, 105.0, 103.5, 104.0625), (103.0, 103.25, 100.0, 100.5)],
                &[&[(100.0, 103.0, 99.5, 102.5), (104.0, 105.0, 103.5, 104.0625), (103.0, 103.5, 100.0, 100.5)],
                  &[(102.0, 103.0, 99.5, 102.5), (104.0, 105.0, 103.5, 104.0625), (103.0, 103.25, 100.0, 100.5)]]),
            ("Abandoned Baby - Bullish", 0, "AB",
                &[(102.5, 103.0, 99.5, 100.0), (98.5, 99.0, 97.5, 98.5625), (99.5, 102.5, 99.25, 102.0)],
                &[&[(102.5, 103.0, 99.5, 100.0), (98.5, 99.0, 97.5, 98.5625), (99.5, 102.5, 99.0, 102.0)],
                  &[(100.5, 103.0, 99.5, 100.0), (98.5, 99.0, 97.5, 98.5625), (99.5, 102.5, 99.25, 102.0)]]),
            ("Dark Cloud Cover - Bearish", 1, "DCC",
                &[(100.0, 103.5, 99.5, 103.0), (104.0, 104.5, 101.0, 101.25)],
                &[&[(100.0, 103.5, 99.5, 103.0), (104.0, 104.5, 101.5, 101.75)]]),
            ("Doji", 2, "D", &[(100.0, 102.0, 98.0, 100.0625)], &[&[(100.0, 102.0, 98.0, 100.5)]]),
            ("Doji Star - Bearish", 1, "DS",
                &[(100.0, 103.25, 99.75, 103.0), (103.5, 104.5, 102.5, 103.5625)],
                &[&[(100.0, 103.25, 99.75, 103.0), (102.75, 103.75, 101.75, 102.8125)]]),
            ("Doji Star - Bullish", 0, "DS",
                &[(103.0, 103.25, 99.75, 100.0), (99.5, 100.5, 98.5, 99.4375)],
                &[&[(103.0, 103.25, 99.75, 100.0), (100.25, 101.25, 99.25, 100.1875)]]),
            ("Downside Tasuki Gap - Bearish", 1, "DTG",
                &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.25, 100.25, 98.75, 99.75)],
                &[&[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.25, 100.5, 98.75, 100.25)],
                  &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (98.75, 99.75, 98.5, 99.75)],
                  &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.75, 100.0, 99.5, 99.875)],
                  &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.25, 99.75, 99.0, 99.5)],
                  &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.25, 100.25, 99.0, 100.0)]]),
            ("Dragonfly Doji - Bullish", 0, "DD",
                &[(100.0, 100.0625, 96.0, 100.0625)], &[&[(100.0, 100.5, 96.0, 100.0625)]]),
            ("Engulfing - Bearish", 1, "BE",
                &[(100.5, 101.25, 100.25, 101.0), (101.25, 101.5, 99.75, 100.0)],
                &[&[(100.5, 101.25, 100.25, 101.0), (100.75, 101.0, 99.25, 99.5)]]),
            ("Engulfing - Bullish", 0, "BE",
                &[(101.0, 101.25, 100.25, 100.5), (100.25, 101.75, 100.0, 101.5)],
                &[&[(101.0, 101.25, 100.25, 100.5), (100.75, 102.0, 100.5, 102.0)]]),
            ("Evening Doji Star - Bearish", 1, "EDS",
                &[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.0625), (104.5, 104.75, 101.0, 101.5)],
                &[&[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.5), (104.5, 104.75, 101.0, 101.5)]]),
            ("Evening Star - Bearish", 1, "ES",
                &[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.5), (104.5, 104.75, 101.0, 101.5)],
                &[&[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.5), (104.5, 104.75, 102.0, 102.25)]]),
            ("Falling Three Methods - Bearish", 1, "FTM",
                &[(104.0, 104.5, 99.5, 100.0), (101.0, 101.75, 100.75, 101.5), (101.0, 101.75, 100.75, 101.5),
                  (101.0, 101.75, 100.75, 101.5), (101.5, 101.75, 98.5, 99.0)],
                &[&[(104.0, 104.5, 99.5, 100.0), (101.0, 101.75, 100.75, 101.5), (101.0, 101.75, 100.75, 101.5),
                  (101.0, 101.75, 100.75, 101.5), (101.5, 101.75, 100.0, 100.25)]]),
            ("Falling Window - Bearish", 1, "FW",
                &[(101.0, 101.5, 99.5, 100.0), (99.0, 99.25, 98.0, 98.5)],
                &[&[(101.0, 101.5, 99.5, 100.0), (99.0, 99.5, 98.0, 98.5)]]),
            ("Gravestone Doji - Bearish", 1, "GD",
                &[(100.0, 104.0, 100.0, 100.0625)], &[&[(100.0, 104.0, 99.5, 100.0625)]]),
            ("Hammer - Bullish", 0, "H", &[(100.0, 100.5, 98.0, 100.5)], &[&[(100.0, 100.75, 98.0, 100.5)]]),
            ("Hanging Man - Bearish", 1, "HM", &[(100.0, 100.5, 98.0, 100.5)], &[&[(100.0, 100.75, 98.0, 100.5)]]),
            ("Harami - Bearish", 1, "BH",
                &[(100.0, 104.5, 99.5, 104.0), (102.5, 103.0, 101.5, 102.0)],
                &[&[(100.0, 104.5, 99.5, 104.0), (102.5, 104.25, 101.5, 102.0)]]),
            ("Harami - Bullish", 0, "BH",
                &[(104.0, 104.5, 99.5, 100.0), (101.5, 103.0, 101.0, 102.0)],
                &[&[(104.0, 104.5, 99.5, 100.0), (101.5, 103.0, 99.75, 102.0)]]),
            ("Harami Cross - Bearish", 1, "HC",
                &[(100.0, 104.5, 99.5, 104.0), (102.0, 103.0, 101.0, 102.0625)],
                &[&[(100.0, 104.5, 99.5, 104.0), (102.0, 103.0, 101.0, 102.25)]]),
            ("Harami Cross - Bullish", 0, "HC",
                &[(104.0, 104.5, 99.5, 100.0), (102.0, 103.0, 101.0, 102.0625)],
                &[&[(104.0, 104.5, 99.5, 100.0), (102.0, 103.0, 101.0, 102.25)]]),
            ("Inverted Hammer - Bullish", 0, "IH", &[(100.0, 102.5, 100.0, 100.5)], &[&[(100.0, 102.5, 99.75, 100.5)]]),
            ("Kicking - Bearish", 1, "K",
                &[(105.0, 107.0, 105.0, 107.0), (104.0, 104.0, 102.0, 102.0)],
                &[&[(105.0, 107.0, 105.0, 107.0), (105.0, 105.0, 103.0, 103.0)]]),
            ("Kicking - Bullish", 0, "K",
                &[(104.0, 104.0, 102.0, 102.0), (105.0, 107.0, 105.0, 107.0)],
                &[&[(104.0, 104.0, 102.0, 102.0), (104.0, 106.0, 104.0, 106.0)]]),
            ("Long Lower Shadow - Bullish", 0, "LLS", &[(100.0, 100.5, 96.0, 100.25)], &[&[(100.0, 101.0, 97.0, 100.25)]]),
            ("Long Upper Shadow - Bearish", 1, "LUS", &[(100.0, 104.0, 99.5, 99.75)], &[&[(100.0, 103.0, 99.0, 99.75)]]),
            ("Marubozu Black - Bearish", 1, "MB", &[(102.0, 102.0, 100.0, 100.0)], &[&[(102.0, 102.0, 99.75, 100.0)]]),
            ("Marubozu White - Bullish", 0, "MW", &[(100.0, 102.0, 100.0, 102.0)], &[&[(100.0, 102.25, 100.0, 102.0)]]),
            ("Morning Doji Star - Bullish", 0, "MDS",
                &[(104.0, 104.25, 99.75, 100.0), (98.5, 99.5, 98.0, 98.5625), (99.5, 103.0, 99.25, 102.5)],
                &[&[(104.0, 104.25, 99.75, 100.0), (99.0, 99.5, 98.0, 98.5), (99.5, 103.0, 99.25, 102.5)]]),
            ("Morning Star - Bullish", 0, "MS",
                &[(104.0, 104.25, 99.75, 100.0), (99.0, 99.5, 98.0, 98.5), (99.5, 103.0, 99.25, 102.5)],
                &[&[(104.0, 104.25, 99.75, 100.0), (99.0, 99.5, 98.0, 98.5), (99.5, 102.0, 99.25, 101.75)]]),
            ("On Neck - Bearish", 1, "ON",
                &[(104.0, 104.5, 99.75, 100.0), (99.0, 100.0, 98.75, 99.75)],
                &[&[(104.0, 104.5, 99.75, 100.0), (99.0, 100.5, 98.75, 100.25)]]),
            ("Piercing - Bullish", 0, "P",
                &[(103.0, 103.5, 99.5, 100.0), (99.0, 102.0, 98.5, 101.75)],
                &[&[(103.0, 103.5, 99.5, 100.0), (99.0, 102.0, 98.5, 101.25)]]),
            ("Rising Three Methods - Bullish", 0, "RTM",
                &[(100.0, 104.5, 99.5, 104.0), (103.0, 103.25, 102.25, 102.5), (103.0, 103.25, 102.25, 102.5),
                  (103.0, 103.25, 102.25, 102.5), (102.5, 105.25, 102.25, 105.0)],
                &[&[(100.0, 104.5, 99.5, 104.0), (103.0, 103.25, 102.25, 102.5), (103.0, 103.25, 102.25, 102.5),
                  (103.0, 103.25, 102.25, 102.5), (102.5, 104.0, 102.25, 103.75)]]),
            ("Rising Window - Bullish", 0, "RW",
                &[(100.0, 101.5, 99.5, 101.0), (102.0, 103.0, 101.75, 102.5)],
                &[&[(100.0, 101.5, 99.5, 101.0), (102.0, 103.0, 101.5, 102.5)]]),
            ("Shooting Star - Bearish", 1, "SS", &[(100.0, 102.5, 100.0, 100.5)], &[&[(100.0, 102.5, 99.75, 100.5)]]),
            ("Spinning Top Black", 2, "STB", &[(100.5, 102.0, 98.0, 100.0)], &[&[(100.5, 101.75, 98.0, 100.0)]]),
            ("Spinning Top White", 2, "STW", &[(100.0, 102.0, 98.0, 100.5)], &[&[(100.0, 101.75, 98.0, 100.5)]]),
            ("Three Black Crows - Bearish", 1, "3BC",
                &[(104.0, 104.5, 102.0, 102.0), (103.0, 103.5, 101.0, 101.0), (102.0, 102.5, 100.0, 100.0)],
                &[&[(104.0, 104.5, 102.0, 102.0), (103.0, 103.5, 101.0, 101.0), (102.0, 102.5, 99.5, 100.0)]]),
            ("Three White Soldiers - Bullish", 0, "3WS",
                &[(100.0, 102.0, 99.5, 102.0), (101.0, 103.0, 100.5, 103.0), (102.0, 104.0, 101.5, 104.0)],
                &[&[(100.0, 102.0, 99.5, 102.0), (101.0, 103.0, 100.5, 103.0), (102.0, 104.5, 101.5, 104.0)]]),
            ("Tri-Star - Bearish", 1, "3S",
                &[(100.0, 101.0, 99.0, 100.0), (102.0, 103.0, 101.0, 102.0), (100.0, 101.0, 99.0, 100.0)],
                &[&[(100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0)]]),
            ("Tri-Star - Bullish", 0, "3S",
                &[(100.0, 101.0, 99.0, 100.0), (98.0, 99.0, 97.0, 98.0), (100.0, 101.0, 99.0, 100.0)],
                &[&[(100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0)]]),
            ("Tweezer Bottom - Bullish", 0, "TB",
                &[(103.0, 103.25, 99.5, 100.0), (100.0, 101.5, 99.5, 101.0)],
                &[&[(103.0, 103.25, 99.5, 100.0), (100.0, 101.5, 99.25, 101.0)]]),
            ("Tweezer Top - Bearish", 1, "TT",
                &[(100.0, 103.5, 99.75, 103.0), (103.0, 103.5, 101.5, 102.0)],
                &[&[(100.0, 103.5, 99.75, 103.0), (103.0, 103.75, 101.5, 102.0)]]),
            ("Upside Tasuki Gap - Bullish", 0, "UTG",
                &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.75, 105.25, 104.0, 104.25)],
                &[&[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.75, 105.25, 103.5, 103.75)],
                  &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.25, 104.5, 104.0, 104.125)],
                  &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (105.5, 105.75, 104.0, 104.25)],
                  &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.75, 105.0, 104.5, 104.5)],
                  &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.75, 105.0, 103.75, 104.0)]]),
        ];
        let none = PatternTrend::NoDetection;
        // Bars near 100 are in a downtrend after closes falling to 90 and in an uptrend after
        // closes rising to 99. A reversal needs the trend it ends, a continuation the trend
        // it continues, and the shapes below need none. Without detection every pattern is
        // marked after either history.
        let falling = walk((90..150).rev().map(f64::from));
        let rising = walk((40..100).map(f64::from));
        for (label, line, abbr, hit, miss) in cases {
            let index = PATTERNS
                .iter()
                .position(|pattern| pattern.label == label)
                .unwrap();
            for (formation, expected) in
                std::iter::once((hit, true)).chain(miss.iter().map(|bars| (*bars, false)))
            {
                let mut data = flat.clone();
                data.extend(
                    formation
                        .iter()
                        .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
                );
                let last = data.len() - 1;
                for only in [None, Some(index)] {
                    let mut study = only
                        .map_or_else(|| CandlestickPatterns::new(none), CandlestickPatterns::only);
                    study.trend = none;
                    study.calculate(&data);
                    let condition = if only.is_some() { 0 } else { index };
                    let names = study.alert_conditions();
                    assert_eq!(
                        names[condition],
                        if only.is_some() {
                            "New pattern detected"
                        } else {
                            label
                        }
                    );
                    assert_eq!(
                        study.alert_condition(condition, last),
                        Some(expected),
                        "{label}, {only:?}"
                    );
                    assert_eq!(study.alert_condition(names.len(), last), None);
                    assert_eq!(study.alert_condition(condition, data.len()), None);
                    if only.is_some() {
                        let tinted: Vec<_> = (0..data.len())
                            .filter(|&bar| study.values().value(1, bar).is_some())
                            .collect();
                        let expected_bars: Vec<_> = if expected {
                            (flat.len()..data.len()).collect()
                        } else {
                            Vec::new()
                        };
                        assert_eq!(tinted, expected_bars, "{label}, {formation:?}");
                        if !expected && formation[..formation.len() - 1] == hit[..hit.len() - 1] {
                            let mut original = flat.clone();
                            original.extend(
                                hit.iter()
                                    .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
                            );
                            study.calculate(&original);
                            study.update(&data, last);
                            assert!(
                                (0..data.len()).all(|bar| study.values().value(1, bar).is_none()),
                                "{label}: live removal"
                            );
                        }
                    }
                }
            }
            assert_eq!(
                text(none, &flat, hit, label, line).as_deref(),
                Some(abbr),
                "{label}"
            );
            for bars in miss {
                for line in 0..3 {
                    assert_eq!(
                        text(none, &flat, bars, label, line),
                        None,
                        "{label} near miss {bars:?}"
                    );
                }
            }
            let trendless = [
                "D", "GD", "K", "LLS", "LUS", "MB", "MW", "STB", "STW", "3BC", "3WS",
            ];
            let continuation = ["DTG", "FTM", "FW", "ON", "RTM", "RW", "UTG"];
            let (with, against) = if (line == 0) != continuation.contains(&abbr) {
                (&falling, &rising)
            } else {
                (&rising, &falling)
            };
            let sma = PatternTrend::Sma50;
            assert_eq!(
                text(sma, with, hit, label, line).as_deref(),
                Some(abbr),
                "{label} in trend"
            );
            assert_eq!(
                text(sma, against, hit, label, line).as_deref(),
                trendless.contains(&abbr).then_some(abbr),
                "{label} against trend"
            );
            for history in [with, against] {
                assert_eq!(
                    text(none, history, hit, label, line).as_deref(),
                    Some(abbr),
                    "{label} without trend detection"
                );
            }
        }

        // The trend is checked on the bar the definition names: a window rising from a bar
        // below SMA(50) is not marked, even when the gap carries the close above it.
        let window: &[Ohlc] = &[(90.0, 90.25, 88.75, 89.0), (120.0, 121.0, 119.5, 120.5)];
        let rw = "Rising Window - Bullish";
        assert_eq!(text(PatternTrend::Sma50, &falling, window, rw, 0), None);

        // A hammer needs a downtrend: the close below SMA(50), and for the stricter rule also
        // SMA(50) below SMA(200). After 150 rising and 60 falling closes SMA(50) is above the
        // close but above SMA(200).
        let hammer: &[Ohlc] = &[(89.0, 89.5, 87.0, 89.5)];
        let turned = walk((1..=150).chain((90..150).rev()).map(f64::from));
        let long_fall = walk((90..300).rev().map(f64::from));
        let h = "Hammer - Bullish";
        assert_eq!(
            text(PatternTrend::Sma50, &falling, hammer, h, 0).as_deref(),
            Some("H")
        );
        assert_eq!(text(PatternTrend::Sma50, &rising, hammer, h, 0), None);
        assert_eq!(
            text(PatternTrend::Sma50, &turned, hammer, h, 0).as_deref(),
            Some("H")
        );
        assert_eq!(text(PatternTrend::Sma50Sma200, &turned, hammer, h, 0), None);
        assert_eq!(
            text(PatternTrend::Sma50Sma200, &long_fall, hammer, h, 0).as_deref(),
            Some("H")
        );

        // Several patterns on a bar are listed in toggle order, one per line.
        let dragonfly: &[Ohlc] = &[(100.0, 100.0625, 96.0, 100.0625)];
        assert_eq!(
            text(none, &flat, dragonfly, "", 0).as_deref(),
            Some("DD\nH\nLLS")
        );
        // Adding one pattern from the picker leaves only its abbreviation on this bar.
        assert_eq!(
            text(none, &flat, dragonfly, "Hammer - Bullish", 0).as_deref(),
            Some("H")
        );

        // A neutral doji closing a bearish Tri-Star shares the high, so the bearish label names it.
        let tri_star: &[Ohlc] = &[
            (100.0, 101.0, 99.0, 100.0),
            (102.0, 103.0, 101.0, 102.0),
            (100.0, 101.0, 99.0, 100.0),
        ];
        assert_eq!(text(none, &flat, tri_star, "", 1).as_deref(), Some("3S\nD"));
        assert_eq!(text(none, &flat, tri_star, "", 2), None);

        // A bullish Tri-Star and the last doji share the low and one label.
        let bullish: &[Ohlc] = &[
            (100.0, 101.0, 99.0, 100.0),
            (98.0, 99.0, 97.0, 98.0),
            (100.0, 101.0, 99.0, 100.0),
        ];
        assert_eq!(text(none, &flat, bullish, "", 0).as_deref(), Some("3S\nD"));
        assert_eq!(text(none, &flat, bullish, "", 2), None);

        // The pattern type filters directional patterns while neutral patterns remain enabled.
        let mut data = flat.clone();
        data.extend(
            tri_star
                .iter()
                .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
        );
        let mut study = CandlestickPatterns::new(none).with_pattern_type(PatternType::Bullish);
        study.calculate(&data);
        let last = data.len() - 1;
        assert_eq!(study.plot_text(1, last), None);
        assert_eq!(study.plot_text(2, last).as_deref(), Some("D"));
        assert_eq!(study.values().value(2, last), Some(99.0));
        let doji = PATTERNS.iter().position(|p| p.label == "Doji").unwrap();
        assert_eq!(study.alert_condition(doji, last), Some(true));
        assert_eq!(
            study.alert_outputs(),
            [(0, "Bullish".into()), (2, "Neutral".into())]
        );
        let mut study = study.with_pattern_type(PatternType::Bearish);
        study.calculate(&data);
        assert_eq!(study.plot_text(1, last).as_deref(), Some("3S\nD"));
        assert_eq!(study.plot_text(2, last), None);
    }

    /// A pattern study starts on SMA50, offers only the trend rule, and restores a changed
    /// rule. Older combined studies keep their trend, side and individual toggles. Kicking -
    /// Bearish and Marubozu White - Bullish saved with the trend rule and three lines restore
    /// without the rule, with their side's colour, and mark the same bar.
    #[test]
    fn pattern_studies_restore_their_saved_inputs() {
        use crate::studies::{IndicatorFactory, InputValue, StudyTemplate};

        let with = |mut history: Vec<Bar>, bars: &[Ohlc]| {
            history.extend(
                bars.iter()
                    .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
            );
            history
        };
        let marks = |study: &mut Box<dyn Indicator>, data: &[Bar], line| {
            study.calculate(data);
            study.plot_text(line, data.len() - 1)
        };
        let hammer: &[Ohlc] = &[(89.0, 89.5, 87.0, 89.5)];
        let turned = with(
            walk((1..=150).chain((90..150).rev()).map(f64::from)),
            hammer,
        );
        let rising = with(walk((40..100).map(f64::from)), hammer);
        let long_fall = with(walk((90..300).rev().map(f64::from)), hammer);

        let mut combined = IndicatorFactory::new()
            .create("All Candlestick Patterns")
            .unwrap();
        let usual = [
            "Abandoned Baby - Bearish",
            "Abandoned Baby - Bullish",
            "Doji",
            "Dragonfly Doji - Bullish",
            "Engulfing - Bearish",
            "Engulfing - Bullish",
            "Hammer - Bullish",
        ];
        for (k, pattern) in PATTERNS.iter().enumerate() {
            let inputs = combined.inputs();
            let input = inputs
                .iter()
                .find(|input| input.id == format!("patterns [{k}]"))
                .unwrap();
            let expected = InputValue::Toggle(usual.contains(&pattern.label));
            assert_eq!(input.value, expected, "{}", pattern.label);
            assert_eq!(input.default, expected, "{}", pattern.label);
        }
        combined.set_input("trend", InputValue::Choice(2)).unwrap();
        let flat = walk((0..14).map(|i| 100.0 + f64::from(i % 2)));
        let dragonfly = with(flat, &[(100.0, 100.0625, 96.0, 100.0625)]);
        assert_eq!(
            marks(&mut combined, &dragonfly, 0).as_deref(),
            Some("DD\nH")
        );

        let mut study = IndicatorFactory::new().create("Hammer - Bullish").unwrap();
        let ids: Vec<String> = study.inputs().into_iter().map(|input| input.id).collect();
        assert_eq!(ids, ["trend"]);
        // Close 89.5 < SMA(50) 113.51, but SMA(50) > SMA(200) 92.5925.
        assert_eq!(marks(&mut study, &turned, 0).as_deref(), Some("H"));
        // After a rise SMA(50) is 75.29, below the hammer's close.
        assert_eq!(marks(&mut study, &rising, 0), None);
        study.set_input("trend", InputValue::Choice(1)).unwrap();
        let saved = serde_json::to_string(&StudyTemplate::from_study(study.as_ref())).unwrap();
        let mut restored = serde_json::from_str::<StudyTemplate>(&saved)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(restored.registered_name(), "Hammer - Bullish");
        assert_eq!(marks(&mut restored, &turned, 0), None);
        // On the long decline SMA(200) is 188.5025, above SMA(50).
        assert_eq!(marks(&mut restored, &long_fall, 0).as_deref(), Some("H"));

        // Literal saved on main before individual pattern studies: no trend detection,
        // bullish only, with Dragonfly Doji (toggle 7) disabled.
        let old = r#"{"name":"All Candlestick Patterns","inputs":[["trend",{"Choice":2}],["pattern_type",{"Choice":1}],["patterns [0]",{"Toggle":true}],["patterns [1]",{"Toggle":true}],["patterns [2]",{"Toggle":true}],["patterns [3]",{"Toggle":true}],["patterns [4]",{"Toggle":true}],["patterns [5]",{"Toggle":true}],["patterns [6]",{"Toggle":true}],["patterns [7]",{"Toggle":false}],["patterns [8]",{"Toggle":true}],["patterns [9]",{"Toggle":true}],["patterns [10]",{"Toggle":true}],["patterns [11]",{"Toggle":true}],["patterns [12]",{"Toggle":true}],["patterns [13]",{"Toggle":true}],["patterns [14]",{"Toggle":true}],["patterns [15]",{"Toggle":true}],["patterns [16]",{"Toggle":true}],["patterns [17]",{"Toggle":true}],["patterns [18]",{"Toggle":true}],["patterns [19]",{"Toggle":true}],["patterns [20]",{"Toggle":true}],["patterns [21]",{"Toggle":true}],["patterns [22]",{"Toggle":true}],["patterns [23]",{"Toggle":true}],["patterns [24]",{"Toggle":true}],["patterns [25]",{"Toggle":true}],["patterns [26]",{"Toggle":true}],["patterns [27]",{"Toggle":true}],["patterns [28]",{"Toggle":true}],["patterns [29]",{"Toggle":true}],["patterns [30]",{"Toggle":true}],["patterns [31]",{"Toggle":true}],["patterns [32]",{"Toggle":true}],["patterns [33]",{"Toggle":true}],["patterns [34]",{"Toggle":true}],["patterns [35]",{"Toggle":true}],["patterns [36]",{"Toggle":true}],["patterns [37]",{"Toggle":true}],["patterns [38]",{"Toggle":true}],["patterns [39]",{"Toggle":true}],["patterns [40]",{"Toggle":true}],["patterns [41]",{"Toggle":true}],["patterns [42]",{"Toggle":true}],["patterns [43]",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CandlestickPatterns.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"Circles","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let template = serde_json::from_str::<StudyTemplate>(old).unwrap();
        let mut restored = template.create().unwrap();
        let flat = walk((0..14).map(|i| 100.0 + f64::from(i % 2)));
        let dragonfly = with(flat.clone(), &[(100.0, 100.0625, 96.0, 100.0625)]);
        assert_eq!(
            marks(&mut restored, &dragonfly, 0).as_deref(),
            Some("H\nLLS")
        );
        let tri_star = with(
            flat.clone(),
            &[
                (100.0, 101.0, 99.0, 100.0),
                (102.0, 103.0, 101.0, 102.0),
                (100.0, 101.0, 99.0, 100.0),
            ],
        );
        assert_eq!(marks(&mut restored, &tri_star, 1), None);

        // Charts saved while every pattern study drew three lines and took the trend rule:
        // Kicking - Bearish without trend detection and an orange bearish line, Marubozu White -
        // Bullish on SMA50 and SMA200 with a teal bullish line.
        let kicking = r#"{"name":"Kicking - Bearish","inputs":[["trend",{"Choice":2}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CandlestickPatterns.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":true},{"color":[255,152,0,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"Circles","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let marubozu = r#"{"name":"Marubozu White - Bullish","inputs":[["trend",{"Choice":1}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CandlestickPatterns.html","plots":[{"color":[8,153,129,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"Circles","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        // Saved on origin/main 2be0b0108, when each single pattern had one plot.
        let doji = r#"{"name":"Doji","inputs":[["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CandlestickPatterns.html","plots":[{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"Circles","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[8,8,8,16],"untagged_plots":[0],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let engulfing = r#"{"name":"Engulfing - Bearish","inputs":[["trend",{"Choice":2}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CandlestickPatterns.html","plots":[{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[15,3,4,16],"untagged_plots":[0],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let saves: [(&str, Color32, &[Ohlc], &str); 4] = [
            (
                kicking,
                Color32::from_rgb(255, 152, 0),
                &[(105.0, 107.0, 105.0, 107.0), (104.0, 104.0, 102.0, 102.0)],
                "K",
            ),
            (
                marubozu,
                Color32::from_rgb(8, 153, 129),
                &[(100.0, 102.0, 100.0, 102.0)],
                "MW",
            ),
            (doji, palette::GRAY, &[(100.0, 102.0, 98.0, 100.0625)], "D"),
            (
                engulfing,
                palette::RED,
                &[
                    (100.5, 101.25, 100.25, 101.0),
                    (101.25, 101.5, 99.75, 100.0),
                ],
                "BE",
            ),
        ];
        for (saved, color, bars, abbr) in saves {
            let template = serde_json::from_str::<StudyTemplate>(saved).unwrap();
            let name = template.name.as_str();
            let mut restored = template.create().unwrap();
            assert_eq!(restored.registered_name(), name);
            assert_eq!(
                restored.inputs().iter().any(|input| input.id == "trend"),
                name == "Engulfing - Bearish",
                "{name}"
            );
            assert_eq!(restored.settings().plots.len(), 2, "{name}");
            assert!(!restored.plot_style(1).visible, "{name}");
            assert_eq!(
                restored.plot_style(1).plot_type,
                crate::studies::PlotType::BackgroundColor,
                "{name}"
            );
            assert_eq!(restored.plot_style(0).color, color, "{name}");
            let data = with(flat.clone(), bars);
            restored.calculate(&data);
            let marked: Vec<(usize, String)> = (0..data.len())
                .filter_map(|bar| Some((bar, restored.plot_text(0, bar)?)))
                .collect();
            assert_eq!(marked, [(data.len() - 1, abbr.to_owned())], "{name}");
        }
    }

    /// Each pattern's own study offers the trend rule only when its rule reads the trend, and
    /// draws its side's marks and an independently editable Background plot.
    #[test]
    fn single_studies_offer_only_the_settings_they_use() {
        use crate::studies::{IndicatorFactory, PlotType};

        // The rules without a trend condition in the definitions above.
        let trendless = [
            "Doji",
            "Gravestone Doji - Bearish",
            "Kicking - Bearish",
            "Kicking - Bullish",
            "Long Lower Shadow - Bullish",
            "Long Upper Shadow - Bearish",
            "Marubozu Black - Bearish",
            "Marubozu White - Bullish",
            "Spinning Top Black",
            "Spinning Top White",
            "Three Black Crows - Bearish",
            "Three White Soldiers - Bullish",
        ];
        let factory = IndicatorFactory::new();
        let changed = Color32::from_rgb(1, 2, 3);
        for pattern in &PATTERNS {
            let label = pattern.label;
            let mut study = factory.create(label).unwrap();
            let ids: Vec<String> = study.inputs().into_iter().map(|input| input.id).collect();
            let expected: &[&str] = if trendless.contains(&label) {
                &[]
            } else {
                &["trend"]
            };
            assert_eq!(ids, expected, "{label}");
            let (side, color, shape) = if label.ends_with(" - Bullish") {
                ("Bullish", palette::BLUE, PlotType::TriangleUp)
            } else if label.ends_with(" - Bearish") {
                ("Bearish", palette::RED, PlotType::TriangleDown)
            } else {
                ("Neutral", palette::GRAY, PlotType::Circles)
            };
            assert_eq!(study.line_names(), [side, "Background"], "{label}");
            let background = study.plot_style(1);
            assert_eq!(background.plot_type, PlotType::BackgroundColor, "{label}");
            assert!(background.visible && study.plot_color_only(1), "{label}");
            let (r, g, b) = match side {
                "Bullish" => (41, 98, 255),
                "Bearish" => (242, 54, 69),
                _ => (120, 123, 134),
            };
            assert_eq!(
                background.color,
                Color32::from_rgba_unmultiplied(r, g, b, 26),
                "{label}"
            );
            assert_eq!(study.alert_outputs(), [(0, side.into())], "{label}");
            assert_eq!(study.plot_text(1, 0), None);
            assert_eq!(study.plot_tooltip(1, 0), None);
            study.set_color(1, changed);
            assert_eq!(study.plot_style(1).color, changed, "{label}");
            assert_eq!(study.plot_style(0).color, color, "{label}");
            let style = study.plot_style(0);
            assert_eq!((style.color, style.plot_type), (color, shape), "{label}");
            study.set_color(0, changed);
            assert_eq!(study.plot_style(0).color, changed, "{label}");
            let saved = crate::studies::StudyTemplate::from_study(study.as_ref());
            let restored = saved.create().unwrap();
            assert_eq!(restored.settings(), saved.settings, "{label}");
            assert!(restored.plot_style(1).visible, "{label}");
            assert_eq!(restored.plot_style(1).color, changed, "{label}");
        }
    }
}
