//! Host-supplied option contracts, quotes, risk observations and exposure snapshots.

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
        let result = snapshot.update(|state| {
            if state.finished {
                return None;
            }
            let result = if rows.len() > 100_000 {
                Err(FeedError::InvalidData(format!(
                    "contract {}: rows exceeds 100,000",
                    rows[100_000].symbol
                )))
            } else {
                rows.iter().try_for_each(|row| row.validate(&state.query))
            };
            Some(match result {
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
            })
        });
        result.unwrap_or(Ok(()))
    }

    /// Report a feed failure and wake the host, retaining the last accepted rows.
    /// A historical request ends on a reported error; a live request can recover on a later push.
    pub fn report_error(&self, error: FeedError) {
        if let Some(snapshot) = self.0.upgrade() {
            snapshot.update(|state| {
                if state.finished {
                    return None;
                }
                state.error = Some(error);
                state.finished = state.query.as_of.is_some();
                Some(())
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

    /// The underlying, expiry range and historical instant of this subscription.
    pub fn query(&self) -> OptionChainQuery {
        self.0.read(|state| state.query.clone())
    }

    /// Last feed or validation error, cleared by the next accepted snapshot.
    pub fn error(&self) -> Option<FeedError> {
        self.0.read(|state| state.error.clone())
    }
}

/// The sensitivity described by an exposure snapshot. Only gamma is computed locally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OptionExposureKind {
    /// Exposure to a change in the underlying price through gamma.
    Gamma,
    /// Exposure through delta.
    Delta,
    /// Exposure to changes in volatility through vanna.
    Vanna,
    /// Exposure to the passage of time through charm.
    Charm,
}

/// Exposure requested for one underlying and inclusive expiry range.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OptionExposureQuery {
    /// Underlying symbol as the feed knows it.
    pub symbol: String,
    /// Inclusive expiry dates. Use the same date at both ends for one expiry.
    pub expiries: RangeInclusive<NaiveDate>,
    /// Requested sensitivity.
    pub kind: OptionExposureKind,
    /// One historical snapshot at this instant, or `None` for continuing live snapshots.
    pub as_of: Option<DateTime<Utc>>,
}

impl OptionExposureQuery {
    /// Request live exposure snapshots for `symbol` over the inclusive expiry range.
    pub fn new(
        symbol: impl Into<String>,
        expiries: RangeInclusive<NaiveDate>,
        kind: OptionExposureKind,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            expiries,
            kind,
            as_of: None,
        }
    }
}

/// Exposure at one expiry and strike, in the snapshot's units. Unknown sides stay absent.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OptionExposureRow {
    /// Contract expiry date; different expiries are never combined.
    pub expiry: NaiveDate,
    /// Positive, finite strike in underlying price units.
    pub strike: f64,
    /// Finite call exposure, or unknown.
    pub calls: Option<f64>,
    /// Finite put exposure, or unknown.
    pub puts: Option<f64>,
    /// Finite net exposure, or unknown. Supplied values need not equal calls plus puts.
    pub net: Option<f64>,
}

impl OptionExposureRow {
    /// Identify an expiry and strike, leaving all exposure figures unknown.
    pub fn new(expiry: NaiveDate, strike: f64) -> Self {
        Self {
            expiry,
            strike,
            calls: None,
            puts: None,
            net: None,
        }
    }
}

/// A complete exposure observation, supplied by a feed or computed from an option chain.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OptionExposure {
    /// Sensitivity represented by these values.
    pub kind: OptionExposureKind,
    /// UTC observation time. Computed gamma uses the newest input observation.
    pub time: DateTime<Utc>,
    /// Display units, supplied by the feed or documented by [`Self::gamma`].
    pub unit: String,
    /// Optional name of the feed's valuation or position model.
    pub model: Option<String>,
    /// Exposure by expiry and strike; supplied rows keep their order and figures.
    pub rows: Vec<OptionExposureRow>,
    /// Contracts omitted because an input was unknown. Zero open interest is not missing.
    pub missing_contracts: usize,
    /// Finite underlying price where the provider's model has zero gamma.
    /// Replaced with every snapshot; never computed from the listed strike exposures.
    pub zero_gamma: Option<f64>,
}

