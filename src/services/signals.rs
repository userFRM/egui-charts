//! Markers, levels and zones from the host's analytics.

use super::Events;
use crate::data::MaybeSend;
use crate::model::{Signal, SignalId, Timeframe};

/// The host's analytics, shown on the chart as markers, levels and zones. The library computes
/// nothing: it draws what the service pushes, lets the user hide signals by category, and reports
/// clicks.
///
/// Methods are called on the UI thread and must return at once. Compute elsewhere and push from
/// any thread, runtime or browser task.
///
/// ```
/// use egui_charts::model::{Direction, Signal, SignalId, SignalShape, Timeframe};
/// use egui_charts::services::{Events, SignalEvent, Signals};
/// use std::time::Duration;
///
/// /// Pushes a marker every second, in place of a host's own analysis.
/// struct Breakouts;
///
/// impl Signals for Breakouts {
///     fn subscribe(&self, symbol: &str, _: Timeframe, events: Events<SignalEvent>) {
///         let symbol = symbol.to_owned();
///         std::thread::spawn(move || {
///             let mut count = 0;
///             while events.is_active() {
///                 std::thread::sleep(Duration::from_secs(1));
///                 count += 1;
///                 events.push(SignalEvent::Upsert(Signal {
///                     id: SignalId(format!("{symbol}-breakout-{count}")),
///                     shape: SignalShape::Marker {
///                         time: chrono::Utc::now(),
///                         price: None,
///                         direction: Direction::Up,
///                     },
///                     category: "Breakouts".into(),
///                     label: Some("B".into()),
///                     tooltip: Some(format!("Breakout {count} on {symbol}")),
///                 }));
///             }
///         });
///     }
/// }
/// ```
pub trait Signals: MaybeSend + 'static {
    /// Starts pushing the signals of `symbol` at `timeframe` into `events` until it is inactive.
    /// Push the current signals first: each subscription starts from none. The terminal subscribes
    /// again on every symbol or interval change and after the stream lagged, each time with a new
    /// stream, and stops the old one.
    ///
    /// The stream takes at most 10,000 pushes between two frames, the current signals included:
    /// send at most that many at once (for example the most recent ones, or those near the loaded
    /// range), or the stream lags on every subscription and no signal shows.
    fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>);

    /// The user clicked the signal `id`, which this service pushed. The default does nothing.
    fn clicked(&self, id: &SignalId) {
        let _ = id;
    }
}

/// A change to the signals of one subscription.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SignalEvent {
    /// Adds the signal, or replaces the one with its id.
    Upsert(Signal),
    /// Removes the signal with this id, if any.
    Remove(SignalId),
    /// Removes every signal of the subscription.
    Clear,
}
