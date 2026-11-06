//! Host-supplied alerts and their trigger log. The host stores and evaluates alerts.

use crate::localization as l10n;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::{Color32, RichText, Ui};
use serde::{Deserialize, Serialize};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSortKey {
    /// Arrival order, as the host pushed the alerts.
    Arrival,
    /// Creation timestamp, with unknown times after known times.
    #[default]
    Created,
    /// Most recent trigger timestamp, with alerts that never fired last.
    Fired,
    /// The notification message, ignoring case.
    Message,
    /// The alert name, ignoring case.
    Name,
    /// The symbol, ignoring case.
    Symbol,
    /// Active, triggered, disabled, then expired.
    Status,
}

/// Saved presentation of the manager. Every optional field starts visible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertListOptions {
    /// Property to order by; creation time by default.
    pub sort: AlertSortKey,
    /// Order newest first or text in reverse alphabetical order; enabled by default. Choosing a
    /// key in the panel starts times newest first and other keys ascending.
    pub descending: bool,
    /// Show the user-assigned alert name.
    pub name: bool,
    /// Show the notification message.
    pub message: bool,
    /// Show an icon for the monitored source.
    pub source_icon: bool,
    /// Show the host's instrument image, or an initial while it is unavailable.
    pub symbol_icon: bool,
    /// Show the most recent firing time when known.
    pub last_firing: bool,
}

impl Default for AlertListOptions {
    fn default() -> Self {
        Self {
            sort: AlertSortKey::Created,
            descending: true,
            name: true,
            message: true,
            source_icon: true,
            symbol_icon: true,
            last_firing: true,
        }
    }
}