impl OptionExposure {
    /// An observation with no model label, zero-gamma level or missing contracts.
    pub fn new(
        kind: OptionExposureKind,
        time: DateTime<Utc>,
        unit: impl Into<String>,
        rows: Vec<OptionExposureRow>,
    ) -> Self {
        Self {
            kind,
            time,
            unit: unit.into(),
            model: None,
            rows,
            missing_contracts: 0,
            zero_gamma: None,
        }
    }

    /// Compute `gamma * open_interest * multiplier * underlying_price² * 0.01` per contract,
    /// in the underlying's price currency per 1% underlying move. Calls use a positive sign
    /// and puts a negative sign: this is a convention, not an inference about positions.
    /// The quotes must belong to one underlying and currency, with unscaled contract Greeks.
    ///
    /// Missing inputs omit and count the contract; zero open interest contributes `Some(0)`.
    /// Rows sort by expiry then strike, summing each side and taking their sum as net.
    /// A side with no usable contracts stays `None`. Returns `Ok(None)` without observations,
    /// or [`FeedError::InvalidData`] for invalid inputs or arithmetic overflow.
    pub fn gamma<'a>(
        quotes: impl IntoIterator<Item = &'a OptionQuote>,
    ) -> Result<Option<Self>, FeedError> {
        let mut time = None;
        let mut missing_contracts = 0;
        let mut contributions = Vec::new();
        for quote in quotes {
            time = Some(time.map_or(quote.time, |time: DateTime<Utc>| time.max(quote.time)));
            let invalid =
                |field| FeedError::InvalidData(format!("contract {}: {field}", quote.symbol));
            if !quote.strike.is_finite() || quote.strike <= 0.0 {
                return Err(invalid("strike"));
            }
            let (Some(gamma), Some(interest), Some(multiplier), Some(price)) = (
                quote.gamma,
                quote.open_interest,
                quote.multiplier,
                quote.underlying_price,
            ) else {
                missing_contracts += 1;
                continue;
            };
            for (field, value, invalid_sign) in [
                ("gamma", gamma, false),
                ("open_interest", interest, interest < 0.0),
                ("multiplier", multiplier, multiplier <= 0.0),
                ("underlying_price", price, price < 0.0),
            ] {
                if !value.is_finite() || invalid_sign {
                    return Err(invalid(field));
                }
            }
            let value = if gamma == 0.0 || interest == 0.0 || price == 0.0 {
                0.0
            } else {
                gamma * interest * multiplier * price * price * 0.01
            };
            if !value.is_finite() {
                return Err(invalid("gamma exposure overflow"));
            }
            let mut row = OptionExposureRow::new(quote.expiry, quote.strike);
            match quote.kind {
                OptionKind::Call => row.calls = Some(value),
                OptionKind::Put => row.puts = Some(-value),
            }
            contributions.push(row);
        }
        let Some(time) = time else {
            return Ok(None);
        };
        contributions.sort_by(|a, b| {
            a.expiry
                .cmp(&b.expiry)
                .then_with(|| a.strike.total_cmp(&b.strike))
        });
        let mut rows: Vec<OptionExposureRow> = Vec::new();
        for contribution in contributions {
            if let Some(row) = rows.last_mut()
                && row.expiry == contribution.expiry
                && row.strike == contribution.strike
            {
                if let Some(value) = contribution.calls {
                    row.calls = Some(row.calls.unwrap_or(0.0) + value);
                }
                if let Some(value) = contribution.puts {
                    row.puts = Some(row.puts.unwrap_or(0.0) + value);
                }
            } else {
                rows.push(contribution);
            }
            let row = rows.last_mut().unwrap();
            row.net = Some(row.calls.unwrap_or(0.0) + row.puts.unwrap_or(0.0));
            if [row.calls, row.puts, row.net]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(FeedError::InvalidData(format!(
                    "exposure {} at {}: sum overflow",
                    row.expiry, row.strike
                )));
            }
        }
        let mut exposure = Self::new(
            OptionExposureKind::Gamma,
            time,
            "price currency per 1% move",
            rows,
        );
        exposure.missing_contracts = missing_contracts;
        Ok(Some(exposure))
    }

    fn validate(&self, query: &OptionExposureQuery) -> Result<(), FeedError> {
        if self.kind != query.kind {
            return Err(FeedError::InvalidData(
                "exposure kind differs from query".into(),
            ));
        }
        if self.zero_gamma.is_some_and(|price| !price.is_finite()) {
            return Err(FeedError::InvalidData("zero gamma price".into()));
        }
        if self.rows.len() > 100_000 {
            return Err(FeedError::InvalidData(
                "exposure rows exceeds 100,000".into(),
            ));
        }
        for row in &self.rows {
            let invalid = |field| {
                FeedError::InvalidData(format!(
                    "exposure {} at {}: {field}",
                    row.expiry, row.strike
                ))
            };
            if !query.expiries.contains(&row.expiry) {
                return Err(invalid("expiry outside query range"));
            }
            if !row.strike.is_finite() || row.strike <= 0.0 {
                return Err(invalid("strike"));
            }
            for (field, value) in [("calls", row.calls), ("puts", row.puts), ("net", row.net)] {
                if value.is_some_and(|value| !value.is_finite()) {
                    return Err(invalid(field));
                }
            }
        }
        Ok(())
    }
}

