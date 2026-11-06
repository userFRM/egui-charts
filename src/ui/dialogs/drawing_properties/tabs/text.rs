//! Text tab: the text, its font, colour and style, and its box.

use crate::drawings::{
    DrawingToolType, FontWeight, LineTextHorizontalAlign, LineTextVerticalAlign,
};
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
    /// Tools with text to write and the trend angle, whose own labels have a font and a colour.
    pub(crate) fn available(tool: DrawingToolType) -> bool {
        tool.supports_text() || tool == DrawingToolType::TrendAngle
    }

    pub fn show(ui: &mut Ui, props: &mut DrawingProps, pickers: &mut [ColorPickerState; 3]) {
        ui.space_md();

        FormGrid::new("text_grid").show(ui, |ui| {
            let writes = (props.tool_settings.as_ref())
                .is_none_or(|template| template.settings.tool_type.supports_text());
            // Label text
            if writes {
                let label = ui.label(l10n::text(ui.ctx(), "Label"));
                ui.add(
                    egui::TextEdit::multiline(&mut props.label)
                        .desired_width(DESIGN_TOKENS.sizing.dialog.input_width * 2.0)
                        .desired_rows(3),
                )
                .labelled_by(label.id);
                ui.end_row();
            }

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
            let line_text = settings.tool_type == DrawingToolType::HorizontalLine;
            if writes && (line_text || settings.tool_type.supports_line_measurements()) {
                let measurements = &mut settings.line_measurements;
                let label = ui.label(l10n::text(ui.ctx(), "Horizontal alignment"));
                ui.label_inputs_by(label.id, |ui| {
                    let ctx = ui.ctx().clone();
                    ui.combo_select(
                        "line_text_horizontal_align",
                        &mut measurements.text_horizontal_align,
                        [
                            LineTextHorizontalAlign::Left,
                            LineTextHorizontalAlign::Center,
                            LineTextHorizontalAlign::Right,
                        ],
                        |align| {
                            l10n::text(
                                &ctx,
                                match align {
                                    LineTextHorizontalAlign::Left => "Left",
                                    LineTextHorizontalAlign::Center => "Center",
                                    LineTextHorizontalAlign::Right => "Right",
                                },
                            )
                        },
                    );
                });
                ui.end_row();

                let label = ui.label(l10n::text(ui.ctx(), "Vertical alignment"));
                ui.label_inputs_by(label.id, |ui| {
                    let ctx = ui.ctx().clone();
                    ui.combo_select(
                        "line_text_vertical_align",
                        &mut measurements.text_vertical_align,
                        // The dialog names the side of the line the text reads on, the setting the edge
                        // of the text that rests on it: "Top" is the text's bottom edge on the line.
                        [
                            LineTextVerticalAlign::Bottom,
                            LineTextVerticalAlign::Middle,
                            LineTextVerticalAlign::Top,
                        ],
                        |align| {
                            l10n::text(
                                &ctx,
                                match align {
                                    LineTextVerticalAlign::Bottom => "Top",
                                    LineTextVerticalAlign::Middle => "Middle",
                                    LineTextVerticalAlign::Top => "Bottom",
                                },
                            )
                        },
                    );
                });
                ui.end_row();
            }
            let boxed = has_text_box(settings.tool_type);
            if !(boxed
                || line_text
                || matches!(
                    settings.tool_type,
                    DrawingToolType::InfoLine | DrawingToolType::TrendAngle
                ))
            {
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
            let box_colors = shown.text_box_colors(ui.ctx());
            let [text_picker, background_picker, border_picker] = pickers;

            ui.label(l10n::text(ui.ctx(), "Text color"));
            if let Some(picked) = color_swatch(ui, text_picker, text, "drawing_props_text_color") {
                // Palette swatches are opaque: keep the text's opacity unless it was edited.
                let [r, g, b, a] = picked.to_srgba_unmultiplied();
                let a = if text_picker.custom_mode { a } else { text.a() };
                settings.text_color = Some([r, g, b, a]);
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

            if !boxed {
                return;
            }
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
                        box_colors.fill,
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
                        box_colors.border.color,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::Drawing;

    /// A horizontal line offers its text placement, colour and emphasis; a horizontal ray keeps
    /// the plain text settings it draws.
    #[test]
    fn horizontal_line_offers_its_text_settings() {
        let chart = crate::Chart::new(crate::model::BarData::default());
        for (tool, offered) in [
            (DrawingToolType::HorizontalLine, true),
            (DrawingToolType::HorizontalRay, false),
        ] {
            let mut props = DrawingProps::from_drawing(&Drawing::new(1, tool), &chart);
            let output = egui::Context::default().run_ui(Default::default(), |ui| {
                let mut pickers = [(); 3].map(|_| ColorPickerState::new(props.color));
                TextTab::show(ui, &mut props, &mut pickers);
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            for label in [
                "Horizontal alignment",
                "Vertical alignment",
                "Text color",
                "Bold",
                "Italic",
            ] {
                assert_eq!(labels.contains(&label), offered, "{tool:?} {label}");
            }
            output.drop_without_applying_deltas();
        }
    }
}
