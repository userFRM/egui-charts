//! A bounded stream of pushes from a service to the terminal.

use crate::data::inbox::Inbox;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Updates a service pushes to the terminal, from any thread, runtime or browser task. Cheap to
/// clone; clones push into the same stream. The first push after a frame wakes the UI, and the
/// pushes are applied in order on the next frame.
///
/// A stream holds at most 10,000 pushes between two frames. A push between frames counts towards
/// that bound, so a burst larger than the bound always ends the stream. A push beyond the bound is
/// dropped and ends the stream: the terminal discards what the stream built and calls the service again, which
/// pushes its current state first.
pub struct Events<T: Send + 'static> {
    inbox: Arc<Inbox>,
    key: u64,
    active: Arc<AtomicBool>,
    event: PhantomData<fn(T)>,
}

impl<T: Send + 'static> Events<T> {
    /// Delivers `event` to the terminal, or nothing once the stream is inactive.
    pub fn push(&self, event: T) {
        self.inbox
            .push_answer(self.key, Box::new(event), &self.active);
    }

    /// False once the terminal stopped listening: the symbol or interval changed, the stream
    /// lagged, or the terminal was dropped. Stop pushing then; later pushes are ignored.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
}

impl<T: Send + 'static> Clone for Events<T> {
    fn clone(&self) -> Self {
        Self {
            inbox: Arc::clone(&self.inbox),
            key: self.key,
            active: Arc::clone(&self.active),
            event: PhantomData,
        }
    }
}

/// The terminal's end of one stream: dropping it ends the stream.
pub(crate) struct Listener(Arc<AtomicBool>);

impl Listener {
    /// A stream whose pushes reach `inbox` under `key`, and its listener.
    pub(crate) fn new<T: Send + 'static>(inbox: &Arc<Inbox>, key: u64) -> (Self, Events<T>) {
        let active = Arc::new(AtomicBool::new(true));
        let events = Events {
            inbox: Arc::clone(inbox),
            key,
            active: Arc::clone(&active),
            event: PhantomData,
        };
        (Self(active), events)
    }

    /// False once the stream lagged: drop this listener, discard what the stream built and its
    /// pushes still in the inbox, and subscribe again.
    pub(crate) fn is_active(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn counting_wakes(inbox: &Inbox) -> Arc<AtomicUsize> {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        inbox.set_waker(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        wakes
    }

    /// Each event with the key of its stream, in the order the inbox delivers them.
    fn drain(inbox: &Inbox) -> Vec<(u64, u32)> {
        let answers = inbox.drain().answers.into_iter();
        answers
            .map(|(key, event)| (key, *event.downcast::<u32>().unwrap()))
            .collect()
    }

    /// Guards the memory bound and the repaint rate: a stream must deliver up to 10,000 pushes per
    /// frame in push order and wake the UI once, the bound must count one stream's pushes since the
    /// last frame (not its lifetime, not every stream's), and the push over it must end that
    /// stream for both ends. A reordered or coalesced queue reverses an upsert and a remove; a
    /// missing bound grows without limit while frames stop.
    #[test]
    fn a_stream_takes_10_000_pushes_per_frame_in_order_then_lags() {
        let inbox = Arc::new(Inbox::default());
        let wakes = counting_wakes(&inbox);
        let (listener, events) = Listener::new::<u32>(&inbox, 1);
        let (_other_listener, other) = Listener::new::<u32>(&inbox, 2);
        let producer = events.clone();
        std::thread::spawn(move || (0..10_000).for_each(|i| producer.push(i)))
            .join()
            .unwrap();
        other.push(7);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        let full_frame = Vec::from_iter((0..10_000).map(|i| (1, i)));
        assert_eq!(drain(&inbox), [full_frame.clone(), vec![(2, 7)]].concat());

        (0..10_000).for_each(|i| events.push(i));
        assert!(events.is_active() && listener.is_active());
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        events.push(10_000);
        other.push(8);
        assert!(!events.is_active() && !listener.is_active());
        assert!(other.is_active());
        assert_eq!(drain(&inbox), [full_frame, vec![(2, 8)]].concat());
    }

    /// Guards the stop signal: once the terminal drops its listener (symbol or interval changed,
    /// terminal dropped), every clone of the stream must read inactive so the host stops, and
    /// pushes that still arrive must neither reach the inbox nor wake the UI.
    #[test]
    fn a_stopped_stream_is_inactive_and_ignored() {
        let inbox = Arc::new(Inbox::default());
        let wakes = counting_wakes(&inbox);
        let (listener, events) = Listener::new::<u32>(&inbox, 1);
        let producer = events.clone();
        events.push(1);
        drop(listener);
        assert!(!events.is_active() && !producer.is_active());
        assert_eq!(drain(&inbox), [(1, 1)]);
        std::thread::spawn(move || producer.push(2)).join().unwrap();
        assert_eq!(drain(&inbox), []);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }
}
