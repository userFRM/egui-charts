//! Instrument metrics published by a host, independent of its price bars.

use super::{Sink, SubscriptionId};
use crate::model::Bar;
use crate::scales::StudyFormat;
use chrono::{DateTime, Utc};

/// One metric offered for an instrument, identified by a host-owned key.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct MetricSeries {
    /// Key used in metric history and subscription requests.
    pub id: String,
    /// Display name, such as "Open interest".
    pub name: String,
    /// Display unit, such as "contracts", "USD" or "%".
    pub unit: String,
    /// Numeric format used in legends, axes and tooltips.
    pub format: StudyFormat,
}

impl MetricSeries {
    /// A metric without a display unit: assign [`unit`](Self::unit) to show one.
    pub fn new(id: impl Into<String>, name: impl Into<String>, format: StudyFormat) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            unit: String::new(),
            format,
        }
    }
}

/// A metric's value from this instant until the next published point.
/// Non-finite values are rejected, like invalid price bars.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct MetricPoint {
    /// UTC instant at which this observation takes effect.
    pub time: DateTime<Utc>,
    /// Metric value in the units declared by `MetricSeries`.
    pub value: f64,
}

impl MetricPoint {
    /// `value` from `time` until the next published point.
    pub fn new(time: DateTime<Utc>, value: f64) -> Self {
        Self { time, value }
    }
}

impl From<MetricPoint> for Bar {
    fn from(point: MetricPoint) -> Self {
        Self::new(
            point.time,
            point.value,
            point.value,
            point.value,
            point.value,
            0.0,
        )
    }
}

/// Up to `count` metric points strictly before `before`, or the latest when absent.
/// Include the point in effect before the chart's first bar when paging back.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MetricRequest {
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// [`MetricSeries::id`] for this instrument.
    pub metric: String,
    /// Exclusive upper time bound, or `None` for the latest points.
    pub before: Option<DateTime<Utc>>,
    /// Maximum number of points to return.
    pub count: usize,
}

impl MetricRequest {
    /// The latest `count` points of `metric` for `symbol`: assign [`before`](Self::before) to
    /// page back.
    ///
    /// ```
    /// use egui_charts::data::MetricRequest;
    ///
    /// let request = MetricRequest::new("AAPL", "open_interest", 100);
    /// assert_eq!(request.before, None);
    /// ```
    pub fn new(symbol: impl Into<String>, metric: impl Into<String>, count: usize) -> Self {
        Self {
            symbol: symbol.into(),
            metric: metric.into(),
            before: None,
            count,
        }
    }
}

/// A page of metric history, in any order. The chart sorts and deduplicates by timestamp.
/// A live correction of the same timestamp takes precedence over history.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct MetricPage {
    /// Metric observations in this page.
    pub points: Vec<MetricPoint>,
    /// Whether this page reaches the beginning of the available history.
    pub end_of_history: bool,
}

impl MetricPage {
    /// A page of `points`, the last one when `end_of_history`.
    pub fn new(points: Vec<MetricPoint>, end_of_history: bool) -> Self {
        Self {
            points,
            end_of_history,
        }
    }
}

/// A live metric stream, stopped through [`super::Datafeed::unsubscribe`].
/// Push with [`Sink::push_metric`]; timestamps are retained without price-bar aggregation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MetricSubscription {
    /// Identifier used to unsubscribe this metric stream.
    pub id: SubscriptionId,
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// Host-owned metric identifier for this instrument.
    pub metric: String,
    /// Destination for updates; stop publishing when it becomes inactive.
    pub sink: Sink,
}

impl MetricSubscription {
    /// Points of `metric` for `symbol`, pushed into `sink` with [`Sink::push_metric`]. Its
    /// [`id`](Self::id) is the sink's. A [`SinkReader`](super::SinkReader) builds a sink outside
    /// a chart, to test [`Datafeed::subscribe_metric`](super::Datafeed::subscribe_metric).
    ///
    /// ```
    /// use egui_charts::data::{
    ///     Datafeed, HistoryPage, HistoryRequest, MetricPoint, MetricSubscription, Reply, SinkReader,
    /// };
    /// use egui_charts::model::Timeframe;
    ///
    /// /// Publishes the open interest of the subscribed symbol.
    /// struct OpenInterest;
    ///
    /// impl Datafeed for OpenInterest {
    ///     fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
    ///         reply.send(Ok(HistoryPage::default()));
    ///     }
    ///
    ///     fn subscribe_metric(&self, subscription: MetricSubscription) {
    ///         let at = "2026-01-05T15:00:00Z".parse().expect("a valid time");
    ///         subscription.sink.push_metric(MetricPoint::new(at, 41_250.0));
    ///     }
    /// }
    ///
    /// let (reader, sink) = SinkReader::new("BTCUSD", Timeframe::Min1, None);
    /// OpenInterest.subscribe_metric(MetricSubscription::new("BTCUSD", "open_interest", sink));
    /// let bars = reader.take().bars;
    /// assert_eq!(bars.len(), 1);
    /// assert_eq!((bars[0].close, bars[0].volume), (41_250.0, 0.0));
    /// ```
    pub fn new(symbol: impl Into<String>, metric: impl Into<String>, sink: Sink) -> Self {
        Self {
            id: sink.id,
            symbol: symbol.into(),
            metric: metric.into(),
            sink,
        }
    }
}

impl Sink {
    /// Publish or correct a metric point. Its value is held until the next point;
    /// an inactive subscription ignores pushes, like [`Self::push_bar`].
    pub fn push_metric(&self, point: MetricPoint) {
        self.push_bar(point.into());
    }
}
