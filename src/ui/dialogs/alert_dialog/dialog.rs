//! Typed alert editing. Hosts supply output identifiers, watchlists and optional preset storage.

use crate::localization as l10n;
use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Months, NaiveDate, NaiveTime, Offset, TimeZone, Timelike, Utc, Weekday};
use egui::{Context, Ui};

use super::preset::{self, AlertPreset, MAX_PRESETS, PresetFields};
use super::{AlertDialogAction, AlertDialogConfig};
use crate::data::inbox::Inbox;
use crate::model::{
    Alert, AlertChannelComparison, AlertComparison, AlertCondition, AlertFrequency,
    AlertPositionProgress, AlertRule, AlertSource, AlertSourceName, NotificationSchedule,
    NotificationWindow, Timeframe, TimezoneId,
};
use crate::services::revisions::{Revisions, Settled, Written};
use crate::services::{ConflictChoice, Storage, StoreKind};
use crate::ui_kit::FormGrid;
use crate::ui_kit::dialog::{DialogFooter, dialog_header};
use crate::ui_kit::modal::Modal;
use crate::ui_kit::status_message::{MessageKind, status_message};

/// The name of the presets document.
const PRESETS: &str = "presets";

/// A dialog that creates or edits the same alert type the host stores and evaluates.
pub struct AlertDialog {
    /// Layout dimensions.
    pub config: AlertDialogConfig,
    /// Whether the modal is visible.
    pub is_open: bool,
    /// Staged edits. Cancel never submits them.
    pub alert: Alert,
    /// Host-provided study outputs and drawings, as stable identifiers and names, which are
    /// translated when shown. Presets compare the names' English text to tell a study they
    /// store from another chart's study with the same identifier.
    pub sources: Vec<(AlertSource, AlertSourceName)>,
    /// Host-provided watchlist identifiers and display names.
    pub watchlists: Vec<(String, String)>,
    /// Live strategy run ids accepted by `AlertEngine::strategy_fills` and their display names.
    /// Strategy fills is offered only when this list has a run; an existing alert on an absent
    /// run displays its id.
    ///
    /// ```
    /// use egui_charts::ui::dialogs::AlertDialog;
    ///
    /// let mut dialog = AlertDialog::new();
    /// dialog.strategy_runs = vec![("run-7".into(), "Momentum live".into())];
    /// assert_eq!(dialog.strategy_runs[0].0, "run-7");
    /// ```
    pub strategy_runs: Vec<(String, String)>,
    /// The zone the expiry date and time are entered in, UTC by default. Set it before
    /// [`open`](Self::open) or [`edit`](Self::edit), usually to the chart's time zone.
    pub timezone: TimezoneId,
    /// The account's time zone, when the host knows it. A notification schedule then offers it as
    /// its zone and starts from it, instead of from [`timezone`](Self::timezone). Set it before
    /// [`open`](Self::open) or [`edit`](Self::edit).
    pub account_timezone: Option<TimezoneId>,
    /// Whether the staged alert starts monitoring at a set time; otherwise it starts when saved.
    starts: bool,
    /// The start date as `YYYY-MM-DD` in [`timezone`](Self::timezone).
    start_date: String,
    /// The start time as `HH:MM` in [`timezone`](Self::timezone).
    start_time: String,
    /// Whether the staged alert expires; without an expiry it stays active until removed.
    expires: bool,
    /// The expiry date as `YYYY-MM-DD` in [`timezone`](Self::timezone).
    expiry_date: String,
    /// The expiry time as `HH:MM` in [`timezone`](Self::timezone).
    expiry_time: String,
    /// Whether the staged alert limits when notifications are delivered.
    scheduled: bool,
    /// The schedule's zone name.
    schedule_zone: String,
    /// The selected weekdays, Monday first.
    schedule_days: [bool; 7],
    /// The windows as `HH:MM` start and end texts.
    schedule_windows: Vec<[String; 2]>,
    /// The schedule of the latest saved alert that had one, offered to the next new alert.
    last_schedule: Option<NotificationSchedule>,
    editing_condition: usize,
    preset_name: String,
    preset_fields: PresetFields,
    presets: BTreeMap<String, AlertPreset>,
    /// The presets being written; they replace `presets` once the host keeps them.
    unsaved_presets: Option<BTreeMap<String, AlertPreset>>,
    /// The alert as first shown, which resetting returns to. Hosts may adjust a staged alert
    /// before its first frame.
    initial: Option<Alert>,
    storage: Option<Arc<dyn Storage>>,
    inbox: Arc<Inbox>,
    /// The revision the presets were read or written at, and a write the host refused.
    revisions: Revisions,
    /// The presets document being written, until the host answers.
    writing: Option<String>,
    pending: bool,
    loaded: bool,
    error: Option<String>,
}

