//! Market quotes and order-book data structures.
//!
//! Market microstructure data at three granularity levels:
//!
//! | Level | Type | Data |
//! |-------|------|------|
//! | Tick | [`Tick`] | Individual trades (time & sales) |
//! | L1 | [`Level1Quote`] | Best bid/ask (top-of-book) |
//! | L2 | [`OrderBook`] | Full depth-of-market order book |
//!
//! A feed pushes trades and the order book into the chart through
//! [`Sink`](crate::data::Sink). [`QuoteSource`] supplies Level 1 quotes, which
//! the host application polls and displays itself.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single trade execution (time & sales entry).
///
/// Each tick represents one matched trade on the exchange, recording the
/// price, size, and aggressor side. Ticks are the finest-grained market
/// data and can be aggregated into bars, footprint charts, or volume
/// profiles. Build it with [`Tick::new`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Tick {
    /// Ts of the trade
    pub ts: DateTime<Utc>,
    /// Trade price
    pub price: f64,
    /// Trade size/volume
    pub size: f64,
    /// Side of the trade (if available)
    pub side: Option<TradeSide>,
    /// Exchange trade id or sequence number, unique per symbol and never reused: a number that
    /// restarts each session, such as a daily sequence number, is not an id unless combined with
    /// the session date. With it, history and live trades join without losing or repeating a
    /// trade, and a live trade whose id was already taken is dropped.
    #[serde(default)]
    pub id: Option<u64>,
}

impl Tick {
    /// A trade without an id.
    pub fn new(ts: DateTime<Utc>, price: f64, size: f64, side: Option<TradeSide>) -> Self {
        Self {
            ts,
            price,
            size,
            side,
            id: None,
        }
    }

    /// Sets [`id`](Self::id).
    pub fn with_id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }
}

/// Aggressor side of a trade.
///
/// Indicates whether the trade was initiated by a buyer lifting the ask
/// or a seller hitting the bid. This is essential for computing delta
/// (buy volume minus sell volume) in footprint and order-flow analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    /// Buy side (aggressor was buyer)
    Buy,
    /// Sell side (aggressor was seller)
    Sell,
    /// Unknown/unclassified
    Unknown,
}

/// A host-supplied session price, independent of the regular close and hovered history.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SessionQuote {
    /// Timestamp of the quote.
    pub time: DateTime<Utc>,
    /// Session price. Renderers ignore non-finite values.
    pub price: f64,
    /// Short host-supplied qualifier shown beside the price.
    pub qualifier: String,
}

impl SessionQuote {
    /// Creates a session quote without deriving it from historical bars.
    pub fn new(time: DateTime<Utc>, price: f64, qualifier: impl Into<String>) -> Self {
        Self {
            time,
            price,
            qualifier: qualifier.into(),
        }
    }
}

/// Level 1 quote -- best bid and ask (top-of-book).
///
/// A Level 1 quote captures the current best bid and ask prices and
/// their sizes. It is the most common form of real-time market data,
/// sufficient for spread calculations and basic order-book imbalance
/// metrics.
///
/// ```
/// use egui_charts::model::Level1Quote;
/// use chrono::Utc;
///
/// let quote = Level1Quote {
///     ts: Utc::now(),
///     symbol: "BTCUSDT".into(),
///     bid: 50_000.0, bid_size: 1.5,
///     ask: 50_010.0, ask_size: 2.0,
/// };
/// assert_eq!(quote.spread(), 10.0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level1Quote {
    /// Ts of the quote
    pub ts: DateTime<Utc>,
    /// Symbol
    pub symbol: String,
    /// Best bid price
    pub bid: f64,
    /// Best bid size
    pub bid_size: f64,
    /// Best ask price
    pub ask: f64,
    /// Best ask size
    pub ask_size: f64,
}

impl Level1Quote {
    /// Calculate bid-ask spread in price
    pub fn spread(&self) -> f64 {
        self.ask - self.bid
    }

    /// Calculate the bid-ask spread in basis points (1 bp = 0.01%).
    pub fn spread_bps(&self) -> f64 {
        if self.bid == 0.0 {
            return 0.0;
        }
        (self.spread() / self.bid) * 10000.0
    }

    /// Calculate mid price
    pub fn mid_price(&self) -> f64 {
        (self.bid + self.ask) / 2.0
    }

    /// Total quoted volume (bid + ask)
    pub fn total_size(&self) -> f64 {
        self.bid_size + self.ask_size
    }
}

/// A single price level in a Level 2 order book.
///
/// Represents the aggregate size (and optionally order count) resting
/// at one price level on either the bid or ask side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    /// Price level
    pub price: f64,
    /// Size at this level
    pub size: f64,
    /// Number of orders at this level (if available)
    pub order_cnt: Option<u32>,
}

/// Full depth-of-market order book snapshot (Level 2).
///
/// Contains all visible bid and ask price levels at a single point in
/// time. Bids are sorted descending (best bid first), asks ascending
/// (best ask first). Use [`imbalance_ratio`](Self::imbalance_ratio) to
/// gauge buying vs. selling pressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    /// Ts of the snapshot
    pub ts: DateTime<Utc>,
    /// Symbol
    pub symbol: String,
    /// Bid levels (sorted descending by price)
    pub bids: Vec<OrderBookLevel>,
    /// Ask levels (sorted ascending by price)
    pub asks: Vec<OrderBookLevel>,
}

impl OrderBook {
    /// Get best bid (highest bid price)
    pub fn best_bid(&self) -> Option<&OrderBookLevel> {
        self.bids.first()
    }

