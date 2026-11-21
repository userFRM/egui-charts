//! Instrument metadata reported by a feed or a search service.

use super::{Timeframe, TimezoneId};
use chrono::{NaiveDate, NaiveTime, TimeDelta, Weekday};
use serde::{Deserialize, Serialize};

/// Symbol type categories for instrument classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SymbolType {
    #[default]
    /// An equity security.
    Stock,
    /// A market index.
    Index,
    /// A currency pair.
    Forex,
    /// A futures contract.
    Futures,
    /// A cryptocurrency instrument.
    Crypto,
    /// A contract for difference.
    Cfd,
    /// A debt security.
    Bond,
    /// An investment fund.
    Fund,
    /// An options contract.
    Option,
    /// A commodity instrument.
    Commodity,
    /// An economic data series.
    Economic,
    /// A depositary receipt.
    Dr,
    /// A subscription right.
    Right,
    /// A warrant.
    Warrant,
    /// A host-defined expression over instruments.
    Expression,
    /// A spread between instruments.
    Spread,
}

impl std::fmt::Display for SymbolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stock => write!(f, "stock"),
            Self::Index => write!(f, "index"),
            Self::Forex => write!(f, "forex"),
            Self::Futures => write!(f, "futures"),
            Self::Crypto => write!(f, "crypto"),
            Self::Cfd => write!(f, "cfd"),
            Self::Bond => write!(f, "bond"),
            Self::Fund => write!(f, "fund"),
            Self::Option => write!(f, "option"),
            Self::Commodity => write!(f, "commodity"),
            Self::Economic => write!(f, "economic"),
            Self::Dr => write!(f, "dr"),
            Self::Right => write!(f, "right"),
            Self::Warrant => write!(f, "warrant"),
            Self::Expression => write!(f, "expression"),
            Self::Spread => write!(f, "spread"),
        }
    }
}

/// An optional service of a [`Datafeed`](crate::data::Datafeed), listed in
/// [`FeedCapabilities::services`](crate::data::FeedCapabilities::services) and [`Instrument::services`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FeedService {
    /// [`Datafeed::trades`](crate::data::Datafeed::trades).
    Trades,
    /// [`Datafeed::depth_history`](crate::data::Datafeed::depth_history).
    DepthHistory,
    /// [`Datafeed::options_volume`](crate::data::Datafeed::options_volume).
    OptionsVolume,
    /// [`Datafeed::option_expiries`](crate::data::Datafeed::option_expiries) and
    /// [`Datafeed::option_chain`](crate::data::Datafeed::option_chain).
    OptionChain,
    /// [`Datafeed::option_exposure`](crate::data::Datafeed::option_exposure).
    OptionExposure,
    /// [`Datafeed::metrics`](crate::data::Datafeed::metrics) and
    /// [`Datafeed::metric_history`](crate::data::Datafeed::metric_history).
    Metrics,
    /// [`Datafeed::price_targets`](crate::data::Datafeed::price_targets) and
    /// [`Datafeed::price_targets_converted`](crate::data::Datafeed::price_targets_converted).
    PriceTargets,
    /// [`Datafeed::financials`](crate::data::Datafeed::financials).
    Financials,
    /// [`Datafeed::symbol_details`](crate::data::Datafeed::symbol_details).
    SymbolDetails,
    /// [`Datafeed::news`](crate::data::Datafeed::news).
    News,
    /// [`Datafeed::calendar`](crate::data::Datafeed::calendar).
    Calendar,
    /// [`Datafeed::screen`](crate::data::Datafeed::screen).
    Screener,
    /// [`Datafeed::hotlist`](crate::data::Datafeed::hotlist).
    Hotlist,
    /// Live executions through [`Sink::push_tick`](crate::data::Sink::push_tick).
    LiveTrades,
    /// Aggressor classification in [`Tick::side`](super::Tick::side) of supplied executions.
    TradeSides,
    /// Best bid and ask through [`Sink::push_quote`](crate::data::Sink::push_quote).
    BestBidAsk,
    /// Price-level depth through [`Sink::push_depth_snapshot`](crate::data::Sink::push_depth_snapshot)
    /// and [`Sink::push_depth_delta`](crate::data::Sink::push_depth_delta).
    Depth,
    /// Individual resting orders through [`Sink::push_order_snapshot`](crate::data::Sink::push_order_snapshot)
    /// and [`Sink::push_order_event`](crate::data::Sink::push_order_event).
    OrderDepth,
}

