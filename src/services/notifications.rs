//! The host's log of alert triggers and other events for the user.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::MaybeSend;

/// Events the host tells the user about, such as alert triggers, filled orders or a lost
/// connection, for the notifications panel. The host keeps the log; the panel shows what the
/// service pushes, newest first.
///
/// [`watch`](Self::watch) is called on the UI thread and must return at once. Push from any
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
}

/// One entry of the log.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Notification {
    pub time: DateTime<Utc>,
    pub title: String,
    /// Detail shown under the title. May be empty.
    pub body: String,
    /// The symbol the entry is about, which the panel offers to open.
    pub symbol: Option<String>,
}

impl Notification {
    /// An entry with a title and nothing else: set the other fields as known.
    pub fn new(time: DateTime<Utc>, title: impl Into<String>) -> Self {
        Self {
            time,
            title: title.into(),
            body: String::new(),
            symbol: None,
        }
    }
}
