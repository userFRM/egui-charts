//! Text tab: the text, its font, colour and style, and its box.

use crate::drawings::{
    DrawingToolType, FontWeight, LineTextHorizontalAlign, LineTextOrientation,
    LineTextVerticalAlign,
};
use crate::localization as l10n;
use egui::Ui;

use super::style::color_swatch;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::color_picker::keep_opacity;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPickerState, FormGrid};

/// Font sizes
const FONT_SIZES: &[f32] = &[8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0];

/// Tools whose text sits in a box drawn from the text, background and border settings; tables,
/// posts and ideas keep the theme's colours for their text.
pub(crate) fn has_text_box(tool: DrawingToolType) -> bool {
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

/// One row picking `value` from `options`, each with the name the row shows for it.
fn alignment_row<T: Copy + PartialEq, const N: usize>(
    ui: &mut Ui,
    label: &str,
    value: &mut T,
    options: [(T, &str); N],
) {
    let label_response = ui.label(l10n::text(ui.ctx(), label));
    ui.label_inputs_by(label_response.id, |ui| {
        let ctx = ui.ctx().clone();
        ui.combo_select(label, value, options.map(|(option, _)| option), |value| {
            let name = options.iter().find(|(option, _)| option == value);
            l10n::text(&ctx, name.map_or("", |(_, name)| *name))
        });
    });
    ui.end_row();
}

/// Text tab renderer
pub struct TextTab;

impl TextTab {
    /// Tools with text to write, and the trend angle, Fibonacci and position tools, whose own
    /// labels have a font and a colour.
    pub(crate) fn available(tool: DrawingToolType) -> bool {
        tool.supports_text()
            || matches!(
                tool,
                DrawingToolType::TrendAngle
                    | DrawingToolType::FibonacciRetracement
                    | DrawingToolType::FibonacciExtension
                    | DrawingToolType::FibonacciChannel
                    | DrawingToolType::LongPos
                    | DrawingToolType::ShortPos
            )
    }

    pub fn show(ui: &mut Ui, props: &mut DrawingProps, pickers: &mut [ColorPickerState; 3]) {
        ui.space_md();
        if let Some(template) = &mut props.tool_settings
            && matches!(
                template.settings.tool_type,
                DrawingToolType::FibonacciRetracement
                    | DrawingToolType::FibonacciExtension
                    | DrawingToolType::FibonacciChannel
            )
        {
            let mut config = template.settings.fibonacci_config().into_owned();
            let original = config.clone();
            FormGrid::new("fibonacci_text").show(ui, |ui| {
                let label = ui.label(l10n::text(ui.ctx(), "Font size"));
                ui.add(
                    egui::DragValue::new(&mut config.label_size)
                        .range(1.0..=256.0)
                        .suffix(" px"),
                )
                .labelled_by(label.id);
                ui.end_row();
                use LineTextHorizontalAlign as H;
                use LineTextVerticalAlign as V;
                let mut position = config.label_position.unwrap_or(if config.labels_on_left {
                    H::Left
                } else {
                    H::Right
                });
                alignment_row(
                    ui,
                    "Label position",
                    &mut position,
                    [
                        (H::Left, "Left"),
                        (H::Center, "Center"),
                        (H::Right, "Right"),
                    ],
                );
                if position
                    != config.label_position.unwrap_or(if config.labels_on_left {
                        H::Left
                    } else {
                        H::Right
                    })
                {
                    config.label_position = Some(position);
                }
                alignment_row(
                    ui,
                    "Label height",
                    &mut config.label_vertical,
                    [
                        (V::Bottom, "Top"),
                        (V::Middle, "Middle"),
                        (V::Top, "Bottom"),
                    ],
                );
                if template.settings.tool_type == DrawingToolType::FibonacciChannel {
                    return;
                }
                ui.label("");
                ui.checkbox(
                    &mut config.show_text,
                    l10n::text(ui.ctx(), "Show level text"),
                );
                ui.end_row();
                alignment_row(
                    ui,
                    "Text position",
                    &mut config.text_position,
                    [
                        (H::Left, "Left"),
                        (H::Center, "Center"),
                        (H::Right, "Right"),
                    ],
                );
                alignment_row(
                    ui,
                    "Text height",
                    &mut config.text_vertical,
                    [
                        (V::Bottom, "Top"),
                        (V::Middle, "Middle"),
                        (V::Top, "Bottom"),
                    ],
                );
            });
            if config != original {
                template.settings.fib_config = Some(config);
            }
            return;
        }
        if let Some(template) = &mut props.tool_settings
            && matches!(
                template.settings.tool_type,
                DrawingToolType::LongPos | DrawingToolType::ShortPos
            )
        {
            let target = template.settings.position_colors()[0];
            let style = &mut template.settings.position_style;
            let [text_picker, background_picker, _] = pickers;
            let rgba = |[r, g, b, a]: [u8; 4]| egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            FormGrid::new("position_text").show(ui, |ui| {
                let label = ui.label(l10n::text(ui.ctx(), "Font size"));
                ui.label_inputs_by(label.id, |ui| {
                    ui.combo_select(
                        "position_font_size",
                        &mut style.font_size,
                        FONT_SIZES.iter().copied(),
                        |v| format!("{}px", *v as i32),
                    );
                });
                ui.end_row();

                ui.label(l10n::text(ui.ctx(), "Text color"));
                let text = style.text_color.map_or(egui::Color32::WHITE, rgba);
                if let Some(picked) = color_swatch(ui, text_picker, text, "position_text_color") {
                    style.text_color = Some(
                        keep_opacity(picked, text, text_picker.custom_mode).to_srgba_unmultiplied(),
                    );
                }
                ui.end_row();

                // Until a plate colour is chosen, each label takes its level's colour.
                let label = ui.label(l10n::text(ui.ctx(), "Background"));
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut style.label_background,
                        l10n::text(ui.ctx(), "Show background"),
                    )
                    .labelled_by(label.id);
                    let plate = rgba(
                        style
                            .label_color
                            .unwrap_or([target[0], target[1], target[2], 255]),
                    );
                    if style.label_background
                        && let Some(picked) =
                            color_swatch(ui, background_picker, plate, "position_label_background")
                    {
                        style.label_color = Some(
                            keep_opacity(picked, plate, background_picker.custom_mode)
                                .to_srgba_unmultiplied(),
                        );
                    }
                });
                ui.end_row();
            });
            return;
        }

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
            let line_text = matches!(
                settings.tool_type,
                DrawingToolType::HorizontalLine
                    | DrawingToolType::HorizontalRay
                    | DrawingToolType::VerticalLine
            );
            if writes && (line_text || settings.tool_type.supports_line_measurements()) {
                use LineTextHorizontalAlign as H;
                use LineTextOrientation as O;
                use LineTextVerticalAlign as V;
                let measurements = &mut settings.line_measurements;
                let (along, across) = (
                    &mut measurements.text_horizontal_align,
                    &mut measurements.text_vertical_align,
                );
                // The dialog names the side of the line the text reads on, the setting the edge of
                // the text that rests on it: "Top" is the text's bottom edge on the line. Down a
                // vertical line the rows trade places: the text runs from the pane's bottom to its
                // top, and its top faces left.
                if settings.tool_type == DrawingToolType::VerticalLine {
                    alignment_row(
                        ui,
                        "Horizontal alignment",
                        across,
                        [
                            (V::Bottom, "Left"),
                            (V::Middle, "Center"),
                            (V::Top, "Right"),
                        ],
                    );
                    alignment_row(
                        ui,
                        "Vertical alignment",
                        along,
                        [
                            (H::Right, "Top"),
                            (H::Center, "Middle"),
                            (H::Left, "Bottom"),
                        ],
                    );
                    alignment_row(
                        ui,
                        "Text orientation",
                        &mut measurements.text_orientation,
                        [(O::Vertical, "Vertical"), (O::Horizontal, "Horizontal")],
                    );
                } else {
                    alignment_row(
                        ui,
                        "Horizontal alignment",
                        along,
                        [
                            (H::Left, "Left"),
                            (H::Center, "Center"),
                            (H::Right, "Right"),
                        ],
                    );
                    alignment_row(
                        ui,
                        "Vertical alignment",
                        across,
                        [
                            (V::Bottom, "Top"),
                            (V::Middle, "Middle"),
                            (V::Top, "Bottom"),
                        ],
                    );
                }
            }
            let boxed = has_text_box(settings.tool_type);
            if !(boxed
                || line_text
                || matches!(
                    settings.tool_type,
                    DrawingToolType::TrendLine
                        | DrawingToolType::Ray
                        | DrawingToolType::ExtendedLine
                        | DrawingToolType::InfoLine
                        | DrawingToolType::TrendAngle
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

    /// Line tools offer the text placement, colour and emphasis they draw, and a vertical line
    /// also offers the orientation of its text.
    #[test]
    fn line_tools_offer_the_text_settings_they_draw() {
        let chart = crate::Chart::new(crate::model::BarData::default());
        for (tool, offered) in [
            (DrawingToolType::HorizontalLine, true),
            (DrawingToolType::TrendLine, true),
            (DrawingToolType::Ray, true),
            (DrawingToolType::ExtendedLine, true),
            (DrawingToolType::HorizontalRay, true),
            (DrawingToolType::VerticalLine, true),
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
            assert_eq!(
                labels.contains(&"Text orientation"),
                tool == DrawingToolType::VerticalLine,
                "{tool:?}"
            );
            output.drop_without_applying_deltas();
        }
    }
}
