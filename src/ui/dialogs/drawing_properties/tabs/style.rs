//! Style tab - color, line width, line style.

use crate::drawings::{ArrowStyle, Drawing, DrawingToolType, LineMeasurementPosition};
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
        // Positions style their lines, labels and zones from their own settings below.
        let position = matches!(
            tool,
            Some(DrawingToolType::LongPos | DrawingToolType::ShortPos)
        );
        ui.space_md();

        FormGrid::new("style_grid").show(ui, |ui| {
            if tool != Some(DrawingToolType::FibonacciChannel) && !position {
                ui.label(l10n::text(ui.ctx(), "Color"));
                color_picker.sel_color = props.color;
                let picker = ColorPicker::new(color_picker, "drawing_props_color");
                // Without tool settings only the paint colour is held, which reads back as
                // another hue at each opacity: the editor then keeps its own exact colour.
                let picked = if props.tool_settings.is_some() {
                    picker.show_unmultiplied(ui, props.color_rgba()).inner
                } else {
                    picker.show(ui).inner.map(|c| c.to_srgba_unmultiplied())
                };
                if let Some([r, g, b, a]) = picked {
                    // The custom editor sets the opacity; a palette swatch keeps the line's.
                    let a = if color_picker.custom_mode {
                        a
                    } else {
                        props.color.a()
                    };
                    let rgba = [r, g, b, a];
                    props.color = Color32::from_rgba_unmultiplied(r, g, b, a);
                    if let Some(template) = &mut props.tool_settings {
                        template.settings.color = rgba;
                    }
                }
                ui.end_row();
            }

            if !position
                && tool.is_none_or(|tool| {
                    tool.draws_line() && tool != DrawingToolType::FibonacciChannel
                })
            {
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

            let exact = props.color_rgba();
            if let Some(template) = &mut props.tool_settings {
                let settings = &mut template.settings;
                if matches!(
                    settings.tool_type,
                    DrawingToolType::FibonacciRetracement
                        | DrawingToolType::FibonacciCircles
                        | DrawingToolType::FibonacciSpeedResistanceArcs
                        | DrawingToolType::FibonacciExtension
                        | DrawingToolType::FibonacciChannel
                        | DrawingToolType::FibonacciWedge
                        | DrawingToolType::FibonacciTimeZones
                        | DrawingToolType::TrendBasedFibTime
                ) {
                    let mut config = settings.fibonacci_config().into_owned();
                    let original = config.clone();
                    ui.label(l10n::text(ui.ctx(), "All colours"));
                    fill_picker.sel_color = props.color;
                    if let Some([r, g, b, a]) = ColorPicker::new(fill_picker, "fibonacci_color")
                        .show_unmultiplied(ui, exact)
                        .inner
                    {
                        let line = if fill_picker.custom_mode || props.color.a() == 0 {
                            a
                        } else {
                            props.color.a()
                        };
                        props.color = Color32::from_rgba_unmultiplied(r, g, b, line);
                        settings.color = [r, g, b, line];
                        for level in &mut config.levels {
                            let opacity = if fill_picker.custom_mode {
                                a
                            } else {
                                level.color[3]
                            };
                            level.color = [r, g, b, opacity];
                        }
                        if let Some(fill) = props.fill_color {
                            let [r, g, b, _] = props.color.to_srgba_unmultiplied();
                            props.fill_color =
                                Some(Color32::from_rgba_unmultiplied(r, g, b, fill.a()));
                        }
                    }
                    ui.end_row();
                    if settings.tool_type != DrawingToolType::FibonacciChannel {
                        ui.label(l10n::text(
                            ui.ctx(),
                            if settings.tool_type == DrawingToolType::FibonacciTimeZones {
                                "Initial line"
                            } else {
                                "Trend line"
                            },
                        ));
                        ui.checkbox(&mut config.trend_visible, l10n::text(ui.ctx(), "Visible"))
                            .accessible_name(l10n::text(ui.ctx(), "Show construction line"));
                        ui.end_row();
                    }
                    ui.label(l10n::text(ui.ctx(), "Level width"));
                    line_width_row(ui, &mut config.level_width);
                    ui.end_row();
                    if config != original {
                        settings.fib_config = Some(config);
                    }
                }
                if settings.tool_type == DrawingToolType::FibonacciSpiral {
                    let mut config = settings.fibonacci_config().into_owned();
                    ui.label("");
                    if ui
                        .checkbox(
                            &mut config.spiral_counterclockwise,
                            l10n::text(ui.ctx(), "Counterclockwise"),
                        )
                        .changed()
                    {
                        settings.fib_config = Some(config);
                    }
                    ui.end_row();
                }
                if position {
                    let style = &mut settings.position_style;
                    let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
                    let shown = style.line_color.map_or_else(
                        || {
                            ui.active_theme()
                                .chart
                                .axis_text_secondary
                                .gamma_multiply(0.65)
                        },
                        rgba,
                    );
                    ui.label(l10n::text(ui.ctx(), "Lines"));
                    if let Some(picked) =
                        color_swatch(ui, color_picker, shown, "position_line_color")
                    {
                        // The theme's line is translucent; a colour picked over it starts opaque.
                        let current = style.line_color.map_or(Color32::WHITE, rgba);
                        style.line_color = Some(
                            keep_opacity(picked, current, color_picker.custom_mode)
                                .to_srgba_unmultiplied(),
                        );
                    }
                    ui.end_row();

                    ui.label(l10n::text(ui.ctx(), "Line width"));
                    line_width_row(ui, &mut style.line_width);
                    ui.end_row();

                    let label = ui.label(l10n::text(ui.ctx(), "Background"));
                    ui.checkbox(&mut style.fill, l10n::text(ui.ctx(), "Show background"))
                        .labelled_by(label.id);
                    ui.end_row();

                    let label = ui.label(l10n::text(ui.ctx(), "Border"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut style.border, l10n::text(ui.ctx(), "Show border"))
                            .labelled_by(label.id);
                        if style.border
                            && let Some(picked) = color_swatch(
                                ui,
                                fill_picker,
                                rgba(style.border_color),
                                "position_border_color",
                            )
                        {
                            style.border_color = keep_opacity(
                                picked,
                                rgba(style.border_color),
                                fill_picker.custom_mode,
                            )
                            .to_srgba_unmultiplied();
                        }
                    });
                    ui.end_row();

                    for (label, value) in [
                        (
                            "Keep position statistics visible",
                            &mut settings.position_readout.persistent,
                        ),
                        (
                            "Condensed statistics",
                            &mut settings.position_readout.condensed,
                        ),
                    ] {
                        ui.label("");
                        ui.checkbox(value, l10n::text(ui.ctx(), label));
                        ui.end_row();
                    }
                }
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
                if settings.tool_type == DrawingToolType::VerticalLine {
                    let label = ui.label(l10n::text(ui.ctx(), "Extend"));
                    ui.checkbox(
                        &mut settings.line_measurements.across_panes,
                        l10n::text(ui.ctx(), "Through all panes"),
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

            if let Some(tool) = tool.filter(|tool| tool.supports_fill() && !position) {
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
    use crate::model::BarData;
    use crate::ui_kit::color_picker::MAIN_COLORS;
    use crate::ui_kit::color_picker::tests::{click, drag_alpha, open_custom_editor, text_pos};
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

    /// Palette swatches retain each line's opacity, including shown and spare Fibonacci levels.
    #[test]
    fn palette_colours_keep_the_line_opacity() {
        for (tool, picker, alpha, fill) in [
            (DrawingToolType::Highlighter, 0, 51, None),
            (DrawingToolType::FibonacciRetracement, 1, 255, None),
            (DrawingToolType::FibonacciChannel, 1, 255, None),
            (DrawingToolType::FibonacciWedge, 1, 255, None),
            (
                DrawingToolType::FibonacciRetracement,
                1,
                255,
                Some([10, 20, 30, 51]),
            ),
        ] {
            let ctx = Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let chart = Chart::new(BarData::new());
            let mut drawing = Drawing::new(0, tool);
            drawing.fill_color = fill;
            if let Some(config) = &mut drawing.fib_config {
                for (i, level) in config.levels.iter_mut().enumerate() {
                    level.color[3] = if i % 2 == 0 { 51 } else { 153 };
                }
            }
            let mut props = DrawingProps::from_drawing(&drawing, &chart);
            let mut pickers: [ColorPickerState; 2] = Default::default();
            pickers[picker].is_open = true;
            let swatch = MAIN_COLORS[3];
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
            let [r, g, b, _] = swatch.to_srgba_unmultiplied();
            assert_eq!(props.color, Color32::from_rgba_unmultiplied(r, g, b, alpha));

            if matches!(
                tool,
                DrawingToolType::FibonacciRetracement
                    | DrawingToolType::FibonacciChannel
                    | DrawingToolType::FibonacciWedge
            ) {
                assert_eq!(
                    props.fill_color,
                    fill.map(|color| Color32::from_rgba_unmultiplied(r, g, b, color[3]))
                );
                assert!(
                    props
                        .tool_settings
                        .as_ref()
                        .unwrap()
                        .settings
                        .fib_config
                        .as_ref()
                        .unwrap()
                        .levels
                        .iter()
                        .enumerate()
                        .all(|(i, level)| level.color
                            == [r, g, b, if i % 2 == 0 { 51 } else { 153 }])
                );
            }
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

    /// The custom colour editor edits the colour as saved. From the dialog of the default
    /// highlighter, with a blue trend line also selected, an opacity dragged over several frames
    /// reaches both drawings and leaves each its hue, and typed channels give both the new hue.
    /// Properties a host assembled without tool settings hold only the paint colour: an opacity
    /// drag, even through 1%, leaves each hue, and a typed hue arrives as that colour holds it
    /// at 20%.
    #[test]
    fn custom_colour_edits_on_a_selection_keep_or_set_each_hue() {
        #[derive(Debug)]
        enum Edit {
            OpacityDragged(&'static [u8]),
            Typed([u8; 3]),
        }
        for (tool_settings, edit, want) in [
            (
                true,
                Edit::OpacityDragged(&[51, 102, 153, 204, 255]),
                [[242, 54, 69, 255], [41, 98, 255, 255]],
            ),
            (
                true,
                Edit::OpacityDragged(&[51, 96, 141, 186, 230]),
                [[242, 54, 69, 230], [41, 98, 255, 230]],
            ),
            (
                true,
                Edit::OpacityDragged(&[51, 141, 230, 192, 153]),
                [[242, 54, 69, 153], [41, 98, 255, 153]],
            ),
            (
                true,
                Edit::Typed([236, 60, 66]),
                [[236, 60, 66, 51], [236, 60, 66, 255]],
            ),
            (
                false,
                Edit::OpacityDragged(&[51, 102, 153, 204, 255]),
                [[242, 54, 69, 255], [41, 98, 255, 255]],
            ),
            (
                false,
                Edit::OpacityDragged(&[51, 96, 141, 186, 230]),
                [[242, 54, 69, 230], [41, 98, 255, 230]],
            ),
            (
                false,
                Edit::OpacityDragged(&[51, 20, 2, 60, 153]),
                [[242, 54, 69, 153], [41, 98, 255, 153]],
            ),
            (
                false,
                Edit::Typed([236, 60, 66]),
                [[235, 60, 65, 51], [235, 60, 65, 255]],
            ),
        ] {
            let ctx = Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut chart = Chart::new(BarData::new());
            let highlighter = Drawing::new(1, DrawingToolType::Highlighter);
            let line = Drawing::new(2, DrawingToolType::TrendLine);
            assert_eq!(
                [highlighter.color, line.color],
                [[242, 54, 69, 51], [41, 98, 255, 255]]
            );
            chart.drawings_mut().drawings = vec![highlighter, line];
            chart.drawings_mut().select_drawings([1, 2]);
            let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
            if !tool_settings {
                props.tool_settings = None;
            }
            let paint = props.color;
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
                let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
                output.drop_without_applying_deltas();
                shapes
            };
            let shapes = open_custom_editor(&mut frame, paint);
            match edit {
                Edit::OpacityDragged(path) => drag_alpha(&mut frame, &shapes, path),
                Edit::Typed(rgb) => {
                    for (channel, value) in ["R ", "G ", "B "].into_iter().zip(rgb) {
                        let shapes = frame(vec![]);
                        frame(click(text_pos(&shapes, channel)));
                        frame(vec![Event::Text(value.to_string())]);
                    }
                }
            }
            props
                .apply_to_chart(
                    crate::ui::dialogs::drawing_properties::actions::DrawingId(1),
                    &mut chart,
                )
                .unwrap();
            assert_eq!(
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| d.color)
                    .collect::<Vec<_>>(),
                want,
                "{edit:?}, tool settings: {tool_settings}"
            );
        }
    }
}
