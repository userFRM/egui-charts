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
    /// Only stories about these countries, in the codes the feed uses, such as `"US"`, or about
    /// every country when empty.
    pub countries: Vec<String>,
    /// Short summaries of what moves the market or the symbol, instead of headlines.
    pub highlights: bool,
    /// Stories wanted at most.
    pub limit: usize,
}

impl NewsQuery {
    /// Up to `limit` headlines about the whole market, of every topic, provider and country.
    /// Assign [`symbols`](Self::symbols), [`topics`](Self::topics),
    /// [`providers`](Self::providers) and [`countries`](Self::countries) to narrow it, and
    /// [`highlights`](Self::highlights) for summaries instead of headlines.
    ///
    /// ```
    /// use egui_charts::data::NewsQuery;
    ///
    /// let mut query = NewsQuery::new(50);
    /// query.symbols.push("AAPL".into());
    /// assert!(query.topics.is_empty());
    /// assert!(query.countries.is_empty());
    /// ```
    pub fn new(limit: usize) -> Self {
        Self {
            symbols: Vec::new(),
            topics: Vec::new(),
            providers: Vec::new(),
            countries: Vec::new(),
            highlights: false,
            limit,
        }
    }
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
#[non_exhaustive]
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
    /// Only economic events of these categories, in the values the feed uses, such as
    /// `"Inflation"`, or of every category when empty. Set for economic events only.
    pub categories: Vec<String>,
    /// Only events at least this important, when set. Set for economic events only.
    pub importance: Option<Importance>,
    /// Only events of these symbols, in the identifiers the chart uses, or of every symbol when
    /// empty. Set for earnings, dividends and IPOs, whose events carry a
    /// [`symbol`](CalendarEvent::symbol).
    pub symbols: Vec<String>,
}

impl CalendarQuery {
    /// The events of the `kind` calendar from `from` until before `to`, of every country and
    /// importance. Assign [`countries`](Self::countries), [`symbols`](Self::symbols) and, for
    /// economic events, [`categories`](Self::categories) and
    /// [`importance`](Self::importance) to narrow it.
    ///
    /// ```
    /// use egui_charts::data::{CalendarKind, CalendarQuery, Importance};
    ///
    /// let mut query = CalendarQuery::new(
    ///     CalendarKind::Economic,
    ///     "2026-01-05T00:00:00Z".parse()?,
    ///     "2026-01-12T00:00:00Z".parse()?,
    /// );
    /// query.importance = Some(Importance::High);
    /// assert!(query.countries.is_empty());
    /// assert!(query.categories.is_empty());
    /// assert!(query.symbols.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(kind: CalendarKind, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        Self {
            kind,
            from,
            to,
            countries: Vec::new(),
            categories: Vec::new(),
            importance: None,
            symbols: Vec::new(),
        }
    }
}

/// One calendar event, its figures already formatted for display.
///
/// `actual`, `forecast` and `prior` are the reported figure, the consensus and the previous
/// figure: the release for an economic event, the earnings per share for earnings, the amount
/// per share for a dividend and the offer price for an IPO. Leave unknown figures `None`.
/// Company calendars also accept the optional figures below; columns absent from the entire
/// answer are hidden. All figures and dates are formatted by the host.
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
    /// The economic event's category, in the values the feed uses, such as `"Inflation"`, shown
    /// above its description, when known.
    pub category: Option<String>,
    /// What the release measures, shown when the user opens the event, when known.
    pub description: Option<String>,
    /// Published figure, formatted by the host, when available.
    pub actual: Option<String>,
    /// Expected figure, formatted by the host, when available.
    pub forecast: Option<String>,
    /// Previous figure, formatted by the host, when available.
    pub prior: Option<String>,
    /// Reported revenue, for earnings.
    pub revenue: Option<String>,
    /// Expected revenue, for earnings.
    pub revenue_forecast: Option<String>,
    /// Company capitalization, for earnings or an IPO.
    pub market_cap: Option<String>,
    /// Reported earnings compared with the estimate, for earnings.
    pub surprise: Option<String>,
    /// Dividend payout relative to the share price, for dividends.
    pub dividend_yield: Option<String>,
    /// Scheduled payment date, for dividends.
    pub payment_date: Option<String>,
    /// Planned capital raise, for an IPO.
    pub deal_amount: Option<String>,
    /// Number of shares offered, for an IPO.
    pub shares_offered: Option<String>,
    /// Listing venue, for an IPO.
    pub exchange: Option<String>,
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
            category: None,
            description: None,
            actual: None,
            forecast: None,
            prior: None,
            revenue: None,
            revenue_forecast: None,
            market_cap: None,
            surprise: None,
            dividend_yield: None,
            payment_date: None,
            deal_amount: None,
            shares_offered: None,
            exchange: None,
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
#[non_exhaustive]
pub enum ScreenerOp {
    /// Greater than the filter's value.
    Above,
    /// Less than the filter's value.
    Below,
}

