//! Host-supplied alerts and their trigger log. The host stores and evaluates alerts.

use crate::localization as l10n;
use std::collections::VecDeque;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::{Color32, RichText, Ui};

use crate::data::{FeedError, inbox::Inbox};
use crate::icons::icons;
use crate::model::{Alert, AlertCondition, AlertId, AlertSource, AlertStatus};
use crate::services::{AlertEvent, Alerts, Listener};
use crate::theme::ThemeExt;
use crate::ui::dialogs::alert_dialog::rule_form;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::notifications::{Toast, Toasts};
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
#[non_exhaustive]
pub enum AlertPanelAction {
    /// No host action.
    #[default]
    None,
    /// Open the shared alert creation dialog; submit its result with [`AlertsPanel::save`].
    CreateAlert,
    /// Edit this host-owned alert in the shared dialog.
    EditAlert(Box<Alert>),
    /// Stage this copy of an alert, without an identifier and active, in the shared dialog
    /// with [`AlertDialog::edit`](crate::ui::dialogs::alert_dialog::AlertDialog::edit).
    DuplicateAlert(Box<Alert>),
    /// Save this CSV text to a file chosen by the host.
    ExportLog(String),
}

/// The property the alert list is ordered by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AlertSortKey {
    /// Arrival order, as the host pushed the alerts.
    #[default]
    Arrival,
    /// The alert name, ignoring case.
    Name,
    /// The symbol, ignoring case.
    Symbol,
    /// Active, triggered, disabled, then expired.
    Status,
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
    /// The alert's message with its placeholders filled in, empty if there is none.
    pub message: String,
}

