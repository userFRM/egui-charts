//! What a feed answers for the market panels beside the chart: news, highlights and the event
//! calendars.

use chrono::{DateTime, Utc};

/// The stories a news or highlights panel asks [`Datafeed::news`](super::Datafeed::news) for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewsQuery {
    /// Stories about this symbol, or about the whole market when `None`.
    pub symbol: Option<String>,
    /// Short summaries of what moves the market or the symbol, instead of headlines.
    pub highlights: bool,
    /// Stories wanted at most.
    pub limit: usize,
}

/// One story, already formatted for display.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NewsStory {
    /// Identifies the story among the stories of one answer.
    pub id: String,
    /// When the story was published; the panel offers to show the chart at this time.
    pub time: DateTime<Utc>,
    pub headline: String,
    /// The publisher or provider, shown beside the time. May be empty.
    pub source: String,
    /// The story's text, shown when it is opened, and under the headline of a highlight. May be
    /// empty.
    pub body: String,
    /// The symbols the story is about.
    pub symbols: Vec<String>,
    /// A link to the full story, which the host opens.
    pub url: Option<String>,
}

impl NewsStory {
    /// A story with a headline and nothing else: set the other fields as known.
    pub fn new(id: impl Into<String>, time: DateTime<Utc>, headline: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            time,
            headline: headline.into(),
            source: String::new(),
            body: String::new(),
            symbols: Vec::new(),
            url: None,
        }
    }
}

/// The calendar an event belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CalendarKind {
    /// Economic releases and central bank decisions.
    Economic,
    Earnings,
    Dividends,
    /// Initial public offerings.
    Ipos,
}

/// How much an economic event is expected to move the market.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Importance {
    Low,
    Medium,
    High,
}

/// The events a calendar panel asks [`Datafeed::calendar`](super::Datafeed::calendar) for. The
/// feed applies the filters.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CalendarQuery {
    pub kind: CalendarKind,
    /// Events at or after this time.
    pub from: DateTime<Utc>,
    /// Events before this time.
    pub to: DateTime<Utc>,
    /// Only events of these countries, in the codes the feed uses, or of every country when empty.
    pub countries: Vec<String>,
    /// Only events at least this important, when set. Set for economic events only.
    pub importance: Option<Importance>,
}

/// One calendar event, its figures already formatted for display.
///
/// `actual`, `forecast` and `prior` are the reported figure, the consensus and the previous
/// figure: the release for an economic event, the earnings per share for earnings, the amount
/// per share for a dividend and the offer price for an IPO. Leave unknown figures `None`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CalendarEvent {
    /// Identifies the event among the events of one answer.
    pub id: String,
    pub kind: CalendarKind,
    /// When the figure is released, the earnings reported, the dividend goes ex or the shares
    /// list.
    pub time: DateTime<Utc>,
    /// The country, in the codes the feed uses, such as `"US"`.
    pub country: String,
    /// The release, such as `"CPI (YoY)"`, or the company's name.
    pub title: String,
    /// The company's symbol, for earnings, dividends and IPOs; the panel offers to open it.
    pub symbol: Option<String>,
    pub importance: Option<Importance>,
    pub actual: Option<String>,
    pub forecast: Option<String>,
    pub prior: Option<String>,
}

impl CalendarEvent {
    /// An event with a title and nothing else: set the other fields as known.
    pub fn new(
        id: impl Into<String>,
        kind: CalendarKind,
        time: DateTime<Utc>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            time,
            country: String::new(),
            title: title.into(),
            symbol: None,
            importance: None,
            actual: None,
            forecast: None,
            prior: None,
        }
    }
}
