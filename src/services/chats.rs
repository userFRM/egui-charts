//! Chat rooms the host connects the user to.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{FeedError, MaybeSend, Reply};

/// The host's chat rooms, for the chats panel. The host keeps the account, the rooms and their
/// messages, and moderates; the panel shows what the service pushes and sends what the user
/// writes through [`send`](Self::send).
///
/// A message the user sends appears once the host pushes it back. Choosing recipients, creating
/// rooms and changing their members are optional operations: the host turns each on in
/// [`capabilities`](Self::capabilities), keeps the accounts and decides whether the signed-in
/// user may do it, and answers a refusal with an error that the panel shows. Methods are called on the UI
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

    /// The optional operations below this host supports. The default supports none, and the panel
    /// then offers only to read and post. Read each frame, so it may change with the account.
    fn capabilities(&self) -> ChatCapabilities {
        ChatCapabilities::default()
    }

    /// The users whose name matches `query`, for choosing recipients and members. The default
    /// answers [`FeedError::NotSupported`].
    fn find_users(&self, query: String, reply: Reply<Vec<ChatUser>>) {
        let _ = query;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Creates `room` and answers its identifier once it exists. Push the room, as
    /// [`ChatEvent::Room`] or [`ChatEvent::Members`], for the panel to show it. The default
    /// answers [`FeedError::NotSupported`].
    fn create_room(&self, room: NewChatRoom, reply: Reply<String>) {
        let _ = room;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Adds the user with identifier `user` to `room`. Answer `Ok` once it is done, and push the
    /// new [`ChatEvent::Members`]. The default answers [`FeedError::NotSupported`].
    fn add_member(&self, room: String, user: String, reply: Reply<()>) {
        let _ = (room, user);
        reply.send(Err(FeedError::NotSupported));
    }

    /// Removes the user with identifier `user` from `room`. Answer `Ok` once it is done, and push
    /// the new [`ChatEvent::Members`]. The default answers [`FeedError::NotSupported`].
    fn remove_member(&self, room: String, user: String, reply: Reply<()>) {
        let _ = (room, user);
        reply.send(Err(FeedError::NotSupported));
    }
}

/// The optional operations of a [`Chats`] host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChatCapabilities {
    /// The user can start a direct message with one other user.
    pub direct_messages: bool,
    /// The user can create private and group rooms.
    pub create_rooms: bool,
    /// The user can add and remove the members of a room that pushes its members.
    pub manage_members: bool,
}

/// A user, as the host identifies and names them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChatUser {
    /// Stable host identifier of the user.
    pub id: String,
    /// The name shown for the user.
    pub name: String,
}

impl ChatUser {
    /// The user `name` with host identifier `id`.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

/// Who may join a room the user creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NewChatRoomKind {
    /// A room of the user and one other user.
    Direct,
    /// A room only the invited users can join.
    Private,
    /// A named room of the invited users that others may be allowed to join.
    Group,
}

/// A room the user asks the host to create.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewChatRoom {
    /// Who may join.
    pub kind: NewChatRoomKind,
    /// The room's name; empty for a direct message.
    pub name: String,
    /// Host identifiers of the users to invite.
    pub members: Vec<String>,
}

/// A change to the user's rooms, pushed through the stream [`Chats::watch`] received.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ChatEvent {
    /// The user can read and post in this room.
    Room(String),
    /// A message in a room, which also adds the room.
    Message(ChatMessage),
    /// The members of a room, replacing those pushed before, which also adds the room.
    Members {
        /// Host identifier of the room.
        room: String,
        /// Everyone in the room.
        members: Vec<ChatUser>,
    },
    /// Removes every room, member and message, so a host that reconnects can push them again.
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