impl std::fmt::Debug for AlertDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlertDialog")
            .field("config", &self.config)
            .field("is_open", &self.is_open)
            .field("alert", &self.alert)
            .field("sources", &self.sources)
            .field("watchlists", &self.watchlists)
            .field("strategy_runs", &self.strategy_runs)
            .field("timezone", &self.timezone)
            .field("account_timezone", &self.account_timezone)
            .field("starts", &self.starts)
            .field("start_date", &self.start_date)
            .field("start_time", &self.start_time)
            .field("expires", &self.expires)
            .field("expiry_date", &self.expiry_date)
            .field("expiry_time", &self.expiry_time)
            .field("scheduled", &self.scheduled)
            .field("schedule_zone", &self.schedule_zone)
            .field("schedule_days", &self.schedule_days)
            .field("schedule_windows", &self.schedule_windows)
            .field("last_schedule", &self.last_schedule)
            .field("editing_condition", &self.editing_condition)
            .field("preset_name", &self.preset_name)
            .field("preset_fields", &self.preset_fields)
            .field("presets", &self.presets)
            .field("unsaved_presets", &self.unsaved_presets)
            .field("initial", &self.initial)
            .field("pending", &self.pending)
            .field("loaded", &self.loaded)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl Default for AlertDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertDialog {
    /// An initially closed dialog, without host catalogs or storage.
    pub fn new() -> Self {
        Self {
            config: AlertDialogConfig::default(),
            is_open: false,
            alert: Alert::new("", AlertCondition::Crossing(0.0)),
            sources: Vec::new(),
            watchlists: Vec::new(),
            strategy_runs: Vec::new(),
            timezone: TimezoneId::utc(),
            account_timezone: None,
            starts: false,
            start_date: String::new(),
            start_time: String::new(),
            expires: false,
            expiry_date: String::new(),
            expiry_time: String::new(),
            scheduled: false,
            schedule_zone: String::new(),
            schedule_days: [false; 7],
            schedule_windows: Vec::new(),
            last_schedule: None,
            editing_condition: 0,
            preset_name: String::new(),
            preset_fields: PresetFields::default(),
            presets: BTreeMap::new(),
            unsaved_presets: None,
            initial: None,
            storage: None,
            inbox: Arc::default(),
            revisions: Revisions::new(StoreKind::AlertPresets),
            writing: None,
            pending: false,
            loaded: false,
            error: None,
        }
    }

    /// Load presets from the host storage service. A failed load never permits overwriting it.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.inbox = Arc::default();
        self.presets.clear();
        self.unsaved_presets = None;
        self.error = None;
        self.loaded = false;
        self.pending = true;
        self.revisions = Revisions::new(StoreKind::AlertPresets);
        self.writing = None;
        self.revisions.load(&*storage, PRESETS, &self.inbox, 1);
        self.storage = Some(storage);
    }

    /// Whether the host refused to write the presets because another client changed them since
    /// they were read. Until it is settled the unsaved presets stay in memory and no preset can
    /// be saved or deleted; show a
    /// [`StorageConflictDialog`](crate::ui::dialogs::StorageConflictDialog) and pass the choice
    /// to [`resolve_storage_conflict`](Self::resolve_storage_conflict).
    pub fn storage_conflict(&self) -> bool {
        self.revisions.conflicts().next().is_some()
    }

    /// Settles the refused write of the presets, taking effect at the next [`show`](Self::show):
    /// [`ConflictChoice::ReloadRemote`] takes the stored presets,
    /// [`ConflictChoice::KeepLocalCopy`] first saves the unsaved ones as `presets (local copy)`,
    /// and [`ConflictChoice::Retry`] writes them over the stored ones.
    pub fn resolve_storage_conflict(&mut self, choice: ConflictChoice) {
        if let Some(storage) = self.storage.clone() {
            self.revisions.resolve(&*storage, PRESETS, choice);
        }
    }

    /// Open a fresh price alert at the last or hovered price, discarding earlier cancelled edits.
    /// It starts with the schedule of the latest saved alert that had one, if any.
    pub fn open(&mut self, symbol: String, price: f64) {
        let mut alert = Alert::new(symbol, AlertCondition::Crossing(price));
        alert.schedule = self.last_schedule.clone().map(Box::new);
        self.edit(alert);
    }

    /// Stage an existing alert, retaining its host identifier and status when saved.
    pub fn edit(&mut self, alert: Alert) {
        if let Some(inputs) = &alert.inputs {
            self.sources = inputs.sources.clone();
        }

        self.starts = alert.starts.is_some();
        [self.start_date, self.start_time] = self.entry_fields(alert.starts);
        self.expires = alert.expires.is_some();
        [self.expiry_date, self.expiry_time] = self.entry_fields(alert.expires);
        self.set_schedule_fields(alert.schedule.as_deref());
        self.alert = alert;
        self.editing_condition = 0;
        self.preset_name.clear();
        self.preset_fields = PresetFields::default();
        self.initial = None;
        self.is_open = true;
    }

    /// The date and time field texts of `at` in the entry zone, empty without one.
    fn entry_fields(&self, at: Option<DateTime<Utc>>) -> [String; 2] {
        let local = at.map(|t| t.with_timezone(&self.timezone));
        let format = |pattern| {
            local
                .as_ref()
                .map_or_else(String::new, |t| t.format(pattern).to_string())
        };
        [format("%Y-%m-%d"), format("%H:%M")]
    }

    /// The instant entered as `date` and `time` in the entry zone, or the matching error of
    /// `errors`: bad date, bad time, time skipped when clocks go forward. Unchanged fields retain
    /// the `saved` seconds and repeated-hour occurrence. A saved instant whose local time is past
    /// the representable range matches no entry.
    fn entered(
        &self,
        saved: Option<DateTime<Utc>>,
        date: &str,
        time: &str,
        errors: [&'static str; 3],
    ) -> Result<DateTime<Utc>, &'static str> {
        let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").map_err(|_| errors[0])?;
        let time = NaiveTime::parse_from_str(time.trim(), "%H:%M").map_err(|_| errors[1])?;
        let saved = saved.filter(|saved| {
            let offset = saved.with_timezone(&self.timezone).offset().fix();
            saved
                .naive_utc()
                .checked_add_offset(offset)
                .is_some_and(|local| {
                    local.date() == date
                        && local.hour() == time.hour()
                        && local.minute() == time.minute()
                })
        });
        if let Some(saved) = saved {
            return Ok(saved);
        }
        // A newly entered time repeated when clocks go back uses its first occurrence.
        Ok(self
            .timezone
            .from_local_datetime(&date.and_time(time))
            .earliest()
            .ok_or(errors[2])?
            .with_timezone(&Utc))
    }

    /// The staged expiry: none without one, otherwise the entered date and time in the entry
    /// zone, which must exist there and lie after `now`.
    fn expiry(&self, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, &'static str> {
        if !self.expires {
            return Ok(None);
        }
        let expiry = self.entered(
            self.alert.expires,
            &self.expiry_date,
            &self.expiry_time,
            [
                "Enter the expiry date as YYYY-MM-DD",
                "Enter the expiry time as HH:MM",
                "The expiry time is skipped when clocks go forward",
            ],
        )?;
        if expiry <= now {
            return Err("Choose an expiry after the current time");
        }
        Ok(Some(expiry))
    }

    /// The staged start: none without one, otherwise the entered date and time in the entry
    /// zone, which must exist there and lie before the staged `expiry`.
    fn start(&self, expiry: Option<DateTime<Utc>>) -> Result<Option<DateTime<Utc>>, &'static str> {
        if !self.starts {
            return Ok(None);
        }
        let start = self.entered(
            self.alert.starts,
            &self.start_date,
            &self.start_time,
            [
                "Enter the start date as YYYY-MM-DD",
                "Enter the start time as HH:MM",
                "The start time is skipped when clocks go forward",
            ],
        )?;
        if expiry.is_some_and(|expiry| start >= expiry) {
            return Err("Choose a start before the expiry");
        }
        Ok(Some(start))
    }

    /// Fill the schedule fields with `schedule`, or clear them.
    fn set_schedule_fields(&mut self, schedule: Option<&NotificationSchedule>) {
        self.scheduled = schedule.is_some();
        self.schedule_zone = schedule.map_or_else(
            || {
                self.account_timezone
                    .as_ref()
                    .unwrap_or(&self.timezone)
                    .name()
                    .into()
            },
            |s| s.timezone.name().into(),
        );
        self.schedule_days = std::array::from_fn(|i| {
            schedule.is_some_and(|s| s.days.contains(&Weekday::try_from(i as u8).unwrap()))
        });
        self.schedule_windows = schedule.map_or_else(Vec::new, |s| {
            let text = |t: NaiveTime| t.format("%H:%M").to_string();
            s.windows
                .iter()
                .map(|w| [text(w.start), text(w.end)])
                .collect()
        });
    }

    /// The staged schedule: none when notifications go out at any time, otherwise the entered
    /// zone, weekdays and windows.
    fn schedule(&self) -> Result<Option<NotificationSchedule>, &'static str> {
        if !self.scheduled {
            return Ok(None);
        }
        let timezone = TimezoneId::named(self.schedule_zone.trim())
            .ok_or("Enter a known time zone, such as UTC or America/New_York")?;
        let days: Vec<Weekday> = (0..7u8)
            .filter(|&i| self.schedule_days[usize::from(i)])
            .map(|i| Weekday::try_from(i).unwrap())
            .collect();
        let time = |text: &str| NaiveTime::parse_from_str(text.trim(), "%H:%M");
        let windows = self
            .schedule_windows
            .iter()
            .map(|[start, end]| match (time(start), time(end)) {
                (Ok(start), Ok(end)) => Ok(NotificationWindow { start, end }),
                _ => Err("Enter the window times as HH:MM"),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let schedule = NotificationSchedule {
            timezone,
            days,
            windows,
        };
        if schedule.days.is_empty() {
            Err("Select at least one weekday")
        } else if schedule.windows.is_empty() {
            Err("Add a notification window")
        } else {
            Ok(Some(schedule))
        }
    }

    /// Dismiss without saving.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    fn poll(&mut self, ctx: &Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let Some(storage) = self.storage.clone() else {
            return;
        };
        for (_, settled) in self.revisions.poll(&*storage, ctx) {
            match settled {
                Settled::Overwrite(document) => {
                    self.pending = true;
                    self.writing = Some(document.clone());
                    self.revisions
                        .save(&*storage, PRESETS, document, &self.inbox, 2);
                }
                // The copy stays in the host's storage, as a copy of the settings does.
                Settled::Copied(_) => {}
                Settled::Reload(stored) => {
                    self.unsaved_presets = None;
                    let read = stored.map_or_else(
                        || Ok(Default::default()),
                        |doc| preset::from_document(&doc, Utc::now()),
                    );
                    match read {
                        // The schedule of the unsaved presets is dropped with them.
                        Ok((presets, last_schedule)) => {
                            self.presets = presets;
                            self.last_schedule = last_schedule;
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.loaded = false;
                        }
                    }
                }
                Settled::CopyFailed(reason) => {
                    self.error = Some(format!("Could not keep a copy of the presets: {reason}"));
                }
            }
        }
        loop {
            let answers = self.inbox.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (key, answer) in answers {
                if key == 1 {
                    let Some(result) =
                        self.revisions
                            .loaded(&*storage, PRESETS, answer, &self.inbox, 1)
                    else {
                        continue;
                    };
                    self.pending = false;
                    let loaded = result.map_err(|error| error.to_string()).and_then(|doc| {
                        doc.map_or_else(
                            || Ok(Default::default()),
                            |doc| preset::from_document(&doc, Utc::now()),
                        )
                    });
                    match loaded {
                        Ok((presets, last_schedule)) => {
                            self.presets = presets;
                            // A schedule saved before the load finished is newer.
                            self.last_schedule = self.last_schedule.take().or(last_schedule);
                            self.loaded = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                } else {
                    let document = self.writing.take().expect("preset write in flight");
                    let Some(written) =
                        self.revisions
                            .saved(&*storage, PRESETS, &document, answer, &self.inbox, 2)
                    else {
                        self.writing = Some(document);
                        continue;
                    };
                    self.pending = false;
                    match written {
                        Written::Saved => {
                            self.presets = self.unsaved_presets.take().unwrap_or_default();
                        }
                        // The unsaved presets stay until the user's choice.
                        Written::Refused(stored) => {
                            self.revisions.refuse(PRESETS, document, stored)
                        }
                        Written::Failed(error) => {
                            self.unsaved_presets = None;
                            self.error = Some(error.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Whether saving under the typed name would add a preset beyond the limit.
    fn preset_limit_reached(&self) -> bool {
        self.presets.len() >= MAX_PRESETS && !self.presets.contains_key(self.preset_name.trim())
    }

    /// Store the staged alert's selected settings under the typed name, replacing any preset
    /// of that name.
    fn save_preset(&mut self, now: DateTime<Utc>) {
        if self.preset_limit_reached() {
            return;
        }
        let mut presets = self.presets.clone();
        presets.insert(
            self.preset_name.trim().into(),
            AlertPreset::capture(&self.alert, &self.sources, self.preset_fields, now),
        );
        self.write_presets(presets);
    }

    fn delete_preset(&mut self) {
        let mut presets = self.presets.clone();
        presets.remove(self.preset_name.trim());
        self.preset_name.clear();
        self.write_presets(presets);
    }

    /// Keep the staged alert's schedule, if it has one, for the next new alert and the storage.
    fn remember_schedule(&mut self) {
        if let Some(schedule) = &self.alert.schedule {
            self.last_schedule = Some((**schedule).clone());
            if self.loaded && !self.pending && !self.revisions.unsettled() {
                self.write_presets(self.presets.clone());
            }
        }
    }

    /// Write `presets` and the remembered schedule to the host storage.
    fn write_presets(&mut self, presets: BTreeMap<String, AlertPreset>) {
        match preset::to_document(&presets, self.last_schedule.as_ref()) {
            Ok(document) => {
                if let Some(storage) = &self.storage {
                    self.pending = true;
                    self.error = None;
                    self.unsaved_presets = Some(presets);
                    self.writing = Some(document.clone());
                    self.revisions
                        .save(&**storage, PRESETS, document, &self.inbox, 2);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Set a preset's selected settings on the staged alert. Its name and fields stay in the
    /// preset controls, so that a replacement starts from them.
    fn apply_preset(&mut self, name: String, now: DateTime<Utc>) {
        let Some(preset) = self.presets.get(&name) else {
            return;
        };
        preset.apply(&mut self.alert, &self.sources, now);
        let fields = preset.fields;
        if fields.expiry {
            self.expires = self.alert.expires.is_some();
            [self.expiry_date, self.expiry_time] = self.entry_fields(self.alert.expires);
        }
        if fields.schedule {
            let schedule = self.alert.schedule.clone();
            self.set_schedule_fields(schedule.as_deref());
        }
        self.editing_condition = 0;
        self.preset_name = name;
        self.preset_fields = fields;
    }

    /// Return to the alert as first shown, whatever presets or edits changed since.
    fn reset(&mut self) {
        if let Some(initial) = self.initial.take() {
            self.edit(initial);
        }
    }

    /// What asks for a shorter text before an edit saves. A stored alert may hold a longer one.
    fn text_too_long(&self) -> Option<&'static str> {
        if self.alert.name.chars().count() > Alert::MAX_NAME_CHARS {
            Some("The name is too long")
        } else if self.alert.message.chars().count() > Alert::MAX_MESSAGE_CHARS {
            Some("The message is too long")
        } else {
            None
        }
    }

    /// Poll preset replies and render the modal. Submit `Create` through the host alerts service.
    pub fn show(&mut self, ctx: &Context) -> AlertDialogAction {
        self.poll(ctx);
        if !self.is_open {
            return AlertDialogAction::None;
        }
        let mut action = AlertDialogAction::None;
        let title = if self.alert.id.is_some() {
            "Edit Alert"
        } else {
            "Create Alert"
        };
        let mut open = true;
        Modal::new("create_alert", "")
            .width(self.config.width + 32.0)
            .height(self.config.height + 130.0)
            .show(ctx, &mut open, |ui| {
                let translated_label = l10n::text(ui.ctx(), title);
                ui.response().widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Dialog, true, &translated_label)
                });
                ui.ctx()
                    .accesskit_node_builder(ui.response().id, |node| node.set_modal());
                if dialog_header(ui, title) {
                    self.close();
                    action = AlertDialogAction::Cancel;
                }
                ui.separator();
                let mut expiry = Ok(None);
                let mut start = Ok(None);
                let mut schedule = Ok(None);
                let mut valid = false;
                egui::ScrollArea::vertical()
                    .max_height(
                        (ui.available_height() - 65.0)
                            .min(self.config.height)
                            .max(0.0),
                    )
                    .show(ui, |ui| {
                        self.form(ui);
                        expiry = self.expiry(Utc::now());
                        start = self.start(expiry.ok().flatten());
                        schedule = self.schedule();
                        let too_long = self.text_too_long();
                        valid = too_long.is_none()
                            && self.alert.is_valid()
                            && expiry.is_ok()
                            && start.is_ok()
                            && schedule.is_ok();
                        if let Some(message) = too_long {
                            status_message(ui, MessageKind::Error, message);
                        } else if !self.alert.is_valid() {
                            let intervals = self.alert.timeframes();
                            let message = if !self.alert.delivery.plain_text_is_valid() {
                                "Enter one email address for plain text, or leave it empty"
                            } else if !self.alert.delivery.webhook_is_valid() {
                                "Enter the webhook URL, starting with http:// or https://"
                            } else if self.alert.additional_conditions.len() > 4 {
                                "An alert accepts five conditions"
                            } else if intervals.len() > 1
                                && intervals.iter().any(|interval| interval.is_trade_based())
                            {
                                "Mixed intervals need time-based bars"
                            } else if matches!(
                                self.alert.condition,
                                AlertCondition::Position { .. }
                            ) && (self.alert.watchlist.is_some()
                                || !self.alert.additional_conditions.is_empty())
                            {
                                "A position alert watches one symbol and takes no other conditions"
                            } else {
                                "Check the subject, levels and lookback"
                            };
                            status_message(ui, MessageKind::Error, message);
                        }
                        if let Err(error) = expiry {
                            status_message(ui, MessageKind::Error, error);
                        }
                        if let Err(error) = start {
                            status_message(ui, MessageKind::Error, error);
                        }
                        if let Err(error) = schedule {
                            status_message(ui, MessageKind::Error, error);
                        }
                        if let Some(error) = &self.error {
                            status_message(ui, MessageKind::Error, error);
                        }
                    });
                let (save, cancel) = DialogFooter::new("Save alert")
                    .secondary("Cancel")
                    .primary_enabled(valid)
                    .show(ui);
                if cancel {
                    self.close();
                    action = AlertDialogAction::Cancel;
                }
                if save {
                    self.alert.starts = start.unwrap();
                    self.alert.expires = expiry.unwrap();
                    self.alert.schedule = schedule.unwrap().map(Box::new);
                    self.remember_schedule();
                    action = AlertDialogAction::Create(Box::new(self.alert.clone()));
                    self.close();
                }
            });
        if !open {
            self.close();
            action = AlertDialogAction::Cancel;
        }
        action
    }

    fn form(&mut self, ui: &mut Ui) {
        if self.initial.is_none() {
            self.initial = Some(self.alert.clone());
        }
        let time_label = l10n::format_text(
            ui.ctx(),
            "Expiry time ({0})",
            &[("0", self.timezone.name().to_owned())],
        );
        let start_label = l10n::format_text(
            ui.ctx(),
            "Start time ({0})",
            &[("0", self.timezone.name().to_owned())],
        );
        let label_width = [
            l10n::text(ui.ctx(), "Expiry date"),
            time_label.clone(),
            start_label.clone(),
        ]
        .into_iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    label,
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                )
                .size()
                .x
        })
        .fold(0.0, f32::max);
        ui.spacing_mut().text_edit_width =
            (ui.available_width() - label_width - ui.spacing().item_spacing.x).max(0.0);
        FormGrid::new("alert_details").show(ui, |ui| {
            ui.label(l10n::text(ui.ctx(), "Name"));
            ui.add(
                egui::TextEdit::singleline(&mut self.alert.name).char_limit(Alert::MAX_NAME_CHARS),
            )
            .accessible_name(l10n::text(ui.ctx(), "Name"));
            ui.end_row();
            ui.label(l10n::text(ui.ctx(), "Symbol"));
            ui.text_edit_singleline(&mut self.alert.symbol)
                .accessible_name(l10n::text(ui.ctx(), "Symbol"));
            ui.end_row();
            ui.label(l10n::text(ui.ctx(), "Watchlist"));
            egui::ComboBox::from_id_salt("alert_watchlist")
                .width(ui.spacing().text_edit_width)
                .truncate()
                .selected_text(l10n::text(
                    ui.ctx(),
                    self.alert
                        .watchlist
                        .as_ref()
                        .and_then(|id| {
                            self.watchlists
                                .iter()
                                .find(|(key, _)| key == id)
                                .map(|(_, name)| name.as_str())
                        })
                        .unwrap_or(self.alert.watchlist.as_deref().unwrap_or("One symbol")),
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.alert.watchlist,
                        None,
                        l10n::text(ui.ctx(), "One symbol"),
                    );
                    for (id, name) in &self.watchlists {
                        ui.selectable_value(
                            &mut self.alert.watchlist,
                            Some(id.clone()),
                            l10n::text(ui.ctx(), name),
                        );
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Watchlist"));
            ui.end_row();
            ui.label(l10n::text(ui.ctx(), "Frequency"));
            egui::ComboBox::from_id_salt("alert_frequency")
                .selected_text(l10n::text(ui.ctx(), self.alert.frequency.to_string()))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (AlertFrequency::Once, "Only once"),
                        (AlertFrequency::EveryTime, "Every time"),
                        (AlertFrequency::OncePerBar, "Once per bar"),
                        (AlertFrequency::OncePerBarClose, "Once per bar close"),
                        (AlertFrequency::OncePerMinute, "Once per minute"),
                    ] {
                        if (self.alert.condition.is_event()
                            && !matches!(value, AlertFrequency::Once | AlertFrequency::EveryTime))
                            || (matches!(self.alert.condition, AlertCondition::Position { .. })
                                && value != AlertFrequency::Once)
                        {
                            continue;
                        }
                        ui.selectable_value(
                            &mut self.alert.frequency,
                            value,
                            l10n::text(ui.ctx(), label),
                        );
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Frequency"));
            ui.end_row();
            ui.scope(|ui| {
                ui.set_min_width(label_width);
                ui.label(l10n::text(ui.ctx(), "Start"));
            });
            let mut start_now = !self.starts;
            if ui
                .checkbox(&mut start_now, l10n::text(ui.ctx(), "Start when saved"))
                .changed()
            {
                self.starts = !start_now;
                if self.starts && self.start_date.is_empty() {
                    [self.start_date, self.start_time] = self.entry_fields(Some(Utc::now()));
                }
            }
            ui.end_row();
            if self.starts {
                ui.label(l10n::text(ui.ctx(), "Start date"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.start_date)
                        .hint_text(l10n::text(ui.ctx(), "YYYY-MM-DD")),
                )
                .accessible_name(l10n::text(ui.ctx(), "Start date"));
                ui.end_row();
                ui.label(&start_label);
                ui.add(
                    egui::TextEdit::singleline(&mut self.start_time)
                        .hint_text(l10n::text(ui.ctx(), "HH:MM")),
                )
                .accessible_name(start_label);
                ui.end_row();
            }
            // The widest expiry label's width keeps the fields in place when No expiry toggles.
            ui.scope(|ui| {
                ui.set_min_width(label_width);
                ui.label(l10n::text(ui.ctx(), "Expiry"));
            });
            let mut no_expiry = !self.expires;
            if ui
                .checkbox(&mut no_expiry, l10n::text(ui.ctx(), "No expiry"))
                .changed()
            {
                self.expires = !no_expiry;
                if self.expires && self.expiry_date.is_empty() {
                    // Start a month ahead, to the minute, so an expiry needs no typing.
                    let now = Utc::now();
                    [self.expiry_date, self.expiry_time] =
                        self.entry_fields(now.checked_add_months(Months::new(1)).or(Some(now)));
                }
            }
            ui.end_row();
            if self.expires {
                ui.label(l10n::text(ui.ctx(), "Expiry date"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.expiry_date)
                        .hint_text(l10n::text(ui.ctx(), "YYYY-MM-DD")),
                )
                .accessible_name(l10n::text(ui.ctx(), "Expiry date"));
                ui.end_row();
                ui.label(&time_label);
                ui.add(
                    egui::TextEdit::singleline(&mut self.expiry_time)
                        .hint_text(l10n::text(ui.ctx(), "HH:MM")),
                )
                .accessible_name(time_label);
                ui.end_row();
            }
            ui.label(l10n::text(ui.ctx(), "Notifications"));
            let mut any_time = !self.scheduled;
            if ui
                .checkbox(&mut any_time, l10n::text(ui.ctx(), "Any time"))
                .changed()
            {
                self.scheduled = !any_time;
            }
            ui.end_row();
            if self.scheduled {
                ui.label(l10n::text(ui.ctx(), "Time zone"));
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.schedule_zone).desired_width(160.0),
                    )
                    .accessible_name(l10n::text(ui.ctx(), "Time zone"));
                    if let Some(account) = &self.account_timezone
                        && ui
                            .selectable_label(
                                self.schedule_zone.trim() == account.name(),
                                l10n::text(ui.ctx(), "Account time zone"),
                            )
                            .clicked()
                    {
                        self.schedule_zone = account.name().into();
                    }
                });
                ui.end_row();
                ui.label(l10n::text(ui.ctx(), "Days"));
                ui.horizontal_wrapped(|ui| {
                    for (selected, day) in self.schedule_days.iter_mut().zip(DAYS) {
                        ui.toggle_value(selected, l10n::text(ui.ctx(), day));
                    }
                });
                ui.end_row();
                ui.label(l10n::text(ui.ctx(), "Windows"));
                ui.vertical(|ui| {
                    let mut remove = None;
                    for (index, [start, end]) in self.schedule_windows.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(start).desired_width(48.0))
                                .accessible_name(l10n::text(ui.ctx(), "Window start"));
                            ui.label("-");
                            ui.add(egui::TextEdit::singleline(end).desired_width(48.0))
                                .accessible_name(l10n::text(ui.ctx(), "Window end"));
                            if ui.button(l10n::text(ui.ctx(), "Remove")).clicked() {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        self.schedule_windows.remove(index);
                    }
                    if ui.button(l10n::text(ui.ctx(), "Add window")).clicked() {
                        self.schedule_windows.push(["09:00".into(), "17:00".into()]);
                    }
                });
                ui.end_row();
            }
            ui.label(l10n::text(ui.ctx(), "Delivery"));
            // Measure every wrapped line as part of the same grid cell.
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let delivery = &mut self.alert.delivery;
                    for (selected, label) in [
                        (&mut delivery.popup, "Popup"),
                        (&mut delivery.sound, "Sound"),
                        (&mut delivery.email, "Email"),
                        (&mut delivery.plain_text, "Plain-text email"),
                        (&mut delivery.push, "Mobile push"),
                        (&mut delivery.webhook, "Webhook"),
                    ] {
                        ui.checkbox(selected, l10n::text(ui.ctx(), label));
                    }
                });
            });
            ui.end_row();
            if self.alert.delivery.plain_text {
                ui.label(l10n::text(ui.ctx(), "Plain-text address"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.alert.delivery.plain_text_address)
                        .hint_text(l10n::text(ui.ctx(), "Account's alternative address")),
                )
                .accessible_name(l10n::text(ui.ctx(), "Plain-text address"));
                ui.end_row();
            }
            if self.alert.delivery.webhook {
                ui.label(l10n::text(ui.ctx(), "Webhook URL"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.alert.delivery.webhook_url)
                        .hint_text("https://"),
                )
                .accessible_name(l10n::text(ui.ctx(), "Webhook URL"));
                ui.end_row();
            }
            ui.label(l10n::text(ui.ctx(), "Message"));
            ui.add(
                egui::TextEdit::multiline(&mut self.alert.message)
                    .char_limit(Alert::MAX_MESSAGE_CHARS),
            )
            .accessible_name(l10n::text(ui.ctx(), "Message"));
            ui.end_row();
        });
        ui.separator();
        let events = self.alert.condition.is_event();
        if !events && !self.alert.additional_conditions.is_empty() {
            ui.label(l10n::text(
                ui.ctx(),
                "Every condition must be true together",
            ));
            for (index, (source, condition, interval)) in self.alert.rules().enumerate() {
                let preview = rule_preview(ui.ctx(), source, condition, interval, &self.sources);
                if ui
                    .add(
                        egui::Button::new(preview)
                            .selected(self.editing_condition == index)
                            .wrap(),
                    )
                    .accessible_name(l10n::format_text(
                        ui.ctx(),
                        "Edit condition {0}",
                        &[("0", (index + 1).to_string())],
                    ))
                    .clicked()
                {
                    self.editing_condition = index;
                }
            }
        }
        self.editing_condition = self
            .editing_condition
            .min(self.alert.additional_conditions.len());
        ui.push_id(("rule", self.editing_condition), |ui| {
            if self.editing_condition == 0 {
                if !events {
                    let source_interval = self.alert.timeframe;
                    interval_picker(ui, &mut self.alert.timeframe, "Chart interval");
                    if self.alert.timeframe != source_interval
                        && let Some(inputs) = &mut self.alert.inputs
                    {
                        // A captured span counts bars of the interval it was captured on.
                        inputs.start = None;
                        inputs.visible = None;
                    }
                }
                rule_form(
                    ui,
                    &mut self.alert.source,
                    &mut self.alert.condition,
                    &self.sources,
                    &self.strategy_runs,
                    true,
                    None,
                );
            } else {
                let index = self.editing_condition - 1;
                self.alert
                    .additional_timeframes
                    .resize(self.alert.additional_conditions.len(), None);
                interval_picker(
                    ui,
                    &mut self.alert.additional_timeframes[index],
                    "Follow first interval",
                );
                let rule = &mut self.alert.additional_conditions[index];
                rule_form(
                    ui,
                    &mut rule.source,
                    &mut rule.condition,
                    &self.sources,
                    &[],
                    false,
                    None,
                );
                if ui
                    .button(l10n::text(ui.ctx(), "Remove condition"))
                    .clicked()
                {
                    self.alert.additional_conditions.remove(index);
                    self.alert.additional_timeframes.remove(index);
                }
            }
        });
        let events = self.alert.condition.is_event();
        if events {
            self.alert.additional_conditions.clear();
            self.alert.additional_timeframes.clear();
            if !matches!(
                self.alert.frequency,
                AlertFrequency::Once | AlertFrequency::EveryTime
            ) {
                self.alert.frequency = AlertFrequency::EveryTime;
            }
        }
        let position = matches!(self.alert.condition, AlertCondition::Position { .. });
        if position {
            // A position alert ends at its exit, so it takes frequency Once.
            self.alert.frequency = AlertFrequency::Once;
        }
        if !events
            && !position
            && ui
                .add_enabled(
                    self.alert.additional_conditions.len() < 4,
                    egui::Button::new(l10n::text(ui.ctx(), "Add condition")),
                )
                .on_disabled_hover_text(l10n::text(ui.ctx(), "An alert accepts five conditions"))
                .clicked()
        {
            self.alert
                .additional_timeframes
                .resize(self.alert.additional_conditions.len(), None);
            self.alert.additional_timeframes.push(self.alert.timeframe);
            self.alert.additional_conditions.push(AlertRule {
                source: self.alert.source.clone(),
                condition: self.alert.condition.clone(),
            });
            self.editing_condition = self.alert.additional_conditions.len();
        }
        while self.alert.additional_timeframes.last() == Some(&None) {
            self.alert.additional_timeframes.pop();
        }
        if self.storage.is_some() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(l10n::text(ui.ctx(), "Preset"));
                ui.text_edit_singleline(&mut self.preset_name)
                    .accessible_name(l10n::text(ui.ctx(), "Preset"));
                let now = Utc::now();
                let (expiry, schedule) = (self.expiry(now), self.schedule());
                let stored = self.presets.contains_key(self.preset_name.trim());
                let ready = self.loaded && !self.pending && !self.revisions.unsettled();
                let fields = self.preset_fields;
                if ui
                    .add_enabled(
                        ready
                            && self.alert.is_valid()
                            && self.text_too_long().is_none()
                            && !self.preset_name.trim().is_empty()
                            && fields.any()
                            && !self.preset_limit_reached()
                            && (!fields.expiry || expiry.is_ok())
                            && (!fields.schedule || schedule.is_ok()),
                        egui::Button::new(l10n::text(
                            ui.ctx(),
                            if stored {
                                "Replace preset"
                            } else {
                                "Save preset"
                            },
                        )),
                    )
                    .clicked()
                {
                    if let Ok(expiry) = expiry {
                        self.alert.expires = expiry;
                    }
                    if let Ok(schedule) = schedule {
                        self.alert.schedule = schedule.map(Box::new);
                    }
                    self.save_preset(now);
                }
                if ui
                    .add_enabled(
                        ready && stored,
                        egui::Button::new(l10n::text(ui.ctx(), "Delete preset")),
                    )
                    .clicked()
                {
                    self.delete_preset();
                }
                if ui.button(l10n::text(ui.ctx(), "Reset changes")).clicked() {
                    self.reset();
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(l10n::text(ui.ctx(), "Keep in preset"));
                let fields = &mut self.preset_fields;
                for (selected, label, name) in [
                    (&mut fields.condition, "Condition", "Keep condition"),
                    (&mut fields.interval, "Interval", "Keep interval"),
                    (&mut fields.frequency, "Frequency", "Keep frequency"),
                    (&mut fields.expiry, "Expiry", "Keep expiry"),
                    (
                        &mut fields.text,
                        "Name and message",
                        "Keep name and message",
                    ),
                    (&mut fields.delivery, "Delivery", "Keep delivery"),
                    (&mut fields.schedule, "Schedule", "Keep schedule"),
                ] {
                    ui.checkbox(selected, l10n::text(ui.ctx(), label))
                        .accessible_name(l10n::text(ui.ctx(), name));
                }
            });
            ui.label(
                egui::RichText::new(l10n::text(
                    ui.ctx(),
                    "Presets leave out levels, drawings, webhook URLs and plain-text addresses.",
                ))
                .weak(),
            );
            if !self.preset_name.trim().is_empty() && self.preset_limit_reached() {
                status_message(
                    ui,
                    MessageKind::Warning,
                    "You can keep five presets. Replace or delete one to save another.",
                );
            }
            let mut apply = None;
            egui::ComboBox::from_id_salt("alert_presets")
                .selected_text(l10n::text(ui.ctx(), "Apply preset"))
                .show_ui(ui, |ui| {
                    for name in self.presets.keys() {
                        if ui
                            .selectable_label(false, l10n::text(ui.ctx(), name))
                            .clicked()
                        {
                            apply = Some(name.clone());
                        }
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Apply preset"));
            if let Some(name) = apply {
                self.apply_preset(name, Utc::now());
            }
        }
    }
}

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn interval_picker(ui: &mut Ui, interval: &mut Option<Timeframe>, inherited: &str) {
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(l10n::text(ui.ctx(), "Interval"));
        egui::ComboBox::from_id_salt("interval")
            .selected_text(
                interval.map_or_else(|| l10n::text(ui.ctx(), inherited), |tf| tf.to_string()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(interval, None, l10n::text(ui.ctx(), inherited));
                for value in Timeframe::all() {
                    ui.selectable_value(interval, Some(value), value.to_string());
                }
            })
            .response
            .labelled_by(label.id);
    });
}

