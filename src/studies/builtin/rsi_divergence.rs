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

/// Text of the marks of the regular bullish, hidden bullish, regular bearish and hidden bearish
/// divergence.
pub(super) const MARKS: [&str; 4] = ["Bull", "H Bull", "Bear", "H Bear"];

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
    undo: Confirmed,
    rsi_color: Color32,
    fill_color: Color32,
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
    rsi: RsiState,
    pivots: Pivots,
}

/// The pivots a divergence search compares and the outputs it draws each divergence on.
pub(super) struct Search {
    left: usize,
    right: usize,
    min_range: usize,
    max_range: usize,
    /// The segment and mark outputs of the regular bullish, hidden bullish, regular bearish and
    /// hidden bearish divergence, `None` for one that is not shown.
    lines: [Option<(usize, usize)>; 4],
}

impl Search {
    /// The regular divergences between the default pivots: the bullish and bearish segments on
    /// outputs `first` and `first + 1`, their marks on the two outputs after.
    pub(super) fn regular(first: usize) -> Self {
        let RsiDivergence {
            left,
            right,
            min_range,
            max_range,
            ..
        } = RsiDivergence::default();
        Self {
            left,
            right,
            min_range,
            max_range,
            lines: [
                Some((first, first + 2)),
                None,
                Some((first + 1, first + 3)),
                None,
            ],
        }
    }
}

/// The bars a divergence search still compares and the last pivot low and high. A study keeps
/// it in its checkpointed state.
#[derive(Clone, Default)]
pub(super) struct Pivots {
    position: usize,
    window: std::collections::VecDeque<Pivot>,
    previous: [Option<Pivot>; 2],
}

/// A divergence found on bar `confirming`: its segment over `sources`, from one pivot's RSI to
/// the other's, and its mark on the last of them.
pub(super) struct Segment {
    lines: (usize, usize),
    sources: Vec<usize>,
    from: f64,
    to: f64,
    confirming: usize,
}

