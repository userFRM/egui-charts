//! Bars from history rows, from stream records and from trades.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeDelta, Utc};
use egui_charts::model::{Bar, BarClock, Timeframe, TimezoneId};
use serde_json::Value;

use super::profile::{BarFields, ConditionRule, FieldPath, Segment, TradeFields, at, number, text};
use super::time::{Stamp, stamp, wall};

/// The milliseconds of a day.
pub(crate) const DAY_MS: i64 = 86_400_000;

/// Whether bars of `timeframe` are whole days, weeks or months: a duration of at least one day
/// that is a multiple of a day. Such a bar is dated, not timed (1.2.6).
pub(crate) fn calendar(timeframe: Timeframe) -> bool {
    let duration = timeframe.duration_ms();
    duration >= DAY_MS && duration % DAY_MS == 0
}

/// The first calendar date of the period of a chart's clock that opens at `open`: the UTC date
/// of `open` when its UTC time of day is 10:00 or earlier, otherwise the next UTC date.
// shortcut: exact for chart time zones from UTC-10:00 to UTC+13:45, because `BarClock` does not
// expose its zone. Upgrade when the clock can name the local date of an instant.
pub(crate) fn period_date(open: DateTime<Utc>) -> NaiveDate {
    let date = open.date_naive();
    if open.time() <= NaiveTime::MIN + TimeDelta::hours(10) {
        date
    } else {
        date.succ_opt().unwrap_or(date)
    }
}

/// The open of the period of `clock` that holds `date`: the period `clock` opens for 10:00 UTC
/// on the date, with the limit stated on [`period_date`].
pub(crate) fn period_open(clock: &BarClock, date: NaiveDate) -> DateTime<Utc> {
    let ten = date.and_time(NaiveTime::MIN + TimeDelta::hours(10));
    clock.bar_open(ten.and_utc())
}

/// The time a bar of a calendar interval carries for the trading date `date`: the later of
/// 00:00 UTC on the date and the open of the period of `clock` that holds the date, which is
/// what `HistoryPage::bars` asks of a feed. The period is the one `clock` opens for 10:00 UTC
/// on the date, with the limit stated on [`period_date`].
pub(crate) fn dated(clock: &BarClock, date: NaiveDate) -> DateTime<Utc> {
    let midnight = date.and_time(NaiveTime::MIN).and_utc();
    period_open(clock, date).max(midnight)
}

/// Whether the slot that opens at `open` is too far ahead of `now` to be real: it opens more
/// than a minute after `now` (two weeks for an interval of a day or longer) and at least two
/// slots after the slot of `now`. This is the test the chart applies to pushed bars
/// (`Sink::set_server_time`), applied before a wrong time can move the bar being built.
pub(crate) fn ahead(
    clock: &BarClock,
    timeframe: Timeframe,
    open: DateTime<Utc>,
    now: DateTime<Utc>,
) -> bool {
    let allowance = if timeframe.duration_ms() < DAY_MS {
        TimeDelta::minutes(1)
    } else {
        TimeDelta::weeks(2)
    };
    let beyond = now
        .checked_add_signed(allowance)
        .is_some_and(|limit| open > limit);
    // The slot before the one of `open` is later than the slot of `now`.
    beyond
        && open
            .checked_sub_signed(TimeDelta::milliseconds(1))
            .is_some_and(|before| clock.bar_open(before) > clock.bar_open(now))
}

/// The value of a mapped field of `record`: `None` when the field is not mapped, missing or
/// `null`.
fn field<'a>(record: &'a Value, path: &[Segment]) -> Option<&'a Value> {
    if path.is_empty() {
        return None;
    }
    at(record, path).filter(|value| !value.is_null())
}

