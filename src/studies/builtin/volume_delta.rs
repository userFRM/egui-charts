//! Volume delta candles from lower-interval candles supplied by the host.
//!
//! Each candle inside a chart bar adds its volume to the bar's delta when it buys and subtracts
//! it when it sells, as [`UpDownVolume`](super::UpDownVolume) classifies them: a candle buys when
//! it closes above its open, or at its open above the previous candle's close, sells when it
//! closes below either, and otherwise takes the side of the candle before it.
//! [`VolumeDelta`] draws each bar as a candle that opens at zero, closes at the bar's delta and
//! spans the lowest and highest running delta after each of its candles.
//! [`CumulativeVolumeDelta`] adds the bar deltas from the first bar of each anchor period, the
//! session by default, and draws each bar from the total at its open to the total at its close.
//! A bar without candles has no value and no candle, and the total carries over it.
//! Both panes draw a dashed grey zero level.
//!
//! [`Indicator::candles`] holds one candle per bar with a value, on plot 1 ("Up") when it closes
//! above its open and on plot 2 ("Down") otherwise, so a Volume Delta bar whose delta is zero is
//! drawn as selling; a Cumulative Volume Delta bar whose total ends where it opened is drawn as
//! buying. Output 0 ("Delta") holds every close,
//! for studies that take this one as their source, and is not drawn; outputs 1 and 2 hold the
//! close in the column of its candle's plot.

use super::anchor_period::AnchorPeriod;
use super::lower_interval;
use crate::model::{Bar, BarClock, LineStyle, Timeframe};
use crate::scales::StudyFormat;
use crate::studies::palette;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, PlotStyle, PlotType,
    StudyCandle, StudyData, StudyDataRequest, StudyInput, StudyTable,
};
use egui::Color32;

const PERIODS: [AnchorPeriod; 7] = [
    AnchorPeriod::Session,
    AnchorPeriod::Week,
    AnchorPeriod::Month,
    AnchorPeriod::Quarter,
    AnchorPeriod::Year,
    AnchorPeriod::Decade,
    AnchorPeriod::Century,
];

#[derive(Clone, Debug)]
struct DeltaCandles {
    clock: BarClock,
    /// The interval chosen in the Lower interval input; `None` picks one from the chart interval.
    lower_interval: Option<Timeframe>,
    /// The period from whose first bar the deltas add up; `None` starts every bar at zero.
    anchor: Option<AnchorPeriod>,
    candles: Vec<StudyCandle>,
    values: IndicatorValues,
    colors: [Color32; 3],
    visible: bool,
    /// Shown while the chosen lower interval is not read.
    notice: Option<StudyTable>,
}

impl DeltaCandles {
    fn new(anchor: Option<AnchorPeriod>) -> Self {
        Self {
            clock: BarClock::new(Timeframe::Min1),
            lower_interval: None,
            anchor,
            candles: Vec::new(),
            values: IndicatorValues::new(),
            colors: [palette::BLUE, palette::UP, palette::DOWN],
            visible: true,
            notice: None,
        }
    }

    fn lower(&self) -> Option<Timeframe> {
        lower_interval::lower_timeframe(&self.clock, self.lower_interval)
    }

