//! Typed alert editing. Hosts supply output identifiers, watchlists and optional preset storage.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::{Context, Ui};

use super::{AlertDialogAction, AlertDialogConfig};
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{
    Alert, AlertComparison, AlertCondition, AlertFrequency, AlertRule, AlertSource,
};
use crate::services::{Storage, StoreKind};
use crate::ui_kit::FormGrid;
use crate::ui_kit::dialog::{DialogFooter, DialogFrame, dialog_header};
use crate::ui_kit::status_message::{MessageKind, status_message};

/// A dialog that creates or edits the same alert type the host stores and evaluates.
pub struct AlertDialog {
    /// Layout dimensions.
    pub config: AlertDialogConfig,
    /// Whether the modal is visible.
    pub is_open: bool,
    /// Staged edits. Cancel never submits them.
    pub alert: Alert,
    /// Host-provided study outputs and drawings, as stable identifiers and display names.
    pub sources: Vec<(AlertSource, String)>,
    /// Host-provided watchlist identifiers and display names.
    pub watchlists: Vec<(String, String)>,
    expires: String,
    preset_name: String,
    presets: BTreeMap<String, Alert>,
    storage: Option<Arc<dyn Storage>>,
    inbox: Arc<Inbox>,
    pending: bool,
    loaded: bool,
    error: Option<String>,
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
            expires: String::new(),
            preset_name: String::new(),
            presets: BTreeMap::new(),
            storage: None,
            inbox: Arc::default(),
            pending: false,
            loaded: false,
            error: None,
        }
    }

    /// Load presets from the host storage service. A failed load never permits overwriting it.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.inbox = Arc::default();
        self.presets.clear();
        self.error = None;
        self.loaded = false;
        self.pending = true;
        storage.load(StoreKind::AlertPresets, "presets", self.inbox.reply(1));
        self.storage = Some(storage);
    }

    /// Open a fresh price alert at the last or hovered price, discarding earlier cancelled edits.
    pub fn open(&mut self, symbol: String, price: f64) {
        self.edit(Alert::new(symbol, AlertCondition::Crossing(price)));
    }

    /// Stage an existing alert, retaining its host identifier and status when saved.
    pub fn edit(&mut self, alert: Alert) {
        self.expires = alert.expires.map(|t| t.to_rfc3339()).unwrap_or_default();
        self.alert = alert;
        self.is_open = true;
    }

    /// Dismiss without saving.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Follow an external close flag.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if !should_be_open {
            self.close();
        }
    }

    fn poll(&mut self, ctx: &Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (key, answer) in self.inbox.drain().answers {
            self.pending = false;
            if key == 1 {
                let result = *answer
                    .downcast::<Result<Option<String>, FeedError>>()
                    .expect("preset load");
                match result {
                    Ok(document) => {
                        let result = document
                            .map(|doc| serde_json::from_str::<PresetDocument>(&doc))
                            .transpose();
                        match result {
                            Ok(doc) if doc.as_ref().is_none_or(|doc| doc.version == 1) => {
                                self.presets = doc.map(|doc| doc.presets).unwrap_or_default();
                                self.loaded = true;
                            }
                            Ok(_) => self.error = Some("Unsupported alert preset version".into()),
                            Err(error) => self.error = Some(error.to_string()),
                        }
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            } else if let Err(error) = *answer
                .downcast::<Result<(), FeedError>>()
                .expect("preset save")
            {
                self.error = Some(error.to_string());
            }
        }
    }

    fn save_preset(&mut self) {
        self.presets
            .insert(self.preset_name.trim().into(), self.alert.clone());
        let document = PresetDocument {
            version: 1,
            presets: self.presets.clone(),
        };
        match serde_json::to_string(&document) {
            Ok(document) => {
                if let Some(storage) = &self.storage {
                    self.pending = true;
                    self.error = None;
                    storage.save(
                        StoreKind::AlertPresets,
                        "presets",
                        document,
                        self.inbox.reply(2),
                    );
                }
            }
            Err(error) => self.error = Some(error.to_string()),
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
        DialogFrame::with_width(title, self.config.width).show(ctx, |ui| {
            if dialog_header(ui, title) {
                self.close();
                action = AlertDialogAction::Cancel;
            }
            ui.separator();
            let mut expiry = Ok(None);
            let mut valid = false;
            egui::ScrollArea::vertical()
                .max_height(
                    (ui.available_height() - 65.0)
                        .min(self.config.height)
                        .max(0.0),
                )
                .show(ui, |ui| {
                    self.form(ui);
                    expiry = if self.expires.trim().is_empty() {
                        Ok(None)
                    } else {
                        DateTime::parse_from_rfc3339(self.expires.trim())
                            .map(|t| Some(t.with_timezone(&Utc)))
                    };
                    valid = self.alert.is_valid() && expiry.is_ok();
                    if !valid {
                        status_message(
                            ui,
                            MessageKind::Error,
                            "Check the subject, levels, lookback and expiry (RFC 3339)",
                        );
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
                self.alert.expires = expiry.unwrap();
                action = AlertDialogAction::Create(Box::new(self.alert.clone()));
                self.close();
            }
        });
        action
    }

    fn form(&mut self, ui: &mut Ui) {
        ui.spacing_mut().text_edit_width = (ui.available_width() - 110.0).clamp(40.0, 280.0);
        FormGrid::new("alert_details").show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.alert.name);
            ui.end_row();
            ui.label("Symbol");
            ui.text_edit_singleline(&mut self.alert.symbol);
            ui.end_row();
            ui.label("Watchlist");
            egui::ComboBox::from_id_salt("alert_watchlist")
                .width(ui.spacing().text_edit_width)
                .truncate()
                .selected_text(
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
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.alert.watchlist, None, "One symbol");
                    for (id, name) in &self.watchlists {
                        ui.selectable_value(&mut self.alert.watchlist, Some(id.clone()), name);
                    }
                });
            ui.end_row();
            ui.label("Frequency");
            egui::ComboBox::from_id_salt("alert_frequency")
                .selected_text(self.alert.frequency.to_string())
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (AlertFrequency::Once, "Only once"),
                        (AlertFrequency::EveryTime, "Every time"),
                        (AlertFrequency::OncePerBar, "Once per bar"),
                        (AlertFrequency::OncePerBarClose, "Once per bar close"),
                    ] {
                        ui.selectable_value(&mut self.alert.frequency, value, label);
                    }
                });
            ui.end_row();
            ui.label("Expiry (optional)");
            ui.text_edit_singleline(&mut self.expires);
            ui.end_row();
            ui.label("Message");
            ui.text_edit_multiline(&mut self.alert.message);
            ui.end_row();
        });
        ui.separator();
        ui.label("All conditions must hold on the same bar");
        ui.push_id("first", |ui| {
            rule_form(
                ui,
                &mut self.alert.source,
                &mut self.alert.condition,
                &self.sources,
            )
        });
        let mut remove = None;
        for (index, rule) in self.alert.additional_conditions.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.separator();
                rule_form(ui, &mut rule.source, &mut rule.condition, &self.sources);
                if ui.button("Remove condition").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            self.alert.additional_conditions.remove(index);
        }
        if ui.button("Add condition").clicked() {
            self.alert.additional_conditions.push(AlertRule {
                source: self.alert.source.clone(),
                condition: self.alert.condition.clone(),
            });
        }
        if self.storage.is_some() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Preset");
                ui.text_edit_singleline(&mut self.preset_name);
                if ui
                    .add_enabled(
                        self.loaded
                            && !self.pending
                            && self.alert.is_valid()
                            && !self.preset_name.trim().is_empty(),
                        egui::Button::new("Save preset"),
                    )
                    .clicked()
                {
                    if self.expires.trim().is_empty() {
                        self.alert.expires = None;
                        self.save_preset();
                    } else if let Ok(t) = DateTime::parse_from_rfc3339(self.expires.trim()) {
                        self.alert.expires = Some(t.with_timezone(&Utc));
                        self.save_preset();
                    }
                }
            });
            let mut apply = None;
            egui::ComboBox::from_id_salt("alert_presets")
                .selected_text("Apply preset")
                .show_ui(ui, |ui| {
                    for (name, alert) in &self.presets {
                        if ui.selectable_label(false, name).clicked() {
                            apply = Some(alert.clone());
                        }
                    }
                });
            if let Some(mut preset) = apply {
                preset.id.clone_from(&self.alert.id);
                preset.status = self.alert.status;
                self.edit(preset);
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PresetDocument {
    version: u32,
    presets: BTreeMap<String, Alert>,
}

fn source_picker(
    ui: &mut Ui,
    id: &str,
    source: &mut AlertSource,
    sources: &[(AlertSource, String)],
) {
    let label = sources
        .iter()
        .find(|(value, _)| value == source)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| format!("{source:?}"));
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width().min(200.0))
        .truncate()
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(source, AlertSource::Price, "Price");
            ui.selectable_value(source, AlertSource::Volume, "Volume");
            for (value, name) in sources {
                ui.selectable_value(source, value.clone(), name);
            }
        });
}

