//! Host-supplied alerts and their trigger log. The host stores and evaluates alerts.

use crate::localization as l10n;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::{Color32, EventFilter, Key, Modifiers, RichText, Sense, Ui};
use serde::{Deserialize, Serialize};

use crate::data::{FeedError, inbox::Inbox};
use crate::icons::icons;
pub use crate::model::AlertTrigger;
use crate::model::{
    Alert, AlertCondition, AlertId, AlertPositionProgress, AlertSource, AlertSourceName,
    AlertStatus,
};
use crate::services::{AlertEvent, Alerts, Listener, Notification, Notifications};
use crate::theme::ThemeExt;
use crate::ui::dialogs::alert_dialog::rule_form;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::modal::dialogs::ConfirmDialog;
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

/// An action the host handles using its shared alert dialog, file export or chart.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum AlertPanelAction {
    /// No host action.
    #[default]
    None,
    /// Open the shared alert creation dialog; submit its result with [`AlertsPanel::save`].
    CreateAlert,
    /// Edit this host-owned alert in the shared dialog. Enable on an alert whose expiry has passed
    /// returns it too, already marked active, so the user chooses a later expiry or none before it
    /// is saved.
    EditAlert(Box<Alert>),
    /// Stage this copy of an alert, without an identifier and active, in the shared dialog
    /// with [`AlertDialog::edit`](crate::ui::dialogs::alert_dialog::AlertDialog::edit). The
    /// copy of a position alert waits for its entry, whatever the original has reached.
    DuplicateAlert(Box<Alert>),
    /// Save this CSV text to a file chosen by the host.
    ExportLog(String),
    /// Show this symbol on the chart: Open chart on a trigger popup, with the symbol that fired,
    /// a watchlist member for an alert on a watchlist.
    SelectSymbol(String),
}

/// The property the alert list is ordered by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
///
/// ```
/// use egui_charts::ui::widget_bar::AlertListOptions;
///
/// let mut options = AlertListOptions::default();
/// options.descending = true;
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
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
    /// Show the symbol of each log entry.
    pub log_symbol: bool,
    /// Show the price of each log entry.
    pub log_price: bool,
    /// Show the message of each log entry.
    pub log_message: bool,
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
            log_symbol: true,
            log_price: true,
            log_message: true,
        }
    }
}

#[derive(Clone, Debug)]
enum PresentationEdit {
    List(AlertListOptions),
    Marker(AlertId, bool, bool),
}

// Reserve both history positions while the host confirms the marker change.
#[derive(Debug)]
struct PresentationRestore {
    redo: bool,
    request: u64,
    source_index: usize,
    destination_index: usize,
    inverse: PresentationEdit,
}

/// Reply key of the inline draft's save, apart from the stream generations and other replies.
const DRAFT_REPLY: u64 = u64::MAX;

/// Triggers the log keeps, and toasts the stack keeps while they wait to be dismissed.
const LOG_BOUND: usize = 10_000;

/// An alert panel connected to the host's [`Alerts`] service.
///
/// Call [`poll`](Self::poll) each frame even while hidden, so triggers produce notifications.
/// Creation returns an intent to open the shared alert dialog. Only host pushes change the list;
/// successful saves and removals never invent local state. Accounts, evaluation and delivery
/// outside the application remain the host's responsibility. The Log tab narrows the log to one
/// alert or one symbol and clears the entries it shows after a confirmation, also from the
/// built-in engine's history.
pub struct AlertsPanel {
    config: AlertsPanelConfig,
    service: Option<Arc<dyn Alerts>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    alerts: Vec<Alert>,
    log: VecDeque<AlertTrigger>,
    /// Keeps row ids stable when the bounded log drops its oldest entries.
    log_offset: usize,
    /// Selected alert and log entry; the log index follows eviction of the oldest entry.
    selected: Option<AlertId>,
    log_selected: Option<usize>,
    /// The alert whose removal the Delete key asks to confirm.
    removing: Option<AlertId>,
    restore_row_focus: bool,
    /// The alert and the symbol the Log tab narrows to.
    log_alert: Option<AlertId>,
    log_symbol: Option<String>,
    /// The confirmation of clearing the log is open.
    clearing: bool,
    error: Option<FeedError>,
    show_log: bool,
    notification_seconds: f32,
    timezone: crate::model::TimezoneId,
    options: AlertListOptions,
    marks: HashMap<String, egui::ImageSource<'static>>,
    undo: Vec<PresentationEdit>,
    redo: Vec<PresentationEdit>,
    restoring: Option<PresentationRestore>,
    staged_display: Vec<(u64, PresentationEdit, bool)>,
    next_reply: u64,
    sources: Vec<(AlertSource, AlertSourceName)>,
    symbol: String,
    last_price: Option<f64>,
    draft: Alert,
    draft_level_text: String,
    draft_level_edited: bool,
    inputs: Option<(crate::model::AlertInputs, Option<crate::model::Timeframe>)>,
    draft_saving: bool,
    pending: usize,
    sound_choice: Option<String>,
    sound: Option<String>,
    desktop: Option<Arc<dyn Notifications>>,
    desktop_inbox: Arc<Inbox>,
    desktop_toast: Option<u64>,
    popup_actions: bool,
    /// Each trigger popup that offers actions: its toast, its alert and the symbol to show.
    popups: Vec<(u64, AlertId, Option<String>)>,
    popup_action: AlertPanelAction,
}

