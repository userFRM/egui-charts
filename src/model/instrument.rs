//! Instrument metadata reported by a feed or a search service.

use super::{Timeframe, TimezoneId};
use chrono::{NaiveDate, NaiveTime, Weekday};
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
}

impl Instrument {
    /// `symbol`, with everything else unknown.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            description: None,
            exchange: None,
            symbol_type: None,
            tick_size: None,
            pip_size: None,
            price_decimals: None,
            volume_decimals: None,
            currency: None,
            time_zone: None,
            session: None,
            native_timeframes: None,
            supported_timeframes: None,
            data_status: None,
        }
    }

    /// Sets [`description`](Self::description).
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets [`exchange`](Self::exchange).
    pub fn with_exchange(mut self, exchange: impl Into<String>) -> Self {
        self.exchange = Some(exchange.into());
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
    /// Regular windows. With no regular or extended windows, the session is always open.
    pub windows: Vec<SessionWindow>,
    /// Pre-market and post-market windows in the exchange zone.
    pub extended_windows: Vec<SessionWindow>,
    /// Trading dates on which every window is closed, including overnight windows.
    pub holidays: Vec<NaiveDate>,
}

impl Session {
    /// Trading hours made of `windows`.
    pub fn new(windows: impl IntoIterator<Item = SessionWindow>) -> Self {
        Self {
            windows: windows.into_iter().collect(),
            extended_windows: Vec::new(),
            holidays: Vec::new(),
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
