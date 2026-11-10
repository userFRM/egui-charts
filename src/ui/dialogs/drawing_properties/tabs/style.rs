//! Style tab - color, line width, line style.

use crate::drawings::{ArrowStyle, Drawing, LineMeasurementPosition};
use crate::localization as l10n;
use crate::model::LineStyle;
use egui::{Color32, Role, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo};

use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::buttons::ButtonSize;
use crate::ui_kit::color_picker::keep_opacity;
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
                let rgba = if color_picker.custom_mode {
                    props.color = color;
                    props.color_rgba()
                } else {
                    // Keep the palette's RGB before translucent paint premultiplies it.
                    let [r, g, b, _] = color.to_srgba_unmultiplied();
                    [r, g, b, props.color.a()]
                };
                props.color = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
                if let Some(template) = &mut props.tool_settings {
                    template.settings.color = rgba;
                }
            }
            ui.end_row();

            if tool.is_none_or(|tool| tool.draws_line()) {
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
            }

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

            if let Some(tool) = tool.filter(|tool| tool.supports_fill()) {
                let label = ui.label(l10n::text(ui.ctx(), "Fill"));
                ui.checkbox(&mut props.fill_enabled, l10n::text(ui.ctx(), "Enable fill"))
                    .labelled_by(label.id);
                ui.end_row();

                if props.fill_enabled {
                    ui.label(l10n::text(ui.ctx(), "Fill color"));
                    let fill = props.fill_color.unwrap_or_else(|| {
                        let [r, g, b, a] =
                            Drawing::first_fill(tool, props.color.to_srgba_unmultiplied());
                        Color32::from_rgba_unmultiplied(r, g, b, a)
                    });
                    props.fill_color = Some(fill);
                    if let Some(picked) = color_swatch(ui, fill_picker, fill, "drawing_props_fill")
                    {
                        props.fill_color =
                            Some(keep_opacity(picked, fill, fill_picker.custom_mode));
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
            // Last, so that the long list of readouts keeps the other rows above the fold.
            if let Some(template) = &mut props.tool_settings
                && template.settings.tool_type.supports_line_measurements()
            {
                let measurements = &mut template.settings.line_measurements;
                for (label, value) in [
                    ("Middle point", &mut measurements.midpoint),
                    ("Price labels", &mut measurements.price_labels),
                    ("Keep statistics visible", &mut measurements.persistent),
                    ("Price change", &mut measurements.price_change),
                    ("Percentage change", &mut measurements.percent_change),
                    ("Pip change", &mut measurements.pip_change),
                    ("Bar count", &mut measurements.bar_count),
                    ("Elapsed time", &mut measurements.elapsed_time),
                    ("Length in periods", &mut measurements.length),
                    ("Inclination", &mut measurements.inclination),
                ] {
                    ui.label(l10n::text(ui.ctx(), label));
                    ui.checkbox(value, l10n::text(ui.ctx(), "Show"))
                        .accessible_name(l10n::text(ui.ctx(), label));
                    ui.end_row();
                }
                let label = ui.label(l10n::text(ui.ctx(), "Statistics placement"));
                ui.label_inputs_by(label.id, |ui| {
                    let ctx = ui.ctx().clone();
                    ui.combo_select(
                        "line_measurement_position",
                        &mut measurements.position,
                        [
                            LineMeasurementPosition::Left,
                            LineMeasurementPosition::Center,
                            LineMeasurementPosition::Right,
                            LineMeasurementPosition::Auto,
                        ],
                        |position| {
                            l10n::text(
                                &ctx,
                                match position {
                                    LineMeasurementPosition::Left => "Left",
                                    LineMeasurementPosition::Center => "Center",
                                    LineMeasurementPosition::Right => "Right",
                                    LineMeasurementPosition::Auto => "Auto",
                                },
                            )
                        },
                    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chart;
    use crate::drawings::DrawingToolType;
    use crate::model::BarData;
    use crate::ui_kit::color_picker::MAIN_COLORS;
    use egui::{Context, Event, Modifiers, PointerButton, Pos2, Rect, Shape};

    /// Switching Fill on starts from the tool's own fill where it differs from its line, as on
    /// the regression trend's upper band and the sector, and from a tint of the line elsewhere.
    #[test]
    fn enabling_fill_starts_from_the_tool_fill_or_a_tint_of_the_line() {
        let line = Color32::from_rgb(10, 20, 30);
        let blue = Color32::from_rgba_unmultiplied(41, 98, 255, 51);
        let tint = Color32::from_rgba_unmultiplied(10, 20, 30, 51);
        for (tool, fill) in [
            (DrawingToolType::RegressionTrend, blue),
            (DrawingToolType::Sector, blue),
            (DrawingToolType::ParallelChannel, tint),
            (DrawingToolType::Brush, tint),
        ] {
            let chart = Chart::new(BarData::new());
            let mut drawing = Drawing::new(0, tool);
            drawing.fill_color = None;
            let mut props = DrawingProps::from_drawing(&drawing, &chart);
            props.fill_enabled = true;
            props.color = line;
            let ctx = Context::default();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                    ..Default::default()
                },
                |ui| StyleTab::show(ui, &mut props, &mut Default::default()),
            );
            output.drop_without_applying_deltas();
            assert_eq!(props.fill_color, Some(fill), "{tool:?}");
        }
    }

    /// A palette swatch picked in the Style tab of one drawing in a selection recolours every
    /// selected drawing and leaves each at its own opacity: a trend line stays opaque and a
    /// highlighter stays at 51, whichever of the two the dialog was opened on.
    #[test]
    fn palette_colour_on_a_selection_keeps_each_line_opacity() {
        let swatch = MAIN_COLORS[3];
        let [r, g, b, _] = swatch.to_srgba_unmultiplied();
        let mut observed = Vec::new();
        for (opened, alpha) in [(1, 51), (2, 51), (1, 0), (2, 0)] {
            let ctx = Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut chart = Chart::new(BarData::new());
            let mut highlighter = Drawing::new(1, DrawingToolType::Highlighter);
            let line = Drawing::new(2, DrawingToolType::TrendLine);
            assert_eq!((highlighter.color[3], line.color[3]), (51, 255));
            highlighter.color[3] = alpha;
            chart.drawings_mut().drawings = vec![highlighter, line];
            chart.drawings_mut().select_drawings([1, 2]);
            let target = chart.drawings().drawings[opened - 1].clone();
            let mut props = DrawingProps::from_drawing(&target, &chart);
            let mut pickers: [ColorPickerState; 2] = Default::default();
            pickers[0].is_open = true;
            let mut frame = |events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| StyleTab::show(ui, &mut props, &mut pickers),
                );
                let well = output.shapes.iter().find_map(|s| match &s.shape {
                    Shape::Rect(r) if r.fill == swatch => Some(r.rect.center()),
                    _ => None,
                });
                output.drop_without_applying_deltas();
                well
            };
            frame(vec![]);
            let pos = frame(vec![]).expect("the palette shows the swatch");
            frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ]);
            props
                .apply_to_chart(
                    crate::ui::dialogs::drawing_properties::actions::DrawingId(opened),
                    &mut chart,
                )
                .unwrap();
            observed.push((
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| d.color)
                    .collect::<Vec<_>>(),
                vec![[r, g, b, alpha], [r, g, b, 255]],
            ));
            let mut custom =
                DrawingProps::from_drawing(&chart.drawings().drawings[opened - 1], &chart);
            custom.color = Color32::from_rgba_unmultiplied(255, 255, 255, 128);
            custom
                .apply_to_chart(
                    crate::ui::dialogs::drawing_properties::actions::DrawingId(opened),
                    &mut chart,
                )
                .unwrap();
            assert!(
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .all(|d| d.color == [255, 255, 255, 128]),
                "an edited opacity applies to the entire selection"
            );
        }
        assert!(
            observed.iter().all(|(got, want)| got == want),
            "{observed:?}"
        );
    }
}