/// The bar in `record`, which is a history row or a stream record. `None` when its time is
/// missing or unreadable.
///
/// On a calendar interval the bar is stamped by [`dated`] on the row's date: the date field,
/// or the day of its instant in `zone`. On any other interval it keeps its instant; a row that
/// has only a date opens at midnight of that date in `zone`.
///
/// A price that is missing, `null` or unreadable is NaN. When all four are, the result is
/// `Bar::whitespace`; otherwise the chart drops a bar with a NaN price and counts it. Volume is
/// zero when it is not mapped, missing or `null`, and NaN when it is unreadable.
pub(crate) fn bar(
    record: &Value,
    fields: &BarFields,
    zone: &TimezoneId,
    clock: &BarClock,
    timeframe: Timeframe,
) -> Option<Bar> {
    let time = match (stamp(record, &fields.time, zone)?, calendar(timeframe)) {
        (Stamp::On(date), true) => dated(clock, date),
        (Stamp::At(instant), true) => dated(clock, instant.with_timezone(zone).date_naive()),
        (Stamp::At(instant), false) => instant,
        (Stamp::On(date), false) => wall(date.and_time(NaiveTime::MIN), zone)?,
    };
    let price = |path: &FieldPath| {
        let value = field(record, path).and_then(number);
        value.unwrap_or(f64::NAN)
    };
    let [open, high, low, close] = [&fields.open, &fields.high, &fields.low, &fields.close];
    let [open, high, low, close] = [open, high, low, close].map(price);
    if [open, high, low, close].into_iter().all(f64::is_nan) {
        return Some(Bar::whitespace(time));
    }
    let volume = field(record, &fields.volume);
    let volume = volume.map_or(0.0, |value| number(value).unwrap_or(f64::NAN));
    Some(Bar::new(time, open, high, low, close, volume))
}

/// One trade of the stream.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Trade {
    pub(crate) time: DateTime<Utc>,
    pub(crate) price: f64,
    pub(crate) size: f64,
    /// What its conditions let it change. With several conditions the strictest rule applies:
    /// `Ignore` before `VolumeOnly` before `PriceAndVolume`.
    pub(crate) rule: ConditionRule,
}

/// The trade in `record`. `None` when its time is missing, unreadable or a date without a time
/// of day, when its price or size is missing or unreadable, or when its size is negative.
pub(crate) fn trade(record: &Value, fields: &TradeFields, zone: &TimezoneId) -> Option<Trade> {
    read_trade(record, fields, zone).ok()
}

/// The trade in `record`, or the part of it that cannot be read: its time, its price or its
/// size. The feed names that part in its log.
pub(crate) fn read_trade(
    record: &Value,
    fields: &TradeFields,
    zone: &TimezoneId,
) -> Result<Trade, &'static str> {
    let Some(Stamp::At(time)) = stamp(record, &fields.time, zone) else {
        return Err("time");
    };
    let amount = |path: &FieldPath| field(record, path).and_then(number);
    let price = amount(&fields.price).ok_or("price")?;
    let size = amount(&fields.size).filter(|size| *size >= 0.0);
    let size = size.ok_or("size")?;
    // A value that is not listed counts price and volume, as a trade without a condition does.
    let rule = |value: &String| {
        let listed = fields.conditions.iter().find(|(listed, _)| listed == value);
        listed.map_or(ConditionRule::PriceAndVolume, |(_, rule)| *rule)
    };
    let strictness = |rule: &ConditionRule| match rule {
        ConditionRule::PriceAndVolume => 0,
        ConditionRule::VolumeOnly => 1,
        ConditionRule::Ignore => 2,
    };
    let rules = conditions(record, fields);
    let rule = rules.iter().map(rule).max_by_key(strictness);
    Ok(Trade {
        time,
        price,
        size,
        rule: rule.unwrap_or_default(),
    })
}

/// The condition values of `record` as the rule list compares them: one for a single value,
/// one per element for a list. Empty when no condition field is mapped or the record has none.
/// The dialog fills its condition list from these.
pub(crate) fn conditions(record: &Value, fields: &TradeFields) -> Vec<String> {
    match field(record, &fields.condition) {
        Some(Value::Array(values)) => values.iter().filter_map(text).collect(),
        Some(value) => text(value).into_iter().collect(),
        None => Vec::new(),
    }
}

