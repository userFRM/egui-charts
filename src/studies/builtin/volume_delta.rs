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
//! Cumulative Volume Delta also offers Ticks aggregation: supplied executions count by their
//! aggressor side or, when missing, by the last price change. Until a price changes they count
//! for neither side. This mode requests no lower-interval candles and uses the chart's trade
//! history, including replay and retention. Both panes draw a dashed grey zero level.
//!
//! [`Indicator::candles`] holds one candle per bar with a value, on plot 1 ("Up") when it closes
//! above its open and on plot 2 ("Down") otherwise, so a Volume Delta bar whose delta is zero is
//! drawn as selling; a Cumulative Volume Delta bar whose total ends where it opened is drawn as
//! buying. Output 0 ("Delta") holds every close,
//! for studies that take this one as their source, and is not drawn; outputs 1 and 2 hold the
//! close in the column of its candle's plot.

use super::anchor_period::AnchorPeriod;
use super::lower_interval;
use super::order_flow::{Reset, reset_key};
use crate::data::{OrderFlowInput, OrderFlowKind};
use crate::model::{Bar, BarClock, LineStyle, Timeframe};
use crate::scales::StudyFormat;
use crate::studies::palette;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, PlotStyle, PlotType,
    StudyCandle, StudyData, StudyDataRequest, StudyInput, StudyTable,
};
use egui::Color32;

/// The Anchor period choices in their saved order: new choices go at the end.
const ANCHORS: [Reset; 8] = [
    Reset::Period(AnchorPeriod::Session),
    Reset::Period(AnchorPeriod::Week),
    Reset::Period(AnchorPeriod::Month),
    Reset::Period(AnchorPeriod::Quarter),
    Reset::Period(AnchorPeriod::Year),
    Reset::Period(AnchorPeriod::Decade),
    Reset::Period(AnchorPeriod::Century),
    Reset::Interval,
];

#[derive(Clone, Debug)]
struct DeltaCandles {
    clock: BarClock,
    /// The chosen lower interval and whether it is in use.
    lower: lower_interval::Choice,
    /// The period from whose first bar the deltas add up; `None` starts every bar at zero.
    anchor: Option<Reset>,
    /// Length of the Interval anchor in minutes, 1 to 1440.
    anchor_minutes: usize,
    /// Data aggregation Ticks: the deltas come from executions, not candles.
    ticks: bool,
    /// Executions of the chart, sided by the feed or the tick rule; CVD only.
    flow: Option<OrderFlowInput>,
    reset_revision: u64,
    candles: Vec<StudyCandle>,
    values: IndicatorValues,
    colors: [Color32; 3],
    visible: bool,
    /// Shown while the chosen lower interval is not read.
    notice: Option<StudyTable>,
}

impl DeltaCandles {
    fn new(anchor: Option<Reset>) -> Self {
        Self {
            clock: BarClock::new(Timeframe::Min1),
            lower: lower_interval::Choice::default(),
            anchor,
            anchor_minutes: 60,
            ticks: false,
            flow: anchor.map(|_| {
                OrderFlowInput::new(OrderFlowKind::FilteredCvd {
                    min: 0.0,
                    max: f64::MAX,
                })
                .with_tick_rule()
            }),
            reset_revision: 0,
            candles: Vec::new(),
            values: IndicatorValues::new(),
            colors: [palette::BLUE, palette::UP, palette::DOWN],
            visible: true,
            notice: None,
        }
    }

    fn lower(&self) -> Option<Timeframe> {
        lower_interval::lower_timeframe(&self.clock, self.lower.in_use())
    }

    fn inputs(&self) -> Vec<StudyInput> {
        let mut inputs = self.lower.inputs(&self.clock).to_vec();
        if let Some(anchor) = self.anchor {
            inputs.push(
                StudyInput::new(
                    "anchor",
                    "Anchor period",
                    InputValue::Choice(ANCHORS.iter().position(|p| *p == anchor).unwrap_or(0)),
                    InputKind::Choice(
                        ANCHORS
                            .iter()
                            .map(|a| match a {
                                Reset::Period(period) => period.label(),
                                Reset::Interval => "Interval",
                                Reset::Continuous => unreachable!("anchors always restart"),
                            })
                            .map(str::to_owned)
                            .collect(),
                    ),
                )
                .with_default(InputValue::Choice(0)),
            );
            inputs.push(
                StudyInput::new(
                    "anchor_minutes",
                    "Anchor interval (minutes)",
                    InputValue::Length(self.anchor_minutes),
                    InputKind::Length(1, 1440),
                )
                .with_default(InputValue::Length(60))
                .shown_when("anchor", [ANCHORS.len() - 1]),
            );
            inputs.push(
                StudyInput::new(
                    "aggregation",
                    "Data aggregation",
                    InputValue::Choice(usize::from(self.ticks)),
                    InputKind::Choice(vec!["Time".into(), "Ticks".into()]),
                )
                .with_default(InputValue::Choice(0)),
            );
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        match (id, value) {
            (_, ref value) if self.lower.set(id, value) => {}
            ("anchor", InputValue::Choice(at)) if self.anchor.is_some() && at < ANCHORS.len() => {
                self.anchor = Some(ANCHORS[at]);
            }
            ("anchor_minutes", InputValue::Length(n))
                if self.anchor.is_some() && (1..=1440).contains(&n) =>
            {
                self.anchor_minutes = n
            }
            ("aggregation", InputValue::Choice(at @ 0..=1)) if self.anchor.is_some() => {
                self.ticks = at == 1
            }
            _ => return Err(crate::studies::StudyError::rejected(&self.inputs(), id)),
        }
        self.refresh(&[], 0, &[]);
        Ok(())
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        if self.ticks {
            self.refresh_ticks(data, from);
            return;
        }
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
        let anchor = self.anchor;
        let clock = self.clock.clone();
        let key = anchor.map(|a| reset_key(a, self.anchor_minutes, &clock));
        let period = |bar: &Bar| key.as_ref().and_then(|k| k(bar.time));
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
            self.push_flow(i, bar, period(bar), flow, &mut last);
        }
    }

