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
//! A feed pushes trades, the order book and Level 1 quotes into the chart through
//! [`Sink`](crate::data::Sink). A [`Quote`] summarises a symbol for watchlists, the
//! symbol's panels and the order ticket; the host streams it through the
//! [`Quotes`](crate::services::Quotes) service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single trade execution (time & sales entry).
///
/// Each tick represents one matched trade on the exchange, recording the
/// price, size, and aggressor side. Ticks are the finest-grained market
/// data and can be aggregated into bars, footprint charts, or volume
/// profiles. Build it with [`Tick::new`]. A trade names no symbol: the
/// [`Sink`](crate::data::Sink) it is pushed into stands for one instrument, so a
/// trade holds no heap data and pushing one allocates nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Tick {
    /// Ts of the trade
    pub ts: DateTime<Utc>,
    /// Trade price
    pub price: f64,
    /// Trade size/volume
    pub size: f64,
    /// Aggressor side, `None` when the feed does not classify the trade.
    pub side: Option<TradeSide>,
    /// Exchange trade id or sequence number, unique per symbol and never reused: a number that
    /// restarts each session, such as a daily sequence number, is not an id unless combined with
    /// the session date. With it, history and live trades join without losing or repeating a
    /// trade, and a live trade whose id was already taken is dropped.
    #[serde(default)]
    pub id: Option<u64>,
    /// Sale conditions the exchange reported, so studies and overlays can leave out prints such
    /// as off-exchange or auction trades.
    #[serde(default)]
    pub conditions: TradeConditions,
    /// Exchange aggressor order id, unique per symbol. Only explicit matching ids combine fills.
    #[serde(default)]
    pub aggressor_order_id: Option<u64>,
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
            conditions: TradeConditions::NONE,
            aggressor_order_id: None,
        }
    }

    /// Sets [`id`](Self::id).
    pub fn with_id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets [`aggressor_order_id`](Self::aggressor_order_id).
    pub fn with_aggressor_order_id(mut self, id: u64) -> Self {
        self.aggressor_order_id = Some(id);
        self
    }

    /// Sets [`conditions`](Self::conditions).
    pub fn with_conditions(mut self, conditions: TradeConditions) -> Self {
        self.conditions = conditions;
        self
    }
}

/// Aggressor side of a trade.
///
/// Indicates whether the trade was initiated by a buyer lifting the ask
/// or a seller hitting the bid. This is essential for computing delta
/// (buy volume minus sell volume) in footprint and order-flow analysis.
/// A trade of unknown side has no `TradeSide`: [`Tick::side`] is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    /// Buy side (aggressor was buyer)
    Buy,
    /// Sell side (aggressor was seller)
    Sell,
}

/// Sale conditions of a trade, as a set of flags. Combine them with `|` and test them with
/// [`intersects`](Self::intersects):
///
/// ```
/// use egui_charts::model::{Tick, TradeConditions};
///
/// let trade = Tick::new(chrono::Utc::now(), 100.0, 5.0, None)
///     .with_conditions(TradeConditions::AUCTION);
/// let leave_out = TradeConditions::OFF_EXCHANGE | TradeConditions::AUCTION;
/// assert!(trade.conditions.intersects(leave_out));
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TradeConditions(u16);

impl TradeConditions {
    /// A regular trade of continuous trading on the exchange.
    pub const NONE: Self = Self(0);
    /// Reported away from the exchange's book, such as by a dark pool or an over-the-counter
    /// dealer.
    pub const OFF_EXCHANGE: Self = Self(1);
    /// Printed by an opening, closing or reopening auction rather than by continuous trading.
    pub const AUCTION: Self = Self(1 << 1);
    /// Smaller than the instrument's round lot.
    pub const ODD_LOT: Self = Self(1 << 2);
    /// Reported late or out of sequence, after trades executed later than it.
    pub const OUT_OF_SEQUENCE: Self = Self(1 << 3);
    /// Executed outside the regular trading session.
    pub const EXTENDED_HOURS: Self = Self(1 << 4);