#[derive(Clone)]
enum PresentationEdit {
    List(AlertListOptions),
    Marker(AlertId, bool, bool),
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
    /// The trigger fell outside the alert's notification schedule: logged without a popup.
    pub silent: bool,
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
    options: AlertListOptions,
    marks: HashMap<String, egui::ImageSource<'static>>,
    undo: Vec<PresentationEdit>,
    redo: Vec<PresentationEdit>,
    restoring: Option<bool>,
    staged_display: Option<(AlertId, bool, bool)>,
    sources: Vec<(AlertSource, String)>,
    symbol: String,
    draft: Alert,
    inputs: Option<(crate::model::AlertInputs, Option<crate::model::Timeframe>)>,
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
            options: AlertListOptions::default(),
            marks: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            restoring: None,
            staged_display: None,
            sources: Vec::new(),
            symbol: String::new(),
            draft: Alert::new("", AlertCondition::Crossing(0.0)),
            inputs: None,
            draft_saving: false,
            pending: 0,
            sound_choice: None,
            sound: None,
        }
    }

    /// Manager presentation for the host to save independently of chart layouts.
    pub fn options(&self) -> &AlertListOptions {
        &self.options
    }

    /// Restore saved manager presentation. This starts a new presentation history.
    pub fn set_options(&mut self, options: AlertListOptions) {
        self.options = options;
        self.undo.clear();
        self.redo.clear();
        self.restoring = None;
    }

    /// Supply the instrument's image. Unknown, loading or unreadable images show an initial.
    pub fn set_mark(&mut self, symbol: &str, mark: egui::ImageSource<'static>) {
        self.marks.insert(symbol.to_owned(), mark);
    }

    fn remember(&mut self, edit: PresentationEdit) {
        self.undo.push(edit);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore_presentation(&mut self, redo: bool) {
        if self.pending > 0 || self.restoring.is_some() {
            return;
        }
        let Some(edit) = (if redo { &self.redo } else { &self.undo }).last().cloned() else {
            return;
        };
        match edit {
            PresentationEdit::List(options) => {
                let inverse = PresentationEdit::List(std::mem::replace(&mut self.options, options));
                self.finish_restore(redo, inverse);
            }
            PresentationEdit::Marker(id, line, blink) => {
                let Some(mut alert) = self
                    .alerts
                    .iter()
                    .find(|a| a.id.as_ref() == Some(&id))
                    .cloned()
                else {
                    return;
                };
                if alert.extend_line == line && alert.blink_on_trigger == blink {
                    self.finish_restore(redo, PresentationEdit::Marker(id, line, blink));
                    return;
                }
                alert.extend_line = line;
                alert.blink_on_trigger = blink;
                if self.save_with(alert, 0) {
                    self.restoring = Some(redo);
                }
            }
        }
    }

    fn finish_restore(&mut self, redo: bool, inverse: PresentationEdit) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        from.pop();
        to.push(inverse);
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

    /// Creation inputs for the inline form, captured from its source view by the host.
    pub fn set_inputs(
        &mut self,
        inputs: crate::model::AlertInputs,
        timeframe: Option<crate::model::Timeframe>,
    ) {
        self.inputs = Some((inputs, timeframe));
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
        self.marks.clear();
        self.undo.clear();
        self.redo.clear();
        self.restoring = None;
        self.staged_display = None;
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
    fn save_with(&mut self, mut alert: Alert, key: u64) -> bool {
        if alert.id.is_none() {
            alert.created_at = Some(Utc::now());
            alert.last_triggered_at = None;
        }
        let Some(service) = &self.service else {
            self.error = Some(FeedError::NotSupported);
            return false;
        };
        let display_only = self
            .alerts
            .iter()
            .any(|old| old.id == alert.id && old.same_monitoring(&alert));
        if !display_only && let Some((available, timeframe)) = &self.inputs {
            if let Some(inputs) = &mut alert.inputs {
                inputs.add_sources_from(available);
            } else if alert.id.is_none() {
                let mut inputs = available.clone();
                alert.timeframe = alert.timeframe.or(*timeframe);
                if alert.symbol != self.symbol
                    || alert.timeframe != *timeframe
                    || alert.watchlist.is_some()
                {
                    inputs.start = None;
                    inputs.visible = None;
                    inputs.count = inputs.count.max(1000);
                }
                alert.inputs = Some(Box::new(inputs));
            }
        }
        if self.alerts.iter().any(|old| {
            old.id == alert.id
                && (old.extend_line != alert.extend_line
                    || old.blink_on_trigger != alert.blink_on_trigger)
        }) {
            self.staged_display = alert
                .id
                .clone()
                .map(|id| (id, alert.extend_line, alert.blink_on_trigger));
        }
        self.pending += 1;
        service.save(alert, self.inbox.reply(key));
        true
    }

    fn last_firing(&self, alert: &Alert) -> Option<DateTime<Utc>> {
        alert
            .last_triggered_at
            .into_iter()
            .chain(
                self.log
                    .iter()
                    .filter(|trigger| Some(&trigger.id) == alert.id.as_ref())
                    .map(|trigger| trigger.time),
            )
            .max()
    }

    /// Missing timestamps stay last in either direction; ties use host identity after reconnects,
    /// shorter ids first so that numbered alerts keep their numeric order.
    fn sorted(&self) -> Vec<&Alert> {
        let mut alerts: Vec<&Alert> = self.alerts.iter().filter(|a| a.id.is_some()).collect();
        let rank = |a: &Alert| match a.status {
            AlertStatus::Active => 0,
            AlertStatus::Triggered => 1,
            AlertStatus::Disabled => 2,
            AlertStatus::Expired => 3,
        };
        alerts.sort_by(|a, b| {
            let timestamp = match self.options.sort {
                AlertSortKey::Created => Some((a.created_at, b.created_at)),
                AlertSortKey::Fired => Some((a.last_triggered_at, b.last_triggered_at)),
                _ => None,
            };
            if let Some((left, right)) = timestamp {
                match (left.is_some(), right.is_some()) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }
            let order = match self.options.sort {
                AlertSortKey::Arrival => return std::cmp::Ordering::Equal,
                AlertSortKey::Created => a.created_at.cmp(&b.created_at),
                AlertSortKey::Fired => a.last_triggered_at.cmp(&b.last_triggered_at),
                AlertSortKey::Message => a.message.to_lowercase().cmp(&b.message.to_lowercase()),
                AlertSortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                AlertSortKey::Symbol => a.symbol.to_lowercase().cmp(&b.symbol.to_lowercase()),
                AlertSortKey::Status => rank(a).cmp(&rank(b)),
            };
            let order = if self.options.descending {
                order.reverse()
            } else {
                order
            };
            order.then_with(|| {
                let (a, b) = (&a.id.as_ref().unwrap().0, &b.id.as_ref().unwrap().0);
                (a.len(), a).cmp(&(b.len(), b))
            })
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
            self.staged_display = None;
            self.restoring = None;
            self.watch();
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                if let Ok(event) = answer.downcast::<AlertEvent>() {
                    self.apply_event(ctx, *event, toasts);
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
                    Err(error) => {
                        self.restoring = None;
                        self.staged_display = None;
                        self.error = Some(error);
                    }
                }
            }
        }
    }

    fn apply_event(&mut self, ctx: &egui::Context, event: AlertEvent, toasts: &mut Toasts) {
        match event {
            AlertEvent::Upsert(alert) => {
                let mut alert = *alert;
                if alert.id.is_none() {
                    return;
                }
                alert.last_triggered_at = self.last_firing(&alert);
                if let Some(index) = self.alerts.iter().position(|a| a.id == alert.id) {
                    let old = &self.alerts[index];
                    if self
                        .staged_display
                        .as_ref()
                        .is_some_and(|(id, line, blink)| {
                            alert.id.as_ref() == Some(id)
                                && alert.extend_line == *line
                                && alert.blink_on_trigger == *blink
                        })
                    {
                        self.staged_display = None;
                        let inverse = PresentationEdit::Marker(
                            alert.id.clone().unwrap(),
                            old.extend_line,
                            old.blink_on_trigger,
                        );
                        if let Some(redo) = self.restoring.take() {
                            self.finish_restore(redo, inverse);
                        } else {
                            self.remember(inverse);
                        }
                    }
                    self.alerts[index] = alert;
                } else {
                    self.alerts.push(alert);
                }
            }
            AlertEvent::Remove(id) => {
                self.alerts.retain(|a| a.id.as_ref() != Some(&id));
                if self
                    .staged_display
                    .as_ref()
                    .is_some_and(|(kept, ..)| *kept == id)
                {
                    self.staged_display = None;
                    self.restoring = None;
                }
                self.undo.retain(
                    |edit| !matches!(edit, PresentationEdit::Marker(kept, ..) if *kept == id),
                );
                self.redo.retain(
                    |edit| !matches!(edit, PresentationEdit::Marker(kept, ..) if *kept == id),
                );
            }
            AlertEvent::Clear => {
                self.alerts.clear();
                self.undo.clear();
                self.redo.clear();
                self.restoring = None;
                self.staged_display = None;
            }
            AlertEvent::TriggeredForSymbol {
                id,
                symbol,
                time,
                price,
                message,
            } => {
                self.record_trigger(
                    ctx,
                    AlertTrigger {
                        id,
                        symbol,
                        time,
                        price,
                        message,
                        silent: false,
                    },
                    toasts,
                );
            }
            AlertEvent::SilentTriggered {
                id,
                symbol,
                time,
                price,
                message,
            } => {
                let price = price.filter(|p| p.is_finite());
                self.log_trigger(AlertTrigger {
                    id,
                    symbol,
                    time,
                    price,
                    message,
                    silent: true,
                });
            }
            AlertEvent::Triggered { id, time, price } => {
                let symbol = self
                    .alerts
                    .iter()
                    .find(|a| a.id.as_ref() == Some(&id))
                    .map(|a| a.symbol.clone())
                    .unwrap_or_default();
                self.record_trigger(
                    ctx,
                    AlertTrigger {
                        id,
                        symbol,
                        time,
                        price,
                        message: String::new(),
                        silent: false,
                    },
                    toasts,
                );
            }
        }
    }

    fn record_trigger(
        &mut self,
        ctx: &egui::Context,
        mut trigger: AlertTrigger,
        toasts: &mut Toasts,
    ) {
        let AlertTrigger {
            id,
            symbol,
            message,
            ..
        } = &trigger;
        let title = l10n::format_text(
            ctx,
            "Alert triggered: {subject}",
            &[(
                "subject",
                if symbol.is_empty() {
                    id.0.as_str()
                } else {
                    symbol.as_str()
                },
            )],
        );
        // A firing of an alert removed meanwhile keeps the default delivery.
        let delivery = self
            .alerts
            .iter()
            .find(|a| a.id.as_ref() == Some(id))
            .map(|a| &a.delivery);
        if delivery.is_none_or(|d| d.popup) {
            toasts.add(
                Toast::info(if message.is_empty() {
                    title
                } else {
                    format!("{title}\n{message}")
                })
                .duration(self.notification_seconds),
            );
            let extra = toasts.len().saturating_sub(5);
            toasts.toasts_mut().drain(..extra);
        }
        if delivery.is_none_or(|d| d.sound) {
            self.sound = self.sound_choice.clone();
        }
        if let Some(alert) = self
            .alerts
            .iter_mut()
            .find(|a| a.id.as_ref() == Some(&trigger.id))
        {
            alert.last_triggered_at = Some(
                alert
                    .last_triggered_at
                    .map_or(trigger.time, |old| old.max(trigger.time)),
            );
        }
        trigger.price = trigger.price.filter(|p| p.is_finite());
        self.log_trigger(trigger);
    }

    fn log_trigger(&mut self, trigger: AlertTrigger) {
        self.log.push_back(trigger);
        if self.log.len() > 10_000 {
            self.log.pop_front();
        }
    }

    /// Export the retained trigger log. Text fields are quoted and embedded quotes doubled.
    pub fn log_csv(&self) -> String {
        let mut csv = String::from("id,symbol,time,price,message,silent\r\n");
        for trigger in &self.log {
            let id = trigger.id.0.replace('"', "\"\"");
            let symbol = trigger.symbol.replace('"', "\"\"");
            let price = trigger.price.map(|p| p.to_string()).unwrap_or_default();
            let message = trigger.message.replace('"', "\"\"");
            csv.push_str(&format!(
                "\"{id}\",\"{symbol}\",{},{},\"{message}\",{}\r\n",
                trigger.time.to_rfc3339(),
                price,
                trigger.silent
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
        let before = self.options.clone();
        let mut history_action = None;
        ui.horizontal_wrapped(|ui| {
            // Completing a restore pops the last history entry, so nothing may add one meanwhile.
            if self.restoring.is_some() {
                ui.disable();
            }
            ui.label(l10n::text(ui.ctx(), "Sort by"));
            egui::ComboBox::from_id_salt("alert_sort")
                .selected_text(l10n::text(
                    ui.ctx(),
                    match self.options.sort {
                        AlertSortKey::Name => "Name",
                        AlertSortKey::Symbol => "Symbol",
                        AlertSortKey::Status => "Status",
                        AlertSortKey::Arrival => "Arrival",
                        AlertSortKey::Created => "Created",
                        AlertSortKey::Fired => "Last firing",
                        AlertSortKey::Message => "Message",
                    },
                ))
                .show_ui(ui, |ui| {
                    for (key, label) in [
                        (AlertSortKey::Arrival, "Arrival"),
                        (AlertSortKey::Created, "Created"),
                        (AlertSortKey::Fired, "Last firing"),
                        (AlertSortKey::Message, "Message"),
                        (AlertSortKey::Name, "Name"),
                        (AlertSortKey::Symbol, "Symbol"),
                        (AlertSortKey::Status, "Status"),
                    ] {
                        ui.selectable_value(
                            &mut self.options.sort,
                            key,
                            l10n::text(ui.ctx(), label),
                        );
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Sort by"));
            if self.options.sort != before.sort {
                self.options.descending = matches!(
                    self.options.sort,
                    AlertSortKey::Created | AlertSortKey::Fired
                );
            }
            ui.add_enabled(
                self.options.sort != AlertSortKey::Arrival,
                egui::Checkbox::new(
                    &mut self.options.descending,
                    l10n::text(ui.ctx(), "Descending"),
                ),
            );
            ui.menu_button(l10n::text(ui.ctx(), "Fields"), |ui| {
                for (value, label) in [
                    (&mut self.options.name, "Name"),
                    (&mut self.options.message, "Message"),
                    (&mut self.options.source_icon, "Source symbol"),
                    (&mut self.options.symbol_icon, "Instrument image"),
                    (&mut self.options.last_firing, "Last firing"),
                ] {
                    ui.checkbox(value, l10n::text(ui.ctx(), label));
                }
            });
            if ui
                .add_enabled(
                    !self.undo.is_empty() && self.pending == 0,
                    egui::Button::new(l10n::text(ui.ctx(), "Undo")),
                )
                .clicked()
            {
                history_action = Some(false);
            }
            if ui
                .add_enabled(
                    !self.redo.is_empty() && self.pending == 0,
                    egui::Button::new(l10n::text(ui.ctx(), "Redo")),
                )
                .clicked()
            {
                history_action = Some(true);
            }
        });
        if self.options != before {
            self.remember(PresentationEdit::List(before));
        }
        if let Some(redo) = history_action {
            self.restore_presentation(redo);
        }
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
                            "{} · {}{}",
                            trigger.symbol,
                            trigger.time.format("%Y-%m-%d %H:%M:%S UTC"),
                            if trigger.silent {
                                format!(" · {}", l10n::text(ui.ctx(), "Silent"))
                            } else {
                                String::new()
                            }
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
            self.sort_controls(ui);
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
            let mut presentation = None;
            egui::ScrollArea::vertical()
                .max_height(self.config.max_height)
                .show(ui, |ui| {
                    for alert in self.sorted() {
                        let Some(id) = &alert.id else { continue };
                        ui.push_id(&id.0, |ui| {
                            let subject = alert.watchlist.as_ref().map_or_else(
                                || alert.symbol.clone(),
                                |id| {
                                    l10n::format_text(
                                        ui.ctx(),
                                        "Watchlist: {name}",
                                        &[("name", id.as_str())],
                                    )
                                },
                            );
                            ui.horizontal(|ui| {
                                if self.options.symbol_icon {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::Vec2::splat(18.0),
                                        egui::Sense::hover(),
                                    );
                                    crate::painting::symbol_mark::paint_symbol_mark(
                                        ui.painter(),
                                        rect,
                                        &alert.symbol,
                                        self.marks.get(&alert.symbol).cloned(),
                                    );
                                }
                                if self.options.source_icon {
                                    let icon = match alert.source {
                                        AlertSource::Price => &icons::CHART_LINE,
                                        AlertSource::Volume => &icons::CHART_COLUMNS,
                                        AlertSource::Study { .. } => &icons::INDICATORS,
                                        AlertSource::Drawing(_) => &icons::TREND_LINE,
                                    };
                                    ui.add(icon.as_image_tinted(
                                        egui::Vec2::splat(16.0),
                                        ui.visuals().text_color(),
                                    ))
                                    .on_hover_text(l10n::text(ui.ctx(), "Alert source"));
                                }
                                ui.label(RichText::new(&subject).strong());
                            });
                            ui.label(l10n::text(ui.ctx(), alert.condition.to_string()));
                            if !alert.additional_conditions.is_empty() {
                                ui.label(l10n::format_text(
                                    ui.ctx(),
                                    "{0} additional conditions",
                                    &[("0", format!("{}", alert.additional_conditions.len()))],
                                ));
                            }
                            if self.options.name && !alert.name.is_empty() {
                                ui.label(l10n::text(ui.ctx(), &alert.name));
                            }
                            let color = self.status_color(ui, alert.status);
                            ui.horizontal(|ui| {
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
                                if let Some(schedule) = &alert.schedule {
                                    ui.add(icons::WIDGET_BAR_CALENDAR.as_image_tinted(
                                        egui::Vec2::splat(16.0),
                                        ui.visuals().text_color(),
                                    ))
                                    .on_hover_ui(|ui| schedule_tooltip(ui, schedule));
                                }
                            });
                            if self.options.message && !alert.message.is_empty() {
                                ui.label(&alert.message);
                            }
                            if self.options.last_firing
                                && let Some(time) = alert.last_triggered_at
                            {
                                let zone =
                                    alert.schedule.as_ref().map(|s| &s.timezone).or_else(|| {
                                        alert.inputs.as_ref().and_then(|i| i.timezone.as_ref())
                                    });
                                let zone =
                                    zone.cloned().unwrap_or_else(crate::model::TimezoneId::utc);
                                ui.label(l10n::format_text(
                                    ui.ctx(),
                                    "Last firing: {time}",
                                    &[(
                                        "time",
                                        time.with_timezone(&zone)
                                            .format("%Y-%m-%d %H:%M:%S %:z")
                                            .to_string(),
                                    )],
                                ));
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
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
                                ui.add_enabled_ui(self.pending == 0, |ui| {
                                    ui.menu_button("...", |ui| {
                                        let mut updated = alert.clone();
                                        let line = ui
                                            .checkbox(
                                                &mut updated.extend_line,
                                                l10n::text(ui.ctx(), "Line across chart"),
                                            )
                                            .changed();
                                        let blink = ui
                                            .checkbox(
                                                &mut updated.blink_on_trigger,
                                                l10n::text(ui.ctx(), "Blink after firing"),
                                            )
                                            .changed();
                                        if line || blink {
                                            presentation = Some(updated);
                                        }
                                    })
                                    .response
                                    .accessible_name(l10n::text(ui.ctx(), "Marker"))
                                    .on_hover_text(l10n::text(ui.ctx(), "Marker options"));
                                });
                                if Button::new("Duplicate").show(ui).clicked() {
                                    let mut copy = alert.clone();
                                    copy.id = None;
                                    copy.created_at = None;
                                    copy.last_triggered_at = None;
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
                        })
                        .response
                        .on_hover_ui(|ui| {
                            if let Some(schedule) = &alert.schedule {
                                schedule_tooltip(ui, schedule);
                            }
                        });
                    }
                });
            if let Some(alert) = presentation {
                self.save(alert);
            }
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

fn schedule_tooltip(ui: &mut Ui, schedule: &crate::model::NotificationSchedule) {
    ui.label(l10n::text(ui.ctx(), "Notification hours"));
    ui.label(schedule.timezone.to_string());
    let days = schedule
        .days
        .iter()
        .map(|day| l10n::text(ui.ctx(), day.to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    ui.label(days);
    for window in &schedule.windows {
        let range = format!(
            "{} - {}",
            window.start.format("%H:%M"),
            window.end.format("%H:%M")
        );
        ui.label(range);
        if window.end <= window.start {
            ui.label(l10n::text(ui.ctx(), "Ends on the following day"));
        }
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
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        events.push(AlertEvent::Triggered {
            id: alert.id.clone().unwrap(),
            time: "2026-10-23T12:00:00Z".parse().unwrap(),
            price: Some(12.5),
        });
        frame(&mut panel);
        alert.last_triggered_at = Some("2026-10-23T12:00:00Z".parse().unwrap());
        assert_eq!(panel.alerts(), &[alert.clone()]);
        assert_eq!(panel.alerts()[0].status, AlertStatus::Active);
        assert_eq!(panel.log().len(), 1);
        assert_eq!(
            panel.log_csv(),
            "id,symbol,time,price,message,silent\r\n\"host,\"\"1\",\"S\"\"IM\",2026-10-23T12:00:00+00:00,12.5,\"\",false\r\n"
        );
        alert.status = AlertStatus::Disabled;
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
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
        let mut display = alert.clone();
        display.extend_line = !display.extend_line;
        panel.save(display);
        host.reply
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(Err(FeedError::NotSupported));
        frame(&mut panel);
        assert_eq!(panel.alerts(), &[alert.clone()]);
        assert!(
            panel.undo.is_empty(),
            "a rejected display edit must not create undo history"
        );
        assert!(panel.staged_display.is_none());
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
            &egui::Context::default(),
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
                .ends_with(",123,\"AAPL closed at \"\"123\"\"\",false\r\n")
        );
        assert_eq!(panel.take_sound(), Some("Bell".into()));
        assert_eq!(panel.take_sound(), None);
        // A silent trigger is logged and marked, without a popup or a sound.
        let popups = toasts.toasts().len();
        panel.apply_event(
            &egui::Context::default(),
            AlertEvent::SilentTriggered {
                id: AlertId("watchlist".into()),
                symbol: "AAPL".into(),
                time: Utc::now(),
                price: Some(124.0),
                message: String::new(),
            },
            &mut toasts,
        );
        assert!(panel.log().back().unwrap().silent);
        assert!(panel.log_csv().ends_with(",124,\"\",true\r\n"));
        assert_eq!(toasts.toasts().len(), popups);
        assert_eq!(panel.take_sound(), None);
        drop(panel);
        assert!(!renewed.is_active());
    }

    #[test]
    fn sorting_orders_by_key_in_either_direction_and_breaks_ties_by_identity() {
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
            alert.created_at = match id {
                "1" | "3" => Some(DateTime::from_timestamp(200, 0).unwrap()),
                "2" => Some(DateTime::from_timestamp(100, 0).unwrap()),
                _ => None,
            };
            alert.last_triggered_at = match id {
                "2" | "3" => Some(DateTime::from_timestamp(300, 0).unwrap()),
                "4" => Some(DateTime::from_timestamp(100, 0).unwrap()),
                _ => None,
            };
            alert.message = match id {
                "1" | "3" => "beta",
                "2" => "Alpha",
                _ => "",
            }
            .into();
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
            (AlertSortKey::Created, false, "2134"),
            (AlertSortKey::Created, true, "1324"),
            (AlertSortKey::Fired, false, "4231"),
            (AlertSortKey::Fired, true, "2341"),
            (AlertSortKey::Message, false, "4213"),
            (AlertSortKey::Message, true, "1324"),
            (AlertSortKey::Name, false, "4213"),
            (AlertSortKey::Name, true, "1324"),
            (AlertSortKey::Symbol, false, "2314"),
            (AlertSortKey::Symbol, true, "4123"),
            (AlertSortKey::Status, false, "2314"),
            (AlertSortKey::Status, true, "4123"),
        ] {
            (panel.options.sort, panel.options.descending) = (key, descending);
            assert_eq!(ids(&panel), expected, "{key:?} descending={descending}");
            if key != AlertSortKey::Arrival {
                panel.alerts.reverse();
                assert_eq!(ids(&panel), expected, "reconnected {key:?}");
                panel.alerts.reverse();
            }
        }
        // Equal keys keep the host's numbering past nine alerts, in either direction, and ids
        // that are not numbers keep one order whatever the push order.
        let numbered: Vec<_> = (1..=12).map(|n| n.to_string()).collect();
        for (pushed, expected) in [
            (numbered.clone(), numbered.concat()),
            (vec!["b".into(), "aa".into(), "a".into()], "abaa".into()),
        ] {
            panel.alerts = pushed
                .iter()
                .map(|id| {
                    let mut alert = Alert::new("AAPL", AlertCondition::Above(1.0));
                    alert.id = Some(AlertId(id.clone()));
                    alert
                })
                .collect();
            for key in [
                AlertSortKey::Created,
                AlertSortKey::Symbol,
                AlertSortKey::Status,
            ] {
                for descending in [false, true] {
                    (panel.options.sort, panel.options.descending) = (key, descending);
                    assert_eq!(ids(&panel), expected, "{key:?} descending={descending}");
                    panel.alerts.reverse();
                    assert_eq!(ids(&panel), expected, "pushed reversed {key:?}");
                }
            }
        }
    }

    #[test]
    fn list_change_during_a_pending_undo_stays_undoable() {
        use egui_kittest::{Harness, kittest::Queryable};
        let host = Arc::new(Host::default());
        let answer = || host.reply.lock().unwrap().take().unwrap().send(Ok(()));
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        let events = host.events.lock().unwrap().clone().unwrap();
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(300.0, 700.0))
            .build_ui_state(
                move |ui, panel: &mut AlertsPanel| {
                    panel.poll(ui.ctx(), &mut toasts);
                    panel.show(ui);
                },
                panel,
            );
        let mut alert = Alert::new("SYM", AlertCondition::Above(1.0));
        alert.id = Some(AlertId("1".into()));
        alert.extend_line = true;
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        h.run_steps(3);
        // The marker menu turns the line off and the host applies it.
        alert.extend_line = false;
        h.state_mut().save(alert.clone());
        answer();
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        h.run_steps(3);
        // Undo it and press Descending before the host pushes the alert back.
        let descending = h.state().options.descending;
        h.get_by_label("Undo").click();
        h.run_steps(3);
        answer();
        h.run_steps(3);
        h.get_by_label("Descending").click();
        h.run_steps(3);
        alert.extend_line = true;
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        h.run_steps(3);
        h.get_by_label("Undo").click();
        h.run_steps(3);
        assert!(h.state().alerts()[0].extend_line);
        assert_eq!(h.state().options.descending, descending);
        // Once the restore has finished the direction changes again and Undo reverts it.
        h.get_by_label("Descending").click();
        h.run_steps(3);
        assert_eq!(h.state().options.descending, !descending);
        h.get_by_label("Undo").click();
        h.run_steps(3);
        assert_eq!(h.state().options.descending, descending);
        assert!(h.state().alerts()[0].extend_line);
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

    #[cfg(feature = "alert-engine")]
    #[test]
    fn manager_fields_and_markers_can_be_changed_undone_and_redone() {
        use egui_kittest::{Harness, kittest::Queryable};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let alert: Alert = serde_json::from_value(serde_json::json!({
                "id":"saved", "symbol":"SAMPLE", "condition":{"Above":100.0},
                "name":"Price watch", "message":"Notify me", "created_at":"2026-10-01T10:00:00Z",
                "last_triggered_at":"2026-10-02T11:30:00Z",
                "schedule":{"timezone":"+05:45", "days":["Fri"], "windows":[{"start":"22:00:00", "end":"02:00:00"}]}
            })).unwrap();
            let service = Arc::new(crate::services::AlertEngine::new().with_alerts(vec![alert]));
            let mut panel = AlertsPanel::default();
            panel.set_service(service.clone());
            let mut toasts = Toasts::new();
            let mut h = Harness::builder()
                .allow_missing_glyphs()
                .with_size(egui::vec2(300.0, 700.0))
                .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
                .build_ui_state(
                    move |ui, panel: &mut AlertsPanel| {
                        panel.poll(ui.ctx(), &mut toasts);
                        panel.show(ui);
                    },
                    panel,
                );
            h.run_steps(3);
            assert!(h.query_by_label("Price watch").is_some());
            assert!(h.query_by_label("Notify me").is_some());
            // 11:30 UTC + 5h45 is 17:15 on the schedule's clock.
            let firing = "Last firing: 2026-10-02 17:15:00 +05:45";
            assert!(h.query_by_label(firing).is_some());
            // The calendar marker sits beside the status and shows the row's schedule.
            let status = h.get_by_label("Active").rect();
            h.hover_at(egui::pos2(status.right() + 14.0, status.center().y));
            h.run_steps(60);
            assert!(h.query_by_label("+05:45").is_some());
            assert!(h.query_by_label("22:00 - 02:00").is_some());
            assert!(h.query_by_label("Ends on the following day").is_some());
            for (field, text) in [
                ("Name", "Price watch"),
                ("Message", "Notify me"),
                ("Last firing", firing),
            ] {
                h.get_by_label("Fields").click();
                h.run_steps(3);
                h.get_by_label(field).click();
                h.run_steps(3);
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert!(h.query_by_label(text).is_none(), "{field}");
                h.get_by_label("Undo").click();
                h.run_steps(3);
                assert!(h.query_by_label(text).is_some(), "undo {field}");
                h.get_by_label("Redo").click();
                h.run_steps(3);
                assert!(h.query_by_label(text).is_none(), "redo {field}");
            }
            // Choosing text starts alphabetically even though times start newest first.
            for (key, descending) in [("Message", false), ("Created", true)] {
                h.get_all_by_label("Sort by").last().unwrap().click();
                h.run_steps(3);
                h.get_all_by_label(key).last().unwrap().click();
                h.run_steps(3);
                assert_eq!(h.state().options.descending, descending, "{key}");
            }
            h.get_by_label("Marker").click();
            h.run_steps(3);
            h.get_by_label("Line across chart").click();
            h.run_steps(3);
            h.get_by_label("Marker").click();
            h.run_steps(3);
            h.get_by_label("Blink after firing").click();
            h.run_steps(3);
            h.key_press(egui::Key::Escape);
            h.run_steps(3);
            let saved = || serde_json::to_value(service.alerts()[0].clone()).unwrap();
            assert_eq!(saved()["extend_line"], false);
            assert_eq!(saved()["blink_on_trigger"], false);
            h.get_by_label("Undo").click();
            h.run_steps(3);
            assert_eq!(saved()["blink_on_trigger"], true);
            assert_eq!(saved()["extend_line"], false);
            h.get_by_label("Undo").click();
            h.run_steps(3);
            assert_eq!(saved()["extend_line"], true);
            h.get_by_label("Redo").click();
            h.run_steps(3);
            assert_eq!(saved()["extend_line"], false);
            // Save and restore the host's record, with no transient UI state to retain.
            let restored: Alert = serde_json::from_value(saved()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), saved());
        }
    }

    #[test]
    fn a_firing_is_logged_and_shown_only_on_the_channels_its_alert_keeps() {
        let host = Arc::new(Host::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        panel.set_sound(Some("Bell".into()));
        let events = host.events.lock().unwrap().clone().unwrap();
        let ctx = egui::Context::default();
        for (popup, sound) in [(true, true), (false, true), (true, false)] {
            let mut alert = Alert::new("SYM", AlertCondition::Above(1.0));
            alert.id = Some(AlertId("1".into()));
            alert.delivery.popup = popup;
            alert.delivery.sound = sound;
            let logged = panel.log().len();
            let mut toasts = Toasts::new();
            events.push(AlertEvent::Upsert(Box::new(alert.clone())));
            events.push(AlertEvent::Triggered {
                id: alert.id.clone().unwrap(),
                time: "2026-10-23T12:00:00Z".parse().unwrap(),
                price: None,
            });
            ctx.run_ui(egui::RawInput::default(), |ui| {
                panel.poll(ui.ctx(), &mut toasts)
            })
            .drop_without_applying_deltas();
            assert_eq!(panel.log().len(), logged + 1);
            assert_eq!(toasts.len(), usize::from(popup));
            assert_eq!(panel.take_sound().is_some(), sound);
        }
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
