//! Lower-interval candles that the volume studies request from the host: the interval they read,
//! the reply they accept, the chart bar each candle belongs to and the volume delta of a bar.

use crate::model::{Bar, BarClock, Timeframe};
use crate::studies::{
    InputKind, InputValue, StudyCell, StudyData, StudyDataRequest, StudyInput, StudyTable,
};
use std::ops::Range;

/// The interval of the candles read below a chart of `clock`'s interval, by its length: one
/// second below a minute, one minute below a day and one hour on longer charts, or one second
/// when the chart interval is not a whole number of those. `None` on tick, volume, range, delta
/// and subsecond charts.
pub(super) fn below(clock: &BarClock) -> Option<Timeframe> {
    let tf = clock.timeframe()?;
    if matches!(tf, Timeframe::Ticks(_)) || tf.duration_ms() < Timeframe::Sec1.duration_ms() {
        return None;
    }
    let lower = if tf.duration_ms() < Timeframe::Min1.duration_ms() {
        Timeframe::Sec1
    } else if tf.duration_ms() < Timeframe::Day1.duration_ms() {
        Timeframe::Min1
    } else {
        Timeframe::Hour1
    };
    Some(if tf.duration_ms() % lower.duration_ms() == 0 {
        lower
    } else {
        Timeframe::Sec1
    })
}

/// The interval the Lower interval input reads on a chart of `clock`'s interval: `chosen` when it
/// is a time interval no longer than the chart's, also one that does not divide it, and `None`
/// when it is longer or trade-based. Unset, five minutes on charts of whole days, such as 1D and
/// 2D, and [`below`] on the others. `None` on tick, volume, range, delta and subsecond charts,
/// whatever is chosen.
pub(super) fn lower_timeframe(clock: &BarClock, chosen: Option<Timeframe>) -> Option<Timeframe> {
    let automatic = below(clock)?;
    let chart = clock.timeframe()?;
    match chosen {
        Some(chosen) => (!chosen.is_trade_based() && chosen.duration_ms() <= chart.duration_ms())
            .then_some(chosen),
        None if matches!(chart, Timeframe::Day1)
            || matches!(chart, Timeframe::Custom(seconds) if seconds.get() % 86_400 == 0) =>
        {
            Some(Timeframe::Min5)
        }
        None => Some(automatic),
    }
}

/// The table `study` shows while the interval chosen in its Lower interval input is not read,
/// being longer than the chart's or trade-based, on a chart that reads lower-interval candles.
pub(super) fn notice(
    study: &str,
    clock: &BarClock,
    chosen: Option<Timeframe>,
) -> Option<StudyTable> {
    let chosen = chosen?;
    let chart = clock.timeframe()?;
    (below(clock).is_some() && lower_timeframe(clock, Some(chosen)).is_none()).then(|| StudyTable {
        title: study.to_owned(),
        columns: vec!["Lower interval".to_owned()],
        rows: vec![vec![StudyCell::Text(format!(
            "{chosen} is not used: choose {chart} or shorter"
        ))]],
        ..StudyTable::default()
    })
}

/// The history request for candles of `timeframe` on the chart symbol.
pub(super) fn request(timeframe: Timeframe) -> StudyDataRequest {
    StudyDataRequest {
        metric: None,
        symbol: String::new(),
        timeframe: Some(timeframe),
    }
}

/// The Lower interval input and its Use custom timeframe switch. Off, the study reads the
/// automatic interval and keeps the chosen one for when the switch is turned on again.
#[derive(Clone, Debug, Default)]
pub(super) struct Choice {
    /// The interval chosen in the Lower interval input; `None` is Auto.
    pub(super) chosen: Option<Timeframe>,
    /// The Use custom timeframe switch.
    pub(super) custom: bool,
}

impl Choice {
    pub(super) fn in_use(&self) -> Option<Timeframe> {
        self.chosen.filter(|_| self.custom)
    }

    pub(super) fn inputs(&self, clock: &BarClock) -> [StudyInput; 2] {
        // Restore the interval first, so the saved switch wins over its implicit enabling.
        [
            input(clock, self.chosen).shown_when("custom_interval", [1]),
            StudyInput::new(
                "custom_interval",
                "Use custom timeframe",
                InputValue::Toggle(self.custom),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(false)),
        ]
    }

    /// Takes a lower-interval setting; setting an interval also enables it for older callers.
    pub(super) fn set(&mut self, id: &str, value: &InputValue) -> bool {
        match (id, value) {
            ("lower_interval", InputValue::Timeframe(chosen)) => {
                self.chosen = *chosen;
                if chosen.is_some() {
                    self.custom = true;
                }
            }
            ("custom_interval", InputValue::Toggle(custom)) => self.custom = *custom,
            _ => return false,
        }
        true
    }
}

