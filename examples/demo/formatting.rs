//! Appearance and formatting samples applied through the public chart API.

use super::DemoApp;
use egui_charts::theme::Theme;

pub(super) fn load_theme_choice(storage: Option<&dyn eframe::Storage>) -> Option<String> {
    let storage = storage?;
    eframe::get_value::<String>(storage, "theme_choice")
        .or_else(|| {
            eframe::get_value::<bool>(storage, "theme_choice")
                .map(|dark| if dark { "dark" } else { "light" }.into())
        })
        .filter(|name| {
            egui_charts::theme::presets()
                .iter()
                .any(|(preset, _)| name == preset)
        })
}

pub(super) fn selected_theme(
    choice: Option<&str>,
    system: Option<egui::Theme>,
    fallback: bool,
) -> Theme {
    egui_charts::theme::presets()
        .into_iter()
        .find(|(name, _)| Some(*name) == choice)
        .map(|(_, theme)| theme)
        .unwrap_or_else(|| {
            if system.map_or(fallback, |theme| theme == egui::Theme::Dark) {
                Theme::dark()
            } else {
                Theme::light()
            }
        })
}

impl DemoApp {
    pub(super) fn show_appearance_controls(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Appearance", |ui| {
            let previous = self.theme_choice.clone();
            ui.radio_value(&mut self.theme_choice, None, "System");
            for (name, _) in egui_charts::theme::presets() {
                let label = match name {
                    "dark" => "Dark",
                    "light" => "Light",
                    _ => "High contrast",
                };
                ui.radio_value(&mut self.theme_choice, Some(name.into()), label);
            }
            if self.theme_choice != previous {
                self.update_theme(ui.ctx());
            }
        });
    }
}

