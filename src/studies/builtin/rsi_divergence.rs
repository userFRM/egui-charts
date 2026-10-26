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

use super::over_finite_bars;
use super::pivot_points_high_low::pivots;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

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
    rsi_color: Color32,
    bull_color: Color32,
    bear_color: Color32,
    visible: bool,
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
            rsi_color: DESIGN_TOKENS.semantic.extended.accent,
            bull_color: DESIGN_TOKENS.semantic.extended.success,
            bear_color: DESIGN_TOKENS.semantic.extended.error,
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

    /// The values over `data`, every bar of which has a finite close, high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::missing(data.len());
        let mut rsi = super::RSI::new(self.period);
        rsi.calculate(data);
        let rsi = rsi.values().line(0);
        if rsi.len() != data.len() || data.is_empty() {
            return values;
        }
        // Every column exists, so a bar has a value per line.
        values.set_value(8, 0, f64::NAN);
        for (bar, &value) in rsi.iter().enumerate() {
            values.set_value(0, bar, value);
        }
        // (pivot highs, price, [(shown, line, RSI rises, price rises)])
        let kinds = [
            (
                false,
                data.iter().map(|b| b.low).collect::<Vec<_>>(),
                [
                    (self.bullish, 1, true, false),
                    (self.hidden_bullish, 2, false, true),
                ],
            ),
            (
                true,
                data.iter().map(|b| b.high).collect(),
                [
                    (self.bearish, 3, false, true),
                    (self.hidden_bearish, 4, true, false),
                ],
            ),
        ];
        for (high, price, divergences) in kinds {
            let mut previous: Option<usize> = None;
            let found = pivots(rsi, self.left, self.right, high);
            for p in (0..rsi.len()).filter(|&p| found[p]) {
                let Some(q) = previous.replace(p) else {
                    continue;
                };
                if !(self.min_range..=self.max_range).contains(&(p - q - 1)) {
                    continue;
                }
                for (shown, line, rsi_rises, price_rises) in divergences {
                    let moved = |rises: bool, from: f64, to: f64| {
                        if rises { to > from } else { to < from }
                    };
                    if shown
                        && moved(rsi_rises, rsi[q], rsi[p])
                        && moved(price_rises, price[q], price[p])
                    {
                        for bar in q..=p {
                            let t = (bar - q) as f64 / (p - q) as f64;
                            values.set_value(line, bar, rsi[q] + (rsi[p] - rsi[q]) * t);
                        }
                        values.set_value(line + 4, p, rsi[p]);
                    }
                }
            }
        }
        values
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
        self.values = over_finite_bars(
            data,
            |b| b.close.is_finite() && b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
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
