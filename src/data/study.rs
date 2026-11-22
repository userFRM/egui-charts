//! Additional price or metric histories requested by a study or drawing from the host's data source.

use crate::model::{Bar, BarClock, Instrument, Timeframe};
use chrono::{DateTime, Utc};

/// A study or drawing's additional history and live subscription. Empty symbols and absent
/// intervals follow the chart. The host owns symbol identifiers and supplies the market data.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StudyDataRequest {
    /// Host symbol identifier, or empty for the chart symbol.
    pub symbol: String,
    /// Requested interval, or `None` for the chart interval.
    pub timeframe: Option<Timeframe>,
    /// Host metric key, or `None` for price bars. Metric points keep their publication times
    /// and hold their last value; they do not use the requested price-bar interval.
    #[serde(default)]
    pub metric: Option<String>,
}

impl StudyDataRequest {
    /// Dated exchange rates from the currency `from` into `to`, as the metric `fx:{from}/{to}`
    /// of the chart symbol, whose rates do not depend on the symbol. Each point is the number of
    /// `to` units one `from` unit buys from its time until the next point; a missing or
    /// non-positive rate leaves the amounts it converts unavailable.
    ///
    /// ```
    /// use egui_charts::data::StudyDataRequest;
    ///
    /// let request = StudyDataRequest::exchange_rate("USD", "EUR");
    /// assert_eq!(request.metric.as_deref(), Some("fx:USD/EUR"));
    /// ```
    pub fn exchange_rate(from: &str, to: &str) -> Self {
        Self {
            symbol: String::new(),
            timeframe: None,
            metric: Some(format!("fx:{from}/{to}")),
        }
    }
}

/// A requested series available during calculation, sorted by time with unique bars.
/// Metric points carry their open, high, low and close and zero volume, without interval aggregation.
/// An unavailable history has no bars; it is never substituted with the chart's prices.
#[derive(Debug)]
pub struct StudyData<'a> {
    /// The declaration this series answers, before resolving chart defaults.
    pub request: StudyDataRequest,
    /// Historical and current bars owned by the chart or the host.
    pub bars: &'a [Bar],
    /// Boundaries of the requested interval in its exchange calendar.
    pub clock: BarClock,
    /// Resolved instrument metadata, when available.
    pub instrument: Option<&'a Instrument>,
    /// Resolved calculation interval.
    pub timeframe: Timeframe,
    /// The chart bars a study reads this history on, which the chart supplies to its studies.
    /// Direct callers may leave it `None`; [`closed_at`](Self::closed_at) then takes each time
    /// as a close.
    pub chart: Option<StudyChart>,
}

/// The chart bars a study reads a [`StudyData`] history on, which
/// [`StudyData::closed_at`] dates the history by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudyChart {
    /// Boundaries of the chart's bars in their exchange calendar.
    pub clock: BarClock,
    /// Whether the chart's latest bar, which is still forming, waits for a bar of an interval
    /// longer than the chart's to close before reading it, as the study's Wait for interval
    /// close input sets. `true` unless the user turns that input off.
    pub wait_for_close: bool,
}

impl<'a> StudyData<'a> {
    /// Index of the bar containing `time`, retaining missing price intervals as gaps.
    /// For metrics, the latest point at or before `time`, held until the next publication.
    pub fn bar_at(&self, time: DateTime<Utc>) -> Option<usize> {
        if self.request.metric.is_some() {
            return self
                .bars
                .partition_point(|bar| bar.time <= time)
                .checked_sub(1);
        }
        let open = self.clock.bar_open(time);
        let at = self
            .bars
            .partition_point(|bar| self.clock.bar_open(bar.time) <= open)
            .checked_sub(1)?;
        self.clock.same_bar(self.bars[at].time, time).then_some(at)
    }