/// Builds the bars of one subscription from its trades (1.2.7).
pub(crate) struct Aggregator {
    clock: BarClock,
    timeframe: Timeframe,
    building: Option<Building>,
}

/// The bar being built and what the rules of [`Aggregator::add`] need to know of it.
struct Building {
    bar: Bar,
    /// The open of the bar's slot.
    slot: DateTime<Utc>,
    /// The times of the earliest and of the latest trade that moved the prices.
    traded: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// History supplied the prices, so the open is the service's and stays.
    supplied: bool,
    /// The bar has prices of its own: it is neither an empty slot nor flat at the close of the
    /// bar before it.
    priced: bool,
}

impl Aggregator {
    /// An aggregator for bars of `timeframe` on the slots of `clock`. It holds no bar yet.
    pub(crate) fn new(clock: BarClock, timeframe: Timeframe) -> Self {
        Self {
            clock,
            timeframe,
            building: None,
        }
    }

    /// Starts from `bar`, the newest bar that history holds, or from nothing. Whatever was
    /// being built is forgotten.
    pub(crate) fn start(&mut self, bar: Option<Bar>) {
        self.building = bar.map(|bar| Building {
            slot: self.clock.bar_open(bar.time),
            traded: None,
            supplied: true,
            priced: !bar.is_whitespace(),
            bar,
        });
    }

    /// Folds `trade` in and returns the bar to push, in its state after the trade. `None` when
    /// the trade changes nothing: its rule is `Ignore`, its slot is older than the bar being
    /// built, its slot opens too far after `now` (1.2.5), or it counts volume only and no
    /// price is known yet.
    pub(crate) fn add(&mut self, trade: &Trade, now: DateTime<Utc>) -> Option<Bar> {
        let slot = self.clock.bar_open(trade.time);
        if trade.rule == ConditionRule::Ignore || ahead(&self.clock, self.timeframe, slot, now) {
            return None;
        }
        let counted = trade.rule == ConditionRule::PriceAndVolume;
        let (price, size) = (trade.price, trade.size);
        match &mut self.building {
            // The trade is late: its bar is closed.
            Some(building) if slot < building.slot => return None,
            Some(building) if slot == building.slot => {
                let bar = &mut building.bar;
                if !counted {
                    if bar.is_whitespace() {
                        return None;
                    }
                    bar.volume += size;
                } else if !building.priced {
                    // The first price of an empty slot or of a flat bar is all four prices.
                    let volume = if bar.is_whitespace() { 0.0 } else { bar.volume };
                    *bar = Bar::new(bar.time, price, price, price, price, volume + size);
                    building.traded = Some((trade.time, trade.time));
                    (building.supplied, building.priced) = (false, true);
                } else {
                    // Until its first trade, every trade is the latest of a bar from history.
                    let (first, last) = building.traded.unwrap_or((trade.time, trade.time));
                    bar.high = bar.high.max(price);
                    bar.low = bar.low.min(price);
                    bar.volume += size;
                    if trade.time >= last {
                        bar.close = price;
                    }
                    if !building.supplied && trade.time < first {
                        bar.open = price;
                    }
                    building.traded = Some((first.min(trade.time), last.max(trade.time)));
                }
            }
            // A newer slot, or no bar yet.
            left => {
                // A trade that counts volume only opens a flat bar at the close before it.
                let carried = left.as_ref().map(|left| &left.bar);
                let carried = carried.filter(|bar| !bar.is_whitespace());
                let at = if counted { price } else { carried?.close };
                let time = if calendar(self.timeframe) {
                    dated(&self.clock, period_date(slot))
                } else {
                    slot
                };
                *left = Some(Building {
                    bar: Bar::new(time, at, at, at, at, size),
                    slot,
                    traded: counted.then_some((trade.time, trade.time)),
                    supplied: false,
                    priced: counted,
                });
            }
        }
        self.bar().cloned()
    }