/// Reply key of the inline draft's save, apart from the stream generations and other replies.
const DRAFT_REPLY: u64 = u64::MAX;

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
    notification_seconds: f32,
    sort: AlertSortKey,
    descending: bool,
    sources: Vec<(AlertSource, String)>,
    symbol: String,
    draft: Alert,
    draft_saving: bool,
    pending: usize,
    sound_choice: Option<String>,
    sound: Option<String>,
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
            notification_seconds: 5.0,
            sort: AlertSortKey::default(),
            descending: false,
            sources: Vec::new(),
            symbol: String::new(),
            draft: Alert::new("", AlertCondition::Crossing(0.0)),
            draft_saving: false,
            pending: 0,
            sound_choice: None,
            sound: None,
        }
    }

    /// Start the Quick alert form on `symbol`, such as the chart's. Call it every frame: the form
    /// follows a change of `symbol` and keeps what the user typed otherwise.
    pub fn set_symbol(&mut self, symbol: &str) {
        if self.symbol != symbol {
            self.symbol = symbol.to_owned();
            self.draft.symbol = symbol.to_owned();
        }
    }

    /// Offer these study outputs and drawings, as stable identifiers and display names, to the
    /// inline alert form.
    pub fn set_sources(&mut self, sources: Vec<(AlertSource, String)>) {
        self.sources = sources;
    }

    /// Keep each trigger notification on screen for this many seconds, five by default.
    pub fn set_notification_seconds(&mut self, seconds: f32) {
        self.notification_seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            5.0
        };
    }

    /// The sound to play when an alert fires, as the user chose it in the alert settings: its
    /// name, empty for the host's first sound, or `None` when the user turned the sound off.
    pub fn set_sound(&mut self, sound: Option<String>) {
        self.sound_choice = sound;
    }

    /// The sound to play for the triggers the last [`poll`](Self::poll) received, if it received
    /// one and [`set_sound`](Self::set_sound) holds a sound. The host plays it, as with
    /// `Trading::take_execution_sound`; call this after `poll`.
    pub fn take_sound(&mut self) -> Option<String> {
        self.sound.take()
    }

    /// Attach a host service, clearing the previous account's alerts and log.
    pub fn set_service(&mut self, service: Arc<dyn Alerts>) {
        self.sound = None;
        self.listener = None;
        self.service = Some(service);
        self.alerts.clear();
        self.log.clear();
        self.error = None;
        self.pending = 0;
        self.draft_saving = false;
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

    /// Save an alert from the shared dialog. The list changes only after a host push.
    pub fn save(&mut self, alert: Alert) {
        if !alert.is_valid() {
            self.error = Some(FeedError::InvalidData(
                "Alert contains invalid conditions".into(),
            ));
            return;
        }
        self.save_with(alert, 0);
    }

    /// Send `alert` and say whether a service took it; its reply arrives under `key`.
    fn save_with(&mut self, alert: Alert, key: u64) -> bool {
        let Some(service) = &self.service else {
            self.error = Some(FeedError::NotSupported);
            return false;
        };
        self.pending += 1;
        service.save(alert, self.inbox.reply(key));
        true
    }

    /// Alerts ordered by `key`, reversed when `descending`; equal values keep arrival order.
    fn sorted(&self) -> Vec<&Alert> {
        let mut alerts: Vec<&Alert> = self.alerts.iter().filter(|a| a.id.is_some()).collect();
        let rank = |a: &Alert| match a.status {
            AlertStatus::Active => 0,
            AlertStatus::Triggered => 1,
            AlertStatus::Disabled => 2,
            AlertStatus::Expired => 3,
        };
        alerts.sort_by(|a, b| {
            let order = match self.sort {
                AlertSortKey::Arrival => return std::cmp::Ordering::Equal,
                AlertSortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                AlertSortKey::Symbol => a.symbol.to_lowercase().cmp(&b.symbol.to_lowercase()),
                AlertSortKey::Status => rank(a).cmp(&rank(b)),
            };
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        alerts
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
            } else if (key == 0 || key == DRAFT_REPLY)
                && let Ok(result) = answer.downcast::<Result<(), FeedError>>()
            {
                self.pending = self.pending.saturating_sub(1);
                self.draft_saving &= key != DRAFT_REPLY;
                match *result {
                    Ok(()) => {
                        self.error = None;
                        if key == DRAFT_REPLY {
                            self.draft =
                                Alert::new(self.symbol.clone(), AlertCondition::Crossing(0.0));
                        }
                    }
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
            AlertEvent::TriggeredForSymbol {
                id,
                symbol,
                time,
                price,
                message,
            } => {
                self.record_trigger(id, symbol, time, price, message, toasts);
            }
            AlertEvent::Triggered { id, time, price } => {
                let symbol = self
                    .alerts
                    .iter()
                    .find(|a| a.id.as_ref() == Some(&id))
                    .map(|a| a.symbol.clone())
                    .unwrap_or_default();
                self.record_trigger(id, symbol, time, price, String::new(), toasts);
            }
        }
    }

    fn record_trigger(
        &mut self,
        id: AlertId,
        symbol: String,
        time: DateTime<Utc>,
        price: Option<f64>,
        message: String,
        toasts: &mut Toasts,
    ) {
        let title = format!(
            "Alert triggered: {}",
            if symbol.is_empty() { &id.0 } else { &symbol }
        );
        toasts.add(
            Toast::info(if message.is_empty() {
                title
            } else {
                format!("{title}\n{message}")
            })
            .duration(self.notification_seconds),
        );
        self.sound = self.sound_choice.clone();
        let extra = toasts.len().saturating_sub(5);
        toasts.toasts_mut().drain(..extra);
        self.log.push_back(AlertTrigger {
            id,
            symbol,
            time,
            price: price.filter(|p| p.is_finite()),
            message,
        });
        if self.log.len() > 10_000 {
            self.log.pop_front();
        }
    }

    /// Export the retained trigger log. Text fields are quoted and embedded quotes doubled.
    pub fn log_csv(&self) -> String {
        let mut csv = String::from("id,symbol,time,price,message\r\n");
        for trigger in &self.log {
            let id = trigger.id.0.replace('"', "\"\"");
            let symbol = trigger.symbol.replace('"', "\"\"");
            let price = trigger.price.map(|p| p.to_string()).unwrap_or_default();
            let message = trigger.message.replace('"', "\"\"");
            csv.push_str(&format!(
                "\"{id}\",\"{symbol}\",{},{},\"{message}\"\r\n",
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

    fn sort_controls(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(l10n::text(ui.ctx(), "Sort by"));
            egui::ComboBox::from_id_salt("alert_sort")
                .selected_text(l10n::text(
                    ui.ctx(),
                    match self.sort {
                        AlertSortKey::Name => "Name",
                        AlertSortKey::Symbol => "Symbol",
                        AlertSortKey::Status => "Status",
                        AlertSortKey::Arrival => "Arrival",
                    },
                ))
                .show_ui(ui, |ui| {
                    for (key, label) in [
                        (AlertSortKey::Arrival, "Arrival"),
                        (AlertSortKey::Name, "Name"),
                        (AlertSortKey::Symbol, "Symbol"),
                        (AlertSortKey::Status, "Status"),
                    ] {
                        ui.selectable_value(&mut self.sort, key, l10n::text(ui.ctx(), label));
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Sort by"));
            ui.add_enabled(
                self.sort != AlertSortKey::Arrival,
                egui::Checkbox::new(&mut self.descending, l10n::text(ui.ctx(), "Descending")),
            );
        });
    }

    /// The inline form for a price, study-output or volume alert; the shared validation and the
    /// connected service decide whether it saves.
    fn draft_form(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new(l10n::text(ui.ctx(), "Quick alert"))
            .id_salt("alert_draft")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(l10n::text(ui.ctx(), "Symbol"));
                    ui.text_edit_singleline(&mut self.draft.symbol)
                        .accessible_name(l10n::text(ui.ctx(), "Symbol"));
                });
                rule_form(
                    ui,
                    &mut self.draft.source,
                    &mut self.draft.condition,
                    &self.sources,
                );
                if self.draft_saving {
                    ui.label(l10n::text(ui.ctx(), "Saving…"));
                }
                if ui
                    .add_enabled(
                        self.draft.is_valid() && !self.draft_saving,
                        egui::Button::new(l10n::text(ui.ctx(), "Create alert")),
                    )
                    .clicked()
                {
                    self.draft_saving = self.save_with(self.draft.clone(), DRAFT_REPLY);
                }
            });
        if self.pending > 0 {
            ui.label(l10n::text(ui.ctx(), "Waiting for the alerts service…"));
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
            ui.selectable_value(&mut self.show_log, false, l10n::text(ui.ctx(), "Alerts"));
            ui.selectable_value(&mut self.show_log, true, l10n::text(ui.ctx(), "Log"));
        });
        if let Some(error) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                l10n::text(ui.ctx(), error.to_string()),
            );
        }
        if self.show_log {
            if Button::new("Export CSV…")
                .enabled(!self.log.is_empty())
                .show(ui)
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
                            ui.label(l10n::text(ui.ctx(), price.to_string()));
                        }
                        if !trigger.message.is_empty() {
                            ui.label(&trigger.message);
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
            self.draft_form(ui);
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
            self.draft_form(ui);
            self.sort_controls(ui);
            let mut remove = None;
            let mut toggle = None;
            let mut duplicate = None;
            egui::ScrollArea::vertical()
                .max_height(self.config.max_height)
                .show(ui, |ui| {
                    for alert in self.sorted() {
                        let Some(id) = &alert.id else { continue };
                        ui.push_id(&id.0, |ui| {
                            let subject = alert.watchlist.as_ref().map_or_else(
                                || alert.symbol.clone(),
                                |id| format!("Watchlist: {id}"),
                            );
                            ui.label(RichText::new(l10n::text(ui.ctx(), subject)).strong());
                            ui.label(l10n::text(ui.ctx(), alert.condition.to_string()));
                            if !alert.additional_conditions.is_empty() {
                                ui.label(l10n::format_text(
                                    ui.ctx(),
                                    "{0} additional conditions",
                                    &[("0", format!("{}", alert.additional_conditions.len()))],
                                ));
                            }
                            if !alert.name.is_empty() {
                                ui.label(l10n::text(ui.ctx(), &alert.name));
                            }
                            let color = self.status_color(ui, alert.status);
                            ui.colored_label(
                                color,
                                l10n::text(
                                    ui.ctx(),
                                    match alert.status {
                                        AlertStatus::Active => "Active",
                                        AlertStatus::Triggered => "Triggered",
                                        AlertStatus::Disabled => "Disabled",
                                        AlertStatus::Expired => "Expired",
                                    },
                                ),
                            );
                            if !alert.message.is_empty() {
                                ui.label(l10n::text(ui.ctx(), &alert.message));
                            }
                            ui.horizontal_wrapped(|ui| {
                                if Button::new("Edit").show(ui).clicked() {
                                    action = AlertPanelAction::EditAlert(Box::new(alert.clone()));
                                }
                                let active = alert.status == AlertStatus::Active;
                                if Button::new(if active { "Pause" } else { "Enable" })
                                    .show(ui)
                                    .clicked()
                                {
                                    let mut changed = alert.clone();
                                    changed.status = if active {
                                        AlertStatus::Disabled
                                    } else {
                                        AlertStatus::Active
                                    };
                                    toggle = Some(changed);
                                }
                                if Button::new("Duplicate").show(ui).clicked() {
                                    let mut copy = alert.clone();
                                    copy.id = None;
                                    copy.status = AlertStatus::Active;
                                    duplicate = Some(copy);
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
                self.pending += 1;
                service.remove(id, self.inbox.reply(0));
            }
            if let Some(alert) = duplicate {
                action = AlertPanelAction::DuplicateAlert(Box::new(alert));
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
        panel.set_notification_seconds(12.0);
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
            "id,symbol,time,price,message\r\n\"host,\"\"1\",\"S\"\"IM\",2026-10-23T12:00:00+00:00,12.5,\"\"\r\n"
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
        assert_eq!(toasts.toasts()[0].duration, 12.0);
        // The sound the user chose comes with the triggers received, and the message is shown in
        // the popup and kept in the log and the export.
        assert_eq!(panel.take_sound(), None);
        panel.set_sound(Some("Bell".into()));
        panel.apply_event(
            AlertEvent::TriggeredForSymbol {
                id: AlertId("watchlist".into()),
                symbol: "AAPL".into(),
                time: Utc::now(),
                price: Some(123.0),
                message: "AAPL closed at \"123\"".into(),
            },
            &mut toasts,
        );
        assert!(toasts.toasts().iter().all(|toast| toast.duration == 12.0));
        assert_eq!(
            toasts.toasts().last().unwrap().message,
            "Alert triggered: AAPL\nAAPL closed at \"123\""
        );
        assert_eq!(panel.log().back().unwrap().symbol, "AAPL");
        assert_eq!(
            panel.log().back().unwrap().message,
            "AAPL closed at \"123\""
        );
        assert!(
            panel
                .log_csv()
                .ends_with(",123,\"AAPL closed at \"\"123\"\"\"\r\n")
        );
        assert_eq!(panel.take_sound(), Some("Bell".into()));
        assert_eq!(panel.take_sound(), None);
        drop(panel);
        assert!(!renewed.is_active());
    }

    #[test]
    fn sorting_orders_by_key_in_either_direction_and_keeps_arrival_order_for_equal_values() {
        let mut panel = AlertsPanel::default();
        for (id, name, symbol, status) in [
            ("1", "beta", "MSFT", AlertStatus::Disabled),
            ("2", "Alpha", "aapl", AlertStatus::Active),
            ("3", "beta", "AAPL", AlertStatus::Active),
            ("4", "", "TSLA", AlertStatus::Expired),
        ] {
            let mut alert = Alert::new(symbol, AlertCondition::Above(1.0));
            alert.id = Some(AlertId(id.into()));
            alert.name = name.into();
            alert.status = status;
            panel.alerts.push(alert);
        }
        let ids = |panel: &AlertsPanel| {
            panel
                .sorted()
                .iter()
                .map(|a| a.id.as_ref().unwrap().0.as_str())
                .collect::<String>()
        };
        for (key, descending, expected) in [
            (AlertSortKey::Arrival, true, "1234"),
            (AlertSortKey::Name, false, "4213"),
            (AlertSortKey::Name, true, "1324"),
            (AlertSortKey::Symbol, false, "2314"),
            (AlertSortKey::Symbol, true, "4123"),
            (AlertSortKey::Status, false, "2314"),
            (AlertSortKey::Status, true, "4123"),
        ] {
            (panel.sort, panel.descending) = (key, descending);
            assert_eq!(ids(&panel), expected, "{key:?} descending={descending}");
        }
    }

    #[test]
    fn inline_draft_stays_pending_until_the_service_answers_and_survives_a_failure() {
        let host = Arc::new(Host::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        let mut frame = |panel: &mut AlertsPanel| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                panel.poll(ui.ctx(), &mut toasts)
            })
            .drop_without_applying_deltas();
        };
        panel.set_symbol("SIM");
        panel.draft.condition = AlertCondition::CrossingUp(10.0);
        panel.draft_saving = panel.save_with(panel.draft.clone(), DRAFT_REPLY);
        assert_eq!((panel.pending, panel.draft_saving), (1, true));
        host.reply
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(Err(FeedError::NotSupported));
        frame(&mut panel);
        assert_eq!((panel.pending, panel.draft_saving), (0, false));
        assert_eq!(panel.error, Some(FeedError::NotSupported));
        assert_eq!(panel.draft.symbol, "SIM");
        panel.draft_saving = panel.save_with(panel.draft.clone(), DRAFT_REPLY);
        host.reply.lock().unwrap().take().unwrap().send(Ok(()));
        frame(&mut panel);
        assert_eq!(panel.error, None);
        assert_eq!(panel.draft.symbol, "SIM");
        assert_eq!(panel.draft.condition, AlertCondition::Crossing(0.0));
        assert!(panel.alerts().is_empty());
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
