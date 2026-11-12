//! The mapped feed: a profile served to the chart through the library's `Datafeed` trait.

use chrono::{DateTime, Utc};
use egui_charts::data::{
    Datafeed, FeedCapabilities, HistoryPage, HistoryRequest, Reply, Subscription, SubscriptionId,
};
use egui_charts::model::{Bar, BarClock, Instrument, Timeframe, TimezoneId};
use serde_json::Value;

use super::Secrets;
use super::http::Request;
use super::profile::{History, Profile};

/// The feed of one profile. It answers history from the REST template and keeps one stream
/// session per subscription. One instance serves every chart of the own-data mode.
pub(crate) struct MappedFeed {}

impl MappedFeed {
    /// A feed that reads `profile` with the key values `secrets`. Nothing is sent until a chart
    /// asks.
    pub(crate) fn new(_profile: Profile, _secrets: Secrets) -> Self {
        unimplemented!("part A")
    }
}

impl Datafeed for MappedFeed {
    fn capabilities(&self, _reply: Reply<FeedCapabilities>) {
        unimplemented!("part A")
    }

    fn resolve(&self, _symbol: &str, _reply: Reply<Instrument>) {
        unimplemented!("part A")
    }

    fn history(&self, _request: HistoryRequest, _reply: Reply<HistoryPage>) {
        unimplemented!("part A")
    }

    fn subscribe(&self, _subscription: Subscription) {
        unimplemented!("part A")
    }

    fn unsubscribe(&self, _id: SubscriptionId) {
        unimplemented!("part A")
    }
}

/// What a history response holds when it is read with a mapping.
pub(crate) struct Reading {
    /// The bars with a readable time, oldest first. Bars of equal time keep the order of the
    /// response.
    pub(crate) bars: Vec<Bar>,
    /// Rows left out because their time is missing or unreadable, or lies too far after `now`
    /// (1.2.5).
    pub(crate) skipped: usize,
}

/// Reads a parsed history response. The feed and the dialog's Test both call it, so the preview
/// cannot disagree with the chart. The error says why nothing can be read: the rows are not
/// where the mapping says, or no row has a readable time.
pub(crate) fn read(
    _response: &Value,
    _history: &History,
    _zone: &TimezoneId,
    _clock: &BarClock,
    _timeframe: Timeframe,
    _now: DateTime<Utc>,
) -> Result<Reading, String> {
    unimplemented!("part A")
}

/// The request for the newest page of `symbol` at `timeframe`, exactly as the feed sends it for
/// a chart's first load. The dialog's Test sends this request. The error says why none can be
/// built: the profile has no history, or its interval table has no row for `timeframe`.
pub(crate) fn latest(
    _profile: &Profile,
    _secrets: &Secrets,
    _symbol: &str,
    _timeframe: Timeframe,
    _clock: &BarClock,
    _now: DateTime<Utc>,
) -> Result<Request, String> {
    unimplemented!("part A")
}
