//! Settings for an existing study, staged until Apply.

use crate::localization as l10n;
use crate::model::LineStyle;
use crate::model::Timeframe;
use crate::studies::{
    Indicator, InputKind, InputValue, PlotType, StudyInput, StudyScale, StudySettings,
};
use crate::theme::Theme;
use crate::ui_kit::{FormGrid, dialog::DialogFooter, modal::Modal};
use crate::widget::SeriesId;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Inputs,
    Style,
    Visibility,
}

/// A complete edit for the host to apply to the original study id.
pub struct StudyEdit {
    /// Stable id of the applied study.
    pub id: SeriesId,
    /// Calculation input ids and their new values.
    pub inputs: Vec<(String, InputValue)>,
    /// Per-plot styles, precision and interval visibility.
    pub settings: StudySettings,
}

/// Inputs, Style and Visibility editor for an applied study. Closing or cancelling discards
/// staged values; no study or output history is cloned while the dialog is open.
#[derive(Default)]
pub struct IndicatorSettingsDialog {
    edit: Option<StudyEdit>,
    inputs: Vec<StudyInput>,
    title: String,
    line_names: Vec<String>,
    tab: Tab,
    current_interval: Option<Timeframe>,
    sources: Vec<(crate::studies::StudySource, String)>,
    overlay: bool,
    price_scale: bool,
}

impl crate::ui_kit::tab_bar::TabLabel for Tab {
    fn tab_label(&self) -> &str {
        match self {
            Self::Inputs => "Inputs",
            Self::Style => "Style",
            Self::Visibility => "Visibility",
        }
    }
}

impl IndicatorSettingsDialog {
    /// Open on the current values of the study identified by `id`.
    pub fn open(&mut self, id: SeriesId, study: &dyn Indicator, interval: Option<Timeframe>) {
        self.edit = Some(StudyEdit {
            id,
            inputs: Vec::new(),
            settings: study.settings(),
        });
        self.inputs = study.inputs();
        self.title = format!("{} settings", study.name());
        self.line_names = study.line_names();
        self.overlay = study.is_overlay();
        self.price_scale = study.scale() == StudyScale::Main;
        self.current_interval = interval;
        self.tab = Tab::Inputs;
        self.sources.clear();
    }

    /// Open on a chart study and offer its other studies as source inputs, excluding cycles.
    pub fn open_for_chart(&mut self, chart: &crate::widget::Chart, id: SeriesId) -> bool {
        let Some((_, study)) = chart.indicators().find(|(candidate, _)| *candidate == id) else {
            return false;
        };
        self.open(id, study, chart.timeframe());
        let studies: Vec<_> = chart.indicators().collect();
        for (candidate, study) in &studies {
            let mut next = Some(candidate.0);
            for _ in 0..=studies.len() {
                let Some(current) = next else {
                    break;
                };
                if current == id.0 {
                    break;
                }
                next = studies
                    .iter()
                    .find(|(id, _)| id.0 == current)
                    .and_then(|(_, study)| match study.source() {
                        Some(crate::studies::StudySource::Study { id, .. }) => Some(id),
                        _ => None,
                    });
            }
            if next.is_some() {
                continue;
            }
            for (plot, name) in study.line_names().into_iter().enumerate() {
                self.sources.push((
                    crate::studies::StudySource::Study {
                        id: candidate.0,
                        plot,
                    },
                    format!("{}: {name}", study.name()),
                ));
            }
        }
        true
    }

    /// Whether an edit is open.
    pub fn is_open(&self) -> bool {
        self.edit.is_some()
    }

