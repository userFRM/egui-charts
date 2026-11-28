//! Price alerts the host keeps, evaluates and reports.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{MaybeSend, Reply};
use crate::model::{Alert, AlertId, AlertTrigger};

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
/// outside the application (email, plain-text email, push notifications, webhooks) are the
/// host's: it can play a sound or send a message when it pushes the trigger, with the subject,
/// body and push title that [`AlertNotice`] builds from the alert and the trigger. The example of
/// `AlertEngine` sends each of them from its `on_trigger` callback.
///
/// Evaluation is the host's, so these rules are the contract between the library that edits an
/// alert and the service that runs it. A channel is inclusive at both boundaries. The state
/// conditions [`InsideChannel`](crate::model::AlertCondition::InsideChannel) and
/// [`OutsideChannel`](crate::model::AlertCondition::OutsideChannel) hold on every evaluation
/// that sees a value in that state, so pair them with a frequency that limits repeats, whereas
/// entering and exiting hold only on the evaluation that crosses a boundary. A crossing,
/// entering or exiting rule of an alert created or changed by [`save`](Alerts::save) needs a
/// value observed after the save on the side it leaves: a move made before the save never fires it.
/// [`AlertFrequency::OncePerMinute`](crate::model::AlertFrequency::OncePerMinute) allows one
/// trigger per UTC clock minute, from `hh:mm:00` up to but excluding the next minute, whatever the
/// chart's interval. An alert's [`schedule`](crate::model::Alert::schedule) allows triggers only when
/// [`NotificationSchedule::allows`](crate::model::NotificationSchedule::allows) accepts the
/// trigger time. Outside it, keep observing values for crossings without consuming an occurrence
/// or disarming an alert that fires once. A [`Position`](crate::model::AlertCondition::Position)
/// fires on its entry and its first exit and keeps its progress in the condition: push it with
/// its new progress along with each trigger, also outside the schedule, where progress still
/// moves, and when an edit of its levels starts it again.
///
/// A [`StrategyFills`](crate::model::AlertCondition::StrategyFills) alert receives each execution
/// of a host-named run through `AlertEngine::strategy_fills`, and each alert its strategy raises
/// through `AlertEngine::strategy_alerts`, with `strategies` and `alert-engine`.
/// Its event carries the fill time, execution price and a message such as
/// "Buy 2 at 187.50, position 4", or the order's alert message. The host supplies bars and
/// retains the run; delivery outside the application remains the host's responsibility. In the same way a
/// [`DetectorSignals`](crate::model::AlertCondition::DetectorSignals) alert fires on each new
/// signal of a named detector, and a [`CustomEvents`](crate::model::AlertCondition::CustomEvents)
/// alert on each event a named host source raises, which a service without that source refuses
/// to save.
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
///             events.push(AlertEvent::Upsert(Box::new(alert.clone())));
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
///             events.push(AlertEvent::Upsert(Box::new(alert)));
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
    /// The local evaluator, when this service uses one. The assembled terminal feeds it from
    /// its datafeed, including subjects that no live chart shows. Server-backed services keep
    /// the default so evaluation remains on the server.
    #[cfg(feature = "alert-engine")]
    fn engine(&self) -> Option<&super::AlertEngine> {
        None
    }

    /// Pushes the host's alerts, then every change and trigger. Called when the terminal starts and
    /// again after a lagged stream, each time with a new stream that starts from no alerts: push
    /// the current list first, then keep pushing until `events` is inactive. On the first call,
    /// also push the triggers that fired while the application was closed: as
    /// [`AlertEvent::Logged`] to add them to the log without popups, or as triggers to show a popup
    /// for each. Do not push them again on a later call. `AlertEngine` pushes the history its host
    /// restored this way.
    ///
    /// The stream takes at most 10,000 pushes between two frames, the current list and the missed
    /// triggers included: send at most that many at once (for example only the most recent
    /// triggers), or the stream lags on every call and no alert shows.
    fn watch(&self, events: Events<AlertEvent>);

    /// Creates (`alert.id` is `None`) or replaces an alert. Keep `alert.inputs` with the alert:
    /// its independent study parameters and source view must survive copies and restoration.
    /// Preserve display choices and creation time on edits, and retain the latest firing time
    /// on every trigger, including repeats, so the manager and chart can restore their state.
    /// Answer `Ok` once the change is kept;
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
    Upsert(Box<Alert>),
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
    /// The alert's conditions held outside its notification schedule. Log it, marked as silent,
    /// without a popup, sound or delivery. It leaves the alert's status and firing time as they
    /// are, so a once alert stays armed, except that a position alert
    /// still moves on and ends at its exit.
    SilentTriggered {
        /// Identifier of the alert whose conditions held.
        id: AlertId,
        /// Symbol whose live source satisfied the conditions.
        symbol: String,
        /// UTC instant of the trigger.
        time: DateTime<Utc>,
        /// Price observed by the host when available.
        price: Option<f64>,
        /// The alert's message with its placeholders filled in, empty if the alert has none.
        message: String,
    },
    /// A trigger from the service's history, such as one that fired before a restart. The panel
    /// adds it to the log as it is, silent or not, without a popup, sound or delivery, and leaves
    /// the alert's status and firing time as they are.
    Logged(AlertTrigger),
}