struct ExposureState {
    query: OptionExposureQuery,
    value: Option<Arc<OptionExposure>>,
    error: Option<FeedError>,
    finished: bool,
}

/// A feed's destination for complete exposure snapshots. Cheap to clone.
/// Updates replace the latest snapshot without queuing or adding to computed gamma.
#[derive(Clone)]
pub struct OptionExposureSink(Weak<Snapshot<ExposureState>>);

impl OptionExposureSink {
    /// Replace the held snapshot and wake the host. Refuse the whole snapshot for a nonfinite
    /// figure, invalid strike or zero-gamma price, expiry outside the query, different kind
    /// or over 100,000 rows.
    /// Report [`FeedError::InvalidData`] without changing the previous snapshot; an accepted
    /// push clears the error. Inactive sinks ignore pushes.
    pub fn push(&self, value: OptionExposure) -> Result<(), FeedError> {
        let Some(snapshot) = self.0.upgrade() else {
            return Ok(());
        };
        snapshot
            .update(|state| {
                if state.finished {
                    return None;
                }
                Some(match value.validate(&state.query) {
                    Ok(()) => {
                        state.value = Some(Arc::new(value));
                        state.error = None;
                        state.finished = state.query.as_of.is_some();
                        Ok(())
                    }
                    Err(error) => {
                        state.error = Some(error.clone());
                        Err(error)
                    }
                })
            })
            .unwrap_or(Ok(()))
    }

    /// Report a failure, retaining the last accepted snapshot, and wake the host.
    /// Historical requests finish on this error; live requests can recover on a later push.
    pub fn report_error(&self, error: FeedError) {
        if let Some(snapshot) = self.0.upgrade() {
            snapshot.update(|state| {
                if state.finished {
                    return None;
                }
                state.error = Some(error);
                state.finished = state.query.as_of.is_some();
                Some(())
            });
        }
    }

    /// Whether a consumer remains and, for a historical request, still awaits an answer.
    pub fn is_active(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|snapshot| snapshot.read(|state| !state.finished))
    }
}

/// The latest exposure for one query, independent of egui. Clones share the subscription;
/// dropping the last handle makes its sink inactive.
#[derive(Clone)]
pub struct OptionExposureHandle(Arc<Snapshot<ExposureState>>);

