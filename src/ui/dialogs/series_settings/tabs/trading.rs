//! Trading tab - trading-related display settings.
//!
//! Requires trading system integration (not yet implemented).

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use egui::Ui;

/// Trading tab renderer
pub struct TradingTab;

impl TradingTab {
    /// Show the trading tab content
    pub fn show(ui: &mut Ui, _settings: &mut SeriesSettings) {
        ui.space_xl();

        ui.vertical_centered(|ui| {
            ui.space_xl();
            ui.heading("Trading");
            ui.space_md();
            ui.label("Coming soon");
            ui.space_sm();
            ui.label("Trading integration will enable:");
            ui.space_sm();

            ui.horizontal(|ui| {
                ui.space_xl();
                ui.vertical(|ui| {
                    ui.label("• Show orders on chart");
                    ui.label("• Show executions on chart");
                    ui.label("• Show positions on chart");
                    ui.label("• Display P&L overlay");
                    ui.label("• Show average price line");
                });
            });

            ui.space_xl();
            ui.colored_label(
                ui.style().visuals.weak_text_color(),
                "Connect a broker to enable trading features",
            );
        });
    }
}