    /// Appends one candle and advances the total only when its values are finite.
    fn push_flow(
        &mut self,
        i: usize,
        bar: &Bar,
        key: Option<i64>,
        flow: lower_interval::Flow,
        last: &mut Option<(Option<i64>, f64)>,
    ) {
        let open = match *last {
            Some((at, total)) if self.anchor.is_some() && at == key => total,
            _ => 0.0,
        };
        let (high, low, close) = (open + flow.high, open + flow.low, open + flow.delta);
        if ![high, low, close].iter().all(|value| value.is_finite()) {
            return;
        }
        // A bar delta of zero sells; an unchanged cumulative total keeps buying.
        let down = if self.anchor.is_some() {
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
        *last = Some((key, close));
    }

    /// Redraws from the execution source, restarting after its history is rebuilt or dropped.
    fn refresh_ticks(&mut self, data: &[Bar], from: usize) {
        let Some(flow) = &self.flow else {
            return;
        };
        let revision = flow.reset_revision();
        let from = if revision != self.reset_revision {
            0
        } else {
            from.min(self.values.len()).min(data.len())
        };
        self.reset_revision = revision;
        let source = data
            .get(from)
            .map(|bar| flow.values_from(self.clock.bar_open(bar.time)));
        self.candles
            .truncate(self.candles.partition_point(|c| c.bars.start < from));
        self.values.truncate(from);
        self.values.resize(data.len(), IndicatorValue::None);
        let clock = self.clock.clone();
        let key = self
            .anchor
            .map(|a| reset_key(a, self.anchor_minutes, &clock));
        let period = |bar: &Bar| key.as_ref().and_then(|k| k(bar.time));
        let mut last = self
            .candles
            .last()
            .map(|c| (period(&data[c.bars.start]), c.bar.close));
        for (i, bar) in data.iter().enumerate().skip(from) {
            if !bar.close.is_finite() {
                continue;
            }
            let Some(row) = source
                .as_ref()
                .and_then(|s| s.get(&clock.bar_open(bar.time)))
            else {
                continue;
            };
            self.push_flow(
                i,
                bar,
                period(bar),
                lower_interval::Flow {
                    delta: row[0],
                    high: row[1],
                    low: row[2],
                    ..Default::default()
                },
                &mut last,
            );
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
                    if self.core.ticks { None } else {
                        lower_interval::notice($name, &self.core.clock, self.core.lower.in_use())
                    };
                Ok(())
            }
            fn needs_order_flow(&self) -> bool {
                self.core.ticks
            }
            fn order_flow_inputs(&self) -> &[OrderFlowInput] {
                if self.core.ticks { self.core.flow.as_slice() } else { &[] }
            }
            fn data_requests(&self) -> Vec<StudyDataRequest> {
                if self.core.ticks { return Vec::new(); }
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
                    if self.core.ticks { None } else {
                        lower_interval::notice($name, &self.core.clock, self.core.lower.in_use())
                    };
                changed
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.update(data, 0);
            }
            fn update(&mut self, data: &[Bar], from: usize) {
                if self.core.ticks {
                    self.core.refresh_ticks(data, from);
                } else {
                    self.core.refresh(data, 0, &[]);
                }
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
            fn clone_for_export(&self) -> Box<dyn Indicator> {
                let mut frozen = self.clone();
                frozen.core.flow = self.core.flow.as_ref().map(OrderFlowInput::frozen);
                Box::new(frozen)
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
    Some(Reset::Period(AnchorPeriod::Session))
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::IndicatorFactory;
    use chrono::{TimeZone, Utc};

    /// Tick rule hand totals: +3,+2,+4,-1 reaches 8 (high 9); -6,-2,+1 reaches
    /// -7 (low -8). A quiet bar is absent, and +4 then -2 ends the last bar at 3.
    #[test]
    fn cumulative_volume_delta_counts_ticks_by_side_or_tick_rule() {
        use crate::model::{
            FeedService,
            OrderSide::{Buy, Sell},
            Tick,
        };
        let clock = BarClock::new(Timeframe::Min1);
        let mut study = CumulativeVolumeDelta::new();
        study.set_bar_clock(&clock);
        study
            .set_input("aggregation", InputValue::Choice(1))
            .unwrap();
        assert!(study.data_requests().is_empty());
        assert!(study.needs_order_flow());
        let input = study.order_flow_inputs()[0].clone();
        assert_eq!(input.services(), &[FeedService::LiveTrades]);
        let tick =
            |s, price, size, side| Tick::new(Utc.timestamp_opt(s, 0).unwrap(), price, size, side);
        let trades = [
            tick(0, 10.0, 5.0, None),
            tick(10, 11.0, 3.0, None),
            tick(20, 11.0, 2.0, None),
            tick(30, 10.5, 4.0, Some(Buy)),
            tick(40, 10.5, 1.0, None),
            tick(60, 10.5, 6.0, Some(Sell)),
            tick(70, 10.5, 2.0, None),
            tick(80, 12.0, 1.0, None),
            tick(180, 12.0, 4.0, None),
        ];
        for trade in &trades {
            input.push_trade(&clock, trade);
        }
        let chart = [0, 60, 120, 180]
            .map(|s| Bar::new(Utc.timestamp_opt(s, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 0.0));
        let drawn = |s: &CumulativeVolumeDelta| {
            s.candles()
                .iter()
                .map(|c| {
                    (
                        c.bars.start,
                        c.plot,
                        [c.bar.open, c.bar.high, c.bar.low, c.bar.close],
                    )
                })
                .collect::<Vec<_>>()
        };
        study.calculate(&chart);
        assert_eq!(
            drawn(&study),
            [
                (0, 1, [0.0, 9.0, 0.0, 8.0]),
                (1, 2, [8.0, 8.0, 0.0, 1.0]),
                (3, 1, [1.0, 5.0, 1.0, 5.0]),
            ]
        );
        assert_eq!(
            (0..4)
                .map(|i| study.values().value(0, i))
                .collect::<Vec<_>>(),
            [Some(8.0), Some(1.0), None, Some(5.0)]
        );
        assert!(study.candles().iter().all(|c| c.bar.volume == 0.0));
        input.push_trade(&clock, &tick(190, 11.5, 2.0, None));
        study.update(&chart, 3);
        assert_eq!(drawn(&study)[2], (3, 1, [1.0, 5.0, 1.0, 3.0]));
        let mut fresh = study.clone();
        fresh.calculate(&chart);
        assert_eq!(drawn(&study), drawn(&fresh));
        // A reset forgets earlier prices as well as rows, and forces a full refresh.
        let mut export = study.clone_for_export();
        input.clear();
        input.push_trade(&clock, &tick(180, 12.0, 4.0, None));
        study.update(&chart, 3);
        assert_eq!(drawn(&study), [(3, 1, [0.0; 4])]);
        export.calculate(&chart);
        assert_eq!(export.values().value(0, 3), Some(3.0));
        #[cfg(feature = "widget")]
        {
            // At 03:05 the unchanged price still takes the uptick from the preceding bar,
            // even though that price change lies outside the cutoff candle.
            let mut held = crate::data::trades::Trades::new(100);
            for trade in trades {
                held.push(trade);
            }
            input.replace_trades(&clock, &held, &chart, None);
            input.set_replay(
                &clock,
                Some(Utc.timestamp_opt(185, 0).unwrap()),
                Some(&held),
                &chart,
            );
            study.calculate(&chart);
            assert_eq!(drawn(&study)[2], (3, 1, [1.0, 5.0, 1.0, 5.0]));
        }
    }

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
        // Two-hour periods from midnight: 22:00, 00:00 and 02:00. A missing 02:00
        // candle does not carry the old total into 03:00.
        cumulative
            .set_input("anchor", InputValue::Choice(7))
            .unwrap();
        cumulative
            .set_input("anchor_minutes", InputValue::Length(120))
            .unwrap();
        update(&mut cumulative, &chart, 0, &candles);
        assert_eq!(
            drawn(&cumulative),
            [
                (0, 1, [0.0, 10.0, 0.0, 6.0]),
                (1, 2, [6.0, 6.0, -2.0, -2.0]),
                (2, 1, [0.0, 7.0, 0.0, 7.0]),
                (3, 1, [7.0, 9.0, 5.0, 9.0]),
                (5, 1, [0.0, 1.0, 0.0, 1.0]),
            ]
        );
        assert_eq!(cumulative.values().value(0, 5), Some(1.0));
        for n in [0, 1441] {
            assert!(
                cumulative
                    .set_input("anchor_minutes", InputValue::Length(n))
                    .is_err()
            );
        }
    }
}
