//! Services a host supplies, each through a small trait. Charts consume `Marks` and `Signals`,
//! symbol choosers `SymbolSearch`, the alerts panel `Alerts`, the notifications panel
//! `Notifications` and the chats panel `Chats`; other contracts define the host side of the
//! assembled terminal.
//!
//! Like [`Datafeed`](crate::data::Datafeed), every service is held as an `Arc<dyn Trait>` and
//! called on the UI thread. Its methods return at once and answer later through a
//! [`Reply`](crate::data::Reply), or push updates through an [`Events`] stream, from any thread,
//! runtime or browser task.

mod alerts;
mod broker;
mod chats;
#[cfg_attr(
    not(any(test, feature = "widget")),
    expect(dead_code, reason = "only the terminal consumes the listener")
)]
mod events;
mod marks;
mod notifications;
mod search;
mod signals;
mod storage;
mod targets;

pub use alerts::{AlertEvent, Alerts};
pub use broker::{AccountEvent, Broker};
pub use chats::{ChatEvent, ChatMessage, Chats};
pub use events::Events;
#[cfg(feature = "widget")]
pub(crate) use events::Listener;
pub use marks::{Marks, MarksPage, MarksRequest};
pub use notifications::{Notification, Notifications};
pub use search::{SymbolQuery, SymbolSearch};
pub use signals::{SignalEvent, Signals};
pub use storage::{Storage, StoreKind};
pub use targets::PriceTargets;