    fn inputs(&self) -> Vec<StudyInput> {
        let mut inputs = vec![lower_interval::input(&self.clock, self.lower_interval)];
        if let Some(anchor) = self.anchor {
            inputs.push(
                StudyInput::new(
                    "anchor",
                    "Anchor period",
                    InputValue::Choice(PERIODS.iter().position(|p| *p == anchor).unwrap_or(0)),
                    InputKind::Choice(PERIODS.iter().map(|p| format!("{p:?}")).collect()),
                )
                .with_default(InputValue::Choice(0)),
            );
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        match (id, value) {
            ("lower_interval", InputValue::Timeframe(chosen)) => self.lower_interval = chosen,
            ("anchor", InputValue::Choice(at)) if self.anchor.is_some() && at < PERIODS.len() => {
                self.anchor = Some(PERIODS[at]);
            }
            _ => return Err(crate::studies::StudyError::rejected(&self.inputs(), id)),
        }
        self.refresh(&[], 0, &[]);
        Ok(())
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let from = from.min(self.values.len()).min(data.len());
        let candles = match self.lower() {
            Some(lower) => {
                match lower_interval::reply(&lower_interval::request(lower), from, sources) {
                    Some(bars) => bars,
                    None => return,
                }
            }
            None => &[],
        };
        self.refresh(data, from, candles);
    }

    /// Redraws the bars of `data[from..]` from `candles`; without candles no bar has a value.
    fn refresh(&mut self, data: &[Bar], from: usize, candles: &[Bar]) {
        let from = if candles.is_empty() {
            0
        } else {
            from.min(self.values.len()).min(data.len())
        };
        self.candles.truncate(
            self.candles
                .partition_point(|candle| candle.bars.start < from),
        );
        if from == 0 {
            self.values = IndicatorValues::missing(data.len());
        } else {
            self.values.truncate(from);
            self.values.resize(data.len(), IndicatorValue::None);
        }
        let Some(lower) = self.lower().filter(|_| !candles.is_empty()) else {
            return;
        };
        let sessions = self.clock.sessions();
        let anchor = self.anchor;
        let period = |bar: &Bar| anchor.map(|anchor| anchor.key(sessions.trading_date(bar.time)));
        let mut last = self
            .candles
            .last()
            .map(|candle| (period(&data[candle.bars.start]), candle.bar.close));
        let ranges = lower_interval::nest(&self.clock, lower, data, candles, from);
        let mut sides = lower_interval::Sides::new(candles);
        for (i, range) in (from..).zip(ranges) {
            let bar = &data[i];
            if range.is_empty() || !bar.close.is_finite() {
                continue;
            }
            let flow = sides.flow(range);
            let key = period(bar);
            let open = match last {
                Some((at, total)) if anchor.is_some() && at == key => total,
                _ => 0.0,
            };
            let (high, low, close) = (open + flow.high, open + flow.low, open + flow.delta);
            if ![high, low, close].iter().all(|value| value.is_finite()) {
                continue;
            }
            // A bar delta of zero sells; an unchanged cumulative total keeps buying.
            let down = if anchor.is_some() {
                close < open
            } else {
                close <= open
            };
            self.values.set(
                i,
                IndicatorValue::Multiple(if down {
                    vec![close, f64::NAN, close]
                } else {
                    vec![close, close, f64::NAN]
                }),
            );
            self.candles.push(StudyCandle {
                bars: i..i + 1,
                bar: Bar::new(bar.time, open, high, low, close, flow.up - flow.down),
                plot: 1 + usize::from(down),
            });
            last = Some((key, close));
        }
    }
}

macro_rules! delta_study {
    ($(#[$meta:meta])* $ty:ident, $name:literal, $desc:literal, $anchor:expr) => {
        $(#[$meta])*
        #[derive(Clone, Debug)]
        pub struct $ty {
            core: DeltaCandles,
        }

        impl $ty {
            /// One-minute candles on a one-minute chart until the chart supplies its interval.
            /// Hosts answer [`Indicator::data_requests`] through the chart or `update_data`.
            pub fn new() -> Self {
                Self {
                    core: DeltaCandles::new($anchor),
                }
            }
        }

        impl Default for $ty {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Indicator for $ty {
            fn name(&self) -> &str {
                $name
            }
            fn desc(&self) -> &str {
                $desc
            }
            fn inputs(&self) -> Vec<StudyInput> {
                self.core.inputs()
            }
            fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
                self.core.set_input(id, value)?;
                self.core.notice =
                    lower_interval::notice($name, &self.core.clock, self.core.lower_interval);
                Ok(())
            }
            fn data_requests(&self) -> Vec<StudyDataRequest> {
                self.core
                    .lower()
                    .map(lower_interval::request)
                    .into_iter()
                    .collect()
            }
            fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
                self.core.update_data(data, from, sources);
            }
            fn set_timeframe(&mut self, timeframe: Timeframe) {
                self.set_bar_clock(&self.core.clock.for_timeframe(timeframe));
            }
            fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
                let changed = self.core.clock != *clock;
                self.core.clock = clock.clone();
                self.core.notice =
                    lower_interval::notice($name, &self.core.clock, self.core.lower_interval);
                changed
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.core.refresh(data, 0, &[]);
            }
            fn is_incremental(&self) -> bool {
                true
            }
            fn is_overlay(&self) -> bool {
                false
            }
            fn pane(&self) -> crate::studies::StudyPane {
                crate::studies::StudyPane {
                    levels: vec![(0.0, palette::GRAY, "Zero")],
                    ..Default::default()
                }
            }
            fn value_format(&self) -> StudyFormat {
                StudyFormat::Volume
            }
            fn plot_style(&self, line: usize) -> PlotStyle {
                PlotStyle {
                    color: self.core.colors.get(line).copied().unwrap_or(Color32::WHITE),
                    width: self.line_style(line).width,
                    pattern: LineStyle::Solid,
                    plot_type: PlotType::Line,
                    visible: line != 0,
                }
            }
            fn values(&self) -> &IndicatorValues {
                &self.core.values
            }
            fn candles(&self) -> &[StudyCandle] {
                &self.core.candles
            }
            fn table(&self) -> Option<&StudyTable> {
                self.core.notice.as_ref()
            }
            fn colors(&self) -> Vec<Color32> {
                self.core.colors.to_vec()
            }
            fn set_colors(&mut self, colors: Vec<Color32>) {
                for (old, new) in self.core.colors.iter_mut().zip(colors) {
                    *old = new;
                }
            }
            fn is_visible(&self) -> bool {
                self.core.visible
            }
            fn set_visible(&mut self, visible: bool) {
                self.core.visible = visible;
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
            fn line_names(&self) -> Vec<String> {
                ["Delta", "Up", "Down"].map(String::from).to_vec()
            }
        }
    };
}

delta_study!(
    /// Each bar's volume delta from lower-interval candles, as a candle from zero, in a separate
    /// pane.
    VolumeDelta,
    "Volume Delta",
    "Volume Delta - Up minus down volume of the lower-interval candles in each bar, as a candle from zero",
    None
);
delta_study!(
    /// Volume delta from lower-interval candles added up from the start of each anchor period,
    /// as candles, in a separate pane.
    CumulativeVolumeDelta,
    "Cumulative Volume Delta",
    "Cumulative Volume Delta - Volume delta of lower-interval candles added up over each anchor period",
    Some(AnchorPeriod::Session)
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::IndicatorFactory;
    use chrono::{TimeZone, Utc};

    /// Volume Delta draws each bar from zero along the running delta of its candles, as buying
    /// only when the delta is above zero; Cumulative Volume Delta carries the total across bars
    /// and gaps, draws an unchanged total as buying, restarts at each anchor period and resumes
    /// it when the last bars are recomputed.
    #[test]
    fn delta_candles_follow_the_running_delta_of_lower_interval_candles() {
        let factory = IndicatorFactory::new();
        for name in ["Volume Delta", "Cumulative Volume Delta"] {
            assert_eq!(factory.category(name), Some("Volume"), "{name}");
        }
        let bar = |s: i64, o: f64, c: f64, v: f64| {
            Bar::new(
                Utc.timestamp_opt(s, 0).unwrap(),
                o,
                o.max(c),
                o.min(c),
                c,
                v,
            )
        };
        let update = |study: &mut dyn Indicator, data: &[Bar], from: usize, candles: &[Bar]| {
            study.update_data(
                data,
                from,
                &[StudyData {
                    request: StudyDataRequest {
                        metric: None,
                        symbol: String::new(),
                        timeframe: Some(Timeframe::Min1),
                    },
                    bars: candles,
                    clock: BarClock::new(Timeframe::Min1),
                    instrument: None,
                    timeframe: Timeframe::Min1,
                    chart: None,
                }],
            );
        };
        let drawn = |study: &dyn Indicator| -> Vec<(usize, usize, [f64; 4])> {
            study
                .candles()
                .iter()
                .map(|c| {
                    (
                        c.bars.start,
                        c.plot,
                        [c.bar.open, c.bar.high, c.bar.low, c.bar.close],
                    )
                })
                .collect()
        };

        // Five-minute bars; the third has no candles.
        let chart = [0, 300, 600, 900, 1200, 1500].map(|s| bar(s, 1.0, 1.0, 999.0));
        let candles = [
            bar(0, 10.0, 11.0, 5.0),   // +5
            bar(60, 11.0, 10.0, 8.0),  // -8, running -3
            bar(120, 10.0, 10.0, 5.0), // doji at the previous close takes the falling side: -5, running -8
            bar(300, 10.0, 9.0, 4.0),  // -4
            bar(360, 9.0, 9.0, 1.0), // doji at the previous close takes the falling side: -1, running -5
            bar(900, 9.0, 8.0, 0.0), // no volume
            bar(1200, 9.0, 10.0, 5.0), // +5
            bar(1260, 10.0, 9.0, 5.0), // -5: a bar delta of zero
            bar(1500, 9.0, 10.0, 5.0), // +5
        ];
        let mut delta = VolumeDelta::new();
        delta.set_bar_clock(&BarClock::new(Timeframe::Min5));
        assert_eq!(delta.data_requests()[0].timeframe, Some(Timeframe::Min1));
        update(&mut delta, &chart, 0, &candles);
        // A delta of zero, without volume or from +5 and -5, is drawn as selling.
        assert_eq!(
            drawn(&delta),
            [
                (0, 2, [0.0, 5.0, -8.0, -8.0]),
                (1, 2, [0.0, 0.0, -5.0, -5.0]),
                (3, 2, [0.0; 4]),
                (4, 2, [0.0, 5.0, 0.0, 0.0]),
                (5, 1, [0.0, 5.0, 0.0, 5.0]),
            ]
        );
        // The Delta output has every close; Up and Down only those of their candles.
        let values = delta.values();
        assert_eq!(
            (0..6).map(|i| values.value(0, i)).collect::<Vec<_>>(),
            [
                Some(-8.0),
                Some(-5.0),
                None,
                Some(0.0),
                Some(0.0),
                Some(5.0)
            ]
        );
        assert_eq!(
            [4, 5].map(|i| [values.value(1, i), values.value(2, i)]),
            [[None, Some(0.0)], [Some(5.0), None]]
        );
        // The cumulative total keeps buying through the bars that leave it unchanged.
        let mut cumulative = CumulativeVolumeDelta::new();
        cumulative.set_bar_clock(&BarClock::new(Timeframe::Min5));
        update(&mut cumulative, &chart, 0, &candles);
        assert_eq!(
            drawn(&cumulative),
            [
                (0, 2, [0.0, 5.0, -8.0, -8.0]),
                (1, 2, [-8.0, -8.0, -13.0, -13.0]),
                (3, 1, [-13.0; 4]),
                (4, 1, [-13.0, -8.0, -13.0, -13.0]),
                (5, 1, [-13.0, -8.0, -13.0, -8.0]),
            ]
        );

        // Hourly bars from 22:00 on 1 January 1970 to 03:00 on the 2nd; 02:00 has no candles.
        let chart =
            [79_200, 82_800, 86_400, 90_000, 93_600, 97_200].map(|s| bar(s, 1.0, 1.0, 999.0));
        let mut candles = vec![
            bar(79_200, 1.0, 2.0, 10.0), // 22:00 +10
            bar(79_260, 2.0, 1.0, 4.0),  // -4: bar delta 6, high 10
            bar(82_800, 1.0, 1.0, 3.0), // 23:00 doji at the previous close takes the falling side: -3
            bar(82_860, 1.0, 0.0, 5.0), // -5: bar delta -8, high 0, low -8
            bar(86_400, 0.0, 2.0, 7.0), // 00:00 +7
            bar(90_000, 2.0, 1.0, 2.0), // 01:00 -2
            bar(97_200, 1.0, 2.0, 1.0), // 03:00 +1
        ];
        let mut cumulative = CumulativeVolumeDelta::new();
        cumulative.set_bar_clock(&BarClock::new(Timeframe::Hour1));
        update(&mut cumulative, &chart, 0, &candles);
        // The session total restarts at midnight and carries over the empty 02:00 bar.
        assert_eq!(
            drawn(&cumulative),
            [
                (0, 1, [0.0, 10.0, 0.0, 6.0]),
                (1, 2, [6.0, 6.0, -2.0, -2.0]),
                (2, 1, [0.0, 7.0, 0.0, 7.0]),
                (3, 2, [7.0, 7.0, 5.0, 5.0]),
                (5, 1, [5.0, 6.0, 5.0, 6.0]),
            ]
        );
        // A live candle at 01:01 (+4) is recomputed from bar 3 on the total before it.
        candles.insert(6, bar(90_060, 1.0, 3.0, 4.0));
        update(&mut cumulative, &chart, 3, &candles);
        assert_eq!(
            drawn(&cumulative)[2..],
            [
                (2, 1, [0.0, 7.0, 0.0, 7.0]),
                (3, 1, [7.0, 9.0, 5.0, 9.0]),
                (5, 1, [9.0, 10.0, 9.0, 10.0]),
            ]
        );
        // 1 and 2 January 1970 are in one ISO week, so a weekly total does not restart.
        cumulative
            .set_input("anchor", InputValue::Choice(1))
            .unwrap();
        update(&mut cumulative, &chart, 0, &candles);
        assert_eq!(
            drawn(&cumulative)[2..],
            [
                (2, 1, [-2.0, 5.0, -2.0, 5.0]),
                (3, 1, [5.0, 7.0, 3.0, 7.0]),
                (5, 1, [7.0, 8.0, 7.0, 8.0]),
            ]
        );
        assert_eq!(cumulative.values().value(0, 5), Some(8.0));
    }
}
