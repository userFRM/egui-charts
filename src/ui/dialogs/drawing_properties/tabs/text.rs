//! Text tab - labels and font settings.

use egui::Ui;

use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::FormGrid;

/// Font sizes
const FONT_SIZES: &[f32] = &[8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0];

/// Text tab renderer
pub struct TextTab;

impl TextTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps) {
        ui.space_md();

        FormGrid::new("text_grid").show(ui, |ui| {
            // Label text
            ui.label("Label:");
            ui.add(
                egui::TextEdit::multiline(&mut props.label)
                    .desired_width(DESIGN_TOKENS.sizing.dialog.input_width * 2.0)
                    .desired_rows(3),
            );
            ui.end_row();

            // Font size
            ui.label("Font Size:");
            ui.combo_select(
                "font_size",
                &mut props.font_size,
                FONT_SIZES.iter().copied(),
                |v| format!("{}px", *v as i32),
            );
            ui.end_row();
        });
    }
}
