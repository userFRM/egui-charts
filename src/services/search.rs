//! Symbol search.

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
    /// Matching results to skip, for the next page.
    pub offset: usize,
    /// Results wanted at most.
    pub limit: usize,
}

/// A fixed list, answered inline: the instruments whose symbol or description contains the text,
/// ignoring case, and whose type and exchange equal the filters that are set. An instrument whose
/// symbol equals the text comes first, then those whose symbol starts with it, then the rest, each
/// group in list order.
impl SymbolSearch for Vec<Instrument> {
    fn search(&self, query: SymbolQuery, reply: Reply<Vec<Instrument>>) {
        let text = query.text.to_lowercase();
        let mut found: Vec<_> = self
            .iter()
            .filter(|instrument| {
                (query.symbol_type.is_none() || query.symbol_type == instrument.symbol_type)
                    && (query.exchange.is_none() || query.exchange == instrument.exchange)
            })
            .filter_map(|instrument| {
                let symbol = instrument.symbol.to_lowercase();
                let described = || {
                    let description = instrument.description.as_deref().unwrap_or_default();
                    description.to_lowercase().contains(&text)
                };
                let rank = if symbol == text {
                    0
                } else if symbol.starts_with(&text) {
                    1
                } else if symbol.contains(&text) || described() {
                    2
                } else {
                    return None;
                };
                Some((rank, instrument))
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
            Instrument::new("SPYG")
                .with_description("SPDR Portfolio S&P 500 Growth ETF")
                .with_symbol_type(SymbolType::Fund)
                .with_exchange("NYSE"),
            Instrument::new("SPY")
                .with_description("SPDR S&P 500 ETF Trust")
                .with_symbol_type(SymbolType::Fund)
                .with_exchange("NYSE"),
            Instrument::new("AAPL")
                .with_description("Apple Inc.")
                .with_symbol_type(SymbolType::Stock)
                .with_exchange("NASDAQ"),
        ];
        let query = |text: &str, symbol_type, exchange: Option<&str>, offset, limit| SymbolQuery {
            text: text.into(),
            symbol_type,
            exchange: exchange.map(Into::into),
            limit,
            offset,
        };
        let cases = [
            (query("aapl", None, None, 0, 10), &["AAPL"][..]),
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