    /// Show the staged edit. Apply returns it once; the host calls
    /// [`Indicator::apply_settings`] on the original study, then lets the chart recalculate.
    pub fn show(&mut self, ctx: &egui::Context, _theme: &Theme) -> Option<StudyEdit> {
        let edit = self.edit.as_mut()?;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Modal::new("indicator_settings", &self.title)
            .width(620.0)
            .height(560.0)
            .top(120.0)
            .show(ctx, &mut open, |ui| {
                crate::ui_kit::tab_bar::TabBar::new(
                    &[Tab::Inputs, Tab::Style, Tab::Visibility],
                    &mut self.tab,
                )
                .show(ui);
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - 72.0).max(0.0))
                    .show(ui, |ui| match self.tab {
                        Tab::Inputs => {
                            if self.inputs.is_empty() {
                                ui.label(l10n::text(
                                    ui.ctx(),
                                    "This study has no editable inputs.",
                                ));
                            }
                            if !self.inputs.is_empty()
                                && ui
                                    .small_button(l10n::text(ui.ctx(), "Reset inputs"))
                                    .clicked()
                            {
                                for input in &mut self.inputs {
                                    input.value = input.default.clone();
                                }
                            }
                            let study_source = self.inputs.iter().any(|input| {
                                matches!(
                                    input.value,
                                    InputValue::Source(crate::studies::StudySource::Study { .. })
                                )
                            });
                            FormGrid::new("study_inputs").show(ui, |ui| {
                                for input in &mut self.inputs {
                                    if study_source
                                        && matches!(
                                            input.id.as_str(),
                                            "calculation_symbol"
                                                | "calculation_timeframe"
                                                | "wait_for_close"
                                        )
                                    {
                                        input.value = input.default.clone();
                                        continue;
                                    }
                                    let label = ui.label(l10n::text(ui.ctx(), &input.label));
                                    let response = match (&input.kind, &mut input.value) {
                                        (
                                            InputKind::Length(min, max),
                                            InputValue::Length(value),
                                        ) => ui.add(egui::DragValue::new(value).range(*min..=*max)),
                                        (
                                            InputKind::Integer(min, max),
                                            InputValue::Integer(value),
                                        ) => ui.add(egui::DragValue::new(value).range(*min..=*max)),
                                        (
                                            InputKind::Number(min, max),
                                            InputValue::Number(value),
                                        ) => ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.1)
                                                .range(*min..=*max),
                                        ),
                                        (InputKind::Choice(choices), InputValue::Choice(value)) => {
                                            egui::ComboBox::from_id_salt(&input.id)
                                                .selected_text(l10n::text(
                                                    ui.ctx(),
                                                    choices.get(*value).map_or("", String::as_str),
                                                ))
                                                .show_ui(ui, |ui| {
                                                    for (index, label) in choices.iter().enumerate()
                                                    {
                                                        ui.selectable_value(
                                                            value,
                                                            index,
                                                            l10n::text(ui.ctx(), label),
                                                        );
                                                    }
                                                })
                                                .response
                                        }
                                        (InputKind::Toggle, InputValue::Toggle(value)) => {
                                            ui.checkbox(value, "")
                                        }
                                        (InputKind::Source, InputValue::Source(value)) => {
                                            egui::ComboBox::from_id_salt(&input.id)
                                                .selected_text(l10n::text(
                                                    ui.ctx(),
                                                    match value {
                                                        crate::studies::StudySource::Price(
                                                            source,
                                                        ) => source.label().to_owned(),
                                                        crate::studies::StudySource::Study {
                                                            ..
                                                        } => self
                                                            .sources
                                                            .iter()
                                                            .find(|(source, _)| source == value)
                                                            .map(|(_, label)| label.clone())
                                                            .unwrap_or_else(|| {
                                                                "Unavailable study".to_owned()
                                                            }),
                                                    },
                                                ))
                                                .show_ui(ui, |ui| {
                                                    for (source, label) in &self.sources {
                                                        ui.selectable_value(
                                                            value,
                                                            *source,
                                                            l10n::text(ui.ctx(), label),
                                                        );
                                                    }
                                                    for source in crate::model::PriceSource::all() {
                                                        ui.selectable_value(
                                                            value,
                                                            crate::studies::StudySource::Price(
                                                                source,
                                                            ),
                                                            l10n::text(ui.ctx(), source.label()),
                                                        );
                                                    }
                                                })
                                                .response
                                        }
                                        (InputKind::Color, InputValue::Color(value)) => {
                                            ui.color_edit_button_srgba(value)
                                        }
                                        (InputKind::Symbol, InputValue::Symbol(value)) => ui.add(
                                            egui::TextEdit::singleline(value)
                                                .hint_text(l10n::text(ui.ctx(), "Chart symbol")),
                                        ),
                                        (InputKind::Timeframe, InputValue::Timeframe(value)) => {
                                            let response = egui::ComboBox::from_id_salt(&input.id)
                                                .selected_text(l10n::text(
                                                    ui.ctx(),
                                                    value.map_or_else(
                                                        || "Chart interval".to_owned(),
                                                        |tf| tf.to_string(),
                                                    ),
                                                ))
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(
                                                        value,
                                                        None,
                                                        l10n::text(ui.ctx(), "Chart interval"),
                                                    );
                                                    for tf in Timeframe::all() {
                                                        ui.selectable_value(
                                                            value,
                                                            Some(tf),
                                                            l10n::text(ui.ctx(), tf.to_string()),
                                                        );
                                                    }
                                                    if ui
                                                        .selectable_label(
                                                            value.is_some_and(|tf| tf.is_custom()),
                                                            l10n::text(ui.ctx(), "Custom"),
                                                        )
                                                        .clicked()
                                                    {
                                                        *value = Some(Timeframe::Custom(
                                                            std::num::NonZeroU64::new(60).unwrap(),
                                                        ));
                                                    }
                                                })
                                                .response;
                                            if let Some(Timeframe::Custom(seconds)) = value {
                                                let mut count = seconds.get();
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut count)
                                                            .range(1..=u64::MAX)
                                                            .suffix(l10n::text(ui.ctx(), " s")),
                                                    )
                                                    .labelled_by(label.id)
                                                    .changed()
                                                {
                                                    *seconds =
                                                        std::num::NonZeroU64::new(count).unwrap();
                                                }
                                            }
                                            response
                                        }
                                        _ => ui.label(l10n::text(
                                            ui.ctx(),
                                            "This input is unavailable.",
                                        )),
                                    };
                                    response.labelled_by(label.id);
                                    ui.end_row();
                                }
                            });
                        }
                        Tab::Style => {
                            for (line, plot) in edit.settings.plots.iter_mut().enumerate() {
                                ui.push_id(line, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.checkbox(
                                            &mut plot.visible,
                                            l10n::text(
                                                ui.ctx(),
                                                self.line_names
                                                    .get(line)
                                                    .map_or("Plot", String::as_str),
                                            ),
                                        );
                                        ui.color_edit_button_srgba(&mut plot.color);
                                        ui.add(
                                            egui::DragValue::new(&mut plot.width)
                                                .range(0.5..=10.0)
                                                .suffix(l10n::text(ui.ctx(), " px")),
                                        )
                                        .accessible_name(l10n::text(ui.ctx(), "Line width"));
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        egui::ComboBox::from_id_salt("pattern")
                                            .selected_text(l10n::text(
                                                ui.ctx(),
                                                format!("{:?}", plot.pattern),
                                            ))
                                            .show_ui(ui, |ui| {
                                                for pattern in [
                                                    LineStyle::Solid,
                                                    LineStyle::Dashed,
                                                    LineStyle::Dotted,
                                                    LineStyle::LargeDashed,
                                                    LineStyle::SparseDotted,
                                                ] {
                                                    ui.selectable_value(
                                                        &mut plot.pattern,
                                                        pattern,
                                                        l10n::text(
                                                            ui.ctx(),
                                                            format!("{pattern:?}"),
                                                        ),
                                                    );
                                                }
                                            })
                                            .response
                                            .accessible_name(l10n::text(ui.ctx(), "Line pattern"));
                                        egui::ComboBox::from_id_salt("plot_type")
                                            .selected_text(l10n::text(
                                                ui.ctx(),
                                                format!("{:?}", plot.plot_type),
                                            ))
                                            .show_ui(ui, |ui| {
                                                for kind in PlotType::ALL {
                                                    let label =
                                                        l10n::text(ui.ctx(), format!("{kind:?}"));
                                                    // A saved choice stays listed and selected.
                                                    match kind
                                                        .incompatibility(
                                                            self.overlay,
                                                            self.price_scale,
                                                        )
                                                        .filter(|_| kind != plot.plot_type)
                                                    {
                                                        Some(reason) => {
                                                            ui.add_enabled(
                                                                false,
                                                                egui::Button::selectable(
                                                                    false, label,
                                                                ),
                                                            )
                                                            .on_disabled_hover_text(l10n::text(
                                                                ui.ctx(),
                                                                reason,
                                                            ));
                                                        }
                                                        None => {
                                                            ui.selectable_value(
                                                                &mut plot.plot_type,
                                                                kind,
                                                                label,
                                                            );
                                                        }
                                                    }
                                                }
                                            })
                                            .response
                                            .accessible_name(l10n::text(ui.ctx(), "Plot type"));
                                    });
                                    ui.separator();
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(l10n::text(ui.ctx(), "Precision"));
                                let mut automatic = edit.settings.precision.is_none();
                                if ui
                                    .checkbox(&mut automatic, l10n::text(ui.ctx(), "Automatic"))
                                    .changed()
                                {
                                    edit.settings.precision =
                                        if automatic { None } else { Some(2) };
                                }
                                if let Some(precision) = &mut edit.settings.precision {
                                    ui.add(
                                        egui::DragValue::new(precision)
                                            .range(0..=12)
                                            .suffix(l10n::text(ui.ctx(), " decimals")),
                                    )
                                    .accessible_name(l10n::text(ui.ctx(), "Precision"));
                                }
                            });
                        }
                        Tab::Visibility => {
                            ui.label(l10n::text(ui.ctx(), "Show this study on these intervals:"));
                            let mut intervals = Timeframe::all();
                            for interval in self
                                .current_interval
                                .iter()
                                .chain(&edit.settings.hidden_intervals)
                            {
                                if !intervals.contains(interval) {
                                    intervals.push(*interval);
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                for interval in intervals {
                                    let mut visible =
                                        !edit.settings.hidden_intervals.contains(&interval);
                                    if ui
                                        .checkbox(
                                            &mut visible,
                                            l10n::text(ui.ctx(), interval.to_string()),
                                        )
                                        .changed()
                                    {
                                        edit.settings.hidden_intervals.retain(|tf| *tf != interval);
                                        if !visible {
                                            edit.settings.hidden_intervals.push(interval);
                                        }
                                    }
                                }
                            });
                        }
                    });
                let valid = self.inputs.iter().all(|input| input.accepts(&input.value));
                (apply, cancel) = DialogFooter::new("Apply")
                    .primary_enabled(valid)
                    .secondary("Cancel")
                    .show(ui);
            });
        if !open || cancel {
            self.edit = None;
            return None;
        }
        if apply {
            let mut edit = self.edit.take()?;
            edit.inputs = self
                .inputs
                .iter()
                .map(|input| (input.id.clone(), input.value.clone()))
                .collect();
            return Some(edit);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{ConfiguredStudy, SMA};

    #[test]
    fn dialog_follows_the_window_when_it_shrinks() {
        for theme in [Theme::light(), Theme::dark()] {
            for (name, tab) in [
                ("MA Ribbon", None),
                ("MA Ribbon", Some(Tab::Inputs)),
                ("MA Ribbon", Some(Tab::Style)),
                ("MA Ribbon", Some(Tab::Visibility)),
                ("Vol Profile FR", Some(Tab::Inputs)),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let study = crate::studies::IndicatorFactory::new()
                    .create(name)
                    .unwrap();
                let mut dialog = IndicatorSettingsDialog::default();
                dialog.open(SeriesId(2), study.as_ref(), Some(Timeframe::Min1));
                let mut browser = super::super::IndicatorDialog::new();
                browser.open();
                if let Some(tab) = tab {
                    dialog.tab = tab;
                }
                for size in [
                    egui::vec2(1440.0, 900.0),
                    egui::vec2(320.0, 360.0),
                    egui::vec2(600.0, 300.0),
                ] {
                    let size = if tab.is_none() {
                        egui::vec2(size.x.max(600.0), size.y.max(500.0))
                    } else {
                        size
                    };
                    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                    for _ in 0..4 {
                        ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            |ui| {
                                if tab.is_some() {
                                    dialog.show(ui.ctx(), &theme);
                                } else {
                                    browser.show(ui.ctx(), &theme);
                                }
                            },
                        )
                        .drop_without_applying_deltas();
                    }
                    let shown = ctx
                        .memory(|m| {
                            m.area_rect(egui::Id::unique(if tab.is_some() {
                                "indicator_settings"
                            } else {
                                "indicator_dialog"
                            }))
                        })
                        .unwrap();
                    assert!(
                        screen.contains_rect(shown),
                        "{name} tab {}: {shown:?} outside {screen:?}",
                        tab.map(|tab| tab as u8).unwrap_or(3)
                    );
                }
            }
        }
    }

    #[test]
    fn escape_discards_staged_inputs_and_presentation() {
        let study = ConfiguredStudy::new(Box::new(SMA::new(20)));
        let mut dialog = IndicatorSettingsDialog::default();
        dialog.open(SeriesId(2), &study, Some(Timeframe::Min1));
        dialog.inputs[0].value = InputValue::Length(2);
        dialog.edit.as_mut().unwrap().settings.plots[0].visible = false;
        let ctx = egui::Context::default();
        for escape in [false, true] {
            let events = if escape {
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            } else {
                Vec::new()
            };
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert!(dialog.show(ui.ctx(), &Theme::dark()).is_none());
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(!dialog.is_open());
        assert_eq!(study.inputs()[0].value, InputValue::Length(20));
        assert!(study.plot_style(0).visible);
    }
}
