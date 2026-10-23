//! Host-supplied alerts and their trigger log. The host stores and evaluates alerts.

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::{Color32, RichText, Ui};

use crate::data::{FeedError, inbox::Inbox};
use crate::icons::icons;
use crate::model::{Alert, AlertId, AlertStatus};
use crate::services::{AlertEvent, Alerts, Listener};
use crate::theme::ThemeExt;
use crate::ui_kit::notifications::Toasts;
use crate::ui_kit::{EmptyState, IconButton, PanelHeader};

/// Configuration for the alerts panel.
#[derive(Clone, Debug)]
pub struct AlertsPanelConfig {
    /// Maximum height of the scrolling list.
    pub max_height: f32,
}

impl Default for AlertsPanelConfig {
    fn default() -> Self {
        Self { max_height: 400.0 }
    }
}

/// An action the host handles using its shared alert dialog or file export.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum AlertPanelAction {
    /// No host action.
    #[default]
    None,
    /// Open the shared alert creation dialog; submit its result with [`AlertsPanel::save`].
    CreateAlert,
    /// Edit this host-owned alert in the shared dialog.
    EditAlert(Box<Alert>),
    /// Save this CSV text to a file chosen by the host.
    ExportLog(String),
}

/// One trigger the host reported, retained even after the alert is removed.
#[derive(Clone, Debug, PartialEq)]
pub struct AlertTrigger {
    /// Host alert identifier.
    pub id: AlertId,
    /// Symbol as known when the trigger arrived, empty if its alert was not yet supplied.
    pub symbol: String,
    /// Host trigger time, including triggers missed while the application was closed.
    pub time: DateTime<Utc>,
    /// Finite trigger price, if supplied.
    pub price: Option<f64>,
}

/// An alert panel connected to the host's [`Alerts`] service.
///
/// Call [`poll`](Self::poll) each frame even while hidden, so triggers produce notifications.
/// Creation returns an intent to open the shared alert dialog. Only host pushes change the list;
/// successful saves and removals never invent local state. Accounts, evaluation and delivery
/// outside the application remain the host's responsibility.
pub struct AlertsPanel {
    config: AlertsPanelConfig,
    service: Option<Arc<dyn Alerts>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    alerts: Vec<Alert>,
    log: VecDeque<AlertTrigger>,
    error: Option<FeedError>,
    show_log: bool,
}

impl Default for AlertsPanel {
    fn default() -> Self {
        Self::new(AlertsPanelConfig::default())
    }
}

impl AlertsPanel {
    /// Create an unconnected panel. Attach a service with [`set_service`](Self::set_service).
    pub fn new(config: AlertsPanelConfig) -> Self {
        Self {
            config,
            service: None,
            inbox: Arc::default(),
            listener: None,
            generation: 0,
            alerts: Vec::new(),
            log: VecDeque::new(),
            error: None,
            show_log: false,
        }
    }

    /// Attach a host service, clearing the previous account's alerts and log.
    pub fn set_service(&mut self, service: Arc<dyn Alerts>) {
        self.listener = None;
        self.service = Some(service);
        self.alerts.clear();
        self.log.clear();
        self.error = None;
        self.inbox = Arc::default();
        self.watch();
    }

    fn watch(&mut self) {
        self.generation += 1;
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        if let Some(service) = &self.service {
            service.watch(events);
        }
    }

    /// The latest alerts supplied by the host.
    pub fn alerts(&self) -> &[Alert] {
        &self.alerts
    }

    /// The most recent 10,000 host triggers in arrival order.
    pub fn log(&self) -> &VecDeque<AlertTrigger> {
        &self.log
    }

    /// Save a price alert from the shared dialog. The list changes only after a host push.
    pub fn save(&mut self, alert: Alert) {
        if !alert.is_valid() {
            self.error = Some(FeedError::InvalidData(
                "Alert contains invalid conditions".into(),
            ));
            return;
        }
        if let Some(service) = &self.service {
            service.save(alert, self.inbox.reply(0));
        } else {
            self.error = Some(FeedError::NotSupported);
        }
    }

