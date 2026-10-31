//! Placing orders with the host's broker.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{FeedError, FeedStatus, MaybeSend, Reply};
use crate::model::{
    Account, BalanceChange, Execution, JournalEntry, Order, OrderId, OrderPreview, OrderRequest,
    Position, TradingRules,
};

/// One trading account at the host's broker, real or simulated. The library holds no balances or
/// positions: it shows what the broker pushes and places the orders the user confirms. Paper
/// trading is an implementation of this trait. One value is one account; several accounts are
/// several values.
///
/// Methods are called on the UI thread and must return at once. Talk to the broker elsewhere and
/// answer or push from any thread, runtime or browser task.
///
/// Every price and quantity pushed must be finite. Quantities are positive, except a position's,
/// which is signed, and an order's filled quantity, which runs from 0 to the requested quantity;
/// the terminal skips events that break this. Prices are in the symbol's price units and
/// quantities in its trading units.
///
/// A paper account that trades one symbol and fills market orders at a fixed price:
///
/// ```
/// use chrono::Utc;
/// use egui_charts::data::{FeedError, Reply};
/// use egui_charts::model::{
///     Account, Capabilities, Execution, Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType, Position,
/// };
/// use egui_charts::services::{AccountEvent, Broker, Events};
/// use std::sync::Mutex;
///
/// const SYMBOL: &str = "AAPL";
/// const PRICE: f64 = 100.0;
///
/// #[derive(Default)]
/// struct Paper {
///     events: Mutex<Option<Events<AccountEvent>>>,
///     /// Orders placed so far, and the net quantity held.
///     state: Mutex<(u64, f64)>,
/// }
///
/// impl Broker for Paper {
///     fn connect(&self, events: Events<AccountEvent>) {
///         let mut capabilities = Capabilities::default();
///         capabilities.market = true;
///         events.push(AccountEvent::Account(Account::new("USD", 10_000.0, capabilities)));
///         // Market orders fill at once, so no order is working; a position is pushed when one is
///         // open.
///         let held = self.state.lock().unwrap().1;
///         if held != 0.0 {
///             events.push(AccountEvent::Position(Position::new(SYMBOL, held, PRICE)));
///         }
///         *self.events.lock().unwrap() = Some(events);
///     }
///
///     fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
///         if order.order_type != OrderType::Market || order.symbol != SYMBOL {
///             return reply.send(Err(FeedError::NotSupported));
///         }
///         let (id, held) = {
///             let mut state = self.state.lock().unwrap();
///             state.0 += 1;
///             state.1 += match order.side {
///                 OrderSide::Buy => order.quantity,
///                 OrderSide::Sell => -order.quantity,
///             };
///             (OrderId(format!("paper-{}", state.0)), state.1)
///         };
///         reply.send(Ok(id.clone()));
///
///         let Some(events) = self.events.lock().unwrap().clone() else {
///             return;
///         };
///         let execution = Execution::new(id.clone(), Utc::now(), order.side, PRICE, order.quantity);
///         let position = Position::new(order.symbol.clone(), held, PRICE);
///         let mut filled = Order::new(id, order, OrderStatus::Filled);
///         filled.filled = filled.request.quantity;
///         events.push(AccountEvent::Order(filled));
///         events.push(AccountEvent::Execution(execution));
///         events.push(AccountEvent::Position(position));
///     }
///
///     fn cancel(&self, id: OrderId, reply: Reply<()>) {
///         reply.send(Err(FeedError::Other(format!("order {id} is already filled"))));
///     }
/// }
/// ```
pub trait Broker: MaybeSend + 'static {
    /// Pushes the account (balance, currency, capabilities) first, then the working orders and
    /// open positions, then orders, positions, executions and connection status as they change.
    /// A broker that keeps history pushes it after the open state: the finished orders, the
    /// balance changes and the journal, oldest first, which the account panel lists.
    /// The terminal calls it when it starts and again, with a new stream, after the stream lagged;
    /// each stream starts from nothing, so push the whole current state again. The old stream is
    /// inactive from then on: push into the newest one only.
    ///
    /// A stream starts as connected. While the connection to the broker is not live, push
    /// [`AccountEvent::Status`] with [`FeedStatus::Connecting`] or [`FeedStatus::Disconnected`],
    /// on each new stream too, and [`FeedStatus::Live`] once it is live again; until then the
    /// account panel and the order ticket mark the broker and the terminal reports it as not
    /// connected. A broker that never pushes a status counts as connected. Meanwhile the terminal
    /// sends no order, cancellation or change: it reports the ones the user confirms as refused,
    /// and a position order confirmed earlier that still waits for its trading rules too, and
    /// sends none of them later. It asks for [`rules`](Self::rules) and
    /// [`history`](Self::history) only while the broker is live, and once it is live again it asks
    /// again for those that were on their way when it stopped being live, or that failed meanwhile
    /// or with a [recoverable](FeedError::is_recoverable) error.
    fn connect(&self, events: Events<AccountEvent>);

    /// Sends `order` to the broker. The terminal calls this only after the user confirmed the
    /// order, or the host performed the place order command itself, so do not ask again. Answer
    /// with the broker's id once the order is accepted, or with a
    /// [`FeedError`](crate::data::FeedError) the terminal shows. The order's
    /// [`AccountEvent::Order`] may arrive before or after that answer.
    fn place(&self, order: OrderRequest, reply: Reply<OrderId>);

    /// Cancels the working order `id`. The terminal calls this only after the user confirmed, so
    /// do not ask again. Answer once the broker accepted the cancellation, or with a
    /// [`FeedError`](crate::data::FeedError) the terminal shows; the order's
    /// [`OrderStatus::Cancelled`](crate::model::OrderStatus::Cancelled) update may arrive before
    /// or after that answer.
    fn cancel(&self, id: OrderId, reply: Reply<()>);

    /// Replaces the working order `id` with `order`, which differs in its prices or brackets,
    /// after the user dragged one of its lines, added or removed a take profit or stop loss, and
    /// confirmed. Called only for an account whose [`Capabilities::modify`](crate::model::Capabilities::modify) is set. Answer
    /// once the broker accepted the change; push the changed order as an [`AccountEvent::Order`].
    /// By default every change is refused as [`FeedError::NotSupported`].
    fn modify(&self, id: OrderId, order: OrderRequest, reply: Reply<()>) {
        let _ = (id, order);
        reply.send(Err(FeedError::NotSupported));
    }

    /// Estimates what `order` would cost without placing it. Called only for an account whose
    /// [`Capabilities::preview`](crate::model::Capabilities::preview) is set, when the user asks
    /// to preview an order; the terminal places the previewed order only after the user confirms
    /// it. By default every preview is refused as [`FeedError::NotSupported`].
    fn preview(&self, order: OrderRequest, reply: Reply<OrderPreview>) {
        let _ = order;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Answers how `symbol` trades: its quantity step, minimum and maximum quantity, point value
    /// and times in force. By default the answer is [`FeedError::NotSupported`], and the order
    /// ticket assumes [`TradingRules::new`].
    fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
        let _ = symbol;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Answers one page of finished history older than what [`connect`](Self::connect) pushed, for
    /// the kind and range of `request`. By default the answer is [`FeedError::NotSupported`], and
    /// only the pushed history exists.
    ///
    /// A page holds [`AccountEvent::Order`] with a finished status, [`AccountEvent::Execution`],
    /// [`AccountEvent::Balance`] or [`AccountEvent::Journal`] events, as the request's
    /// [`HistoryKind`] says, oldest first, and never working orders or positions: those come only
    /// from the pushed stream. The first page of a request holds the newest entries of its range
    /// and each page after it, asked for with the [`next`](HistoryPage::next) cursor, the entries
    /// before the previous one. Entries that were pushed live may appear again in a page: an order
    /// is matched by its id and other events by value, so repeat them unchanged. Answer for the
    /// account `connect` last served; a consumer drops an answer that arrives after the account
    /// changed.
    ///
    /// The account panel asks once the account arrived, while the broker is live, and the user
    /// scrolls to the end of a history: first for the range that ends one second after the oldest
    /// entry it lists (for an order, when it closed), so that entries sharing that time are not
    /// skipped, then for each `next` until it is `None` or the history lists 1,000 entries. After
    /// an error, which it shows unless it is `NotSupported`, it asks for that history again only
    /// after the next `connect`; after a [recoverable](FeedError::is_recoverable) error or one that
    /// arrives while the broker reports that it is not live, it asks again once the broker
    /// reports that it is live again.
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let _ = request;
        reply.send(Err(FeedError::NotSupported));
    }
}