    /// The bar being built: the newest bar this subscription knows.
    pub(crate) fn bar(&self) -> Option<&Bar> {
        self.building.as_ref().map(|building| &building.bar)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::profile::{DateFormat, TimeSpec, TimeUnit};
    use super::*;

    #[test]
    fn trades_build_bars_by_condition_rule() {
        use ConditionRule::{Ignore, PriceAndVolume, VolumeOnly};
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let fields = TradeFields {
            time: TimeSpec::Instant {
                field: key("t"),
                unit: TimeUnit::Milliseconds,
            },
            price: key("p"),
            size: key("s"),
            condition: key("c"),
            conditions: vec![("T".to_owned(), VolumeOnly), ("X".to_owned(), Ignore)],
        };
        let utc = TimezoneId::utc();
        // 14:30:00 UTC on 11 November 2026 in milliseconds. A row gives its time as an offset.
        const T: i64 = 1_794_407_400_000;
        // The slots of 14:30 and 14:31 in seconds.
        const M30: i64 = 1_794_407_400;
        const M31: i64 = 1_794_407_460;
        let instant = |offset: i64| DateTime::<Utc>::from_timestamp_millis(T + offset).unwrap();
        // What a row compares: the time of the returned bar in seconds and its five values.
        type Seen = Option<(i64, [f64; 5])>;
        let seen = |bar: Option<Bar>| -> Seen {
            bar.map(|bar| {
                let values = [bar.open, bar.high, bar.low, bar.close, bar.volume];
                (bar.time.timestamp(), values)
            })
        };
        let bar = |slot: i64, values: [f64; 5]| -> Seen { Some((slot, values)) };
        let none = || json!(null);
        // Reads each row as the feed reads a record and folds it in with the clock at its time.
        let run = |minutes: &mut Aggregator, rows: &[(i64, f64, f64, Value, Seen)]| {
            for (offset, price, size, condition, expected) in rows {
                let record = json!({"t": T + offset, "p": price, "s": size, "c": condition});
                let read = trade(&record, &fields, &utc).unwrap();
                let returned = seen(minutes.add(&read, instant(*offset)));
                assert_eq!(returned, *expected, "trade at {offset} ms");
            }
        };

        let mut minutes = Aggregator::new(BarClock::new(Timeframe::Min1), Timeframe::Min1);
        // Ten trades from nothing. `T` counts volume only and `X` is ignored.
        minutes.start(None);
        #[rustfmt::skip]
        let from_nothing = [
            (1_000, 100.0, 10.0, none(), bar(M30, [100.0, 100.0, 100.0, 100.0, 10.0])),
            (2_000, 101.5, 5.0, none(), bar(M30, [100.0, 101.5, 100.0, 101.5, 15.0])),
            // Volume only: the prices stay.
            (3_000, 99.0, 7.0, json!("T"), bar(M30, [100.0, 101.5, 100.0, 101.5, 22.0])),
            (4_000, 50.0, 1000.0, json!("X"), None),
            // Earlier than the first trade: the open moves, the close does not.
            (500, 99.5, 3.0, none(), bar(M30, [99.5, 101.5, 99.5, 101.5, 25.0])),
            // Volume only in a new slot: a flat bar at the close before it.
            (60_000, 98.0, 4.0, json!("T"), bar(M31, [101.5, 101.5, 101.5, 101.5, 4.0])),
            // The first price of the flat bar is all four prices.
            (62_000, 102.0, 6.0, none(), bar(M31, [102.0, 102.0, 102.0, 102.0, 10.0])),
            // A trade of the slot before is late.
            (59_000, 100.0, 1.0, none(), None),
            // `@` is not listed and `T` is the stricter of the two.
            (65_000, 103.0, 2.0, json!(["@", "T"]), bar(M31, [102.0, 102.0, 102.0, 102.0, 12.0])),
            (66_000, 103.0, 2.0, json!(["T", "X"]), None),
        ];
        run(&mut minutes, &from_nothing);
        // Three trades from a bar of history: its open is the service's and stays.
        minutes.start(Some(Bar::new(instant(0), 100.0, 102.0, 99.0, 101.0, 500.0)));
        #[rustfmt::skip]
        let from_history = [
            (40_000, 101.5, 5.0, none(), bar(M30, [100.0, 102.0, 99.0, 101.5, 505.0])),
            // Earlier than the trade before it: the close stays.
            (10_000, 98.0, 1.0, none(), bar(M30, [100.0, 102.0, 98.0, 101.5, 506.0])),
            (-1_000, 97.0, 1.0, none(), None),
        ];
        run(&mut minutes, &from_history);
        // From a slot that history holds empty.
        minutes.start(Some(Bar::whitespace(instant(0))));
        #[rustfmt::skip]
        let from_an_empty_slot = [
            (5_000, 99.0, 7.0, json!("T"), None),
            (6_000, 100.0, 2.0, none(), bar(M30, [100.0, 100.0, 100.0, 100.0, 2.0])),
        ];
        run(&mut minutes, &from_an_empty_slot);

        // A time in the wrong unit lands far ahead: 14:40:00 with the clock at 14:30:06 is more
        // than a minute and nine slots ahead. It must not become the bar being built.
        let far = json!({"t": T + 600_000, "p": 500.0, "s": 1.0});
        let far = trade(&far, &fields, &utc).unwrap();
        assert_eq!(seen(minutes.add(&far, instant(6_000))), None);
        let after = bar(M30, [100.0, 100.5, 100.0, 100.5, 3.0]);
        run(&mut minutes, &[(7_000, 100.5, 1.0, none(), after)]);
        // The next slot is not far ahead, also with a clock half a second behind it.
        let next = json!({"t": T + 60_000, "p": 101.0, "s": 1.0});
        let next = trade(&next, &fields, &utc).unwrap();
        assert_eq!(
            seen(minutes.add(&next, instant(59_500))),
            bar(M31, [101.0, 101.0, 101.0, 101.0, 1.0])
        );

        // On a daily chart the bar of a trade is dated, not timed. The trade is at 14:30:01 UTC
        // on 11 November, whose midnight UTC is second 1,794,355,200. In New York that day
        // opened at 05:00 UTC, 18,000 seconds later. In Tokyo it opened at 15:00 UTC on the
        // 10th, so the stamp is 00:00 UTC on its date.
        let first = json!({"t": T + 1_000, "p": 100.0, "s": 10.0});
        let first = trade(&first, &fields, &utc).unwrap();
        for (zone, stamp) in [
            ("America/New_York", 1_794_373_200),
            ("Asia/Tokyo", 1_794_355_200),
        ] {
            let clock = BarClock::new(Timeframe::Day1).with_timezone(zone.parse().unwrap());
            let mut days = Aggregator::new(clock, Timeframe::Day1);
            days.start(None);
            let dated = bar(stamp, [100.0, 100.0, 100.0, 100.0, 10.0]);
            assert_eq!(seen(days.add(&first, first.time)), dated, "{zone}");
        }

        // What is not a trade: no time, no price, no size, a number that is none, a negative
        // size.
        for record in [
            json!({"p": 100.0, "s": 1.0}),
            json!({"t": "soon", "p": 100.0, "s": 1.0}),
            json!({"t": T, "s": 1.0}),
            json!({"t": T, "p": "NaN", "s": 1.0}),
            json!({"t": T, "p": 100.0}),
            json!({"t": T, "p": 100.0, "s": -1.0}),
        ] {
            assert_eq!(trade(&record, &fields, &utc), None, "{record}");
        }
        // A date without a time of day is the time of a bar, not of a trade.
        let by_date = TradeFields {
            time: TimeSpec::Wall {
                date: key("d"),
                format: DateFormat::Compact,
                clock: None,
            },
            ..fields.clone()
        };
        let dated = json!({"d": 20261111, "p": 100.0, "s": 1.0});
        assert_eq!(trade(&dated, &by_date, &utc), None);
        // Numbers sent as text are numbers, and a size of zero is a trade.
        let texts = json!({"t": T.to_string(), "p": "100.5", "s": "0"});
        let read = Trade {
            time: instant(0),
            price: 100.5,
            size: 0.0,
            rule: PriceAndVolume,
        };
        assert_eq!(trade(&texts, &fields, &utc), Some(read));
    }
}
