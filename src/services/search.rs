//! Symbol search.

mod expression;

pub use expression::{ExpressionError, SymbolExpression};

use crate::data::{MaybeSend, Reply};
use crate::model::{Instrument, SymbolType};

/// Finds instruments for the symbol search dialog, from a source that need not be the feed's.
///
/// The terminal calls [`search`](Self::search) on the UI thread as the user types, and shows the
/// answer to the newest query only. Return at once and answer once through `reply`, from any
/// thread, runtime or browser task; an answer to a query the user has since changed is ignored,
/// so there is nothing to cancel. The symbol the user picks is loaded from the
/// [`Datafeed`](crate::data::Datafeed), so answer with [`symbol`](Instrument::symbol)s as the feed
/// knows them. A fixed list of instruments is a service on its own: see the
/// implementation for `Vec<Instrument>`.
///
/// ```
/// use egui_charts::data::{FeedError, Reply};
/// use egui_charts::model::Instrument;
/// use egui_charts::services::{SymbolQuery, SymbolSearch};
///
/// /// Asks a reference-data service from a worker thread.
/// struct ReferenceData;
///
/// impl SymbolSearch for ReferenceData {
///     fn search(&self, query: SymbolQuery, reply: Reply<Vec<Instrument>>) {
///         std::thread::spawn(move || reply.send(lookup(&query)));
///     }
/// }
///
/// /// Stands in for the request to the service.
/// fn lookup(query: &SymbolQuery) -> Result<Vec<Instrument>, FeedError> {
///     let apple = Instrument::new("AAPL")
///         .with_description("Apple Inc.")
///         .with_exchange("NASDAQ");
///     Ok([apple].into_iter().skip(query.offset).take(query.limit).collect())
/// }
/// ```
pub trait SymbolSearch: MaybeSend + 'static {
    /// Up to `query.limit` instruments matching `query`, best first.
    fn search(&self, query: SymbolQuery, reply: Reply<Vec<Instrument>>);
}

/// What the user typed into the symbol search dialog, and the filters chosen there.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SymbolQuery {
    /// The typed text, as typed. Empty asks for any instrument that passes the filters.
    pub text: String,
    /// Only instruments of this type, when set.
    pub symbol_type: Option<SymbolType>,
    /// Only instruments of this exchange, when set.
    pub exchange: Option<String>,
    /// Listing country, as an ISO 3166-1 alpha-2 code, when set.
    pub country: Option<String>,
    /// Company sector, when set.
    pub sector: Option<String>,
    /// Data provider, independent of the exchange, when set.
    pub source: Option<String>,
    /// Matching results to skip, for the next page.
    pub offset: usize,
    /// Results wanted at most.
    pub limit: usize,
}

