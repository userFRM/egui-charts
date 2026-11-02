//! Host-supplied option contracts, quotes and risk observations.

use super::options_volume::Snapshot;
use super::{Datafeed, FeedError};
use chrono::{DateTime, NaiveDate, Utc};
use std::ops::RangeInclusive;
use std::sync::{Arc, Weak};

/// Contracts requested for one underlying, live or at a historical instant.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OptionChainQuery {
    /// Underlying symbol as the feed knows it.
    pub symbol: String,
    /// Inclusive expiry dates. Use the same date at both ends for one expiry.
    pub expiries: RangeInclusive<NaiveDate>,
    /// One historical snapshot at this instant, or `None` for continuing live snapshots.
    pub as_of: Option<DateTime<Utc>>,
}

impl OptionChainQuery {
    /// Request live snapshots for `symbol` over the inclusive expiry range.
    pub fn new(symbol: impl Into<String>, expiries: RangeInclusive<NaiveDate>) -> Self {
        Self {
            symbol: symbol.into(),
            expiries,
            as_of: None,
        }
    }
}

/// The right conveyed by an option contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OptionKind {
    /// The right to buy the underlying at the strike.
    Call,
    /// The right to sell the underlying at the strike.
    Put,
}

/// One listed contract's observation. Unknown values stay `None`, including Greeks;
/// consumers must not substitute zero. Prices use the underlying's price units.
/// Greeks describe one unit of the underlying, without the contract multiplier applied.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OptionQuote {
    /// Contract symbol as the feed knows it, distinct from the underlying symbol.
    pub symbol: String,
    /// Contract expiry date.
    pub expiry: NaiveDate,
    /// Positive, finite strike in underlying price units.
    pub strike: f64,
    /// Call or put.
    pub kind: OptionKind,
    /// Positive, finite underlying units per contract, or unknown.
    pub multiplier: Option<f64>,
    /// UTC observation time of these values.
    pub time: DateTime<Utc>,
    /// Nonnegative, finite bid price, or unknown.
    pub bid: Option<f64>,
    /// Nonnegative, finite ask price, or unknown.
    pub ask: Option<f64>,
    /// Nonnegative, finite last traded price, or unknown.
    pub last: Option<f64>,
    /// Nonnegative, finite session volume in contracts, or unknown.
    pub volume: Option<f64>,
    /// Nonnegative, finite open interest in contracts, or unknown.
    pub open_interest: Option<f64>,
    /// Nonnegative, finite implied volatility as a fraction: 0.20 means 20%, or unknown.
    pub implied_volatility: Option<f64>,
    /// Finite delta for one unit of the underlying, or unknown.
    pub delta: Option<f64>,
    /// Finite gamma for one unit of the underlying per one unit of underlying price, or unknown.
    pub gamma: Option<f64>,
    /// Finite theta per calendar day for one unit of the underlying, or unknown.
    pub theta: Option<f64>,
    /// Finite vega per volatility point (one percentage point) for one unit of the underlying,
    /// or unknown.
    pub vega: Option<f64>,
    /// Nonnegative, finite underlying price used to compute these values, or unknown.
    pub underlying_price: Option<f64>,
}

impl OptionQuote {
    /// Identify a contract and its observation time, leaving all optional figures unknown.
    pub fn new(
        symbol: impl Into<String>,
        expiry: NaiveDate,
        strike: f64,
        kind: OptionKind,
        time: DateTime<Utc>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            expiry,
            strike,
            kind,
            multiplier: None,
            time,
            bid: None,
            ask: None,
            last: None,
            volume: None,
            open_interest: None,
            implied_volatility: None,
            delta: None,
            gamma: None,
            theta: None,
            vega: None,
            underlying_price: None,
        }
    }

    fn validate(&self, query: &OptionChainQuery) -> Result<(), FeedError> {
        let invalid = |field| FeedError::InvalidData(format!("contract {}: {field}", self.symbol));
        if !query.expiries.contains(&self.expiry) {
            return Err(invalid("expiry outside query range"));
        }
        for (field, value) in [
            ("strike", Some(self.strike)),
            ("multiplier", self.multiplier),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
                return Err(invalid(field));
            }
        }
        for (field, value) in [
            ("bid", self.bid),
            ("ask", self.ask),
            ("last", self.last),
            ("volume", self.volume),
            ("open_interest", self.open_interest),
            ("implied_volatility", self.implied_volatility),
            ("underlying_price", self.underlying_price),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(invalid(field));
            }
        }
        for (field, value) in [
            ("delta", self.delta),
            ("gamma", self.gamma),
            ("theta", self.theta),
            ("vega", self.vega),
        ] {
            if value.is_some_and(|value| !value.is_finite()) {
                return Err(invalid(field));
            }
        }
        Ok(())
    }
}

