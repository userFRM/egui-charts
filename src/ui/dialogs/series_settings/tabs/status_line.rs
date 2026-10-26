//! Status line tab - legend/status line display settings.

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use crate::ui_kit::FormGrid;
use egui::Ui;

/// Status line tab renderer
pub struct StatusLineTab;

impl StatusLineTab {
    /// Show the status line tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

        ui.label("Status line settings");
        ui.space_sm();

        FormGrid::new("status_line_grid").show(ui, |ui| {
            // Show OHLC values
            let label = ui.label("Show OHLC values");
            ui.checkbox(&mut settings.show_ohlc_values, "")
                .labelled_by(label.id);
            ui.end_row();

            // Show change
            let label = ui.label("Show change");
            ui.checkbox(&mut settings.show_change, "")
                .labelled_by(label.id);
            ui.end_row();

            // Show volume
            let label = ui.label("Show volume");
            ui.checkbox(&mut settings.show_volume, "")
                .labelled_by(label.id);
            ui.end_row();

            // Show bar change
            let label = ui.label("Show bar change");
            ui.checkbox(&mut settings.show_bar_change, "")
                .labelled_by(label.id);
            ui.end_row();
        });

        ui.space_xl();
        ui.label("Values format");
        ui.space_sm();

        FormGrid::new("values_format_grid").show(ui, |ui| {
            // Decimal places
            let label = ui.label("Decimal places");
            ui.add(egui::DragValue::new(&mut settings.decimal_places).range(0..=8))
                .labelled_by(label.id);
            ui.end_row();
        });
    }
}