fn rule_preview(
    ctx: &egui::Context,
    source: &AlertSource,
    condition: &AlertCondition,
    interval: Timeframe,
    sources: &[(AlertSource, AlertSourceName)],
) -> String {
    let name = |source: &AlertSource| {
        sources
            .iter()
            .find(|(value, _)| value == source)
            .map(|(_, label)| label.text(ctx))
            .unwrap_or_else(|| {
                l10n::text(
                    ctx,
                    match source {
                        AlertSource::Price => "Price".into(),
                        AlertSource::Volume => "Volume".into(),
                        _ => format!("{source:?}"),
                    },
                )
            })
    };
    let others: Vec<_> = condition
        .partners()
        .into_iter()
        .flatten()
        .map(name)
        .collect();
    format!(
        "{} · {}{} · {interval}",
        name(source),
        l10n::text(ctx, condition.to_string()),
        if others.is_empty() {
            String::new()
        } else {
            format!(" ({})", others.join(", "))
        }
    )
}

fn source_picker(
    ui: &mut Ui,
    id: &str,
    source: &mut AlertSource,
    sources: &[(AlertSource, AlertSourceName)],
) {
    let label = sources
        .iter()
        .find(|(value, _)| value == source)
        .map_or_else(
            || l10n::text(ui.ctx(), format!("{source:?}")),
            |(_, name)| name.text(ui.ctx()),
        );
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width().min(200.0))
        .truncate()
        .selected_text(label)
        .show_ui(ui, |ui| {
            if !id.starts_with("boundary_") {
                ui.selectable_value(source, AlertSource::Price, l10n::text(ui.ctx(), "Price"));
                ui.selectable_value(source, AlertSource::Volume, l10n::text(ui.ctx(), "Volume"));
            }
            for (value, name) in sources {
                if id != "source" && matches!(value, AlertSource::StudyCondition { .. }) {
                    continue;
                }
                ui.selectable_value(source, value.clone(), name.text(ui.ctx()));
            }
        })
        .response
        .accessible_name(l10n::text(
            ui.ctx(),
            match id {
                "other" => "Comparison source",
                "boundary_a" => "First boundary",
                "boundary_b" => "Second boundary",
                _ => "Source",
            },
        ));
}

