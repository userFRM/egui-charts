//! Price alerts the host keeps, evaluates and reports.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{MaybeSend, Reply};
use crate::model::{Alert, AlertId};

/// The host's alerts. The host stores and evaluates them, on its own prices or on a server; the
/// library shows what the host pushes and sends every change the user makes through
/// [`save`](Alerts::save) and [`remove`](Alerts::remove). With the `alert-engine` feature,
/// `services::AlertEngine` is a ready-made implementation that evaluates
/// the alerts on the bars the host feeds it; without it the library evaluates nothing.
///
/// The host is the source of truth. The terminal shows only the alerts and triggers that
/// [`watch`](Alerts::watch) pushed: an alert the user creates appears once the host pushes it, and
/// the terminal never marks an alert triggered on its own. When the host pushes
/// [`AlertEvent::Triggered`], the terminal shows a popup and an entry in the alert log; the status
/// it shows is the one the host last pushed in an [`AlertEvent::Upsert`]. Sound and delivery
/// outside the application (email, push notifications) are the host's: it can play a sound or send
/// a message when it pushes the trigger.
///
/// Evaluation is the host's, so these rules are the contract between the library that edits an
/// alert and the service that runs it. A channel is inclusive at both boundaries. The state
/// conditions [`InsideChannel`](crate::model::AlertCondition::InsideChannel) and
/// [`OutsideChannel`](crate::model::AlertCondition::OutsideChannel) hold on every evaluation
/// while the value is in that state, so pair them with a frequency that limits repeats, whereas
/// entering and exiting hold only on the evaluation that crosses a boundary.
/// [`AlertFrequency::OncePerMinute`](crate::model::AlertFrequency::OncePerMinute) allows one
/// trigger per UTC clock minute, from `hh:mm:00` up to but excluding the next minute, whatever the
/// chart's interval. An alert's [`schedule`](crate::model::Alert::schedule) limits notifications
/// only: evaluate and trigger the alert as usual, and deliver the notification when
/// [`NotificationSchedule::allows`](crate::model::NotificationSchedule::allows) accepts the
/// trigger time.
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
        /// Identifier of the alert that fired.
        id: AlertId,
        /// UTC instant of the trigger.
        time: DateTime<Utc>,
        /// Price observed by the host when the alert fired, when available.
        price: Option<f64>,
    },
    /// A trigger with its actual subject, including a member of a watchlist alert.
    /// The panel retains this symbol even after the alert or watchlist changes.
    TriggeredForSymbol {
        /// Identifier of the alert that fired.
        id: AlertId,
        /// Symbol whose live source satisfied the conditions.
        symbol: String,
        /// UTC instant of the trigger.
        time: DateTime<Utc>,
        /// Price observed by the host when available.
        price: Option<f64>,
        /// The alert's message with its placeholders filled in, as [`Alert::expand_message`]
        /// does, empty if the alert has none. The terminal shows it in the popup and the log.
        message: String,
    },
}