fn rule_form(
    ui: &mut Ui,
    source: &mut AlertSource,
    condition: &mut AlertCondition,
    sources: &[(AlertSource, String)],
) {
    ui.add_enabled_ui(
        !matches!(condition, AlertCondition::VolumeAboveAverage { .. }),
        |ui| source_picker(ui, "source", source, sources),
    );
    let level = condition.level().unwrap_or(0.0);
    let choices = [
        (AlertCondition::Crossing(level), "Crossing"),
        (AlertCondition::CrossingUp(level), "Crossing up"),
        (AlertCondition::CrossingDown(level), "Crossing down"),
        (AlertCondition::Above(level), "Greater than"),
        (AlertCondition::Below(level), "Less than"),
        (
            AlertCondition::EnteringChannel {
                lower: level,
                upper: level + 1.0,
            },
            "Entering channel",
        ),
        (
            AlertCondition::ExitingChannel {
                lower: level,
                upper: level + 1.0,
            },
            "Exiting channel",
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
    ];
    let selected = choices
        .iter()
        .find(|(c, _)| std::mem::discriminant(c) == std::mem::discriminant(condition))
        .map(|(_, name)| *name)
        .unwrap_or("Condition");
    egui::ComboBox::from_id_salt("condition")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (value, label) in choices {
                if ui.selectable_label(label == selected, label).clicked() && label != selected {
                    if matches!(value, AlertCondition::VolumeAboveAverage { .. }) {
                        *source = AlertSource::Volume;
                    }
                    *condition = value;
                }
            }
        });
    ui.horizontal_wrapped(|ui| match condition {
        AlertCondition::Crossing(v)
        | AlertCondition::CrossingUp(v)
        | AlertCondition::CrossingDown(v)
        | AlertCondition::Above(v)
        | AlertCondition::Below(v) => {
            ui.label("Level");
            ui.add(egui::DragValue::new(v).speed(0.01));
        }
        AlertCondition::EnteringChannel { lower, upper }
        | AlertCondition::ExitingChannel { lower, upper } => {
            ui.label("Lower / upper");
            ui.add(egui::DragValue::new(lower));
            ui.add(egui::DragValue::new(upper));
        }
        AlertCondition::MovingBy { value, bars }
        | AlertCondition::MovingByPercent {
            percent: value,
            bars,
        } => {
            ui.label("Change / bars");
            ui.add(egui::DragValue::new(value));
            ui.add(egui::DragValue::new(bars).range(1..=u32::MAX));
        }
        AlertCondition::VolumeAboveAverage { multiplier, bars } => {
            ui.label("Multiplier / preceding bars");
            ui.add(egui::DragValue::new(multiplier).speed(0.1));
            ui.add(egui::DragValue::new(bars).range(1..=u32::MAX));
        }
        AlertCondition::Compare { other, comparison } => {
            source_picker(ui, "other", other, sources);
            egui::ComboBox::from_id_salt("comparison")
                .selected_text(comparison.to_string())
                .show_ui(ui, |ui| {
                    for value in [
                        AlertComparison::Crossing,
                        AlertComparison::CrossingUp,
                        AlertComparison::CrossingDown,
                        AlertComparison::Above,
                        AlertComparison::Below,
                    ] {
                        ui.selectable_value(comparison, value, value.to_string());
                    }
                });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
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

    #[test]
    fn alert_form_and_actions_fit_narrow_windows_in_both_themes() {
        for dark in [false, true] {
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
                "A long drawing name".repeat(4),
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
            for size in [egui::vec2(1440.0, 900.0), egui::vec2(320.0, 450.0)] {
                for invalid in [false, true] {
                    dialog.expires = if invalid { "invalid" } else { "" }.into();
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
                        output.drop_without_applying_deltas();
                        if frame > 0 {
                            let (clip, button) = save.expect("save action is painted");
                            assert!(
                                clip.contains_rect(button),
                                "save action is clipped: {button:?}"
                            );
                        }
                    }
                    let rect = ctx
                        .memory(|m| m.area_rect(egui::Id::unique(Some("Create Alert"))))
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
    fn presets_survive_a_new_dialog_and_unknown_versions_cannot_be_overwritten() {
        let store = Arc::new(Store::default());
        let ctx = Context::default();
        let mut dialog = AlertDialog::new();
        dialog.set_storage(store.clone());
        dialog.poll(&ctx);
        dialog.open("EURUSD".into(), 1.125);
        dialog.alert.name = "Breakout".into();
        dialog.alert.frequency = AlertFrequency::OncePerBarClose;
        dialog.preset_name = "Close".into();
        dialog.save_preset();
        dialog.poll(&ctx);
        let expected = dialog.alert.clone();
        let mut restored = AlertDialog::new();
        restored.set_storage(store.clone());
        restored.poll(&ctx);
        assert_eq!(restored.presets["Close"], expected);
        *store.0.lock().unwrap() = Some(r#"{"version":2,"presets":{}}"#.into());
        restored.set_storage(store);
        restored.poll(&ctx);
        assert!(!restored.loaded);
        assert!(restored.error.is_some());
        restored.open("AAPL".into(), 123.0);
        assert_eq!(restored.alert.condition, AlertCondition::Crossing(123.0));
        assert!(restored.alert.name.is_empty());
    }
}
