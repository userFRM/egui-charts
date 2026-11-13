//! Relative Strength Index (RSI) indicator.
//!
//! RSI is a momentum oscillator that measures the speed and magnitude of
//! price movements on a 0-100 scale. Developed by J. Welles Wilder Jr.,
//! it is one of the most popular technical indicators for identifying
//! overbought and oversold conditions.
//!
//! # Formula
//!
//! ```text
//! RS = Average Gain / Average Loss   (over N periods, using Wilder's smoothing)
//! RSI = 100 - (100 / (1 + RS))
//! ```
//!
//! The initial average gain/loss is a simple mean of the first `N` changes.
//! Subsequent values use Wilder's smoothing:
//! `Avg = (Prev_Avg * (N-1) + Current) / N`.
//!
//! The formula is evaluated as `100 * Gain / (Gain + Loss)`, which is identical but needs no
//! price-unit floor, so scaling the prices does not change the result. A window with gains and
//! no losses is 100, one with losses and no gains is 0, and a fully flat window is the neutral
//! 50.
//!
//! # Interpretation
//!
//! - RSI > 70: overbought -- potential pullback.
//! - RSI < 30: oversold -- potential bounce.
//! - Divergence between RSI and price often precedes reversals.
//! - Default period: 14.
//!
//! This is a **non-overlay** indicator (drawn in its own sub-pane).
//!
//! # RSI-based moving average
//!
//! [`RSI::default`] also draws a 14-bar simple average of the RSI for reading crossovers. Its type
//! input offers none, SMA, SMA with Bollinger Bands, EMA, SMMA (RMA), WMA and VWMA, with the length
//! and the band deviation as inputs of their own. [`RSI::new`] starts without it.
//!
//! # Divergences
//!
//! With its divergence input on, the RSI also marks the regular divergences between its swings
//! and the price's, found as [`RsiDivergence`](super::RsiDivergence) finds them with its default
//! pivots: 5 bars on each side of a pivot and 5 to 60 bars between two pivots. A pivot low of the
//! RSI above the previous one, on a bar whose low is below that pivot's, is bullish; a pivot high
//! below the previous one, on a bar whose high is above, is bearish. Each is a segment of the RSI
//! between the two pivots and a "Bull" or "Bear" mark on the second. A source other than the
//! close changes the RSI and leaves the highs and lows compared. The input is off by default.
//!
//! # Default colour
//!
//! Purple (#7E57C2), with a one-pixel line, the average yellow (#FDD835) and its bands green
//! (#4CAF50). Bullish divergences are green (#4CAF50) and bearish ones red (#F23645).
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{RSI, Indicator, IndicatorValue};
//!
//! let mut rsi = RSI::new(14);
//! rsi.calculate(&bars);
//! // The first `period` values are warmup.
//! assert!(rsi.values().to_rows()[..14].iter().all(|v| matches!(v, IndicatorValue::None)));
//! assert!(matches!(rsi.values().to_rows()[14], IndicatorValue::Single(_)));
//! ```

use super::rsi_divergence::{Confirmed, MARKS, Pivots, Search};
use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Relative Strength Index (RSI) indicator.
///
/// A bounded momentum oscillator (0-100) that compares the magnitude of
/// recent gains to recent losses. Drawn in a separate sub-pane.
#[derive(Clone)]
pub struct RSI {
    period: usize,
    smoothing: SmoothingType,
    smoothing_length: usize,
    smoothing_deviation: f64,
    divergence: bool,
    values: IndicatorValues,
    stepper: Stepper<Averaged>,
    confirmed: Confirmed,
    color: Color32,
    smoothing_colors: [Color32; 3],
    fill_color: Color32,
    /// The colours of the bullish and the bearish divergences.
    divergence_colors: [Color32; 2],
    visible: bool,
}

/// The RSI, the average of its values and the pivots its divergences compare.
#[derive(Clone, Default)]
struct Averaged {
    rsi: State,
    smoothed: Smoothed,
    bar: usize,
    pivots: Pivots,
}

#[derive(Clone)]
pub(super) struct State {
    previous: Option<f64>,
    usable: usize,
    gain: f64,
    loss: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            previous: None,
            usable: 0,
            gain: -0.0,
            loss: -0.0,
        }
    }
}

/// RSI from Wilder's average gain and loss: 100 with no loss, 0 with no gain, 50 when both are 0.
pub(super) fn from_averages(gain: f64, loss: f64) -> f64 {
    let total = gain + loss;
    if total > 0.0 {
        100.0 * gain / total
    } else {
        50.0
    }
}

