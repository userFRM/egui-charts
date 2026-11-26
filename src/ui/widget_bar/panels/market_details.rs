//! Host-supplied market detail: period returns, seasonality and the listed detail sections of a
//! symbol, and the earnings, dividends and news the wide watchlist lists. The library asks and
//! shows; the host answers.

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
/// [`SymbolDetailsPanel::take_market_requests`](crate::ui::widget_bar::SymbolDetailsPanel::take_market_requests).
/// Answer with [`set_period_returns`](crate::ui::widget_bar::SymbolDetailsPanel::set_period_returns),
/// [`set_seasonality`](crate::ui::widget_bar::SymbolDetailsPanel::set_seasonality) or
/// [`set_detail_rows`](crate::ui::widget_bar::SymbolDetailsPanel::set_detail_rows) for the same
/// symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarketDetailRequest {
    /// The returns of this symbol over the usual look-back periods.
    PeriodReturns(String),
    /// The average return of this symbol in each calendar month.
    Seasonality(String),
    /// The rows of this section for this symbol, asked for only when the host offers the section.
    Section(DetailSection, String),
}

/// A section of the symbol details panel that lists the [`DetailRow`]s the host writes, such as
/// analyst estimates or a company's filings. Offer the ones the host answers with
/// [`SymbolDetailsPanel::set_detail_sections`](crate::ui::widget_bar::SymbolDetailsPanel::set_detail_sections).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DetailSection {
    /// Analyst ratings, price targets and estimates, titled "Analyst estimates".
    AnalystEstimates,
    /// Options figures such as implied volatility and the put/call ratio, titled "Options".
    Options,
    /// Recordings of earnings calls, titled "Earnings calls".
    EarningsCalls,
    /// Filings, reports and other company documents, titled "Documents".
    Documents,
    /// The funds that hold the symbol, or the holdings of a fund, titled "Fund holdings".
    FundHoldings,
    /// The issuer's debt credit ratings, titled "Credit ratings".
    CreditRatings,
    /// The bonds the issuer has outstanding, titled "Bonds".
    Bonds,
}

impl DetailSection {
    /// Every section, in panel order.
    pub const ALL: [Self; 7] = [
        Self::AnalystEstimates,
        Self::Options,
        Self::EarningsCalls,
        Self::Documents,
        Self::FundHoldings,
        Self::CreditRatings,
        Self::Bonds,
    ];

    /// The section's title, its name in the Layout menu and in
    /// [`hidden_sections`](crate::ui::widget_bar::SymbolDetailsPanel::hidden_sections).
    pub const fn title(self) -> &'static str {
        match self {
            Self::AnalystEstimates => "Analyst estimates",
            Self::Options => "Options",
            Self::EarningsCalls => "Earnings calls",
            Self::Documents => "Documents",
            Self::FundHoldings => "Fund holdings",
            Self::CreditRatings => "Credit ratings",
            Self::Bonds => "Bonds",
        }
    }
}

/// One row of a [`DetailSection`], as the host writes it.
///
/// ```
/// use egui_charts::ui::widget_bar::DetailRow;
/// let report = DetailRow::new("Annual report 2025", "PDF")
///     .with_note("Filed Mar 2, 2026")
///     .with_url("https://example.com/reports/2025.pdf");
/// assert_eq!(report.url.as_deref(), Some("https://example.com/reports/2025.pdf"));
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct DetailRow {
    /// The name on the left, such as a filing, a fund, a rating agency or a bond.
    pub label: String,
    /// The text on the right, such as `"4.2%"`, `"AA-"` or `"Buy"`; empty for none.
    pub value: String,
    /// A smaller line under the label, such as a date or an outlook; empty for none.
    pub note: String,
    /// The document, recording or page the row opens. A row with one is a link, and choosing it
    /// is reported by
    /// [`take_opened_row`](crate::ui::widget_bar::SymbolDetailsPanel::take_opened_row).
    pub url: Option<String>,
}

impl DetailRow {
    /// A row with `label` on the left and `value` on the right.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    /// Adds `note` under the label.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }

    /// Makes the row open `url` through the host.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
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