/// Kind of a security identifier in [`Instrument::identifiers`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdentifierKind {
    /// International Securities Identification Number (ISO 6166), such as "US0378331005".
    Isin,
    /// CUSIP number, such as "037833100".
    Cusip,
    /// SEDOL code, such as "2046251".
    Sedol,
    /// Financial Instrument Global Identifier, such as "BBG000B9XRY4".
    Figi,
    /// Any other code, named by the label shown before its value, such as "WKN".
    Other(String),
}

impl std::fmt::Display for IdentifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Isin => "ISIN",
            Self::Cusip => "CUSIP",
            Self::Sedol => "SEDOL",
            Self::Figi => "FIGI",
            Self::Other(label) => label,
        })
    }
}

/// What a feed or a search service reports about a symbol. Build it with [`Instrument::new`] and the `with_` setters;
/// fields left out stay unknown.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Instrument {
    /// The symbol as the feed knows it.
    pub symbol: String,
    /// Full name, such as "Apple Inc.".
    pub description: Option<String>,
    /// Exchange or venue, such as "NASDAQ".
    pub exchange: Option<String>,
    /// URI of the symbol's logo for egui's installed image loaders; supply a square image. The
    /// legend and symbol choosers draw it in a circle, and the symbol's first letter or digit
    /// while it is unknown, loading or unreadable. A prefixed symbol such as `VENUE:ABC` shows V.
    pub logo: Option<String>,
    /// URI of the exchange's mark, for egui's installed image loaders. Symbol search shows the
    /// exchange name alone while it is unknown, loading or unreadable.
    pub exchange_logo: Option<String>,
    /// Country of the listing, as an ISO 3166-1 alpha-2 code such as "US".
    pub country: Option<String>,
    /// Company sector reported by the host, such as "Technology".
    pub sector: Option<String>,
    /// Data provider for this listing; independent of its exchange. Keep the feed lookup key in
    /// [`symbol`](Self::symbol), using distinct keys when the provider changes the feed request.
    pub source: Option<String>,
    /// Root shared by related instruments, such as "ES" for the contracts of a futures root or
    /// one security listed on several exchanges. Symbol search lists the first instrument of a
    /// root and offers the others under it, keeping the futures of each exchange apart.
    pub root: Option<String>,
    /// Security identifiers, such as the ISIN and CUSIP of a listing, each with its kind. The
    /// search over a fixed list finds the instrument by any of them typed exactly, ignoring case
    /// and whitespace, and the symbol choosers show the identifier a query names. Empty, the
    /// default, when the host supplies none.
    pub identifiers: Vec<(IdentifierKind, String)>,
    /// Instrument type.
    pub symbol_type: Option<SymbolType>,
    /// Smallest price increment: positive and finite. Consumers ignore any other value.
    pub tick_size: Option<f64>,
    /// Price represented by one pip: positive and finite. Unknown for instruments without pips.
    pub pip_size: Option<f64>,
    /// Decimals of price labels.
    pub price_decimals: Option<u8>,
    /// Decimals of volume readouts, of studies in volume units without a precision chosen in
    /// their settings, of measured volume, of footprint volumes and of volume profile row values.
    /// They describe the whole number: an abbreviated K, M or B value keeps the decimals its
    /// readout writes without them when they are fewer, one in footprint cells and statistics and
    /// two elsewhere, and takes them when they are more. At 0 a volume of 1,234.56 reads 1.23K,
    /// at 3 it reads 1.235K. Trade counts and open interest keep their own format.
    pub volume_decimals: Option<u8>,
    /// Currency of prices, such as "USD". A code that mixes upper and lower case names a minor
    /// unit and matches only itself, such as "GBp" for pence against "GBP"; other codes match
    /// regardless of case.
    pub currency: Option<String>,
    /// Host-supplied price unit, such as "USD/barrel"; no conversion is performed.
    pub unit: Option<String>,
    /// Exchange time zone.
    pub time_zone: Option<TimezoneId>,
    /// Regular trading hours, in exchange time.
    pub session: Option<Session>,
    /// OHLCV intervals supplied without aggregation. `None` means unspecified; an empty list
    /// means no native bar history. Derived intervals remain available through host resampling.
    pub native_timeframes: Option<Vec<Timeframe>>,
    /// Intervals [`Datafeed::history`](crate::data::Datafeed::history) answers, native or
    /// derived. `None` allows every interval. A chart does not ask the feed for any other
    /// interval. Pass the list to `ui::top_toolbar::TimeframeSelector::set_supported_timeframes`
    /// so that the interval picker offers only these.
    pub supported_timeframes: Option<Vec<Timeframe>>,
    /// Optional services supplied for this symbol, narrowing the feed's
    /// [`services`](crate::data::FeedCapabilities::services). `None` defers to the feed's list;
    /// an empty list supplies none. Use [`FeedCapabilities::declares`](crate::data::FeedCapabilities::declares)
    /// to require explicit confirmation from either list while respecting both.
    pub services: Option<Vec<FeedService>>,
    /// How current the feed's data is. The legend marks delayed and end-of-day data.
    pub data_status: Option<DataStatus>,
    /// Whether the feed supplies dividend-adjusted history when
    /// [`HistoryRequest::adjusted`](crate::data::HistoryRequest::adjusted) is set.
    pub dividend_adjustment: bool,
    /// Bar intervals the feed suggests for volume profile studies and drawings, by profile
    /// length. A study whose source follows the chart, or a fixed range or anchored drawing
    /// without executions, reads the shortest suggested interval shorter than the chart's that
    /// serves its length. Empty, the default, suggests none; profiles then spread each chart
    /// bar's volume over its range.
    pub profile_resolutions: Vec<ProfileResolution>,
}