    /// Index of the latest bar closed when the chart bar at `time` closes, as the chart shows
    /// a study on another interval: an hour reaches the chart bar that closes with it, and the
    /// chart bars before read the hour before. Against a longer interval, a daily or longer bar
    /// of a clock with an exchange session closes with its last trading window, and so does a
    /// daily or longer chart bar, so a day reaches the bar that ends its session and a week the
    /// week's last trading day. [`chart`](Self::chart) dates the chart bars; without it, `time`
    /// is taken as the close. Pass `developing` for the chart's latest bar, which is still
    /// forming: it reads the newest bar, which may change with each feed update, unless the
    /// chart waits for interval close, when a bar of an interval longer than the chart's counts
    /// only once it has closed by that chart bar's open. A gap holds the bar before it. A metric
    /// reads the latest point published before the close. [`bar_at`](Self::bar_at) returns the
    /// bar containing `time` instead, also while it is open.
    ///
    /// ```
    /// use chrono::{DateTime, Duration};
    /// use egui_charts::data::{StudyChart, StudyData, StudyDataRequest};
    /// use egui_charts::model::{Bar, BarClock, Timeframe};
    ///
    /// let hour = |at: i64, close: f64| {
    ///     Bar::new(DateTime::UNIX_EPOCH + Duration::hours(at), close, close, close, close, 1.0)
    /// };
    /// let bars = [hour(0, 10.5), hour(1, 20.5), hour(2, 30.5)];
    /// let minutes = |at: i64| DateTime::UNIX_EPOCH + Duration::minutes(at);
    /// let mut history = StudyData {
    ///     request: StudyDataRequest {
    ///         symbol: String::new(),
    ///         timeframe: Some(Timeframe::Hour1),
    ///         metric: None,
    ///     },
    ///     bars: &bars,
    ///     clock: BarClock::new(Timeframe::Hour1),
    ///     instrument: None,
    ///     timeframe: Timeframe::Hour1,
    ///     chart: Some(StudyChart {
    ///         clock: BarClock::new(Timeframe::Min30),
    ///         wait_for_close: true,
    ///     }),
    /// };
    /// // On a 30-minute chart the first hour reaches the 00:30 bar, which closes with it.
    /// assert_eq!(history.closed_at(minutes(0), false), None);
    /// assert_eq!(history.closed_at(minutes(30), false), Some(0));
    /// // The latest bar, 02:30, waits for the hour that opened at 02:00, unless the chart reads
    /// // the developing bar.
    /// assert_eq!(history.closed_at(minutes(150), true), Some(1));
    /// history.chart = Some(StudyChart {
    ///     clock: BarClock::new(Timeframe::Min30),
    ///     wait_for_close: false,
    /// });
    /// assert_eq!(history.closed_at(minutes(150), true), Some(2));
    /// // Without a chart the time is the close: by 01:30 only the first hour has closed, while a
    /// // metric has published its 01:00 point.
    /// history.chart = None;
    /// assert_eq!(history.closed_at(minutes(90), false), Some(0));
    /// history.request.metric = Some("open_interest".into());
    /// assert_eq!(history.closed_at(minutes(90), false), Some(1));
    /// ```
    pub fn closed_at(&self, time: DateTime<Utc>, developing: bool) -> Option<usize> {
        match &self.chart {
            Some(chart) => self.closed_on(&chart.clock, chart.wait_for_close, time, developing),
            None => self.closed_by(time, developing, false),
        }
    }

    /// [`closed_at`](Self::closed_at) on chart bars of `clock`, whatever [`chart`](Self::chart)
    /// holds.
    pub(crate) fn closed_on(
        &self,
        clock: &BarClock,
        wait_for_close: bool,
        time: DateTime<Utc>,
        developing: bool,
    ) -> Option<usize> {
        let longer = clock
            .timeframe()
            .is_some_and(|chart| self.timeframe.duration_ms() > chart.duration_ms());
        let close = if developing && wait_for_close && longer {
            time
        } else {
            let open = clock.bar_open(time);
            // Against a longer interval the chart bar closes with its last trading window, as
            // the history's bars then do, so a day reaches the bar that ends its session.
            let end = if longer {
                clock.bar_close(open)
            } else {
                clock.next_open(open)
            };
            end.unwrap_or(time)
        };
        self.closed_by(close, developing && !wait_for_close, longer)
    }

