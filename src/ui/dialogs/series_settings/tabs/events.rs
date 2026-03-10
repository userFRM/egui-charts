//! Events tab - calendar events display settings.
//!
//! Requires economic calendar/events system (not yet implemented).

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use egui::Ui;

/// Events tab renderer
pub struct EventsTab;

impl EventsTab {
    /// Show the events tab content
    pub fn show(ui: &mut Ui, _settings: &mut SeriesSettings) {
        ui.space_xl();

        ui.vertical_centered(|ui| {
            ui.space_xl();
            ui.heading("Events");
            ui.space_md();
            ui.label("Coming soon");
            ui.space_sm();
            ui.label("Events system will enable:");
            ui.space_sm();

            ui.horizontal(|ui| {
                ui.space_xl();
                ui.vertical(|ui| {
                    ui.label("• Dividend markers");
                    ui.label("• Stock split markers");
                    ui.label("• Earnings announcements");
                    ui.label("• Economic calendar events");
                    ui.label("• News integration");
                });
            });

            ui.space_xl();
            ui.colored_label(
                ui.style().visuals.weak_text_color(),
                "Events/calendar integration pending",
            );
        });
    }
}
