//! Price alerts the host keeps, evaluates and reports.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{MaybeSend, Reply};
use crate::model::{Alert, AlertId};

/// The host's alerts. The host stores and evaluates them, on its own prices or on a server; the
/// library shows what the host pushes, sends every change the user makes through
/// [`save`](Alerts::save) and [`remove`](Alerts::remove), and evaluates nothing.
///
/// The host is the source of truth. The terminal shows only the alerts and triggers that
/// [`watch`](Alerts::watch) pushed: an alert the user creates appears once the host pushes it, and
/// the terminal never marks an alert triggered on its own. When the host pushes
/// [`AlertEvent::Triggered`], the terminal shows a popup and an entry in the alert log; the status
/// it shows is the one the host last pushed in an [`AlertEvent::Upsert`]. Sound and delivery
/// outside the application (email, push notifications) are the host's: it can play a sound or send
/// a message when it pushes the trigger.
///
/// These methods are called on the UI thread, while the terminal shows a frame or runs a command,
/// and must return at once. Do the work elsewhere and answer through the [`Reply`] or push through
/// the [`Events`] you were given, from any thread, runtime or browser task. A `Reply` dropped
/// unanswered reports [`FeedError::Unanswered`](crate::data::FeedError::Unanswered).
///
/// ```
/// use std::sync::Mutex;
///
/// use egui_charts::data::Reply;
/// use egui_charts::model::{Alert, AlertId};
/// use egui_charts::services::{AlertEvent, Alerts, Events};
///
/// /// Alerts held in memory, lost when the application exits. Answers inline. A real host also
/// /// evaluates the alerts on its prices and pushes `AlertEvent::Triggered` when one fires.
/// #[derive(Default)]
/// struct InMemory(Mutex<State>);
///
/// #[derive(Default)]
/// struct State {
///     alerts: Vec<Alert>,
///     events: Option<Events<AlertEvent>>,
///     next_id: u64,
/// }
///
/// impl Alerts for InMemory {
///     fn watch(&self, events: Events<AlertEvent>) {
///         let mut state = self.0.lock().unwrap();
///         for alert in &state.alerts {
///             events.push(AlertEvent::Upsert(alert.clone()));
///         }
///         state.events = Some(events);
///     }
///
///     fn save(&self, mut alert: Alert, reply: Reply<()>) {
///         let mut state = self.0.lock().unwrap();
///         if alert.id.is_none() {
///             state.next_id += 1;
///             alert.id = Some(AlertId(state.next_id.to_string()));
///         }
///         state.alerts.retain(|kept| kept.id != alert.id);
///         state.alerts.push(alert.clone());
///         if let Some(events) = &state.events {
///             events.push(AlertEvent::Upsert(alert));
///         }
///         reply.send(Ok(()));
///     }
///
///     fn remove(&self, id: AlertId, reply: Reply<()>) {
///         let mut state = self.0.lock().unwrap();
///         state.alerts.retain(|kept| kept.id.as_ref() != Some(&id));
///         if let Some(events) = &state.events {
///             events.push(AlertEvent::Remove(id));
///         }
///         reply.send(Ok(()));
///     }
/// }
///
/// let alerts: std::sync::Arc<dyn Alerts> = std::sync::Arc::new(InMemory::default());
/// # let _ = alerts;
/// ```
pub trait Alerts: MaybeSend + 'static {
    /// Pushes the host's alerts, then every change and trigger. Called when the terminal starts and
    /// again after a lagged stream, each time with a new stream that starts from no alerts: push
    /// the current list first, then keep pushing until `events` is inactive. On the first call,
    /// also push the triggers that fired while the application was closed; do not push them again
    /// on a later call.
    ///
    /// The stream takes at most 10,000 pushes between two frames, the current list and the missed
    /// triggers included: send at most that many at once (for example only the most recent
    /// triggers), or the stream lags on every call and no alert shows.
    fn watch(&self, events: Events<AlertEvent>);

    /// Creates (`alert.id` is `None`) or replaces an alert. Answer `Ok` once the change is kept;
    /// the alert appears when `watch` pushes it, so push the saved alert, with its id, as
    /// [`AlertEvent::Upsert`].
    fn save(&self, alert: Alert, reply: Reply<()>);

    /// Deletes the alert `id`. Answer `Ok` once it is deleted and push [`AlertEvent::Remove`].
    fn remove(&self, id: AlertId, reply: Reply<()>);
}

/// A change to the host's alerts, pushed through the stream [`Alerts::watch`] received.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AlertEvent {
    /// Adds the alert, or replaces the one with its id. An alert without an id is ignored.
    Upsert(Alert),
    /// Removes the alert with this id, if any.
    Remove(AlertId),
    /// Removes every alert, so a host that reconnects can push its full list again.
    Clear,
    /// The alert fired at `time`, at `price` (finite) when the host knows it. The terminal shows a
    /// popup and logs it; push an `Upsert` as well when the alert's status changes.
    Triggered {
        id: AlertId,
        time: DateTime<Utc>,
        price: Option<f64>,
    },
}
