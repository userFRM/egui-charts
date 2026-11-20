//! The host's log of alert triggers and other events for the user.

use chrono::{DateTime, Utc};

use super::{Events, ProfileId};
use crate::data::{MaybeSend, Reply};

/// Events the host tells the user about, such as alert triggers, filled orders or a lost
/// connection, for the notifications panel. The host keeps the log; the panel shows what the
/// service pushes, newest first.
///
/// Methods are called on the UI thread and must return at once. Push and answer from any
/// thread, runtime or browser task.
///
/// ```
/// use egui_charts::services::{Events, Notification, Notifications};
///
/// /// Tells the user once that the session started.
/// struct Session;
///
/// impl Notifications for Session {
///     fn watch(&self, events: Events<Notification>) {
///         events.push(Notification::new(chrono::Utc::now(), "Session started"));
///     }
/// }
/// ```
pub trait Notifications: MaybeSend + 'static {
    /// Pushes the recent notifications, oldest first, then each new one until `events` is
    /// inactive. Called when the panel starts and again after a lagged stream, each time with a
    /// new stream that starts from none.
    ///
    /// The stream takes at most 10,000 pushes between two frames: push at most that many recent
    /// notifications at once, or the stream lags on every call and none shows.
    fn watch(&self, events: Events<Notification>);

    /// Shows `notification` on the desktop, asking the user's permission first where the
    /// platform needs it. The alerts panel calls it for each trigger popup once the host passes
    /// this service to its `set_desktop`. Return at once and answer through `reply` once shown,
    /// or with the reason the permission was refused or the delivery failed, such as
    /// [`FeedError::Other`](crate::data::FeedError::Other): the user sees it. The default answers
    /// [`FeedError::NotSupported`](crate::data::FeedError::NotSupported).
    fn deliver(&self, notification: Notification, reply: Reply<()>) {
        let _ = notification;
        reply.send(Err(crate::data::FeedError::NotSupported));
    }
}

/// One entry of the log.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Notification {
    /// UTC timestamp supplied by the host.
    pub time: DateTime<Utc>,
    /// Display title.
    pub title: String,
    /// Detail shown under the title. May be empty.
    pub body: String,
    /// The symbol the entry is about, which the panel offers to open.
    pub symbol: Option<String>,
    /// The profile, chat room or post a social entry is about, which the panel offers to open.
    pub target: Option<NotificationTarget>,
}

/// What a social notification opens.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotificationTarget {
    /// The profile of a user, as [`Profiles`](super::Profiles) knows it.
    Profile(ProfileId),
    /// A chat room, by the identifier [`Chats`](super::Chats) uses.
    Room(String),
    /// An idea or short post, by the host's identifier: the feed entry identifier the ideas
    /// panel opens and the link [`Posts`](super::Posts) resolves.
    Post(String),
}

impl Notification {
    /// An entry with a title and nothing else: set the other fields as known.
    pub fn new(time: DateTime<Utc>, title: impl Into<String>) -> Self {
        Self {
            time,
            title: title.into(),
            body: String::new(),
            symbol: None,
            target: None,
        }
    }
}
