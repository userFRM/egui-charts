//! Quotes of many symbols at once, for watchlists, the symbol's panels and the order ticket.

use super::Events;
use crate::data::MaybeSend;
use crate::model::Quote;

/// The host's quote stream. The watchlist subscribes to the symbols of its active list and the
/// chart's symbol, and hands the chart symbol's quote to the symbol details panel, the symbol
/// header and the order ticket.
///
/// Methods are called on the UI thread and must return at once. Push from any thread, runtime or
/// browser task.
///
/// ```
/// use egui_charts::model::Quote;
/// use egui_charts::services::{Events, Quotes};
/// use std::time::Duration;
///
/// /// Pushes every symbol at 100 once a second, in place of a host's market data.
/// struct Flat;
///
/// impl Quotes for Flat {
///     fn subscribe(&self, symbols: &[String], events: Events<(String, Quote)>) {
///         let symbols = symbols.to_vec();
///         std::thread::spawn(move || {
///             while events.is_active() {
///                 for symbol in &symbols {
///                     let quote = Quote::new(100.0, 99.0, chrono::Utc::now()).with_volume(1e6);
///                     events.push((symbol.clone(), quote));
///                 }
///                 std::thread::sleep(Duration::from_secs(1));
///             }
///         });
///     }
/// }
/// ```
pub trait Quotes: MaybeSend + 'static {
    /// Starts pushing the quotes of `symbols` into `events`, each with its symbol, until the
    /// stream is inactive: that is the unsubscription, so stop then. Push the current quote of
    /// each symbol first, since a subscription starts from none. The watchlist subscribes again,
    /// with a new stream, whenever its symbols change and after the stream lagged, and stops the
    /// old one.
    ///
    /// The stream takes at most 10,000 pushes between two frames: coalesce the updates of a
    /// symbol rather than pushing each tick.
    fn subscribe(&self, symbols: &[String], events: Events<(String, Quote)>);
}