/// Which finished history [`Broker::history`] is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HistoryKind {
    /// Finished orders, as [`AccountEvent::Order`].
    Orders,
    /// Fills, as [`AccountEvent::Execution`].
    Executions,
    /// Changes of the cash balance, as [`AccountEvent::Balance`].
    Balance,
    /// Lines of the trading journal, as [`AccountEvent::Journal`].
    Journal,
}

/// A request for one page of older history.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HistoryRequest {
    /// The history asked for.
    pub kind: HistoryKind,
    /// Inclusive start of the range, in UTC; `None` reaches back as far as the broker keeps.
    pub from: Option<DateTime<Utc>>,
    /// Exclusive end of the range, in UTC; `None` ends at the newest entry.
    pub to: Option<DateTime<Utc>>,
    /// The [`next`](HistoryPage::next) of the previous page of this request; `None` for the first.
    pub cursor: Option<String>,
    /// The number of entries wanted, at least 1. The page may hold fewer, and more only if
    /// entries share a timestamp.
    pub limit: u32,
}

impl HistoryRequest {
    /// The first page of `kind` over all the broker keeps, of at most `limit` entries.
    pub fn new(kind: HistoryKind, limit: u32) -> Self {
        Self {
            kind,
            from: None,
            to: None,
            cursor: None,
            limit,
        }
    }
}

/// One page of older history.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct HistoryPage {
    /// The entries, oldest first.
    pub events: Vec<AccountEvent>,
    /// The cursor of the page before this one, or `None` when the range holds nothing older.
    pub next: Option<String>,
}

impl HistoryPage {
    /// A page of `events` with the cursor of the page before it.
    pub fn new(events: Vec<AccountEvent>, next: Option<String>) -> Self {
        Self { events, next }
    }
}

/// A change to the account a [`Broker`] reports.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AccountEvent {
    /// The account's balance, currency or capabilities.
    Account(Account),
    /// Adds the order, or replaces the one with its id.
    Order(Order),
    /// Adds the position in its symbol, or replaces it; a quantity of 0 removes it.
    Position(Position),
    /// One fill, drawn on charts of its order's symbol: push the order first.
    Execution(Execution),
    /// The connection to the broker; a stream starts as [`FeedStatus::Live`]. See
    /// [`Broker::connect`].
    Status(FeedStatus),
    /// A change of the cash balance, for the balance history.
    Balance(BalanceChange),
    /// A line of the trading journal.
    Journal(JournalEntry),
}