/// The Lower interval input. Unset, it reads the interval [`lower_timeframe`] picks on a chart of
/// `clock`'s interval, which its kind names for the settings to show.
fn input(clock: &BarClock, chosen: Option<Timeframe>) -> StudyInput {
    StudyInput::new(
        "lower_interval",
        "Lower interval",
        InputValue::Timeframe(chosen),
        InputKind::AutoTimeframe(lower_timeframe(clock, None)),
    )
    .with_default(InputValue::Timeframe(None))
}

/// The candles answering `request` among `sources`: empty when none does, and `None`, keeping
/// the study's values, when the answer resolved to another interval or, from bar 0, is out of
/// order or holds a non-finite open, close or volume or a negative volume.
pub(super) fn reply<'a>(
    request: &StudyDataRequest,
    from: usize,
    sources: &[StudyData<'a>],
) -> Option<&'a [Bar]> {
    let Some(source) = sources.iter().find(|source| source.request == *request) else {
        return Some(&[]);
    };
    if Some(source.timeframe) != request.timeframe
        || (from == 0
            && (source
                .bars
                .windows(2)
                .any(|pair| pair[0].time >= pair[1].time)
                || source.bars.iter().any(|bar| {
                    !bar.open.is_finite()
                        || !bar.close.is_finite()
                        || !bar.volume.is_finite()
                        || bar.volume < 0.0
                })))
    {
        return None;
    }
    Some(source.bars)
}

/// The candles inside each chart bar of `data[from..]`, as index ranges into `candles`, which are
/// sorted by time. A candle belongs to the bar holding its open; on daily and longer charts, to
/// the bar holding its trading date.
pub(super) fn nest(
    clock: &BarClock,
    lower: Timeframe,
    data: &[Bar],
    candles: &[Bar],
    from: usize,
) -> Vec<Range<usize>> {
    let from = from.min(data.len());
    let Some(tf) = clock.timeframe().filter(|tf| tf.duration_ms() > 0) else {
        return vec![0..0; data.len() - from];
    };
    let daily = tf.duration_ms() % Timeframe::Day1.duration_ms() == 0;
    // A daily or longer candle is dated by its label, not by the session holding its instant.
    let sessions = clock.for_timeframe(lower).sessions();
    let key = |bar: &Bar| {
        if daily {
            BarClock::new(tf).bar_open(
                sessions
                    .trading_date(bar.time)
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
            )
        } else {
            clock.bar_open(bar.time)
        }
    };
    let parent = |bar: &Bar| {
        if daily {
            BarClock::new(tf).bar_open(bar.time)
        } else {
            clock.bar_open(bar.time)
        }
    };
    let mut at = data.get(from).map_or(0, |bar| {
        candles.partition_point(|candle| key(candle) < parent(bar))
    });
    data[from..]
        .iter()
        .map(|bar| {
            let parent = parent(bar);
            while candles.get(at).is_some_and(|candle| key(candle) < parent) {
                at += 1;
            }
            let start = at;
            while candles.get(at).is_some_and(|candle| key(candle) == parent) {
                at += 1;
            }
            start..at
        })
        .collect()
}

/// The volume of some candles split by direction, and the running delta they reach, which
/// starts at zero.
#[derive(Clone, Copy, Default)]
pub(super) struct Flow {
    /// Volume of the rising candles, zero or more.
    pub(super) up: f64,
    /// Volume of the falling candles, negated: zero or less.
    pub(super) down: f64,
    /// Lowest running delta, zero or less.
    pub(super) low: f64,
    /// Highest running delta, zero or more.
    pub(super) high: f64,
    /// Running delta after the last candle.
    pub(super) delta: f64,
}

/// The sides of lower-interval candles, read in time order. A candle buys when it closes above
/// its open, or at its open above the previous candle's close, and sells when it closes below
/// either; one at its open and at the previous close takes the side of the candle before it.
/// The first candle, with none before it, buys when it closes at its open.
pub(super) struct Sides<'a> {
    candles: &'a [Bar],
    /// The candles before this index have been read.
    read: usize,
    /// Whether the candle before `read` buys; true before the first candle.
    buying: bool,
}

impl<'a> Sides<'a> {
    /// Reads `candles`, which are sorted by time, from the first.
    pub(super) fn new(candles: &'a [Bar]) -> Self {
        Self {
            candles,
            read: 0,
            buying: true,
        }
    }