impl State {
    pub(super) fn next(&mut self, bar: &Bar, period: usize) -> IndicatorValue {
        if !bar.close.is_finite() {
            return IndicatorValue::None;
        }
        let previous = self.previous.replace(bar.close);
        let i = self.usable;
        self.usable += 1;
        let Some(previous) = previous else {
            return IndicatorValue::None;
        };
        let change = bar.close - previous;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        if i <= period {
            self.gain += gain;
            self.loss += loss;
            if i < period {
                return IndicatorValue::None;
            }
            self.gain /= period as f64;
            self.loss /= period as f64;
        } else {
            self.gain = (self.gain * (period - 1) as f64 + gain) / period as f64;
            self.loss = (self.loss * (period - 1) as f64 + loss) / period as f64;
        }
        IndicatorValue::Single(from_averages(self.gain, self.loss))
    }
}

impl RSI {
    /// Create a new RSI indicator without an RSI-based moving average.
    ///
    /// # Arguments
    /// * `period` -- The lookback period (default convention: 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            smoothing: SmoothingType::None,
            smoothing_length: 14,
            smoothing_deviation: 2.0,
            divergence: false,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            confirmed: Confirmed::default(),
            color: palette::PURPLE,
            smoothing_colors: SMOOTHING_COLORS,
            fill_color: palette::PURPLE.gamma_multiply_u8(26),
            divergence_colors: [palette::GREEN, palette::RED],
            visible: true,
        }
    }

    /// Whether to mark the regular divergences between the RSI and price (builder pattern).
    pub fn with_divergence(mut self, divergence: bool) -> Self {
        self.divergence = divergence;
        self
    }

    /// Which of the four divergence outputs `line` is: the bullish segment, the bearish segment,
    /// the bullish mark or the bearish mark.
    fn divergence_output(&self, line: usize) -> Option<usize> {
        line.checked_sub(1 + self.smoothing.lines())
            .filter(|&output| self.divergence && output < 4)
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.set_colors(vec![color]);
        self
    }
}

/// Construct with the conventional default parameters: a 14-bar RSI and its 14-bar SMA.
impl Default for RSI {
    fn default() -> Self {
        Self {
            smoothing: SmoothingType::Sma,
            ..Self::new(14)
        }
    }
}