impl Instrument {
    /// `symbol`, with everything else unknown.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            description: None,
            exchange: None,
            logo: None,
            exchange_logo: None,
            country: None,
            sector: None,
            source: None,
            root: None,
            identifiers: Vec::new(),
            symbol_type: None,
            tick_size: None,
            pip_size: None,
            price_decimals: None,
            volume_decimals: None,
            currency: None,
            unit: None,
            time_zone: None,
            session: None,
            native_timeframes: None,
            supported_timeframes: None,
            services: None,
            data_status: None,
            dividend_adjustment: false,
            profile_resolutions: Vec::new(),
        }
    }

    /// Futures roots belong to a venue; other roots gather listings across venues and providers.
    pub(crate) fn search_group(&self) -> Option<(&str, Option<SymbolType>, Option<&str>)> {
        Some((
            self.root.as_deref()?,
            self.symbol_type,
            if self.symbol_type == Some(SymbolType::Futures) {
                self.exchange
                    .as_deref()
                    .or_else(|| self.symbol.split_once(':').map(|(venue, _)| venue))
            } else {
                None
            },
        ))
    }

    /// Sets [`description`](Self::description).
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the price [`unit`](Self::unit).
    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Sets [`exchange`](Self::exchange).
    pub fn with_exchange(mut self, exchange: impl Into<String>) -> Self {
        self.exchange = Some(exchange.into());
        self
    }

    /// Sets [`logo`](Self::logo).
    pub fn with_logo(mut self, uri: impl Into<String>) -> Self {
        self.logo = Some(uri.into());
        self
    }

    /// Sets [`exchange_logo`](Self::exchange_logo).
    pub fn with_exchange_logo(mut self, uri: impl Into<String>) -> Self {
        self.exchange_logo = Some(uri.into());
        self
    }

    /// Sets [`country`](Self::country).
    pub fn with_country(mut self, country: impl Into<String>) -> Self {
        self.country = Some(country.into());
        self
    }

    /// Sets [`sector`](Self::sector).
    pub fn with_sector(mut self, sector: impl Into<String>) -> Self {
        self.sector = Some(sector.into());
        self
    }

    /// Sets [`source`](Self::source).
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Sets [`root`](Self::root).
    pub fn with_root(mut self, root: impl Into<String>) -> Self {
        self.root = Some(root.into());
        self
    }

    /// Adds an identifier to [`identifiers`](Self::identifiers), after those already set.
    ///
    /// ```
    /// use egui_charts::model::{IdentifierKind, Instrument};
    ///
    /// let apple = Instrument::new("AAPL")
    ///     .with_identifier(IdentifierKind::Isin, "US0378331005")
    ///     .with_identifier(IdentifierKind::Other("WKN".into()), "865985");
    /// assert_eq!(apple.identifiers.len(), 2);
    /// ```
    pub fn with_identifier(mut self, kind: IdentifierKind, value: impl Into<String>) -> Self {
        self.identifiers.push((kind, value.into()));
        self
    }

    /// Sets [`symbol_type`](Self::symbol_type).
    pub fn with_symbol_type(mut self, symbol_type: SymbolType) -> Self {
        self.symbol_type = Some(symbol_type);
        self
    }

    /// Sets [`tick_size`](Self::tick_size).
    pub fn with_tick_size(mut self, tick_size: f64) -> Self {
        self.tick_size = Some(tick_size);
        self
    }

    /// Sets [`pip_size`](Self::pip_size).
    pub fn with_pip_size(mut self, pip_size: f64) -> Self {
        self.pip_size = Some(pip_size);
        self
    }

    /// Sets [`price_decimals`](Self::price_decimals).
    pub fn with_price_decimals(mut self, decimals: u8) -> Self {
        self.price_decimals = Some(decimals);
        self
    }

    /// Sets [`volume_decimals`](Self::volume_decimals).
    pub fn with_volume_decimals(mut self, decimals: u8) -> Self {
        self.volume_decimals = Some(decimals);
        self
    }

    /// Sets [`currency`](Self::currency).
    pub fn with_currency(mut self, currency: impl Into<String>) -> Self {
        self.currency = Some(currency.into());
        self
    }

    /// Sets [`time_zone`](Self::time_zone).
    pub fn with_time_zone(mut self, time_zone: TimezoneId) -> Self {
        self.time_zone = Some(time_zone);
        self
    }

    /// Sets [`session`](Self::session).
    pub fn with_session(mut self, session: Session) -> Self {
        self.session = Some(session);
        self
    }

    /// Sets [`native_timeframes`](Self::native_timeframes), reported by [`Datafeed::resolve`](crate::data::Datafeed::resolve).
    pub fn with_native_timeframes(
        mut self,
        timeframes: impl IntoIterator<Item = Timeframe>,
    ) -> Self {
        self.native_timeframes = Some(timeframes.into_iter().collect());
        self
    }

    /// Sets [`supported_timeframes`](Self::supported_timeframes).
    pub fn with_supported_timeframes(
        mut self,
        timeframes: impl IntoIterator<Item = Timeframe>,
    ) -> Self {
        self.supported_timeframes = Some(timeframes.into_iter().collect());
        self
    }

    /// Sets [`services`](Self::services).
    pub fn with_services(mut self, services: impl IntoIterator<Item = FeedService>) -> Self {
        self.services = Some(services.into_iter().collect());
        self
    }

    /// Sets [`dividend_adjustment`](Self::dividend_adjustment).
    pub fn with_dividend_adjustment(mut self, adjustment: bool) -> Self {
        self.dividend_adjustment = adjustment;
        self
    }

    /// Sets [`data_status`](Self::data_status).
    pub fn with_data_status(mut self, status: DataStatus) -> Self {
        self.data_status = Some(status);
        self
    }

    /// Sets [`profile_resolutions`](Self::profile_resolutions).
    pub fn with_profile_resolutions(
        mut self,
        resolutions: impl IntoIterator<Item = ProfileResolution>,
    ) -> Self {
        self.profile_resolutions = resolutions.into_iter().collect();
        self
    }
}