impl OptionExposureHandle {
    /// Subscribe outside egui closures, waking the host on every snapshot or error.
    /// Returns `None` for an empty symbol or reversed expiry range. Unsupported feeds report
    /// [`FeedError::NotSupported`] through [`Self::error`], leaving gamma fallback available.
    pub fn new(
        feed: &dyn Datafeed,
        query: OptionExposureQuery,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Option<Self> {
        if query.symbol.is_empty() || query.expiries.is_empty() {
            return None;
        }
        let snapshot = Snapshot::new(
            ExposureState {
                query: query.clone(),
                value: None,
                error: None,
                finished: false,
            },
            wake,
        );
        feed.option_exposure(query, OptionExposureSink(Arc::downgrade(&snapshot)));
        Some(Self(snapshot))
    }

    /// The symbol, expiry range, sensitivity and observation instant of this subscription.
    pub fn query(&self) -> OptionExposureQuery {
        self.0.read(|state| state.query.clone())
    }

    /// Latest accepted snapshot, exactly as delivered, or `None` until one arrives.
    pub fn snapshot(&self) -> Option<Arc<OptionExposure>> {
        self.0.read(|state| state.value.clone())
    }

    /// Prefer a supplied snapshot, including one with no rows, over computed gamma.
    /// Before one arrives, compute gamma from `quotes` within the query's expiry range.
    /// Supply quotes for this underlying and, for historical requests, the requested instant.
    /// Other exposure kinds have no local fallback. Never adds computed and supplied values.
    pub fn snapshot_or_gamma(
        &self,
        quotes: &[OptionQuote],
    ) -> Result<Option<Arc<OptionExposure>>, FeedError> {
        let (value, query) = self
            .0
            .read(|state| (state.value.clone(), state.query.clone()));
        if value.is_some() || query.kind != OptionExposureKind::Gamma {
            return Ok(value);
        }
        OptionExposure::gamma(
            quotes
                .iter()
                .filter(|quote| query.expiries.contains(&quote.expiry)),
        )
        .map(|value| value.map(Arc::new))
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
    struct Feed {
        chain: Mutex<Option<(OptionChainQuery, OptionChainSink)>>,
        exposure: Mutex<Option<(OptionExposureQuery, OptionExposureSink)>>,
    }

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }

        fn option_chain(&self, query: OptionChainQuery, sink: OptionChainSink) {
            *self.chain.lock().unwrap() = Some((query, sink));
        }

        fn option_exposure(&self, query: OptionExposureQuery, sink: OptionExposureSink) {
            *self.exposure.lock().unwrap() = Some((query, sink));
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
    fn gamma_keeps_expiries_separate_counts_missing_inputs_and_preserves_zero_interest() {
        let e1 = *query().expiries.start();
        let e2 = *query().expiries.end();
        let mut quotes: Vec<_> = [
            (e2, 100.0, OptionKind::Call, 200.0, Some(0.01), 4.0),
            (e1, 110.0, OptionKind::Call, 100.0, Some(0.01), 0.0),
            (e1, 100.0, OptionKind::Put, 100.0, Some(0.01), 5.0),
            (e1, 100.0, OptionKind::Call, 100.0, Some(0.02), 10.0),
            (e1, 120.0, OptionKind::Call, 100.0, None, 10.0),
        ]
        .into_iter()
        .map(|(expiry, strike, kind, price, gamma, interest)| {
            let mut row = quote("CONTRACT");
            row.expiry = expiry;
            row.strike = strike;
            row.kind = kind;
            row.underlying_price = Some(price);
            row.gamma = gamma;
            row.open_interest = Some(interest);
            row.multiplier = Some(100.0);
            row
        })
        .collect();
        let exposure = OptionExposure::gamma(&quotes).unwrap().unwrap();
        assert_eq!(exposure.kind, OptionExposureKind::Gamma);
        assert_eq!(exposure.time, quotes[0].time);
        assert_eq!(exposure.unit, "price currency per 1% move");
        assert_eq!(exposure.model, None);
        assert_eq!(exposure.missing_contracts, 1);
        let expected = [
            (e1, 100.0, Some(2000.0), Some(-500.0), 1500.0),
            (e1, 110.0, Some(0.0), None, 0.0),
            (e2, 100.0, Some(1600.0), None, 1600.0),
        ];
        assert_eq!(exposure.rows.len(), expected.len());
        for (row, (expiry, strike, calls, puts, net)) in exposure.rows.iter().zip(expected) {
            assert_eq!((row.expiry, row.strike), (expiry, strike));
            assert_eq!((row.calls, row.puts, row.net), (calls, puts, Some(net)));
        }

        type Field = fn(&mut OptionQuote) -> &mut Option<f64>;
        for select in [
            (|r| &mut r.gamma) as Field,
            (|r| &mut r.open_interest) as Field,
            (|r| &mut r.multiplier) as Field,
            (|r| &mut r.underlying_price) as Field,
        ] {
            let mut row = quotes[0].clone();
            *select(&mut row) = None;
            let exposure = OptionExposure::gamma([&row]).unwrap().unwrap();
            assert!(exposure.rows.is_empty());
            assert_eq!(exposure.missing_contracts, 1);
        }
        assert_eq!(OptionExposure::gamma(&[]).unwrap(), None);

        quotes[0].time += chrono::TimeDelta::seconds(1);
        quotes.push(quotes[3].clone());
        quotes.push(quotes[2].clone());
        let exposure = OptionExposure::gamma(&quotes).unwrap().unwrap();
        assert_eq!(exposure.time, quotes[0].time);
        assert_eq!(exposure.rows[0].calls, Some(4000.0));
        assert_eq!(exposure.rows[0].puts, Some(-1000.0));
        assert_eq!(exposure.rows[0].net, Some(3000.0));

        for (gamma, interest, multiplier, price, succeeds) in [
            (f64::NAN, 1.0, 100.0, 100.0, false),
            (0.01, -1.0, 100.0, 100.0, false),
            (0.01, 1.0, 0.0, 100.0, false),
            (0.01, 1.0, 100.0, f64::INFINITY, false),
            (1.0, 1.0, 100.0, f64::MAX, false),
            (1.0, 0.0, 100.0, f64::MAX, true),
            (0.01, 1.0, 100.0, 0.0, true),
            (f64::MAX, f64::MAX, 100.0, 0.0, true),
        ] {
            let mut row = quotes[0].clone();
            row.gamma = Some(gamma);
            row.open_interest = Some(interest);
            row.multiplier = Some(multiplier);
            row.underlying_price = Some(price);
            let result = OptionExposure::gamma([&row]);
            if succeeds {
                assert_eq!(result.unwrap().unwrap().rows[0].calls, Some(0.0));
            } else {
                assert!(matches!(result, Err(FeedError::InvalidData(_))));
            }
        }
        let mut row = quotes[0].clone();
        row.gamma = Some(1.0e308);
        row.open_interest = Some(1.0);
        row.multiplier = Some(1.0);
        row.underlying_price = Some(1.0);
        assert!(matches!(
            OptionExposure::gamma(std::iter::repeat_n(&row, 200)),
            Err(FeedError::InvalidData(_))
        ));
    }

    #[test]
    fn supplied_exposure_replaces_gamma_and_invalid_snapshots_preserve_the_whole_observation() {
        for kind in [
            OptionExposureKind::Gamma,
            OptionExposureKind::Delta,
            OptionExposureKind::Vanna,
            OptionExposureKind::Charm,
        ] {
            let feed = Feed::default();
            let query = OptionExposureQuery::new("SAMPLE", query().expiries, kind);
            let handle = OptionExposureHandle::new(&feed, query.clone(), || {}).unwrap();
            let (received, sink) = feed.exposure.lock().unwrap().clone().unwrap();
            assert_eq!(received, query);
            assert!(handle.snapshot().is_none());
            let mut contract = quote("CALL");
            contract.gamma = Some(0.02);
            contract.open_interest = Some(10.0);
            contract.multiplier = Some(100.0);
            contract.underlying_price = Some(100.0);
            let fallback = handle
                .snapshot_or_gamma(std::slice::from_ref(&contract))
                .unwrap();
            assert_eq!(fallback.is_some(), kind == OptionExposureKind::Gamma);
            if let Some(fallback) = fallback {
                assert_eq!(fallback.rows[0].calls, Some(2000.0));
            }
            contract.expiry = query.expiries.start().pred_opt().unwrap();
            assert!(handle.snapshot_or_gamma(&[contract]).unwrap().is_none());

            let mut row = OptionExposureRow::new(*query.expiries.end(), 110.0);
            row.puts = Some(17.0);
            row.net = Some(-42.0);
            let mut supplied =
                OptionExposure::new(kind, quote("CALL").time, "host units", vec![row]);
            supplied.model = Some("host model".into());
            supplied.missing_contracts = 3;
            sink.push(supplied.clone()).unwrap();
            let held = handle.snapshot().unwrap();
            // Even invalid fallback quotes cannot change or prevent a supplied observation.
            let mut invalid_quote = quote("INVALID");
            invalid_quote.strike = f64::NAN;
            let chosen = handle.snapshot_or_gamma(&[invalid_quote]).unwrap().unwrap();
            assert!(Arc::ptr_eq(&held, &chosen));
            assert_eq!(*chosen, supplied);

            type Field = fn(&mut OptionExposureRow) -> &mut Option<f64>;
            let mut invalid = Vec::new();
            for select in [
                (|r| &mut r.calls) as Field,
                (|r| &mut r.puts) as Field,
                (|r| &mut r.net) as Field,
            ] {
                for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                    let mut bad = supplied.clone();
                    *select(&mut bad.rows[0]) = Some(value);
                    bad.rows
                        .insert(0, OptionExposureRow::new(*query.expiries.start(), 90.0));
                    invalid.push(bad);
                }
            }
            for strike in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
                let mut bad = supplied.clone();
                bad.rows[0].strike = strike;
                invalid.push(bad);
            }
            for price in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut bad = supplied.clone();
                bad.zero_gamma = Some(price);
                invalid.push(bad);
            }
            let mut bad = supplied.clone();
            bad.rows[0].expiry = query.expiries.end().succ_opt().unwrap();
            invalid.push(bad);
            let mut bad = supplied.clone();
            bad.kind = if kind == OptionExposureKind::Gamma {
                OptionExposureKind::Delta
            } else {
                OptionExposureKind::Gamma
            };
            invalid.push(bad);
            let mut bad = supplied.clone();
            bad.rows = vec![bad.rows[0].clone(); 100_001];
            invalid.push(bad);
            for bad in invalid {
                let error = sink.push(bad).unwrap_err();
                assert!(matches!(error, FeedError::InvalidData(_)));
                assert_eq!(handle.error(), Some(error));
                assert!(Arc::ptr_eq(&handle.snapshot().unwrap(), &held));
            }
            sink.report_error(FeedError::Network("offline".into()));
            assert!(Arc::ptr_eq(&handle.snapshot().unwrap(), &held));
            supplied.rows.clear();
            sink.push(supplied.clone()).unwrap();
            assert_eq!(*handle.snapshot_or_gamma(&[]).unwrap().unwrap(), supplied);
            assert_eq!(handle.error(), None);
        }
    }

    #[test]
    fn exposure_snapshots_stop_after_historical_answers_or_the_last_consumer() {
        for historical in [false, true] {
            for fail in [false, true] {
                let feed = Feed::default();
                let mut query =
                    OptionExposureQuery::new("SAMPLE", query().expiries, OptionExposureKind::Gamma);
                query.as_of = historical.then_some(quote("CALL").time);
                let wakes = Arc::new(AtomicUsize::new(0));
                let counter = wakes.clone();
                let handle = OptionExposureHandle::new(&feed, query, move || {
                    counter.fetch_add(1, Ordering::Relaxed);
                })
                .unwrap();
                let (_, sink) = feed.exposure.lock().unwrap().clone().unwrap();
                let clone = handle.clone();
                let supplied = OptionExposure::new(
                    OptionExposureKind::Gamma,
                    quote("CALL").time,
                    "host units",
                    Vec::new(),
                );
                if fail {
                    sink.report_error(FeedError::NotSupported);
                } else {
                    sink.push(supplied.clone()).unwrap();
                }
                assert_eq!(sink.is_active(), !historical);
                assert_eq!(wakes.load(Ordering::Relaxed), 1);
                sink.push(supplied).unwrap();
                sink.report_error(FeedError::Network("late".into()));
                assert_eq!(
                    wakes.load(Ordering::Relaxed),
                    if historical { 1 } else { 3 }
                );
                assert_eq!(handle.snapshot(), clone.snapshot());
                if historical {
                    assert_eq!(handle.snapshot().is_none(), fail);
                    assert_eq!(handle.error(), fail.then_some(FeedError::NotSupported));
                }
                drop(handle);
                assert_eq!(sink.is_active(), !historical);
                drop(clone);
                assert!(!sink.is_active());
                let before = wakes.load(Ordering::Relaxed);
                sink.report_error(FeedError::NotSupported);
                assert_eq!(wakes.load(Ordering::Relaxed), before);
            }
        }
    }

    #[test]
    fn invalid_snapshots_name_the_contract_and_field_and_preserve_all_held_rows() {
        let feed = Feed::default();
        let chain = OptionChain::new(&feed, query(), || {}).unwrap();
        let (_, sink) = feed.chain.lock().unwrap().clone().unwrap();
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
            let (received, sink) = feed.chain.lock().unwrap().clone().unwrap();
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
                sink.report_error(FeedError::NotSupported);
                assert!(chain.rows().is_empty());
                assert_eq!(chain.error(), Some(FeedError::Network("offline".into())));
                assert_eq!(wakes.load(Ordering::Relaxed), 1);
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
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let chain = OptionChain::new(&feed, query, move || {
            counter.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
        let (_, sink) = feed.chain.lock().unwrap().clone().unwrap();
        sink.push(vec![quote("CALL")]).unwrap();
        let held = chain.rows();
        assert!(!sink.is_active());
        sink.push(Vec::new()).unwrap();
        sink.report_error(FeedError::NotSupported);
        assert!(Arc::ptr_eq(&chain.rows(), &held));
        assert_eq!(chain.error(), None);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
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
            let chain = OptionChain::new(&HistoryOnly, query.clone(), || {}).unwrap();
            assert!(chain.rows().is_empty());
            assert_eq!(chain.error(), Some(FeedError::NotSupported));
            for kind in [
                OptionExposureKind::Gamma,
                OptionExposureKind::Delta,
                OptionExposureKind::Vanna,
                OptionExposureKind::Charm,
            ] {
                let mut exposure_query =
                    OptionExposureQuery::new("SAMPLE", query.expiries.clone(), kind);
                exposure_query.as_of = query.as_of;
                let handle =
                    OptionExposureHandle::new(&HistoryOnly, exposure_query, || {}).unwrap();
                assert!(handle.snapshot().is_none());
                assert_eq!(handle.error(), Some(FeedError::NotSupported));
            }
        }

        let feed = Feed::default();
        let mut empty_symbol = query();
        empty_symbol.symbol.clear();
        let mut reversed = query();
        reversed.expiries = *reversed.expiries.end()..=*reversed.expiries.start();
        for query in [empty_symbol, reversed] {
            let exposure_query = OptionExposureQuery::new(
                &query.symbol,
                query.expiries.clone(),
                OptionExposureKind::Gamma,
            );
            assert!(OptionChain::new(&feed, query, || {}).is_none());
            assert!(OptionExposureHandle::new(&feed, exposure_query, || {}).is_none());
        }
        assert!(feed.chain.lock().unwrap().is_none());
        assert!(feed.exposure.lock().unwrap().is_none());
    }
}
