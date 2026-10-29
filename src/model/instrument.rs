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
    /// URI of the symbol's mark, such as a company logo, for egui's installed image loaders.
    /// Symbol choosers show the symbol's initial while it is unknown, loading or unreadable.
    pub logo: Option<String>,
    /// URI of the exchange's mark, for egui's installed image loaders. Symbol search shows the
    /// exchange name alone while it is unknown, loading or unreadable.
    pub exchange_logo: Option<String>,
    /// Country of the listing, as an ISO 3166-1 alpha-2 code such as "US".
    pub country: Option<String>,
    /// Root shared by related instruments, such as "ES" for the contracts of a futures root or
    /// one security listed on several exchanges. Symbol search lists the first instrument of a
    /// root and offers the others under it.
    pub root: Option<String>,
    /// Instrument type.
    pub symbol_type: Option<SymbolType>,
    /// Smallest price increment: positive and finite. Consumers ignore any other value.
    pub tick_size: Option<f64>,
    /// Price represented by one pip: positive and finite. Unknown for instruments without pips.
    pub pip_size: Option<f64>,
    /// Decimals of price labels.
    pub price_decimals: Option<u8>,
    /// Decimals of volume readouts, including abbreviated K/M/B values.
    pub volume_decimals: Option<u8>,
    /// Currency of prices, such as "USD".
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
    /// How current the feed's data is. The legend marks delayed and end-of-day data.
    pub data_status: Option<DataStatus>,
    /// Bar intervals the feed suggests for volume profile studies, by profile length. A profile
    /// study whose source interval follows the chart reads the shortest suggested interval that
    /// is shorter than the chart's and serves its length. Empty, the default, suggests none, and
    /// the studies spread each chart bar's volume over its range.
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
            root: None,
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
            data_status: None,
            profile_resolutions: Vec::new(),
        }
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

    /// Sets [`root`](Self::root).
    pub fn with_root(mut self, root: impl Into<String>) -> Self {
        self.root = Some(root.into());
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

/// A bar interval a feed suggests for volume profile studies, for profiles up to a length.
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Session {
    /// Regular windows. With no regular or extended windows, the session is always open, apart
    /// from its holidays and the dates in [`overrides`](Self::overrides).
    pub windows: Vec<SessionWindow>,
    /// Pre-market and post-market windows in the exchange zone.
    pub extended_windows: Vec<SessionWindow>,
    /// Trading dates on which every window is closed, including overnight windows.
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

    /// Drops the pre-market and post-market windows, weekly and date-specific.
    pub(crate) fn clear_extended_windows(&mut self) {
        self.extended_windows.clear();
        for date in &mut self.overrides {
            date.extended_windows.clear();
        }
    }
}

/// The hours of one trading date, in place of its weekly windows and holiday.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionOverride {
    /// The trading date, named by the day on which the windows close, as in
    /// [`SessionWindow::days`].
    pub date: NaiveDate,
    /// Regular windows of the date; an early close is one window ending early.
    pub windows: Vec<OverrideWindow>,
    /// Pre-market and post-market windows of the date.
    pub extended_windows: Vec<OverrideWindow>,
}

impl SessionOverride {
    /// A closure: every window of `date` is closed. Add hours with
    /// [`with_windows`](Self::with_windows) and
    /// [`with_extended_windows`](Self::with_extended_windows).
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            windows: Vec::new(),
            extended_windows: Vec::new(),
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
}

/// One window of a [`SessionOverride`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

    #[test]
    fn test_symbol_type_display() {
        assert_eq!(SymbolType::Stock.to_string(), "stock");
        assert_eq!(SymbolType::Crypto.to_string(), "crypto");
        assert_eq!(SymbolType::Futures.to_string(), "futures");
        assert_eq!(SymbolType::Forex.to_string(), "forex");
    }
}