    /// The flow of `candles[range]`, for ranges taken in time order. The candles skipped since
    /// the previous range give the side of the candle before this one, back to the last that
    /// closed away from its open or the previous close.
    pub(super) fn flow(&mut self, range: Range<usize>) -> Flow {
        debug_assert!(
            range.start >= self.read,
            "ranges must be taken in time order"
        );
        let mut buying = (self.read..range.start)
            .rev()
            .find_map(|at| side(self.candles, at))
            .unwrap_or(self.buying);
        let mut flow = Flow::default();
        for at in range.clone() {
            buying = side(self.candles, at).unwrap_or(buying);
            let volume = self.candles[at].volume;
            if buying {
                flow.up += volume;
                flow.delta += volume;
            } else {
                flow.down -= volume;
                flow.delta -= volume;
            }
            flow.high = flow.high.max(flow.delta);
            flow.low = flow.low.min(flow.delta);
        }
        self.read = range.end;
        self.buying = buying;
        flow
    }
}

/// The side `candles[at]` takes by itself: buying when it closes above its open or, at its open,
/// above the previous candle's close, selling when below; `None` when neither decides, as for a
/// first candle at its open or a NaN close.
fn side(candles: &[Bar], at: usize) -> Option<bool> {
    let candle = &candles[at];
    if candle.close > candle.open {
        return Some(true);
    }
    if candle.close < candle.open {
        return Some(false);
    }
    let before = candles[..at].last()?.close;
    if candle.close > before {
        Some(true)
    } else if candle.close < before {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, BarClock, Timeframe};
    use crate::studies::{IndicatorFactory, StudyData};
    use chrono::{TimeZone, Utc};

    /// Chart bar times, candles as (time, open, close, volume) in seconds, and per chart bar the
    /// Volume Delta and Cumulative Volume Delta candles (open, high, low, close) and the Up/Down
    /// Volume values (up, down, delta).
    type Row = (
        &'static str,
        &'static [i64],
        &'static [(i64, f64, f64, f64)],
        &'static [[f64; 4]],
        &'static [[f64; 4]],
        &'static [[f64; 3]],
    );

    /// Disabling a chosen interval keeps it for the next enable and through template restore.
    #[test]
    fn custom_timeframe_switch_keeps_the_chosen_interval() {
        use crate::studies::{InputValue, StudyTemplate};
        let factory = IndicatorFactory::new();
        for name in ["Up/Down Volume", "Volume Delta", "Cumulative Volume Delta"] {
            let mut study = factory.create(name).unwrap();
            let clock = BarClock::new(Timeframe::Min5);
            study.set_bar_clock(&clock);
            let requested = |study: &dyn crate::studies::Indicator| {
                study.data_requests().first().and_then(|r| r.timeframe)
            };
            let value = |study: &dyn crate::studies::Indicator, id: &str| {
                study
                    .inputs()
                    .into_iter()
                    .find(|i| i.id == id)
                    .unwrap()
                    .value
            };
            study
                .set_input(
                    "lower_interval",
                    InputValue::Timeframe(Some(Timeframe::Sec30)),
                )
                .unwrap();
            assert_eq!(requested(study.as_ref()), Some(Timeframe::Sec30), "{name}");
            assert_eq!(
                value(study.as_ref(), "custom_interval"),
                InputValue::Toggle(true)
            );
            study
                .set_input("custom_interval", InputValue::Toggle(false))
                .unwrap();
            assert_eq!(requested(study.as_ref()), Some(Timeframe::Min1));
            assert_eq!(
                value(study.as_ref(), "lower_interval"),
                InputValue::Timeframe(Some(Timeframe::Sec30))
            );
            study
                .set_input("custom_interval", InputValue::Toggle(true))
                .unwrap();
            assert_eq!(requested(study.as_ref()), Some(Timeframe::Sec30));
            study
                .set_input(
                    "lower_interval",
                    InputValue::Timeframe(Some(Timeframe::Hour1)),
                )
                .unwrap();
            assert!(study.table().is_some());
            study
                .set_input("custom_interval", InputValue::Toggle(false))
                .unwrap();
            assert!(study.table().is_none());
            assert_eq!(requested(study.as_ref()), Some(Timeframe::Min1));
            let mut restored = StudyTemplate::from_study(study.as_ref())
                .create_with(&factory)
                .unwrap();
            restored.set_bar_clock(&clock);
            assert_eq!(requested(restored.as_ref()), Some(Timeframe::Min1));
            assert_eq!(
                value(restored.as_ref(), "custom_interval"),
                InputValue::Toggle(false)
            );
            assert_eq!(
                value(restored.as_ref(), "lower_interval"),
                InputValue::Timeframe(Some(Timeframe::Hour1))
            );
        }
    }

    /// Literal templates from before the new inputs keep their intervals, anchors and styles.
    #[test]
    fn studies_saved_before_the_switch_restore_as_drawn() {
        use crate::studies::{Indicator, InputValue, StudyTemplate, UpDownVolume};
        assert_eq!(UpDownVolume::new().plot_style(2).plot_type.name(), "Dashes");
        let factory = IndicatorFactory::new();
        for (saved, lower) in [
            (
                r#"{"name":"Up/Down Volume","inputs":[["lower_interval",{"Timeframe":"Min1"}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.UpDownVolume.html","plots":[{"color":[30,70,32,102],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[97,22,28,102],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[2,4,2,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
                Timeframe::Min1,
            ),
            (
                r#"{"name":"Cumulative Volume Delta","inputs":[["lower_interval",{"Timeframe":"Sec30"}],["anchor",{"Choice":1}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CumulativeVolumeDelta.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[8,153,129,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
                Timeframe::Sec30,
            ),
        ] {
            let template: StudyTemplate = serde_json::from_str(saved).unwrap();
            let mut restored = template.create_with(&factory).unwrap();
            restored.set_bar_clock(&BarClock::new(Timeframe::Min5));
            assert_eq!(restored.settings(), template.settings);
            assert_eq!(restored.data_requests()[0].timeframe, Some(lower));
            let inputs = restored.inputs();
            let value = |id: &str| &inputs.iter().find(|i| i.id == id).unwrap().value;
            assert_eq!(value("lower_interval"), &InputValue::Timeframe(Some(lower)));
            assert_eq!(value("custom_interval"), &InputValue::Toggle(true));
            if template.name == "Up/Down Volume" {
                assert_eq!(restored.plot_style(2).plot_type.name(), "Crosses");
            } else {
                assert_eq!(value("anchor"), &InputValue::Choice(1));
                assert_eq!(value("anchor_minutes"), &InputValue::Length(60));
                assert_eq!(value("aggregation"), &InputValue::Choice(0));
                assert!(restored.order_flow_inputs().is_empty());
            }
        }
    }

    /// The three volume studies saved with the Lower interval unset, read or not read, on time,
    /// tick and subsecond charts, restore from their templates with the inputs they were saved
    /// with, before and after the chart gives them its interval.
    #[test]
    fn saved_lower_intervals_restore_unchanged() {
        use crate::studies::{ConfiguredStudy, Indicator, InputValue, StudyTemplate};
        use std::num::NonZeroU64;
        let factory = IndicatorFactory::new();
        let custom = |seconds| Timeframe::Custom(NonZeroU64::new(seconds).unwrap());
        for (name, chart, chosen) in [
            ("Volume Delta", Timeframe::Min5, None),
            ("Up/Down Volume", Timeframe::Min5, Some(custom(180))),
            ("Up/Down Volume", Timeframe::Min5, Some(Timeframe::Hour1)),
            (
                "Cumulative Volume Delta",
                Timeframe::Sec30,
                Some(Timeframe::Min1),
            ),
            (
                "Volume Delta",
                Timeframe::Ticks(NonZeroU64::new(100).unwrap()),
                None,
            ),
            (
                "Cumulative Volume Delta",
                Timeframe::Ms500,
                Some(Timeframe::Sec1),
            ),
        ] {
            let mut study = ConfiguredStudy::new(factory.create(name).unwrap());
            study.set_bar_clock(&BarClock::new(chart));
            study
                .set_input("lower_interval", InputValue::Timeframe(chosen))
                .unwrap();
            let saved = StudyTemplate::from_study(&study);
            let mut restored = saved
                .create_with(&factory)
                .unwrap_or_else(|error| panic!("{name} on {chart}: {error}"));
            assert_eq!(
                StudyTemplate::from_study(restored.as_ref()).inputs,
                saved.inputs,
                "{name} on {chart}, restored"
            );
            restored.set_bar_clock(&BarClock::new(chart));
            assert_eq!(
                StudyTemplate::from_study(restored.as_ref()).inputs,
                saved.inputs,
                "{name} on {chart}, on the chart"
            );
        }
    }

    /// A lower-interval candle that closes at its open and at the previous close takes the side
    /// of the candle before it, inside a chart bar, across chart bars and from before the first
    /// one, alike in the three studies that split volume by side and when only the last chart bar
    /// is recomputed.
    #[test]
    fn flat_candles_take_the_side_of_the_candle_before() {
        const ROWS: [Row; 6] = [
            (
                "falling, then two flat candles",
                &[0],
                &[
                    (0, 10.0, 9.0, 5.0),
                    (60, 9.0, 9.0, 3.0),
                    (120, 9.0, 9.0, 2.0),
                ],
                &[[0.0, 0.0, -10.0, -10.0]],
                &[[0.0, 0.0, -10.0, -10.0]],
                &[[0.0, -10.0, -10.0]],
            ),
            (
                "rising, then a flat candle",
                &[0],
                &[(0, 9.0, 10.0, 5.0), (60, 10.0, 10.0, 3.0)],
                &[[0.0, 8.0, 0.0, 8.0]],
                &[[0.0, 8.0, 0.0, 8.0]],
                &[[8.0, 0.0, 8.0]],
            ),
            (
                "a flat candle opens the bar after falling and flat ones",
                &[0, 300],
                &[
                    (0, 10.0, 9.0, 5.0),
                    (60, 9.0, 9.0, 2.0),
                    (300, 9.0, 9.0, 3.0),
                ],
                &[[0.0, 0.0, -7.0, -7.0], [0.0, 0.0, -3.0, -3.0]],
                &[[0.0, 0.0, -7.0, -7.0], [-7.0, -7.0, -10.0, -10.0]],
                &[[0.0, -7.0, -7.0], [0.0, -3.0, -3.0]],
            ),
            (
                "a flat first candle buys",
                &[0],
                &[(0, 9.0, 9.0, 4.0)],
                &[[0.0, 4.0, 0.0, 4.0]],
                &[[0.0, 4.0, 0.0, 4.0]],
                &[[4.0, 0.0, 4.0]],
            ),
            (
                "a falling candle before the first chart bar",
                &[0],
                &[(-60, 10.0, 9.0, 1.0), (0, 9.0, 9.0, 4.0)],
                &[[0.0, 0.0, -4.0, -4.0]],
                &[[0.0, 0.0, -4.0, -4.0]],
                &[[0.0, -4.0, -4.0]],
            ),
            (
                "flat candles away from the previous close",
                &[0],
                &[
                    (0, 10.0, 9.0, 5.0),
                    (60, 10.0, 10.0, 3.0),
                    (120, 11.0, 12.0, 1.0),
                    (180, 11.0, 11.0, 2.0),
                ],
                &[[0.0, 0.0, -5.0, -3.0]],
                &[[0.0, 0.0, -5.0, -3.0]],
                &[[4.0, -7.0, -3.0]],
            ),
        ];
        let candle = |s: i64, o: f64, c: f64, v: f64| {
            Bar::new(
                Utc.timestamp_opt(s, 0).unwrap(),
                o,
                o.max(c),
                o.min(c),
                c,
                v,
            )
        };
        let factory = IndicatorFactory::new();
        for (row, times, candles, delta, cumulative, up_down) in ROWS {
            let chart: Vec<_> = times.iter().map(|&s| candle(s, 1.0, 1.0, 999.0)).collect();
            let lower: Vec<_> = candles
                .iter()
                .map(|&(s, o, c, v)| candle(s, o, c, v))
                .collect();
            for (name, want) in [
                (
                    "Volume Delta",
                    delta.iter().map(|r| r.to_vec()).collect::<Vec<_>>(),
                ),
                (
                    "Cumulative Volume Delta",
                    cumulative.iter().map(|r| r.to_vec()).collect(),
                ),
                (
                    "Up/Down Volume",
                    up_down.iter().map(|r| r.to_vec()).collect(),
                ),
            ] {
                let mut study = factory.create(name).unwrap();
                study.set_bar_clock(&BarClock::new(Timeframe::Min5));
                // The whole history, then the last chart bar alone on the values before it.
                for from in [0, chart.len() - 1] {
                    study.update_data(
                        &chart,
                        from,
                        &[StudyData {
                            request: super::request(Timeframe::Min1),
                            bars: &lower,
                            clock: BarClock::new(Timeframe::Min1),
                            instrument: None,
                            timeframe: Timeframe::Min1,
                            chart: None,
                        }],
                    );
                    let got: Vec<Vec<f64>> = if name == "Up/Down Volume" {
                        (0..chart.len())
                            .map(|i| {
                                (0..3)
                                    .map(|line| study.values().value(line, i).unwrap_or(f64::NAN))
                                    .collect()
                            })
                            .collect()
                    } else {
                        study
                            .candles()
                            .iter()
                            .map(|c| vec![c.bar.open, c.bar.high, c.bar.low, c.bar.close])
                            .collect()
                    };
                    assert_eq!(got, want, "{row}: {name} from bar {from}");
                }
            }
        }
    }
}
