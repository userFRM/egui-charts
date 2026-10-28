//! RSI Divergence Indicator.
//!
//! The RSI with the divergences between its swings and the price's: a regular divergence,
//! where price makes a new extreme the RSI does not confirm, warns of a reversal; a hidden
//! divergence, where the RSI makes the new extreme, of a continuation.
//!
//! # Rule
//!
//! The RSI is Wilder's RSI of the close ([`RSI`](super::RSI)). Its pivot lows and highs are found
//! with `left` and `right` bars on each side as in
//! [`PivotPointsHighLow`](super::PivotPointsHighLow), so a pivot is known `right` bars later.
//! Each pivot low is compared with the previous pivot low `q` when `min_range <= p - q - 1 <=
//! max_range` bars lie between them:
//!
//! ```text
//! Regular bullish: RSI[p] > RSI[q] and low[p] < low[q]
//! Hidden bullish:  RSI[p] < RSI[q] and low[p] > low[q]
//! Regular bearish: RSI[p] < RSI[q] and high[p] > high[q]   (pivot highs)
//! Hidden bearish:  RSI[p] > RSI[q] and high[p] < high[q]   (pivot highs)
//! ```
//!
//! Each divergence is a segment of the RSI from `q` to `p`, the values interpolated so that
//! the line joins the two pivots, and a "Bull", "H Bull", "Bear" or "H Bear" mark at the RSI
//! of `p`. Hidden divergences are off by default. The defaults are an RSI of 14, 5 bars on
//! each side of a pivot and 5 to 60 bars between pivots.
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..120).map(|i| { let p = 100.0 + (i as f64 * 0.2).sin() * (5.0 + i as f64 * 0.05); egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, RsiDivergence};
//!
//! let mut divergence = RsiDivergence::default();
//! divergence.calculate(&bars);
//! // Each "Bull" mark sits on the RSI at the second pivot of a regular bullish divergence.
//! for bar in 0..bars.len() {
//!     if let Some(text) = divergence.plot_text(5, bar) {
//!         assert_eq!(text, "Bull");
//!         assert_eq!(divergence.values().value(5, bar), divergence.values().value(0, bar));
//!     }
//! }
//! ```

use super::rsi::State as RsiState;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// RSI values closer than this are equal: the RSI is the same number whenever the averages
/// scale together, as on a flat bar, but rounds to neighbouring floats.
const RSI_TIE: f64 = 1e-9;

/// Text of the marks on outputs 5 to 8.
const MARKS: [&str; 4] = ["Bull", "H Bull", "Bear", "H Bear"];

/// RSI Divergence Indicator: the RSI with regular and hidden divergences against price.
#[derive(Clone)]
pub struct RsiDivergence {
    period: usize,
    left: usize,
    right: usize,
    min_range: usize,
    max_range: usize,
    bullish: bool,
    hidden_bullish: bool,
    bearish: bool,
    hidden_bearish: bool,
    values: IndicatorValues,
    stepper: Stepper<State>,
    undo: Vec<(usize, usize, f64)>,
    rsi_color: Color32,
    bull_color: Color32,
    bear_color: Color32,
    visible: bool,
}

#[derive(Clone, Copy)]
struct Pivot {
    source: usize,
    position: usize,
    rsi: f64,
    high: f64,
    low: f64,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    position: usize,
    rsi: RsiState,
    window: std::collections::VecDeque<Pivot>,
    previous: [Option<Pivot>; 2],
}

struct Segment {
    line: usize,
    sources: Vec<usize>,
    from: f64,
    to: f64,
    confirming: usize,
}

impl RsiDivergence {
    /// RSI of `period`, pivots with `left` and `right` bars on each side, lengths raised to
    /// at least 1, and `min_range` to `max_range` bars between two pivots compared.
    pub fn new(
        period: usize,
        left: usize,
        right: usize,
        min_range: usize,
        max_range: usize,
    ) -> Self {
        Self {
            period: period.max(1),
            left: left.max(1),
            right: right.max(1),
            min_range,
            max_range,
            bullish: true,
            hidden_bullish: false,
            bearish: true,
            hidden_bearish: false,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            undo: Vec::new(),
            rsi_color: palette::ACCENT,
            bull_color: palette::GREEN,
            bear_color: palette::RED,
            visible: true,
        }
    }

    /// Which divergences to show: regular bullish, hidden bullish, regular bearish and hidden
    /// bearish.
    pub fn with_divergences(
        mut self,
        bullish: bool,
        hidden_bullish: bool,
        bearish: bool,
        hidden_bearish: bool,
    ) -> Self {
        (
            self.bullish,
            self.hidden_bullish,
            self.bearish,
            self.hidden_bearish,
        ) = (bullish, hidden_bullish, bearish, hidden_bearish);
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for RsiDivergence {
    fn default() -> Self {
        Self::new(14, 5, 5, 5, 60)
    }
}

impl Indicator for RsiDivergence {
    crate::studies::inputs::study_inputs!(self;
        period: length("RSI period", 1, usize::MAX),
        right: length("Pivot lookback right", 1, usize::MAX),
        left: length("Pivot lookback left", 1, usize::MAX),
        max_range: length("Max of lookback range", 0, usize::MAX),
        min_range: length("Min of lookback range", 0, usize::MAX),
        bullish: toggle("Plot bullish"),
        hidden_bullish: toggle("Plot hidden bullish"),
        bearish: toggle("Plot bearish"),
        hidden_bearish: toggle("Plot hidden bearish")
    ; {});

    fn name(&self) -> &str {
        "RSI Divergence Indicator"
    }

    fn desc(&self) -> &str {
        "RSI Divergence Indicator - Regular and hidden divergences between RSI and price swings"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        if len > 0 && from == len && data.len() == len {
            return;
        }
        if len > 0 && from == len - 1 && data.len() >= len {
            // A developing confirmation can be withdrawn, restoring the earlier segment it replaced.
            for &(line, bar, value) in &self.undo {
                self.values.set_value(line, bar, value);
            }
        }
        self.undo.clear();
        let mut segments = Vec::new();
        let keep = self
            .left
            .max(self.max_range.saturating_add(1))
            .saturating_add(self.right)
            .saturating_add(1);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let source = state.source;
                state.source += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                    return IndicatorValue::None;
                }
                let rsi = match state.rsi.next(bar, self.period) {
                    IndicatorValue::Single(value) if value.is_finite() => value,
                    _ => f64::NAN,
                };
                let current = Pivot {
                    source,
                    position: state.position,
                    rsi,
                    high: bar.high,
                    low: bar.low,
                };
                state.position += 1;
                state.window.push_back(current);
                if state.window.len() > keep {
                    state.window.pop_front();
                }
                if let Some(at) = state
                    .window
                    .len()
                    .checked_sub(self.right.saturating_add(1))
                    .filter(|&at| at >= self.left)
                {
                    let p = state.window[at];
                    let window = state.window.range(at - self.left..);
                    if window.clone().all(|pivot| pivot.rsi.is_finite()) {
                        for high in [false, true] {
                            let before = state.window.range(at - self.left..at).all(|v| {
                                if high {
                                    v.rsi <= p.rsi + RSI_TIE
                                } else {
                                    v.rsi >= p.rsi - RSI_TIE
                                }
                            });
                            let after = state.window.range(at + 1..).all(|v| {
                                if high {
                                    v.rsi < p.rsi - RSI_TIE
                                } else {
                                    v.rsi > p.rsi + RSI_TIE
                                }
                            });
                            if !(before && after) {
                                continue;
                            }
                            let previous = state.previous[usize::from(high)].replace(p);
                            let Some(q) = previous else {
                                continue;
                            };
                            if !(self.min_range..=self.max_range)
                                .contains(&(p.position - q.position - 1))
                            {
                                continue;
                            }
                            let divergences = if high {
                                [
                                    (self.bearish, 3, false, true),
                                    (self.hidden_bearish, 4, true, false),
                                ]
                            } else {
                                [
                                    (self.bullish, 1, true, false),
                                    (self.hidden_bullish, 2, false, true),
                                ]
                            };
                            for (shown, line, rsi_rises, price_rises) in divergences {
                                // Prices compare exactly, so the rule holds at any price scale.
                                let moved = |rises: bool, from: f64, to: f64, tie: f64| {
                                    if rises {
                                        to > from + tie
                                    } else {
                                        to < from - tie
                                    }
                                };
                                let price =
                                    |pivot: Pivot| if high { pivot.high } else { pivot.low };
                                if shown
                                    && moved(rsi_rises, q.rsi, p.rsi, RSI_TIE)
                                    && moved(price_rises, price(q), price(p), 0.0)
                                {
                                    segments.push(Segment {
                                        line,
                                        sources: state
                                            .window
                                            .iter()
                                            .filter(|v| {
                                                (q.position..=p.position).contains(&v.position)
                                            })
                                            .map(|v| v.source)
                                            .collect(),
                                        from: q.rsi,
                                        to: p.rsi,
                                        confirming: source,
                                    });
                                }
                            }
                        }
                    }
                }
                let mut row = vec![f64::NAN; 9];
                row[0] = rsi;
                IndicatorValue::Multiple(row)
            });
        for segment in segments {
            let last = segment.sources.len() - 1;
            for (position, &bar) in segment.sources.iter().enumerate() {
                if segment.confirming + 1 == data.len() {
                    self.undo.push((
                        segment.line,
                        bar,
                        self.values.value(segment.line, bar).unwrap_or(f64::NAN),
                    ));
                }
                let t = position as f64 / last as f64;
                self.values.set_value(
                    segment.line,
                    bar,
                    segment.from + (segment.to - segment.from) * t,
                );
            }
            let bar = segment.sources[last];
            if segment.confirming + 1 == data.len() {
                self.undo.push((
                    segment.line + 4,
                    bar,
                    self.values.value(segment.line + 4, bar).unwrap_or(f64::NAN),
                ));
            }
            self.values.set_value(segment.line + 4, bar, segment.to);
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        use crate::studies::PlotType;
        let hidden = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 51);
        let (color, plot_type) = match line {
            0 => (self.rsi_color, PlotType::Line),
            1 => (self.bull_color, PlotType::Line),
            2 => (hidden(self.bull_color), PlotType::Line),
            3 => (self.bear_color, PlotType::Line),
            4 => (hidden(self.bear_color), PlotType::Line),
            5 => (self.bull_color, PlotType::TriangleUp),
            6 => (hidden(self.bull_color), PlotType::TriangleUp),
            7 => (self.bear_color, PlotType::TriangleDown),
            _ => (hidden(self.bear_color), PlotType::TriangleDown),
        };
        crate::studies::PlotStyle {
            color,
            width: 2.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type,
            visible: true,
        }
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let mark = MARKS.get(line.checked_sub(5)?)?;
        self.values.value(line, bar).map(|_| mark.to_string())
    }

    fn precision(&self) -> Option<usize> {
        Some(2)
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.rsi_color, self.bull_color, self.bear_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (color, slot) in colors.into_iter().zip([
            &mut self.rsi_color,
            &mut self.bull_color,
            &mut self.bear_color,
        ]) {
            *slot = color;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = Color32::from_gray(128);
        crate::studies::StudyPane {
            height: Some(138.0),
            range: Some((0.0, 100.0)),
            levels: vec![
                (70.0, gray, "Overbought"),
                (50.0, gray, "Middle"),
                (30.0, gray, "Oversold"),
            ],
            fill_between_levels: true,
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
            "RSI",
            "Regular Bullish",
            "Hidden Bullish",
            "Regular Bearish",
            "Hidden Bearish",
            "Regular Bullish Label",
            "Hidden Bullish Label",
            "Regular Bearish Label",
            "Hidden Bearish Label",
        ]
        .map(String::from)
        .to_vec()
    }
}
