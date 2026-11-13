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

pub(super) fn appearance_label(preset: &str) -> &'static str {
    match preset {
        "dark" => "Dark",
        "light" => "Light",
        _ => "High contrast",
    }
}

pub(super) fn selected_theme(
    choice: Option<&str>,
    system: Option<egui::Theme>,
    fallback: bool,
) -> Theme {
    // Runs every frame: build the presets only for an explicit choice.
    choice
        .and_then(|choice| {
            egui_charts::theme::presets()
                .into_iter()
                .find(|(name, _)| *name == choice)
        })
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
        let previous = self.theme_choice.clone();
        ui.radio_value(&mut self.theme_choice, None, "System");
        for (name, _) in egui_charts::theme::presets() {
            ui.radio_value(
                &mut self.theme_choice,
                Some(name.into()),
                appearance_label(name),
            );
        }
        if self.theme_choice != previous {
            self.update_theme(ui.ctx());
        }
    }
}

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct Formatting {
    locale: Option<String>,
    price: bool,
    percentage: bool,
    volume: bool,
}

impl Formatting {
    pub(super) fn apply(&self, chart: &mut egui_charts::Chart) {
        use egui_charts::scales::{CustomPriceFormatter, PriceFormatter};
        use std::sync::Arc;
        chart.set_locale(self.locale.clone());
        chart.set_price_formatter(self.price.then(|| {
            Arc::new(CustomPriceFormatter::new(|value| format!("{value:.3} u")))
                as Arc<dyn PriceFormatter>
        }));
        chart.set_percentage_formatter(self.percentage.then(|| {
            Arc::new(CustomPriceFormatter::new(|value| {
                format!("{value:+.2} pct")
            })) as Arc<dyn PriceFormatter>
        }));
        chart.set_volume_formatter(self.volume.then(|| {
            Arc::new(CustomPriceFormatter::new(|value| {
                format!("{value:.0} units")
            })) as Arc<dyn PriceFormatter>
        }));
    }
}

impl DemoApp {
    pub(super) fn show_formatting_controls(&mut self, ui: &mut egui::Ui) {
        let previous = self.formatting.clone();
        ui.label("Date label language");
        for (locale, name) in [
            (None, "English (default)"),
            (Some("de-DE"), "Deutsch"),
            (Some("fr-FR"), "Français"),
        ] {
            ui.radio_value(&mut self.formatting.locale, locale.map(str::to_owned), name);
        }
        ui.small("Time zone is independent: use the zone selector below the chart.");
        ui.separator();
        ui.checkbox(&mut self.formatting.price, "Price: 123.456 u");
        ui.checkbox(&mut self.formatting.percentage, "Percentage: +12.34 pct");
        ui.checkbox(&mut self.formatting.volume, "Volume: 123456 units");
        ui.small("Use the % scale button to see percentage axis labels.");
        if ui.button("Restore default formatting").clicked() {
            self.formatting = Formatting::default();
        }
        if self.formatting != previous {
            self.apply_formatting();
        }
    }
}