fn drawing_id(source: &AlertSource) -> Option<&str> {
    match source {
        AlertSource::Drawing(id) => Some(id.split('#').next().unwrap_or(id)),
        _ => None,
    }
}

/// Seed a channel from the selected drawing, retaining an existing pair verbatim.
fn drawing_channel(
    source: &AlertSource,
    condition: &AlertCondition,
    sources: &[(AlertSource, AlertSourceName)],
) -> Option<AlertCondition> {
    if let AlertCondition::Channel { .. } = condition {
        return Some(condition.clone());
    }
    let selected = match condition {
        AlertCondition::Compare { other, .. } if drawing_id(other).is_some() => Some(other),
        _ if drawing_id(source).is_some() => Some(source),
        _ => sources
            .iter()
            .find(|(s, _)| {
                drawing_id(s).is_some()
                    && sources
                        .iter()
                        .filter(|(other, _)| drawing_id(other) == drawing_id(s))
                        .count()
                        >= 2
            })
            .map(|(s, _)| s),
    }?;
    let mut levels = sources
        .iter()
        .filter(|(s, _)| drawing_id(s) == drawing_id(selected))
        .rev();
    let b = levels.next()?.0.clone();
    let a = levels.next()?.0.clone();
    Some(AlertCondition::Channel {
        boundaries: Box::new([a, b]),
        comparison: AlertChannelComparison::Entering,
    })
}

/// Seed a position alert from the position drawing the rule names, or the first one offered,
/// retaining an existing one verbatim.
fn drawing_position(
    source: &AlertSource,
    condition: &AlertCondition,
    sources: &[(AlertSource, AlertSourceName)],
) -> Option<AlertCondition> {
    use crate::drawings::DrawingToolType as T;
    if let AlertCondition::Position { .. } = condition {
        return Some(condition.clone());
    }
    let positions: Vec<&AlertSource> = sources
        .iter()
        .filter(|(_, name)| {
            name.label("tool")
                .is_some_and(|tool| tool == T::LongPos.as_str() || tool == T::ShortPos.as_str())
        })
        .map(|(source, _)| source)
        .collect();
    let named = match condition {
        AlertCondition::Compare { other, .. } => drawing_id(other),
        _ => None,
    }
    .or(drawing_id(source))
    .filter(|id| positions.iter().any(|s| drawing_id(s) == Some(*id)));
    let id = named.or_else(|| positions.first().and_then(|s| drawing_id(s)))?;
    let levels: Vec<AlertSource> = positions
        .into_iter()
        .filter(|s| drawing_id(s) == Some(id))
        .cloned()
        .collect();
    Some(AlertCondition::Position {
        levels: Box::new(levels.try_into().ok()?),
        progress: AlertPositionProgress::AwaitingEntry,
    })
}

