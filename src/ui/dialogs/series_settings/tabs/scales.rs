//! Scales and lines tab - price scale and line visibility settings.

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use crate::scales::PriceScaleMode;
use crate::ui_kit::FormGrid;
use egui::Ui;

/// Scales and lines tab renderer
pub struct ScalesTab;

impl ScalesTab {
    /// Show the scales and lines tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

        ui.label("Price scale");
        ui.space_sm();

        FormGrid::new("scales_price_grid").show(ui, |ui| {
            // Scale mode
            ui.label("Scale mode");
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
            ui.end_row();

            // Invert scale
            ui.label("Invert scale");
            ui.checkbox(&mut settings.invert_scale, "");
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        ui.label("Lines");
        ui.space_sm();

        FormGrid::new("scales_lines_grid").show(ui, |ui| {
            // Show price line
            ui.label("Show last price line");
            ui.checkbox(&mut settings.show_price_line, "");
            ui.end_row();

            // Show previous close line
            ui.label("Show previous close line");
            ui.checkbox(&mut settings.show_prev_close_line, "");
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