impl SymbolQuery {
    /// An unfiltered query with a page size of 20. Set filters and pagination before searching.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            symbol_type: None,
            exchange: None,
            country: None,
            sector: None,
            source: None,
            offset: 0,
            limit: 20,
        }
    }

    /// The venue and remaining text in a `VENUE:SYMBOL` query, including an empty symbol while
    /// typing. The explicit exchange filter stays independent. Arithmetic expressions are
    /// resolved separately by the chooser, rather than treated as one qualified symbol.
    pub fn qualified(&self) -> Option<(&str, &str)> {
        let (venue, symbol) = self.text.trim().split_once(':')?;
        let (venue, symbol) = (venue.trim(), symbol.trim());
        (!venue.is_empty()
            && !venue.contains(char::is_whitespace)
            && !venue.contains(['+', '-', '*', '/', '(', ')'])
            && !symbol.contains([':', '+', '*', '/', '(', ')']))
        .then_some((venue, symbol))
    }

    /// Whether no text or filters have been entered. Used for the initial recent-symbol list.
    #[cfg(feature = "ui")]
    pub(crate) fn unfiltered(&self) -> bool {
        self.text.is_empty()
            && self.symbol_type.is_none()
            && self.exchange.is_none()
            && self.country.is_none()
            && self.sector.is_none()
            && self.source.is_none()
    }

    #[cfg(feature = "http")]
    fn http_params(&self) -> Vec<(&'static str, String)> {
        let mut params = vec![
            ("query", self.text.clone()),
            (
                "type",
                self.symbol_type
                    .map(|kind| kind.to_string())
                    .unwrap_or_default(),
            ),
            ("exchange", self.exchange.clone().unwrap_or_default()),
            ("offset", self.offset.to_string()),
            ("limit", self.limit.to_string()),
        ];
        for (name, value) in [
            ("country", &self.country),
            ("sector", &self.sector),
            ("source", &self.source),
        ] {
            if let Some(value) = value {
                params.push((name, value.clone()));
            }
        }
        params
    }

    fn matches_filters(&self, instrument: &Instrument) -> bool {
        let listing = instrument.symbol.split_once(':');
        let exchange = instrument
            .exchange
            .as_deref()
            .or(listing.map(|(venue, _)| venue));
        let matches = |wanted: Option<&str>, actual: Option<&str>| {
            wanted.is_none_or(|wanted| {
                actual.is_some_and(|actual| actual.eq_ignore_ascii_case(wanted))
            })
        };
        let qualified = self.qualified();
        if self.symbol_type.is_some() && self.symbol_type != instrument.symbol_type
            || !matches(self.exchange.as_deref(), exchange)
            || !(matches(qualified.map(|(venue, _)| venue), exchange)
                || matches(
                    qualified.map(|(venue, _)| venue),
                    listing.map(|(venue, _)| venue),
                ))
            || !matches(self.country.as_deref(), instrument.country.as_deref())
            || !matches(self.sector.as_deref(), instrument.sector.as_deref())
            || !matches(self.source.as_deref(), instrument.source.as_deref())
        {
            return false;
        }
        true
    }

    /// Match and rank one listing before pagination.
    pub(crate) fn rank(&self, instrument: &Instrument) -> Option<u8> {
        if !self.matches_filters(instrument) {
            return None;
        }
        let qualified = self.qualified();
        let listing = instrument.symbol.split_once(':');
        let text = qualified
            .map_or(self.text.trim(), |(_, symbol)| symbol)
            .to_lowercase();
        let symbol = listing
            .map_or(instrument.symbol.as_str(), |(_, symbol)| symbol)
            .to_lowercase();
        if symbol == text {
            Some(0)
        } else if symbol.starts_with(&text) {
            Some(1)
        } else if symbol.contains(&text)
            || instrument
                .description
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&text)
        {
            Some(2)
        } else {
            None
        }
    }

    /// Keep eligible members of a matching root available for expansion, even when their own
    /// ticker or description does not contain the query. Direct matches remain ahead of them.
    pub(crate) fn rank_in(&self, instrument: &Instrument, catalog: &[Instrument]) -> Option<u8> {
        if let Some(rank) = self.rank(instrument) {
            return Some(rank);
        }
        let group = instrument.search_group()?;
        (self.matches_filters(instrument)
            && catalog
                .iter()
                .any(|parent| parent.search_group() == Some(group) && self.rank(parent).is_some()))
        .then_some(3)
    }
}

#[cfg(feature = "http")]
impl SymbolSearch for crate::data::http::HttpDatafeed {
    fn search(&self, query: SymbolQuery, reply: Reply<Vec<Instrument>>) {
        self.search_symbols(&query.http_params(), query.limit, reply);
    }
}