/// Whether two currency codes name the same currency. A code that mixes upper and lower case
/// names a minor unit, such as `GBp` for pence, and matches only itself; other codes match
/// regardless of ASCII case, so `gbp` is `GBP`.
#[cfg(any(feature = "studies", feature = "drawings", feature = "strategies"))]
pub(crate) fn same_currency(a: &str, b: &str) -> bool {
    let mixed = |code: &str| {
        code.bytes().any(|c| c.is_ascii_lowercase()) && code.bytes().any(|c| c.is_ascii_uppercase())
    };
    if mixed(a) || mixed(b) {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

/// A bar interval a feed suggests for volume profile studies and drawings, for profiles up to a length.
/// Report them in [`Instrument::profile_resolutions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileResolution {
    /// Interval of the bars whose volume the profile distributes.
    pub timeframe: Timeframe,
    /// The longest profile this interval serves, from the open of the profile's first chart
    /// bar to the close of its last. `None` serves profiles of any length.
    pub max_span: Option<TimeDelta>,
}

impl ProfileResolution {
    /// Suggests `timeframe` for profiles up to `max_span` long, or of any length for `None`.
    pub fn new(timeframe: Timeframe, max_span: Option<TimeDelta>) -> Self {
        Self {
            timeframe,
            max_span,
        }
    }

    /// The shortest time interval shorter than `interval` that serves `span`, from the open of
    /// the profile's first bar to the close of its last.
    #[cfg(any(feature = "studies", feature = "drawings"))]
    pub(crate) fn suggested(
        resolutions: &[Self],
        interval: Timeframe,
        span: TimeDelta,
    ) -> Option<Timeframe> {
        resolutions
            .iter()
            .filter(|hint| {
                !hint.timeframe.is_trade_based()
                    && hint.timeframe.duration_ms() < interval.duration_ms()
                    && hint.max_span.is_none_or(|max| span <= max)
            })
            .min_by_key(|hint| hint.timeframe.duration_ms())
            .map(|hint| hint.timeframe)
    }
}

/// How current a feed's data is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataStatus {
    /// Updates as the market trades.
    Realtime,
    /// Updates some time after the market trades, as exchanges require of many free feeds.
    Delayed,
    /// Updates once a day, after the close.
    EndOfDay,
}