    /// Drain service pushes and replies, recover a lagged stream, and append trigger notifications.
    /// Render `toasts` once per frame with the host's other notifications.
    pub fn poll(&mut self, ctx: &egui::Context, toasts: &mut Toasts) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.listener = None;
            self.alerts.clear();
            self.watch();
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                if let Ok(event) = answer.downcast::<AlertEvent>() {
                    self.apply_event(*event, toasts);
                }
            } else if key == 0
                && let Ok(result) = answer.downcast::<Result<(), FeedError>>()
            {
                match *result {
                    Ok(()) => self.error = None,
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }

    fn apply_event(&mut self, event: AlertEvent, toasts: &mut Toasts) {
        match event {
            AlertEvent::Upsert(alert) => {
                if alert.id.is_none() {
                    return;
                }
                if let Some(existing) = self.alerts.iter_mut().find(|a| a.id == alert.id) {
                    *existing = alert;
                } else {
                    self.alerts.push(alert);
                }
            }
            AlertEvent::Remove(id) => self.alerts.retain(|a| a.id.as_ref() != Some(&id)),
            AlertEvent::Clear => self.alerts.clear(),
            AlertEvent::Triggered { id, time, price } => {
                let symbol = self
                    .alerts
                    .iter()
                    .find(|a| a.id.as_ref() == Some(&id))
                    .map(|a| a.symbol.clone())
                    .unwrap_or_default();
                toasts.info(format!(
                    "Alert triggered: {}",
                    if symbol.is_empty() { &id.0 } else { &symbol }
                ));
                let extra = toasts.len().saturating_sub(5);
                toasts.toasts_mut().drain(..extra);
                self.log.push_back(AlertTrigger {
                    id,
                    symbol,
                    time,
                    price: price.filter(|p| p.is_finite()),
                });
                if self.log.len() > 10_000 {
                    self.log.pop_front();
                }
            }
        }
    }

    /// Export the retained trigger log. Text fields are quoted and embedded quotes doubled.
    pub fn log_csv(&self) -> String {
        let mut csv = String::from("id,symbol,time,price\r\n");
        for trigger in &self.log {
            let id = trigger.id.0.replace('"', "\"\"");
            let symbol = trigger.symbol.replace('"', "\"\"");
            let price = trigger.price.map(|p| p.to_string()).unwrap_or_default();
            csv.push_str(&format!(
                "\"{id}\",\"{symbol}\",{},{}\r\n",
                trigger.time.to_rfc3339(),
                price
            ));
        }
        csv
    }

    fn status_color(&self, ui: &Ui, status: AlertStatus) -> Color32 {
        let theme = ui.active_theme();
        match status {
            AlertStatus::Active => theme.ui.success,
            AlertStatus::Triggered => theme.ui.warning,
            AlertStatus::Disabled => theme.ui.text_disabled,
            AlertStatus::Expired => theme.ui.error,
        }
    }

    /// Render the host's alerts or trigger log after `poll`, returning creation or export intents.
    pub fn show(&mut self, ui: &mut Ui) -> AlertPanelAction {
        let mut action = AlertPanelAction::None;
        PanelHeader::new("Alerts").show(ui, |ui| {
            if IconButton::new(&icons::PLUS)
                .tooltip("New alert")
                .show(ui)
                .clicked()
            {
                action = AlertPanelAction::CreateAlert;
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.show_log, false, "Alerts");
            ui.selectable_value(&mut self.show_log, true, "Log");
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error.to_string());
        }
        if self.show_log {
            if ui
                .add_enabled(!self.log.is_empty(), egui::Button::new("Export CSV…"))
                .clicked()
            {
                action = AlertPanelAction::ExportLog(self.log_csv());
            }
            egui::ScrollArea::vertical()
                .max_height(self.config.max_height)
                .show(ui, |ui| {
                    for trigger in self.log.iter().rev() {
                        ui.label(format!(
                            "{} · {}",
                            trigger.symbol,
                            trigger.time.format("%Y-%m-%d %H:%M:%S UTC")
                        ));
                        if let Some(price) = trigger.price {
                            ui.label(price.to_string());
                        }
                        ui.separator();
                    }
                });
            if self.log.is_empty() {
                EmptyState::new("No triggers yet")
                    .icon(&icons::ALERTS)
                    .fill_height()
                    .show(ui);
            }
        } else if self.alerts.is_empty() {
            if EmptyState::new("No alerts yet")
                .icon(&icons::ALERTS)
                .description("Create an alert with the + button or the chart menu.")
                .action("Create alert")
                .fill_height()
                .show(ui)
            {
                action = AlertPanelAction::CreateAlert;
            }
        } else {
            let mut remove = None;
            let mut toggle = None;
            egui::ScrollArea::vertical()
                .max_height(self.config.max_height)
                .show(ui, |ui| {
                    for alert in &self.alerts {
                        let Some(id) = &alert.id else { continue };
                        ui.push_id(&id.0, |ui| {
                            ui.label(RichText::new(&alert.symbol).strong());
                            ui.label(alert.condition.to_string());
                            if !alert.name.is_empty() {
                                ui.label(&alert.name);
                            }
                            let color = self.status_color(ui, alert.status);
                            ui.colored_label(
                                color,
                                match alert.status {
                                    AlertStatus::Active => "Active",
                                    AlertStatus::Triggered => "Triggered",
                                    AlertStatus::Disabled => "Disabled",
                                    AlertStatus::Expired => "Expired",
                                },
                            );
                            if !alert.message.is_empty() {
                                ui.label(&alert.message);
                            }
                            ui.horizontal(|ui| {
                                if ui.button("Edit").clicked() {
                                    action = AlertPanelAction::EditAlert(Box::new(alert.clone()));
                                }
                                let active = alert.status == AlertStatus::Active;
                                if ui.button(if active { "Pause" } else { "Enable" }).clicked() {
                                    let mut changed = alert.clone();
                                    changed.status = if active {
                                        AlertStatus::Disabled
                                    } else {
                                        AlertStatus::Active
                                    };
                                    toggle = Some(changed);
                                }
                                if IconButton::new(&icons::TRASH)
                                    .tooltip("Remove alert")
                                    .show(ui)
                                    .clicked()
                                {
                                    remove = Some(id.clone());
                                }
                            });
                            ui.separator();
                        });
                    }
                });
            if let Some(alert) = toggle {
                self.save(alert);
            }
            if let Some(id) = remove
                && let Some(service) = &self.service
            {
                service.remove(id, self.inbox.reply(0));
            }
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::model::AlertCondition;
    use crate::services::Events;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host {
        events: Mutex<Option<Events<AlertEvent>>>,
        reply: Mutex<Option<Reply<()>>>,
    }

    impl Alerts for Host {
        fn watch(&self, events: Events<AlertEvent>) {
            *self.events.lock().unwrap() = Some(events);
        }
        fn save(&self, _: Alert, reply: Reply<()>) {
            *self.reply.lock().unwrap() = Some(reply);
        }
        fn remove(&self, _: AlertId, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    #[test]
    fn only_host_pushes_change_the_list_and_triggers_survive_removal_and_export() {
        let host = Arc::new(Host::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        let events = host.events.lock().unwrap().clone().unwrap();
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        let mut frame = |panel: &mut AlertsPanel| {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                panel.poll(ui.ctx(), &mut toasts)
            });
            output.drop_without_applying_deltas();
        };
        let mut alert = Alert::new("S\"IM", AlertCondition::CrossingUp(12.5));
        alert.id = Some(AlertId("host,\"1".into()));
        panel.save(alert.clone());
        host.reply.lock().unwrap().take().unwrap().send(Ok(()));
        frame(&mut panel);
        assert!(panel.alerts().is_empty());
        events.push(AlertEvent::Upsert(alert.clone()));
        events.push(AlertEvent::Triggered {
            id: alert.id.clone().unwrap(),
            time: "2026-10-23T12:00:00Z".parse().unwrap(),
            price: Some(12.5),
        });
        frame(&mut panel);
        assert_eq!(panel.alerts(), &[alert.clone()]);
        assert_eq!(panel.alerts()[0].status, AlertStatus::Active);
        assert_eq!(panel.log().len(), 1);
        assert_eq!(
            panel.log_csv(),
            "id,symbol,time,price\r\n\"host,\"\"1\",\"S\"\"IM\",2026-10-23T12:00:00+00:00,12.5\r\n"
        );
        alert.status = AlertStatus::Disabled;
        events.push(AlertEvent::Upsert(alert.clone()));
        frame(&mut panel);
        assert_eq!(panel.alerts(), &[alert.clone()]);
        panel.save(alert.clone());
        host.reply
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(Err(FeedError::NotSupported));
        frame(&mut panel);
        assert_eq!(panel.error, Some(FeedError::NotSupported));
        events.push(AlertEvent::Remove(alert.id.clone().unwrap()));
        frame(&mut panel);
        assert!(panel.alerts().is_empty());
        assert_eq!(panel.log().len(), 1);
        panel.save(alert);
        for _ in 0..10_001 {
            events.push(AlertEvent::Clear);
        }
        frame(&mut panel);
        assert!(!events.is_active());
        let renewed = host.events.lock().unwrap().clone().unwrap();
        assert!(renewed.is_active());
        host.reply
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(Err(FeedError::Unanswered));
        frame(&mut panel);
        assert_eq!(panel.error, Some(FeedError::Unanswered));
        assert_eq!(panel.log().len(), 1);
        drop(panel);
        assert!(!renewed.is_active());
    }

    #[test]
    fn default_alert_status_colours_follow_theme_changes() {
        let panel = AlertsPanel::default();
        let ctx = egui::Context::default();
        for (_, theme) in crate::theme::presets() {
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.run_ui(Default::default(), |ui| {
                for (status, expected) in [
                    (AlertStatus::Active, theme.ui.success),
                    (AlertStatus::Triggered, theme.ui.warning),
                    (AlertStatus::Disabled, theme.ui.text_disabled),
                    (AlertStatus::Expired, theme.ui.error),
                ] {
                    assert_eq!(panel.status_color(ui, status), expected);
                }
            })
            .drop_without_applying_deltas();
        }
    }
}