/// What a host sends for one trigger by email, plain-text email, mobile push or webhook, built
/// from the alert and its [`AlertEvent::TriggeredForSymbol`] as `AlertEngine::on_trigger` hands
/// them over, so that every host sends the same content for the same alert. The alert's
/// [`delivery`](Alert::delivery) says which channels to use and where: an
/// [`email`](crate::model::AlertDelivery::email) to the account's address, a
/// [`plain_text`](crate::model::AlertDelivery::plain_text) email to the
/// [`plain_text_address`](crate::model::AlertDelivery::plain_text_address) without surrounding
/// spaces, or to the account's alternative address when it is empty, and a post to the
/// [`webhook_url`](crate::model::AlertDelivery::webhook_url) with
/// [`content_type`](Self::content_type).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AlertNotice {
    /// The symbol and the alert's name, or its first condition without one, such as
    /// "AAPL: Crossing up 100". Control characters, such as line breaks, read as spaces.
    pub subject: String,
    /// The subject cut to at most 50 characters, ending in "…" when cut, for a push title.
    pub push_title: String,
    /// The email, push and webhook body: the message with its placeholders filled in, or without
    /// one a description of the trigger, such as
    /// "AAPL: Crossing up 100 at 2026-01-01T10:00:00+00:00, price 101.5", without the price when
    /// the trigger has none.
    pub body: String,
    /// The webhook's `Content-Type`: `application/json` when the body is a valid JSON text,
    /// including an object, array, number, string, boolean or `null`, with surrounding whitespace
    /// allowed; otherwise `text/plain; charset=utf-8`.
    pub content_type: &'static str,
}