impl Indicator for RSI {
    fn level_fill_color(&self) -> Color32 {
        self.fill_color
    }

    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    /// A source stands in for the close and leaves each bar's high and low, which the
    /// divergences compare.
    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        let bars: Vec<_> = data
            .iter()
            .enumerate()
            .map(|(at, bar)| Bar {
                close: source.get(at).copied().unwrap_or(f64::NAN),
                ..bar.clone()
            })
            .collect();
        self.update(&bars, from);
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smoothing: choice("Smoothing", SmoothingType::ALL),
        smoothing_length: length("Smoothing length", 1, usize::MAX)
            if smoothing in SmoothingType::WITH_LENGTH,
        smoothing_deviation: number("Band deviation", 0.0, f64::MAX)
            if smoothing in SmoothingType::WITH_BANDS,
        divergence: toggle("Calculate divergence")
    ; {});

    fn name(&self) -> &str {
        "RSI"
    }

    fn precision(&self) -> Option<usize> {
        Some(2)
    }

    fn desc(&self) -> &str {
        "Relative Strength Index - Momentum oscillator (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if !self.confirmed.begin(&mut self.values, data.len(), from) {
            return;
        }
        let (period, kind, length, deviation) = (
            self.period,
            self.smoothing,
            self.smoothing_length,
            self.smoothing_deviation,
        );
        let first = 1 + kind.lines();
        let search = self.divergence.then(|| Search::regular(first));
        let mut found = Vec::new();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let value = match state.rsi.next(bar, period) {
                    // Without an average the value passes through, so skip the call on every bar.
                    IndicatorValue::Single(rsi) if kind != SmoothingType::None => state
                        .smoothed
                        .next(kind, length, deviation, rsi, bar.volume),
                    other => other,
                };
                let Some(search) = &search else {
                    return value;
                };
                let source = state.bar;
                state.bar += 1;
                let mut row = match value {
                    IndicatorValue::None => Vec::new(),
                    IndicatorValue::Single(rsi) => vec![rsi],
                    IndicatorValue::Multiple(row) => row,
                };
                if [bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                    let rsi = row.first().copied().filter(|v| v.is_finite());
                    state.pivots.next(
                        search,
                        source,
                        rsi.unwrap_or(f64::NAN),
                        bar.high,
                        bar.low,
                        &mut found,
                    );
                }
                if row.is_empty() {
                    return IndicatorValue::None;
                }
                // The divergence outputs are empty until a later bar confirms one.
                row.resize(first + 4, f64::NAN);
                IndicatorValue::Multiple(row)
            });
        self.confirmed.draw(&mut self.values, found, data.len());
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    /// The RSI, the average, its upper and lower band, the band between the levels, and the
    /// bullish and bearish divergences.
    fn colors(&self) -> Vec<Color32> {
        let [average, upper, lower] = self.smoothing_colors;
        let [bullish, bearish] = self.divergence_colors;
        vec![
            self.color,
            average,
            upper,
            lower,
            self.fill_color,
            bullish,
            bearish,
        ]
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        self.smoothing.fills()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
            for (slot, color) in self.smoothing_colors.iter_mut().zip(&colors[1..]) {
                *slot = *color;
            }
            self.fill_color = colors.get(4).copied().unwrap_or_else(|| {
                Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), 16)
            });
            for (slot, color) in self.divergence_colors.iter_mut().zip(colors.iter().skip(5)) {
                *slot = *color;
            }
        }
    }

    /// A divergence's segment and mark share its colour.
    fn set_color(&mut self, line: usize, color: Color32) {
        if let Some(output) = self.divergence_output(line) {
            self.divergence_colors[output % 2] = color;
        } else {
            let mut colors = self.colors();
            if line < colors.len() {
                colors[line] = color;
                self.set_colors(colors);
            }
        }
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        use crate::studies::PlotType;
        let (color, width, plot_type) = match self.divergence_output(line) {
            Some(output) => (
                self.divergence_colors[output % 2],
                2.0,
                match output {
                    2 => PlotType::TriangleUp,
                    3 => PlotType::TriangleDown,
                    _ => PlotType::Line,
                },
            ),
            None => (
                self.colors().get(line).copied().unwrap_or(Color32::WHITE),
                self.line_style(line).width,
                PlotType::Line,
            ),
        };
        crate::studies::PlotStyle {
            color,
            width,
            pattern: crate::model::LineStyle::Solid,
            plot_type,
            visible: true,
        }
    }

    /// The RSI and its average keep their tags on the scale beside the divergence outputs.
    fn plot_tagged(&self, line: usize) -> bool {
        line < 1 + self.smoothing.lines()
    }

    fn plot_label_colors(&self, line: usize) -> Option<(Color32, Color32)> {
        let output = self.divergence_output(line).filter(|&output| output >= 2)?;
        Some((self.divergence_colors[output % 2], Color32::WHITE))
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let output = self.divergence_output(line).filter(|&output| output >= 2)?;
        // Each regular mark is listed before its hidden one.
        let mark = MARKS[(output % 2) * 2];
        self.values.value(line, bar).map(|_| mark.to_string())
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: Some(138.0),
            range: Some((0.0, 100.0)),
            levels: vec![
                (70.0, palette::MUTED, "Overbought"),
                (30.0, palette::MUTED, "Oversold"),
                (50.0, palette::MUTED, "Middle"),
            ],
            fill_between_levels: true,
        }
    }

    fn is_overlay(&self) -> bool {
        false // RSI is drawn in separate pane
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

    fn line_count(&self) -> usize {
        1 + self.smoothing.lines() + 4 * usize::from(self.divergence)
    }

    fn line_names(&self) -> Vec<String> {
        let mut names = vec![format!("RSI({})", self.period)];
        names.extend(self.smoothing.line_names("RSI-based MA"));
        if self.divergence {
            names.extend(
                [
                    "Regular Bullish",
                    "Regular Bearish",
                    "Regular Bullish Label",
                    "Regular Bearish Label",
                ]
                .map(String::from),
            );
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    fn rsi_of(closes: &[f64], period: usize) -> Vec<Option<f64>> {
        let mut rsi = RSI::new(period);
        rsi.calculate(&bars_from_closes(closes));
        rsi.values()
            .to_rows()
            .into_iter()
            .map(|v| match v {
                IndicatorValue::Single(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Endpoints and scale: hand values for period 2 are RSI[2] = 100 * 1 / (1 + 0.5) from the
    /// mean gain 1 and loss 0.5, then 100 * 1.5 / (1.5 + 0.25) after Wilder smoothing. Scaling
    /// the prices must not move them; no loss is 100, no gain 0 and a flat window the neutral 50.
    #[test]
    fn endpoints_and_scale_invariance() {
        let ordinary = [10.0, 12.0, 11.0, 13.0];
        for scale in [1.0, 1e-12, 1e12] {
            let closes: Vec<f64> = ordinary.iter().map(|c| c * scale).collect();
            let got = rsi_of(&closes, 2);
            assert_eq!(got[..2], [None, None]);
            assert!((got[2].unwrap() - 200.0 / 3.0).abs() < 1e-9, "{scale}");
            assert!((got[3].unwrap() - 600.0 / 7.0).abs() < 1e-9, "{scale}");
        }
        assert_eq!(rsi_of(&[1e-12, 2e-12, 3e-12, 4e-12], 2)[2], Some(100.0));
        assert_eq!(rsi_of(&[4e-12, 3e-12, 2e-12, 1e-12], 2)[2], Some(0.0));
        assert_eq!(rsi_of(&[5.0, 5.0, 5.0, 5.0], 2)[2..], [Some(50.0); 2]);
    }

    /// RSI(1) of 1, 2, 1, 2, 3 is missing, 100, 0, 100, 100. The two-bar SMA is their mean: 50, 50,
    /// 100. The two-bar EMA starts from that same first mean, then adds 2/3 of each step: 250/3,
    /// then 850/9. The default study is a 14-bar SMA, first defined on bar 27, the 14th of the RSI
    /// values that start on bar 14.
    #[test]
    fn the_average_of_the_rsi_follows_its_inputs() {
        let bars = bars_from_closes(&[1.0, 2.0, 1.0, 2.0, 3.0]);
        for (smoothing, want) in [
            (1, [50.0, 50.0, 100.0]),
            (3, [50.0, 250.0 / 3.0, 850.0 / 9.0]),
        ] {
            let mut study = RSI::default();
            study
                .set_input("period", crate::studies::InputValue::Length(1))
                .unwrap();
            study
                .set_input("smoothing_length", crate::studies::InputValue::Length(2))
                .unwrap();
            study
                .set_input("smoothing", crate::studies::InputValue::Choice(smoothing))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.values().value(0, 1), Some(100.0));
            assert_eq!(study.plot_value(1, 1), None);
            for (bar, want) in (2..).zip(want) {
                let got = study.values().value(1, bar).unwrap();
                assert!((got - want).abs() < 1e-9, "{smoothing} bar {bar}: {got}");
            }
        }

        let added = crate::studies::IndicatorFactory::new().create("RSI");
        assert_eq!(added.unwrap().line_count(), 2);
        let mut study = RSI::default();
        assert_eq!(study.line_count(), 2);
        assert_eq!(study.line_names()[1], "RSI-based MA");
        assert_eq!(study.plot_style(1).color, palette::YELLOW);
        study.calculate(&bars_from_closes(
            &(0..30).map(|i| f64::from(i % 3)).collect::<Vec<_>>(),
        ));
        assert_eq!(study.plot_value(1, 26), None);
        assert!(study.plot_value(1, 27).is_some());
        study
            .set_input("smoothing", crate::studies::InputValue::Choice(0))
            .unwrap();
        assert_eq!(study.line_count(), 1);
    }

    /// Closes step by 1 from 100, so each rise moves an RSI(2) halfway to 100 and each fall
    /// halves it: 100 on bar 2, five falls to 100/32 on bar 7, five rises to 100 * 993/1024 on
    /// bar 13, four falls to 100 * 993/16384 on bar 17 and three rises to 100 * 115681/131072 on
    /// bar 20. Bar 10 has no close and takes no place among them. Bar 21 falls by 2^-40, which
    /// lowers the RSI by under 1e-10: a tie, so the later bar 21 is the pivot high, not bar 20.
    /// Five falls follow, the last on bar 26.
    ///
    /// Every bar reaches 0.5 above and 0.25 below its close, but bar 17 falls to 90 and bar 21
    /// rises to 110. Pivot lows 7 and 17: the RSI rose and the low fell from 96.75 to 90 while
    /// the close rose, which is bullish over ten usable bars, bar 11 a third of the way along.
    /// Pivot highs 13 and 21: the RSI fell and the high rose from 102.5 to 110 while the close
    /// fell, which is bearish once bar 26 confirms the pivot. A close of 1000 on bar 26 lifts its
    /// RSI over the pivot's and withdraws that divergence.
    #[test]
    fn divergences_follow_the_wicks_through_a_gap_a_tie_and_a_correction() {
        use crate::studies::InputValue;
        let tie = 2f64.powi(-40);
        let runs = [
            (1.0, 2),
            (-1.0, 5),
            (1.0, 2),
            (f64::NAN, 1),
            (1.0, 3),
            (-1.0, 4),
            (1.0, 3),
            (-tie, 1),
            (-1.0, 5),
        ];
        let (mut last, mut closes) = (100.0, vec![100.0]);
        for (step, count) in runs {
            for _ in 0..count {
                if step.is_nan() {
                    closes.push(f64::NAN);
                } else {
                    last += step;
                    closes.push(last);
                }
            }
        }
        let mut bars = bars_from_closes(&closes);
        for bar in &mut bars {
            let near = if bar.close.is_nan() { 100.0 } else { bar.close };
            (bar.high, bar.low) = (near + 0.5, near - 0.25);
        }
        bars[17].low = 90.0;
        bars[21].high = 110.0;
        let (low, high) = (100.0 * 993.0 / 16384.0, 100.0 * 993.0 / 1024.0);
        let tied = 100.0 * 115681.0 / 131072.0 / (1.0 + tie);

        // The four outputs follow the RSI, or the average and its bands.
        for (smoothing, first) in [(0, 1), (2, 4)] {
            let mut study = RSI::new(2);
            study
                .set_input("smoothing", InputValue::Choice(smoothing))
                .unwrap();
            study
                .set_input("divergence", InputValue::Toggle(true))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.line_count(), first + 4);
            let value =
                |study: &RSI, output: usize, bar: usize| study.values().value(first + output, bar);
            let drawn = |study: &RSI, output: usize| -> Vec<usize> {
                (0..bars.len())
                    .filter(|&bar| value(study, output, bar).is_some())
                    .collect()
            };
            assert_eq!(drawn(&study, 0), [7, 8, 9, 11, 12, 13, 14, 15, 16, 17]);
            assert_eq!(value(&study, 0, 7), Some(3.125));
            let third = value(&study, 0, 11).unwrap();
            assert!((third - (3.125 + (low - 3.125) / 3.0)).abs() < 1e-12);
            assert_eq!(value(&study, 0, 17), Some(low));
            assert_eq!(drawn(&study, 2), [17]);
            assert_eq!(value(&study, 2, 17), Some(low));
            assert_eq!(study.plot_text(first + 2, 17).as_deref(), Some("Bull"));

            assert_eq!(drawn(&study, 1), [13, 14, 15, 16, 17, 18, 19, 20, 21]);
            assert_eq!(value(&study, 1, 13), Some(high));
            assert_eq!(drawn(&study, 3), [21]);
            assert!((value(&study, 3, 21).unwrap() - tied).abs() < 1e-11);
            assert_eq!(study.plot_text(first + 3, 21).as_deref(), Some("Bear"));

            // A source takes the place of the close alone, so the wicks still decide.
            let mut sourced = study.clone();
            sourced.update_source(&bars, 0, &closes);
            assert_eq!(
                format!("{:?}", sourced.values()),
                format!("{:?}", study.values())
            );

            // An update that changes nothing leaves the correction after it able to withdraw.
            let mut corrected = bars.clone();
            corrected[26].close = 1000.0;
            study.update(&bars, bars.len());
            study.update(&corrected, 26);
            assert!(drawn(&study, 1).is_empty() && drawn(&study, 3).is_empty());
            assert_eq!(drawn(&study, 0).len(), 10);
        }
    }

    /// The divergence plots follow the average's, so an average with bands puts three plots
    /// before them and no average takes those away again. Each divergence plot keeps the style
    /// set for it and stays off the scale, and the average's plots start with their own.
    #[test]
    fn divergence_styles_stay_with_their_plots_when_the_average_changes() {
        use crate::studies::{ConfiguredStudy, InputValue, PlotType};
        let mut study = ConfiguredStudy::new(Box::new(RSI::new(14).with_divergence(true)));
        let mut settings = study.settings();
        settings.plots[1].color = Color32::BLUE;
        for (smoothing, first) in [(2, 4), (0, 1)] {
            let inputs = [("smoothing".to_owned(), InputValue::Choice(smoothing))];
            study.apply_settings(&inputs, settings).unwrap();
            assert_eq!(study.line_count(), first + 4);
            assert!(first == 1 || study.plot_style(1).color == palette::YELLOW);
            assert_eq!(study.plot_style(first).color, Color32::BLUE);
            assert_eq!(study.plot_style(first + 1).color, palette::RED);
            let marks = [first + 2, first + 3].map(|line| study.plot_style(line).plot_type);
            assert_eq!(marks, [PlotType::TriangleUp, PlotType::TriangleDown]);
            let tagged: Vec<_> = (0..first + 4).filter(|&l| study.plot_tagged(l)).collect();
            assert_eq!(tagged, (0..first).collect::<Vec<_>>());
            settings = study.settings();
        }
        assert_eq!(settings.plots[0].color, palette::PURPLE);
    }
}