impl std::fmt::Debug for AlertsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlertsPanel")
            .field("config", &self.config)
            .field("generation", &self.generation)
            .field("alerts", &self.alerts)
            .field("log", &self.log)
            .field("error", &self.error)
            .field("show_log", &self.show_log)
            .field("notification_seconds", &self.notification_seconds)
            .field("timezone", &self.timezone)
            .field("options", &self.options)
            .field("marks", &self.marks)
            .field("next_reply", &self.next_reply)
            .field("sources", &self.sources)
            .field("symbol", &self.symbol)
            .field("draft", &self.draft)
            .field("inputs", &self.inputs)
            .field("draft_saving", &self.draft_saving)
            .field("pending", &self.pending)
            .field("sound_choice", &self.sound_choice)
            .field("sound", &self.sound)
            .finish_non_exhaustive()
    }
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
            log_offset: 0,
            selected: None,
            log_selected: None,
            removing: None,
            restore_row_focus: false,
            log_alert: None,
            log_symbol: None,
            clearing: false,
            error: None,
            show_log: false,
            notification_seconds: 20.0,
            timezone: crate::model::TimezoneId::utc(),
            options: AlertListOptions::default(),
            marks: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            restoring: None,
            staged_display: Vec::new(),
            next_reply: DRAFT_REPLY - 1,
            sources: Vec::new(),
            symbol: String::new(),
            last_price: None,
            draft: Alert::new("", AlertCondition::Crossing(f64::NAN)),
            draft_level_text: String::new(),
            draft_level_edited: false,
            inputs: None,
            draft_saving: false,
            pending: 0,
            sound_choice: None,
            sound: None,
            desktop: None,
            desktop_inbox: Arc::default(),
            desktop_toast: None,
            popup_actions: false,
            popups: Vec::new(),
            popup_action: AlertPanelAction::None,
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
        self.staged_display.clear();
    }

    /// Supply the instrument's image. Unknown, loading or unreadable images show an initial.
    pub fn set_mark(&mut self, symbol: &str, mark: egui::ImageSource<'static>) {
        self.marks.insert(symbol.to_owned(), mark);
    }

    fn remember(&mut self, edit: PresentationEdit, keep_redo: bool) {
        self.undo.push(edit);
        if self.undo.len() > 100 {
            self.undo.remove(0);
            if let Some(restore) = &mut self.restoring {
                if restore.redo {
                    restore.destination_index = restore.destination_index.saturating_sub(1);
                } else {
                    restore.source_index = restore.source_index.saturating_sub(1);
                }
            }
        }
        if !keep_redo {
            self.redo.clear();
        }
    }

    fn restore_presentation(&mut self, redo: bool) {
        if self.pending > 0 || self.restoring.is_some() {
            return;
        }
        let Some(edit) = (if redo { &self.redo } else { &self.undo }).last().cloned() else {
            return;
        };
        // Acknowledged saves can still be waiting for their host pushes.
        for (_, _, keep_redo) in &mut self.staged_display {
            *keep_redo = true;
        }
        match edit {
            PresentationEdit::List(options) => {
                let inverse = PresentationEdit::List(std::mem::replace(&mut self.options, options));
                (if redo { &mut self.redo } else { &mut self.undo }).pop();
                (if redo { &mut self.undo } else { &mut self.redo }).push(inverse);
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
                    (if redo { &mut self.redo } else { &mut self.undo }).pop();
                    (if redo { &mut self.undo } else { &mut self.redo })
                        .push(PresentationEdit::Marker(id, line, blink));
                    return;
                }
                let inverse =
                    PresentationEdit::Marker(id, alert.extend_line, alert.blink_on_trigger);
                alert.extend_line = line;
                alert.blink_on_trigger = blink;
                if let Some(key) = self.save_with(alert, 0) {
                    let from = if redo { &mut self.redo } else { &mut self.undo };
                    from.pop();
                    self.restoring = Some(PresentationRestore {
                        redo,
                        request: key,
                        source_index: from.len(),
                        destination_index: if redo {
                            self.undo.len()
                        } else {
                            self.redo.len()
                        },
                        inverse,
                    });
                }
            }
        }
    }

    fn cancel_restore(&mut self) {
        let Some(PresentationRestore {
            redo,
            request: key,
            source_index,
            ..
        }) = self.restoring.take()
        else {
            return;
        };
        if let Some((_, edit, _)) = self
            .staged_display
            .iter()
            .find(|(request, ..)| *request == key)
        {
            let from = if redo { &mut self.redo } else { &mut self.undo };
            from.insert(source_index.min(from.len()), edit.clone());
        }
    }

    /// Start the Quick alert form on `symbol`, such as the chart's. Call it every frame: the form
    /// follows a change of `symbol` and keeps what the user typed otherwise.
    pub fn set_symbol(&mut self, symbol: &str) {
        if self.symbol != symbol {
            self.symbol = symbol.to_owned();
            self.draft.symbol = symbol.to_owned();
            self.last_price = None;
            self.seed_draft_price();
        }
    }

    /// Supply the chart symbol's current price and optional tick size for Quick alert. Call
    /// every frame, including while hidden, so a saved form starts again at the latest price.
    /// A user-entered level stays until a successful save. Without a finite price, an unedited
    /// level is empty and cannot be saved. Invalid tick sizes use the price's display decimals.
    pub fn set_price(&mut self, symbol: &str, price: Option<f64>, tick_size: Option<f64>) {
        self.set_symbol(symbol);
        self.last_price = price
            .map(|price| {
                crate::scales::snap_price(price, tick_size, crate::scales::price_decimals(price))
            })
            .filter(|price| price.is_finite());
        self.seed_draft_price();
    }

    fn seed_draft_price(&mut self) {
        if !self.draft_level_edited && self.draft.source == AlertSource::Price {
            let price = self.last_price.filter(|_| self.draft.symbol == self.symbol);
            match &mut self.draft.condition {
                AlertCondition::Crossing(level)
                | AlertCondition::CrossingUp(level)
                | AlertCondition::CrossingDown(level)
                | AlertCondition::Above(level)
                | AlertCondition::Below(level) => {
                    *level = price.unwrap_or(f64::NAN);
                    // Keep two decimals without rounding away a finer suggested price.
                    self.draft_level_text = price
                        .map(|price| {
                            let text = price.to_string();
                            if text
                                .split_once('.')
                                .is_none_or(|(_, fraction)| fraction.len() < 2)
                            {
                                format!("{price:.2}")
                            } else {
                                text
                            }
                        })
                        .unwrap_or_default();
                }
                _ => {}
            }
        }
    }

    /// Offer these study outputs and drawings, as stable identifiers and names, which are
    /// translated when shown, to the inline alert form.
    pub fn set_sources(&mut self, sources: Vec<(AlertSource, AlertSourceName)>) {
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

    /// Keep each trigger notification on screen for this many seconds, 20 by default, or until
    /// the user dismisses it when zero. The stack keeps the most recent 10,000 toasts, as many as
    /// the [`log`](Self::log) keeps triggers.
    pub fn set_notification_seconds(&mut self, seconds: f32) {
        self.notification_seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            20.0
        };
    }

    /// Write log times, in the Log tab and the CSV export, in `timezone`, such as the chart's;
    /// UTC until set.
    pub fn set_timezone(&mut self, timezone: crate::model::TimezoneId) {
        self.timezone = timezone;
    }

    /// Also show each trigger popup as a desktop notification through
    /// [`Notifications::deliver`], or only in the app when `None`, the default. A refused or
    /// failed delivery shows an error toast that stays until the user dismisses it; deliveries
    /// that fail while it shows add none.
    pub fn set_desktop(&mut self, service: Option<Arc<dyn Notifications>>) {
        self.desktop = service;
    }

    /// The sound to play when an alert whose delivery keeps Sound fires: its name as the user
    /// chose it in the alert settings, empty for the host's first sound, or `None` to play none.
    pub fn set_sound(&mut self, sound: Option<String>) {
        self.sound_choice = sound;
    }

    /// The sound to play for the triggers the last [`poll`](Self::poll) received, if it received
    /// one and [`set_sound`](Self::set_sound) holds a sound. The host plays it, as with
    /// `Trading::take_execution_sound`; call this after `poll`.
    pub fn take_sound(&mut self) -> Option<String> {
        self.sound.take()
    }

    /// Offer Open chart and Edit alert on trigger popups, returned by
    /// [`take_popup_action`](Self::take_popup_action). Off by default, so a host that does not
    /// read them shows no buttons.
    pub fn set_popup_actions(&mut self, offered: bool) {
        self.popup_actions = offered;
    }

    /// The action the user chose on a trigger popup since the last call:
    /// [`AlertPanelAction::SelectSymbol`] with the symbol that fired for Open chart, or
    /// [`AlertPanelAction::EditAlert`] with the alert as the host last pushed it for Edit alert;
    /// [`None`](AlertPanelAction::None) otherwise, also when the alert was removed meanwhile.
    /// Call it after [`poll`](Self::poll), which also runs while the panel is hidden.
    pub fn take_popup_action(&mut self) -> AlertPanelAction {
        std::mem::take(&mut self.popup_action)
    }

    /// Attach a host service, clearing the previous account's alerts and log.
    pub fn set_service(&mut self, service: Arc<dyn Alerts>) {
        self.sound = None;
        self.listener = None;
        self.service = Some(service);
        self.alerts.clear();
        self.log_offset += self.log.len();
        self.log.clear();
        self.selected = None;
        self.log_selected = None;
        self.removing = None;
        self.log_alert = None;
        self.log_symbol = None;
        self.clearing = false;
        self.popups.clear();
        self.popup_action = AlertPanelAction::None;
        self.marks.clear();
        self.undo.clear();
        self.redo.clear();
        self.restoring = None;
        self.staged_display.clear();
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
        let _ = self.save_with(alert, 0);
    }

    /// Send `alert` and return its reply key, or `None` without a service.
    fn save_with(&mut self, mut alert: Alert, key: u64) -> Option<u64> {
        if alert.id.is_none() {
            alert.created_at = Some(Utc::now());
            alert.last_triggered_at = None;
        }
        let Some(service) = &self.service else {
            self.error = Some(FeedError::NotSupported);
            return None;
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
        // A reply belongs to its own save even when the host answers out of order.
        let key = if key == 0 {
            let key = self.next_reply;
            self.next_reply -= 1;
            key
        } else {
            key
        };
        if self.alerts.iter().any(|old| {
            old.id == alert.id
                && (old.extend_line != alert.extend_line
                    || old.blink_on_trigger != alert.blink_on_trigger)
        }) && let Some(id) = alert.id.clone()
        {
            // A later confirmation must not erase the redo produced by the pending restore.
            self.staged_display.push((
                key,
                PresentationEdit::Marker(id, alert.extend_line, alert.blink_on_trigger),
                self.restoring.is_some(),
            ));
        }
        self.pending += 1;
        service.save(alert, self.inbox.reply(key));
        Some(key)
    }

    fn last_firing(&self, alert: &Alert) -> Option<DateTime<Utc>> {
        alert
            .last_triggered_at
            .into_iter()
            .chain(
                self.log
                    .iter()
                    .filter(|trigger| !trigger.silent && Some(&trigger.id) == alert.id.as_ref())
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
        if !self.popups.is_empty() {
            for (toast, id, symbol) in &self.popups {
                let Some(index) = toasts.take_pressed(*toast) else {
                    continue;
                };
                // Open chart comes first when the popup offers it.
                let alert = self.alerts.iter().find(|a| a.id.as_ref() == Some(id));
                self.popup_action = match (index, symbol, alert) {
                    (0, Some(symbol), _) => AlertPanelAction::SelectSymbol(symbol.clone()),
                    (_, _, Some(alert)) => AlertPanelAction::EditAlert(Box::new(alert.clone())),
                    _ => AlertPanelAction::None,
                };
            }
            let live: std::collections::HashSet<u64> =
                toasts.toasts().iter().map(|t| t.id).collect();
            self.popups.retain(|(toast, ..)| live.contains(toast));
        }
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.listener = None;
            self.alerts.clear();
            self.cancel_restore();
            self.staged_display.clear();
            self.watch();
        }
        let repaint = ctx.clone();
        self.desktop_inbox
            .set_waker(move || repaint.request_repaint());
        for (_, answer) in self.desktop_inbox.drain().answers {
            let Ok(result) = answer.downcast::<Result<(), FeedError>>() else {
                continue;
            };
            // One failure toast on screen at a time, however many triggers fail meanwhile. It
            // stays until dismissed: a lifetime would bring it back with every later trigger.
            if let Err(error) = *result
                && !self
                    .desktop_toast
                    .is_some_and(|id| toasts.toasts().iter().any(|t| t.id == id))
            {
                let toast = Toast::error(l10n::format_text(
                    ctx,
                    "Desktop notification failed: {error}",
                    &[("error", error.to_string().as_str())],
                ))
                .permanent();
                self.desktop_toast = Some(toast.id);
                toasts.add(toast);
            }
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                if let Ok(event) = answer.downcast::<AlertEvent>() {
                    self.apply_event(ctx, *event, toasts);
                }
            } else if let Ok(result) = answer.downcast::<Result<(), FeedError>>() {
                self.pending = self.pending.saturating_sub(1);
                self.draft_saving &= key != DRAFT_REPLY;
                match *result {
                    Ok(()) => {
                        self.error = None;
                        if key == DRAFT_REPLY {
                            self.draft =
                                Alert::new(self.symbol.clone(), AlertCondition::Crossing(f64::NAN));
                            self.draft_level_edited = false;
                            self.seed_draft_price();
                        }
                    }
                    Err(error) => {
                        if self
                            .restoring
                            .as_ref()
                            .is_some_and(|restore| restore.request == key)
                        {
                            self.cancel_restore();
                        }
                        self.staged_display.retain(|(request, ..)| *request != key);
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
                    if let Some(staged) = self.staged_display.iter().position(|(_, edit, _)| {
                        matches!(edit, PresentationEdit::Marker(id, line, blink)
                            if alert.id.as_ref() == Some(id)
                                && alert.extend_line == *line
                                && alert.blink_on_trigger == *blink)
                    }) {
                        let (key, _, keep_redo) = self.staged_display.remove(staged);
                        let inverse = PresentationEdit::Marker(
                            alert.id.clone().unwrap(),
                            old.extend_line,
                            old.blink_on_trigger,
                        );
                        if self
                            .restoring
                            .as_ref()
                            .is_some_and(|restore| restore.request == key)
                        {
                            let restore = self.restoring.take().unwrap();
                            let to = if restore.redo {
                                &mut self.undo
                            } else {
                                &mut self.redo
                            };
                            to.insert(restore.destination_index.min(to.len()), restore.inverse);
                        } else if old.extend_line != alert.extend_line
                            || old.blink_on_trigger != alert.blink_on_trigger
                        {
                            self.remember(inverse, keep_redo);
                        }
                    }
                    self.alerts[index] = alert;
                } else {
                    self.alerts.push(alert);
                }
            }
            AlertEvent::Remove(id) => {
                self.alerts.retain(|a| a.id.as_ref() != Some(&id));
                if let Some(restore) = &self.restoring
                    && self.staged_display.iter().any(|(request, edit, _)| {
                        *request == restore.request
                            && matches!(edit, PresentationEdit::Marker(kept, ..) if *kept == id)
                    })
                {
                    self.restoring = None;
                }
                self.staged_display.retain(|(_, edit, _)|
                    !matches!(edit, PresentationEdit::Marker(kept, ..) if *kept == id));
                if let Some(restore) = &mut self.restoring {
                    let (from, to) = if restore.redo {
                        (&self.redo, &self.undo)
                    } else {
                        (&self.undo, &self.redo)
                    };
                    let retained = |history: &[PresentationEdit], index| {
                        history.iter().take(index).filter(
                        |edit| !matches!(edit, PresentationEdit::Marker(kept, ..) if *kept == id)
                    ).count()
                    };
                    restore.source_index = retained(from, restore.source_index);
                    restore.destination_index = retained(to, restore.destination_index);
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
                self.staged_display.clear();
            }
            AlertEvent::TriggeredForSymbol {
                id,
                symbol,
                time,
                price,
                message,
            } => {
                let name = self.name_of(&id);
                self.record_trigger(
                    ctx,
                    AlertTrigger {
                        id,
                        name,
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
                let name = self.name_of(&id);
                self.log_trigger(AlertTrigger {
                    id,
                    name,
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
                let name = self.name_of(&id);
                self.record_trigger(
                    ctx,
                    AlertTrigger {
                        id,
                        name,
                        symbol,
                        time,
                        price,
                        message: String::new(),
                        silent: false,
                    },
                    toasts,
                );
            }
            AlertEvent::Logged(mut trigger) => {
                trigger.price = trigger.price.filter(|p| p.is_finite());
                if trigger.name.is_empty() {
                    trigger.name = self.name_of(&trigger.id);
                }
                self.log_trigger(trigger);
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
            name,
            symbol,
            message,
            ..
        } = &trigger;
        let alert = if name.is_empty() { &id.0 } else { name };
        let title = if symbol.is_empty() {
            alert.clone()
        } else {
            l10n::format_text(
                ctx,
                "{alert} on {symbol}",
                &[("alert", alert), ("symbol", symbol)],
            )
        };
        let body = if message.is_empty() {
            l10n::text(ctx, "Alert triggered")
        } else {
            message.clone()
        };
        // A firing of an alert removed meanwhile keeps the default delivery.
        let delivery = self
            .alerts
            .iter()
            .find(|a| a.id.as_ref() == Some(id))
            .map(|a| &a.delivery);
        if delivery.is_none_or(|d| d.popup) {
            if let Some(desktop) = &self.desktop {
                let mut notification = Notification::new(trigger.time, title.clone());
                notification.body.clone_from(&body);
                notification.symbol = (!symbol.is_empty()).then(|| symbol.clone());
                desktop.deliver(notification, self.desktop_inbox.reply(0));
            }
            // Make room by closing the oldest toasts that would close by themselves; the others
            // stay until the user dismisses them, as many as the log keeps.
            let mut extra = toasts.len().saturating_sub(4);
            toasts.toasts_mut().retain(|toast| {
                let drop = extra > 0 && toast.duration > 0.0;
                extra -= usize::from(drop);
                !drop
            });
            let extra = toasts.len().saturating_sub(LOG_BOUND - 1);
            toasts.toasts_mut().drain(..extra);
            let mut toast = Toast::info(body)
                .with_title(title)
                .duration(self.notification_seconds);
            if self.popup_actions {
                let open = (!symbol.is_empty()).then(|| symbol.clone());
                if open.is_some() {
                    toast = toast.with_action("Open chart");
                }
                // Editing needs the alert as the host pushed it.
                if delivery.is_some() {
                    toast = toast.with_action("Edit alert");
                }
                if !toast.actions.is_empty() {
                    self.popups.push((toast.id, id.clone(), open));
                }
            }
            toasts.add(toast);
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

    /// The display name of the alert `id` as last pushed, empty when the host has not pushed it.
    fn name_of(&self, id: &AlertId) -> String {
        self.alerts
            .iter()
            .find(|a| a.id.as_ref() == Some(id))
            .map(Alert::display_name)
            .unwrap_or_default()
    }

    fn log_trigger(&mut self, trigger: AlertTrigger) {
        self.log.push_back(trigger);
        if self.log.len() > LOG_BOUND {
            self.log.pop_front();
            self.log_offset += 1;
            self.log_selected = self.log_selected.and_then(|index| index.checked_sub(1));
        }
    }

    /// Apply Pause or Enable, opening an alert past its expiry in the editor first.
    fn toggled(alert: &Alert) -> (Alert, bool) {
        let active = alert.status == AlertStatus::Active;
        let mut changed = alert.clone();
        changed.status = if active {
            AlertStatus::Disabled
        } else {
            AlertStatus::Active
        };
        // Enabled as it is, an alert past its expiry would expire again at the next tick.
        let edit = !active && alert.expires.is_some_and(|at| at <= Utc::now());
        (changed, edit)
    }

    /// The fresh, active copy Duplicate stages in the shared dialog.
    fn duplicate_of(alert: &Alert) -> Alert {
        let mut copy = alert.clone();
        copy.id = None;
        copy.created_at = None;
        copy.last_triggered_at = None;
        copy.status = AlertStatus::Active;
        // The copy has seen no entry of its own.
        if let AlertCondition::Position { progress, .. } = &mut copy.condition {
            *progress = AlertPositionProgress::AwaitingEntry;
        }
        copy
    }

    /// Export the retained trigger log, times in the panel's time zone. Text fields are quoted and
    /// embedded quotes doubled.
    pub fn log_csv(&self) -> String {
        self.csv(self.log.iter())
    }

    fn csv<'a>(&self, triggers: impl Iterator<Item = &'a AlertTrigger>) -> String {
        let mut csv = String::from("id,name,symbol,time,price,message,silent\r\n");
        for trigger in triggers {
            let id = trigger.id.0.replace('"', "\"\"");
            let name = trigger.name.replace('"', "\"\"");
            let symbol = trigger.symbol.replace('"', "\"\"");
            let price = trigger.price.map(|p| p.to_string()).unwrap_or_default();
            let message = trigger.message.replace('"', "\"\"");
            csv.push_str(&format!(
                "\"{id}\",\"{name}\",\"{symbol}\",{},{},\"{message}\",{}\r\n",
                trigger.time.with_timezone(&self.timezone).to_rfc3339(),
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

    /// The boxes that narrow the Log tab to one alert, named by its newest entry, or one symbol.
    fn log_filters(&mut self, ui: &mut Ui) {
        let label = |trigger: &AlertTrigger| {
            if trigger.name.is_empty() {
                trigger.id.0.clone()
            } else {
                trigger.name.clone()
            }
        };
        ui.horizontal_wrapped(|ui| {
            let all = l10n::text(ui.ctx(), "All alerts");
            let selected = match &self.log_alert {
                Some(id) => {
                    let newest = self.log.iter().rev().find(|t| t.id == *id);
                    newest.map_or_else(|| id.0.clone(), label)
                }
                None => all.clone(),
            };
            egui::ComboBox::from_id_salt("alert_log_alert")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.log_alert, None, all);
                    let mut seen = std::collections::HashSet::new();
                    let mut alerts: Vec<_> = self
                        .log
                        .iter()
                        .rev()
                        .filter(|t| seen.insert(&t.id))
                        .map(|t| (label(t), t.id.clone()))
                        .collect();
                    alerts.sort_by_cached_key(|(text, id)| (text.to_lowercase(), id.0.clone()));
                    for (text, id) in alerts {
                        ui.selectable_value(&mut self.log_alert, Some(id), text);
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Filter by alert"));
            let all = l10n::text(ui.ctx(), "All symbols");
            egui::ComboBox::from_id_salt("alert_log_symbol")
                .selected_text(self.log_symbol.clone().unwrap_or_else(|| all.clone()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.log_symbol, None, all);
                    let mut symbols: Vec<_> = self
                        .log
                        .iter()
                        .map(|t| t.symbol.as_str())
                        .filter(|symbol| !symbol.is_empty())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    symbols.sort_by_cached_key(|symbol| symbol.to_lowercase());
                    for symbol in symbols {
                        ui.selectable_value(&mut self.log_symbol, Some(symbol.to_owned()), symbol);
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Filter by symbol"));
        });
    }

    fn list_controls(&mut self, ui: &mut Ui, log: bool) {
        let before = self.options.clone();
        let mut history_action = None;
        ui.horizontal_wrapped(|ui| {
            // Keep list preferences steady until the host confirms the marker restore.
            if self.restoring.is_some() {
                ui.disable();
            }
            if !log {
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
            }
            ui.menu_button(l10n::text(ui.ctx(), "Fields"), |ui| {
                if log {
                    for (value, label) in [
                        (&mut self.options.log_symbol, "Symbol"),
                        (&mut self.options.log_price, "Price"),
                        (&mut self.options.log_message, "Message"),
                    ] {
                        ui.checkbox(value, l10n::text(ui.ctx(), label));
                    }
                } else {
                    for (value, label) in [
                        (&mut self.options.name, "Name"),
                        (&mut self.options.message, "Message"),
                        (&mut self.options.source_icon, "Source symbol"),
                        (&mut self.options.symbol_icon, "Instrument image"),
                        (&mut self.options.last_firing, "Last firing"),
                    ] {
                        ui.checkbox(value, l10n::text(ui.ctx(), label));
                    }
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
            self.remember(PresentationEdit::List(before), false);
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
                self.seed_draft_price();
                self.draft_level_edited |= rule_form(
                    ui,
                    &mut self.draft.source,
                    &mut self.draft.condition,
                    &self.sources,
                    &[],
                    true,
                    false,
                    Some(&mut self.draft_level_text),
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
                    self.draft_saving = self.save_with(self.draft.clone(), DRAFT_REPLY).is_some();
                }
            });
        if self.pending > 0 {
            ui.label(l10n::text(ui.ctx(), "Waiting for the alerts service…"));
        }
    }

    /// Render the host's alerts or trigger log after [`poll`](Self::poll), returning host intents.
    /// Clicking a row selects and focuses it; arrows and Space move the selection. On alert rows,
    /// Enter or a double click returns [`EditAlert`](AlertPanelAction::EditAlert), Delete asks
    /// before removal, and a secondary click or long press offers the row's actions.
    pub fn show(&mut self, ui: &mut Ui) -> AlertPanelAction {
        let mut action = AlertPanelAction::None;
        let arrows = EventFilter {
            vertical_arrows: true,
            ..Default::default()
        };
        let theme = ui.active_theme();
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
            self.log_filters(ui);
            self.list_controls(ui, true);
            let (alert, symbol) = (self.log_alert.clone(), self.log_symbol.clone());
            let shown = move |trigger: &AlertTrigger| {
                alert.as_ref().is_none_or(|id| *id == trigger.id)
                    && symbol
                        .as_ref()
                        .is_none_or(|symbol| *symbol == trigger.symbol)
            };
            let any = self.log.iter().any(&shown);
            ui.horizontal(|ui| {
                if Button::new("Export CSV…").enabled(any).show(ui).clicked() {
                    action =
                        AlertPanelAction::ExportLog(self.csv(self.log.iter().filter(|t| shown(t))));
                }
                if Button::new("Clear log…").enabled(any).show(ui).clicked() {
                    self.clearing = true;
                }
            });
            let rows = ui.scope_id().with("alert_log_rows");
            let order: Vec<_> = self
                .log
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, trigger)| shown(trigger))
                .map(|(index, _)| index)
                .collect();
            let mut reveal = false;
            if let Some((focused, index)) = ui.memory(|m| {
                let focused = m.focused()?;
                order
                    .iter()
                    .find(|&&index| rows.with(self.log_offset + index) == focused)
                    .map(|index| (focused, *index))
            }) {
                self.log_selected = Some(index);
                ui.memory_mut(|m| m.set_focus_lock_filter(focused, arrows));
                let step = ui.input_mut(|i| {
                    let down = i.consume_key(Modifiers::NONE, Key::ArrowDown)
                        | i.consume_key(Modifiers::NONE, Key::Space);
                    down as isize - i.consume_key(Modifiers::NONE, Key::ArrowUp) as isize
                });
                if step != 0 && !order.is_empty() {
                    let next = self
                        .log_selected
                        .and_then(|selected| order.iter().position(|i| *i == selected))
                        .map_or(0, |at| at.saturating_add_signed(step).min(order.len() - 1));
                    self.log_selected = Some(order[next]);
                    ui.memory_mut(|m| {
                        m.request_focus(rows.with(self.log_offset + order[next]));
                        m.set_focus_lock_filter(rows.with(self.log_offset + order[next]), arrows);
                    });
                    reveal = true;
                }
            }
            let mut select = None;
            egui::ScrollArea::vertical()
                .max_height(self.config.max_height)
                .show(ui, |ui| {
                    for &index in &order {
                        let trigger = &self.log[index];
                        let mut parts = vec![if trigger.name.is_empty() {
                            trigger.id.0.clone()
                        } else {
                            trigger.name.clone()
                        }];
                        if self.options.log_symbol && !trigger.symbol.is_empty() {
                            parts.push(trigger.symbol.clone());
                        }
                        let time = trigger.time.with_timezone(&self.timezone);
                        parts.push(time.format("%Y-%m-%d %H:%M:%S %:z").to_string());
                        if trigger.silent {
                            parts.push(l10n::text(ui.ctx(), "Silent"));
                        }
                        let label = parts.join(" · ");
                        let selected = self.log_selected == Some(index);
                        let row = ui.scope_builder(
                            egui::UiBuilder::new()
                                .scope_id(rows.with(self.log_offset + index))
                                .sense(Sense::click()),
                            |ui| {
                                ui.style_mut().interaction.selectable_labels = false;
                                let hovered = ui.response().hovered();
                                egui::Frame::new()
                                    .fill(if selected {
                                        theme.ui.btn_bg_active
                                    } else if hovered {
                                        theme.ui.btn_bg_hover
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        let heading = ui.label(&label);
                                        if self.options.log_price
                                            && let Some(price) = trigger.price
                                        {
                                            ui.label(l10n::text(ui.ctx(), price.to_string()));
                                        }
                                        if self.options.log_message && !trigger.message.is_empty() {
                                            ui.label(&trigger.message);
                                        }
                                        ui.separator();
                                        heading.id
                                    })
                                    .inner
                            },
                        );
                        let response = row.response.labelled_by(row.inner);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::ListBoxOption, true, &label)
                        });
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_selected(selected)
                        });
                        if selected && reveal {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            select = Some(index);
                            ui.memory_mut(|m| {
                                m.request_focus(response.id);
                                m.set_focus_lock_filter(response.id, arrows);
                            });
                        }
                    }
                });
            if let Some(index) = select {
                self.log_selected = Some(index);
            }
            if !any {
                EmptyState::new("No triggers yet")
                    .icon(&icons::ALERTS)
                    .fill_height()
                    .show(ui);
            }
            if self.clearing
                && ConfirmDialog::new(
                    "alert_log_clear",
                    "Clear log",
                    "Remove the log entries shown? This cannot be undone.",
                )
                .confirm_text("Clear")
                .danger()
                .show(ui.ctx(), &mut self.clearing)
                .is_confirmed()
            {
                self.log_offset += self.log.len();
                self.log.retain(|trigger| !shown(trigger));
                self.log_selected = None;
                // Otherwise the engine would restore the entries after a restart.
                #[cfg(feature = "alert-engine")]
                if let Some(engine) = self.service.as_ref().and_then(|service| service.engine()) {
                    engine.retain_triggers(|trigger| !shown(trigger));
                }
                self.log_alert = None;
                self.log_symbol = None;
            }
        } else if self.alerts.is_empty() {
            self.draft_form(ui);
            self.list_controls(ui, false);
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
            self.list_controls(ui, false);
            let mut remove = None;
            let mut toggle = None;
            let mut duplicate = None;
            let mut presentation = None;
            let rows = ui.scope_id().with("alert_rows");
            if self.restore_row_focus {
                // A closing modal blocks the rows for one more frame.
                if ui.memory(|m| m.allows_interaction(ui.layer_id())) {
                    if let Some(id) = &self.selected {
                        ui.memory_mut(|m| m.request_focus(rows.with(&id.0)));
                    }
                    self.restore_row_focus = false;
                } else {
                    ui.ctx().request_repaint();
                }
            }
            let order: Vec<_> = self
                .sorted()
                .iter()
                .filter_map(|alert| alert.id.clone())
                .collect();
            let mut reveal = false;
            if let Some((focused, id)) = ui.memory(|m| {
                let focused = m.focused()?;
                order
                    .iter()
                    .find(|id| rows.with(&id.0) == focused)
                    .map(|id| (focused, id.clone()))
            }) {
                self.selected = Some(id);
                ui.memory_mut(|m| m.set_focus_lock_filter(focused, arrows));
                let (step, edit, delete) = ui.input_mut(|i| {
                    let down = i.consume_key(Modifiers::NONE, Key::ArrowDown)
                        | i.consume_key(Modifiers::NONE, Key::Space);
                    (
                        down as isize - i.consume_key(Modifiers::NONE, Key::ArrowUp) as isize,
                        i.consume_key(Modifiers::NONE, Key::Enter),
                        i.consume_key(Modifiers::NONE, Key::Delete),
                    )
                });
                if step != 0 && !order.is_empty() {
                    let next = self
                        .selected
                        .as_ref()
                        .and_then(|selected| order.iter().position(|id| id == selected))
                        .map_or(0, |at| at.saturating_add_signed(step).min(order.len() - 1));
                    self.selected = Some(order[next].clone());
                    ui.memory_mut(|m| {
                        m.request_focus(rows.with(&order[next].0));
                        m.set_focus_lock_filter(rows.with(&order[next].0), arrows);
                    });
                    reveal = true;
                }
                if let Some(alert) = self.alerts.iter().find(|alert| alert.id == self.selected) {
                    if edit {
                        action = AlertPanelAction::EditAlert(Box::new(alert.clone()));
                    }
                    if delete {
                        self.removing = alert.id.clone();
                    }
                }
            }
            let mut select = None;
            egui::ScrollArea::vertical()
                .max_height(self.config.max_height)
                .show(ui, |ui| {
                    for alert in self.sorted() {
                        let Some(id) = &alert.id else { continue };
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
                        let selected = self.selected.as_ref() == Some(id);
                        let response = ui
                            .scope_builder(
                                egui::UiBuilder::new()
                                    .scope_id(rows.with(&id.0))
                                    .sense(Sense::click()),
                                |ui| {
                                    ui.style_mut().interaction.selectable_labels = false;
                                    let hovered = ui.response().hovered();
                                    egui::Frame::new()
                                        .fill(if selected {
                                            theme.ui.btn_bg_active
                                        } else if hovered {
                                            theme.ui.btn_bg_hover
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
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
                                                        AlertSource::Volume => {
                                                            &icons::CHART_COLUMNS
                                                        }
                                                        AlertSource::Study { .. }
                                                        | AlertSource::StudyCondition { .. } => {
                                                            &icons::INDICATORS
                                                        }
                                                        AlertSource::Drawing(_) => {
                                                            &icons::TREND_LINE
                                                        }
                                                        AlertSource::TPOLevel(_) => {
                                                            &icons::CHART_TPO
                                                        }
                                                    };
                                                    ui.add(icon.as_image_tinted(
                                                        egui::Vec2::splat(16.0),
                                                        ui.visuals().text_color(),
                                                    ))
                                                    .on_hover_text(l10n::text(
                                                        ui.ctx(),
                                                        "Alert source",
                                                    ));
                                                }
                                                ui.label(RichText::new(&subject).strong());
                                            });
                                            ui.label(l10n::text(
                                                ui.ctx(),
                                                alert.condition.to_string(),
                                            ));
                                            if !alert.additional_conditions.is_empty() {
                                                ui.label(l10n::format_text(
                                                    ui.ctx(),
                                                    "{0} additional conditions",
                                                    &[(
                                                        "0",
                                                        format!(
                                                            "{}",
                                                            alert.additional_conditions.len()
                                                        ),
                                                    )],
                                                ));
                                            }
                                            if self.options.name && !alert.name.is_empty() {
                                                ui.label(&alert.name);
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
                                                    ui.add(
                                                        icons::WIDGET_BAR_CALENDAR.as_image_tinted(
                                                            egui::Vec2::splat(16.0),
                                                            ui.visuals().text_color(),
                                                        ),
                                                    )
                                                    .on_hover_ui(|ui| {
                                                        schedule_tooltip(ui, schedule)
                                                    });
                                                }
                                            });
                                            if self.options.message && !alert.message.is_empty() {
                                                ui.label(&alert.message);
                                            }
                                            if self.options.last_firing
                                                && let Some(time) = alert.last_triggered_at
                                            {
                                                let zone = alert
                                                    .schedule
                                                    .as_ref()
                                                    .map(|s| &s.timezone)
                                                    .or_else(|| {
                                                        alert
                                                            .inputs
                                                            .as_ref()
                                                            .and_then(|i| i.timezone.as_ref())
                                                    });
                                                let zone = zone
                                                    .cloned()
                                                    .unwrap_or_else(crate::model::TimezoneId::utc);
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
                                                    action = AlertPanelAction::EditAlert(Box::new(
                                                        alert.clone(),
                                                    ));
                                                }
                                                let active = alert.status == AlertStatus::Active;
                                                if Button::new(if active {
                                                    "Pause"
                                                } else {
                                                    "Enable"
                                                })
                                                .show(ui)
                                                .clicked()
                                                {
                                                    let (changed, edit) = Self::toggled(alert);
                                                    if edit {
                                                        action = AlertPanelAction::EditAlert(
                                                            Box::new(changed),
                                                        );
                                                    } else {
                                                        toggle = Some(changed);
                                                    }
                                                }
                                                ui.add_enabled_ui(self.pending == 0, |ui| {
                                                    ui.menu_button("...", |ui| {
                                                        let mut updated = alert.clone();
                                                        let line = ui
                                                            .checkbox(
                                                                &mut updated.extend_line,
                                                                l10n::text(
                                                                    ui.ctx(),
                                                                    "Line across chart",
                                                                ),
                                                            )
                                                            .changed();
                                                        let blink = ui
                                                            .checkbox(
                                                                &mut updated.blink_on_trigger,
                                                                l10n::text(
                                                                    ui.ctx(),
                                                                    "Blink after firing",
                                                                ),
                                                            )
                                                            .changed();
                                                        if line || blink {
                                                            presentation = Some(updated);
                                                        }
                                                    })
                                                    .response
                                                    .accessible_name(l10n::text(ui.ctx(), "Marker"))
                                                    .on_hover_text(l10n::text(
                                                        ui.ctx(),
                                                        "Marker options",
                                                    ));
                                                });
                                                if Button::new("Duplicate").show(ui).clicked() {
                                                    duplicate = Some(Self::duplicate_of(alert));
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
                                },
                            )
                            .response;
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::ListBoxOption,
                                true,
                                format!("{subject}: {}", alert.display_name()),
                            )
                        });
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_selected(selected)
                        });
                        if selected && reveal {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            select = Some(id.clone());
                            ui.memory_mut(|m| {
                                m.request_focus(response.id);
                                m.set_focus_lock_filter(response.id, arrows);
                            });
                        }
                        if response.double_clicked() {
                            action = AlertPanelAction::EditAlert(Box::new(alert.clone()));
                        }
                        response.context_menu(|ui| {
                            if ui.button(l10n::text(ui.ctx(), "Edit")).clicked() {
                                action = AlertPanelAction::EditAlert(Box::new(alert.clone()));
                                ui.close();
                            }
                            let active = alert.status == AlertStatus::Active;
                            if ui
                                .button(l10n::text(
                                    ui.ctx(),
                                    if active { "Pause" } else { "Enable" },
                                ))
                                .clicked()
                            {
                                let (changed, edit) = Self::toggled(alert);
                                if edit {
                                    action = AlertPanelAction::EditAlert(Box::new(changed));
                                } else {
                                    toggle = Some(changed);
                                }
                                ui.close();
                            }
                            if ui.button(l10n::text(ui.ctx(), "Duplicate")).clicked() {
                                duplicate = Some(Self::duplicate_of(alert));
                                ui.close();
                            }
                            if ui.button(l10n::text(ui.ctx(), "Remove")).clicked() {
                                remove = Some(id.clone());
                                ui.close();
                            }
                        });
                        response.on_hover_ui(|ui| {
                            if let Some(schedule) = &alert.schedule {
                                schedule_tooltip(ui, schedule);
                            }
                        });
                    }
                });
            if let Some(id) = select {
                self.selected = Some(id);
            }
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
        if self.removing.is_some() {
            let mut open = true;
            if ConfirmDialog::new(
                "alert_remove",
                "Remove alert",
                "Remove the selected alert? This cannot be undone.",
            )
            .confirm_text("Remove")
            .danger()
            .show(ui.ctx(), &mut open)
            .is_confirmed()
                && let Some(id) = self.removing.take()
                && let Some(service) = &self.service
            {
                self.pending += 1;
                service.remove(id, self.inbox.reply(0));
            }
            if !open && self.removing.take().is_some() {
                self.restore_row_focus = true;
                ui.ctx().request_repaint();
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
    use crate::ui_kit::notifications::ToastKind;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host {
        events: Mutex<Option<Events<AlertEvent>>>,
        reply: Mutex<Option<Reply<()>>>,
        saved: Mutex<Vec<Alert>>,
    }

    impl Alerts for Host {
        fn watch(&self, events: Events<AlertEvent>) {
            *self.events.lock().unwrap() = Some(events);
        }
        fn save(&self, alert: Alert, reply: Reply<()>) {
            self.saved.lock().unwrap().push(alert);
            *self.reply.lock().unwrap() = Some(reply);
        }
        fn remove(&self, _: AlertId, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    #[test]
    fn a_silent_log_entry_is_not_taken_for_a_firing() {
        let host = Arc::new(Host::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        let silent = "2026-11-02T03:00:00Z".parse().unwrap();
        let delivered = "2026-11-02T09:00:00Z".parse().unwrap();
        for previous in [None, Some("2026-11-01T09:00:00Z".parse().unwrap())] {
            let mut alert = Alert::new("AAPL", AlertCondition::Above(100.0));
            alert.id = Some(AlertId(format!("{previous:?}")));
            alert.last_triggered_at = previous;
            panel.apply_event(
                &ctx,
                AlertEvent::Upsert(Box::new(alert.clone())),
                &mut toasts,
            );
            panel.apply_event(
                &ctx,
                AlertEvent::SilentTriggered {
                    id: alert.id.clone().unwrap(),
                    symbol: alert.symbol.clone(),
                    time: silent,
                    price: Some(101.0),
                    message: String::new(),
                },
                &mut toasts,
            );
            alert.status = AlertStatus::Disabled;
            panel.apply_event(
                &ctx,
                AlertEvent::Upsert(Box::new(alert.clone())),
                &mut toasts,
            );
            let mut resumed = panel.alerts().last().unwrap().clone();
            assert_eq!(resumed.last_triggered_at, previous);
            resumed.status = AlertStatus::Active;
            panel.save(resumed);
            assert_eq!(
                host.saved.lock().unwrap().last().unwrap().last_triggered_at,
                previous
            );
            host.reply.lock().unwrap().take().unwrap().send(Ok(()));
            assert!(panel.log().back().unwrap().silent);
            assert!(panel.log_csv().ends_with(",101,\"\",true\r\n"));
            panel.apply_event(
                &ctx,
                AlertEvent::Triggered {
                    id: alert.id.clone().unwrap(),
                    time: delivered,
                    price: Some(102.0),
                },
                &mut toasts,
            );
            // A stale push keeps the delivered firing, and a newer host firing wins over it.
            for expected in [delivered, delivered + chrono::TimeDelta::hours(1)] {
                if expected != delivered {
                    alert.last_triggered_at = Some(expected);
                }
                panel.apply_event(
                    &ctx,
                    AlertEvent::Upsert(Box::new(alert.clone())),
                    &mut toasts,
                );
                assert_eq!(
                    panel.alerts().last().unwrap().last_triggered_at,
                    Some(expected)
                );
            }
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
        // Another alert on the symbol without a name or a message, removed once it has fired.
        let mut other = Alert::new("S\"IM", AlertCondition::CrossingDown(12.5));
        other.id = Some(AlertId("2".into()));
        panel.save(alert.clone());
        host.reply.lock().unwrap().take().unwrap().send(Ok(()));
        frame(&mut panel);
        assert!(panel.alerts().is_empty());
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        events.push(AlertEvent::Upsert(Box::new(other.clone())));
        for id in [&alert.id, &other.id] {
            events.push(AlertEvent::Triggered {
                id: id.clone().unwrap(),
                time: "2026-10-23T12:00:00Z".parse().unwrap(),
                price: Some(12.5),
            });
        }
        events.push(AlertEvent::Remove(other.id.unwrap()));
        frame(&mut panel);
        alert.last_triggered_at = Some("2026-10-23T12:00:00Z".parse().unwrap());
        assert_eq!(panel.alerts(), &[alert.clone()]);
        assert_eq!(panel.alerts()[0].status, AlertStatus::Active);
        assert_eq!(panel.log().len(), 2);
        // Each line names its alert, by its condition without a name, in the panel's time zone.
        let csv = "id,name,symbol,time,price,message,silent\r\n\"host,\"\"1\",\"Crossing up 12.5\",\"S\"\"IM\",{time},12.5,\"\",false\r\n\"2\",\"Crossing down 12.5\",\"S\"\"IM\",{time},12.5,\"\",false\r\n";
        assert_eq!(
            panel.log_csv(),
            csv.replace("{time}", "2026-10-23T12:00:00+00:00")
        );
        panel.set_timezone("-04:00".parse().unwrap());
        assert_eq!(
            panel.log_csv(),
            csv.replace("{time}", "2026-10-23T08:00:00-04:00")
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
        assert!(panel.staged_display.is_empty());
        events.push(AlertEvent::Remove(alert.id.clone().unwrap()));
        frame(&mut panel);
        assert!(panel.alerts().is_empty());
        assert_eq!(panel.log().len(), 2);
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
        assert_eq!(panel.log().len(), 2);
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
        // The alert was never pushed, so the popup names it by its id.
        let toast = toasts.toasts().last().unwrap();
        assert_eq!(toast.title.as_deref(), Some("watchlist on AAPL"));
        assert_eq!(toast.message, "AAPL closed at \"123\"");
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
    fn marker_change_during_a_pending_undo_stays_undoable() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (reverse_pushes, duplicate, edit_before_restore) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, false),
            (false, false, true),
            (true, false, true),
        ] {
            for pending_redo in [false, true] {
                for next_redo in [false, true] {
                    let host = Arc::new(Host::default());
                    let answer = || host.reply.lock().unwrap().take().unwrap().send(Ok(()));
                    let mut panel = AlertsPanel::default();
                    panel.set_service(host.clone());
                    let events = host.events.lock().unwrap().clone().unwrap();
                    let mut toasts = Toasts::new();
                    let mut h = Harness::builder()
                        .allow_missing_glyphs()
                        .with_size(egui::vec2(300.0, 900.0))
                        .with_context_setup(move |ctx| {
                            let theme = if reverse_pushes {
                                crate::theme::Theme::dark()
                            } else {
                                crate::theme::Theme::light()
                            };
                            crate::theme::apply_to_egui(ctx, &theme);
                        })
                        .build_ui_state(
                            move |ui, panel: &mut AlertsPanel| {
                                panel.poll(ui.ctx(), &mut toasts);
                                panel.show(ui);
                            },
                            panel,
                        );
                    let mut first = Alert::new("AAPL", AlertCondition::Above(100.0));
                    first.id = Some(AlertId("1".into()));
                    first.extend_line = true;
                    let mut second = first.clone();
                    second.id = Some(AlertId("2".into()));
                    events.push(AlertEvent::Upsert(Box::new(first.clone())));
                    events.push(AlertEvent::Upsert(Box::new(second.clone())));
                    h.run_steps(3);
                    first.extend_line = false;
                    h.state_mut().save(first.clone());
                    answer();
                    events.push(AlertEvent::Upsert(Box::new(first.clone())));
                    h.run_steps(3);
                    if edit_before_restore {
                        second.extend_line = false;
                        h.state_mut().save(second.clone());
                        answer();
                        h.run_steps(3);
                    }
                    h.get_by_label("Undo").click();
                    h.run_steps(3);
                    answer();
                    h.run_steps(3);
                    first.extend_line = true;
                    if pending_redo {
                        events.push(AlertEvent::Upsert(Box::new(first.clone())));
                        h.run_steps(3);
                        h.get_by_label("Redo").click();
                        h.run_steps(3);
                        answer();
                        h.run_steps(3);
                        first.extend_line = false;
                    }
                    // The host answered the restore but has not pushed it. A chart marker menu
                    // uses this public save path while the row menu is also available again.
                    second.extend_line = false;
                    if !edit_before_restore {
                        h.state_mut().save(second.clone());
                        answer();
                        h.run_steps(3);
                    }
                    if duplicate {
                        h.state_mut().save(second.clone());
                        answer();
                        h.run_steps(3);
                    }
                    for alert in if reverse_pushes {
                        [&second, &first]
                    } else {
                        [&first, &second]
                    } {
                        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                        h.run_steps(3);
                        if duplicate && alert.id == second.id {
                            events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                            h.run_steps(3);
                        }
                    }
                    let saved = host.saved.lock().unwrap().len();
                    h.get_by_label(if next_redo { "Redo" } else { "Undo" })
                        .click();
                    h.run_steps(3);
                    let saves = host.saved.lock().unwrap();
                    if pending_redo && next_redo {
                        assert_eq!(saves.len(), saved, "a completed redo leaves no redo entry");
                    } else {
                        assert_eq!(
                            saves.len(),
                            saved + 1,
                            "reverse_pushes={reverse_pushes}, duplicate={duplicate}, edit_before_restore={edit_before_restore}, pending_redo={pending_redo}, next_redo={next_redo}"
                        );
                        let sent = saves.last().unwrap();
                        let expected = if next_redo { ("1", false) } else { ("2", true) };
                        assert_eq!(
                            (sent.id.as_ref().unwrap().0.as_str(), sent.extend_line),
                            expected
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn marker_restore_and_another_save_keep_their_own_failures() {
        for reject_restore in [false, true] {
            for reverse_replies in [false, true] {
                let host = Arc::new(Host::default());
                let mut panel = AlertsPanel::default();
                panel.set_service(host.clone());
                let events = host.events.lock().unwrap().clone().unwrap();
                let ctx = egui::Context::default();
                let mut toasts = Toasts::new();
                let mut poll = |panel: &mut AlertsPanel| {
                    ctx.run_ui(Default::default(), |ui| panel.poll(ui.ctx(), &mut toasts))
                        .drop_without_applying_deltas();
                };
                let mut first = Alert::new("AAPL", AlertCondition::Above(100.0));
                first.id = Some(AlertId("1".into()));
                first.extend_line = true;
                let mut second = first.clone();
                second.id = Some(AlertId("2".into()));
                let mut older = first.clone();
                older.id = Some(AlertId("older".into()));
                events.push(AlertEvent::Upsert(Box::new(older.clone())));
                events.push(AlertEvent::Upsert(Box::new(first.clone())));
                events.push(AlertEvent::Upsert(Box::new(second.clone())));
                poll(&mut panel);
                older.extend_line = false;
                panel.save(older.clone());
                host.reply.lock().unwrap().take().unwrap().send(Ok(()));
                events.push(AlertEvent::Upsert(Box::new(older.clone())));
                poll(&mut panel);
                first.extend_line = false;
                panel.save(first.clone());
                host.reply.lock().unwrap().take().unwrap().send(Ok(()));
                events.push(AlertEvent::Upsert(Box::new(first.clone())));
                poll(&mut panel);
                panel.restore_presentation(false);
                let restore = host.reply.lock().unwrap().take().unwrap();
                second.extend_line = false;
                panel.save(second.clone());
                let edit = host.reply.lock().unwrap().take().unwrap();
                // A confirmed new edit stays newer than a failed restore even if an earlier
                // history entry is removed before the replies arrive.
                if reject_restore {
                    events.push(AlertEvent::Upsert(Box::new(second)));
                }
                events.push(AlertEvent::Remove(older.id.unwrap()));
                let mut replies = vec![(restore, reject_restore), (edit, !reject_restore)];
                if reverse_replies {
                    replies.reverse();
                }
                for (reply, reject) in replies {
                    reply.send(if reject {
                        Err(FeedError::NotSupported)
                    } else {
                        Ok(())
                    });
                }
                if !reject_restore {
                    first.extend_line = true;
                    events.push(AlertEvent::Upsert(Box::new(first)));
                }
                poll(&mut panel);
                panel.restore_presentation(!reject_restore);
                let saves = host.saved.lock().unwrap();
                assert_eq!(
                    saves.len(),
                    5,
                    "reject_restore={reject_restore}, reverse_replies={reverse_replies}"
                );
                let sent = saves.last().unwrap();
                assert_eq!(
                    (sent.id.as_ref().unwrap().0.as_str(), sent.extend_line),
                    if reject_restore {
                        ("2", true)
                    } else {
                        ("1", false)
                    }
                );
            }
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
        panel.set_price("SIM", Some(98.9137), Some(0.25));
        assert_eq!(panel.draft.condition, AlertCondition::Crossing(99.0));
        panel.draft_saving = panel.save_with(panel.draft.clone(), DRAFT_REPLY).is_some();
        assert_eq!((panel.pending, panel.draft_saving), (1, true));
        assert_eq!(
            host.saved.lock().unwrap().last().unwrap().condition,
            AlertCondition::Crossing(99.0)
        );
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
        panel.draft_saving = panel.save_with(panel.draft.clone(), DRAFT_REPLY).is_some();
        panel.set_price("SIM", Some(100.127), Some(0.25));
        host.reply.lock().unwrap().take().unwrap().send(Ok(()));
        frame(&mut panel);
        assert_eq!(panel.error, None);
        assert_eq!(panel.draft.symbol, "SIM");
        assert_eq!(panel.draft.condition, AlertCondition::Crossing(100.25));
        panel.set_price("NEXT", Some(200.137), Some(0.25));
        assert_eq!(panel.draft.symbol, "NEXT");
        assert_eq!(panel.draft.condition, AlertCondition::Crossing(200.25));
        panel.set_symbol("UNKNOWN");
        assert!(panel.draft.condition.level().unwrap().is_nan());
        assert!(!panel.draft.is_valid());
        assert!(panel.alerts().is_empty());
    }

    /// Guards #3737: an unknown price cannot save zero; replacing a suggested level
    /// accepts grouped negative input and survives quote and symbol changes.
    #[test]
    fn inline_level_is_empty_without_a_quote_and_keeps_user_input() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut panel = AlertsPanel::default();
        panel.set_symbol("SIM");
        let mut h = Harness::builder()
            .with_size(egui::vec2(300.0, 400.0))
            .build_ui_state(|ui, panel: &mut AlertsPanel| panel.draft_form(ui), panel);
        h.get_by_label("Quick alert").click();
        h.run();
        assert!(
            h.get_by_label("Create alert")
                .accesskit_node()
                .is_disabled()
        );
        let level = h
            .get_by_role_and_label(egui::Role::TextInput, "Level")
            .rect();
        let label = h.get_by_role_and_label(egui::Role::Label, "Level").rect();
        assert!(
            level.y_range().contains(label.center().y),
            "the numeric field keeps its row"
        );
        assert!(
            h.get_by_role_and_label(egui::Role::TextInput, "Level")
                .value()
                .is_none_or(|value| value.is_empty())
        );
        for (quote, tick, price, text) in [
            (98.9137, 0.25, 99.0, "99.00"),
            (1.084537, 0.00001, 1.08454, "1.08454"),
        ] {
            h.state_mut().set_price("SIM", Some(quote), Some(tick));
            h.run();
            assert_eq!(h.state().draft.condition, AlertCondition::Crossing(price));
            assert_eq!(
                h.get_by_role_and_label(egui::Role::TextInput, "Level")
                    .value()
                    .as_deref(),
                Some(text),
            );
        }
        h.get_by_role_and_label(egui::Role::TextInput, "Level")
            .click();
        h.run();
        h.get_by_role_and_label(egui::Role::TextInput, "Level")
            .type_text("−1 234.56");
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(
            h.state().draft.condition,
            AlertCondition::Crossing(-1234.56)
        );
        assert!(
            !h.get_by_label("Create alert")
                .accesskit_node()
                .is_disabled()
        );
        h.state_mut().set_price("SIM", Some(99.0), Some(0.25));
        h.state_mut().set_price("NEXT", Some(200.0), Some(0.25));
        h.run();
        assert_eq!(
            h.state().draft.condition,
            AlertCondition::Crossing(-1234.56)
        );
    }

    /// A level chosen from a finite channel keeps its lower bound; other conditions preserve
    /// the previous level text and keep following the quote until the level is entered.
    #[test]
    fn inline_channel_to_level_keeps_lower_bound() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut failures = Vec::new();
        for (origin, quote, typed, bound, level, text) in [
            (
                "Entering channel",
                Some(99.0),
                None,
                Some(95.0),
                Some(95.0),
                "95",
            ),
            ("Entering channel", None, None, Some(95.0), Some(95.0), "95"),
            (
                "Entering channel",
                Some(99.0),
                Some("120"),
                Some(95.0),
                Some(95.0),
                "95",
            ),
            (
                "Entering channel",
                None,
                Some("120"),
                Some(95.0),
                Some(95.0),
                "95",
            ),
            ("Moving up", Some(99.0), None, None, Some(99.0), "99.00"),
            ("Moving up", None, None, None, None, ""),
            (
                "Moving up",
                Some(99.0),
                Some("120"),
                None,
                Some(120.0),
                "120",
            ),
            ("Entering channel", None, None, None, None, ""),
        ] {
            let mut panel = AlertsPanel::default();
            panel.set_price("SIM", quote, None);
            let mut h = Harness::builder()
                .with_size(egui::vec2(300.0, 500.0))
                .build_ui_state(|ui, panel: &mut AlertsPanel| panel.draft_form(ui), panel);
            h.get_by_label("Quick alert").click();
            h.run();
            if quote.is_some() && typed.is_none() {
                h.get_by_label("Condition").click();
                h.run();
                h.get_by_label("Crossing up").click();
                h.run();
                h.state_mut().set_price("SIM", Some(101.0), None);
                h.run();
                assert_eq!(h.state().draft.condition, AlertCondition::CrossingUp(101.0));
                h.state_mut().set_price("SIM", quote, None);
                h.run();
            }
            if let Some(text) = typed {
                h.get_by_role_and_label(egui::Role::TextInput, "Level")
                    .click();
                h.run();
                h.get_by_role_and_label(egui::Role::TextInput, "Level")
                    .type_text(text);
                h.key_press(egui::Key::Enter);
                h.run();
                assert_eq!(h.state().draft.condition.level(), Some(120.0));
            }
            h.get_by_label("Condition").click();
            h.run();
            h.get_by_label(origin).scroll_to_me();
            h.run();
            h.get_by_label(origin).click();
            h.run();
            assert_eq!(
                matches!(h.state().draft.condition, AlertCondition::MovingBy { .. }),
                origin == "Moving up",
                "{origin}: {:?}",
                h.state().draft.condition
            );
            if let Some(lower) = bound {
                h.state_mut().draft.condition = AlertCondition::EnteringChannel {
                    lower,
                    upper: 100.0,
                };
                h.run();
            }
            h.get_by_label("Condition").click();
            h.run();
            h.get_by_label("Crossing").scroll_to_me();
            h.run();
            h.get_by_label("Crossing").click();
            h.run();
            let follows_quote = bound.is_none() && typed.is_none();
            for (quote_update, level, text) in [
                (None, level, text),
                (
                    Some(101.0),
                    if follows_quote { Some(101.0) } else { level },
                    if follows_quote { "101.00" } else { text },
                ),
            ] {
                if let Some(price) = quote_update {
                    h.state_mut().set_price("SIM", Some(price), None);
                    h.run();
                }
                let AlertCondition::Crossing(actual_level) = h.state().draft.condition else {
                    panic!(
                        "expected Crossing after {origin}: {:?}",
                        h.state().draft.condition
                    );
                };
                let actual = (
                    (!actual_level.is_nan()).then_some(actual_level),
                    h.get_by_role_and_label(egui::Role::TextInput, "Level")
                        .value()
                        .unwrap_or_default(),
                    h.get_by_label("Create alert")
                        .accesskit_node()
                        .is_disabled(),
                );
                let expected = (level, text.to_owned(), level.is_none());
                if actual != expected {
                    failures.push(format!(
                        "{origin}, quote={quote:?}, typed={typed:?}, bound={bound:?}, quote_update={quote_update:?}: expected {expected:?}, got {actual:?}"
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
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

    /// Enable on an alert past its expiry, expired or paused, opens it in the editor instead of
    /// saving it, since the engine would expire it again at the next tick. An alert without an
    /// expiry is enabled at once.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn enable_opens_an_alert_past_its_expiry_in_the_editor() {
        use AlertStatus::{Active, Disabled, Expired};
        use egui_kittest::{Harness, kittest::Queryable};
        let now = Utc::now();
        let yesterday = Some(now - chrono::TimeDelta::days(1));
        let alerts = [
            ("1", Expired, yesterday),
            ("2", Disabled, None),
            ("3", Disabled, yesterday),
        ]
        .map(|(id, status, expires)| {
            let mut alert = Alert::new("AAPL", AlertCondition::Above(100.0));
            alert.id = Some(AlertId(id.into()));
            alert.status = status;
            alert.expires = expires;
            alert
        });
        let engine = Arc::new(crate::services::AlertEngine::new().with_alerts(alerts.to_vec()));
        let mut panel = AlertsPanel::new(AlertsPanelConfig { max_height: 800.0 });
        panel.set_service(engine.clone());
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(400.0, 1000.0))
            .build_ui_state(
                move |ui, (panel, actions): &mut (AlertsPanel, Vec<AlertPanelAction>)| {
                    panel.poll(ui.ctx(), &mut toasts);
                    let action = panel.show(ui);
                    if action != AlertPanelAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            );
        h.run_steps(3);
        // The list is ordered by id; enabling the second alert turns its button into Pause.
        for row in [0, 2, 1] {
            h.get_all_by_label("Enable").nth(row).unwrap().click();
            h.run_steps(3);
        }
        let [mut resumed, _, mut paused] = alerts;
        resumed.status = Active;
        paused.status = Active;
        assert_eq!(
            h.state().1,
            [resumed.clone(), paused].map(|alert| AlertPanelAction::EditAlert(Box::new(alert)))
        );
        let status = || engine.alerts().iter().map(|a| a.status).collect::<Vec<_>>();
        assert_eq!(status(), [Expired, Active, Disabled]);
        // Saved from the editor with a later expiry, it stays active through the next tick.
        resumed.expires = Some(now + chrono::TimeDelta::days(1));
        h.state_mut().0.save(resumed);
        engine.tick(Utc::now());
        assert_eq!(status(), [Active, Active, Disabled]);
    }

    /// A copy is a new alert that has seen no entry: saved, it waits for one, and the original
    /// stays where it was.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn a_duplicated_position_alert_waits_for_its_entry() {
        use AlertPositionProgress::{AwaitingEntry, Entered};
        use egui_kittest::{Harness, kittest::Queryable};
        let position = |progress| AlertCondition::Position {
            levels: Box::new(["p", "p#1", "p#2"].map(|id| AlertSource::Drawing(id.into()))),
            progress,
        };
        let mut original = Alert::new("AAPL", position(Entered));
        original.id = Some(AlertId("1".into()));
        let engine = Arc::new(crate::services::AlertEngine::new().with_alerts(vec![original]));
        let mut panel = AlertsPanel::new(AlertsPanelConfig { max_height: 800.0 });
        panel.set_service(engine.clone());
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(400.0, 1000.0))
            .build_ui_state(
                move |ui, (panel, actions): &mut (AlertsPanel, Vec<AlertPanelAction>)| {
                    panel.poll(ui.ctx(), &mut toasts);
                    let action = panel.show(ui);
                    if action != AlertPanelAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            );
        h.run_steps(3);
        h.get_by_label("Duplicate").click();
        h.run_steps(3);
        let [AlertPanelAction::DuplicateAlert(copy)] = h.state().1.as_slice() else {
            panic!("{:?}", h.state().1);
        };
        let copy = (**copy).clone();
        // A host that keeps alerts itself stores the copy as the panel returns it.
        assert_eq!(copy.condition, position(AwaitingEntry));
        h.state_mut().0.save(copy);
        let saved: Vec<_> = engine.alerts().into_iter().map(|a| a.condition).collect();
        assert_eq!(saved, [position(Entered), position(AwaitingEntry)]);
    }

    #[cfg(feature = "alert-engine")]
    #[test]
    fn the_log_narrows_to_an_alert_or_a_symbol_and_clears_what_it_shows() {
        use egui_kittest::{Harness, kittest::Queryable};
        let alert = |id: &str, name: &str, symbol: &str, condition| {
            let mut alert = Alert::new(symbol, condition);
            alert.id = Some(AlertId(id.into()));
            alert.name = name.into();
            alert
        };
        let trigger = |id: &str, name: &str, symbol: &str, time: &str| {
            let mut trigger = AlertTrigger::new(AlertId(id.into()), symbol, time.parse().unwrap());
            trigger.name = name.into();
            trigger.price = Some(123.5);
            trigger.message = "A logged message".into();
            trigger
        };
        let dip = trigger("3", "Dip", "MSFT", "2026-10-23T12:02:00Z");
        let engine = Arc::new(
            crate::services::AlertEngine::new()
                .with_alerts(vec![
                    alert("1", "Breakout", "AAPL", AlertCondition::Above(100.0)),
                    alert("2", "", "AAPL", AlertCondition::Above(100.0)),
                    alert("3", "Dip", "MSFT", AlertCondition::Below(50.0)),
                ])
                .with_triggers(vec![
                    trigger("1", "Breakout", "AAPL", "2026-10-23T12:00:00Z"),
                    trigger("2", "", "AAPL", "2026-10-23T12:01:00Z"),
                    dip.clone(),
                ]),
        );
        let mut panel = AlertsPanel::new(AlertsPanelConfig { max_height: 600.0 });
        panel.set_service(engine.clone());
        panel.set_timezone("+05:45".parse().unwrap());
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(400.0, 800.0))
            .build_ui_state(
                move |ui, panel: &mut AlertsPanel| {
                    panel.poll(ui.ctx(), &mut toasts);
                    panel.show(ui);
                },
                panel,
            );
        h.run_steps(3);
        h.get_by_label("Log").click();
        h.run_steps(3);
        // 12:00 UTC is 17:45 at +05:45; the unnamed alert is named by its condition on arrival.
        let rows = [
            "Breakout · AAPL · 2026-10-23 17:45:00 +05:45",
            "Greater than 100 · AAPL · 2026-10-23 17:46:00 +05:45",
            "Dip · MSFT · 2026-10-23 17:47:00 +05:45",
        ];
        let shown = |h: &Harness<'_, AlertsPanel>| rows.map(|row| h.query_by_label(row).is_some());
        assert_eq!(shown(&h), [true; 3]);
        for (filter, entry, expected) in [
            ("Filter by symbol", "AAPL", [true, true, false]),
            ("Filter by alert", "Breakout", [true, false, false]),
            ("Filter by alert", "All alerts", [true, true, false]),
        ] {
            h.get_by_label(filter).click();
            h.run_steps(3);
            h.get_by_label(entry).click();
            h.run_steps(3);
            assert_eq!(shown(&h), expected, "{filter}: {entry}");
        }
        // Old presentation options keep every log field visible.
        let old: AlertListOptions = serde_json::from_str(r#"{"sort":"Created"}"#).unwrap();
        let restored = serde_json::to_value(old).unwrap();
        for field in ["log_symbol", "log_price", "log_message"] {
            assert_eq!(restored[field], true, "{field}");
        }
        for field in ["Message", "Price"] {
            h.get_by_label("Fields").click();
            h.run();
            h.get_by_label(field).click();
            h.run();
        }
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.query_all_by_label("A logged message").count() == 0);
        assert!(h.query_all_by_label("123.5").count() == 0);
        // Export is a fixed format independent of the visible fields.
        assert!(h.state().log_csv().contains("123.5,\"A logged message\""));
        for _ in 0..2 {
            h.get_by_label("Undo").click();
            h.run();
        }
        assert!(h.query_all_by_label("A logged message").count() > 0);
        assert!(h.query_all_by_label("123.5").count() > 0);
        // Clearing removes the entries shown, also from the engine, and shows the whole log again.
        for (answer, kept, expected) in [
            ("Cancel", 3, [true, true, false]),
            ("Clear", 1, [false, false, true]),
        ] {
            h.get_by_label("Clear log…").click();
            h.run_steps(3);
            h.get_by_label(answer).click();
            h.run_steps(3);
            assert_eq!(engine.triggers().len(), kept, "{answer}");
            assert_eq!(shown(&h), expected, "{answer}");
        }
        assert_eq!(*h.state().log(), std::slice::from_ref(&dip));
        assert_eq!(engine.triggers(), [dip]);
    }

    #[cfg(feature = "alert-engine")]
    #[test]
    fn a_row_opens_on_double_click_and_its_menu_acts_like_the_buttons() {
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        let mut first = Alert::new("AAPL", AlertCondition::Above(100.0));
        first.id = Some(AlertId("1".into()));
        first.name = "First".into();
        let mut second = first.clone();
        second.id = Some(AlertId("2".into()));
        second.name = "Second".into();
        second.status = AlertStatus::Expired;
        second.expires = Some(Utc::now() - chrono::TimeDelta::days(1));
        let engine = Arc::new(
            crate::services::AlertEngine::new().with_alerts(vec![first.clone(), second.clone()]),
        );
        let mut panel = AlertsPanel::new(AlertsPanelConfig { max_height: 800.0 });
        panel.set_service(engine.clone());
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(60)
            .allow_missing_glyphs()
            .with_size(egui::vec2(400.0, 1000.0))
            .build_ui_state(
                move |ui, (panel, actions): &mut (AlertsPanel, Vec<AlertPanelAction>)| {
                    panel.poll(ui.ctx(), &mut toasts);
                    let action = panel.show(ui);
                    if action != AlertPanelAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            );
        h.run();
        for _ in 0..2 {
            h.get_by_role_and_label(Role::ListBoxOption, "AAPL: First")
                .click();
            h.run_steps(1);
        }
        assert_eq!(
            h.state().1,
            [AlertPanelAction::EditAlert(Box::new(first.clone()))]
        );
        h.get_by_role_and_label(Role::ListBoxOption, "AAPL: First")
            .click_secondary();
        h.run();
        h.get_by_role(Role::Menu).get_by_label("Pause").click();
        h.run();
        assert_eq!(engine.alerts()[0].status, AlertStatus::Disabled);
        h.get_by_role_and_label(Role::ListBoxOption, "AAPL: Second")
            .click_secondary();
        h.run();
        h.get_by_role(Role::Menu).get_by_label("Enable").click();
        h.run();
        second.status = AlertStatus::Active;
        assert_eq!(
            h.state().1.last(),
            Some(&AlertPanelAction::EditAlert(Box::new(second)))
        );
        assert_eq!(engine.alerts()[1].status, AlertStatus::Expired);
        h.get_by_role_and_label(Role::ListBoxOption, "AAPL: First")
            .click_secondary();
        h.run();
        h.get_by_role(Role::Menu).get_by_label("Duplicate").click();
        h.run();
        first.id = None;
        first.created_at = None;
        first.last_triggered_at = None;
        assert_eq!(
            h.state().1.last(),
            Some(&AlertPanelAction::DuplicateAlert(Box::new(first)))
        );
        h.get_by_role_and_label(Role::ListBoxOption, "AAPL: Second")
            .click_secondary();
        h.run();
        h.get_by_role(Role::Menu).get_by_label("Remove").click();
        h.run();
        assert_eq!(engine.alerts().len(), 1);
    }

    #[test]
    fn a_focused_log_row_keeps_its_trigger_when_the_log_evicts_entries() {
        use egui::{Key, accesskit::Role};
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };

        let trigger = |name: &str, symbol: &str| {
            let mut trigger = AlertTrigger::new(
                AlertId("shared".into()),
                symbol,
                "2026-11-20T12:00:00Z".parse().unwrap(),
            );
            trigger.name = name.into();
            trigger
        };
        let mut panel = AlertsPanel::default();
        for _ in 0..LOG_BOUND - 3 {
            panel.log_trigger(trigger("Hidden", "MSFT"));
        }
        for name in ["Older", "Kept", "Newer"] {
            panel.log_trigger(trigger(name, "AAPL"));
        }
        panel.show_log = true;
        panel.log_symbol = Some("AAPL".into());
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(400.0, 800.0))
            .build_ui_state(
                |ui, panel: &mut AlertsPanel| {
                    panel.show(ui);
                },
                panel,
            );
        h.run();
        let kept = "Kept · AAPL · 2026-11-20 12:00:00 +00:00";
        h.get_by_role_and_label(Role::ListBoxOption, kept).click();
        h.run();
        assert_eq!(
            h.get_by_role_and_label(Role::ListBoxOption, kept)
                .accesskit_node()
                .is_selected(),
            Some(true)
        );
        for name in ["Latest", "Newest"] {
            h.state_mut().log_trigger(trigger(name, "AAPL"));
            h.run();
            assert_eq!(h.state().log().len(), LOG_BOUND);
            let row = h.get_by_role_and_label(Role::ListBoxOption, kept);
            assert_eq!(row.accesskit_node().is_selected(), Some(true));
            assert!(row.accesskit_node().is_focused());
        }
        h.key_press(Key::ArrowDown);
        h.run();
        let older = h.get_by_role_and_label(
            Role::ListBoxOption,
            "Older · AAPL · 2026-11-20 12:00:00 +00:00",
        );
        assert!(older.accesskit_node().is_focused());
        assert_eq!(older.accesskit_node().is_selected(), Some(true));
    }

    #[cfg(feature = "alert-engine")]
    #[test]
    fn the_keys_move_the_selection_and_enter_edits_the_alert() {
        use egui::{Key, accesskit::Role};
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let alerts: Vec<_> = ["First", "Second", "Third"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let mut alert = Alert::new("AAPL", AlertCondition::Above(100.0));
                alert.id = Some(AlertId(i.to_string()));
                alert.name = name.into();
                alert
            })
            .collect();
        let triggers = alerts
            .iter()
            .map(|alert| {
                let mut trigger = AlertTrigger::new(
                    alert.id.clone().unwrap(),
                    "AAPL",
                    "2026-11-20T12:00:00Z".parse().unwrap(),
                );
                trigger.name = alert.name.clone();
                trigger
            })
            .collect();
        let engine = Arc::new(
            crate::services::AlertEngine::new()
                .with_alerts(alerts.clone())
                .with_triggers(triggers),
        );
        let mut panel = AlertsPanel::new(AlertsPanelConfig { max_height: 800.0 });
        panel.set_service(engine.clone());
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(400.0, 1000.0))
            .build_ui_state(
                move |ui, (panel, actions): &mut (AlertsPanel, Vec<AlertPanelAction>)| {
                    panel.poll(ui.ctx(), &mut toasts);
                    let action = panel.show(ui);
                    if action != AlertPanelAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            );
        h.run();
        h.get_by_role_and_label(Role::ListBoxOption, "AAPL: First")
            .click();
        h.run();
        for _ in 0..20 {
            h.key_press(Key::Tab);
            h.run();
            if h.get_by_role_and_label(Role::ListBoxOption, "AAPL: Second")
                .accesskit_node()
                .is_focused()
            {
                break;
            }
        }
        let second = h.get_by_role_and_label(Role::ListBoxOption, "AAPL: Second");
        assert!(second.accesskit_node().is_focused());
        assert_eq!(second.accesskit_node().is_selected(), Some(true));
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(
            h.state_mut().1.pop(),
            Some(AlertPanelAction::EditAlert(Box::new(alerts[1].clone())))
        );
        h.key_press(Key::ArrowUp);
        h.run();
        h.key_press(Key::ArrowDown);
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(
            h.state().1,
            [AlertPanelAction::EditAlert(Box::new(alerts[1].clone()))]
        );
        assert_eq!(
            h.get_by_role_and_label(Role::ListBoxOption, "AAPL: Second")
                .accesskit_node()
                .is_selected(),
            Some(true)
        );
        h.key_press(Key::Delete);
        h.run();
        h.get_by_label("Cancel").click();
        h.run();
        assert_eq!(engine.alerts().len(), 3);
        h.key_press(Key::ArrowDown);
        h.run();
        assert_eq!(
            h.get_by_role_and_label(Role::ListBoxOption, "AAPL: Third")
                .accesskit_node()
                .is_selected(),
            Some(true),
            "Cancel restores keyboard browsing"
        );
        h.get_by_label("Log").click();
        h.run();
        h.get_all_by_role(Role::ListBoxOption)
            .next()
            .unwrap()
            .click();
        h.run();
        h.key_press(Key::Tab);
        h.run();
        let second = h.get_all_by_role(Role::ListBoxOption).nth(1).unwrap();
        assert!(second.accesskit_node().is_focused());
        assert_eq!(second.accesskit_node().is_selected(), Some(true));
        h.key_press(Key::ArrowUp);
        h.run();
        assert_eq!(
            h.get_all_by_role(Role::ListBoxOption)
                .next()
                .unwrap()
                .accesskit_node()
                .is_selected(),
            Some(true)
        );
        for (key, selected) in [(Key::ArrowDown, 1), (Key::Space, 2), (Key::ArrowUp, 1)] {
            h.key_press(key);
            h.run();
            let states: Vec<_> = h
                .get_all_by_role(Role::ListBoxOption)
                .map(|row| row.accesskit_node().is_selected())
                .collect();
            assert_eq!(
                states,
                (0..3).map(|i| Some(i == selected)).collect::<Vec<_>>(),
                "{key:?}"
            );
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

    /// Trigger toasts kept until dismissed are never closed to make room, and an auto-closing
    /// trigger toast still shows beside five of them. The stack keeps as many as the log, the
    /// oldest closing beyond.
    #[test]
    fn toasts_kept_until_dismissed_are_not_closed_to_make_room() {
        let mut panel = AlertsPanel::default();
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        let trigger = |n: u32| AlertTrigger {
            id: AlertId(n.to_string()),
            name: String::new(),
            symbol: format!("S{n}"),
            time: Utc::now(),
            price: None,
            message: String::new(),
            silent: false,
        };
        panel.set_notification_seconds(0.0);
        for n in 0..6 {
            panel.record_trigger(&ctx, trigger(n), &mut toasts);
        }
        assert_eq!(toasts.len(), 6);
        panel.set_notification_seconds(20.0);
        panel.record_trigger(&ctx, trigger(6), &mut toasts);
        assert_eq!(toasts.len(), 7);
        assert_eq!(toasts.toasts()[6].title.as_deref(), Some("6 on S6"));
        panel.set_notification_seconds(0.0);
        while toasts.len() < 10_000 {
            toasts.add(Toast::info("kept").permanent());
        }
        for n in 7..9 {
            panel.record_trigger(&ctx, trigger(n), &mut toasts);
        }
        assert_eq!(toasts.len(), 10_000);
        assert_eq!(toasts.toasts()[0].title.as_deref(), Some("1 on S1"));
        assert_eq!(toasts.toasts()[9_999].title.as_deref(), Some("8 on S8"));
    }

    /// A popup names the alert and its symbol, and once the host offers them, Open chart and Edit
    /// alert close it and return their actions.
    #[test]
    fn popup_actions_open_the_chart_and_edit_the_alert() {
        use crate::ui_kit::notifications::NotificationPanel;
        use egui_kittest::{Harness, kittest::Queryable};
        let host = Arc::new(Host::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        let events = host.events.lock().unwrap().clone().unwrap();
        let mut alert = Alert::new("AAPL", AlertCondition::Above(100.0));
        alert.id = Some(AlertId("1".into()));
        alert.name = "Breakout".into();
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        let time = "2026-10-23T12:00:00Z".parse().unwrap();
        let fire = || {
            events.push(AlertEvent::Triggered {
                id: AlertId("1".into()),
                time,
                price: Some(101.0),
            });
        };
        fire();
        type State = (AlertsPanel, Toasts, Vec<AlertPanelAction>);
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(800.0, 600.0))
            .build_ui_state(
                |ui, (panel, toasts, actions): &mut State| {
                    panel.poll(ui.ctx(), toasts);
                    let action = panel.take_popup_action();
                    if action != AlertPanelAction::None {
                        actions.push(action);
                    }
                    NotificationPanel::new().show_with_toasts(ui.ctx(), toasts);
                },
                (panel, Toasts::new(), Vec::new()),
            );
        h.run_steps(3);
        let toast = &h.state().1.toasts()[0];
        assert_eq!(toast.title.as_deref(), Some("Breakout on AAPL"));
        assert_eq!(toast.message, "Alert triggered");
        assert!(h.query_by_label("Open chart").is_none());
        h.state_mut().1.clear();
        h.state_mut().0.set_popup_actions(true);
        for action in ["Open chart", "Edit alert"] {
            fire();
            h.run_steps(3);
            h.get_by_label(action).click();
            h.run_steps(3);
            assert!(h.state().1.is_empty(), "{action} closes the popup");
        }
        alert.last_triggered_at = Some(time);
        assert_eq!(
            h.state().2,
            [
                AlertPanelAction::SelectSymbol("AAPL".into()),
                AlertPanelAction::EditAlert(Box::new(alert))
            ]
        );
    }

    /// Each trigger popup also goes to the desktop when the host routes it there, and a refused
    /// delivery shows one error toast, which stays after the trigger toasts close.
    #[test]
    fn a_popup_goes_to_the_desktop_and_a_refusal_is_shown_once() {
        #[derive(Default)]
        struct Desktop(Mutex<Vec<Notification>>);
        impl Notifications for Desktop {
            fn watch(&self, _: Events<Notification>) {}
            fn deliver(&self, notification: Notification, reply: Reply<()>) {
                self.0.lock().unwrap().push(notification);
                reply.send(Err(FeedError::Other("permission denied".into())));
            }
        }
        let host = Arc::new(Host::default());
        let desktop = Arc::new(Desktop::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        panel.set_desktop(Some(desktop.clone()));
        let events = host.events.lock().unwrap().clone().unwrap();
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        let frame = |panel: &mut AlertsPanel, toasts: &mut Toasts| {
            ctx.run_ui(egui::RawInput::default(), |ui| panel.poll(ui.ctx(), toasts))
                .drop_without_applying_deltas();
        };
        let mut alert = Alert::new("SYM", AlertCondition::Above(1.0));
        alert.id = Some(AlertId("1".into()));
        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        let mut errors = Vec::new();
        for delivered in 1..=2 {
            events.push(AlertEvent::Triggered {
                id: alert.id.clone().unwrap(),
                time: "2026-10-23T12:00:00Z".parse().unwrap(),
                price: None,
            });
            frame(&mut panel, &mut toasts);
            frame(&mut panel, &mut toasts);
            let sent = desktop.0.lock().unwrap();
            assert_eq!(sent.len(), delivered);
            assert_eq!(sent[0].title, "Greater than 1 on SYM");
            assert_eq!(sent[0].body, "Alert triggered");
            assert_eq!(sent[0].symbol.as_deref(), Some("SYM"));
            errors.push(
                toasts
                    .toasts()
                    .iter()
                    .filter(|t| t.kind == ToastKind::Error)
                    .map(|t| (t.id, t.message.clone()))
                    .collect::<Vec<_>>(),
            );
            // A minute later the trigger toast has closed; the error waits to be dismissed.
            for toast in toasts.toasts_mut() {
                toast.created_at -= 60.0;
            }
            toasts.cleanup_expired();
        }
        let refused = "Desktop notification failed: permission denied".to_owned();
        assert_eq!(errors[0], [(errors[0][0].0, refused)]);
        assert_eq!(errors[1], errors[0], "the first error toast stays, alone");
    }

    #[test]
    fn a_user_alert_name_is_shown_as_saved_under_a_translator() {
        use egui_kittest::{Harness, kittest::Queryable};
        let host = Arc::new(Host::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        // The name is also the catalogue key of the status shown below it.
        let mut alert = Alert::new("SAMPLE", AlertCondition::Above(100.0));
        alert.id = Some(AlertId("saved".into()));
        alert.name = "Active".into();
        let events = host.events.lock().unwrap().clone().unwrap();
        events.push(AlertEvent::Upsert(Box::new(alert)));
        let mut toasts = Toasts::new();
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(300.0, 700.0))
            .with_context_setup(|ctx| {
                l10n::set_translator(ctx, |key| (key == "Active").then(|| "Aktiv".to_owned()));
            })
            .build_ui_state(
                move |ui, panel: &mut AlertsPanel| {
                    panel.poll(ui.ctx(), &mut toasts);
                    panel.show(ui);
                },
                panel,
            );
        h.run_steps(3);
        let shown = |text: &str| h.query_all_by_label(text).count();
        assert_eq!(shown("Active"), 1, "the name as saved");
        assert_eq!(shown("Aktiv"), 1, "the translated status");

        let id = AlertId("saved".into());
        let mut trigger = AlertTrigger::new(
            id.clone(),
            "SAMPLE",
            "2026-11-20T12:00:00Z".parse().unwrap(),
        );
        trigger.name = "Active".into();
        events.push(AlertEvent::Logged(trigger));
        h.state_mut().show_log = true;
        h.state_mut().log_alert = Some(id);
        h.run_steps(3);
        assert_eq!(
            (
                h.query_all_by_label("Active · SAMPLE · 2026-11-20 12:00:00 +00:00")
                    .count(),
                h.get_by_label("Filter by alert").value(),
                h.query_all_by_label("Aktiv").count(),
            ),
            (1, Some("Active".to_owned()), 0),
            "the log row and selected alert filter show the saved name"
        );
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