impl AlertNotice {
    /// The notice of `event` on `alert`, or `None` for any event other than
    /// [`AlertEvent::TriggeredForSymbol`]: a silent trigger is not delivered, and
    /// [`AlertEvent::Triggered`] names no symbol or message.
    pub fn new(alert: &Alert, event: &AlertEvent) -> Option<Self> {
        let AlertEvent::TriggeredForSymbol {
            symbol,
            time,
            price,
            message,
            ..
        } = event
        else {
            return None;
        };
        let name = alert.name.trim();
        let title = if name.is_empty() {
            alert.condition.to_string()
        } else {
            name.to_owned()
        };
        // A user-written name must not add mail headers.
        let subject: String = format!("{symbol}: {title}")
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let push_title = if subject.chars().count() > 50 {
            subject.chars().take(49).chain(['…']).collect()
        } else {
            subject.clone()
        };
        let body = if message.trim().is_empty() {
            let price = price
                .filter(|price| price.is_finite())
                .map(|price| format!(", price {price}"))
                .unwrap_or_default();
            format!(
                "{symbol}: {} at {}{price}",
                alert.condition,
                time.to_rfc3339()
            )
        } else {
            message.clone()
        };
        let json = serde_json::from_str::<serde_json::Value>(&body).is_ok();
        Some(Self {
            subject,
            push_title,
            body,
            content_type: if json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AlertCondition;

    #[test]
    fn notice_gives_each_channel_its_subject_title_body_and_content_type() {
        const S: &str = "NYSE:IBM: Crossing up 100";
        const DESCRIBED: &str = "NYSE:IBM: Crossing up 100 at 2026-01-01T10:00:00+00:00";
        const T: &str = "text/plain; charset=utf-8";
        const J: &str = "application/json";
        // Fifty characters, more bytes: a push title keeps it whole.
        const FIFTY: &str = "NYSE:IBM: Percée au-dessus de la moyenne mobile 50";
        // A cut after 49 bytes would split the second "é".
        const LONG: &str = "Percée au-dessus du plus haut de la séance d'ouverture";
        let rows = [
            // Name, message, price, subject, push title, body, content type.
            (
                "",
                "",
                Some(101.5),
                S,
                S,
                "NYSE:IBM: Crossing up 100 at 2026-01-01T10:00:00+00:00, price 101.5",
                T,
            ),
            ("  ", " \n", Some(f64::NAN), S, S, DESCRIBED, T),
            (
                "Percée au-dessus de la moyenne mobile 50",
                "",
                None,
                FIFTY,
                FIFTY,
                DESCRIBED,
                T,
            ),
            (
                "Line\r\nBcc: x",
                r#"{"close": 101.5}"#,
                None,
                "NYSE:IBM: Line  Bcc: x",
                "NYSE:IBM: Line  Bcc: x",
                r#"{"close": 101.5}"#,
                J,
            ),
            (
                LONG,
                " [1, 2] \n",
                None,
                "NYSE:IBM: Percée au-dessus du plus haut de la séance d'ouverture",
                "NYSE:IBM: Percée au-dessus du plus haut de la séa…",
                " [1, 2] \n",
                J,
            ),
            ("", "42", None, S, S, "42", J),
            ("", r#""text""#, None, S, S, r#""text""#, J),
            ("", "true", None, S, S, "true", J),
            ("", "false", None, S, S, "false", J),
            ("", "null", None, S, S, "null", J),
            ("", " null \n", None, S, S, " null \n", J),
            ("", "plain", None, S, S, "plain", T),
            ("", "42 43", None, S, S, "42 43", T),
            ("", r#""text"#, None, S, S, r#""text"#, T),
            ("", r#"{"close": NaN}"#, None, S, S, r#"{"close": NaN}"#, T),
            ("", "Closed at 101.5", None, S, S, "Closed at 101.5", T),
        ];
        let time = DateTime::from_timestamp(1_767_261_600, 0).unwrap();
        let alert = |name: &str| {
            let mut alert = Alert::new("NYSE:IBM", AlertCondition::CrossingUp(100.0));
            alert.name = name.into();
            alert
        };
        for (index, (name, message, price, subject, push_title, body, content_type)) in
            rows.into_iter().enumerate()
        {
            let event = AlertEvent::TriggeredForSymbol {
                id: AlertId("1".into()),
                symbol: "NYSE:IBM".into(),
                time,
                price,
                message: message.into(),
            };
            let notice = AlertNotice::new(&alert(name), &event).unwrap();
            assert_eq!(
                (
                    notice.subject.as_str(),
                    notice.push_title.as_str(),
                    notice.body.as_str(),
                    notice.content_type,
                ),
                (subject, push_title, body, content_type),
                "row {index}"
            );
        }
        let silent = AlertEvent::SilentTriggered {
            id: AlertId("1".into()),
            symbol: "NYSE:IBM".into(),
            time,
            price: Some(101.5),
            message: "Closed at 101.5".into(),
        };
        assert_eq!(AlertNotice::new(&alert(""), &silent), None, "not delivered");
    }
}
