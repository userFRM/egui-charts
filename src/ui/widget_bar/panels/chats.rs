//! The host's chat rooms: read a room and post to it, reply to, link and ignore messages, start
//! chats and manage members.

use crate::localization as l10n;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::Offset;
use egui::{RichText, Ui};

use super::controls::dropdown;
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::icons::icons;
use crate::model::TimezoneId;
use crate::services::{
    ChatCapabilities, ChatEvent, ChatMessage, ChatUser, Chats, Listener, NewChatRoom,
    NewChatRoomKind,
};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, translated_status_message};
use crate::ui_kit::{EmptyState, IconButton, PanelHeader};

/// Messages kept at most across the rooms; past it the room holding the most loses its oldest.
const KEPT: usize = 5_000;

/// Messages kept at most of one room; past it the room loses its oldest.
const ROOM_KEPT: usize = 1_000;

/// Messages asked for in one page of a room's older messages.
const PAGE: usize = 50;

/// Inbox key of the answers to [`Chats::send`]; streams and other requests use keys from 1.
const SENT: u64 = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum View {
    #[default]
    Room,
    NewChat,
    Members,
    Ignored,
}

/// What the user picked in a message's menu.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MessageAction {
    Reply,
    CopyLink,
    Ignore,
}

#[derive(Debug)]
struct Composer {
    kind: NewChatRoomKind,
    name: String,
    recipients: Vec<ChatUser>,
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            kind: NewChatRoomKind::Direct,
            name: String::new(),
            recipients: Vec::new(),
        }
    }
}

/// Where paging back through a room's older messages stands; a room without one asks for the
/// page before its oldest message once its top shows.
#[derive(Debug)]
enum Paging {
    /// The request under this inbox key waits for its answer.
    Loading(u64),
    /// The host has nothing older.
    Ended,
    /// The last request failed; Retry asks again.
    Failed(FeedError),
}

/// Whether `a` and `b` are one message of a room: the same host identifier, or equal contents
/// when either has none.
fn same(a: &ChatMessage, b: &ChatMessage) -> bool {
    a.room == b.room
        && match (&a.id, &b.id) {
            (Some(x), Some(y)) => x == y,
            _ => a.author == b.author && a.time == b.time && a.text == b.text,
        }
}

/// Whether `message` is by one of the `ignored` users.
fn by_ignored(ignored: &[ChatUser], message: &ChatMessage) -> bool {
    message
        .author_id
        .as_ref()
        .is_some_and(|id| ignored.iter().any(|user| &user.id == id))
}

/// Whether the host lets the user create a room of `kind`.
fn allowed(kind: NewChatRoomKind, capabilities: &ChatCapabilities) -> bool {
    match kind {
        NewChatRoomKind::Direct => capabilities.direct_messages,
        _ => capabilities.create_rooms,
    }
}

/// Chat rooms pushed by the host's [`Chats`] service.
///
/// The user picks a room, reads its messages and posts; a posted message appears once the host
/// pushes it. Messages show in time order, a message the room holds already is not added again,
/// and the panel keeps the newest 1,000 messages of each room and 5,000 in all; past that the
/// room holding the most loses its oldest, sparing a room receiving a history page. Where the
/// service offers [`history`](Chats::history), reaching the top of a room asks for its older
/// messages a page at a time and keeps the view on the message that led the list. Where the
/// service's [`capabilities`](Chats::capabilities) allow, the header offers to start a direct message or
/// create a private or group room, choosing the recipients from the service's
/// [`find_users`](Chats::find_users), and to add or remove the members of a room whose members
/// the service pushes. Where the capabilities allow, a message's author opens a menu to reply to
/// the message, copy the host's link to it or ignore the author, whose messages then stay hidden
/// until the user stops ignoring them from the header's list. Each action shows while it is
/// pending and its failure, and attaching another service clears them. Message times and a separator for each day are written in the
/// zone set with [`set_timezone`](Self::set_timezone), UTC until then. Call [`poll`](Self::poll)
/// each frame, even while the panel is hidden, then [`show`](Self::show) while it is open.
/// Accounts and moderation are the host's.
#[derive(Default)]
pub struct ChatsPanel {
    service: Option<Arc<dyn Chats>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    /// The last inbox key handed out; streams and requests draw from it.
    keys: u64,
    generation: u64,
    rooms: Vec<String>,
    room: usize,
    /// Each room's messages, oldest first.
    messages: HashMap<String, VecDeque<ChatMessage>>,
    /// Paging back through each room's older messages, by room.
    paging: HashMap<String, Paging>,
    /// The first message of the shown room last frame and its top below the top of the list, to
    /// keep the view on it when older messages join above.
    first: Option<(ChatMessage, f32)>,
    members: HashMap<String, Vec<ChatUser>>,
    draft: FilterWidget,
    error: Option<FeedError>,
    view: View,
    composer: Composer,
    search: FilterWidget,
    found: Vec<ChatUser>,
    /// Inbox keys of the pending requests, or 0 for none.
    search_key: u64,
    create_key: u64,
    member_key: u64,
    /// A room to select once the service pushes it.
    open: Option<String>,
    action_error: Option<FeedError>,
    timezone: TimezoneId,
    /// The users whose messages are not shown, as the service pushed them.
    ignored: Vec<ChatUser>,
    /// The message the composer answers.
    replying: Option<ChatMessage>,
    /// Inbox key of the pending ignore request, or 0 for none.
    ignore_key: u64,
}

impl std::fmt::Debug for ChatsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatsPanel")
            .field("keys", &self.keys)
            .field("generation", &self.generation)
            .field("rooms", &self.rooms)
            .field("room", &self.room)
            .field("messages", &self.messages)
            .field("paging", &self.paging)
            .field("members", &self.members)
            .field("draft", &self.draft)
            .field("error", &self.error)
            .field("search", &self.search)
            .field("found", &self.found)
            .field("search_key", &self.search_key)
            .field("create_key", &self.create_key)
            .field("member_key", &self.member_key)
            .field("open", &self.open)
            .field("action_error", &self.action_error)
            .field("timezone", &self.timezone)
            .field("ignored", &self.ignored)
            .field("replying", &self.replying)
            .field("ignore_key", &self.ignore_key)
            .finish_non_exhaustive()
    }
}

