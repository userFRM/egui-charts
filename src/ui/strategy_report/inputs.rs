//! The input row shared by study settings and the strategy tester.

use crate::localization as l10n;
use crate::model::Timeframe;
use crate::studies::{InputKind, InputValue, StudyInput, StudySource};

pub(crate) fn show_input(
    ui: &mut egui::Ui,
    input: &mut StudyInput,
    sources: &[(StudySource, String)],
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
        (InputKind::Timeframe, InputValue::Timeframe(value)) => {
            let response = egui::ComboBox::from_id_salt(&input.id)
                .selected_text(l10n::text(
                    ui.ctx(),
                    value.map_or_else(|| "Chart interval".to_owned(), |tf| tf.to_string()),
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, None, l10n::text(ui.ctx(), "Chart interval"));
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
        _ => ui.label(l10n::text(ui.ctx(), "This input is unavailable.")),
    };
    response.labelled_by(label.id);
    ui.end_row();
    previous != input.value
}
