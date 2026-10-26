//! Scales and lines tab - price scale and line visibility settings.

use crate::chart::series::SeriesSettings;
use crate::localization as l10n;
use crate::scales::PriceScaleMode;
use crate::ui_kit::FormGrid;
use crate::ui_kit::ext::UiExt;
use egui::Ui;

/// Scales and lines tab renderer
pub struct ScalesTab;

impl ScalesTab {
    /// Show the scales and lines tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

        ui.label(l10n::text(ui.ctx(), "Price scale"));
        ui.space_sm();

        FormGrid::new("scales_price_grid").show(ui, |ui| {
            // Scale mode
            let label = ui.label(l10n::text(ui.ctx(), "Scale mode"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "scale_mode",
                    &mut settings.price_scale_mode,
                    [
                        PriceScaleMode::Normal,
                        PriceScaleMode::Logarithmic,
                        PriceScaleMode::Percentage,
                        PriceScaleMode::IndexedTo100,
                    ],
                    |v| Self::scale_mode_label(*v).to_string(),
                );
            });
            ui.end_row();

            // Invert scale
            let label = ui.label(l10n::text(ui.ctx(), "Invert scale"));
            ui.checkbox(&mut settings.invert_scale, "")
                .labelled_by(label.id);
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        ui.label(l10n::text(ui.ctx(), "Lines"));
        ui.space_sm();

        FormGrid::new("scales_lines_grid").show(ui, |ui| {
            // Show price line
            let label = ui.label(l10n::text(ui.ctx(), "Show last price line"));
            ui.checkbox(&mut settings.show_price_line, "")
                .labelled_by(label.id);
            ui.end_row();

            // Show previous close line
            let label = ui.label(l10n::text(ui.ctx(), "Show previous close line"));
            ui.checkbox(&mut settings.show_prev_close_line, "")
                .labelled_by(label.id);
            ui.end_row();
        });
    }

    fn scale_mode_label(mode: PriceScaleMode) -> &'static str {
        match mode {
            PriceScaleMode::Normal => "Regular",
            PriceScaleMode::Logarithmic => "Logarithmic",
            PriceScaleMode::Percentage => "Percentage",
            PriceScaleMode::IndexedTo100 => "Indexed to 100",
        }
    }
}