struct ChainState {
    query: OptionChainQuery,
    rows: Arc<[OptionQuote]>,
    error: Option<FeedError>,
    finished: bool,
}

/// A feed's destination for complete chain snapshots. Cheap to clone.
/// Only the latest accepted rows and error are retained; updates never queue.
#[derive(Clone)]
pub struct OptionChainSink(Weak<Snapshot<ChainState>>);

impl OptionChainSink {
    /// Replace all rows, including clearing them with an empty snapshot, and wake the host.
    /// Reject the whole snapshot with [`FeedError::InvalidData`] for an invalid field,
    /// expiry outside the query or more than 100,000 rows. The error names the contract and
    /// field, is exposed by [`OptionChain::error`], and leaves the previous rows untouched.
    /// A successful push clears the last error. Inactive sinks ignore pushes.
    pub fn push(&self, rows: Vec<OptionQuote>) -> Result<(), FeedError> {
        let Some(snapshot) = self.0.upgrade() else {
            return Ok(());
        };
        snapshot.update(|state| {
            if state.finished {
                return Ok(());
            }
            let result = if rows.len() > 100_000 {
                Err(FeedError::InvalidData(format!(
                    "contract {}: rows exceeds 100,000",
                    rows[100_000].symbol
                )))
            } else {
                rows.iter().try_for_each(|row| row.validate(&state.query))
            };
            match result {
                Ok(()) => {
                    state.rows = rows.into();
                    state.error = None;
                    state.finished = state.query.as_of.is_some();
                    Ok(())
                }
                Err(error) => {
                    state.error = Some(error.clone());
                    Err(error)
                }
            }
        })
    }

    /// Report a feed failure and wake the host, retaining the last accepted rows.
    /// A historical request ends on a reported error; a live request can recover on a later push.
    pub fn report_error(&self, error: FeedError) {
        if let Some(snapshot) = self.0.upgrade() {
            snapshot.update(|state| {
                if !state.finished {
                    state.error = Some(error);
                    state.finished = state.query.as_of.is_some();
                }
            });
        }
    }

    /// Whether a consumer remains and, for a historical request, still awaits an answer.
    /// Stop producing when false, including after the last handle is dropped.
    pub fn is_active(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|snapshot| snapshot.read(|state| !state.finished))
    }
}

/// The latest option chain for one query, independent of egui.
/// Clones share the subscription; dropping the last handle makes its sink inactive.
#[derive(Clone)]
pub struct OptionChain(Arc<Snapshot<ChainState>>);

