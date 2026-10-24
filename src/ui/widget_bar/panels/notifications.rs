//! The host's log of alert triggers and other events, newest first.

use std::collections::VecDeque;
use std::sync::Arc;

use egui::{RichText, Ui};

use crate::data::inbox::Inbox;
use crate::icons::icons;
use crate::services::{Listener, Notification, Notifications};
use crate::theme::ThemeExt;
use crate::ui_kit::{EmptyState, PanelHeader};

/// Entries kept at most; older ones are dropped.
const KEPT: usize = 1_000;

/// An action the host performs for the notifications panel.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum NotificationsAction {
    /// No host action.
    #[default]
    None,
    /// Show the symbol of an entry, such as on the chart.
    SelectSymbol(String),
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

    /// The entries in arrival order, the most recent 1,000.
    pub fn log(&self) -> &VecDeque<Notification> {
        &self.log
    }

    /// Entries pushed since the panel was last shown.
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// Receive the service's pushes, and watch again after a lagged stream.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.watch();
        }
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (_, answer) in self.inbox.drain().answers {
            if let Ok(entry) = answer.downcast::<Notification>() {
                self.log.push_back(*entry);
                self.unread += 1;
                if self.log.len() > KEPT {
                    self.log.pop_front();
                }
            }
        }
        self.unread = self.unread.min(self.log.len());
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
                    ui.label(
                        RichText::new(entry.time.format("%b %-d, %H:%M:%S UTC").to_string())
                            .size(theme.text.small)
                            .color(theme.ui.text_secondary),
                    );
                    ui.label(RichText::new(&entry.title).strong());
                    if !entry.body.is_empty() {
                        ui.label(&entry.body);
                    }
                    if let Some(symbol) = &entry.symbol
                        && ui.link(symbol).clicked()
                    {
                        action = NotificationsAction::SelectSymbol(symbol.clone());
                    }
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
        assert_eq!((panel.log().len(), panel.unread()), (1, 1));
        for n in 0..=10_000 {
            events.push(entry(n));
        }
        assert!(!events.is_active());
        panel.poll(&ctx);
        let renewed = host.0.lock().unwrap().clone().unwrap();
        renewed.push(entry(1));
        panel.poll(&ctx);
        assert_eq!(panel.log(), &[entry(1)]);
        assert_eq!(panel.unread(), 1);
    }
}
