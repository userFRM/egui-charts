//! Canvas tab - background and grid settings.

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use crate::localization as l10n;
use crate::ui_kit::FormGrid;
use egui::Ui;

/// Canvas tab renderer
pub struct CanvasTab;

impl CanvasTab {
    /// Show the canvas tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

        ui.label(l10n::text(ui.ctx(), "Background"));
        ui.space_sm();

        FormGrid::new("canvas_bg_grid").show(ui, |ui| {
            let label = ui.label(l10n::text(ui.ctx(), "Type"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select("bg_type", &mut settings.background_type, [0u8, 1], |v| {
                    Self::background_type_label(*v).to_string()
                });
            });
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        ui.label(l10n::text(ui.ctx(), "Grid lines"));
        ui.space_sm();

        FormGrid::new("canvas_grid_grid").show(ui, |ui| {
            let label = ui.label(l10n::text(ui.ctx(), "Vertical grid lines"));
            ui.checkbox(&mut settings.show_vertical_grid, "")
                .labelled_by(label.id);
            ui.end_row();

            let label = ui.label(l10n::text(ui.ctx(), "Horizontal grid lines"));
            ui.checkbox(&mut settings.show_horizontal_grid, "")
                .labelled_by(label.id);
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        ui.label(l10n::text(ui.ctx(), "Crosshair"));
        ui.space_sm();

        FormGrid::new("canvas_crosshair_grid").show(ui, |ui| {
            let label = ui.label(l10n::text(ui.ctx(), "Mode"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "crosshair_mode",
                    &mut settings.crosshair_mode,
                    [0u8, 1, 2],
                    |v| Self::crosshair_mode_label(*v).to_string(),
                );
            });
            ui.end_row();
        });
    }

    fn background_type_label(bg_type: u8) -> &'static str {
        match bg_type {
            0 => "Solid",
            1 => "Gradient",
            _ => "Custom",
        }
    }

    fn crosshair_mode_label(mode: u8) -> &'static str {
        match mode {
            0 => "Full",
            1 => "Cross",
            _ => "None",
        }
    }
}
