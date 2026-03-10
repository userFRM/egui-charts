//! Canvas tab - background and grid settings.

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use crate::ui_kit::FormGrid;
use egui::Ui;

/// Canvas tab renderer
pub struct CanvasTab;

impl CanvasTab {
    /// Show the canvas tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

        ui.label("Background");
        ui.space_sm();

        FormGrid::new("canvas_bg_grid").show(ui, |ui| {
            ui.label("Type");
            ui.combo_select("bg_type", &mut settings.background_type, [0u8, 1], |v| {
                Self::background_type_label(*v).to_string()
            });
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        ui.label("Grid lines");
        ui.space_sm();

        FormGrid::new("canvas_grid_grid").show(ui, |ui| {
            ui.label("Vertical grid lines");
            ui.checkbox(&mut settings.show_vertical_grid, "");
            ui.end_row();

            ui.label("Horizontal grid lines");
            ui.checkbox(&mut settings.show_horizontal_grid, "");
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        ui.label("Crosshair");
        ui.space_sm();

        FormGrid::new("canvas_crosshair_grid").show(ui, |ui| {
            ui.label("Mode");
            ui.combo_select(
                "crosshair_mode",
                &mut settings.crosshair_mode,
                [0u8, 1, 2],
                |v| Self::crosshair_mode_label(*v).to_string(),
            );
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