impl OptionChain {
    /// Subscribe through `feed`, outside egui closures, and wake the host on each snapshot or
    /// error. Returns `None` for an empty underlying symbol or reversed expiry range.
    /// Unsupported feeds immediately report [`FeedError::NotSupported`] through [`Self::error`].
    pub fn new(
        feed: &dyn Datafeed,
        query: OptionChainQuery,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Option<Self> {
        if query.symbol.is_empty() || query.expiries.is_empty() {
            return None;
        }
        let snapshot = Snapshot::new(
            ChainState {
                query: query.clone(),
                rows: Arc::from([]),
                error: None,
                finished: false,
            },
            wake,
        );
        feed.option_chain(query, OptionChainSink(Arc::downgrade(&snapshot)));
        Some(Self(snapshot))
    }

    /// Latest complete accepted snapshot, in the feed's order. Empty until one arrives.
    /// Missing figures remain `None`; no calculation or aggregation is applied.
    pub fn rows(&self) -> Arc<[OptionQuote]> {
        self.0.read(|state| Arc::clone(&state.rows))
    }

    /// Last feed or validation error, cleared by the next accepted snapshot.
    pub fn error(&self) -> Option<FeedError> {
        self.0.read(|state| state.error.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::inbox::Inbox;
    use crate::data::{HistoryPage, HistoryRequest, Reply};
    use chrono::TimeZone;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Feed(Mutex<Option<(OptionChainQuery, OptionChainSink)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }

        fn option_chain(&self, query: OptionChainQuery, sink: OptionChainSink) {
            *self.0.lock().unwrap() = Some((query, sink));
        }
    }

    fn query() -> OptionChainQuery {
        let start = NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
        let end = NaiveDate::from_ymd_opt(2027, 1, 15).unwrap();
        OptionChainQuery::new("SAMPLE", start..=end)
    }

    fn quote(symbol: &str) -> OptionQuote {
        OptionQuote::new(
            symbol,
            *query().expiries.start(),
            100.0,
            OptionKind::Call,
            Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap(),
        )
    }

    #[test]
    fn invalid_snapshots_name_the_contract_and_field_and_preserve_all_held_rows() {
        let feed = Feed::default();
        let chain = OptionChain::new(&feed, query(), || {}).unwrap();
        let (_, sink) = feed.0.lock().unwrap().clone().unwrap();
        let mut put = quote("PUT");
        put.kind = OptionKind::Put;
        put.expiry = *query().expiries.end();
        sink.push(vec![quote("CALL"), put]).unwrap();
        let held = chain.rows();

        type Field = fn(&mut OptionQuote) -> &mut Option<f64>;
        let mut cases = Vec::new();
        for (field, select, positive, signed) in [
            ("bid", (|r| &mut r.bid) as Field, false, false),
            ("ask", (|r| &mut r.ask) as Field, false, false),
            ("last", (|r| &mut r.last) as Field, false, false),
            ("volume", (|r| &mut r.volume) as Field, false, false),
            (
                "open_interest",
                (|r| &mut r.open_interest) as Field,
                false,
                false,
            ),
            (
                "implied_volatility",
                (|r| &mut r.implied_volatility) as Field,
                false,
                false,
            ),
            (
                "underlying_price",
                (|r| &mut r.underlying_price) as Field,
                false,
                false,
            ),
            ("multiplier", (|r| &mut r.multiplier) as Field, true, false),
            ("delta", (|r| &mut r.delta) as Field, false, true),
            ("gamma", (|r| &mut r.gamma) as Field, false, true),
            ("theta", (|r| &mut r.theta) as Field, false, true),
            ("vega", (|r| &mut r.vega) as Field, false, true),
        ] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
                if value.is_finite() && (signed || (value == 0.0 && !positive)) {
                    continue;
                }
                let mut row = quote("INVALID");
                *select(&mut row) = Some(value);
                cases.push((field, vec![quote("CHANGED"), row]));
            }
        }
        for strike in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
            let mut row = quote("INVALID");
            row.strike = strike;
            cases.push(("strike", vec![quote("CHANGED"), row]));
        }
        for expiry in [
            query().expiries.start().pred_opt().unwrap(),
            query().expiries.end().succ_opt().unwrap(),
        ] {
            let mut row = quote("INVALID");
            row.expiry = expiry;
            cases.push(("expiry", vec![quote("CHANGED"), row]));
        }
        cases.push(("rows", vec![quote("INVALID"); 100_001]));

