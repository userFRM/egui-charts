//! The input row shared by study settings and the strategy tester.

use crate::localization as l10n;
use crate::model::Timeframe;
use crate::studies::{InputKind, InputValue, StudyInput, StudySource};
use chrono::{NaiveTime, Timelike};

pub(crate) fn show_input(
    ui: &mut egui::Ui,
    input: &mut StudyInput,
    sources: &[(StudySource, String)],
    zones: &[crate::model::TimezoneId],
) -> bool {
    let previous = input.value.clone();
    let label = ui.label(l10n::text(ui.ctx(), &input.label));
    let response = match (&input.kind, &mut input.value) {
        (InputKind::Length(min, max), InputValue::Length(value)) => {
            ui.add(egui::DragValue::new(value).range(*min..=*max))
        }
        (InputKind::Integer(min, max), InputValue::Integer(value)) => {
            ui.add(egui::DragValue::new(value).range(*min..=*max))
        }
        (InputKind::Number(min, max), InputValue::Number(value)) => {
            ui.add(egui::DragValue::new(value).speed(0.1).range(*min..=*max))
        }
        (InputKind::Choice(choices), InputValue::Choice(value)) => {
            egui::ComboBox::from_id_salt(&input.id)
                .selected_text(l10n::text(
                    ui.ctx(),
                    choices.get(*value).map_or("", String::as_str),
                ))
                .show_ui(ui, |ui| {
                    for (index, label) in choices.iter().enumerate() {
                        ui.selectable_value(value, index, l10n::text(ui.ctx(), label));
                    }
                })
                .response
        }
        (InputKind::Toggle, InputValue::Toggle(value)) => ui.checkbox(value, ""),
        (InputKind::Source, InputValue::Source(value)) => {
            egui::ComboBox::from_id_salt(&input.id)
                .selected_text(l10n::text(
                    ui.ctx(),
                    match value {
                        crate::studies::StudySource::Price(source) => source.label().to_owned(),
                        crate::studies::StudySource::Study { .. } => sources
                            .iter()
                            .find(|(source, _)| source == value)
                            .map(|(_, label)| label.clone())
                            .unwrap_or_else(|| "Unavailable study".to_owned()),
                    },
                ))
                .show_ui(ui, |ui| {
                    for (source, label) in sources {
                        ui.selectable_value(value, *source, l10n::text(ui.ctx(), label));
                    }
                    for source in crate::model::PriceSource::all() {
                        ui.selectable_value(
                            value,
                            crate::studies::StudySource::Price(source),
                            l10n::text(ui.ctx(), source.label()),
                        );
                    }
                })
                .response
        }
        (InputKind::Color, InputValue::Color(value)) => ui.color_edit_button_srgba(value),
        (InputKind::Symbol, InputValue::Symbol(value)) => ui
            .add(egui::TextEdit::singleline(value).hint_text(l10n::text(ui.ctx(), "Chart symbol"))),
        (InputKind::Text, InputValue::Text(value)) => ui.add(egui::TextEdit::singleline(value)),
        (InputKind::Timeframe | InputKind::AutoTimeframe(_), InputValue::Timeframe(value)) => {
            // Unset, the input follows the chart or reads the interval the study picks.
            let unset = match &input.kind {
                InputKind::AutoTimeframe(Some(picked)) => l10n::format_text(
                    ui.ctx(),
                    "Auto ({interval})",
                    &[("interval", l10n::text(ui.ctx(), picked.to_string()))],
                ),
                InputKind::AutoTimeframe(None) => l10n::text(ui.ctx(), "Auto"),
                _ => l10n::text(ui.ctx(), "Chart interval"),
            };
            let response = egui::ComboBox::from_id_salt(&input.id)
                .selected_text(
                    value.map_or_else(|| unset.clone(), |tf| l10n::text(ui.ctx(), tf.to_string())),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, None, unset);
                    for tf in Timeframe::all() {
                        ui.selectable_value(value, Some(tf), l10n::text(ui.ctx(), tf.to_string()));
                    }
                    if ui
                        .selectable_label(
                            value.is_some_and(|tf| tf.is_custom()),
                            l10n::text(ui.ctx(), "Custom"),
                        )
                        .clicked()
                    {
                        *value = Some(Timeframe::Custom(std::num::NonZeroU64::new(60).unwrap()));
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
                    *seconds = std::num::NonZeroU64::new(count).unwrap();
                }
            }
            response
        }
        (InputKind::TimeOfDay, InputValue::TimeOfDay(time)) => {
            ui.horizontal(|ui| {
                let (mut hours, mut minutes) = (time.hour(), time.minute());
                let name = l10n::text(ui.ctx(), &input.label);
                let two_digits = |value: f64, _| format!("{:02}", value as u32);
                let hours_field = ui
                    .add(
                        egui::DragValue::new(&mut hours)
                            .range(0..=23)
                            .custom_formatter(two_digits),
                    )
                    .accessible_name(l10n::format_text(
                        ui.ctx(),
                        "{input} hours",
                        &[("input", &name)],
                    ));
                ui.label(":");
                let minutes_field = ui
                    .add(
                        egui::DragValue::new(&mut minutes)
                            .range(0..=59)
                            .custom_formatter(two_digits),
                    )
                    .accessible_name(l10n::format_text(
                        ui.ctx(),
                        "{input} minutes",
                        &[("input", &name)],
                    ));
                *time = NaiveTime::from_hms_opt(hours, minutes, 0).unwrap_or(*time);
                hours_field | minutes_field
            })
            .inner
        }
        (InputKind::TimeZone { exchange }, InputValue::TimeZone(name)) => {
            let exchange_label = l10n::text(ui.ctx(), "Exchange time");
            egui::ComboBox::from_id_salt(&input.id)
                .selected_text(if name.is_empty() {
                    exchange_label.clone()
                } else {
                    name.clone()
                })
                .show_ui(ui, |ui| {
                    if *exchange {
                        ui.selectable_value(name, String::new(), exchange_label);
                    }
                    // A zone the list lacks keeps a row of its own, as the chart settings' list does.
                    if !name.is_empty() && !zones.iter().any(|zone| zone.name() == name.as_str()) {
                        let current = name.clone();
                        ui.selectable_value(name, current.clone(), current);
                    }
                    for zone in zones {
                        ui.selectable_value(name, zone.name().to_owned(), zone.name());
                    }
                })
                .response
        }
        _ => ui.label(l10n::text(ui.ctx(), "This input is unavailable.")),
    };
    // The two time fields already name their part of the input; the row label would replace it.
    if !matches!(input.kind, InputKind::TimeOfDay) {
        response.labelled_by(label.id);
    }
    ui.end_row();
    previous != input.value
}
