//! Chat rooms the host connects the user to.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{MaybeSend, Reply};

/// The host's chat rooms, for the chats panel. The host keeps the account, the rooms and their
/// messages, and moderates; the panel shows what the service pushes and sends what the user
/// writes through [`send`](Self::send).
///
/// A message the user sends appears once the host pushes it back. Methods are called on the UI
/// thread and must return at once; answer and push from any thread, runtime or browser task.
///
/// ```
/// use std::sync::Mutex;
///
/// use egui_charts::data::Reply;
/// use egui_charts::services::{ChatEvent, ChatMessage, Chats, Events};
///
/// /// One room that echoes what the user writes. A real host sends it to its chat server.
/// #[derive(Default)]
/// struct Echo(Mutex<Option<Events<ChatEvent>>>);
///
/// impl Chats for Echo {
///     fn watch(&self, events: Events<ChatEvent>) {
///         events.push(ChatEvent::Room("General".into()));
///         *self.0.lock().unwrap() = Some(events);
///     }
///
///     fn send(&self, room: String, text: String, reply: Reply<()>) {
///         if let Some(events) = &*self.0.lock().unwrap() {
///             let message = ChatMessage::new(room, "You", chrono::Utc::now(), text);
///             events.push(ChatEvent::Message(message));
///         }
///         reply.send(Ok(()));
///     }
/// }
/// ```
pub trait Chats: MaybeSend + 'static {
    /// Pushes the user's rooms and their recent messages, oldest first, then every new room and
    /// message until `events` is inactive. Called when the panel starts and again after a lagged
    /// stream, each time with a new stream that starts from no rooms.
    ///
    /// The stream takes at most 10,000 pushes between two frames: push at most that many recent
    /// messages at once, or the stream lags on every call and nothing shows.
    fn watch(&self, events: Events<ChatEvent>);

    /// Posts `text` to `room`. Answer `Ok` once it is posted, and push the message.
    fn send(&self, room: String, text: String, reply: Reply<()>);
}

/// A change to the user's rooms, pushed through the stream [`Chats::watch`] received.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ChatEvent {
    /// The user can read and post in this room.
    Room(String),
    /// A message in a room, which also adds the room.
    Message(ChatMessage),
    /// Removes every room and message, so a host that reconnects can push them again.
    Clear,
}

/// One message of a room.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ChatMessage {
    /// Host identifier of the chat room.
    pub room: String,
    /// The author's display name.
    pub author: String,
    /// UTC timestamp supplied by the host.
    pub time: DateTime<Utc>,
    /// Message text.
    pub text: String,
}

impl ChatMessage {
    /// The message `text` of `author` in `room`, posted at `time`.
    pub fn new(
        room: impl Into<String>,
        author: impl Into<String>,
        time: DateTime<Utc>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            room: room.into(),
            author: author.into(),
            time,
            text: text.into(),
        }
    }
}