    /// Get best ask (lowest ask price)
    pub fn best_ask(&self) -> Option<&OrderBookLevel> {
        self.asks.first()
    }

    /// Convert to Level 1 quote
    pub fn to_level1(&self) -> Option<Level1Quote> {
        let bid = self.best_bid()?;
        let ask = self.best_ask()?;

        Some(Level1Quote {
            ts: self.ts,
            symbol: self.symbol.clone(),
            bid: bid.price,
            bid_size: bid.size,
            ask: ask.price,
            ask_size: ask.size,
        })
    }

    /// Calculate total bid volume
    pub fn total_bid_volume(&self) -> f64 {
        self.bids.iter().map(|level| level.size).sum()
    }

    /// Calculate total ask volume
    pub fn total_ask_volume(&self) -> f64 {
        self.asks.iter().map(|level| level.size).sum()
    }

    /// Calculate the order-book imbalance ratio (`bid_volume / total_volume`).
    ///
    /// Returns a value in `[0, 1]`. Values above 0.5 indicate more resting
    /// bid volume (buying interest); values below 0.5 indicate more ask
    /// volume (selling interest). Returns 0.5 when the book is empty.
    pub fn imbalance_ratio(&self) -> f64 {
        let bid_vol = self.total_bid_volume();
        let ask_vol = self.total_ask_volume();
        let total = bid_vol + ask_vol;

        if total == 0.0 {
            return 0.5;
        }

        bid_vol / total
    }

    /// Get mid price from best bid/ask
    pub fn mid_price(&self) -> Option<f64> {
        let bid = self.best_bid()?.price;
        let ask = self.best_ask()?.price;
        Some((bid + ask) / 2.0)
    }
}

/// Provider of real-time Level 1 (best bid/ask) quotes.
///
/// Implement this trait to supply top-of-book quotes. The chart does not
/// consume quote sources yet: the host application calls
/// [`poll_quotes`](Self::poll_quotes) to drain newly arrived quotes and
/// displays them itself.
pub trait QuoteSource: Send + Sync {
    /// Subscribe to Level 1 quotes for a symbol
    fn subscribe_quotes(&mut self, symbol: String) -> Result<(), QuoteSourceError>;

    /// Unsubscribe from Level 1 quotes
    fn unsubscribe_quotes(&mut self, symbol: String) -> Result<(), QuoteSourceError>;

    /// Poll for latest quotes
    fn poll_quotes(&mut self) -> Vec<Level1Quote>;

    /// Get latest quote for a symbol
    fn latest_quote(&self, symbol: &str) -> Option<&Level1Quote>;
}

/// Error returned by [`QuoteSource`] operations.
#[derive(Debug, Clone)]
pub enum QuoteSourceError {
    /// Symbol not found or invalid
    InvalidSymbol(String),
    /// Conn error
    ConnError(String),
    /// Subscription limit reached
    SubscriptionLimitReached,
    /// General error
    Other(String),
}

impl std::fmt::Display for QuoteSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSymbol(s) => write!(f, "Invalid symbol: {s}"),
            Self::ConnError(e) => write!(f, "Conn error: {e}"),
            Self::SubscriptionLimitReached => write!(f, "Subscription limit reached"),
            Self::Other(e) => write!(f, "Error: {e}"),
        }
    }
}

impl std::error::Error for QuoteSourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level1_quote_calculations() {
        let quote = Level1Quote {
            ts: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            bid: 50000.0,
            bid_size: 1.5,
            ask: 50010.0,
            ask_size: 2.0,
        };

        assert_eq!(quote.spread(), 10.0);
        assert_eq!(quote.mid_price(), 50005.0);
        assert_eq!(quote.total_size(), 3.5);

        // Spread in basis points: (10 / 50000) * 10000 = 2 bps
        assert!((quote.spread_bps() - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_order_book_imbalance() {
        let order_book = OrderBook {
            ts: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            bids: vec![
                OrderBookLevel {
                    price: 50000.0,
                    size: 3.0,
                    order_cnt: Some(5),
                },
                OrderBookLevel {
                    price: 49990.0,
                    size: 2.0,
                    order_cnt: Some(3),
                },
            ],
            asks: vec![
                OrderBookLevel {
                    price: 50010.0,
                    size: 1.0,
                    order_cnt: Some(2),
                },
                OrderBookLevel {
                    price: 50020.0,
                    size: 1.0,
                    order_cnt: Some(1),
                },
            ],
        };

        // Total bid: 5.0, total ask: 2.0, imbalance: 5/7 ≈ 0.714
        let imbalance = order_book.imbalance_ratio();
        assert!((imbalance - 0.714).abs() < 0.01);

        assert_eq!(order_book.total_bid_volume(), 5.0);
        assert_eq!(order_book.total_ask_volume(), 2.0);
    }

    #[test]
    fn test_order_book_to_level1() {
        let order_book = OrderBook {
            ts: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            bids: vec![OrderBookLevel {
                price: 50000.0,
                size: 1.5,
                order_cnt: None,
            }],
            asks: vec![OrderBookLevel {
                price: 50010.0,
                size: 2.0,
                order_cnt: None,
            }],
        };

        let level1 = order_book.to_level1().unwrap();
        assert_eq!(level1.bid, 50000.0);
        assert_eq!(level1.ask, 50010.0);
        assert_eq!(level1.bid_size, 1.5);
        assert_eq!(level1.ask_size, 2.0);
    }
}