impl ChatsPanel {
    /// An unconnected panel. Attach a service with [`set_service`](Self::set_service).
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a host service, clearing the previous one's rooms, members, recipients and pending
    /// actions.
    pub fn set_service(&mut self, service: Arc<dyn Chats>) {
        self.service = Some(service);
        self.view = View::Room;
        self.composer = Composer::default();
        self.reset_search();
        self.create_key = 0;
        self.member_key = 0;
        self.ignore_key = 0;
        self.replying = None;
        self.open = None;
        self.action_error = None;
        self.watch();
    }

    /// Writes message times and day separators in `timezone`, such as the chart's. UTC until set.
    pub fn set_timezone(&mut self, timezone: TimezoneId) {
        self.timezone = timezone;
    }

    fn next_key(&mut self) -> u64 {
        self.keys += 1;
        self.keys
    }

    fn reset_search(&mut self) {
        self.search.clear();
        self.found.clear();
        self.search_key = 0;
    }

    fn watch(&mut self) {
        self.rooms.clear();
        self.messages.clear();
        self.paging.clear();
        self.first = None;
        self.members.clear();
        self.ignored.clear();
        self.generation = self.next_key();
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        if let Some(service) = &self.service {
            service.watch(events);
        }
    }

    fn add_room(&mut self, room: &str) {
        if !self.rooms.iter().any(|known| known == room) {
            self.rooms.push(room.to_owned());
        }
    }

    /// Bounds the messages after `room` grew: `room` keeps its newest [`ROOM_KEPT`], then while
    /// the rooms hold more than [`KEPT`] the room holding the most loses its oldest, the first
    /// listed among equals, sparing `room` when it grew by a page. A room that loses messages
    /// has older ones again, so a history that ended may be asked again.
    fn bound(&mut self, room: &str, spare: bool) {
        if let Some(log) = self.messages.get_mut(room)
            && log.len() > ROOM_KEPT
        {
            log.drain(..log.len() - ROOM_KEPT);
            self.reopen(room);
        }
        while self.messages.values().map(VecDeque::len).sum::<usize>() > KEPT {
            let Some(largest) = self
                .rooms
                .iter()
                .filter(|name| !(spare && name.as_str() == room))
                .filter_map(|name| self.messages.get(name).map(|log| (name, log.len())))
                .min_by_key(|(_, len)| std::cmp::Reverse(*len))
                .map(|(name, _)| name.clone())
            else {
                break;
            };
            if let Some(log) = self.messages.get_mut(&largest) {
                log.pop_front();
            }
            self.reopen(&largest);
        }
    }

    /// Lets a room that lost messages ask again: a history that ended has older ones again, and
    /// a page on its way, asked before a message the room no longer holds, would leave a hole.
    fn reopen(&mut self, room: &str) {
        if matches!(
            self.paging.get(room),
            Some(Paging::Ended | Paging::Loading(_))
        ) {
            self.paging.remove(room);
        }
    }

    /// The room whose page of older messages is on its way under `key`.
    fn loading(&self, key: u64) -> Option<String> {
        self.paging.iter().find_map(|(room, paging)| {
            matches!(paging, Paging::Loading(at) if *at == key).then(|| room.clone())
        })
    }