        for (field, rows) in cases {
            let error = sink.push(rows).unwrap_err();
            assert!(
                matches!(&error, FeedError::InvalidData(message)
                    if message.contains("INVALID") && message.contains(field)),
                "{field}: {error:?}"
            );
            assert_eq!(chain.error(), Some(error));
            assert!(Arc::ptr_eq(&chain.rows(), &held), "{field}");
        }
        sink.push((0..100_000).map(|i| quote(&format!("CALL-{i}"))).collect())
            .unwrap();
        assert_eq!(chain.rows().len(), 100_000);
        assert_eq!(chain.error(), None);
    }

    #[test]
    fn snapshots_replace_without_filling_missing_figures_and_end_with_the_last_consumer() {
        for historical in [false, true] {
            let feed = Feed::default();
            let mut query = query();
            query.as_of = historical.then_some(quote("CALL").time);
            let wakes = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&wakes);
            let chain = OptionChain::new(&feed, query.clone(), move || {
                counter.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();
            let (received, sink) = feed.0.lock().unwrap().clone().unwrap();
            assert_eq!(received, query);
            assert!(chain.rows().is_empty());
            assert_eq!(chain.error(), None);
            assert!(sink.is_active());
            let clone = chain.clone();

            sink.report_error(FeedError::Network("offline".into()));
            assert_eq!(chain.error(), Some(FeedError::Network("offline".into())));
            if historical {
                assert!(!sink.is_active());
                sink.push(vec![quote("LATE")]).unwrap();
                assert!(chain.rows().is_empty());
                continue;
            }
            for interest in [0.0, 1200.0] {
                let mut row = quote("CALL");
                row.bid = Some(0.0);
                row.open_interest = Some(interest);
                row.gamma = Some(-0.04);
                row.theta = Some(-0.5);
                sink.push(vec![row.clone()]).unwrap();
                assert_eq!(chain.rows().as_ref(), [row]);
                let rows = chain.rows();
                assert!(
                    [
                        rows[0].multiplier,
                        rows[0].ask,
                        rows[0].last,
                        rows[0].volume,
                        rows[0].implied_volatility,
                        rows[0].delta,
                        rows[0].vega,
                        rows[0].underlying_price,
                    ]
                    .iter()
                    .all(Option::is_none)
                );
                assert_eq!(clone.rows(), chain.rows());
                assert_eq!(chain.error(), None);
                assert!(sink.is_active());
            }
            sink.push(Vec::new()).unwrap();
            assert!(chain.rows().is_empty());
            assert_eq!(wakes.load(Ordering::Relaxed), 4);
            drop(chain);
            assert!(sink.is_active());
            drop(clone);
            assert!(!sink.is_active());
            let before = wakes.load(Ordering::Relaxed);
            sink.push(vec![quote("LATE")]).unwrap();
            sink.report_error(FeedError::NotSupported);
            assert_eq!(wakes.load(Ordering::Relaxed), before);
        }

        let feed = Feed::default();
        let mut query = query();
        query.as_of = Some(quote("CALL").time);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let (_, sink) = feed.0.lock().unwrap().clone().unwrap();
        sink.push(vec![quote("CALL")]).unwrap();
        let held = chain.rows();
        assert!(!sink.is_active());
        sink.push(Vec::new()).unwrap();
        sink.report_error(FeedError::NotSupported);
        assert!(Arc::ptr_eq(&chain.rows(), &held));
        assert_eq!(chain.error(), None);
    }

    #[test]
    fn history_only_feeds_report_unsupported_options_and_bad_queries_never_subscribe() {
        struct HistoryOnly;
        impl Datafeed for HistoryOnly {
            fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
                reply.send(Ok(HistoryPage::default()));
            }
        }

        let inbox = Arc::new(Inbox::default());
        HistoryOnly.option_expiries("SAMPLE", inbox.reply(0));
        let answers = inbox.drain().answers;
        assert_eq!(answers.len(), 1);
        assert_eq!(
            *answers
                .into_iter()
                .next()
                .unwrap()
                .1
                .downcast::<Result<Vec<NaiveDate>, FeedError>>()
                .unwrap(),
            Err(FeedError::NotSupported)
        );
        for historical in [false, true] {
            let mut query = query();
            query.as_of = historical.then_some(quote("CALL").time);
            let chain = OptionChain::new(&HistoryOnly, query, || {}).unwrap();
            assert!(chain.rows().is_empty());
            assert_eq!(chain.error(), Some(FeedError::NotSupported));
        }

        let feed = Feed::default();
        let mut empty_symbol = query();
        empty_symbol.symbol.clear();
        let mut reversed = query();
        reversed.expiries = *reversed.expiries.end()..=*reversed.expiries.start();
        for query in [empty_symbol, reversed] {
            assert!(OptionChain::new(&feed, query, || {}).is_none());
        }
        assert!(feed.0.lock().unwrap().is_none());
    }
}
