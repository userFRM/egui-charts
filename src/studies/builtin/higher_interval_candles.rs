//! Higher-interval candle bodies and wicks built from the chart's own bars.

use crate::model::{Bar, BarClock, Instrument, Timeframe};
use crate::studies::{Indicator, IndicatorValues, InputKind, InputValue, StudyCandle, StudyInput};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Candles spanning the loaded bars of a higher interval, daily by default.
///
/// The chart supplies its input interval and instrument metadata. Direct callers set them through
/// [`Indicator::set_timeframe`] and [`Indicator::set_instrument`] before calculating, and
/// [`Indicator::set_bar_clock`] when their calendar differs from UTC. Input bars
/// must be chronological. The last period is rebuilt on every update, including corrections to
/// its opening bar. Incompatible intervals have no output. No external data is requested.
///
/// [`Indicator::candles`] exposes period OHLCV and source ranges. The two output columns contain
/// each period's close on its source bars, in the up or down column; unchanged closes count as up.
#[derive(Clone)]
pub struct HigherIntervalCandles {
    interval: Timeframe,
    clock: BarClock,
    instrument: Instrument,
    extended_hours: bool,
    candles: Vec<StudyCandle>,
    values: IndicatorValues,
    colors: [Color32; 2],
    visible: bool,
}

impl HigherIntervalCandles {
    /// Aggregate into `interval`. Count intervals cannot be derived from OHLCV bars.
    pub fn new(interval: Timeframe) -> Self {
        Self {
            interval,
            clock: BarClock::new(Timeframe::Min1),
            instrument: Instrument::new(""),
            extended_hours: true,
            candles: Vec::new(),
            values: IndicatorValues::new(),
            colors: [
                DESIGN_TOKENS.semantic.chart.bullish,
                DESIGN_TOKENS.semantic.chart.bearish,
            ],
            visible: true,
        }
    }

    fn intervals(&self) -> Vec<Timeframe> {
        let mut intervals = Timeframe::all();
        if !intervals.contains(&self.interval) {
            intervals.push(self.interval);
        }
        intervals
    }
    fn calculate_from(&mut self, data: &[Bar], start: usize) {
        self.values.truncate(start);
        self.values
            .resize(data.len(), crate::studies::IndicatorValue::None);
        self.candles.retain(|candle| candle.bars.end <= start);
        if start == 0 && !data.is_empty() {
            self.values.set_value(1, 0, f64::NAN);
        }
        if self.clock.timeframe().is_none_or(|source| {
            source.duration_ms() == 0 || self.interval.duration_ms() <= source.duration_ms()
        }) {
            return;
        }
        // A bad direct-call input has no value and does not contaminate the rest of its period.
        let indices: Vec<_> = data
            .iter()
            .enumerate()
            .skip(start)
            .filter(|(_, bar)| crate::data::merge::valid(bar))
            .map(|(i, _)| i)
            .collect();
        let bars: Vec<_> = indices.iter().map(|&i| data[i].clone()).collect();
        let Some(periods) = crate::data::resample_periods(
            &bars,
            self.clock.clone(),
            self.clock.for_timeframe(self.interval),
            &self.instrument,
            self.extended_hours,
        ) else {
            if start > 0 {
                self.calculate_from(data, 0);
            }
            return;
        };
        for (range, bar) in periods {
            let plot = usize::from(bar.close < bar.open);
            for &i in &indices[range.clone()] {
                self.values.set_value(plot, i, bar.close);
            }
            self.candles.push(StudyCandle {
                bars: indices[range.start]..indices[range.end - 1] + 1,
                bar,
                plot,
            });
        }
    }
}

impl Default for HigherIntervalCandles {
    fn default() -> Self {
        Self::new(Timeframe::Day1)
    }
}

