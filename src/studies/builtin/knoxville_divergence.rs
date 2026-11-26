//! Knoxville Divergence.
//!
//! Marks a new high or low that momentum does not confirm after an RSI extreme: a new low with
//! stronger momentum is bullish, a new high with weaker momentum is bearish.
//!
//! # Rule
//!
//! Momentum is the close minus the close `momentum_period` bars before, and the RSI is Wilder's
//! ([`RSI`](super::RSI)) of the close. Each bar `t` is compared with the `lookback` bars before
//! it, or with all earlier bars while there are fewer. For a bearish signal `e` is the one of them
//! with the highest high, for a bullish signal the one with the lowest low, the latest when
//! several share it:
//!
//! ```text
//! Bearish: high[t] > high[e]  and momentum[t] < momentum[e]  and RSI >= overbought on a bar of e..=t
//! Bullish: low[t]  < low[e]   and momentum[t] > momentum[e]  and RSI <= oversold on a bar of e..=t
//! ```
//!
//! A signal is marked on `t`, at its high for a bearish signal and its low for a bullish one, and
//! never moves once `t` is complete. [`price_segments`](Indicator::price_segments) joins the high
//! of `e` to the high of `t` (the lows for a bullish signal) on the price chart in the signal's
//! style. A momentum or RSI without a value, in the warm-up, takes no part. Bars with a
//! non-finite high, low or close are skipped and do not count toward `lookback`. The study draws
//! on the price chart, and its momentum is its only legend readout: the divergence values only
//! place the marks, and the price scale starts without their tags. The defaults are a momentum of
//! 20 bars, an RSI of 21, a look-back of 150 bars, and an RSI of 30 and 70. Price divergence lines
//! start enabled; older saves keep them off until enabled.

use super::rsi::State as RsiState;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudySegment};
use egui::Color32;
use std::collections::VecDeque;

/// Knoxville Divergence: new highs and lows that momentum does not confirm after an RSI extreme.
#[derive(Clone, Debug)]
pub struct KnoxvilleDivergence {
    momentum_period: usize,
    rsi_period: usize,
    lookback: usize,
    oversold: usize,
    overbought: usize,
    price_lines: bool,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: [Color32; 3],
    visible: bool,
}

/// A bar of the look-back window that a later bar may be compared with.
#[derive(Clone, Copy, Debug)]
struct Extreme {
    /// Its position among the finite bars.
    position: usize,
    /// Its source bar.
    bar: usize,
    /// Its low, or its high.
    price: f64,
    momentum: f64,
}

#[derive(Clone, Debug, Default)]
struct State {
    /// Source bars stepped, non-finite ones included.
    bars: usize,
    /// Finite bars stepped.
    position: usize,
    /// The divergence lines found so far.
    segments: Vec<StudySegment>,
    rsi: RsiState,
    closes: VecDeque<f64>,
    /// Window candidates, oldest first, with the latest of equal extremes at the front.
    extremes: [VecDeque<Extreme>; 2],
    /// Latest finite positions whose RSI reached the oversold and overbought limits.
    limits: [Option<usize>; 2],
}

