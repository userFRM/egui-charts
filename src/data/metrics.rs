//! Instrument metrics published by a host, independent of its price bars.

use super::{Sink, SubscriptionId};
use crate::model::Bar;
use crate::scales::StudyFormat;
use chrono::{DateTime, Utc};

/// One metric offered for an instrument, identified by a host-owned key.
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// A metric's value from this instant until the next published point.
/// Non-finite values are rejected, like invalid price bars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricPoint {
    /// UTC instant at which this observation takes effect.
    pub time: DateTime<Utc>,
    /// Metric value in the units declared by `MetricSeries`.
    pub value: f64,
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

/// A page of metric history, in any order. The chart sorts and deduplicates by timestamp.
/// A live correction of the same timestamp takes precedence over history.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricPage {
    /// Metric observations in this page.
    pub points: Vec<MetricPoint>,
    /// Whether this page reaches the beginning of the available history.
    pub end_of_history: bool,
}

/// A live metric stream, stopped through [`super::Datafeed::unsubscribe`].
/// Push with [`Sink::push_metric`]; timestamps are retained without price-bar aggregation.
#[derive(Debug, Clone)]
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

impl Sink {
    /// Publish or correct a metric point. Its value is held until the next point;
    /// an inactive subscription ignores pushes, like [`Self::push_bar`].
    pub fn push_metric(&self, point: MetricPoint) {
        self.push_bar(point.into());
    }
}
