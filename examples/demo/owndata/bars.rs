//! Bars from history rows, from stream records and from trades.

use chrono::{DateTime, NaiveDate, Utc};
use egui_charts::model::{Bar, BarClock, Timeframe, TimezoneId};
use serde_json::Value;

use super::profile::{BarFields, ConditionRule, TradeFields};

/// Whether bars of `timeframe` are whole days, weeks or months: a duration of at least one day
/// that is a multiple of a day. Such a bar is dated, not timed (1.2.6).
pub(crate) fn calendar(_timeframe: Timeframe) -> bool {
    unimplemented!("part A")
}

/// The first calendar date of the period of a chart's clock that opens at `open`: the UTC date
/// of `open` when its UTC time of day is 10:00 or earlier, otherwise the next UTC date.
// shortcut: exact for chart time zones from UTC-10:00 to UTC+13:45, because `BarClock` does not
// expose its zone. Upgrade when the clock can name the local date of an instant.
pub(crate) fn period_date(_open: DateTime<Utc>) -> NaiveDate {
    unimplemented!("part A")
}

/// The time a bar of a calendar interval carries for the trading date `date`: the later of
/// 00:00 UTC on the date and the open of the period of `clock` that holds the date, which is
/// what `HistoryPage::bars` asks of a feed. The period is the one `clock` opens for 10:00 UTC
/// on the date, with the limit stated on [`period_date`].
pub(crate) fn dated(_clock: &BarClock, _date: NaiveDate) -> DateTime<Utc> {
    unimplemented!("part A")
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
    _record: &Value,
    _fields: &BarFields,
    _zone: &TimezoneId,
    _clock: &BarClock,
    _timeframe: Timeframe,
) -> Option<Bar> {
    unimplemented!("part A")
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
pub(crate) fn trade(_record: &Value, _fields: &TradeFields, _zone: &TimezoneId) -> Option<Trade> {
    unimplemented!("part A")
}

/// The condition values of `record` as the rule list compares them: one for a single value,
/// one per element for a list. Empty when no condition field is mapped or the record has none.
/// The dialog fills its condition list from these.
pub(crate) fn conditions(_record: &Value, _fields: &TradeFields) -> Vec<String> {
    unimplemented!("part A")
}

/// Builds the bars of one subscription from its trades (1.2.7).
pub(crate) struct Aggregator {}

impl Aggregator {
    /// An aggregator for bars of `timeframe` on the slots of `clock`. It holds no bar yet.
    pub(crate) fn new(_clock: BarClock, _timeframe: Timeframe) -> Self {
        unimplemented!("part A")
    }

    /// Starts from `bar`, the newest bar that history holds, or from nothing. Whatever was
    /// being built is forgotten.
    pub(crate) fn start(&mut self, _bar: Option<Bar>) {
        unimplemented!("part A")
    }

    /// Folds `trade` in and returns the bar to push, in its state after the trade. `None` when
    /// the trade changes nothing: its rule is `Ignore`, its slot is older than the bar being
    /// built, its slot opens too far after `now` (1.2.5), or it counts volume only and no
    /// price is known yet.
    pub(crate) fn add(&mut self, _trade: &Trade, _now: DateTime<Utc>) -> Option<Bar> {
        unimplemented!("part A")
    }

    /// The bar being built: the newest bar this subscription knows.
    pub(crate) fn bar(&self) -> Option<&Bar> {
        unimplemented!("part A")
    }
}