impl Pivots {
    /// Adds bar `source`, whose prices are finite and whose RSI is NaN until it has a value, and
    /// pushes the divergences that bar confirms onto `found`. A bar with a missing price is not
    /// added, so it takes no place between pivots.
    pub(super) fn next(
        &mut self,
        search: &Search,
        source: usize,
        rsi: f64,
        high: f64,
        low: f64,
        found: &mut Vec<Segment>,
    ) {
        let keep = search
            .left
            .max(search.max_range.saturating_add(1))
            .saturating_add(search.right)
            .saturating_add(1);
        self.window.push_back(Pivot {
            source,
            position: self.position,
            rsi,
            high,
            low,
        });
        self.position += 1;
        if self.window.len() > keep {
            self.window.pop_front();
        }
        let Some(at) = self
            .window
            .len()
            .checked_sub(search.right.saturating_add(1))
            .filter(|&at| at >= search.left)
        else {
            return;
        };
        let p = self.window[at];
        let mut window = self.window.range(at - search.left..);
        if !window.all(|pivot| pivot.rsi.is_finite()) {
            return;
        }
        for high in [false, true] {
            let before = self.window.range(at - search.left..at).all(|v| {
                if high {
                    v.rsi <= p.rsi + RSI_TIE
                } else {
                    v.rsi >= p.rsi - RSI_TIE
                }
            });
            let after = self.window.range(at + 1..).all(|v| {
                if high {
                    v.rsi < p.rsi - RSI_TIE
                } else {
                    v.rsi > p.rsi + RSI_TIE
                }
            });
            if !(before && after) {
                continue;
            }
            let previous = self.previous[usize::from(high)].replace(p);
            let Some(q) = previous else {
                continue;
            };
            if !(search.min_range..=search.max_range).contains(&(p.position - q.position - 1)) {
                continue;
            }
            let divergences = if high {
                [(2, false, true), (3, true, false)]
            } else {
                [(0, true, false), (1, false, true)]
            };
            for (kind, rsi_rises, price_rises) in divergences {
                let Some(lines) = search.lines[kind] else {
                    continue;
                };
                // Prices compare exactly, so the rule holds at any price scale.
                let moved = |rises: bool, from: f64, to: f64, tie: f64| {
                    if rises {
                        to > from + tie
                    } else {
                        to < from - tie
                    }
                };
                let price = |pivot: Pivot| if high { pivot.high } else { pivot.low };
                if moved(rsi_rises, q.rsi, p.rsi, RSI_TIE)
                    && moved(price_rises, price(q), price(p), 0.0)
                {
                    found.push(Segment {
                        lines,
                        sources: self
                            .window
                            .iter()
                            .filter(|v| (q.position..=p.position).contains(&v.position))
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

/// What the divergences confirmed on the last bar replaced. A study keeps it beside its values,
/// so that a correction of that bar can withdraw them.
#[derive(Clone, Default)]
pub(super) struct Confirmed(Vec<(usize, usize, f64)>);

impl Confirmed {
    /// Readies `values` for an update from bar `from` of `len`: a last bar about to be
    /// recalculated gives back what its divergences replaced. `false` when no bar changed, where
    /// the update must do nothing so that a later correction can still withdraw them.
    pub(super) fn begin(&mut self, values: &mut IndicatorValues, len: usize, from: usize) -> bool {
        let old = values.len();
        if old > 0 && from == old && len == old {
            return false;
        }
        if old > 0 && from == old - 1 && len >= old {
            // A developing confirmation can be withdrawn, restoring the earlier segment it replaced.
            for &(line, bar, value) in &self.0 {
                values.set_value(line, bar, value);
            }
        }
        self.0.clear();
        true
    }

    /// Draws `found` on `values` of `len` bars, each segment interpolated so that the line joins
    /// its two pivots.
    pub(super) fn draw(&mut self, values: &mut IndicatorValues, found: Vec<Segment>, len: usize) {
        for segment in found {
            let mut set = |line: usize, bar: usize, value: f64| {
                if segment.confirming + 1 == len {
                    self.0
                        .push((line, bar, values.value(line, bar).unwrap_or(f64::NAN)));
                }
                values.set_value(line, bar, value);
            };
            let (line, mark) = segment.lines;
            let last = segment.sources.len() - 1;
            for (position, &bar) in segment.sources.iter().enumerate() {
                let t = position as f64 / last as f64;
                set(line, bar, segment.from + (segment.to - segment.from) * t);
            }
            set(mark, segment.sources[last], segment.to);
        }
    }
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
            undo: Confirmed::default(),
            rsi_color: palette::BLUE,
            fill_color: palette::LIGHT_BLUE.gamma_multiply_u8(26),
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
    fn level_style(&self, _value: f64) -> crate::model::LineStyle {
        crate::model::LineStyle::Dotted
    }

    fn level_fill_color(&self) -> Color32 {
        self.fill_color
    }

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
        if !self.undo.begin(&mut self.values, data.len(), from) {
            return;
        }
        let search = Search {
            left: self.left,
            right: self.right,
            min_range: self.min_range,
            max_range: self.max_range,
            lines: [
                self.bullish.then_some((1, 5)),
                self.hidden_bullish.then_some((2, 6)),
                self.bearish.then_some((3, 7)),
                self.hidden_bearish.then_some((4, 8)),
            ],
        };
        let mut found = Vec::new();
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
                state
                    .pivots
                    .next(&search, source, rsi, bar.high, bar.low, &mut found);
                let mut row = vec![f64::NAN; 9];
                row[0] = rsi;
                IndicatorValue::Multiple(row)
            });
        self.undo.draw(&mut self.values, found, data.len());
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
            6 => (self.bull_color, PlotType::TriangleUp),
            7 => (self.bear_color, PlotType::TriangleDown),
            _ => (self.bear_color, PlotType::TriangleDown),
        };
        crate::studies::PlotStyle {
            color,
            width: 2.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type,
            visible: true,
        }
    }

    fn plot_label_colors(&self, line: usize) -> Option<(Color32, Color32)> {
        (line >= 5).then_some((
            if line < 7 {
                self.bull_color
            } else {
                self.bear_color
            },
            Color32::WHITE,
        ))
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
        vec![
            self.rsi_color,
            self.bull_color,
            self.bear_color,
            self.fill_color,
        ]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.fill_color = colors.get(3).copied().unwrap_or_else(|| {
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 16)
            });
        }
        for (color, slot) in colors.into_iter().zip([
            &mut self.rsi_color,
            &mut self.bull_color,
            &mut self.bear_color,
        ]) {
            *slot = color;
        }
    }

    fn set_color(&mut self, line: usize, color: Color32) {
        match line {
            0 => self.rsi_color = color,
            1 => self.bull_color = color,
            2 => self.bear_color = color,
            _ => {}
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = palette::GRAY;
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