    /// Latest bar closed by `close`, or with `developing` the newest bar when it opened before
    /// `close`; for metrics, the latest point published before `close`. A bar closes at its
    /// next open or, with `trading`, at `BarClock::bar_close`, so that a daily or longer bar of
    /// a clock with a session closes with its last trading window.
    fn closed_by(&self, close: DateTime<Utc>, developing: bool, trading: bool) -> Option<usize> {
        if self.request.metric.is_some() {
            return self
                .bars
                .partition_point(|point| point.time < close)
                .checked_sub(1);
        }
        let end = self
            .bars
            .partition_point(|bar| self.clock.bar_open(bar.time) < close);
        let at = end.checked_sub(1)?;
        if developing && at + 1 == self.bars.len() {
            return Some(at);
        }
        let ends = |bar: &Bar| {
            let open = self.clock.bar_open(bar.time);
            if trading {
                self.clock.bar_close(open)
            } else {
                self.clock.next_open(open)
            }
        };
        // A bar without a scheduled close, such as a trade-based one, has closed once a later
        // bar opened. A trading date's last window can close after the next date's bar opens,
        // as a post-market past midnight UTC does, so earlier bars are checked too.
        (0..=at)
            .rev()
            .find(|&before| ends(&self.bars[before]).map_or(before < at, |end| end <= close))
    }

    /// This history as read on the bars `chart` describes.
    pub(crate) fn read_on(&self, chart: &StudyChart) -> StudyData<'a> {
        StudyData {
            request: self.request.clone(),
            bars: self.bars,
            clock: self.clock.clone(),
            instrument: self.instrument,
            timeframe: self.timeframe,
            chart: Some(chart.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, SessionWindow, TimezoneId};
    use chrono::NaiveTime;
    use chrono::Weekday::{Fri, Mon, Thu, Tue, Wed};

    /// Guards #4020 where a trading date's last window closes after midnight UTC, as a
    /// post-market ending at 20:00 in UTC-5 does, so its daily bar, dated at midnight UTC, still
    /// trades after the next date's bar opens. Against a longer interval a daily or weekly bar
    /// reaches the chart bar that closes with its last window and no earlier one, also when that
    /// chart bar is a daily one; a history of the chart's own interval keeps its dates.
    #[test]
    fn longer_bars_close_with_their_last_trading_window() {
        let hm = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let session = Session::new([SessionWindow::new(weekdays, hm(9, 30), hm(16, 0))])
            .with_extended_windows([
                SessionWindow::new(weekdays, hm(4, 0), hm(9, 30)),
                SessionWindow::new(weekdays, hm(16, 0), hm(20, 0)),
            ]);
        let zone = TimezoneId::fixed_offset(-5 * 3600).unwrap();
        let clock =
            |timeframe| BarClock::new(timeframe).with_session(zone.clone(), session.clone(), true);
        let t = |rfc3339: &str| rfc3339.parse::<DateTime<Utc>>().unwrap();
        let bars = |stamps: &[&str]| -> Vec<Bar> {
            stamps
                .iter()
                .map(|stamp| Bar::new(t(stamp), 1.0, 1.0, 1.0, 1.0, 1.0))
                .collect()
        };
        let days = bars(&[
            "2024-03-11T00:00:00Z",
            "2024-03-12T00:00:00Z",
            "2024-03-13T00:00:00Z",
        ]);
        let weeks = bars(&["2024-03-04T00:00:00Z", "2024-03-11T00:00:00Z"]);
        let (minutes, daily) = (clock(Timeframe::Min30), clock(Timeframe::Day1));
        // The history's interval, the chart's clock, a chart bar's time and the bar read there.
        #[rustfmt::skip]
        let rows = [
            // Tuesday 18:30 and 19:00 close before Tuesday's post-market does, at 01:00 UTC.
            (Timeframe::Day1, &minutes, "2024-03-12T23:30:00Z", Some(0)),
            (Timeframe::Day1, &minutes, "2024-03-13T00:00:00Z", Some(0)),
            (Timeframe::Day1, &minutes, "2024-03-13T00:30:00Z", Some(1)),
            // Friday's daily bar closes with the week, at 01:00 UTC on Saturday.
            (Timeframe::Week1, &daily, "2024-03-08T00:00:00Z", Some(0)),
            (Timeframe::Day1, &daily, "2024-03-12T00:00:00Z", Some(1)),
        ];
        for (timeframe, chart, time, read) in rows {
            let history = StudyData {
                request: StudyDataRequest {
                    symbol: String::new(),
                    timeframe: Some(timeframe),
                    metric: None,
                },
                bars: if timeframe == Timeframe::Week1 {
                    &weeks
                } else {
                    &days
                },
                clock: clock(timeframe),
                instrument: None,
                timeframe,
                chart: Some(StudyChart {
                    clock: chart.clone(),
                    wait_for_close: true,
                }),
            };
            assert_eq!(
                history.closed_at(t(time), false),
                read,
                "{timeframe} at {time}"
            );
        }
    }
}
