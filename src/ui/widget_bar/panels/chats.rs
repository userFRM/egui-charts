//! The host's chat rooms: read a room and post to it.

use std::collections::VecDeque;
use std::sync::Arc;

use egui::{RichText, Ui};

use super::controls::dropdown;
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::icons::icons;
use crate::services::{ChatEvent, ChatMessage, Chats, Listener};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, PanelHeader};

/// Messages kept at most across the rooms; older ones are dropped.
const KEPT: usize = 5_000;

/// Inbox key of the answers to [`Chats::send`]; streams use keys from 1.
const SENT: u64 = 0;

/// Chat rooms pushed by the host's [`Chats`] service.
///
/// The user picks a room, reads its messages and posts; a posted message appears once the host
/// pushes it. Call [`poll`](Self::poll) each frame, even while the panel is hidden, then
/// [`show`](Self::show) while it is open. Accounts and moderation are the host's.
#[derive(Default)]
pub struct ChatsPanel {
    service: Option<Arc<dyn Chats>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    rooms: Vec<String>,
    room: usize,
    messages: VecDeque<ChatMessage>,
    draft: FilterWidget,
    error: Option<FeedError>,
}

impl ChatsPanel {
    /// An unconnected panel. Attach a service with [`set_service`](Self::set_service).
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a host service, clearing the previous one's rooms.
    pub fn set_service(&mut self, service: Arc<dyn Chats>) {
        self.service = Some(service);
        self.watch();
    }

    fn watch(&mut self) {
        self.rooms.clear();
        self.messages.clear();
        self.generation += 1;
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
                Ok(ChatEvent::Clear) => {
                    self.rooms.clear();
                    self.messages.clear();
                }
                Err(_) => {}
            }
        }
        self.room = self.room.min(self.rooms.len().saturating_sub(1));
    }

    /// Render the chosen room after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) {
        let theme = ui.active_theme();
        PanelHeader::new("Chats").show_plain(ui);
        let Some(room) = self.rooms.get(self.room).cloned() else {
            EmptyState::new("No chat rooms")
                .icon(&icons::WIDGET_BAR_CHATS)
                .fill_height()
                .show(ui);
            return;
        };
        dropdown(ui, "chat_room", &room, |ui| {
            for (index, name) in self.rooms.iter().enumerate() {
                ui.selectable_value(&mut self.room, index, name);
            }
        });
        let room = self.rooms[self.room].clone();
        if let Some(error) = &self.error {
            status_message(
                ui,
                MessageKind::Error,
                &format!("Message not sent: {error}"),
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
                            .placeholder(format!("Message {room}"))
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
                                    ui.label(RichText::new(&message.author).strong());
                                    ui.label(
                                        RichText::new(message.time.format("%H:%M").to_string())
                                            .size(theme.text.small)
                                            .color(theme.ui.text_secondary),
                                    );
                                });
                                ui.label(&message.text);
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
}
