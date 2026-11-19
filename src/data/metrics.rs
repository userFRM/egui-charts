//! Instrument metrics published by a host, independent of its price bars.

use super::{Sink, SubscriptionId};
use crate::model::Bar;
use crate::scales::StudyFormat;
use chrono::{DateTime, Utc};
use egui::Color32;

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
    /// How the study draws values until the user picks another plot type in its Style tab.
    /// Metrics saved before plots could be declared draw as a step line.
    #[serde(default)]
    pub plot: MetricPlot,
    /// Plot colours and the bars each applies to, or `None` for the chart's default colour.
    /// The Style tab edits these colours and saves the user's choices with the study.
    #[serde(default)]
    pub colors: Option<MetricColors>,
}

impl MetricSeries {
    /// A metric without a display unit, drawn as a step line in the chart's default colour:
    /// assign [`unit`](Self::unit), [`plot`](Self::plot) and [`colors`](Self::colors) to change them.
    ///
    /// ```
    /// use ecolor::Color32;
    /// use egui_charts::data::{MetricColors, MetricPlot, MetricSeries};
    /// use egui_charts::scales::StudyFormat;
    ///
    /// let mut funding = MetricSeries::new("funding_rate", "Funding rate", StudyFormat::Percent);
    /// funding.unit = "%".into();
    /// funding.plot = MetricPlot::Columns;
    /// funding.colors = Some(MetricColors::Sign {
    ///     positive: Color32::from_rgb(8, 153, 129),
    ///     negative: Color32::from_rgb(242, 54, 69),
    /// });
    /// let interest = MetricSeries::new("open_interest", "Open interest", StudyFormat::Volume);
    /// assert_eq!((interest.plot, interest.colors), (MetricPlot::StepLine, None));
    /// ```
    pub fn new(id: impl Into<String>, name: impl Into<String>, format: StudyFormat) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            unit: String::new(),
            format,
            plot: MetricPlot::StepLine,
            colors: None,
        }
    }
}

/// How a metric's study draws its values until the user picks a plot type in its Style tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum MetricPlot {
    /// Values joined by straight segments.
    Line,
    /// Each value held as a horizontal segment until the next one.
    #[default]
    StepLine,
    /// Columns from zero, as wide as the bar spacing allows.
    Columns,
    /// Thin bars from zero.
    Histogram,
    /// A line over a fill down to zero.
    Area,
    /// A candle on each bar from the open, high, low and close of the points published in it
    /// (see [`MetricPoint::ohlc`]).
    /// A bar without such a point draws a flat candle at the value it holds.
    Candles,
}

/// The colours of a metric's plot and the bars each applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum MetricColors {
    /// Every bar in one colour.
    Single(Color32),
    /// Bars whose value rose in `rising` and bars whose value fell in `falling`.
    /// A value falls below the previous bar's, or for candles below its open;
    /// an unchanged value keeps the previous colour, and the first scalar value is rising.
    Change {
        /// Colour of rising and first values.
        rising: Color32,
        /// Colour of falling values.
        falling: Color32,
    },
    /// Values at or above zero in `positive` and values below zero in `negative`.
    Sign {
        /// Colour of values at or above zero.
        positive: Color32,
        /// Colour of values below zero.
        negative: Color32,
    },
}

/// A metric's value from this instant until the next published point, with the open,
/// high and low of the period it closes for a metric drawn as candles. Non-finite values
/// are rejected, like invalid price bars, as are highs below the open or close and lows above them.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct MetricPoint {
    /// UTC instant at which this observation takes effect.
    pub time: DateTime<Utc>,
    /// Metric value in the units declared by `MetricSeries`, the close of a candle.
    pub value: f64,
    /// Value at the start of the period this point closes; [`new`](Self::new) sets it to `value`.
    pub open: f64,
    /// Highest value in that period; [`new`](Self::new) sets it to `value`.
    pub high: f64,
    /// Lowest value in that period; [`new`](Self::new) sets it to `value`.
    pub low: f64,
}

impl MetricPoint {
    /// `value` from `time` until the next published point.
    pub fn new(time: DateTime<Utc>, value: f64) -> Self {
        Self {
            time,
            value,
            open: value,
            high: value,
            low: value,
        }
    }

    /// `close` from `time` until the next published point, with the `open`, `high` and `low`
    /// of the period it closes.
    ///
    /// ```
    /// use egui_charts::data::MetricPoint;
    ///
    /// let at = "2026-01-05T00:00:00Z".parse().expect("a valid time");
    /// let day = MetricPoint::ohlc(at, 41_000.0, 41_900.0, 40_800.0, 41_250.0);
    /// assert_eq!((day.open, day.high, day.low, day.value), (41_000.0, 41_900.0, 40_800.0, 41_250.0));
    /// let point = MetricPoint::new(at, 41_250.0);
    /// assert_eq!((point.open, point.high, point.low), (41_250.0, 41_250.0, 41_250.0));
    /// ```
    pub fn ohlc(time: DateTime<Utc>, open: f64, high: f64, low: f64, close: f64) -> Self {
        Self {
            time,
            value: close,
            open,
            high,
            low,
        }
    }
}

impl From<MetricPoint> for Bar {
    fn from(point: MetricPoint) -> Self {
        Self::new(
            point.time,
            point.open,
            point.high,
            point.low,
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