impl Indicator for HigherIntervalCandles {
    fn name(&self) -> &str {
        "Higher Interval Candles"
    }
    fn desc(&self) -> &str {
        "Higher-interval OHLC candles built from chart bars"
    }
    fn inputs(&self) -> Vec<StudyInput> {
        let intervals = self.intervals();
        vec![
            StudyInput::new(
                "interval",
                "Interval",
                InputValue::Choice(intervals.iter().position(|t| *t == self.interval).unwrap()),
                InputKind::Choice(intervals.iter().map(ToString::to_string).collect()),
            ),
            StudyInput::new(
                "extended_hours",
                "Include extended hours",
                InputValue::Toggle(self.extended_hours),
                InputKind::Toggle,
            ),
        ]
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        match (id, value) {
            ("interval", InputValue::Choice(index)) => {
                self.interval = *self.intervals().get(index).ok_or("invalid interval")?
            }
            ("extended_hours", InputValue::Toggle(enabled)) => self.extended_hours = enabled,
            _ => return Err("invalid candle input"),
        }
        self.candles.clear();
        self.values.clear();
        Ok(())
    }
    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.clock = self.clock.for_timeframe(timeframe);
    }
    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        if self.clock == *clock {
            return false;
        }
        self.clock = clock.clone();
        true
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let next = instrument.cloned().unwrap_or_else(|| Instrument::new(""));
        let changed =
            self.instrument.time_zone != next.time_zone || self.instrument.session != next.session;
        self.instrument = next;
        changed
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.calculate_from(data, 0);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        let start = self
            .candles
            .last()
            .filter(|candle| {
                from > 0
                    && from >= candle.bars.start
                    && from <= self.values.len()
                    && candle.bars.start < data.len()
            })
            .map_or(0, |candle| candle.bars.start);
        self.calculate_from(data, start);
    }
    fn is_incremental(&self) -> bool {
        !self.candles.is_empty()
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn candles(&self) -> &[StudyCandle] {
        &self.candles
    }
    fn line_names(&self) -> Vec<String> {
        vec!["Up".into(), "Down".into()]
    }
    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
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
    use crate::model::{Session, SessionWindow, TimezoneId};
    use chrono::{NaiveTime, Weekday};

    #[test]
    fn daily_candles_cover_session_gaps_and_replace_the_forming_period() {
        let clock = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
        let instrument = Instrument::new("OVERNIGHT")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap())
            .with_session(Session::new([SessionWindow::new(
                [Weekday::Fri, Weekday::Mon, Weekday::Tue],
                clock(18),
                clock(17),
            )]));
        let bar =
            |time: &str, [o, h, l, c]: [f64; 4]| Bar::new(time.parse().unwrap(), o, h, l, c, 1.0);
        let mut data = vec![
            bar("2024-03-07T23:00:00Z", [17.0, 18.0, 9.0, 12.0]),
            bar("2024-03-08T02:00:00Z", [12.0, 15.0, 11.0, 14.0]),
            bar("2024-03-08T21:00:00Z", [14.0, 16.0, 10.0, 11.0]),
            // A weekend gap and the spring clock change precede the next session.
            bar("2024-03-10T22:00:00Z", [11.0, 12.0, 7.0, 8.0]),
            bar("2024-03-11T03:00:00Z", [8.0, 10.0, 6.0, 9.0]),
            // Tuesday is still forming, with only its first hourly bar available.
            bar("2024-03-11T22:00:00Z", [9.0, 11.0, 8.0, 10.0]),
        ];
        let mut study = crate::studies::IndicatorFactory::new()
            .create("Higher Interval Candles")
            .unwrap();
        study.set_timeframe(Timeframe::Hour1);
        study.set_instrument(Some(&instrument));
        study.calculate(&data);
        let check = |study: &dyn Indicator, expected: &[[f64; 4]]| {
            assert_eq!(study.candles().len(), 3);
            assert_eq!(study.values().len(), expected.len());
            for (i, expected) in expected.iter().enumerate() {
                let candle = study
                    .candles()
                    .iter()
                    .find(|c| c.bars.contains(&i))
                    .unwrap();
                let bar = &candle.bar;
                assert_eq!(
                    [bar.open, bar.high, bar.low, bar.close],
                    *expected,
                    "source bar {i}"
                );
                assert_eq!(study.values().value(candle.plot, i), Some(bar.close));
            }
        };
        let mut expected = vec![[17.0, 18.0, 9.0, 11.0]; 3];
        expected.extend([[11.0, 12.0, 6.0, 9.0]; 2]);
        expected.push([9.0, 11.0, 8.0, 10.0]);
        check(study.as_ref(), &expected);
        assert_eq!(
            study
                .candles()
                .iter()
                .map(|c| c.bar.time.to_rfc3339())
                .collect::<Vec<_>>(),
            [
                "2024-03-08T00:00:00+00:00",
                "2024-03-11T00:00:00+00:00",
                "2024-03-12T00:00:00+00:00"
            ]
        );
        // Correct the forming bar from up to down; neither an old high nor an up output survives.
        data[5] = bar("2024-03-11T22:00:00Z", [9.0, 10.0, 5.0, 6.0]);
        study.update(&data, 5);
        expected[5] = [9.0, 10.0, 5.0, 6.0];
        check(study.as_ref(), &expected);
        assert_eq!(study.values().value(0, 5), None);
        data.push(bar("2024-03-11T23:00:00Z", [6.0, 12.0, 4.0, 11.0]));
        study.update(&data, 6);
        expected[5] = [9.0, 12.0, 4.0, 11.0];
        expected.push(expected[5]);
        check(study.as_ref(), &expected);
        assert_eq!(study.values().value(1, 5), None);
        #[cfg(feature = "widget")]
        {
            use crate::chart::coords::ChartMapping;
            use crate::chart::renderers::{IndicatorRenderer, RenderContext};
            use crate::scales::{PriceMap, PriceRange, PriceScaleMode, PriceTransform};
            use crate::studies::ConfiguredStudy;
            use egui::{Pos2, Rect, Shape};
            let mut options = crate::config::ChartOptions::default();
            options.time_scale.timezone =
                crate::scales::TimezoneMode::Timezone(chrono_tz::Asia::Tokyo);
            let mut chart = crate::widget::ChartBuilder::new()
                .with_chart_options(options)
                .build();
            chart.set_timeframe(Timeframe::Day1);
            chart.set_instrument(Some(
                Instrument::new("TOKYO").with_time_zone(TimezoneId::named("Asia/Tokyo").unwrap()),
            ));
            let monday = Bar::new(
                "2024-03-10T15:00:00Z".parse().unwrap(),
                10.0,
                13.0,
                9.0,
                12.0,
                1.0,
            );
            chart.set_data(crate::widget::SeriesId::MAIN, vec![monday.clone()]);
            let id = chart.add_indicator(Box::new(HigherIntervalCandles::new(Timeframe::Week1)));
            assert_eq!(
                chart
                    .indicators()
                    .find(|(candidate, _)| *candidate == id)
                    .unwrap()
                    .1
                    .candles()[0]
                    .bar
                    .time,
                monday.time
            );
            // Three hourly centres are 59, 99 and 139; the period covers x=41..157.
            // A linear 0..20 map places O=10 at y=50 and C=15 at y=25.
            let start: chrono::DateTime<chrono::Utc> = "2024-03-08T00:00:00Z".parse().unwrap();
            let bars: Vec<_> = (0..3)
                .map(|i| {
                    Bar::new(
                        start + chrono::TimeDelta::hours(i),
                        10.0,
                        18.0,
                        2.0,
                        15.0,
                        1.0,
                    )
                })
                .collect();
            for (mode, inverted, y_open, y_close, y_high, y_low) in [
                (
                    PriceScaleMode::Normal,
                    false,
                    50.0_f32,
                    25.0_f32,
                    10.0,
                    90.0,
                ),
                (PriceScaleMode::Normal, true, 50.0, 75.0, 90.0, 10.0),
                // log10(1)..log10(100): O=10 is halfway, C=15 at 100-50*log10(15).
                (
                    PriceScaleMode::Logarithmic,
                    false,
                    50.0,
                    41.195435,
                    37.236374,
                    84.9485,
                ),
            ] {
                let mut study = ConfiguredStudy::new(Box::new(HigherIntervalCandles::default()));
                study.set_timeframe(Timeframe::Hour1);
                study.calculate(&bars);
                let mut settings = study.settings();
                settings.plots[0].color = Color32::BLUE;
                study.apply_settings(&[], settings).unwrap();
                let mut state =
                    crate::model::ChartState::new(crate::model::BarData::from_bars(bars.clone()));
                state.time_scale_mut().set_width(40.0);
                state.time_scale_mut().set_bar_spacing(40.0);
                state.price_scale_mut().options_mut().mode = mode;
                let transform = state.fit_price_scale_with_series(
                    [&study as &dyn Indicator],
                    false,
                    None,
                    std::iter::empty(),
                );
                let fitted = state.price_scale().range();
                assert!(
                    transform.to_price(fitted.min) <= 2.0 && transform.to_price(fitted.max) >= 18.0,
                    "the visible portion of a period must fit its complete wick"
                );
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 100.0));
                let range = if mode == PriceScaleMode::Normal {
                    PriceRange::new(0.0, 20.0)
                } else {
                    PriceRange::new(0.0, 100.0_f64.ln())
                };
                let price = PriceMap::new(
                    PriceTransform { mode, base: 1.0 },
                    range,
                    0.0,
                    100.0,
                    inverted,
                );
                let coords = ChartMapping::new(rect, 40.0, 0, 3, 0.0, price);
                let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    IndicatorRenderer::render_study(
                        &RenderContext::new(ui.painter(), rect),
                        &study,
                        &bars,
                        0,
                        &price,
                        &coords,
                    );
                });
                let body = output.shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::Rect(body) if body.stroke.color == Color32::BLUE => Some(body.rect),
                    _ => None,
                });
                let wick = output.shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::LineSegment { points, stroke } if stroke.color == Color32::BLUE => {
                        Some(*points)
                    }
                    _ => None,
                });
                output.drop_without_applying_deltas();
                let body = body.expect("period body must replace the close-price line");
                let wick = wick.expect("period wick");
                for (actual, expected) in [
                    (body.left(), 41.0),
                    (body.right(), 157.0),
                    (body.top(), y_open.min(y_close)),
                    (body.bottom(), y_open.max(y_close)),
                    (wick[0].x, 99.0),
                    (wick[0].y, y_high),
                    (wick[1].y, y_low),
                ] {
                    assert!(
                        (actual - expected).abs() < 0.001,
                        "{mode:?}, inverted {inverted}: {actual} != {expected}"
                    );
                }
            }
        }
    }
}