impl KnoxvilleDivergence {
    /// Momentum and RSI of the given periods, and signals against the extremes of the `lookback`
    /// bars before each bar, each raised to at least 1, with RSI limits of 30 and 70, set through
    /// the `oversold` and `overbought` inputs.
    pub fn new(momentum_period: usize, rsi_period: usize, lookback: usize) -> Self {
        Self {
            momentum_period: momentum_period.max(1),
            rsi_period: rsi_period.max(1),
            lookback: lookback.max(1),
            oversold: 30,
            overbought: 70,
            price_lines: true,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [palette::ACCENT, palette::GREEN, palette::RED],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for KnoxvilleDivergence {
    fn default() -> Self {
        Self::new(20, 21, 150)
    }
}

impl Indicator for KnoxvilleDivergence {
    crate::studies::inputs::study_inputs!(self;
        momentum_period: length("Momentum period", 1, usize::MAX),
        rsi_period: length("RSI period", 1, usize::MAX),
        lookback: length("Bars back", 1, usize::MAX),
        oversold: length("RSI oversold", 0, 100),
        overbought: length("RSI overbought", 0, 100),
        price_lines: toggle("Price divergence lines")
    ; {});

    fn name(&self) -> &str {
        "Knoxville Divergence"
    }

    fn desc(&self) -> &str {
        "Knoxville Divergence - New highs and lows that momentum does not confirm after an RSI extreme"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.bars;
                state.bars += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                    return IndicatorValue::None;
                }
                let rsi = match state.rsi.next(bar, self.rsi_period) {
                    IndicatorValue::Single(value) if value.is_finite() => value,
                    _ => f64::NAN,
                };
                state.closes.push_back(bar.close);
                if state.closes.len() > self.momentum_period.saturating_add(1) {
                    state.closes.pop_front();
                }
                let momentum = if state.closes.len() > self.momentum_period {
                    bar.close - state.closes[0]
                } else {
                    f64::NAN
                };
                let position = state.position;
                state.position += 1;
                // A limit reached on this bar counts for its own comparison.
                if rsi <= self.oversold as f64 {
                    state.limits[0] = Some(position);
                }
                if rsi >= self.overbought as f64 {
                    state.limits[1] = Some(position);
                }
                let first = position.saturating_sub(self.lookback);
                let mut row = [momentum, f64::NAN, f64::NAN];
                for high in [false, true] {
                    let side = usize::from(high);
                    let price = if high { bar.high } else { bar.low };
                    let extremes = &mut state.extremes[side];
                    while extremes.front().is_some_and(|e| e.position < first) {
                        extremes.pop_front();
                    }
                    if let Some(&e) = extremes.front() {
                        // A NaN momentum fails both comparisons.
                        let diverges = if high {
                            price > e.price && momentum < e.momentum
                        } else {
                            price < e.price && momentum > e.momentum
                        };
                        if diverges && state.limits[side].is_some_and(|at| at >= e.position) {
                            row[1 + side] = price;
                            state.segments.push(StudySegment {
                                start_bar: e.bar,
                                start_price: e.price,
                                end_bar: index,
                                end_price: price,
                                plot: 1 + side,
                            });
                        }
                    }
                    // A bar at least as extreme replaces the earlier candidates.
                    while extremes.back().is_some_and(|e| {
                        if high {
                            e.price <= price
                        } else {
                            e.price >= price
                        }
                    }) {
                        extremes.pop_back();
                    }
                    extremes.push_back(Extreme {
                        position,
                        bar: index,
                        price,
                        momentum,
                    });
                }
                IndicatorValue::Multiple(row.to_vec())
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 => crate::studies::PlotType::Line,
                1 => crate::studies::PlotType::TriangleUp,
                _ => crate::studies::PlotType::TriangleDown,
            },
            visible: true,
        }
    }

    fn plot_shape_side(&self, line: usize) -> crate::studies::ShapeSide {
        // A bullish divergence marks its bar's low, a bearish one its high.
        match line {
            1 => crate::studies::ShapeSide::Below,
            2 => crate::studies::ShapeSide::Above,
            _ => crate::studies::ShapeSide::Centred,
        }
    }

    fn readout_only(&self, line: usize) -> bool {
        // Momentum is far from the prices it would share the price scale with.
        line == 0
    }

    fn price_segments(&self) -> &[StudySegment] {
        if self.price_lines {
            &self.stepper.state().segments
        } else {
            &[]
        }
    }

    // The divergence values only place their marks; the momentum is the study's reading.
    fn legend_lines(&self) -> Vec<usize> {
        vec![0]
    }