/// Trading hours: the windows in which an instrument trades.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Session {
    /// Regular windows. With no regular, extended or overnight windows, the session is always
    /// open, apart from its holidays and the dates in [`overrides`](Self::overrides).
    pub windows: Vec<SessionWindow>,
    /// Pre-market and post-market windows in the exchange zone.
    pub extended_windows: Vec<SessionWindow>,
    /// Overnight windows in the exchange zone, such as 20:00 to 04:00 between the post-market
    /// close and the next pre-market open. Windows spanning midnight belong to their closing
    /// weekday, as in [`SessionWindow::days`]. The chart requests them with overnight hours,
    /// shades them separately and names their market phase. Older saved hours load with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overnight_windows: Vec<SessionWindow>,
    /// Trading dates on which every window is closed, including windows closing after midnight.
    pub holidays: Vec<NaiveDate>,
    /// Date-specific hours, such as early closes and exceptional openings. An override replaces
    /// the weekly windows and any holiday of its date.
    pub overrides: Vec<SessionOverride>,
}

impl Session {
    /// Trading hours made of `windows`.
    pub fn new(windows: impl IntoIterator<Item = SessionWindow>) -> Self {
        Self {
            windows: windows.into_iter().collect(),
            extended_windows: Vec::new(),
            overnight_windows: Vec::new(),
            holidays: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Adds the extended windows beside the regular windows.
    pub fn with_extended_windows(
        mut self,
        windows: impl IntoIterator<Item = SessionWindow>,
    ) -> Self {
        self.extended_windows = windows.into_iter().collect();
        self
    }

    /// Adds the overnight windows beside the regular and extended windows.
    ///
    /// ```
    /// use chrono::{DateTime, NaiveTime, Utc, Weekday::*};
    /// use egui_charts::model::{ExchangeSessionProvider, Session, SessionWindow, TimezoneId};
    ///
    /// let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
    /// let weekdays = [Mon, Tue, Wed, Thu, Fri];
    /// let session = Session::new([SessionWindow::new(weekdays, at(9, 30), at(16, 0))])
    ///     .with_extended_windows([
    ///         SessionWindow::new(weekdays, at(4, 0), at(9, 30)),
    ///         SessionWindow::new(weekdays, at(16, 0), at(20, 0)),
    ///     ])
    ///     // Named by the day each window closes: Monday's opens on Sunday at 20:00.
    ///     .with_overnight_windows([SessionWindow::new(weekdays, at(20, 0), at(4, 0))]);
    /// let hours = ExchangeSessionProvider::new(TimezoneId::utc(), Some(session));
    /// // Tuesday at 22:00 is outside regular hours and inside the overnight session.
    /// let night = "2024-07-09T22:00:00Z".parse::<DateTime<Utc>>()?;
    /// assert!(!hours.is_open(night, false) && hours.is_open(night, true));
    /// assert!(hours.is_extended(night));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_overnight_windows(
        mut self,
        windows: impl IntoIterator<Item = SessionWindow>,
    ) -> Self {
        self.overnight_windows = windows.into_iter().collect();
        self
    }

    /// Closes every window on these trading dates.
    pub fn with_holidays(mut self, holidays: impl IntoIterator<Item = NaiveDate>) -> Self {
        self.holidays = holidays.into_iter().collect();
        self
    }

    /// Replaces the hours of the dates in `overrides`. For a date listed twice the first applies.
    pub fn with_overrides(mut self, overrides: impl IntoIterator<Item = SessionOverride>) -> Self {
        self.overrides = overrides.into_iter().collect();
        self
    }

    /// Drops the windows of the hours not selected, weekly and date-specific: the pre-market and
    /// post-market windows without `extended`, the overnight windows without `overnight`. Hours
    /// left without any weekly window stay scheduled, with one window on no day, so that they
    /// stay closed instead of becoming continuous.
    pub(crate) fn select_hours(&mut self, extended: bool, overnight: bool) {
        let scheduled = !self.windows.is_empty()
            || !self.extended_windows.is_empty()
            || !self.overnight_windows.is_empty();
        if !extended {
            self.extended_windows.clear();
        }
        if !overnight {
            self.overnight_windows.clear();
        }
        for date in &mut self.overrides {
            if !extended {
                date.extended_windows.clear();
            }
            if !overnight {
                date.overnight_windows.clear();
            }
        }
        if scheduled
            && self.windows.is_empty()
            && self.extended_windows.is_empty()
            && self.overnight_windows.is_empty()
        {
            // No window at all would mean always open.
            self.windows
                .push(SessionWindow::new([], NaiveTime::MIN, NaiveTime::MIN));
        }
    }
}

/// The hours of one trading date, in place of its weekly windows and holiday.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct SessionOverride {
    /// The trading date, named by the day on which the windows close, as in
    /// [`SessionWindow::days`].
    pub date: NaiveDate,
    /// Regular windows of the date; an early close is one window ending early.
    pub windows: Vec<OverrideWindow>,
    /// Pre-market and post-market windows of the date.
    pub extended_windows: Vec<OverrideWindow>,
    /// Overnight windows of the date.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overnight_windows: Vec<OverrideWindow>,
}

