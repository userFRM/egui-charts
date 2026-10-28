//! What a feed answers for the market panels beside the chart: news, highlights, the event
//! calendars, the screener and the hotlists.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The stories a news or highlights panel asks [`Datafeed::news`](super::Datafeed::news) for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewsQuery {
    /// Stories about any of these symbols, or about the whole market when empty.
    pub symbols: Vec<String>,
    /// Only stories of these topics, in the values the feed supports, or of every topic when
    /// empty.
    pub topics: Vec<String>,
    /// Only stories from these providers, in the identifiers the feed uses, or from every
    /// provider when empty.
    pub providers: Vec<String>,
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
    /// Headline displayed in the news list.
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
    /// Company earnings releases.
    Earnings,
    /// Dividend events.
    Dividends,
    /// Initial public offerings.
    Ipos,
}

/// How much an economic event is expected to move the market.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Importance {
    /// Low importance.
    Low,
    /// Medium importance.
    Medium,
    /// High importance.
    High,
}

/// The events a calendar panel asks [`Datafeed::calendar`](super::Datafeed::calendar) for. The
/// feed applies the filters.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CalendarQuery {
    /// Calendar category, such as earnings or economic releases.
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
    /// Calendar category, such as earnings or economic releases.
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
    /// Importance assigned by the host, when applicable.
    pub importance: Option<Importance>,
    /// Published figure, formatted by the host, when available.
    pub actual: Option<String>,
    /// Expected figure, formatted by the host, when available.
    pub forecast: Option<String>,
    /// Previous figure, formatted by the host, when available.
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

/// A column the screener shows, filters and sorts by.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScreenerColumn {
    /// Identifies the column to the feed, such as `"change_percent"`.
    pub id: String,
    /// The heading, such as `"Chg %"`.
    pub title: String,
}

impl ScreenerColumn {
    /// The column `id` headed `title`.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
        }
    }
}

/// A named set of columns the user switches between, such as `"Overview"` or `"Performance"`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScreenerColumnSet {
    /// Display name.
    pub name: String,
    /// Columns offered by this set, in display order.
    pub columns: Vec<ScreenerColumn>,
}

impl ScreenerColumnSet {
    /// The set `name` of `columns`, in display order.
    pub fn new(name: impl Into<String>, columns: Vec<ScreenerColumn>) -> Self {
        Self {
            name: name.into(),
            columns,
        }
    }
}

/// A market the screener screens, such as stocks, funds, bonds, coins or the pairs of one
/// exchange, as the host names it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScreenerUniverse {
    /// Identifies the universe to the feed, such as `"coin"`.
    pub id: String,
    /// The name shown, such as `"Coins"`.
    pub name: String,
}

impl ScreenerUniverse {
    /// The universe `id` named `name`.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

/// How a screener filter compares a column's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScreenerOp {
    /// Greater than the filter's value.
    Above,
    /// Less than the filter's value.
    Below,
}

/// One condition every row must meet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenerFilter {
    /// The [`ScreenerColumn::id`] compared.
    pub column: String,
    /// Comparison applied to the selected column.
    pub op: ScreenerOp,
    /// Comparison threshold.
    pub value: f64,
}

/// The rows a screener asks [`Datafeed::screen`](super::Datafeed::screen) for. The feed
/// filters, sorts and limits.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ScreenerQuery {
    /// The [`ScreenerUniverse::id`] to screen, or `None` when the host offers no universes.
    pub universe: Option<String>,
    /// What the user wrote to describe the screen, or empty for none. The feed interprets it
    /// together with the other criteria, answering
    /// [`Datafeed::screen_text`](super::Datafeed::screen_text).
    pub text: String,
    /// The [`ScreenerColumn::id`]s each row answers a value for, in order.
    pub columns: Vec<String>,
    /// Conditions every row meets.
    pub filters: Vec<ScreenerFilter>,
    /// The column to sort by, or the feed's own order when `None`.
    pub sort_by: Option<String>,
    /// Sort from the largest value.
    pub descending: bool,
    /// Rows to skip, for the next page of the same screen.
    pub offset: usize,
    /// Rows wanted at most.
    pub limit: usize,
}

/// What a feed answers for a screen that has a text description.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ScreenerTextAnswer {
    /// The rows that pass the screen, as for [`Datafeed::screen`](super::Datafeed::screen).
    pub rows: Vec<ScreenerRow>,
    /// How the feed understood the description, such as the criteria it applied, shown above the
    /// rows. May be empty.
    pub explanation: String,
}

impl ScreenerTextAnswer {
    /// The answer `rows`, understood as `explanation`.
    pub fn new(rows: Vec<ScreenerRow>, explanation: impl Into<String>) -> Self {
        Self {
            rows,
            explanation: explanation.into(),
        }
    }
}

/// One symbol that passes the screen, with one formatted value per query column.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScreenerRow {
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// The values of [`ScreenerQuery::columns`], in that order, already formatted.
    pub values: Vec<String>,
}

impl ScreenerRow {
    /// The row of `symbol` with `values` in the order of the query's columns.
    pub fn new(symbol: impl Into<String>, values: Vec<String>) -> Self {
        Self {
            symbol: symbol.into(),
            values,
        }
    }
}

/// The symbols of one hotlist that a hotlists panel asks
/// [`Datafeed::hotlist`](super::Datafeed::hotlist) for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HotlistQuery {
    /// The list, one of the names the host gave the panel, such as `"Top gainers"`.
    pub list: String,
    /// Symbols wanted at most.
    pub limit: usize,
}

/// One symbol of a hotlist, with its last price and its change since the previous close.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HotlistEntry {
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// Last traded price.
    pub last: f64,
    /// The change since the previous close in percent, such as `2.5` for 2.5 %.
    pub change_percent: f64,
    /// Decimals of the price, or a default from its magnitude when `None`.
    pub price_decimals: Option<u8>,
}

impl HotlistEntry {
    /// `symbol` last at `last`, `change_percent` from the previous close.
    pub fn new(symbol: impl Into<String>, last: f64, change_percent: f64) -> Self {
        Self {
            symbol: symbol.into(),
            last,
            change_percent,
            price_decimals: None,
        }
    }
}