    fn plot_tagged(&self, line: usize) -> bool {
        line == 0
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

    fn is_overlay(&self) -> bool {
        true
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    // Its price segments sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        ["Momentum", "Bullish Divergence", "Bearish Divergence"]
            .map(String::from)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::InputValue;

    /// New window extremes with weaker momentum are marked immediately, gated by an RSI
    /// extreme since the compared bar. Warm-up, ties, gaps and live corrections keep that rule.
    #[test]
    fn divergences_are_marked_on_the_bar_that_breaks_the_window_extreme() {
        let study = |inputs: &[(&str, usize)]| {
            let mut study = KnoxvilleDivergence::default();
            for &(id, value) in inputs {
                study.set_input(id, InputValue::Length(value)).unwrap();
            }
            study
        };
        let bars = |closes: &[f64]| {
            super::super::hlc_bars(&closes.iter().map(|&c| (c, c, c)).collect::<Vec<_>>())
        };
        let closes: Vec<_> = (0..120)
            .map(|i| {
                if i < 60 {
                    100.0 + i as f64
                } else {
                    159.0 + 0.5 * (i - 59) as f64
                }
            })
            .collect();
        // RSI is 100 (0 in the mirror). Momentum falls from 20 to 10 on bars 60..80,
        // then stays at 10, so exactly those 20 bars diverge from the preceding extreme.
        for (sign, plot) in [(1.0, 2), (-1.0, 1)] {
            let prices: Vec<_> = closes.iter().map(|c| sign * c).collect();
            let bars = bars(&prices);
            let mut kd = study(&[]);
            kd.calculate(&bars);
            for (t, &price) in prices.iter().enumerate() {
                assert_eq!(
                    kd.values().value(plot, t),
                    (60..80).contains(&t).then_some(price),
                    "bar {t}, plot {plot}"
                );
                assert_eq!(kd.values().value(3 - plot, t), None);
            }
            let expected: Vec<_> = (60..80)
                .map(|t| StudySegment {
                    start_bar: t - 1,
                    start_price: prices[t - 1],
                    end_bar: t,
                    end_price: prices[t],
                    plot,
                })
                .collect();
            assert_eq!(kd.price_segments(), expected);
            assert_eq!(
                kd.price_segments().first(),
                Some(&StudySegment {
                    start_bar: 59,
                    start_price: sign * 159.0,
                    end_bar: 60,
                    end_price: sign * 159.5,
                    plot,
                })
            );
            assert_eq!(
                kd.price_segments().last(),
                Some(&StudySegment {
                    start_bar: 78,
                    start_price: sign * 168.5,
                    end_bar: 79,
                    end_price: sign * 169.0,
                    plot,
                })
            );
            super::super::assert_replay_matches_batch(&kd, &bars);
        }
        let closes = [10.0, 12.0, 14.0, 11.0, 14.5, 12.5, 14.6];
        // RSI(2): 100, 40, 75, 45, 70.11 from bar 2. The 75 at the extreme counts;
        // the 100 before it does not. A one-bar window compares with bar 5 instead.
        for (sign, lookback, limit, signal) in [
            (1.0, 10, 72, true),
            (1.0, 10, 80, false),
            (1.0, 2, 72, true),
            (1.0, 1, 70, false),
            (-1.0, 10, 28, true),
            (-1.0, 10, 20, false),
        ] {
            let prices: Vec<_> = closes.iter().map(|c| sign * c).collect();
            let bars = bars(&prices);
            let mut kd = study(&[
                ("momentum_period", 1),
                ("rsi_period", 2),
                ("lookback", lookback),
                (if sign > 0.0 { "overbought" } else { "oversold" }, limit),
            ]);
            kd.calculate(&bars);
            let plot = if sign > 0.0 { 2 } else { 1 };
            for t in 0..bars.len() {
                assert_eq!(
                    kd.values().value(plot, t),
                    (signal && t == 6).then_some(sign * 14.6),
                    "sign {sign}, lookback {lookback}, limit {limit}, bar {t}"
                );
                assert_eq!(kd.values().value(3 - plot, t), None);
            }
            let expected: Vec<_> = signal
                .then_some(StudySegment {
                    start_bar: 4,
                    start_price: sign * 14.5,
                    end_bar: 6,
                    end_price: sign * 14.6,
                    plot,
                })
                .into_iter()
                .collect();
            assert_eq!(kd.price_segments(), expected);
            if sign > 0.0 && lookback == 10 && limit == 72 {
                super::super::assert_replay_matches_batch(&kd, &bars);
            }
        }
        let mut kd = study(&[("momentum_period", 1), ("rsi_period", 2), ("lookback", 10)]);
        kd.calculate(&bars(&[10.0, 14.5, 12.0, 14.5, 13.0, 14.6]));
        assert_eq!(
            kd.values().value(2, 3),
            None,
            "an equal high is not a breakout"
        );
        assert_eq!(
            kd.price_segments(),
            [StudySegment {
                start_bar: 3,
                start_price: 14.5,
                end_bar: 5,
                end_price: 14.6,
                plot: 2,
            }],
            "the latest equal extreme supplies the momentum and the line start"
        );

        let mut kd = study(&[
            ("momentum_period", 1),
            ("rsi_period", 2),
            ("lookback", 10),
            ("overbought", 72),
        ]);
        let bars = bars(&closes);
        let line = StudySegment {
            start_bar: 4,
            start_price: 14.5,
            end_bar: 6,
            end_price: 14.6,
            plot: 2,
        };
        let mut gapped = super::super::hlc_bars(&[(f64::NAN, f64::NAN, f64::NAN)]);
        gapped.extend(bars.iter().cloned());
        kd.calculate(&gapped);
        assert_eq!(kd.values().value(2, 7), Some(14.6));
        assert_eq!(
            kd.price_segments(),
            [StudySegment {
                start_bar: 5,
                end_bar: 7,
                ..line
            }]
        );
        // A gap inside the two-bar window consumes no finite position.
        let mut narrow = study(&[
            ("momentum_period", 1),
            ("rsi_period", 2),
            ("lookback", 2),
            ("overbought", 72),
        ]);
        let mut gapped = bars.clone();
        gapped.insert(
            6,
            super::super::hlc_bars(&[(f64::NAN, f64::NAN, f64::NAN)])[0].clone(),
        );
        narrow.calculate(&gapped);
        assert_eq!(
            narrow.price_segments(),
            [StudySegment { end_bar: 7, ..line }]
        );
        kd.calculate(&bars);
        let mut tick = bars.clone();
        (tick[6].high, tick[6].low, tick[6].close) = (14.4, 14.4, 14.4);
        kd.update(&tick, 6);
        assert_eq!(kd.values().value(2, 6), None);
        assert!(kd.price_segments().is_empty());
        kd.update(&bars, 6);
        assert_eq!(kd.price_segments(), [line]);
        kd.calculate(&[]);
        assert!(kd.price_segments().is_empty());

        // One outside bar can break both extremes. Its momentum -1 lies between -3 at
        // the earlier low and +2 at the earlier high; RSI reaches 40 and then 66.67.
        let bars = super::super::hlc_bars(&[
            (11.0, 9.0, 10.0),
            (14.0, 11.0, 12.0),
            (12.0, 7.0, 9.0),
            (13.0, 8.0, 11.0),
            (15.0, 6.0, 10.0),
        ]);
        let mut kd = study(&[
            ("momentum_period", 1),
            ("rsi_period", 2),
            ("lookback", 10),
            ("oversold", 40),
            ("overbought", 60),
        ]);
        kd.calculate(&bars);
        assert_eq!(kd.values().value(1, 4), Some(6.0));
        assert_eq!(kd.values().value(2, 4), Some(15.0));
        assert_eq!(
            kd.price_segments(),
            [
                StudySegment {
                    start_bar: 2,
                    start_price: 7.0,
                    end_bar: 4,
                    end_price: 6.0,
                    plot: 1
                },
                StudySegment {
                    start_bar: 1,
                    start_price: 14.0,
                    end_bar: 4,
                    end_price: 15.0,
                    plot: 2
                },
            ]
        );
    }

    /// A price line appears when the chart receives the bar that makes the divergence, and
    /// the study adds no pane.
    #[cfg(feature = "widget")]
    #[test]
    fn divergence_lines_are_drawn_on_the_price_chart() {
        use crate::model::{BarData, LogicalRange};
        use crate::widget::{Chart, SeriesId};

        let bars = super::super::hlc_bars(&[10.0, 8.0, 9.0, 7.5, 8.5].map(|c| (c, c, c)));
        let mut chart = Chart::new(BarData::from_bars(bars[..3].to_vec()));
        let mut study = crate::studies::IndicatorFactory::new()
            .create("Knoxville Divergence")
            .unwrap();
        for (id, value) in [("momentum_period", 1), ("rsi_period", 2), ("lookback", 10)] {
            study.set_input(id, InputValue::Length(value)).unwrap();
        }
        let panes = chart.panes();
        let id = chart.add_indicator(study);
        assert_eq!(chart.panes(), panes, "the study draws on the price pane");
        let ctx = egui::Context::default();
        let frame = |chart: &mut Chart| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            );
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(path)
                        if !path.closed
                            && path.points.len() == 2
                            && path.stroke.color
                                == egui::epaint::ColorMode::Solid(palette::GREEN) =>
                    {
                        Some([path.points[0], path.points[1]])
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            lines
        };
        frame(&mut chart);
        chart.set_visible_logical_range(LogicalRange::new(-1.0, 6.0));
        assert!(
            frame(&mut chart).is_empty(),
            "no line before the divergence bar"
        );
        chart.update(SeriesId::MAIN, bars[3].clone());
        frame(&mut chart);
        let lines = frame(&mut chart);
        assert_eq!(lines.len(), 1);
        let [start, end] = lines[0];
        let x = |bar: f64| chart.index_to_x(bar).unwrap();
        assert!(
            (start.x - x(1.0)).abs() < 0.5 && (end.x - x(3.0)).abs() < 0.5,
            "{lines:?}"
        );
        assert!(start.y < end.y, "the low of 8 lies above the low of 7.5");
        let price = chart.get_rendered_price_rect();
        assert!(
            price.contains(start) && price.contains(end),
            "{lines:?} in {price:?}"
        );
        let study = chart.indicator_mut(id).unwrap();
        let mut settings = study.settings();
        settings.plots[1].visible = false;
        study.apply_settings(&[], settings).unwrap();
        assert!(
            frame(&mut chart).is_empty(),
            "hiding the signal also hides its price line"
        );
    }
}