impl SessionOverride {
    /// A closure: every window of `date` is closed. Add hours with
    /// [`with_windows`](Self::with_windows),
    /// [`with_extended_windows`](Self::with_extended_windows) and
    /// [`with_overnight_windows`](Self::with_overnight_windows).
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            windows: Vec::new(),
            extended_windows: Vec::new(),
            overnight_windows: Vec::new(),
        }
    }

    /// Sets the regular windows of the date.
    pub fn with_windows(mut self, windows: impl IntoIterator<Item = OverrideWindow>) -> Self {
        self.windows = windows.into_iter().collect();
        self
    }

    /// Sets the extended windows of the date.
    pub fn with_extended_windows(
        mut self,
        windows: impl IntoIterator<Item = OverrideWindow>,
    ) -> Self {
        self.extended_windows = windows.into_iter().collect();
        self
    }

    /// Sets the overnight windows of the date.
    ///
    /// ```
    /// use chrono::{DateTime, NaiveDate, NaiveTime, Utc, Weekday::*};
    /// use egui_charts::model::{
    ///     ExchangeSessionProvider, OverrideWindow, Session, SessionOverride, SessionWindow,
    ///     TimezoneId,
    /// };
    ///
    /// let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    /// let monday = NaiveDate::from_ymd_opt(2024, 7, 15).unwrap();
    /// // Monday's regular hours are closed; its overnight window still opens on Sunday.
    /// let session = Session::new([SessionWindow::new([Mon], at(9), at(16))]).with_overrides([
    ///     SessionOverride::new(monday).with_overnight_windows([OverrideWindow::new(at(20), at(4))]),
    /// ]);
    /// let hours = ExchangeSessionProvider::new(TimezoneId::utc(), Some(session));
    /// let sunday_night = "2024-07-14T22:00:00Z".parse::<DateTime<Utc>>()?;
    /// let monday_noon = "2024-07-15T12:00:00Z".parse::<DateTime<Utc>>()?;
    /// assert!(hours.is_open(sunday_night, true) && !hours.is_open(sunday_night, false));
    /// assert!(!hours.is_open(monday_noon, true));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_overnight_windows(
        mut self,
        windows: impl IntoIterator<Item = OverrideWindow>,
    ) -> Self {
        self.overnight_windows = windows.into_iter().collect();
        self
    }
}

