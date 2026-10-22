//! Services a host supplies, each through a small trait. Nothing in the crate calls them yet;
//! they define the contract for the assembled terminal.
//!
//! Like [`Datafeed`](crate::data::Datafeed), every service is held as an `Arc<dyn Trait>` and
//! called on the UI thread. Its methods return at once and answer later through a
//! [`Reply`](crate::data::Reply), or push updates through an [`Events`] stream, from any thread,
//! runtime or browser task.

mod alerts;
mod broker;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the terminal consumes the listener")
)]
mod events;
mod search;
mod signals;
mod storage;

pub use alerts::{AlertEvent, Alerts};
pub use broker::{AccountEvent, Broker};
pub use events::Events;
pub use search::{SymbolQuery, SymbolSearch};
pub use signals::{SignalEvent, Signals};
pub use storage::{Storage, StoreKind};