/// One condition every row must meet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ScreenerFilter {
    /// The [`ScreenerColumn::id`] compared.
    pub column: String,
    /// Comparison applied to the selected column.
    pub op: ScreenerOp,
    /// Comparison threshold.
    pub value: f64,
}

impl ScreenerFilter {
    /// Rows whose `column` compares to `value` as `op` says.
    ///
    /// ```
    /// use egui_charts::data::{ScreenerFilter, ScreenerOp};
    ///
    /// let filter = ScreenerFilter::new("price", ScreenerOp::Above, 10.0);
    /// assert_eq!(filter.column, "price");
    /// ```
    pub fn new(column: impl Into<String>, op: ScreenerOp, value: f64) -> Self {
        Self {
            column: column.into(),
            op,
            value,
        }
    }
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

impl ScreenerQuery {
    /// Up to `limit` rows from the first, in the feed's own order, without a universe,
    /// description, column or filter. Assign [`columns`](Self::columns) for the values each row
    /// answers, and the other fields to describe, filter, sort and page the screen.
    ///
    /// ```
    /// use egui_charts::data::{ScreenerFilter, ScreenerOp, ScreenerQuery};
    ///
    /// let mut query = ScreenerQuery::new(200);
    /// query.columns = vec!["price".into(), "change_percent".into()];
    /// query.filters.push(ScreenerFilter::new("price", ScreenerOp::Above, 10.0));
    /// assert_eq!(query.offset, 0);
    /// ```
    pub fn new(limit: usize) -> Self {
        Self {
            universe: None,
            text: String::new(),
            columns: Vec::new(),
            filters: Vec::new(),
            sort_by: None,
            descending: false,
            offset: 0,
            limit,
        }
    }
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
    /// Only symbols of these exchanges, in the codes the feed uses, or of every exchange when
    /// empty.
    pub exchanges: Vec<String>,
    /// Only symbols of these countries, in the codes the feed uses, or of every country when
    /// empty.
    pub countries: Vec<String>,
}

impl HotlistQuery {
    /// Up to `limit` symbols of the hotlist `list`, of every exchange and country. Assign
    /// [`exchanges`](Self::exchanges) and [`countries`](Self::countries) to narrow it.
    ///
    /// ```
    /// use egui_charts::data::HotlistQuery;
    ///
    /// let query = HotlistQuery::new("Top gainers", 50);
    /// assert_eq!(query.list, "Top gainers");
    /// assert!(query.exchanges.is_empty() && query.countries.is_empty());
    /// ```
    pub fn new(list: impl Into<String>, limit: usize) -> Self {
        Self {
            list: list.into(),
            limit,
            exchanges: Vec::new(),
            countries: Vec::new(),
        }
    }
}

/// One symbol of a hotlist, with its last price, its change since the previous close and
/// optionally the figure the list ranks by.
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
    /// The figure the list ranks by, as its column heading and its value, both formatted by the
    /// host, such as `("Volume", "12.4M")` or `("Gap", "+3.1%")`. The panel shows it as a column
    /// headed by the first entry that has one, and `-` for entries without. `None` by default.
    pub figure: Option<(String, String)>,
}

impl HotlistEntry {
    /// `symbol` last at `last`, `change_percent` from the previous close.
    ///
    /// ```
    /// use egui_charts::data::HotlistEntry;
    ///
    /// let mut entry = HotlistEntry::new("AAPL", 189.5, 2.4);
    /// entry.figure = Some(("Volume".into(), "12.4M".into()));
    /// assert_eq!(entry.price_decimals, None);
    /// ```
    pub fn new(symbol: impl Into<String>, last: f64, change_percent: f64) -> Self {
        Self {
            symbol: symbol.into(),
            last,
            change_percent,
            price_decimals: None,
            figure: None,
        }
    }
}
