//! Services a host supplies, each through a small trait. Charts consume `Marks`, `Signals` and
//! `Posts` and keep their drawings in `Storage`, symbol choosers consume `SymbolSearch`, the
//! alerts panel `Alerts`, the watchlist `Quotes`, the notifications panel `Notifications` and the
//! chats panel `Chats`. `SettingsStore` and the layout menu keep settings and layouts in `Storage`;
//! other contracts define the host side of the assembled terminal. [`PaperAccount`] is a
//! ready-made [`Broker`] that simulates an account, and `AlertEngine`, with the `alert-engine`
//! feature, an [`Alerts`] service that evaluates alerts on the bars the host feeds it.
//! `OrderFlowDetectors`, with the `detectors` feature, is a [`Signals`] service that finds
//! order-flow events in the trades and depth the host feeds it.
//!
//! Like [`Datafeed`](crate::data::Datafeed), every service is held as an `Arc<dyn Trait>` and
//! called on the UI thread. Its methods return at once and answer later through a
//! [`Reply`](crate::data::Reply), or push updates through an [`Events`] stream, from any thread,
//! runtime or browser task.

#[cfg(feature = "alert-engine")]
mod alert_engine;
mod alerts;
mod broker;
mod chats;
#[cfg(feature = "detectors")]
mod detectors;
#[cfg(feature = "widget")]
pub(crate) mod document;
#[cfg_attr(
    not(any(test, feature = "widget")),
    expect(dead_code, reason = "only the terminal consumes the listener")
)]
mod events;
mod marks;
mod notifications;
mod paper;
mod posts;
#[cfg(feature = "widget")]
mod profile_directory;
mod profiles;
mod quotes;
mod search;
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
mod settings;
mod sharing;
mod signals;
mod storage;

#[cfg(feature = "alert-engine")]
#[cfg_attr(docsrs, doc(cfg(feature = "alert-engine")))]
pub use alert_engine::AlertEngine;
#[cfg(all(feature = "alert-engine", feature = "ui"))]
pub use alert_engine::AlertFeed;
pub use alerts::{AlertEvent, Alerts};
pub use broker::{AccountEvent, Broker, HistoryKind, HistoryPage, HistoryRequest};
pub use chats::{
    ChatCapabilities, ChatEvent, ChatMessage, ChatUser, Chats, NewChatRoom, NewChatRoomKind,
};
#[cfg(feature = "detectors")]
#[cfg_attr(docsrs, doc(cfg(feature = "detectors")))]
pub use detectors::{
    BigTradeRule, DetectionKind, Detector, DetectorSettings, OrderFlowDetectors, Threshold,
};
pub use events::Events;
#[cfg(feature = "widget")]
pub(crate) use events::Listener;
pub use marks::{Marks, MarksPage, MarksRequest};
pub use notifications::{Notification, NotificationTarget, Notifications};
pub use paper::PaperAccount;
pub use posts::Posts;
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub use profile_directory::{FollowOperation, FollowStatus, ProfileDirectory};
pub use profiles::{Profile, ProfileEvent, ProfileId, Profiles};
pub use quotes::Quotes;
pub use search::{ExpressionError, SymbolExpression, SymbolQuery, SymbolSearch};
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub use settings::{MarketHeatmapSelection, SettingsStore, UserSettings};
pub use sharing::{ShareKind, ShareLink, ShareOffer, Sharing};
pub use signals::{SignalEvent, Signals};
pub use storage::{ConflictChoice, Revision, SaveOutcome, Storage, StoreKind, Versioned};