    /// Whether any condition of `other` is set.
    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl std::ops::BitOr for TradeConditions {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A host-supplied session price, independent of the regular close and hovered history.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SessionQuote {
    /// Session price. Renderers ignore non-finite values.
    pub price: f64,
    /// Short host-supplied qualifier shown beside the price.
    pub qualifier: String,
}

impl SessionQuote {
    /// Creates a session quote without deriving it from historical bars.
    pub fn new(price: f64, qualifier: impl Into<String>) -> Self {
        Self {
            price,
            qualifier: qualifier.into(),
        }
    }
}

/// The quote of one symbol: its last price and change from the previous close, the session's
/// volume, the best bid and ask, and the latest extended-hours trade. Fields the host does not
/// know stay `None`.
///
/// ```
/// use egui_charts::model::Quote;
///
/// let quote = Quote::new(105.0, 100.0, chrono::Utc::now()).with_bid_ask(104.99, 105.01);
/// assert_eq!((quote.change(), quote.change_percent()), (5.0, 5.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Quote {
    /// Last traded price of the regular session.
    pub last: f64,
    /// Close of the previous regular session.
    pub prev_close: f64,
    /// When the quote last changed.
    pub time: DateTime<Utc>,
    /// Volume traded in the current session.
    pub volume: Option<f64>,
    /// Best bid.
    pub bid: Option<f64>,
    /// Best ask.
    pub ask: Option<f64>,
    /// The latest trade before or after the regular session.
    pub extended: Option<ExtendedHoursQuote>,
}

impl Quote {
    /// A quote of `last`, changed from `prev_close`, at `time`.
    pub fn new(last: f64, prev_close: f64, time: DateTime<Utc>) -> Self {
        Self {
            last,
            prev_close,
            time,
            volume: None,
            bid: None,
            ask: None,
            extended: None,
        }
    }

    /// Sets [`volume`](Self::volume).
    pub fn with_volume(mut self, volume: f64) -> Self {
        self.volume = Some(volume);
        self
    }

    /// Sets [`bid`](Self::bid) and [`ask`](Self::ask).
    pub fn with_bid_ask(mut self, bid: f64, ask: f64) -> Self {
        (self.bid, self.ask) = (Some(bid), Some(ask));
        self
    }

    /// Sets [`extended`](Self::extended).
    pub fn with_extended(mut self, extended: ExtendedHoursQuote) -> Self {
        self.extended = Some(extended);
        self
    }

    /// `last` minus `prev_close`.
    pub fn change(&self) -> f64 {
        self.last - self.prev_close
    }

    /// The change divided by the absolute previous close, times 100; NaN when the previous close
    /// is 0.
    pub fn change_percent(&self) -> f64 {
        if self.prev_close == 0.0 {
            f64::NAN
        } else {
            self.change() / self.prev_close.abs() * 100.0
        }
    }
}

/// The session of an extended-hours trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtendedSession {
    /// Before the regular session opens.
    PreMarket,
    /// After the regular session closes.
    PostMarket,
}

/// The latest trade of a symbol outside its regular session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtendedHoursQuote {
    /// Whether the trade was before or after the regular session.
    pub session: ExtendedSession,
    /// Last traded price. Its change is measured from the regular session's last price,
    /// [`Quote::last`].
    pub last: f64,
    /// When `last` traded.
    pub time: DateTime<Utc>,
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

    /// Whether the best bid is above the best ask. Bids and asks overlap during pre-open and
    /// auction phases, while a venue publishes an indicative opening price, and in books
    /// consolidated from several venues, so a crossed book is valid data: the chart keeps it as
    /// the feed sends it. A best bid equal to the best ask is locked, not crossed.
    pub fn is_crossed(&self) -> bool {
        self.best_bid()
            .zip(self.best_ask())
            .is_some_and(|(bid, ask)| bid.price > ask.price)
    }

    /// Get mid price from best bid/ask
    pub fn mid_price(&self) -> Option<f64> {
        let bid = self.best_bid()?.price;
        let ask = self.best_ask()?.price;
        Some((bid + ask) / 2.0)
    }
}

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
