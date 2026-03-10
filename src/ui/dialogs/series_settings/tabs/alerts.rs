//! Alerts tab - alert display settings.
//!
//! Requires alerts system integration (not yet implemented).

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use egui::Ui;

/// Alerts tab renderer
pub struct AlertsTab;

impl AlertsTab {
    /// Show the alerts tab content
    pub fn show(ui: &mut Ui, _settings: &mut SeriesSettings) {
        ui.space_xl();

        ui.vertical_centered(|ui| {
            ui.space_xl();
            ui.heading("Alerts");
            ui.space_md();
            ui.label("Coming soon");
            ui.space_sm();
            ui.label("Alert system will enable:");
            ui.space_sm();

            ui.horizontal(|ui| {
                ui.space_xl();
                ui.vertical(|ui| {
                    ui.label("• Price alerts on chart");
                    ui.label("• Alert labels display");
                    ui.label("• Line style customization");
                    ui.label("• Sound notifications");
                    ui.label("• Push notifications");
                });
            });

            ui.space_xl();
            ui.colored_label(
                ui.style().visuals.weak_text_color(),
                "Alert system integration pending",
            );
        });
    }
}
