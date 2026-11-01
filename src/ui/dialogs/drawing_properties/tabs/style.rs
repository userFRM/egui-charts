//! Style tab - color, line width, line style.

use crate::drawings::ArrowStyle;
use crate::localization as l10n;
use crate::model::LineStyle;
use egui::{Color32, Role, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo};

use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::buttons::ButtonSize;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid};

/// Line widths offered as swatches; the stepper next to them takes any width.
const LINE_WIDTHS: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

/// Style tab renderer
pub struct StyleTab;

impl StyleTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps, pickers: &mut [ColorPickerState; 2]) {
        let tool = props.tool_settings.as_ref().map(|t| t.settings.tool_type);
        let [color_picker, fill_picker] = pickers;
        ui.space_md();

        FormGrid::new("style_grid").show(ui, |ui| {
            ui.label(l10n::text(ui.ctx(), "Color"));
            if let Some(color) = color_swatch(ui, color_picker, props.color, "drawing_props_color")
            {
                props.color = color;
            }
            ui.end_row();

            ui.label(l10n::text(ui.ctx(), "Line width"));
            line_width_row(ui, &mut props.line_width);
            ui.end_row();

            let label = ui.label(l10n::text(ui.ctx(), "Line style"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "line_style",
                    &mut props.line_style,
                    LineStyle::all().iter().copied(),
                    |v| v.name().to_string(),
                );
            });
            ui.end_row();

            if let Some(template) = &mut props.tool_settings {
                let settings = &mut template.settings;
                if settings.tool_type.supports_line_ends() {
                    for (label, style) in [
                        ("Line start", &mut settings.arrow_start_style),
                        ("Line end", &mut settings.arrow_end_style),
                    ] {
                        let label = ui.label(l10n::text(ui.ctx(), label));
                        ui.label_inputs_by(label.id, |ui| {
                            // Keep host-supplied endpoint styles selectable when reopening them.
                            let mut choices = vec![ArrowStyle::None, ArrowStyle::Arrow];
                            if !choices.contains(style) {
                                choices.push(*style);
                            }
                            let ctx = ui.ctx().clone();
                            ui.combo_select(label.id, style, choices, |v| {
                                l10n::text(
                                    &ctx,
                                    match v {
                                        ArrowStyle::None => "Plain",
                                        ArrowStyle::Arrow => "Arrow",
                                        ArrowStyle::OpenArrow => "Open arrow",
                                        ArrowStyle::Circle => "Circle",
                                        ArrowStyle::Square => "Square",
                                        ArrowStyle::Diamond => "Diamond",
                                    },
                                )
                            });
                        });
                        ui.end_row();
                    }
                }
                if settings.tool_type.supports_middle_line() {
                    let label = ui.label(l10n::text(ui.ctx(), "Middle line"));
                    ui.checkbox(
                        &mut settings.channel_midline,
                        l10n::text(ui.ctx(), "Show middle line"),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                }
                let (price, time) = crate::drawings::Drawing::axis_labels_of(settings.tool_type);
                if price || time {
                    let label = ui.label(l10n::text(ui.ctx(), "Axis labels"));
                    ui.horizontal(|ui| {
                        if price {
                            ui.checkbox(
                                &mut settings.price_axis_label,
                                l10n::text(ui.ctx(), "Price"),
                            )
                            .labelled_by(label.id);
                        }
                        if price && time {
                            ui.add_space(
                                ui.active_theme().spacing.xxl - ui.spacing().item_spacing.x,
                            );
                        }
                        if time {
                            ui.checkbox(
                                &mut settings.time_axis_label,
                                l10n::text(ui.ctx(), "Time"),
                            )
                            .labelled_by(label.id);
                        }
                    });
                    ui.end_row();
                }
            }

            if tool.is_some_and(|tool| tool.supports_fill()) {
                let label = ui.label(l10n::text(ui.ctx(), "Fill"));
                ui.checkbox(&mut props.fill_enabled, l10n::text(ui.ctx(), "Enable fill"))
                    .labelled_by(label.id);
                ui.end_row();

                if props.fill_enabled {
                    ui.label(l10n::text(ui.ctx(), "Fill color"));
                    // A first fill is a light tint of the line colour.
                    let fill = props.fill_color.unwrap_or_else(|| {
                        let [r, g, b, _] = props.color.to_srgba_unmultiplied();
                        Color32::from_rgba_unmultiplied(r, g, b, 51)
                    });
                    props.fill_color = Some(fill);
                    if let Some(picked) = color_swatch(ui, fill_picker, fill, "drawing_props_fill")
                    {
                        // Palette swatches are opaque: keep the fill's opacity unless it was edited.
                        let [r, g, b, a] = picked.to_srgba_unmultiplied();
                        let a = if fill_picker.custom_mode { a } else { fill.a() };
                        props.fill_color = Some(Color32::from_rgba_unmultiplied(r, g, b, a));
                    }
                    ui.end_row();
                }
            }

            if tool.is_some_and(|tool| tool.supports_extend()) {
                ui.label(l10n::text(ui.ctx(), "Extend"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut props.extend_left, l10n::text(ui.ctx(), "Left"));
                    ui.add_space(ui.active_theme().spacing.xxl - ui.spacing().item_spacing.x);
                    ui.checkbox(&mut props.extend_right, l10n::text(ui.ctx(), "Right"));
                });
                ui.end_row();
            }
        });
    }
}

/// Kit colour swatch for `color`; returns the colour picked this frame.
pub(super) fn color_swatch(
    ui: &mut Ui,
    state: &mut ColorPickerState,
    color: Color32,
    id: &str,
) -> Option<Color32> {
    state.sel_color = color;
    ColorPicker::new(state, id).show(ui).inner
}

/// Swatches that draw each preset width, then a stepper for any width from 1 to 100 px; opening
/// it keeps a width outside that range until it is edited.
fn line_width_row(ui: &mut Ui, width: &mut f32) {
    let theme = ui.active_theme();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing.xs;
        for preset in LINE_WIDTHS {
            let size = Vec2::splat(ButtonSize::MD.pixels());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            if response.clicked() {
                *width = preset;
            }
            let selected = *width == preset;
            let translated_label = l10n::format_text(
                ui.ctx(),
                "{preset} px",
                &[("preset", format!("{}", preset))],
            );
            response.widget_info(|| {
                WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &translated_label)
            });

            let visuals = ui.visuals();
            let radius = ui.active_theme().radius.md;
            if selected {
                ui.painter()
                    .rect_filled(rect, radius, visuals.selection.bg_fill);
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(rect, radius, visuals.widgets.hovered.bg_fill);
            }
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, theme.ui.border_control),
                StrokeKind::Inside,
            );
            crate::styles::focus::draw_focus_ring(ui, &response, radius);
            let line = if selected {
                visuals.selection.stroke.color
            } else {
                visuals.text_color()
            };
            ui.painter().hline(
                rect.x_range().shrink(theme.spacing.md),
                rect.center().y,
                Stroke::new(preset, line),
            );
        }
        ui.add_space(theme.spacing.lg - theme.spacing.xs);
        ui.add(
            egui::DragValue::new(width)
                .range(1.0..=crate::drawings::validation::MAX_STROKE_WIDTH)
                .clamp_existing_to_range(false)
                .speed(0.5)
                .max_decimals(1)
                .custom_formatter(|v, _| v.to_string())
                .suffix(l10n::text(ui.ctx(), " px")),
        )
        .accessible_name(l10n::text(ui.ctx(), "Line width"));
    });
}
