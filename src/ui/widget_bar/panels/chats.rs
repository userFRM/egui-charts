//! The host's chat rooms: read a room and post to it, start chats and manage members.

use crate::localization as l10n;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use egui::{RichText, Ui};

use super::controls::dropdown;
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::icons::icons;
use crate::services::{
    ChatCapabilities, ChatEvent, ChatMessage, ChatUser, Chats, Listener, NewChatRoom,
    NewChatRoomKind,
};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, translated_status_message};
use crate::ui_kit::{EmptyState, IconButton, PanelHeader};

/// Messages kept at most across the rooms; older ones are dropped.
const KEPT: usize = 5_000;

/// Inbox key of the answers to [`Chats::send`]; streams and other requests use keys from 1.
const SENT: u64 = 0;

#[derive(Clone, Copy, Default, PartialEq)]
enum View {
    #[default]
    Room,
    NewChat,
    Members,
}

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
/// pushes it. Where the service's [`capabilities`](Chats::capabilities) allow, the header offers
/// to start a direct message or create a private or group room, choosing the recipients from
/// the service's [`find_users`](Chats::find_users), and to add or remove the members of a room
/// whose members the service pushes. Each action shows while it is pending and its failure, and
/// attaching another service clears them. Call [`poll`](Self::poll) each frame, even while the
/// panel is hidden, then [`show`](Self::show) while it is open. Accounts and moderation are the
/// host's.
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
    messages: VecDeque<ChatMessage>,
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
        self.open = None;
        self.action_error = None;
        self.watch();
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
        self.members.clear();
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
            if key != self.generation {
                continue;
            }
            match answer.downcast::<ChatEvent>().map(|event| *event) {
                Ok(ChatEvent::Room(room)) => self.add_room(&room),
                Ok(ChatEvent::Message(message)) => {
                    self.add_room(&message.room);
                    self.messages.push_back(message);
                    if self.messages.len() > KEPT {
                        self.messages.pop_front();
                    }
                }
                Ok(ChatEvent::Members { room, members }) => {
                    self.add_room(&room);
                    self.members.insert(room, members);
                }
                Ok(ChatEvent::Clear) => {
                    self.rooms.clear();
                    self.messages.clear();
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
        if (!can_create && self.view == View::NewChat)
            || (!can_manage && self.view == View::Members)
        {
            self.view = View::Room;
        }
        let mut view = self.view;
        PanelHeader::new("Chats").show(ui, |ui| {
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
        egui::Panel::bottom(ui.scope_id().with("chat_compose"))
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.add_space(theme.spacing.sm);
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for message in self.messages.iter().filter(|m| m.room == room) {
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
                                    ui.label(
                                        RichText::new(l10n::text(ui.ctx(), &message.author))
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(l10n::text(
                                            ui.ctx(),
                                            message.time.format("%H:%M").to_string(),
                                        ))
                                        .size(theme.text.small)
                                        .color(theme.ui.text_secondary),
                                    );
                                });
                                ui.label(l10n::text(ui.ctx(), &message.text));
                            });
                    });
                    ui.add_space(theme.spacing.sm);
                }
            });
        if send && let Some(service) = &self.service {
            let text = std::mem::take(self.draft.query_mut());
            service.send(room, text.trim().to_owned(), self.inbox.reply(SENT));
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

    #[derive(Default)]
    struct Host {
        created: Mutex<Vec<(NewChatRoom, Reply<String>)>>,
        events: Mutex<Option<Events<ChatEvent>>>,
    }

    impl Chats for Host {
        fn watch(&self, events: Events<ChatEvent>) {
            *self.events.lock().unwrap() = Some(events);
        }
        fn send(&self, _: String, _: String, reply: Reply<()>) {
            reply.send(Ok(()));
        }
        fn capabilities(&self) -> ChatCapabilities {
            ChatCapabilities {
                direct_messages: true,
                create_rooms: true,
                manage_members: false,
            }
        }
        fn create_room(&self, room: NewChatRoom, reply: Reply<String>) {
            self.created.lock().unwrap().push((room, reply));
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
}