    /// Asks the service for the page before the oldest message of `room`, or for its newest when
    /// it holds none, unless a request is on its way or failed, its history ended, the room holds
    /// [`ROOM_KEPT`] messages, or its oldest message has no identifier.
    fn ask(&mut self, room: &str) {
        let log = self.messages.get(room);
        let held = log.map_or(0, VecDeque::len);
        let oldest = log.and_then(VecDeque::front);
        if self.paging.contains_key(room)
            || held >= ROOM_KEPT
            || oldest.is_some_and(|message| message.id.is_none())
        {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        let before = oldest.and_then(|message| message.id.clone());
        let key = self.next_key();
        self.paging.insert(room.to_owned(), Paging::Loading(key));
        let limit = PAGE.min(ROOM_KEPT - held) as u32;
        service.history(room.to_owned(), before, limit, self.inbox.reply(key));
    }

    /// Joins the messages of `room` in `page` that it does not hold, in time order, and ends
    /// paging for the room when none is new.
    fn merge(&mut self, room: String, page: Vec<ChatMessage>) {
        let log = self.messages.entry(room.clone()).or_default();
        let mut added = false;
        // Newest first, each before the held messages of its time, so a page keeps its order and
        // goes before the message it was asked before.
        for message in page.into_iter().rev() {
            if message.room != room || log.iter().any(|held| same(held, &message)) {
                continue;
            }
            let at = log.partition_point(|held| held.time < message.time);
            log.insert(at, message);
            added = true;
        }
        if added {
            self.paging.remove(&room);
        } else {
            self.paging.insert(room.clone(), Paging::Ended);
        }
        self.bound(&room, true);
    }

    /// Receive the service's pushes and answers, and watch again after a lagged stream.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.watch();
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == SENT {
                if let Ok(result) = answer.downcast::<Result<(), FeedError>>() {
                    self.error = result.err();
                }
                continue;
            }
            if key == self.search_key {
                self.search_key = 0;
                if let Ok(result) = answer.downcast::<Result<Vec<ChatUser>, FeedError>>() {
                    match *result {
                        Ok(users) => self.found = users,
                        Err(error) => self.action_error = Some(error),
                    }
                }
                continue;
            }
            if key == self.create_key {
                self.create_key = 0;
                if let Ok(result) = answer.downcast::<Result<String, FeedError>>() {
                    match *result {
                        Ok(room) => {
                            self.composer = Composer::default();
                            self.reset_search();
                            self.view = View::Room;
                            self.open = Some(room);
                        }
                        Err(error) => self.action_error = Some(error),
                    }
                }
                continue;
            }
            if key == self.member_key {
                self.member_key = 0;
                if let Ok(result) = answer.downcast::<Result<(), FeedError>>() {
                    self.action_error = result.err();
                }
                continue;
            }
            if key == self.ignore_key {
                self.ignore_key = 0;
                if let Ok(result) = answer.downcast::<Result<(), FeedError>>() {
                    self.action_error = result.err();
                }
                continue;
            }
            if key != self.generation {
                if let Some(room) = self.loading(key)
                    && let Ok(result) = answer.downcast::<Result<Vec<ChatMessage>, FeedError>>()
                {
                    match *result {
                        Ok(page) => self.merge(room, page),
                        Err(error) => {
                            self.paging.insert(room, Paging::Failed(error));
                        }
                    }
                }
                continue;
            }
            match answer.downcast::<ChatEvent>().map(|event| *event) {
                Ok(ChatEvent::Room(room)) => self.add_room(&room),
                Ok(ChatEvent::Message(message)) => {
                    self.add_room(&message.room);
                    let room = message.room.clone();
                    let log = self.messages.entry(room.clone()).or_default();
                    // A history page may answer before the stream pushes the same message.
                    let duplicate =
                        message.id.is_some() && log.iter().any(|held| same(held, &message));
                    if !duplicate {
                        let at = log.partition_point(|held| held.time <= message.time);
                        log.insert(at, message);
                        self.bound(&room, false);
                    }
                }
                Ok(ChatEvent::Members { room, members }) => {
                    self.add_room(&room);
                    self.members.insert(room, members);
                }
                Ok(ChatEvent::Ignored(users)) => self.ignored = users,
                Ok(ChatEvent::Clear) => {
                    self.ignored.clear();
                    self.rooms.clear();
                    self.messages.clear();
                    self.paging.clear();
                    self.first = None;
                    self.members.clear();
                }
                Err(_) => {}
            }
        }
        if let Some(index) = self
            .open
            .as_ref()
            .and_then(|open| self.rooms.iter().position(|room| room == open))
        {
            self.room = index;
            self.open = None;
        }
        self.room = self.room.min(self.rooms.len().saturating_sub(1));
    }

    /// Render the chosen room, or the chat and member controls the header opens, after
    /// [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) {
        let theme = ui.active_theme();
        let capabilities = self
            .service
            .as_ref()
            .map(|service| service.capabilities())
            .unwrap_or_default();
        let can_create = capabilities.direct_messages || capabilities.create_rooms;
        let can_manage = capabilities.manage_members
            && self
                .rooms
                .get(self.room)
                .is_some_and(|room| self.members.contains_key(room));
        let can_unignore = capabilities.ignore_users && !self.ignored.is_empty();
        if (!can_create && self.view == View::NewChat)
            || (!can_manage && self.view == View::Members)
            || (!can_unignore && self.view == View::Ignored)
        {
            self.view = View::Room;
        }
        let mut view = self.view;
        PanelHeader::new("Chats").show(ui, |ui| {
            if can_unignore
                && IconButton::new(&icons::EYE_HIDE)
                    .tooltip("Ignored users")
                    .selected(self.view == View::Ignored)
                    .show(ui)
                    .clicked()
            {
                view = if self.view == View::Ignored {
                    View::Room
                } else {
                    View::Ignored
                };
            }
            if can_manage
                && IconButton::new(&icons::USERS)
                    .tooltip("Members")
                    .selected(self.view == View::Members)
                    .show(ui)
                    .clicked()
            {
                view = if self.view == View::Members {
                    View::Room
                } else {
                    View::Members
                };
            }
            if can_create
                && IconButton::new(&icons::PLUS)
                    .tooltip("New chat")
                    .selected(self.view == View::NewChat)
                    .show(ui)
                    .clicked()
            {
                view = if self.view == View::NewChat {
                    View::Room
                } else {
                    View::NewChat
                };
            }
        });
        if view != self.view {
            self.view = view;
            self.reset_search();
            self.action_error = None;
        }
        if let Some(error) = &self.action_error {
            translated_status_message(
                ui,
                MessageKind::Error,
                l10n::format_text(
                    ui.ctx(),
                    "Chat action failed: {error}",
                    &[("error", error.to_string().as_str())],
                ),
            );
        }
        match self.view {
            View::NewChat => return self.show_new_chat(ui, &capabilities),
            View::Members => return self.show_members(ui),
            View::Ignored => return self.show_ignored(ui),
            View::Room => {}
        }
        let Some(room) = self.rooms.get(self.room).cloned() else {
            EmptyState::new("No chat rooms")
                .icon(&icons::WIDGET_BAR_CHATS)
                .fill_height()
                .show(ui);
            return;
        };
        dropdown(ui, "chat_room", &room, |ui| {
            for (index, name) in self.rooms.iter().enumerate() {
                ui.selectable_value(&mut self.room, index, l10n::text(ui.ctx(), name));
            }
        });
        let room = self.rooms[self.room].clone();
        if self.replying.as_ref().is_some_and(|message| message.room != room) {
            self.replying = None;
        }
        if let Some(error) = &self.error {
            translated_status_message(
                ui,
                MessageKind::Error,
                l10n::format_text(
                    ui.ctx(),
                    "Message not sent: {error}",
                    &[("error", error.to_string().as_str())],
                ),
            );
        }
        let mut send = false;
        let mut cancel = false;
        egui::Panel::bottom(ui.scope_id().with("chat_compose"))
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.add_space(theme.spacing.sm);
                if let Some(replying) = &self.replying {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(l10n::format_text(
                                ui.ctx(),
                                "Replying to {name}",
                                &[("name", replying.author.as_str())],
                            ))
                            .color(theme.ui.text_secondary),
                        );
                        cancel = IconButton::new(&icons::CLOSE)
                            .tooltip("Cancel reply")
                            .show(ui)
                            .clicked();
                    });
                }
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        send = Button::new("Send")
                            .enabled(!self.draft.query().trim().is_empty())
                            .show(ui)
                            .clicked();
                        self.draft = std::mem::take(&mut self.draft)
                            .placeholder(l10n::format_text(
                                ui.ctx(),
                                "Message {room}",
                                &[("room", room.as_str())],
                            ))
                            .show_icon(false)
                            .show_clear_button(false);
                        let response = self.draft.show(ui).response;
                        let entered =
                            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        send |= entered && !self.draft.query().trim().is_empty();
                    });
                });
            });
        if cancel {
            self.replying = None;
        }
        let log = self.messages.get(&room);
        let ignored = &self.ignored;
        // The message that led the list last frame, when another one leads it now.
        let moved = self.first.take().filter(|(was, _)| {
            log.into_iter()
                .flatten()
                .find(|message| !by_ignored(ignored, message))
                .is_some_and(|front| !same(was, front))
        });
        let ignore_free = self.ignore_key == 0;
        let mut picked = None;
        let mut first = None;
        let mut kept = false;
        let mut retry = false;
        let output = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let top = ui.cursor().top();
                if capabilities.history {
                    match self.paging.get(&room) {
                        Some(Paging::Loading(_)) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(l10n::text(ui.ctx(), "Loading older messages"));
                            });
                        }
                        Some(Paging::Ended) => {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), "No older messages"))
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                            );
                        }
                        Some(Paging::Failed(error)) => {
                            translated_status_message(
                                ui,
                                MessageKind::Error,
                                l10n::format_text(
                                    ui.ctx(),
                                    "Older messages not loaded: {error}",
                                    &[("error", error.to_string().as_str())],
                                ),
                            );
                            retry = Button::new("Retry").small().show(ui).clicked();
                        }
                        None => {}
                    }
                }
                let mut day = None;
                let shown = log
                    .into_iter()
                    .flatten()
                    .filter(|message| !by_ignored(ignored, message));
                for (index, message) in shown.enumerate() {
                    let local = message.time.with_timezone(&self.timezone);
                    if day != Some(local.date_naive()) {
                        day = Some(local.date_naive());
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(l10n::text(
                                    ui.ctx(),
                                    local.format("%A, %b %-d, %Y").to_string(),
                                ))
                                .size(theme.text.small)
                                .color(theme.ui.text_secondary),
                            );
                        });
                    }
                    // Below the day separator: the scroll keeping follows the message itself.
                    let y = ui.cursor().top() - top;
                    if index == 0 {
                        first = Some((message.clone(), y));
                    } else if !kept
                        && let Some((was, was_y)) = &moved
                        && same(was, message)
                    {
                        // Older messages joined above the message that led the list: keep it
                        // where it was, and lay the frame out again so the jump never shows.
                        ui.scroll_with_delta_animation(
                            egui::vec2(0.0, was_y - y),
                            egui::style::ScrollAnimation::none(),
                        );
                        ui.ctx()
                            .request_discard("Older chat messages joined above the list");
                        kept = true;
                    }
                    ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::hover()), |ui| {
                        let hovered = ui.response().hovered();
                        egui::Frame::new()
                            .fill(if hovered {
                                theme.ui.btn_bg_hover
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .inner_margin(egui::Margin::symmetric(8, 0))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.set_min_height(26.0);
                                ui.horizontal(|ui| {
                                    let author =
                                        RichText::new(l10n::text(ui.ctx(), &message.author))
                                            .strong();
                                    let reply = capabilities.replies && message.id.is_some();
                                    let ignore =
                                        capabilities.ignore_users && message.author_id.is_some();
                                    if reply || capabilities.links || ignore {
                                        ui.menu_button(author, |ui| {
                                            let mut pick = None;
                                            if reply
                                                && ui.button(l10n::text(ui.ctx(), "Reply")).clicked()
                                            {
                                                pick = Some(MessageAction::Reply);
                                            }
                                            if capabilities.links
                                                && ui
                                                    .button(l10n::text(ui.ctx(), "Copy link"))
                                                    .clicked()
                                            {
                                                pick = Some(MessageAction::CopyLink);
                                            }
                                            let name = l10n::format_text(
                                                ui.ctx(),
                                                "Ignore {name}",
                                                &[("name", message.author.as_str())],
                                            );
                                            if ignore
                                                && ui
                                                    .add_enabled(
                                                        ignore_free,
                                                        egui::Button::new(name),
                                                    )
                                                    .clicked()
                                            {
                                                pick = Some(MessageAction::Ignore);
                                            }
                                            if let Some(pick) = pick {
                                                picked = Some((pick, message.clone()));
                                                ui.close();
                                            }
                                        });
                                    } else {
                                        ui.label(author);
                                    }
                                    let zone = super::symbol_details::zone_label(
                                        local.offset().fix().local_minus_utc(),
                                    );
                                    ui.label(
                                        RichText::new(l10n::text(
                                            ui.ctx(),
                                            local.format("%H:%M").to_string(),
                                        ))
                                        .size(theme.text.small)
                                        .color(theme.ui.text_secondary),
                                    )
                                    .on_hover_text(format!(
                                        "{} {zone}",
                                        local.format("%b %-d, %Y, %H:%M")
                                    ));
                                });
                                if let Some(to) = &message.reply_to {
                                    // The answered message, unless the room no longer holds it or
                                    // its author is ignored.
                                    let line = log
                                        .into_iter()
                                        .flatten()
                                        .find(|held| held.id.as_ref() == Some(to))
                                        .filter(|held| !by_ignored(ignored, held))
                                        .map_or_else(
                                            || l10n::text(ui.ctx(), "Reply to an earlier message"),
                                            |held| {
                                                format!(
                                                    "{}: {}",
                                                    l10n::text(ui.ctx(), &held.author),
                                                    l10n::text(ui.ctx(), &held.text)
                                                )
                                            },
                                        );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(line)
                                                .size(theme.text.small)
                                                .color(theme.ui.text_secondary),
                                        )
                                        .truncate(),
                                    );
                                }
                                ui.label(l10n::text(ui.ctx(), &message.text));
                            });
                    });
                    ui.add_space(theme.spacing.sm);
                }
            });
        self.first = first;
        if let Some((pick, message)) = picked {
            self.pick(ui.ctx(), pick, message);
        }
        if retry {
            self.paging.remove(&room);
            self.ask(&room);
        }
        if capabilities.history && !kept && output.state.clamped_offset().y <= 0.0 {
            self.ask(&room);
        }
        if send && let Some(service) = &self.service {
            let text = std::mem::take(self.draft.query_mut()).trim().to_owned();
            match self.replying.take().and_then(|message| message.id) {
                Some(to) => service.send_reply(room, text, to, self.inbox.reply(SENT)),
                None => service.send(room, text, self.inbox.reply(SENT)),
            }
        }
    }

    /// Carries out what the user picked in the menu of `message`.
    fn pick(&mut self, ctx: &egui::Context, pick: MessageAction, message: ChatMessage) {
        let Some(service) = self.service.clone() else {
            return;
        };
        match pick {
            MessageAction::Reply => self.replying = Some(message),
            MessageAction::CopyLink => match service.link(&message) {
                Some(link) => ctx.copy_text(link),
                None => self.action_error = Some(FeedError::NotSupported),
            },
            MessageAction::Ignore => {
                if let Some(user) = message.author_id {
                    self.action_error = None;
                    self.ignore_key = self.next_key();
                    service.ignore_user(user, true, self.inbox.reply(self.ignore_key));
                }
            }
        }
    }

    /// The ignored users, each with a button to stop ignoring them.
    fn show_ignored(&mut self, ui: &mut Ui) {
        let pending = self.ignore_key != 0;
        let mut unignore = None;
        for user in &self.ignored {
            ui.push_id(&user.id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(l10n::text(ui.ctx(), &user.name));
                    if Button::new("Stop ignoring")
                        .small()
                        .enabled(!pending)
                        .show(ui)
                        .clicked()
                    {
                        unignore = Some(user.id.clone());
                    }
                })
            });
        }
        if pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(l10n::text(ui.ctx(), "Updating ignored users"));
            });
        }
        if let Some(user) = unignore
            && let Some(service) = self.service.clone()
        {
            self.action_error = None;
            self.ignore_key = self.next_key();
            service.ignore_user(user, false, self.inbox.reply(self.ignore_key));
        }
    }

    /// The search field for people and the matches to choose from, other than those in `skip`.
    /// The host is asked again whenever the text changes.
    fn find_people(&mut self, ui: &mut Ui, skip: &[ChatUser]) -> Option<ChatUser> {
        let before = self.search.query().to_owned();
        self.search = std::mem::take(&mut self.search)
            .placeholder("Find people")
            .show_icon(false)
            .show_clear_button(false);
        self.search.show(ui);
        if self.search.query() != before {
            self.found.clear();
            self.search_key = 0;
            self.action_error = None;
            let query = self.search.query().trim().to_owned();
            if !query.is_empty()
                && let Some(service) = self.service.clone()
            {
                self.search_key = self.next_key();
                service.find_users(query, self.inbox.reply(self.search_key));
            }
        }
        if self.search_key != 0 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(l10n::text(ui.ctx(), "Searching"));
            });
        }
        let mut picked = None;
        for user in &self.found {
            if !skip.iter().any(|known| known.id == user.id)
                && ui
                    .selectable_label(false, l10n::text(ui.ctx(), &user.name))
                    .clicked()
            {
                picked = Some(user.clone());
            }
        }
        picked
    }

    fn show_new_chat(&mut self, ui: &mut Ui, capabilities: &ChatCapabilities) {
        let kinds = [
            (NewChatRoomKind::Direct, "Direct message"),
            (NewChatRoomKind::Private, "Private group"),
            (NewChatRoomKind::Group, "Group"),
        ];
        if !allowed(self.composer.kind, capabilities)
            && let Some((kind, _)) = kinds.iter().find(|(kind, _)| allowed(*kind, capabilities))
        {
            self.composer.kind = *kind;
        }
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in kinds {
                if allowed(kind, capabilities)
                    && Button::new(label)
                        .small()
                        .selected(self.composer.kind == kind)
                        .show(ui)
                        .clicked()
                {
                    self.composer.kind = kind;
                }
            }
        });
        let direct = self.composer.kind == NewChatRoomKind::Direct;
        if direct {
            self.composer.recipients.truncate(1);
        } else {
            ui.add(
                egui::TextEdit::singleline(&mut self.composer.name)
                    .hint_text(l10n::text(ui.ctx(), "Room name"))
                    .desired_width(f32::INFINITY),
            );
        }
        let mut remove = None;
        for (index, user) in self.composer.recipients.iter().enumerate() {
            ui.push_id(&user.id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(l10n::text(ui.ctx(), &user.name));
                    if IconButton::new(&icons::CLOSE)
                        .tooltip("Remove")
                        .show(ui)
                        .clicked()
                    {
                        remove = Some(index);
                    }
                })
            });
        }
        if let Some(index) = remove {
            self.composer.recipients.remove(index);
        }
        let recipients = self.composer.recipients.clone();
        if let Some(user) = self.find_people(ui, &recipients) {
            if direct {
                self.composer.recipients.clear();
            }
            self.composer.recipients.push(user);
        }
        let ready = !self.composer.recipients.is_empty()
            && (direct || !self.composer.name.trim().is_empty());
        let pending = self.create_key != 0;
        if pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(l10n::text(ui.ctx(), "Creating chat"));
            });
        }
        if Button::new("Create")
            .enabled(ready && !pending)
            .show(ui)
            .clicked()
            && let Some(service) = self.service.clone()
        {
            self.action_error = None;
            self.create_key = self.next_key();
            let room = NewChatRoom {
                kind: self.composer.kind,
                name: if direct {
                    String::new()
                } else {
                    self.composer.name.trim().to_owned()
                },
                members: self
                    .composer
                    .recipients
                    .iter()
                    .map(|u| u.id.clone())
                    .collect(),
            };
            service.create_room(room, self.inbox.reply(self.create_key));
        }
    }

    fn show_members(&mut self, ui: &mut Ui) {
        let Some(room) = self.rooms.get(self.room).cloned() else {
            return;
        };
        let members = self.members.get(&room).cloned().unwrap_or_default();
        let pending = self.member_key != 0;
        let mut remove = None;
        for user in &members {
            ui.push_id(&user.id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(l10n::text(ui.ctx(), &user.name));
                    if IconButton::new(&icons::CLOSE)
                        .tooltip("Remove from room")
                        .enabled(!pending)
                        .show(ui)
                        .clicked()
                    {
                        remove = Some(user.id.clone());
                    }
                })
            });
        }
        let add = self.find_people(ui, &members);
        if pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(l10n::text(ui.ctx(), "Updating members"));
            });
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        if let Some(user) = remove {
            self.action_error = None;
            self.member_key = self.next_key();
            service.remove_member(room, user, self.inbox.reply(self.member_key));
        } else if let Some(user) = add.filter(|_| !pending) {
            self.action_error = None;
            self.member_key = self.next_key();
            service.add_member(room, user.id, self.inbox.reply(self.member_key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::services::Events;
    use egui::{Role, Vec2};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A call of [`Chats::history`]: the room, the message asked before, the limit and the reply.
    type Asked = (String, Option<String>, u32, Reply<Vec<ChatMessage>>);

    #[derive(Default)]
    struct Host {
        created: Mutex<Vec<(NewChatRoom, Reply<String>)>>,
        events: Mutex<Option<Events<ChatEvent>>>,
        history: AtomicBool,
        replies: AtomicBool,
        links: AtomicBool,
        ignore_users: AtomicBool,
        asked: Mutex<Vec<Asked>>,
        /// The room and text of each call of [`Chats::send`].
        sent: Mutex<Vec<(String, String)>>,
        /// The room, text and answered message of each call of [`Chats::send_reply`].
        replied: Mutex<Vec<(String, String, String)>>,
        ignores: Mutex<Vec<(String, bool, Reply<()>)>>,
    }

    impl Chats for Host {
        fn watch(&self, events: Events<ChatEvent>) {
            *self.events.lock().unwrap() = Some(events);
        }
        fn send(&self, room: String, text: String, reply: Reply<()>) {
            self.sent.lock().unwrap().push((room, text));
            reply.send(Ok(()));
        }
        fn send_reply(&self, room: String, text: String, to: String, reply: Reply<()>) {
            self.replied.lock().unwrap().push((room, text, to));
            reply.send(Ok(()));
        }
        fn link(&self, message: &ChatMessage) -> Option<String> {
            let id = message.id.as_ref()?;
            Some(format!("https://example.invalid/{}/{id}", message.room))
        }
        fn ignore_user(&self, user: String, ignore: bool, reply: Reply<()>) {
            self.ignores.lock().unwrap().push((user, ignore, reply));
        }
        fn capabilities(&self) -> ChatCapabilities {
            ChatCapabilities {
                direct_messages: true,
                create_rooms: true,
                manage_members: false,
                history: self.history.load(Ordering::Relaxed),
                replies: self.replies.load(Ordering::Relaxed),
                links: self.links.load(Ordering::Relaxed),
                ignore_users: self.ignore_users.load(Ordering::Relaxed),
            }
        }
        fn create_room(&self, room: NewChatRoom, reply: Reply<String>) {
            self.created.lock().unwrap().push((room, reply));
        }
        fn history(
            &self,
            room: String,
            before: Option<String>,
            limit: u32,
            reply: Reply<Vec<ChatMessage>>,
        ) {
            let call = (room, before, limit, reply);
            self.asked.lock().unwrap().push(call);
        }
    }

    /// Guards creating a room through the host: the chosen recipients and name are sent once, a
    /// refusal keeps the form and shows the failure, an acceptance opens the pushed room, and
    /// attaching another service clears the form.
    #[test]
    fn creating_a_room_sends_the_recipients_and_keeps_the_form_on_refusal() {
        let host = Arc::new(Host::default());
        let mut panel = ChatsPanel::new();
        panel.set_service(host.clone());
        panel.view = View::NewChat;
        panel.composer.kind = NewChatRoomKind::Private;
        panel.composer.name = " Desk ".into();
        panel.composer.recipients = vec![ChatUser::new("u1", "Ada"), ChatUser::new("u2", "Bo")];
        let mut h = Harness::builder()
            .with_size(Vec2::new(320.0, 600.0))
            .with_context_setup(|ctx| {
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                egui_extras::install_image_loaders(ctx);
            })
            .build_ui_state(|ui, panel: &mut ChatsPanel| panel.show(ui), panel);
        h.run();
        h.get_by_role_and_label(Role::Button, "Create").click();
        h.step();
        let ctx = egui::Context::default();
        let (room, reply) = {
            let mut created = host.created.lock().unwrap();
            assert_eq!(created.len(), 1);
            created.pop().unwrap()
        };
        assert_eq!(
            room,
            NewChatRoom {
                kind: NewChatRoomKind::Private,
                name: "Desk".into(),
                members: vec!["u1".into(), "u2".into()],
            }
        );
        reply.send(Err(FeedError::Other("not allowed".into())));
        h.state_mut().poll(&ctx);
        h.run();
        let panel = h.state();
        assert!(panel.view == View::NewChat);
        assert_eq!(panel.composer.recipients.len(), 2);
        assert_eq!(
            panel.action_error,
            Some(FeedError::Other("not allowed".into()))
        );

        h.get_by_role_and_label(Role::Button, "Create").click();
        h.step();
        let (_, reply) = host.created.lock().unwrap().pop().unwrap();
        host.events
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .push(ChatEvent::Room("r1".into()));
        reply.send(Ok("r1".into()));
        h.state_mut().poll(&ctx);
        let panel = h.state();
        assert!(panel.view == View::Room);
        assert_eq!(panel.rooms, ["r1"]);
        assert!(panel.composer.recipients.is_empty() && panel.action_error.is_none());

        let panel = h.state_mut();
        panel.view = View::NewChat;
        panel.composer.recipients = vec![ChatUser::new("u1", "Ada")];
        panel.set_service(Arc::new(Host::default()));
        assert!(panel.composer.recipients.is_empty() && panel.view == View::Room);
    }

    /// Guards the bounds on held messages: each room keeps its newest, and past the total the
    /// room holding the most loses its oldest, so the traffic of a busy room never empties a
    /// quiet one.
    #[test]
    fn a_busy_room_keeps_its_newest_messages_and_never_empties_a_quiet_one() {
        let host = Arc::new(Host::default());
        let mut panel = ChatsPanel::new();
        panel.set_service(host.clone());
        let events = host.events.lock().unwrap().clone().unwrap();
        let push = |room: &str, count: usize| {
            for i in 0..count {
                let time = chrono::DateTime::UNIX_EPOCH;
                let message = ChatMessage::new(room, "Ann", time, format!("{room} {i}"));
                events.push(ChatEvent::Message(message));
            }
        };
        let ctx = egui::Context::default();
        push("quiet", 10);
        push("busy", 6_000);
        panel.poll(&ctx);
        let mut h = Harness::builder()
            .with_size(Vec2::new(320.0, 800.0))
            .with_context_setup(|ctx| {
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                egui_extras::install_image_loaders(ctx);
            })
            .build_ui_state(|ui, panel: &mut ChatsPanel| panel.show(ui), panel);
        h.run();
        for i in 0..10 {
            h.get_by_label(&format!("quiet {i}"));
        }
        let busy = &h.state().messages["busy"];
        assert_eq!(
            (busy.len(), busy[0].text.as_str(), busy[999].text.as_str()),
            (1_000, "busy 5000", "busy 5999")
        );

        // The last ten messages of r4 pass 5,000 in all: busy, r1, r2 and r3 lose one each in
        // list order, twice, then r4, holding the most, loses two.
        for room in ["r1", "r2", "r3", "r4"] {
            push(room, 1_000);
        }
        h.state_mut().poll(&ctx);
        let panel = h.state();
        let held: Vec<_> = panel
            .rooms
            .iter()
            .map(|room| {
                let log = &panel.messages[room];
                (log.len(), log[0].text.as_str())
            })
            .collect();
        assert_eq!(
            held,
            [
                (10, "quiet 0"),
                (998, "busy 5002"),
                (998, "r1 2"),
                (998, "r2 2"),
                (998, "r3 2"),
                (998, "r4 2"),
            ]
        );
    }

    /// Guards paging a room's older messages: without the capability nothing is asked and no
    /// paging row shows; at the top one request asks for the page before the oldest message; the
    /// answer joins in time order without the messages the room holds or another room's, and the
    /// view stays on the message that led the list, so the next page waits for the next scroll;
    /// the stream pushing a message the page brought adds nothing; a failure offers Retry; an
    /// empty answer shows the end and stops paging.
    #[test]
    fn reaching_the_top_pages_older_messages_once_and_merges_them_in_order() {
        let host = Arc::new(Host::default());
        let mut panel = ChatsPanel::new();
        panel.set_service(host.clone());
        let message = |room: &str, author: &str, minute: i64, text: &str| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minute);
            ChatMessage::new(room, author, time, text).with_id(text)
        };
        let events = host.events.lock().unwrap().clone().unwrap();
        for i in 0..20 {
            let pushed = message("desk", "Ann", i, &format!("m{i}"));
            events.push(ChatEvent::Message(pushed));
        }
        let ctx = egui::Context::default();
        panel.poll(&ctx);
        let mut h = Harness::builder()
            .with_size(Vec2::new(320.0, 400.0))
            .with_context_setup(|ctx| {
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                egui_extras::install_image_loaders(ctx);
            })
            .build_ui_state(|ui, panel: &mut ChatsPanel| panel.show(ui), panel);
        // Steps, not runs: a request started on the way shows a spinner, which repaints.
        let to_top = |h: &mut Harness<'_, ChatsPanel>| {
            for _ in 0..20 {
                h.get_by_role_and_label(Role::Button, "desk").scroll_up();
                h.run_steps(2);
            }
        };
        let take = || {
            let mut asked = host.asked.lock().unwrap();
            assert_eq!(asked.len(), 1);
            asked.pop().unwrap()
        };

        h.run();
        to_top(&mut h);
        assert!(host.asked.lock().unwrap().is_empty());
        assert!(h.query_by_label("Loading older messages").is_none());
        assert!(h.query_by_label("No older messages").is_none());

        host.history.store(true, Ordering::Relaxed);
        h.run_steps(3);
        h.get_by_label("Loading older messages");
        h.run_steps(3);
        let (room, before, limit, reply) = take();
        assert_eq!(
            (room.as_str(), before.as_deref(), limit),
            ("desk", Some("m0"), 50)
        );

        reply.send(Ok(vec![
            message("desk", "Bo", -2, "p2"),
            message("desk", "Bo", -1, "p1"),
            ChatMessage::new("desk", "Ann", chrono::DateTime::UNIX_EPOCH, "m0"),
            message("desk", "Bo", -1, "p1"),
            message("other", "Bo", -3, "x"),
        ]));
        h.state_mut().poll(&ctx);
        h.run();
        let texts: Vec<_> = h.state().messages["desk"]
            .iter()
            .map(|message| message.text.clone())
            .collect();
        let expected: Vec<_> = ["p2".to_owned(), "p1".to_owned()]
            .into_iter()
            .chain((0..20).map(|i| format!("m{i}")))
            .collect();
        assert_eq!(texts, expected);
        assert_eq!(h.state().rooms, ["desk"]);
        assert!(host.asked.lock().unwrap().is_empty());

        // The stream pushing a message the page brought adds nothing, even with a newer time,
        // and a late message joins in time order.
        events.push(ChatEvent::Message(message("desk", "Bo", -1, "p1")));
        events.push(ChatEvent::Message(message("desk", "Bo", 30, "p1")));
        events.push(ChatEvent::Message(message("desk", "Cy", 5, "late")));
        h.state_mut().poll(&ctx);
        let log = &h.state().messages["desk"];
        assert_eq!((log.len(), log[8].text.as_str()), (23, "late"));

        to_top(&mut h);
        let (_, before, limit, reply) = take();
        assert_eq!((before.as_deref(), limit), (Some("p2"), 50));
        reply.send(Err(FeedError::Other("offline".into())));
        h.state_mut().poll(&ctx);
        h.run();
        h.get_by_label("Older messages not loaded: offline");
        assert!(host.asked.lock().unwrap().is_empty());
        h.get_by_role_and_label(Role::Button, "Retry").click();
        h.run_steps(3);
        let (_, before, _, reply) = take();
        assert_eq!(before.as_deref(), Some("p2"));
        reply.send(Ok(Vec::new()));
        h.state_mut().poll(&ctx);
        h.run();
        h.get_by_label("No older messages");
        to_top(&mut h);
        assert!(host.asked.lock().unwrap().is_empty());
    }

    /// Guards message times: each is written in the zone the host sets, UTC until then, under a
    /// separator for each local day, and its hover names the date and the zone.
    #[test]
    fn times_follow_the_zone_with_a_separator_per_local_day() {
        use chrono::TimeZone;
        let utc = |day, hour| chrono::Utc.with_ymd_and_hms(2026, 11, day, hour, 30, 0).unwrap();
        for (zone, expected, hover) in [
            (
                None,
                [
                    "Friday, Nov 20, 2026",
                    "23:30",
                    "Saturday, Nov 21, 2026",
                    "04:30",
                    "05:30",
                ],
                "Nov 20, 2026, 23:30 UTC",
            ),
            (
                TimezoneId::fixed_offset(-5 * 3600),
                [
                    "Friday, Nov 20, 2026",
                    "18:30",
                    "23:30",
                    "Saturday, Nov 21, 2026",
                    "00:30",
                ],
                "Nov 20, 2026, 18:30 GMT-5",
            ),
        ] {
            let host = Arc::new(Host::default());
            let mut panel = ChatsPanel::new();
            panel.set_service(host.clone());
            if let Some(zone) = zone {
                panel.set_timezone(zone);
            }
            let events = host.events.lock().unwrap().clone().unwrap();
            for (n, time) in [utc(20, 23), utc(21, 4), utc(21, 5)].into_iter().enumerate() {
                let message = ChatMessage::new("desk", "Ann", time, format!("m{n}"));
                events.push(ChatEvent::Message(message));
            }
            panel.poll(&egui::Context::default());
            let mut h = Harness::builder()
                .with_size(Vec2::new(320.0, 600.0))
                .with_context_setup(|ctx| {
                    crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                    egui_extras::install_image_loaders(ctx);
                    ctx.all_styles_mut(|style| {
                        style.interaction.tooltip_delay = 0.0;
                        style.interaction.show_tooltips_only_when_still = false;
                    });
                })
                .build_ui_state(|ui, panel: &mut ChatsPanel| panel.show(ui), panel);
            h.run();
            let painted: Vec<String> = h
                .output()
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .filter(|text| {
                    text.ends_with(", 2026")
                        || chrono::NaiveTime::parse_from_str(text, "%H:%M").is_ok()
                })
                .collect();
            assert_eq!(painted, expected);
            h.get_by_label(expected[1]).hover();
            h.run();
            h.get_by_label(hover);
        }
    }

    /// Guards the message menu: Reply quotes the message and answers it through the host, Copy
    /// link copies the host's link, and Ignore asks the host and hides the author's messages once
    /// it pushes them, with a list to stop ignoring. A reply shows the message it answers, or that
    /// the room no longer holds it. Without the capabilities the author is a plain label and Send
    /// posts a new message.
    #[test]
    fn message_menu_replies_links_and_ignores_through_the_host() {
        for available in [true, false] {
            let host = Arc::new(Host::default());
            for flag in [&host.replies, &host.links, &host.ignore_users] {
                flag.store(available, Ordering::Relaxed);
            }
            let mut panel = ChatsPanel::new();
            panel.set_service(host.clone());
            let events = host.events.lock().unwrap().clone().unwrap();
            let time = |minute| chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minute);
            for message in [
                ChatMessage::new("r", "Ana", time(0), "Morning")
                    .with_id("m1")
                    .with_author_id("ana"),
                ChatMessage::new("r", "Bo", time(1), "Same here")
                    .with_id("m2")
                    .with_reply_to("m1"),
                ChatMessage::new("r", "Bo", time(2), "Late")
                    .with_id("m3")
                    .with_reply_to("gone"),
            ] {
                events.push(ChatEvent::Message(message));
            }
            let ctx = egui::Context::default();
            panel.poll(&ctx);
            let mut h = Harness::builder()
                .with_size(Vec2::new(320.0, 600.0))
                .with_context_setup(|ctx| {
                    crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                    egui_extras::install_image_loaders(ctx);
                })
                .build_ui_state(|ui, panel: &mut ChatsPanel| panel.show(ui), panel);
            h.run();
            h.get_by_label("Ana: Morning");
            h.get_by_label("Reply to an earlier message");
            let write = |h: &mut Harness<'_, ChatsPanel>, text: &str| {
                h.get_by(|node| node.placeholder() == Some("Message r"))
                    .focus();
                h.run();
                h.get_by(|node| node.placeholder() == Some("Message r"))
                    .type_text(text);
                h.run();
                h.get_by_role_and_label(Role::Button, "Send").click();
                h.run();
            };
            if !available {
                h.get_by_role_and_label(Role::Label, "Ana");
                write(&mut h, "Agreed");
                assert_eq!(*host.sent.lock().unwrap(), [("r".into(), "Agreed".into())]);
                assert!(host.replied.lock().unwrap().is_empty());
                continue;
            }
            let menu = |h: &mut Harness<'_, ChatsPanel>, entry: &str| {
                h.get_by_role_and_label(Role::Button, "Ana").click();
                h.run();
                h.get_by_role_and_label(Role::Button, entry).click();
                h.step();
            };
            menu(&mut h, "Reply");
            h.run();
            h.get_by_label("Replying to Ana");
            write(&mut h, "Agreed");
            let expected = ("r".to_owned(), "Agreed".to_owned(), "m1".to_owned());
            assert_eq!(*host.replied.lock().unwrap(), [expected]);
            assert!(host.sent.lock().unwrap().is_empty());
            assert!(h.query_by_label("Replying to Ana").is_none());

            menu(&mut h, "Copy link");
            let copied = h.output().platform_output.commands.iter().any(|command| {
                command == &egui::OutputCommand::CopyText("https://example.invalid/r/m1".into())
            });
            assert!(copied);

            menu(&mut h, "Ignore Ana");
            let (user, ignore, reply) = host.ignores.lock().unwrap().pop().unwrap();
            assert_eq!((user.as_str(), ignore), ("ana", true));
            events.push(ChatEvent::Ignored(vec![ChatUser::new("ana", "Ana")]));
            reply.send(Ok(()));
            h.state_mut().poll(&ctx);
            h.run();
            assert!(h.query_by_label("Morning").is_none());
            assert!(h.query_by_label("Ana: Morning").is_none());
            assert_eq!(h.query_all_by_label("Reply to an earlier message").count(), 2);
            h.get_by_role_and_label(Role::Button, "Ignored users").click();
            h.run();
            h.get_by_label("Ana");
            h.get_by_role_and_label(Role::Button, "Stop ignoring").click();
            // Steps, not a run: the pending request shows a spinner, which repaints.
            h.run_steps(3);
            let (user, ignore, _) = host.ignores.lock().unwrap().pop().unwrap();
            assert_eq!((user.as_str(), ignore), ("ana", false));
        }
    }

    /// Guards a room's history against a hole: a page asked before a message the room has since
    /// lost to the bounds is dropped, and the next request asks before the new oldest message.
    #[test]
    fn a_page_asked_before_a_message_the_room_lost_is_dropped() {
        let host = Arc::new(Host::default());
        let mut panel = ChatsPanel::new();
        panel.set_service(host.clone());
        let events = host.events.lock().unwrap().clone().unwrap();
        let push = |room: &str, count: usize| {
            for i in 0..count {
                let text = format!("{room} {i}");
                let time = chrono::DateTime::UNIX_EPOCH;
                let message = ChatMessage::new(room, "Ann", time, text.clone()).with_id(text);
                events.push(ChatEvent::Message(message));
            }
        };
        let ctx = egui::Context::default();
        for room in ["desk", "r1", "r2", "r3", "r4"] {
            push(room, 999);
        }
        panel.poll(&ctx);
        panel.ask("desk");
        let (_, before, _, reply) = host.asked.lock().unwrap().pop().unwrap();
        assert_eq!(before.as_deref(), Some("desk 0"));

        // Past 5,000 in all, desk, the first of the rooms holding the most, loses "desk 0".
        push("r5", 6);
        panel.poll(&ctx);
        let older = chrono::DateTime::UNIX_EPOCH - chrono::TimeDelta::minutes(1);
        reply.send(Ok(vec![
            ChatMessage::new("desk", "Bo", older, "page").with_id("page"),
        ]));
        panel.poll(&ctx);
        assert_eq!(panel.messages["desk"][0].text, "desk 1");

        panel.ask("desk");
        let (_, before, _, _) = host.asked.lock().unwrap().pop().unwrap();
        assert_eq!(before.as_deref(), Some("desk 1"));
    }
}