/// One window of a [`SessionOverride`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct OverrideWindow {
    /// Exchange time.
    pub open: NaiveTime,
    /// Exchange time. At or before `open`, the window spans midnight.
    pub close: NaiveTime,
}

impl OverrideWindow {
    /// `open` to `close`.
    pub fn new(open: NaiveTime, close: NaiveTime) -> Self {
        Self { open, close }
    }
}

/// One trading window, repeated on each of `days`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct SessionWindow {
    /// Trading days, named by the day on which the window closes.
    pub days: Vec<Weekday>,
    /// Exchange time.
    pub open: NaiveTime,
    /// Exchange time. At or before `open`, the window spans midnight.
    pub close: NaiveTime,
}

impl SessionWindow {
    /// `open` to `close` on each of `days`.
    pub fn new(days: impl IntoIterator<Item = Weekday>, open: NaiveTime, close: NaiveTime) -> Self {
        Self {
            days: days.into_iter().collect(),
            open,
            close,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hours saved before overnight windows existed load with none, and hours without them
    /// save as before.
    #[test]
    fn hours_without_overnight_windows_save_as_before() {
        let saved = r#"{"windows":[{"days":["Mon"],"open":"09:30:00","close":"16:00:00"}],"extended_windows":[{"days":["Mon"],"open":"04:00:00","close":"09:30:00"}],"holidays":["2024-07-04"],"overrides":[{"date":"2024-11-29","windows":[{"open":"09:30:00","close":"13:00:00"}],"extended_windows":[]}]}"#;
        let session: Session = serde_json::from_str(saved).unwrap();
        assert!(session.overnight_windows.is_empty());
        assert!(session.overrides[0].overnight_windows.is_empty());
        assert_eq!(serde_json::to_string(&session).unwrap(), saved);
    }

    #[test]
    fn test_symbol_type_display() {
        assert_eq!(SymbolType::Stock.to_string(), "stock");
        assert_eq!(SymbolType::Crypto.to_string(), "crypto");
        assert_eq!(SymbolType::Futures.to_string(), "futures");
        assert_eq!(SymbolType::Forex.to_string(), "forex");
    }
}
