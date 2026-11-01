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

/// A requested series available during calculation, sorted by time with unique bars.
/// Metric points use their value for OHLC and zero volume, without interval aggregation.
/// An unavailable history has no bars; it is never substituted with the chart's prices.
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
}

impl StudyData<'_> {
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

    /// Latest output available at a chart bar's close. Historical higher-interval outputs
    /// appear only after their bar closes. Disabling `wait_for_close` also exposes the newest
    /// developing bar on the chart's latest bar, which may change with each feed update.
    pub(crate) fn output_at(&self, close: DateTime<Utc>, developing: bool) -> Option<usize> {
        let end = self
            .bars
            .partition_point(|bar| self.clock.bar_open(bar.time) < close);
        let at = end.checked_sub(1)?;
        if developing && at + 1 == self.bars.len() {
            return Some(at);
        }
        let closed = self
            .clock
            .next_open(self.clock.bar_open(self.bars[at].time))
            .is_some_and(|end| end <= close);
        if closed { Some(at) } else { at.checked_sub(1) }
    }
}
