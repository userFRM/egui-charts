//! Style tab - color, line width, line style.

use egui::{Color32, Ui};

use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::{DrawingLineStyle, DrawingProps};
use crate::ui_kit::FormGrid;

/// Style tab renderer
pub struct StyleTab;

impl StyleTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps) {
        ui.space_md();

        FormGrid::new("style_grid").show(ui, |ui| {
            // Color picker
            ui.label("Color:");
            ui.horizontal(|ui| {
                let mut color_arr = props.color.to_array();
                if ui
                    .color_edit_button_srgba_unmultiplied(&mut color_arr)
                    .changed()
                {
                    props.color = Color32::from_rgba_unmultiplied(
                        color_arr[0],
                        color_arr[1],
                        color_arr[2],
                        color_arr[3],
                    );
                }
            });
            ui.end_row();

            // Line width
            ui.label("Line Width:");
            ui.add(
                egui::Slider::new(&mut props.line_width, 1.0..=10.0)
                    .step_by(0.5)
                    .suffix("px"),
            );
            ui.end_row();

            // Line style
            ui.label("Line Style:");
            ui.combo_select(
                "line_style",
                &mut props.line_style,
                DrawingLineStyle::all(),
                |v| v.label().to_string(),
            );
            ui.end_row();

            // Fill options
            ui.label("Fill:");
            ui.checkbox(&mut props.fill_enabled, "Enable fill");
            ui.end_row();

            if props.fill_enabled {
                ui.label("Fill Color:");
                ui.horizontal(|ui| {
                    let fill = props
                        .fill_color
                        .unwrap_or(DESIGN_TOKENS.semantic.extended.accent.gamma_multiply(0.2));
                    let mut color_arr = fill.to_array();
                    if ui
                        .color_edit_button_srgba_unmultiplied(&mut color_arr)
                        .changed()
                    {
                        props.fill_color = Some(Color32::from_rgba_unmultiplied(
                            color_arr[0],
                            color_arr[1],
                            color_arr[2],
                            color_arr[3],
                        ));
                    }
                });
                ui.end_row();
            }

            // Extend options
            ui.label("Extend:");
            ui.horizontal(|ui| {
                ui.checkbox(&mut props.extend_left, "Left");
                ui.checkbox(&mut props.extend_right, "Right");
            });
            ui.end_row();

            // Show price
            ui.label("Labels:");
            ui.checkbox(&mut props.show_price, "Show price");
            ui.end_row();
        });
    }
}
