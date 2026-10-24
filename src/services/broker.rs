//! Placing orders with the host's broker.

use super::Events;
use crate::data::{FeedError, FeedStatus, MaybeSend, Reply};
use crate::model::{Account, Execution, Order, OrderId, OrderRequest, Position, TradingRules};

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
///     Account, Capabilities, Execution, Order, OrderId, OrderRequest, OrderSide, OrderStatus,
///     OrderType, Position,
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
    /// The terminal calls it when it starts and again, with a new stream, after the stream lagged;
    /// each stream starts from nothing, so push the whole current state again. The old stream is
    /// inactive from then on: push into the newest one only.
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

    /// Answers how `symbol` trades: its quantity step, minimum and maximum quantity, point value
    /// and times in force. By default the answer is [`FeedError::NotSupported`], and the order
    /// ticket assumes [`TradingRules::new`].
    fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
        let _ = symbol;
        reply.send(Err(FeedError::NotSupported));
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
    /// One fill.
    Execution(Execution),
    /// The connection to the broker.
    Status(FeedStatus),
}