/// Returns whether a single threshold was entered through the optional text editor or menu.
pub(crate) fn rule_form(
    ui: &mut Ui,
    source: &mut AlertSource,
    condition: &mut AlertCondition,
    sources: &[(AlertSource, AlertSourceName)],
    runs: &[(String, String)],
    offer_position: bool,
    mut level_text: Option<&mut String>,
) -> bool {
    let mut level_edited = false;
    ui.add_enabled_ui(
        !matches!(condition, AlertCondition::VolumeAboveAverage { .. }) && !condition.is_event(),
        |ui| source_picker(ui, "source", source, sources),
    );
    if !condition.pairs_with(source) {
        *condition = if matches!(source, AlertSource::StudyCondition { .. }) {
            AlertCondition::Holds
        } else {
            AlertCondition::Crossing(0.0)
        };
    }
    // A channel keeps its bounds and a level keeps its value across operators.
    let (lower, upper) = preset::seeded_bounds(condition).unwrap_or((0.0, 1.0));
    let channel = drawing_channel(source, condition, sources);
    let position = offer_position
        .then(|| drawing_position(source, condition, sources))
        .flatten();
    let choices = [
        (AlertCondition::Crossing(lower), "Crossing"),
        (AlertCondition::CrossingUp(lower), "Crossing up"),
        (AlertCondition::CrossingDown(lower), "Crossing down"),
        (AlertCondition::Above(lower), "Greater than"),
        (AlertCondition::Below(lower), "Less than"),
        (
            AlertCondition::EnteringChannel { lower, upper },
            "Entering channel",
        ),
        (
            AlertCondition::ExitingChannel { lower, upper },
            "Exiting channel",
        ),
        (
            AlertCondition::InsideChannel { lower, upper },
            "Inside channel",
        ),
        (
            AlertCondition::OutsideChannel { lower, upper },
            "Outside channel",
        ),
        (
            AlertCondition::MovingBy {
                value: 1.0,
                bars: 1,
            },
            "Moving by value",
        ),
        (
            AlertCondition::MovingByPercent {
                percent: 1.0,
                bars: 1,
            },
            "Moving by percent",
        ),
        (
            AlertCondition::Compare {
                other: AlertSource::Price,
                comparison: AlertComparison::Crossing,
            },
            "Compare outputs",
        ),
        (
            AlertCondition::VolumeAboveAverage {
                multiplier: 2.0,
                bars: 20,
            },
            "Volume above average",
        ),
    ]
    .into_iter()
    .chain(
        sources
            .iter()
            .any(|(source, _)| matches!(source, AlertSource::StudyCondition { .. }))
            .then_some((AlertCondition::Holds, "Study condition")),
    )
    .chain(channel.map(|c| (c, "Drawing channel")))
    .chain(position.map(|c| (c, "Position entry and exit")))
    .chain(
        runs.first()
            .map(|(id, _)| (AlertCondition::StrategyFills(id.clone()), "Strategy fills")),
    )
    .collect::<Vec<_>>();
    let selected = choices
        .iter()
        .find(|(c, _)| std::mem::discriminant(c) == std::mem::discriminant(condition))
        .map(|(_, name)| *name)
        .unwrap_or(match condition {
            AlertCondition::StrategyFills(_) => "Strategy fills",
            AlertCondition::DetectorSignals(_) => "Detector signals",
            AlertCondition::CustomEvents(_) => "Custom events",
            _ => "Condition",
        });
    egui::ComboBox::from_id_salt("condition")
        .selected_text(l10n::text(ui.ctx(), selected))
        .show_ui(ui, |ui| {
            for (value, label) in choices {
                if ui
                    .selectable_label(label == selected, l10n::text(ui.ctx(), label))
                    .clicked()
                    && label != selected
                {
                    if matches!(value, AlertCondition::VolumeAboveAverage { .. }) {
                        *source = AlertSource::Volume;
                    } else if matches!(value, AlertCondition::Holds) {
                        if !matches!(source, AlertSource::StudyCondition { .. })
                            && let Some((first, _)) = sources
                                .iter()
                                .find(|(s, _)| matches!(s, AlertSource::StudyCondition { .. }))
                        {
                            *source = first.clone();
                        }
                    } else if value.is_event()
                        || matches!(source, AlertSource::StudyCondition { .. })
                    {
                        *source = AlertSource::Price;
                    }
                    if matches!(
                        condition,
                        AlertCondition::EnteringChannel { .. }
                            | AlertCondition::ExitingChannel { .. }
                            | AlertCondition::InsideChannel { .. }
                            | AlertCondition::OutsideChannel { .. }
                    ) && let Some(level) = value.level()
                        && level.is_finite()
                        && let Some(text) = level_text.as_deref_mut()
                    {
                        *text = level.to_string();
                        level_edited = true;
                    }
                    *condition = value;
                }
            }
        })
        .response
        .accessible_name(l10n::text(ui.ctx(), "Condition"));
    ui.horizontal_wrapped(|ui| match condition {
        AlertCondition::Holds => {}
        AlertCondition::Position { progress, .. } => {
            ui.label(l10n::text(ui.ctx(), progress.to_string()));
        }
        AlertCondition::StrategyFills(run) => {
            let name = runs
                .iter()
                .find(|(id, _)| id == run)
                .map_or(run.as_str(), |(_, name)| name.as_str());
            egui::ComboBox::from_id_salt("strategy_run")
                .width(ui.available_width().min(200.0))
                .truncate()
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for (id, name) in runs {
                        ui.selectable_value(run, id.clone(), name);
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Strategy run"));
        }
        AlertCondition::DetectorSignals(detector) => {
            ui.text_edit_singleline(detector)
                .accessible_name(l10n::text(ui.ctx(), "Detector"));
        }
        AlertCondition::CustomEvents(source) => {
            ui.text_edit_singleline(source)
                .accessible_name(l10n::text(ui.ctx(), "Event source"));
        }
        AlertCondition::Crossing(v)
        | AlertCondition::CrossingUp(v)
        | AlertCondition::CrossingDown(v)
        | AlertCondition::Above(v)
        | AlertCondition::Below(v) => {
            ui.label(l10n::text(ui.ctx(), "Level"));
            if let Some(text) = level_text {
                let mut field = egui::TextEdit::singleline(text)
                    .desired_width(0.0)
                    .clip_text(false)
                    .min_size(ui.spacing().interact_size)
                    .show(ui);
                if field.response.gained_focus() {
                    field
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::select_all(&field.galley)));
                    field.state.store(ui.ctx(), field.response.id);
                }
                level_edited |= field
                    .response
                    .response
                    .accessible_name(l10n::text(ui.ctx(), "Level"))
                    .changed();
                let number: String = text.split_whitespace().collect();
                *v = number.replace('−', "-").parse().unwrap_or(f64::NAN);
            } else {
                ui.add(egui::DragValue::new(v).speed(0.01))
                    .accessible_name(l10n::text(ui.ctx(), "Level"));
            }
        }
        AlertCondition::EnteringChannel { lower, upper }
        | AlertCondition::ExitingChannel { lower, upper }
        | AlertCondition::InsideChannel { lower, upper }
        | AlertCondition::OutsideChannel { lower, upper } => {
            ui.label(l10n::text(ui.ctx(), "Lower / upper"));
            ui.add(egui::DragValue::new(lower))
                .accessible_name(l10n::text(ui.ctx(), "Lower"));
            ui.add(egui::DragValue::new(upper))
                .accessible_name(l10n::text(ui.ctx(), "Upper"));
        }
        AlertCondition::MovingBy { value, bars }
        | AlertCondition::MovingByPercent {
            percent: value,
            bars,
        } => {
            ui.label(l10n::text(ui.ctx(), "Change / bars"));
            ui.add(egui::DragValue::new(value))
                .accessible_name(l10n::text(ui.ctx(), "Change"));
            ui.add(egui::DragValue::new(bars).range(1..=u32::MAX))
                .accessible_name(l10n::text(ui.ctx(), "Bars"));
        }
        AlertCondition::VolumeAboveAverage { multiplier, bars } => {
            ui.label(l10n::text(ui.ctx(), "Multiplier / preceding bars"));
            ui.add(egui::DragValue::new(multiplier).speed(0.1))
                .accessible_name(l10n::text(ui.ctx(), "Multiplier"));
            ui.add(egui::DragValue::new(bars).range(1..=u32::MAX))
                .accessible_name(l10n::text(ui.ctx(), "Bars"));
        }
        AlertCondition::Channel {
            boundaries,
            comparison,
        } => {
            let drawings: Vec<_> = sources
                .iter()
                .filter(|(s, _)| drawing_id(s).is_some())
                .cloned()
                .collect();
            ui.label(l10n::text(ui.ctx(), "Drawing boundaries"));
            source_picker(ui, "boundary_a", &mut boundaries[0], &drawings);
            source_picker(ui, "boundary_b", &mut boundaries[1], &drawings);
            egui::ComboBox::from_id_salt("channel_comparison")
                .selected_text(l10n::text(ui.ctx(), comparison.to_string()))
                .show_ui(ui, |ui| {
                    for value in [
                        AlertChannelComparison::Entering,
                        AlertChannelComparison::Exiting,
                        AlertChannelComparison::Inside,
                        AlertChannelComparison::Outside,
                    ] {
                        ui.selectable_value(
                            comparison,
                            value,
                            l10n::text(ui.ctx(), value.to_string()),
                        );
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Channel condition"));
        }
        AlertCondition::Compare { other, comparison } => {
            source_picker(ui, "other", other, sources);
            egui::ComboBox::from_id_salt("comparison")
                .selected_text(l10n::text(ui.ctx(), comparison.to_string()))
                .show_ui(ui, |ui| {
                    for value in [
                        AlertComparison::Crossing,
                        AlertComparison::CrossingUp,
                        AlertComparison::CrossingDown,
                        AlertComparison::Above,
                        AlertComparison::Below,
                    ] {
                        ui.selectable_value(
                            comparison,
                            value,
                            l10n::text(ui.ctx(), value.to_string()),
                        );
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Comparison"));
        }
    });
    level_edited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{FeedError, Reply};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Store(Mutex<Option<String>>);
    impl Storage for Store {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }
        fn load(&self, _: StoreKind, _: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().clone()));
        }
        fn save(&self, _: StoreKind, _: &str, doc: String, reply: Reply<()>) {
            *self.0.lock().unwrap() = Some(doc);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    struct Refusing;
    impl Storage for Refusing {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }
        fn load(&self, _: StoreKind, _: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(None));
        }
        fn save(&self, _: StoreKind, _: &str, _: String, reply: Reply<()>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    #[test]
    fn condition_previews_edit_intervals_limit_and_restore_the_host_request() {
        use crate::model::{AlertInputs, Timeframe};
        use egui_kittest::{Harness, kittest::Queryable};
        let mut dialog = AlertDialog::new();
        dialog.edit(Alert::new("SAMPLE", AlertCondition::Above(100.0)));
        dialog.alert.timeframe = Some(Timeframe::Min5);
        let seed = chrono::DateTime::from_timestamp(0, 0);
        dialog.alert.inputs = Some(Box::new(AlertInputs {
            start: seed,
            visible: seed.map(|time| (time, time)),
            ..Default::default()
        }));
        let mut h = Harness::builder()
            .with_size(egui::vec2(560.0, 900.0))
            .build_ui_state(|ui, dialog: &mut AlertDialog| dialog.form(ui), dialog);
        h.run();
        assert!(h.query_by_label("Edit condition 1").is_none());
        for _ in 0..4 {
            h.get_by_label("Add condition").click();
            h.run();
        }
        assert_eq!(h.state().alert.additional_conditions.len(), 4);
        h.get_by_label("Add condition").click();
        h.run();
        assert_eq!(
            h.state().alert.additional_conditions.len(),
            4,
            "five includes the first condition"
        );
        h.get_by_label("Edit condition 2").click();
        h.run();
        h.get_by_label("Interval").click();
        h.run();
        h.get_by_label("1h").scroll_to_me();
        h.run();
        h.get_by_label("1h").click();
        h.run();
        assert_eq!(
            serde_json::to_value(&h.state().alert).unwrap()["additional_timeframes"][0],
            serde_json::json!("Hour1")
        );
        let saved = serde_json::to_string(&h.state().alert).unwrap();
        h.state_mut().edit(serde_json::from_str(&saved).unwrap());
        h.run();
        h.get_by_label("Edit condition 2").click();
        h.run();
        assert_eq!(
            serde_json::to_value(&h.state().alert).unwrap()["additional_timeframes"][0],
            serde_json::json!("Hour1")
        );
        assert_eq!(
            h.state().alert.inputs.as_ref().unwrap().start,
            seed,
            "an additional interval leaves the source span"
        );
        h.get_by_label("Remove condition").click();
        h.run();
        assert_eq!(
            serde_json::to_value(&h.state().alert).unwrap()["additional_timeframes"],
            serde_json::json!(["Min5", "Min5", "Min5"])
        );
        h.get_by_label("Edit condition 1").click();
        h.run();
        h.get_by_label("Interval").click();
        h.run();
        h.get_by_label("1min").scroll_to_me();
        h.run();
        h.get_by_label("1min").click();
        h.run();
        assert!(h.state().alert.is_valid());
        assert_eq!(h.state().alert.timeframe, Some(Timeframe::Min1));
        assert_eq!(
            serde_json::to_value(&h.state().alert).unwrap()["additional_timeframes"],
            serde_json::json!(["Min5", "Min5", "Min5"])
        );
        // A span captured on the source chart's interval does not describe another interval.
        let inputs = h.state().alert.inputs.as_ref().unwrap();
        assert_eq!((inputs.start, inputs.visible), (None, None));
    }

    /// The limits count characters, not bytes, and apply where the user edits the text. A
    /// plain-text email address must be one address, and the saved alert carries it.
    #[test]
    fn save_stops_at_text_limits_and_invalid_plain_text_addresses() {
        use egui::accesskit::Role;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        type Edit = fn(&mut Alert);
        const ADDRESS: &str = "Enter one email address for plain text, or leave it empty";
        // `None` saves; otherwise Save is off and the dialog says why.
        let rows: [(Edit, Option<&str>); 7] = [
            (|a| a.name = "é".repeat(Alert::MAX_NAME_CHARS), None),
            (
                |a| a.name = "é".repeat(Alert::MAX_NAME_CHARS + 1),
                Some("The name is too long"),
            ),
            (|a| a.message = "€".repeat(Alert::MAX_MESSAGE_CHARS), None),
            (
                |a| a.message = "€".repeat(Alert::MAX_MESSAGE_CHARS + 1),
                Some("The message is too long"),
            ),
            (
                |a| {
                    a.delivery.plain_text = true;
                    a.delivery.plain_text_address = "pager@example.com".into();
                },
                None,
            ),
            (
                |a| {
                    a.delivery.plain_text = true;
                    a.delivery.plain_text_address = "pager@example".into();
                },
                Some(ADDRESS),
            ),
            // Without the plain-text email the address is not used, so it is not checked.
            (
                |a| a.delivery.plain_text_address = "pager@example".into(),
                None,
            ),
        ];
        for (index, (edit, error)) in rows.into_iter().enumerate() {
            let mut edited = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
            edit(&mut edited);
            let mut dialog = AlertDialog::new();
            dialog.edit(edited.clone());
            let mut h = Harness::builder()
                .allow_missing_glyphs()
                .with_context_setup(|ctx| {
                    crate::theme::apply_to_egui(ctx, &crate::theme::Theme::light());
                })
                .with_size(egui::vec2(1440.0, 900.0))
                .build_ui_state(
                    |ui, (dialog, saved): &mut (AlertDialog, Option<Alert>)| {
                        if let AlertDialogAction::Create(alert) = dialog.show(ui.ctx()) {
                            *saved = Some(*alert);
                        }
                    },
                    (dialog, None),
                );
            h.run();
            if edited.delivery.plain_text {
                let address = h
                    .get_by_role_and_label(Role::TextInput, "Plain-text address")
                    .rect();
                for label in [
                    "Popup",
                    "Sound",
                    "Email",
                    "Plain-text email",
                    "Mobile push",
                    "Webhook",
                ] {
                    assert!(
                        h.get_by_label(label).rect().bottom() <= address.top(),
                        "{label} overlaps the plain-text address in row {index}"
                    );
                }
            }
            assert_eq!(
                h.get_by_label("Save alert").accesskit_node().is_disabled(),
                error.is_some(),
                "row {index}"
            );
            if let Some(error) = error {
                assert!(h.query_by_label(error).is_some(), "row {index}");
                continue;
            }
            h.get_by_label("Save alert").click();
            h.run();
            assert_eq!(
                h.state()
                    .1
                    .as_ref()
                    .map(|alert| (&alert.name, &alert.message, &alert.delivery)),
                Some((&edited.name, &edited.message, &edited.delivery)),
                "row {index}"
            );
        }
    }

    #[test]
    fn drawing_channel_keeps_selected_boundaries_through_edits() {
        use egui_kittest::{Harness, kittest::Queryable};
        for dark in [false, true] {
            for width in [320.0, 960.0] {
                let drawing = |id: &str| AlertSource::Drawing(id.into());
                let sources = vec![
                    (drawing("a"), "Other line".into()),
                    (drawing("a#1"), "Other boundary".into()),
                    (drawing("b%23saved"), "Median".into()),
                    (drawing("b%23saved#1"), "Upper".into()),
                    (drawing("b%23saved#2"), "Lower".into()),
                ];
                let single = AlertCondition::Compare {
                    other: drawing("b%23saved"),
                    comparison: AlertComparison::Crossing,
                };
                let mut dialog = AlertDialog::new();
                dialog.sources = sources;
                dialog.edit(Alert::new("SAMPLE", single));
                let mut h = Harness::builder()
                    .with_size(egui::vec2(width, 900.0))
                    .build_ui_state(
                        |ui, dialog: &mut AlertDialog| {
                            ui.ctx().set_visuals(if dark {
                                egui::Visuals::dark()
                            } else {
                                egui::Visuals::light()
                            });
                            dialog.form(ui)
                        },
                        dialog,
                    );
                h.run();
                h.get_by_label("Condition").click();
                h.run();
                h.get_by_label("Drawing channel").scroll_to_me();
                h.run();
                h.get_by_label("Drawing channel").click();
                h.run();
                let expected: AlertCondition = serde_json::from_value(serde_json::json!({"Channel": {
                    "boundaries": [{"Drawing":"b%23saved#1"},{"Drawing":"b%23saved#2"}], "comparison":"Entering"
                }})).unwrap();
                assert_eq!(h.state().alert.condition, expected);
                let saved = serde_json::to_string(&h.state().alert).unwrap();
                h.state_mut().edit(serde_json::from_str(&saved).unwrap());
                h.run();
                assert_eq!(h.state().alert.condition, expected);
                for label in ["First boundary", "Second boundary", "Channel condition"] {
                    let rect = h.get_by_label(label).rect();
                    assert!(
                        rect.left() >= 0.0 && rect.right() <= width,
                        "{label}: {rect:?} at {width}"
                    );
                }
            }
        }
    }

    /// The condition menu keeps a channel's bounds across channel operators, gives a single
    /// level the lower bound and seeds a channel from a level.
    #[test]
    fn condition_menu_keeps_channel_bounds() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut dialog = AlertDialog::new();
        dialog.edit(Alert::new(
            "SAMPLE",
            AlertCondition::EnteringChannel {
                lower: 100.0,
                upper: 110.0,
            },
        ));
        let mut h = Harness::builder()
            .with_size(egui::vec2(960.0, 900.0))
            .build_ui_state(|ui, dialog: &mut AlertDialog| dialog.form(ui), dialog);
        h.run();
        let (lower, upper) = (100.0, 110.0);
        let steps = [
            (
                "Exiting channel",
                AlertCondition::ExitingChannel { lower, upper },
            ),
            (
                "Inside channel",
                AlertCondition::InsideChannel { lower, upper },
            ),
            (
                "Outside channel",
                AlertCondition::OutsideChannel { lower, upper },
            ),
            (
                "Entering channel",
                AlertCondition::EnteringChannel { lower, upper },
            ),
            ("Crossing", AlertCondition::Crossing(100.0)),
            (
                "Entering channel",
                AlertCondition::EnteringChannel {
                    lower: 100.0,
                    upper: 101.0,
                },
            ),
        ];
        for (label, expected) in steps {
            h.get_by_label("Condition").click();
            h.run();
            h.get_by_label(label).scroll_to_me();
            h.run();
            h.get_by_label(label).click();
            h.run();
            assert_eq!(h.state().alert.condition, expected, "{label}");
        }
    }

    #[test]
    fn a_position_level_offers_the_alert_from_entry_to_exit() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut dialog = AlertDialog::new();
        for (id, tool) in [("7", "Long Position"), ("9", "Short Position")] {
            for (suffix, level) in [("", "Entry"), ("#1", "Target"), ("#2", "Stop")] {
                dialog.sources.push((
                    AlertSource::Drawing(format!("{id}{suffix}")),
                    AlertSourceName::new(
                        "{tool} {id}: {level}",
                        &[("id", id)],
                        &[("tool", tool), ("level", level)],
                    ),
                ));
            }
        }
        let mut alert = Alert::new(
            "SAMPLE",
            AlertCondition::Compare {
                other: AlertSource::Drawing("9#2".into()),
                comparison: AlertComparison::Crossing,
            },
        );
        alert.frequency = AlertFrequency::EveryTime;
        dialog.edit(alert);
        let mut h = Harness::builder()
            .with_size(egui::vec2(960.0, 900.0))
            .build_ui_state(|ui, dialog: &mut AlertDialog| dialog.form(ui), dialog);
        h.run();
        h.get_by_label("Condition").click();
        h.run();
        h.get_by_label("Position entry and exit").scroll_to_me();
        h.run();
        h.get_by_label("Position entry and exit").click();
        h.run();
        h.run();
        assert_eq!(
            h.state().alert.condition,
            AlertCondition::Position {
                levels: Box::new(["9", "9#1", "9#2"].map(|id| AlertSource::Drawing(id.into()))),
                progress: AlertPositionProgress::AwaitingEntry,
            }
        );
        assert_eq!(h.state().alert.frequency, AlertFrequency::Once);
        assert!(h.query_by_label("Add condition").is_none());
    }

    #[test]
    fn study_condition_sources_pair_with_holds_and_price_restores_crossing() {
        use egui_kittest::{Harness, kittest::Queryable};
        let first = AlertSource::StudyCondition {
            id: "3".into(),
            condition: 0,
        };
        let change = AlertSource::StudyCondition {
            id: "3".into(),
            condition: 2,
        };
        let mut dialog = AlertDialog::new();
        dialog.sources = vec![
            (
                AlertSource::Study {
                    id: "3".into(),
                    output: 0,
                },
                "SuperTrend: Trend".into(),
            ),
            (first.clone(), "SuperTrend: Trend turns up".into()),
            (change.clone(), "SuperTrend: Trend change".into()),
        ];
        dialog.edit(Alert::new("SAMPLE", AlertCondition::Crossing(1.0)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(960.0, 900.0))
            .build_ui_state(|ui, dialog: &mut AlertDialog| dialog.form(ui), dialog);
        h.run();
        h.get_by_label("Source").click();
        h.run();
        h.get_by_label("SuperTrend: Trend change").click();
        h.run();
        assert_eq!(h.state().alert.source, change);
        assert_eq!(h.state().alert.condition, AlertCondition::Holds);
        assert!(h.state().alert.is_valid());
        h.get_by_label("Source").click();
        h.run();
        h.get_by_label("Price").click();
        h.run();
        assert_eq!(h.state().alert.source, AlertSource::Price);
        assert_eq!(h.state().alert.condition, AlertCondition::Crossing(0.0));
        h.get_by_label("Condition").click();
        h.run();
        h.get_by_label("Study condition").scroll_to_me();
        h.run();
        h.get_by_label("Study condition").click();
        h.run();
        assert_eq!(h.state().alert.source, first);
        assert_eq!(h.state().alert.condition, AlertCondition::Holds);
        assert!(h.state().alert.is_valid());
    }

    #[test]
    fn strategy_fills_offer_the_host_runs_by_name() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut dialog = AlertDialog::new();
        dialog.strategy_runs = vec![
            ("run-7".into(), "Momentum live".into()),
            ("run-8".into(), "Breakout live".into()),
        ];
        let mut alert = Alert::new("SAMPLE", AlertCondition::Above(1.0));
        alert.source = AlertSource::Volume;
        alert.frequency = AlertFrequency::OncePerBar;
        alert.additional_conditions.push(AlertRule::new(
            AlertSource::Price,
            AlertCondition::Above(2.0),
        ));
        alert.additional_timeframes.push(Some(Timeframe::Min5));
        dialog.edit(alert);
        let mut h = Harness::builder()
            .with_size(egui::vec2(960.0, 900.0))
            .build_ui_state(|ui, dialog: &mut AlertDialog| dialog.form(ui), dialog);
        h.run();
        h.get_by_label("Condition").click();
        h.run();
        h.get_by_label("Strategy fills").scroll_to_me();
        h.run();
        h.get_by_label("Strategy fills").click();
        h.run();
        assert_eq!(
            h.state().alert.condition,
            AlertCondition::StrategyFills("run-7".into())
        );
        assert_eq!(h.state().alert.source, AlertSource::Price);
        assert_eq!(h.state().alert.frequency, AlertFrequency::EveryTime);
        assert!(h.state().alert.additional_conditions.is_empty());
        assert!(h.state().alert.additional_timeframes.is_empty());
        assert!(h.query_by_label("Add condition").is_none());
        h.get_by_label("Strategy run").click();
        h.run();
        h.get_by_label("Breakout live").click();
        h.run();
        assert_eq!(
            h.state().alert.condition,
            AlertCondition::StrategyFills("run-8".into())
        );
        h.state_mut().strategy_runs.clear();
        h.state_mut()
            .edit(Alert::new("SAMPLE", AlertCondition::Above(1.0)));
        h.run();
        h.get_by_label("Condition").click();
        h.run();
        assert!(h.query_by_label("Strategy fills").is_none());
        h.key_press(egui::Key::Escape);
        h.state_mut().edit(Alert::new(
            "SAMPLE",
            AlertCondition::StrategyFills("gone".into()),
        ));
        h.run();
        assert!(
            h.get_by_label("Strategy run")
                .value()
                .is_some_and(|text| text == "gone")
        );
    }

    /// Names the chart builds keep their parts through storage, so the source list shows the
    /// study output, drawing tool and level translated around the study name, drawing number and
    /// level value, and an alert saved with finished names restores its source and takes the
    /// chart's parts when edited.
    #[test]
    fn chart_built_source_names_reach_the_translator_and_saved_names_restore() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use crate::model::{AlertInputs, Bar, BarData};
        use egui_kittest::{Harness, kittest::Queryable};
        let bars = (0..4)
            .map(|i| {
                let time = DateTime::from_timestamp(60 * i, 0).unwrap();
                Bar::new(time, 100.0, 100.0, 100.0, 100.0, 1.0)
            })
            .collect();
        let mut chart = crate::Chart::new(BarData::from_bars(bars));
        chart.add_indicator(Box::new(crate::studies::MACD::new(12, 26, 9)));
        let mut fork = Drawing::new(7, DrawingToolType::Pitchfork);
        fork.chart_points = vec![
            ChartPoint::new(0.0, 100.0),
            ChartPoint::new(1.0, 200.0),
            ChartPoint::new(2.0, 50.0),
        ];
        // Saved without a config: levels 0, 0.5, 1 and 1.5, all shown.
        fork.pitchfork = None;
        fork.completed = true;
        let line = fork.alert_source(2);
        chart.drawings_mut().drawings.push(fork);

        let stored = serde_json::to_string(&chart.alert_inputs()).unwrap();
        let inputs: AlertInputs = serde_json::from_str(&stored).unwrap();
        let mut fresh = AlertDialog::new();
        fresh.open("SAMPLE".into(), 100.0);
        fresh.sources = inputs.sources;

        let mut saved = Alert::new("SAMPLE", AlertCondition::Crossing(100.0));
        saved.source = line;
        saved.inputs = Some(Box::new(chart.alert_inputs()));
        let mut stored = serde_json::to_value(&saved).unwrap();
        // Alerts saved before names kept their parts hold the finished English text.
        stored["inputs"]["sources"] =
            serde_json::json!([[{"Drawing": "7#2"}, "Pitchfork 7: 0.5: first line"]]);
        let mut saved: Alert = serde_json::from_value(stored).unwrap();
        chart.prepare_alert(&mut saved);
        let mut restored = AlertDialog::new();
        restored.edit(saved);

        // Words drop out under `mark_keys` and values stay: MACD's Signal, Histogram and two
        // conditions, the median and both lines at 0.5, of which the restored alert saved one.
        for dialog in [fresh, restored] {
            let mut h = Harness::builder()
                .with_size(egui::vec2(600.0, 900.0))
                .build_ui_state(|ui, dialog: &mut AlertDialog| dialog.form(ui), dialog);
            h.run();
            h.get_by_label("Source").click();
            h.run();
            crate::localization::mark_keys(&h.ctx);
            h.run();
            for (label, count) in [("~MACD~~~", 4), ("~~~7~~~", 1), ("~~~7~0.5~", 2)] {
                assert_eq!(h.query_all_by_label(label).count(), count, "{label}");
            }
        }
    }

    #[test]
    fn form_controls_fill_the_dialog_width_for_each_source() {
        for (source, condition) in [
            (AlertSource::Price, AlertCondition::Above(100.0)),
            (
                AlertSource::Study {
                    id: "study".into(),
                    output: 0,
                },
                AlertCondition::Above(100.0),
            ),
            (AlertSource::Volume, AlertCondition::Above(100.0)),
            (
                AlertSource::Price,
                AlertCondition::StrategyFills("run".into()),
            ),
        ] {
            for width in [360.0, 560.0] {
                let ctx = Context::default();
                let mut dialog = AlertDialog::new();
                dialog.alert = Alert::new("SAMPLE", condition.clone());
                dialog.alert.source = source.clone();
                dialog.strategy_runs = vec![("run".into(), "Momentum live ".repeat(20))];
                let mut form_width = 0.0;
                for _ in 0..3 {
                    let output = ctx.run_ui(Default::default(), |ui| {
                        ui.set_width(width);
                        let form = ui.vertical(|ui| dialog.form(ui));
                        form_width = form.response.rect.width();
                    });
                    if matches!(condition, AlertCondition::StrategyFills(_)) {
                        assert!(
                            !output.shapes.iter().any(|shape| matches!(
                                &shape.shape, egui::Shape::Text(text)
                                    if text.galley.text() == "Add condition"
                            )),
                            "fill alerts cannot add bar conditions"
                        );
                    }
                    output.drop_without_applying_deltas();
                }
                assert!(
                    (form_width - width).abs() < 16.0,
                    "{source:?}: {form_width} at {width}"
                );
            }
        }
    }

    #[test]
    fn alert_form_and_actions_fit_narrow_windows_in_both_themes() {
        for (dark, fills) in [(false, false), (true, false), (false, true), (true, true)] {
            let ctx = Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let mut dialog = AlertDialog::new();
            dialog.set_storage(Arc::new(Store::default()));
            dialog.open("SAMPLE".into(), 100.0);
            dialog
                .watchlists
                .push(("list".into(), "A long host watchlist name".repeat(4)));
            dialog.alert.watchlist = Some("list".into());
            dialog.alert.frequency = AlertFrequency::OncePerBarClose;
            dialog.sources.push((
                AlertSource::Drawing("7".into()),
                "A long drawing name".repeat(4).into(),
            ));
            dialog.alert.additional_conditions.push(AlertRule {
                source: AlertSource::Volume,
                condition: AlertCondition::VolumeAboveAverage {
                    multiplier: 2.0,
                    bars: 20,
                },
            });
            dialog.alert.additional_conditions.push(AlertRule {
                source: AlertSource::Price,
                condition: AlertCondition::Compare {
                    other: AlertSource::Drawing("7".into()),
                    comparison: AlertComparison::Crossing,
                },
            });
            dialog.sources.push((
                AlertSource::Drawing("7#1".into()),
                "A long second boundary name".repeat(4).into(),
            ));
            dialog.alert.additional_conditions.push(AlertRule {
                source: AlertSource::Price,
                condition: AlertCondition::Channel {
                    boundaries: Box::new([
                        AlertSource::Drawing("7".into()),
                        AlertSource::Drawing("7#1".into()),
                    ]),
                    comparison: AlertChannelComparison::Entering,
                },
            });
            if fills {
                dialog.alert = Alert::new("SAMPLE", AlertCondition::StrategyFills("run".into()));
            }
            for size in [
                egui::vec2(1440.0, 900.0),
                egui::vec2(600.0, 360.0),
                egui::vec2(320.0, 450.0),
            ] {
                for invalid in [false, true] {
                    dialog.expires = invalid;
                    dialog.expiry_date = if invalid { "invalid" } else { "" }.into();
                    for frame in 0..3 {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                dialog.show(ui.ctx());
                            },
                        );
                        let save = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == "Save alert" => {
                                Some((
                                    shape.clip_rect,
                                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                                ))
                            }
                            _ => None,
                        });
                        let message_visible = output.shapes.iter().any(|shape| matches!(
                            &shape.shape, egui::Shape::Text(text) if text.galley.text() == "Message"
                                && shape.clip_rect.contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        ));
                        output.drop_without_applying_deltas();
                        if frame > 0 {
                            if size.y == 900.0 {
                                assert!(message_visible, "the default height fits the message row");
                            }
                            let (clip, button) = save.expect("save action is painted");
                            assert!(
                                clip.contains_rect(button),
                                "save action is clipped: {button:?}"
                            );
                        }
                    }
                    let rect = ctx
                        .memory(|m| m.area_rect(egui::Id::unique("create_alert")))
                        .unwrap();
                    assert!(
                        rect.left() >= 0.0
                            && rect.right() <= size.x
                            && rect.top() >= 0.0
                            && rect.bottom() <= size.y,
                        "dark={dark}, invalid={invalid}, size={size:?}: {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn expiry_fields_resolve_in_the_entry_zone() {
        let at = |y, mo, d, h, mi| Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap();
        let check = |zone: TimezoneId, fields: Option<(&str, &str)>, expected| {
            let mut dialog = AlertDialog::new();
            dialog.timezone = zone.clone();
            dialog.expires = fields.is_some();
            (dialog.expiry_date, dialog.expiry_time) = fields
                .map(|(date, time)| (date.into(), time.into()))
                .unwrap_or_default();
            let expiry = dialog.expiry(at(2026, 1, 1, 0, 0));
            assert_eq!(expiry, expected, "{} {fields:?}", zone.name());
            // Editing a saved alert shows its expiry in the same fields.
            if let (Ok(Some(saved)), Some(fields)) = (expiry, fields) {
                let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
                alert.expires = Some(saved);
                dialog.edit(alert);
                let shown = (dialog.expiry_date.as_str(), dialog.expiry_time.as_str());
                assert_eq!(shown, fields);
            }
        };
        let check_saved = |zone: TimezoneId, saved, edited: &str, expected| {
            let mut dialog = AlertDialog::new();
            dialog.timezone = zone;
            let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
            alert.expires = Some(saved);
            dialog.edit(alert);
            let now = at(2026, 1, 1, 0, 0);
            assert_eq!(
                dialog.expiry(now),
                Ok(Some(saved)),
                "unchanged fields retain the saved instant"
            );
            dialog.expiry_time = edited.into();
            assert_eq!(
                dialog.expiry(now),
                Ok(Some(expected)),
                "edited fields resolve in the entry zone"
            );
        };
        let utc = TimezoneId::utc;
        check_saved(
            utc(),
            at(2026, 3, 8, 2, 30) + chrono::Duration::milliseconds(12_345),
            "02:31",
            at(2026, 3, 8, 2, 31),
        );
        let east_of_utc = |hours: i32| TimezoneId::fixed_offset(hours * 3600).unwrap();
        for (zone, fields, expected) in [
            (utc(), None, Ok(None)),
            (
                utc(),
                Some(("2026-03-08", "02:30")),
                Ok(Some(at(2026, 3, 8, 2, 30))),
            ),
            (
                east_of_utc(-5),
                Some(("2026-03-08", "02:30")),
                Ok(Some(at(2026, 3, 8, 7, 30))),
            ),
            (
                east_of_utc(9),
                Some(("2026-03-08", "02:30")),
                Ok(Some(at(2026, 3, 7, 17, 30))),
            ),
            (
                utc(),
                Some(("08/03/2026", "02:30")),
                Err("Enter the expiry date as YYYY-MM-DD"),
            ),
            (
                utc(),
                Some(("2026-03-08", "2:30 pm")),
                Err("Enter the expiry time as HH:MM"),
            ),
            (
                utc(),
                Some(("2025-12-31", "23:59")),
                Err("Choose an expiry after the current time"),
            ),
        ] {
            check(zone, fields, expected);
        }
        // A saved expiry at the end of the range has no local time on that side of UTC, so an
        // entered time replaces it.
        for (hours, saved) in [
            (9, DateTime::<Utc>::MAX_UTC),
            (-9, DateTime::<Utc>::MIN_UTC),
        ] {
            let mut dialog = AlertDialog::new();
            dialog.timezone = east_of_utc(hours);
            let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
            alert.expires = Some(saved);
            dialog.edit(alert);
            dialog.expiry_date = "2030-01-01".into();
            dialog.expiry_time = "10:00".into();
            assert_eq!(
                dialog.expiry(at(2026, 1, 1, 0, 0)),
                Ok(Some(at(2030, 1, 1, (10 - hours) as u32, 0)))
            );
        }
        #[cfg(feature = "timezones")]
        {
            let new_york = || TimezoneId::named("America/New_York").unwrap();
            check_saved(
                new_york(),
                at(2026, 11, 1, 6, 30) + chrono::Duration::seconds(17),
                "01:31",
                at(2026, 11, 1, 5, 31),
            );
            // Clocks went from 02:00 to 03:00 on 8 March 2026, and on 1 November 2026 from
            // 02:00 daylight time (UTC-4) back to 01:00 standard time (UTC-5).
            for (fields, expected) in [
                (
                    ("2026-03-08", "02:30"),
                    Err("The expiry time is skipped when clocks go forward"),
                ),
                (("2026-11-01", "01:30"), Ok(Some(at(2026, 11, 1, 5, 30)))),
                (("2026-07-04", "11:45"), Ok(Some(at(2026, 7, 4, 15, 45)))),
            ] {
                check(new_york(), Some(fields), expected);
            }
        }
    }

    #[test]
    fn schedule_fields_resolve_to_a_valid_schedule_or_name_the_problem() {
        let mut dialog = AlertDialog::new();
        assert_eq!(dialog.schedule(), Ok(None));
        dialog.scheduled = true;
        dialog.schedule_zone = "UTC".into();
        assert_eq!(dialog.schedule(), Err("Select at least one weekday"));
        dialog.schedule_days[4] = true;
        assert_eq!(dialog.schedule(), Err("Add a notification window"));
        dialog.schedule_windows = vec![["22:00".into(), "2am".into()]];
        assert_eq!(dialog.schedule(), Err("Enter the window times as HH:MM"));
        dialog.schedule_windows = vec![["22:00".into(), "02:00".into()]];
        dialog.schedule_zone = "Mars/Olympus".into();
        assert!(dialog.schedule().is_err());
        dialog.schedule_zone = "UTC".into();
        let schedule = dialog.schedule().unwrap().unwrap();
        assert_eq!(schedule.days, [Weekday::Fri]);
        // Editing a saved alert shows the same fields.
        let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
        alert.schedule = Some(Box::new(schedule.clone()));
        let mut other = AlertDialog::new();
        other.edit(alert);
        assert_eq!(other.schedule(), Ok(Some(schedule)));
        // Equal bounds are a full day.
        dialog.schedule_windows = vec![["22:00".into(), "22:00".into()]];
        assert!(dialog.schedule().is_ok());
    }

    #[test]
    fn a_new_schedule_starts_in_the_account_zone_and_the_last_schedule_is_remembered() {
        let store = Arc::new(Store::default());
        let ctx = Context::default();
        let mut dialog = AlertDialog::new();
        dialog.set_storage(store.clone());
        dialog.poll(&ctx);
        dialog.timezone = TimezoneId::fixed_offset(3600).unwrap();
        dialog.open("AAPL".into(), 1.0);
        assert_eq!(dialog.schedule_zone, "+01:00");
        dialog.account_timezone = Some(TimezoneId::fixed_offset(-7200).unwrap());
        dialog.open("AAPL".into(), 1.0);
        assert_eq!(dialog.schedule_zone, "-02:00");
        assert!(!dialog.scheduled, "no schedule is remembered yet");

        dialog.scheduled = true;
        dialog.schedule_days[0] = true;
        dialog.schedule_windows = vec![["09:00".into(), "09:00".into()]];
        let schedule = dialog.schedule().unwrap().unwrap();
        dialog.alert.schedule = Some(Box::new(schedule.clone()));
        dialog.remember_schedule();
        dialog.poll(&ctx);

        // A new dialog on the same storage offers it to a new alert but not to an edited one.
        let mut restored = AlertDialog::new();
        restored.set_storage(store);
        restored.poll(&ctx);
        restored.open("AAPL".into(), 1.0);
        assert_eq!(restored.schedule(), Ok(Some(schedule)));
        restored.edit(Alert::new("AAPL", AlertCondition::Crossing(1.0)));
        assert_eq!(restored.schedule(), Ok(None));
    }

    #[test]
    fn reset_keeps_the_schedule_a_new_alert_opened_with() {
        let ctx = Context::default();
        let mut dialog = AlertDialog::new();
        let reset = |dialog: &mut AlertDialog| {
            ctx.run_ui(Default::default(), |ui| dialog.form(ui))
                .drop_without_applying_deltas();
            dialog.scheduled = false;
            dialog.reset();
            dialog.schedule().unwrap()
        };
        dialog.open("AAPL".into(), 1.0);
        dialog.scheduled = true;
        dialog.schedule_zone = "UTC".into();
        dialog.schedule_days[0] = true;
        dialog.schedule_windows = vec![["09:00".into(), "17:00".into()]];
        let schedule = dialog.schedule().unwrap().unwrap();
        dialog.alert.schedule = Some(Box::new(schedule.clone()));
        dialog.remember_schedule();

        dialog.open("AAPL".into(), 1.0);
        assert_eq!(reset(&mut dialog), Some(schedule.clone()));
        // An edited alert returns to its own schedule, or to none.
        let own = NotificationSchedule {
            days: vec![chrono::Weekday::Tue],
            ..schedule
        };
        let mut alert = Alert::new("AAPL", AlertCondition::Crossing(1.0));
        dialog.edit(alert.clone());
        assert_eq!(reset(&mut dialog), None);
        alert.schedule = Some(Box::new(own.clone()));
        dialog.edit(alert);
        assert_eq!(reset(&mut dialog), Some(own));
    }

    #[test]
    fn a_start_must_precede_the_expiry() {
        let mut dialog = AlertDialog::new();
        assert_eq!(dialog.start(None), Ok(None));
        dialog.starts = true;
        dialog.start_date = "2026-03-08".into();
        dialog.start_time = "02:30".into();
        let start = "2026-03-08T02:30:00Z".parse::<DateTime<Utc>>().unwrap();
        assert_eq!(dialog.start(None), Ok(Some(start)));
        assert_eq!(
            dialog.start(Some(start)),
            Err("Choose a start before the expiry")
        );
        dialog.start_time = "2:30 pm".into();
        assert_eq!(dialog.start(None), Err("Enter the start time as HH:MM"));
    }

    #[test]
    fn presets_keep_their_limit_and_fields_through_replace_delete_reset_and_restart() {
        let store = Arc::new(Store::default());
        let ctx = Context::default();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let mut dialog = AlertDialog::new();
        dialog.set_storage(store.clone());
        dialog.poll(&ctx);
        dialog.open("EURUSD".into(), 1.125);
        dialog.alert.name = "Breakout".into();
        dialog.alert.frequency = AlertFrequency::OncePerBarClose;
        dialog.alert.delivery.webhook_url = "https://example.test/hook".into();
        dialog.preset_fields = PresetFields {
            text: false,
            ..PresetFields::default()
        };
        dialog.preset_name = " Close ".into();
        dialog.save_preset(now);
        dialog.poll(&ctx);
        let stored = store.0.lock().unwrap().clone().unwrap();
        for excluded in ["EURUSD", "1.125", "Breakout", "example.test"] {
            assert!(!stored.contains(excluded), "{excluded} in {stored}");
        }

        for name in ["A", "B", "C", "D"] {
            dialog.preset_name = name.into();
            dialog.save_preset(now);
            dialog.poll(&ctx);
        }
        assert_eq!(dialog.presets.len(), MAX_PRESETS);
        dialog.preset_name = "Sixth".into();
        assert!(dialog.preset_limit_reached());
        dialog.save_preset(now);
        dialog.poll(&ctx);
        assert_eq!(dialog.presets.len(), MAX_PRESETS, "a sixth is refused");
        assert!(!dialog.presets.contains_key("Sixth"));

        // A replacement holds what is selected now, even at the limit.
        dialog.preset_name = "A".into();
        assert!(!dialog.preset_limit_reached());
        dialog.preset_fields = PresetFields {
            frequency: false,
            ..PresetFields::default()
        };
        dialog.save_preset(now);
        dialog.poll(&ctx);
        assert_eq!(dialog.presets.len(), MAX_PRESETS);
        dialog.preset_name = "D".into();
        dialog.delete_preset();
        dialog.poll(&ctx);
        assert_eq!(dialog.presets.len(), MAX_PRESETS - 1);

        let mut restored = AlertDialog::new();
        restored.set_storage(store.clone());
        restored.poll(&ctx);
        assert_eq!(restored.presets, dialog.presets);
        restored.open("AAPL".into(), 123.0);
        // A host may adjust the staged alert before its first frame; that is the initial state.
        restored.alert.delivery.webhook_url = "https://own.test".into();
        ctx.run_ui(Default::default(), |ui| restored.form(ui))
            .drop_without_applying_deltas();
        let opened = restored.alert.clone();
        restored.apply_preset("Close".into(), now);
        assert_eq!(restored.alert.symbol, "AAPL");
        assert_eq!(restored.alert.condition, AlertCondition::Crossing(123.0));
        assert_eq!(restored.alert.frequency, AlertFrequency::OncePerBarClose);
        assert!(restored.alert.name.is_empty(), "the name was not selected");
        assert_eq!(restored.alert.delivery.webhook_url, "https://own.test");
        assert_eq!(restored.preset_name, "Close");
        restored.apply_preset("A".into(), now);
        assert_eq!(
            restored.alert.frequency,
            AlertFrequency::OncePerBarClose,
            "frequency left out of the replacement stays"
        );
        assert_eq!(
            restored.alert.name, "Breakout",
            "the replacement holds the name it selected"
        );
        restored.reset();
        assert_eq!(restored.alert, opened);
        assert!(restored.preset_name.is_empty());
        assert_eq!(restored.preset_fields, PresetFields::default());

        // A refused save leaves the kept presets as they are.
        let mut failing = AlertDialog::new();
        failing.set_storage(Arc::new(Refusing));
        failing.poll(&ctx);
        failing.open("AAPL".into(), 123.0);
        failing.preset_name = "New".into();
        failing.save_preset(now);
        failing.poll(&ctx);
        assert!(failing.presets.is_empty());
        assert!(failing.error.is_some());

        *store.0.lock().unwrap() = Some(r#"{"version":4,"presets":{}}"#.into());
        restored.set_storage(store);
        restored.poll(&ctx);
        assert!(!restored.loaded);
        assert!(restored.error.is_some());
    }

    #[test]
    fn presets_changed_elsewhere_wait_for_the_users_choice() {
        use crate::services::document::tests::Revised;
        let ctx = Context::default();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let stored = |host: &Revised| {
            let (presets, _) = preset::from_document(&host.get(PRESETS).unwrap(), now).unwrap();
            presets.into_keys().collect::<Vec<_>>()
        };
        for (choice, kept) in [
            (ConflictChoice::ReloadRemote, "Remote"),
            (ConflictChoice::Retry, "Local"),
        ] {
            let host = Arc::new(Revised::default());
            let mut dialog = AlertDialog::new();
            dialog.set_storage(host.clone());
            dialog.poll(&ctx);
            // Another client saves a preset after this one read that there were none.
            let mut other = AlertDialog::new();
            other.set_storage(host.clone());
            other.poll(&ctx);
            other.open("AAPL".into(), 1.0);
            other.preset_name = "Remote".into();
            other.save_preset(now);
            other.poll(&ctx);
            dialog.open("EURUSD".into(), 1.125);
            dialog.preset_name = "Local".into();
            dialog.save_preset(now);
            dialog.poll(&ctx);
            assert!(dialog.storage_conflict());
            assert!(dialog.presets.is_empty());
            assert_eq!(stored(&host), ["Remote"]);
            dialog.resolve_storage_conflict(choice);
            dialog.poll(&ctx);
            assert!(!dialog.storage_conflict());
            assert_eq!(dialog.presets.keys().collect::<Vec<_>>(), [kept]);
            assert_eq!(stored(&host), [kept]);
        }
    }
}
