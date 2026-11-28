//! The host's log of alert triggers and other events, newest first.

use crate::localization as l10n;
use std::collections::VecDeque;
use std::sync::Arc;

use egui::{RichText, Ui};

use crate::data::inbox::Inbox;
use crate::icons::icons;
use crate::services::{Listener, Notification, NotificationTarget, Notifications, ProfileId};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::{EmptyState, PanelHeader};

/// Entries kept at most; older ones are dropped.
const KEPT: usize = 1_000;

/// An action the host performs for the notifications panel.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum NotificationsAction {
    /// No host action.
    #[default]
    None,
    /// Show the symbol of an entry, such as on the chart.
    SelectSymbol(String),
    /// Show the profile an entry is about.
    OpenProfile(ProfileId),
    /// Show the chat room an entry is about, by the host's identifier, such as with
    /// [`ChatsPanel::open_room`](crate::ui::widget_bar::ChatsPanel::open_room).
    OpenRoom(String),
    /// Show the idea or post an entry is about, by the host's identifier, as the ideas panel's
    /// [`IdeasAction::Open`](crate::ui::ideas::IdeasAction::Open) does.
    OpenPost(String),
    /// Resolve the news story by its feed identifier and publication time, then pass it
    /// it to [`NewsPanel::open_story`](crate::ui::widget_bar::NewsPanel::open_story).
    OpenStory {
        /// The stable identifier assigned by the feed.
        id: String,
        /// The publication time of the story that triggered this notification.
        time: chrono::DateTime<chrono::Utc>,
    },
}

/// The log pushed by the host's [`Notifications`] service.
///
/// Call [`poll`](Self::poll) each frame, even while the panel is hidden, and show
/// [`unread`](Self::unread) as the bar entry's badge with
/// [`WidgetBar::set_notification_cnt`](crate::ui::widget_bar::WidgetBar::set_notification_cnt).
/// [`show`](Self::show) marks every entry read.
#[derive(Default)]
pub struct NotificationsPanel {
    service: Option<Arc<dyn Notifications>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    log: VecDeque<Notification>,
    unread: usize,
}

impl std::fmt::Debug for NotificationsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationsPanel")
            .field("generation", &self.generation)
            .field("log", &self.log)
            .field("unread", &self.unread)
            .finish_non_exhaustive()
    }
}

impl NotificationsPanel {
    /// An unconnected panel. Attach a service with [`set_service`](Self::set_service).
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a host service, clearing the previous one's entries.
    pub fn set_service(&mut self, service: Arc<dyn Notifications>) {
        self.service = Some(service);
        self.watch();
    }

    fn watch(&mut self) {
        self.log.clear();
        self.unread = 0;
        self.inbox = Arc::default();
        self.generation += 1;
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        if let Some(service) = &self.service {
            service.watch(events);
        }
    }

    /// Entries pushed since the panel was last shown.
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// Append a local entry, such as a terminal alert, to the same bounded log.
    pub fn push(&mut self, entry: Notification) {
        self.log.push_back(entry);
        if self.log.len() > KEPT {
            self.log.pop_front();
        }
        self.unread = (self.unread + 1).min(self.log.len());
    }

