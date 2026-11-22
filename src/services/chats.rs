//! Chat rooms the host connects the user to.

use chrono::{DateTime, Utc};

use super::Events;
use crate::data::{FeedError, MaybeSend, Reply};

/// The host's chat rooms, for the chats panel. The host keeps the account, the rooms and their
/// messages, and moderates; the panel shows what the service pushes and sends what the user
/// writes through [`send`](Self::send).
///
/// A message the user sends appears once the host pushes it back. Choosing recipients, creating
/// rooms, changing their members, reading older messages, replying to a message, linking to it
/// and ignoring its author are optional operations: the host
/// turns each on in [`capabilities`](Self::capabilities), keeps the accounts and decides whether
/// the signed-in user may do it, and answers a refusal with an error that the panel shows.
/// Methods are called on the UI thread and must return at once; answer and push from any thread,
/// runtime or browser task.
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
///             let message =
///                 ChatMessage::new(room, "You", chrono::Utc::now(), text).with_author_id("you");
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
    /// [`ChatEvent::Room`] or [`ChatEvent::Members`], for the panel to show it. For a direct room,
    /// answer the identifier of the room the user already has with that member, when there is
    /// one. The default answers [`FeedError::NotSupported`].
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

    /// Posts `text` to `room` as an answer to the message with identifier `to`. Answer `Ok` once
    /// it is posted, and push the message with its [`reply_to`](ChatMessage::reply_to) set. The
    /// default answers [`FeedError::NotSupported`].
    fn send_reply(&self, room: String, text: String, to: String, reply: Reply<()>) {
        let _ = (room, text, to);
        reply.send(Err(FeedError::NotSupported));
    }

    /// A link to `message` the user can share, or `None` when it has none. Called on the UI thread
    /// when the user asks to copy it, so it must return at once. The default has none.
    fn link(&self, message: &ChatMessage) -> Option<String> {
        let _ = message;
        None
    }

    /// Ignores the user with identifier `user`, or stops ignoring them when `ignore` is false.
    /// Answer `Ok` once it is done, and push the new [`ChatEvent::Ignored`]. Refuse to ignore the
    /// signed-in user. The default answers [`FeedError::NotSupported`].
    fn ignore_user(&self, user: String, ignore: bool, reply: Reply<()>) {
        let _ = (user, ignore);
        reply.send(Err(FeedError::NotSupported));
    }

    /// Answers the up to `limit` messages of `room` that come right before the message with
    /// identifier `before`, oldest first, or the newest of the room when `before` is `None`. An
    /// empty answer says the room has nothing older. The default answers
    /// [`FeedError::NotSupported`].
    ///
    /// The chats panel asks where [`ChatCapabilities::history`] is on, when the user reaches the
    /// top of a room: for the messages before the oldest one it holds, which must carry an
    /// [`id`](ChatMessage::id), or for the newest when it holds none, one page at a time and at
    /// most 1,000 messages of a room. It keeps the messages of `room` it does not hold yet, matched
    /// by identifier or, without one, by value, in time order; an answer that adds none ends
    /// paging for the room until the room loses messages. A failed answer shows with a button to
    /// ask again.
    ///
    /// ```
    /// use chrono::DateTime;
    ///
    /// use egui_charts::data::{Reply, ReplyReader};
    /// use egui_charts::services::{ChatCapabilities, ChatEvent, ChatMessage, Chats, Events};
    ///
    /// /// One room of 100 numbered messages, a minute apart, identified by their number.
    /// struct Numbered;
    ///
    /// impl Chats for Numbered {
    ///     fn watch(&self, _: Events<ChatEvent>) {}
    ///
    ///     fn send(&self, _: String, _: String, reply: Reply<()>) {
    ///         reply.send(Ok(()));
    ///     }
    ///
    ///     fn capabilities(&self) -> ChatCapabilities {
    ///         let mut capabilities = ChatCapabilities::default();
    ///         capabilities.history = true;
    ///         capabilities
    ///     }
    ///
    ///     fn history(
    ///         &self,
    ///         room: String,
    ///         before: Option<String>,
    ///         limit: u32,
    ///         reply: Reply<Vec<ChatMessage>>,
    ///     ) {
    ///         let end: u32 = before.map_or(100, |id| id.parse().unwrap_or(0));
    ///         let page = (end.saturating_sub(limit)..end)
    ///             .map(|n| {
    ///                 let time = DateTime::from_timestamp(i64::from(n) * 60, 0).unwrap();
    ///                 ChatMessage::new(room.clone(), "Ana", time, format!("#{n}"))
    ///                     .with_id(n.to_string())
    ///             })
    ///             .collect();
    ///         reply.send(Ok(page));
    ///     }
    /// }
    ///
    /// let (answer, reply) = ReplyReader::new();
    /// Numbered.history("General".into(), Some("3".into()), 50, reply);
    /// let page = answer.take().unwrap().unwrap();
    /// let ids: Vec<_> = page.iter().filter_map(|m| m.id.as_deref()).collect();
    /// assert_eq!(ids, ["0", "1", "2"]);
    /// ```
    fn history(
        &self,
        room: String,
        before: Option<String>,
        limit: u32,
        reply: Reply<Vec<ChatMessage>>,
    ) {
        let _ = (room, before, limit);
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
    /// The user can read a room's older messages, through [`Chats::history`].
    pub history: bool,
    /// The user can answer a message that has an identifier, through [`Chats::send_reply`].
    pub replies: bool,
    /// The user can copy a link to a message, from [`Chats::link`].
    pub links: bool,
    /// The user can ignore and stop ignoring authors that have an identifier, through
    /// [`Chats::ignore_user`].
    pub ignore_users: bool,
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

impl NewChatRoom {
    /// A room of `kind` named `name`, inviting the users whose host identifiers are `members`.
    /// The name of a direct message is empty. Values are stored as given.
    ///
    /// ```
    /// use std::sync::Mutex;
    ///
    /// use egui_charts::data::{Reply, ReplyReader};
    /// use egui_charts::services::{
    ///     ChatEvent, Chats, Events, EventsReader, NewChatRoom, NewChatRoomKind,
    /// };
    ///
    /// /// Creates every room it is asked for, identified by its name.
    /// #[derive(Default)]
    /// struct Rooms(Mutex<Option<Events<ChatEvent>>>);
    ///
    /// impl Chats for Rooms {
    ///     fn watch(&self, events: Events<ChatEvent>) {
    ///         *self.0.lock().unwrap() = Some(events);
    ///     }
    ///
    ///     fn send(&self, _: String, _: String, reply: Reply<()>) {
    ///         reply.send(Ok(()));
    ///     }
    ///
    ///     fn create_room(&self, room: NewChatRoom, reply: Reply<String>) {
    ///         if let Some(events) = &*self.0.lock().unwrap() {
    ///             events.push(ChatEvent::Room(room.name.clone()));
    ///         }
    ///         reply.send(Ok(room.name));
    ///     }
    /// }
    ///
    /// let rooms = Rooms::default();
    /// let (pushed, events) = EventsReader::new();
    /// rooms.watch(events);
    /// let (answer, reply) = ReplyReader::new();
    /// let members = vec!["ana".to_owned(), "ben".to_owned()];
    /// rooms.create_room(NewChatRoom::new(NewChatRoomKind::Group, "Futures desk", members), reply);
    /// assert_eq!(answer.take(), Some(Ok("Futures desk".to_owned())));
    /// assert_eq!(pushed.take(), [ChatEvent::Room("Futures desk".into())]);
    /// ```
    pub fn new(kind: NewChatRoomKind, name: impl Into<String>, members: Vec<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            members,
        }
    }
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
    /// The users the signed-in user ignores, replacing those pushed before. Their messages are
    /// not shown.
    Ignored(Vec<ChatUser>),
    /// Removes every room, member, message and ignored user, so a host that reconnects can push
    /// them again.
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
    /// Host identifier of the message, unique in its room, or `None`. The chats panel asks
    /// [`Chats::history`] for the messages before the oldest one it holds by this identifier.
    pub id: Option<String>,
    /// Host identifier of the author, or `None`. Without one the author cannot be ignored or
    /// opened as a profile.
    pub author_id: Option<String>,
    /// Identifier of the message of the same room this one answers, or `None`.
    pub reply_to: Option<String>,
}

impl ChatMessage {
    /// The message `text` of `author` in `room`, posted at `time`, without identifiers or a
    /// message it answers.
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
            id: None,
            author_id: None,
            reply_to: None,
        }
    }

    /// Sets [`id`](Self::id).
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets [`author_id`](Self::author_id).
    pub fn with_author_id(mut self, id: impl Into<String>) -> Self {
        self.author_id = Some(id.into());
        self
    }

    /// Sets [`reply_to`](Self::reply_to).
    pub fn with_reply_to(mut self, id: impl Into<String>) -> Self {
        self.reply_to = Some(id.into());
        self
    }
}
