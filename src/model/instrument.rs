//! Instrument metadata reported by a feed or a search service.

use chrono::{NaiveTime, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// Symbol type categories for instrument classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SymbolType {
    #[default]
    Stock,
    Index,
    Forex,
    Futures,
    Crypto,
    Cfd,
    Bond,
    Fund,
    Option,
    Commodity,
    Economic,
    Dr,
    Right,
    Warrant,
    Expression,
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
    /// Decimals of price labels.
    pub price_decimals: Option<u8>,
    /// Currency of prices, such as "USD".
    pub currency: Option<String>,
    /// Exchange time zone.
    pub time_zone: Option<TimezoneId>,
    /// Regular trading hours, in exchange time.
    pub session: Option<Session>,
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
            price_decimals: None,
            currency: None,
            time_zone: None,
            session: None,
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

    /// Sets [`price_decimals`](Self::price_decimals).
    pub fn with_price_decimals(mut self, decimals: u8) -> Self {
        self.price_decimals = Some(decimals);
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
}

/// A time zone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimezoneId(Tz);

impl TimezoneId {
    /// Coordinated Universal Time.
    pub fn utc() -> Self {
        Self(Tz::UTC)
    }

    /// The zone of an IANA name such as "America/New_York", if known.
    pub fn named(name: &str) -> Option<Self> {
        name.parse().ok().map(Self)
    }

    /// The IANA name, such as "America/New_York" or "UTC".
    pub fn name(&self) -> &str {
        self.0.name()
    }

    #[cfg(feature = "studies")]
    pub(crate) fn tz(&self) -> Tz {
        self.0
    }
}

/// Trading hours: the windows in which an instrument trades.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Session {
    /// No window means always open.
    pub windows: Vec<SessionWindow>,
}

impl Session {
    /// Trading hours made of `windows`.
    pub fn new(windows: impl IntoIterator<Item = SessionWindow>) -> Self {
        Self {
            windows: windows.into_iter().collect(),
        }
    }
}

/// One trading window, repeated on each of `days`.
#[derive(Clone, Debug, PartialEq)]
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
