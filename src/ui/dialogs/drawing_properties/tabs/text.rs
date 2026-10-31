//! Text tab: the text, its font, colour and style, and its box.

use crate::drawings::{DrawingToolType, FontWeight};
use crate::localization as l10n;
use egui::Ui;

use super::style::color_swatch;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPickerState, FormGrid};

/// Font sizes
const FONT_SIZES: &[f32] = &[8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0];

/// Tools whose text sits in a box drawn from the text, background and border settings; tables,
/// posts and ideas keep the theme's colours for their text.
fn has_text_box(tool: DrawingToolType) -> bool {
    use DrawingToolType as T;
    matches!(
        tool,
        T::TextLabel
            | T::AnchoredText
            | T::Note
            | T::AnchoredNote
            | T::FlagNote
            | T::Callout
            | T::Comment
            | T::PriceNote
            | T::Signpost
    )
}

/// Text tab renderer
pub struct TextTab;

impl TextTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps, pickers: &mut [ColorPickerState; 3]) {
        ui.space_md();

        FormGrid::new("text_grid").show(ui, |ui| {
            // Label text
            let label = ui.label(l10n::text(ui.ctx(), "Label"));
            ui.add(
                egui::TextEdit::multiline(&mut props.label)
                    .desired_width(DESIGN_TOKENS.sizing.dialog.input_width * 2.0)
                    .desired_rows(3),
            )
            .labelled_by(label.id);
            ui.end_row();

            // Font size
            let label = ui.label(l10n::text(ui.ctx(), "Font size"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "font_size",
                    &mut props.font_size,
                    FONT_SIZES.iter().copied(),
                    |v| format!("{}px", *v as i32),
                );
            });
            ui.end_row();

            let DrawingProps {
                tool_settings: Some(template),
                color,
                fill_enabled,
                fill_color,
                ..
            } = props
            else {
                return;
            };
            let settings = &mut template.settings;
            if !has_text_box(settings.tool_type) {
                return;
            }
            // The swatches show the colours the chart draws with the edits so far.
            let mut shown = settings.clone();
            shown.color = color.to_srgba_unmultiplied();
            shown.fill_color = fill_enabled.then(|| {
                fill_color
                    .unwrap_or(color.gamma_multiply(0.2))
                    .to_srgba_unmultiplied()
            });
            let text = shown.text_box_colors(ui.ctx()).text;
            // A box switched on this frame shows its colours at once.
            shown.background = true;
            shown.border = true;
            let boxed = shown.text_box_colors(ui.ctx());
            let [text_picker, background_picker, border_picker] = pickers;

            ui.label(l10n::text(ui.ctx(), "Text color"));
            if let Some(picked) = color_swatch(ui, text_picker, text, "drawing_props_text_color") {
                settings.text_color = Some(picked.to_srgba_unmultiplied());
            }
            ui.end_row();

            ui.label(l10n::text(ui.ctx(), "Text style"));
            ui.horizontal(|ui| {
                let mut bold = settings.font_weight == FontWeight::Bold;
                if ui
                    .checkbox(&mut bold, l10n::text(ui.ctx(), "Bold"))
                    .changed()
                {
                    settings.font_weight = if bold {
                        FontWeight::Bold
                    } else {
                        FontWeight::Normal
                    };
                }
                ui.checkbox(&mut settings.italic, l10n::text(ui.ctx(), "Italic"));
            });
            ui.end_row();

            let label = ui.label(l10n::text(ui.ctx(), "Background"));
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut settings.background,
                    l10n::text(ui.ctx(), "Show background"),
                )
                .labelled_by(label.id);
                if settings.background
                    && let Some(picked) = color_swatch(
                        ui,
                        background_picker,
                        boxed.fill,
                        "drawing_props_background",
                    )
                {
                    *fill_enabled = true;
                    *fill_color = Some(picked);
                }
            });
            ui.end_row();

            let label = ui.label(l10n::text(ui.ctx(), "Border"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut settings.border, l10n::text(ui.ctx(), "Show border"))
                    .labelled_by(label.id);
                if settings.border
                    && let Some(picked) = color_swatch(
                        ui,
                        border_picker,
                        boxed.border.color,
                        "drawing_props_border",
                    )
                {
                    settings.border_color = Some(picked.to_srgba_unmultiplied());
                }
            });
            ui.end_row();
        });
    }
}
