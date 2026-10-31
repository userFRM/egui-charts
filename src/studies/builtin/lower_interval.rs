//! Lower-interval candles that the volume studies request from the host: the interval they read,
//! the reply they accept, the chart bar each candle belongs to and the volume delta of a bar.

use crate::model::{Bar, BarClock, Timeframe};
use crate::studies::{InputKind, InputValue, StudyData, StudyDataRequest, StudyInput};
use std::ops::Range;

/// The interval read below the chart's: `chosen` when it is shorter than the chart interval and
/// divides it, otherwise one second below a minute, one minute below a day, five minutes on daily
/// and one hour on longer charts, or one second when the chart interval is not a whole number of
/// those. `None` on tick, volume, range, delta and subsecond charts, whatever is chosen.
pub(super) fn lower_timeframe(clock: &BarClock, chosen: Option<Timeframe>) -> Option<Timeframe> {
    let tf = clock.timeframe()?;
    if matches!(tf, Timeframe::Ticks(_)) || tf.duration_ms() < Timeframe::Sec1.duration_ms() {
        return None;
    }
    if let Some(chosen) = chosen.filter(|chosen| {
        chosen.duration_ms() > 0
            && chosen.duration_ms() < tf.duration_ms()
            && tf.duration_ms() % chosen.duration_ms() == 0
    }) {
        return Some(chosen);
    }
    let lower = if tf.duration_ms() < Timeframe::Min1.duration_ms() {
        Timeframe::Sec1
    } else if tf.duration_ms() < Timeframe::Day1.duration_ms() {
        Timeframe::Min1
    } else if tf == Timeframe::Day1 {
        Timeframe::Min5
    } else {
        Timeframe::Hour1
    };
    Some(if tf.duration_ms() % lower.duration_ms() == 0 {
        lower
    } else {
        Timeframe::Sec1
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

/// The Lower interval input. `None`, which the settings show as the chart interval, and any
/// interval that is not shorter than the chart's or does not divide it read the automatic one.
pub(super) fn input(chosen: Option<Timeframe>) -> StudyInput {
    StudyInput::new(
        "lower_interval",
        "Lower interval",
        InputValue::Timeframe(chosen),
        InputKind::Timeframe,
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

/// The flow of `candles[range]`. A candle rises when it closes above its open, or at its open
/// and not below the previous candle's close; the first candle of `candles` has none.
pub(super) fn flow(candles: &[Bar], range: Range<usize>) -> Flow {
    let mut flow = Flow::default();
    for at in range {
        let candle = &candles[at];
        let rising = candle.close > candle.open
            || (candle.close == candle.open
                && at
                    .checked_sub(1)
                    .is_none_or(|before| candle.close >= candles[before].close));
        if rising {
            flow.up += candle.volume;
            flow.delta += candle.volume;
        } else {
            flow.down -= candle.volume;
            flow.delta -= candle.volume;
        }
        flow.high = flow.high.max(flow.delta);
        flow.low = flow.low.min(flow.delta);
    }
    flow
}
