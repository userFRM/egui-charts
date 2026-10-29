//! Host-supplied market detail: period returns and seasonality for a symbol, and the earnings,
//! dividends and news the wide watchlist lists. The library asks and shows; the host answers.

use crate::data::{CalendarEvent, NewsStory};

/// What the host has answered for one section, until it answers again.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum MarketDetail<T> {
    /// The request is out.
    Loading,
    /// The answer. An empty one shows that there is nothing to list.
    Ready(T),
    /// The host does not supply this section.
    #[default]
    Unavailable,
    /// The request failed with this message.
    Failed(String),
}

/// A request for the symbol details panel's market detail, from
/// [`SymbolDetailsPanel::take_market_requests`](super::SymbolDetailsPanel::take_market_requests).
/// Answer with [`set_period_returns`](super::SymbolDetailsPanel::set_period_returns) or
/// [`set_seasonality`](super::SymbolDetailsPanel::set_seasonality) for the same symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarketDetailRequest {
    /// The returns of this symbol over the usual look-back periods.
    PeriodReturns(String),
    /// The average return of this symbol in each calendar month.
    Seasonality(String),
}

/// The return over one look-back period.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PeriodReturn {
    /// The period as the host names it, such as `"1M"` or `"YTD"`.
    pub period: String,
    /// The return in percent, or `None` when the history does not cover the period.
    pub percent: Option<f64>,
}

impl PeriodReturn {
    /// The return of `period`, `percent` when known.
    pub fn new(period: impl Into<String>, percent: Option<f64>) -> Self {
        Self {
            period: period.into(),
            percent,
        }
    }
}

/// How one calendar month has performed over the years of history.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SeasonalityMonth {
    /// The month, 1 for January to 12 for December.
    pub month: u8,
    /// The average return of the month in percent, or `None` without history.
    pub average_percent: Option<f64>,
    /// The share of years with a positive return in the month, from 0 to 1.
    pub positive_share: Option<f64>,
}

impl SeasonalityMonth {
    /// The summary of `month`.
    pub fn new(month: u8, average_percent: Option<f64>, positive_share: Option<f64>) -> Self {
        Self {
            month,
            average_percent,
            positive_share,
        }
    }
}

/// The monthly seasonality of a symbol.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Seasonality {
    /// The years of history the figures cover.
    pub years: u32,
    /// The months that have a summary, in calendar order.
    pub months: Vec<SeasonalityMonth>,
}

impl Seasonality {
    /// The summaries of `months` over `years` of history.
    pub fn new(years: u32, months: Vec<SeasonalityMonth>) -> Self {
        Self { years, months }
    }
}

/// The sections of the wide watchlist beyond its table, as the host answered them. The panel
/// lists the rows of the symbols it shows; earnings and dividends are the events of
/// [`CalendarKind::Earnings`](crate::data::CalendarKind::Earnings) and
/// [`CalendarKind::Dividends`](crate::data::CalendarKind::Dividends) with a symbol, and news
/// the stories that name one. By default every section is unavailable.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct WatchlistDetails {
    /// Upcoming and reported earnings.
    pub earnings: MarketDetail<Vec<CalendarEvent>>,
    /// Ex-dividend dates and amounts.
    pub dividends: MarketDetail<Vec<CalendarEvent>>,
    /// Stories about the symbols.
    pub news: MarketDetail<Vec<NewsStory>>,
}