    /// Receive the service's pushes and return them, for popup notifications, and watch again
    /// after a lagged stream. Entries added with [`push`](Self::push) are not returned.
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<Notification> {
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.watch();
        }
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let mut received = Vec::new();
        for (_, answer) in self.inbox.drain().answers {
            if let Ok(entry) = answer.downcast::<Notification>() {
                self.push((*entry).clone());
                received.push(*entry);
            }
        }
        received
    }

    /// Render the log, newest first, after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> NotificationsAction {
        let mut action = NotificationsAction::None;
        let theme = ui.active_theme();
        self.unread = 0;
        PanelHeader::new("Notifications").show_plain(ui);
        if self.log.is_empty() {
            EmptyState::new("No notifications yet")
                .icon(&icons::WIDGET_BAR_NOTIFICATIONS)
                .fill_height()
                .show(ui);
            return action;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in self.log.iter().rev() {
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
                                ui.label(
                                    RichText::new(l10n::text(
                                        ui.ctx(),
                                        entry.time.format("%b %-d, %H:%M:%S UTC").to_string(),
                                    ))
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                                );
                                ui.label(
                                    RichText::new(l10n::text(ui.ctx(), &entry.title)).strong(),
                                );
                                if !entry.body.is_empty() {
                                    ui.label(l10n::text(ui.ctx(), &entry.body));
                                }
                                if let Some(symbol) = &entry.symbol
                                    && Button::new(symbol).small().show(ui).clicked()
                                {
                                    action = NotificationsAction::SelectSymbol(symbol.clone());
                                }
                                if let Some(target) = &entry.target {
                                    let (label, open) = match target {
                                        NotificationTarget::Profile(id) => (
                                            "Open profile",
                                            NotificationsAction::OpenProfile(id.clone()),
                                        ),
                                        NotificationTarget::Room(room) => (
                                            "Open room",
                                            NotificationsAction::OpenRoom(room.clone()),
                                        ),
                                        NotificationTarget::Post(id) => {
                                            ("Open post", NotificationsAction::OpenPost(id.clone()))
                                        }
                                        NotificationTarget::Story(id) => (
                                            "Open story",
                                            NotificationsAction::OpenStory {
                                                id: id.clone(),
                                                time: entry.time,
                                            },
                                        ),
                                    };
                                    if Button::new(l10n::text(ui.ctx(), label))
                                        .small()
                                        .show(ui)
                                        .clicked()
                                    {
                                        action = open;
                                    }
                                }
                            });
                    });
                    ui.separator();
                }
            });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::Events;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host(Mutex<Option<Events<Notification>>>);

    impl Notifications for Host {
        fn watch(&self, events: Events<Notification>) {
            *self.0.lock().unwrap() = Some(events);
        }
    }

    /// Every entry button carries the host's identifier for the thing it opens.
    #[test]
    fn each_entry_button_reports_its_own_action() {
        use egui::{Event, Modifiers, PointerButton, RawInput, Rect, Shape};
        for (symbol, target, label, expected) in [
            (
                Some("AAPL"),
                None,
                "AAPL",
                NotificationsAction::SelectSymbol("AAPL".into()),
            ),
            (
                None,
                Some(NotificationTarget::Profile(ProfileId("ada".into()))),
                "Open profile",
                NotificationsAction::OpenProfile(ProfileId("ada".into())),
            ),
            (
                None,
                Some(NotificationTarget::Room("desk".into())),
                "Open room",
                NotificationsAction::OpenRoom("desk".into()),
            ),
            (
                Some("AAPL"),
                Some(NotificationTarget::Post("42".into())),
                "Open post",
                NotificationsAction::OpenPost("42".into()),
            ),
            (
                None,
                Some(NotificationTarget::Post(String::new())),
                "Open post",
                NotificationsAction::OpenPost(String::new()),
            ),
            (
                Some("AAPL"),
                Some(NotificationTarget::Story("story-7".into())),
                "Open story",
                NotificationsAction::OpenStory {
                    id: "story-7".into(),
                    time: chrono::DateTime::from_timestamp(1, 0).unwrap(),
                },
            ),
        ] {
            let mut panel = NotificationsPanel::new();
            let mut entry =
                Notification::new(chrono::DateTime::from_timestamp(1, 0).unwrap(), "Update");
            entry.symbol = symbol.map(str::to_owned);
            entry.target = target;
            panel.push(entry);
            let ctx = egui::Context::default();
            let mut action = NotificationsAction::None;
            let mut step = |events| {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        action = panel.show(ui);
                    },
                )
            };
            let output = step(vec![]);
            let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                Shape::Text(text) if text.galley.text() == label => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()).center())
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            let pos = pos.expect(label);
            step(vec![Event::PointerMoved(pos)]).drop_without_applying_deltas();
            for pressed in [true, false] {
                step(vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }])
                .drop_without_applying_deltas();
            }
            assert_eq!(action, expected, "{label}");
        }
    }

    /// Guards the log against a lagged stream: the service pushes its recent entries again on
    /// the new stream, so the panel must start the log over rather than show them twice, and the
    /// badge must count only what is in the log.
    #[test]
    fn a_lagged_stream_starts_the_log_over() {
        let host = Arc::new(Host::default());
        let mut panel = NotificationsPanel::new();
        panel.set_service(host.clone());
        let ctx = egui::Context::default();
        let entry =
            |n: i64| Notification::new(chrono::DateTime::from_timestamp(n, 0).unwrap(), "x");
        let events = host.0.lock().unwrap().clone().unwrap();
        events.push(entry(1));
        panel.poll(&ctx);
        assert_eq!((panel.log.len(), panel.unread()), (1, 1));
        for n in 0..=10_000 {
            events.push(entry(n));
        }
        assert!(!events.is_active());
        panel.poll(&ctx);
        let renewed = host.0.lock().unwrap().clone().unwrap();
        renewed.push(entry(1));
        panel.poll(&ctx);
        assert_eq!(panel.log, [entry(1)]);
        assert_eq!(panel.unread(), 1);
    }
}