/// A fixed list, answered inline: the instruments whose symbol or description contains the text,
/// ignoring case, and whose metadata passes every chosen filter. Qualified queries also constrain
/// the listing venue without replacing the explicit exchange filter. An instrument whose
/// symbol equals the text comes first, then those whose symbol starts with it, then the rest, each
/// group in list order. Eligible members of a matching root follow direct matches, so an exact
/// continuous-contract query still offers its dated contracts.
impl SymbolSearch for Vec<Instrument> {
    fn search(&self, query: SymbolQuery, reply: Reply<Vec<Instrument>>) {
        let mut found: Vec<_> = self
            .iter()
            .filter_map(|instrument| {
                query
                    .rank_in(instrument, self)
                    .map(|rank| (rank, instrument))
            })
            .collect();
        found.sort_by_key(|&(rank, _)| rank);
        let found = found.into_iter().skip(query.offset).take(query.limit);
        reply.send(Ok(found
            .map(|(_, instrument)| instrument.clone())
            .collect()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::FeedError;
    use crate::data::inbox::Inbox;
    use std::sync::Arc;

    /// Guards the fallback's contract with the dialog: matching ignores case and reaches the
    /// description, each filter excludes what it should (also an instrument without the field),
    /// an exact symbol outranks a longer one listed first, a prefix outranks a match elsewhere
    /// listed first, and pagination cuts after ranking.
    #[test]
    fn list_search_matches_and_filters() {
        let list = vec![
            Instrument::new("BTCUSD").with_symbol_type(SymbolType::Crypto),
            Instrument::new("X:DUAL").with_exchange("Venue X"),
            Instrument::new("SPYG")
                .with_description("SPDR Portfolio S&P 500 Growth ETF")
                .with_symbol_type(SymbolType::Fund)
                .with_exchange("NYSE")
                .with_country("US")
                .with_sector("Financials")
                .with_source("Primary"),
            Instrument::new("SPY")
                .with_description("SPDR S&P 500 ETF Trust")
                .with_symbol_type(SymbolType::Fund)
                .with_exchange("NYSE"),
            Instrument::new("AAPL")
                .with_description("Apple Inc.")
                .with_symbol_type(SymbolType::Stock)
                .with_exchange("NASDAQ")
                .with_country("US")
                .with_sector("Technology")
                .with_source("Primary"),
        ];
        let query = |text: &str, symbol_type, exchange: Option<&str>, offset, limit| SymbolQuery {
            text: text.into(),
            symbol_type,
            exchange: exchange.map(Into::into),
            country: None,
            sector: None,
            source: None,
            limit,
            offset,
        };
        let mut cases = vec![
            (query("nasdaq:aapl", None, None, 0, 10), &["AAPL"][..]),
            (query("aapl", None, None, 0, 10), &["AAPL"]),
            (query("NASDAQ:", None, None, 0, 10), &["AAPL"]),
            (query("NASDAQ:AAPL", None, Some("NYSE"), 0, 10), &[]),
            (query("x:dual", None, None, 0, 10), &["X:DUAL"]),
            (query("x:dual", None, Some("Venue X"), 0, 10), &["X:DUAL"]),
            (query(" :aapl", None, None, 0, 10), &[]),
            (query("APPLE", None, None, 0, 10), &["AAPL"]),
            (query("s&p 500", None, None, 0, 10), &["SPYG", "SPY"]),
            (query("s&p 500", None, None, 1, 10), &["SPY"]),
            (query("spy", None, None, 2, 10), &[]),
            (query("spy", None, None, 0, 10), &["SPY", "SPYG"]),
            (query("spy", None, None, 0, 1), &["SPY"]),
            (
                query("", Some(SymbolType::Crypto), None, 0, 10),
                &["BTCUSD"],
            ),
            (query("", None, Some("NASDAQ"), 0, 10), &["AAPL"]),
            (query("s", None, None, 0, 10), &["SPYG", "SPY", "BTCUSD"]),
            (query("xyz", None, None, 0, 10), &[]),
        ];
        for (country, sector, source, expected) in [
            (Some("us"), None, None, &["SPYG", "AAPL"][..]),
            (Some("GB"), None, None, &[]),
            (None, Some("technology"), None, &["AAPL"]),
            (None, None, Some("primary"), &["SPYG", "AAPL"]),
            (Some("US"), Some("Financials"), Some("Primary"), &["SPYG"]),
            (Some("US"), Some("Technology"), Some("Other"), &[]),
        ] {
            let mut filtered = query("", None, None, 0, 10);
            filtered.country = country.map(Into::into);
            filtered.sector = sector.map(Into::into);
            filtered.source = source.map(Into::into);
            cases.push((filtered, expected));
        }
        let mut page = query("", None, None, 1, 1);
        page.country = Some("US".into());
        cases.push((page, &["AAPL"]));
        for (query, expected) in cases {
            let inbox = Arc::new(Inbox::default());
            list.search(query.clone(), inbox.reply(0));
            let answers = inbox.drain().answers;
            assert_eq!(answers.len(), 1, "{query:?} not answered inline");
            let (_, answer) = answers.into_iter().next().unwrap();
            let found = answer
                .downcast::<Result<Vec<Instrument>, FeedError>>()
                .unwrap();
            let symbols: Vec<_> = found.unwrap().into_iter().map(|i| i.symbol).collect();
            assert_eq!(symbols, expected, "{query:?}");
        }
    }
}
