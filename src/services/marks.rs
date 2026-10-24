//! Bar annotations and time-axis events fetched for the visible range.

use crate::data::{MaybeSend, Reply};
use crate::model::{Marker, TimeScaleEvent, Timeframe};
use chrono::{DateTime, Utc};

/// Host source of marks. Attach with `Chart::set_mark_source`.
///
/// Called on the UI thread; return promptly and answer through the reply from any thread,
/// runtime or browser task. Each answer replaces the previous source marks. Manually supplied
/// markers and events remain independent. The chart ignores answers for an older view.
pub trait Marks: MaybeSend + 'static {
    /// All marks for the inclusive UTC range, including visible whitespace. Called again after
    /// the symbol, interval or range changes, or `Chart::reload_marks` is requested.
    fn marks(&self, request: MarksRequest, reply: Reply<MarksPage>);
}

/// The chart view for which a host supplies annotations and events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarksRequest {
    /// Current instrument key.
    pub symbol: String,
    /// Current chart interval.
    pub timeframe: Timeframe,
    /// Inclusive left edge of the view in UTC.
    pub from: DateTime<Utc>,
    /// Inclusive right edge of the view in UTC.
    pub to: DateTime<Utc>,
}

/// One complete answer for a visible range. Empty vectors clear the source's marks.
#[derive(Debug, Clone, Default)]
pub struct MarksPage {
    /// Bar annotations, including their optional hover text.
    pub bar_marks: Vec<Marker>,
    /// Generic time-axis badges, including their optional hover text and host IDs.
    pub time_scale_marks: Vec<Marker>,
    /// Typed corporate, economic and news events, filtered by chart event visibility.
    pub events: Vec<TimeScaleEvent>,
}
