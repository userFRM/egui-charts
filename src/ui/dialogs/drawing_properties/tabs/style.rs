//! Style tab - color, line width, line style.

use crate::drawings::glyphs::GLYPHS;
use crate::drawings::{
    ArrowStyle, Drawing, DrawingToolType, FontWeight, GhostFeedConfig, LineMeasurementPosition,
    RangeSettings, StyleControls,
};
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
#[derive(Debug)]
pub struct StyleTab;

impl StyleTab {
    /// `badge_pickers` edit the text, background and border of a price label's badge, a price
    /// note's label or a range's readout label, card and border. `card_pickers` edit the
    /// background, text, border and anchor colours of a forecast, in the order of its thirteen
    /// rows.
    pub fn show(
        ui: &mut Ui,
        props: &mut DrawingProps,
        pickers: &mut [ColorPickerState; 3],
        badge_pickers: &mut [ColorPickerState; 3],
        card_pickers: &mut [ColorPickerState; 13],
        custom: Option<&crate::drawings::RegisteredDrawingTool>,
    ) {
        let style = custom.map_or_else(StyleControls::default, |tool| tool.style);
        let tool = props.tool_settings.as_ref().map(|t| t.settings.tool_type);
        // A price label and the label of a price note style their text, background and border
        // in the rows below; a price label has no other colour.
        let badge = matches!(
            tool,
            Some(DrawingToolType::PriceLabel | DrawingToolType::PriceNote)
        );
        let [color_picker, fill_picker, glyph_picker] = pickers;
        // Positions style their lines, labels and zones from their own settings below.
        let position = matches!(
            tool,
            Some(DrawingToolType::LongPos | DrawingToolType::ShortPos)
        );
        ui.space_md();

        FormGrid::new("style_grid").show(ui, |ui| {
            // The regression trend styles its fitted line, which takes the drawing's colour and
            // width, beside its bands.
            let regression = tool == Some(DrawingToolType::RegressionTrend);
            let projection = tool == Some(DrawingToolType::ProjectionTool);
            if projection {
                projection_rows(
                    ui,
                    props,
                    [&mut *color_picker, &mut *fill_picker, &mut *glyph_picker],
                );
            }
            if regression {
                regression_rows(
                    ui,
                    props,
                    [
                        &mut *color_picker,
                        &mut *glyph_picker,
                        &mut badge_pickers[1],
                    ],
                );
            }
            if tool.is_none_or(|tool| tool.supports_color())
                && tool != Some(DrawingToolType::PriceLabel)
                && !position
                && !regression
                && !projection
                && style.color
            {
                ui.label(l10n::text(ui.ctx(), "Color"));
                // Text with a colour of its own is painted in it.
                let text = props
                    .tool_settings
                    .as_ref()
                    .and_then(|template| template.settings.text_color_override());
                color_picker.sel_color = text.map_or(props.color, |[r, g, b, a]| {
                    Color32::from_rgba_unmultiplied(r, g, b, a)
                });
                let picker = ColorPicker::new(color_picker, "drawing_props_color");
                // Without tool settings only the paint colour is held, which reads back as
                // another hue at each opacity: the editor then keeps its own exact colour.
                let picked = if let Some(text) = text {
                    picker.show_unmultiplied(ui, text).inner
                } else if props.tool_settings.is_some() {
                    picker.show_unmultiplied(ui, props.color_rgba()).inner
                } else {
                    picker.show(ui).inner.map(|c| c.to_srgba_unmultiplied())
                };
                if let Some([r, g, b, a]) = picked {
                    // The custom editor sets the opacity, of the text where that is what the row
                    // edits; a palette swatch keeps the current one.
                    let custom = color_picker.custom_mode;
                    let line = if custom && text.is_none() {
                        a
                    } else {
                        props.color.a()
                    };
                    props.color = Color32::from_rgba_unmultiplied(r, g, b, line);
                    if let Some(template) = &mut props.tool_settings {
                        template.settings.color = [r, g, b, line];
                        if let Some(text) = text {
                            template.settings.text_color =
                                Some([r, g, b, if custom { a } else { text[3] }]);
                            props.color_row_text_pick = template.settings.text_color;
                        }
                    }
                }
                ui.end_row();
            }

            if tool.is_none_or(|tool| tool.supports_line_width())
                && !regression
                && !projection
                && style.line_width
            {
                ui.label(l10n::text(ui.ctx(), "Line width"));
                line_width_row(ui, &mut props.line_width);
                ui.end_row();
            }
            if tool.is_none_or(|tool| tool.supports_line_style()) && style.line_style {
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
                if settings.tool_type == DrawingToolType::Signpost {
                    let marker = &mut settings.marker;
                    let label = ui.label(l10n::text(ui.ctx(), "Emoji"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut marker.show_glyph, l10n::text(ui.ctx(), "Show emoji"))
                            .labelled_by(label.id);
                        if !marker.show_glyph {
                            return;
                        }
                        // Until a disc colour is chosen, the disc takes the drawing's.
                        let [r, g, b, a] = marker.glyph_color.unwrap_or(exact);
                        let disc = Color32::from_rgba_unmultiplied(r, g, b, a);
                        if let Some(picked) =
                            color_swatch(ui, glyph_picker, disc, "signpost_glyph_color")
                        {
                            marker.glyph_color = Some(
                                keep_opacity(picked, disc, glyph_picker.custom_mode)
                                    .to_srgba_unmultiplied(),
                            );
                        }
                        let emoji = || GLYPHS.iter().filter(|g| g.category.starts_with("Emoji"));
                        // Keep a host-supplied glyph selectable when reopening it.
                        let mut choices: Vec<String> = emoji().map(|g| g.id.to_owned()).collect();
                        if !choices.contains(&marker.glyph) {
                            choices.push(marker.glyph.clone());
                        }
                        let ctx = ui.ctx().clone();
                        let font = egui::TextStyle::Button.resolve(ui.style());
                        ui.combo_select("signpost_glyph", &mut marker.glyph, choices, |id| {
                            let Some(glyph) = emoji().find(|g| g.id == id) else {
                                return id.clone();
                            };
                            let name = l10n::text(&ctx, glyph.name);
                            // Name alone where the fonts cannot draw the emoji.
                            if ctx.fonts_mut(|f| f.has_glyphs(&font, id)) {
                                format!("{id} {name}")
                            } else {
                                name
                            }
                        });
                    });
                    ui.end_row();
                }
                if matches!(
                    settings.tool_type,
                    DrawingToolType::FontIcon | DrawingToolType::Sticker
                ) {
                    let label = ui.label(l10n::text(ui.ctx(), "Angle"));
                    let mut degrees = settings.glyph_angle.rem_euclid(360.0);
                    ui.label_inputs_by(label.id, |ui| {
                        if ui
                            .add(
                                egui::DragValue::new(&mut degrees)
                                    .range(0.0..=360.0)
                                    .speed(1.0)
                                    .max_decimals(1)
                                    .suffix("°"),
                            )
                            .changed()
                        {
                            settings.set_glyph_angle(degrees);
                        }
                    });
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
                if settings.tool_type == DrawingToolType::GhostFeed {
                    ghost_feed_rows(
                        ui,
                        &mut settings.ghost_feed,
                        [&mut *color_picker, &mut *fill_picker, &mut *glyph_picker],
                        badge_pickers,
                    );
                }
                if settings.tool_type == DrawingToolType::Forecast {
                    forecast_rows(ui, settings, card_pickers);
                }
                if settings.tool_type.supports_line_ends() && style.line_ends {
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
                    // A rectangle's middle line keeps the line colour, hairline and dashes.
                    let styled = settings.tool_type == DrawingToolType::ParallelChannel;
                    let label = ui.label(l10n::text(ui.ctx(), "Middle line"));
                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut settings.channel_midline,
                            l10n::text(ui.ctx(), "Show middle line"),
                        )
                        .labelled_by(label.id);
                        if styled && settings.channel_midline {
                            let [r, g, b, a] = settings.middle_line.color.unwrap_or(exact);
                            let shown = Color32::from_rgba_unmultiplied(r, g, b, a);
                            if let Some(picked) =
                                color_swatch(ui, glyph_picker, shown, "channel_middle_line_color")
                            {
                                settings.middle_line.color = Some(
                                    keep_opacity(picked, shown, glyph_picker.custom_mode)
                                        .to_srgba_unmultiplied(),
                                );
                            }
                        }
                    });
                    ui.end_row();
                    if styled && settings.channel_midline {
                        ui.label(l10n::text(ui.ctx(), "Middle line width"));
                        line_width_row(ui, &mut settings.middle_line.width);
                        ui.end_row();
                        let label = ui.label(l10n::text(ui.ctx(), "Middle line style"));
                        ui.label_inputs_by(label.id, |ui| {
                            ui.combo_select(
                                "middle_line_style",
                                &mut settings.middle_line.line_style,
                                LineStyle::all().iter().copied(),
                                |v| v.name().to_string(),
                            );
                        });
                        ui.end_row();
                    }
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

            if badge {
                badge_rows(ui, props, badge_pickers);
            }

            if let Some(tool) =
                tool.filter(|tool| tool.supports_fill() && !position && !badge && style.fill)
            {
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

            // A date and price range offers its extensions with its border, in `range_rows`.
            if tool.is_some_and(|tool| {
                tool.supports_extend() && tool != DrawingToolType::DateAndPriceRange
            }) {
                ui.label(l10n::text(ui.ctx(), "Extend"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut props.extend_left, l10n::text(ui.ctx(), "Left"));
                    ui.add_space(ui.active_theme().spacing.xxl - ui.spacing().item_spacing.x);
                    ui.checkbox(&mut props.extend_right, l10n::text(ui.ctx(), "Right"));
                });
                ui.end_row();
            }
            if let Some(template) = &mut props.tool_settings
                && matches!(
                    template.settings.tool_type,
                    DrawingToolType::Pitchfork
                        | DrawingToolType::SchiffPitchfork
                        | DrawingToolType::ModifiedSchiffPitchfork
                        | DrawingToolType::InsidePitchfork
                )
            {
                // A fork saved without levels shows the ones it is drawn with.
                let mut config = super::pitchfork_shown(&template.settings);
                let label = ui.label(l10n::text(ui.ctx(), "Extend lines"));
                if ui
                    .checkbox(&mut config.extend_lines, "")
                    .labelled_by(label.id)
                    .changed()
                {
                    template.settings.pitchfork = Some(config);
                }
                ui.end_row();
            }
            if let Some(template) = &mut props.tool_settings
                && template.settings.tool_type == DrawingToolType::Highlighter
            {
                let label = ui.label(l10n::text(ui.ctx(), "Smoothing"));
                // Up to 20 here; a larger value a host stored stays until edited.
                ui.add(
                    egui::DragValue::new(&mut template.settings.smoothing)
                        .range(0..=20)
                        .clamp_existing_to_range(false),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
            if let Some(template) = &mut props.tool_settings
                && matches!(
                    template.settings.tool_type,
                    DrawingToolType::DateRange | DrawingToolType::DateAndPriceRange
                )
            {
                range_rows(
                    ui,
                    template.settings.tool_type,
                    &mut template.settings.range_settings,
                    (&mut props.extend_left, &mut props.extend_right),
                    (exact, props.line_width, props.line_style),
                    badge_pickers,
                    glyph_picker,
                );
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
        if let Some(tool) = custom
            && let Some(template) = &mut props.tool_settings
        {
            ui.space_md();
            tool.implementation
                .settings(ui, &mut template.settings.custom_settings);
        }
    }
}

/// Rows for the badge of a price label or the label of a price note: its text colour, font size
/// and, on a note, emphasis, then its background and border. A palette colour keeps the opacity
/// in use; the custom editor sets it.
fn badge_rows(ui: &mut Ui, props: &mut DrawingProps, pickers: &mut [ColorPickerState; 3]) {
    let DrawingProps {
        tool_settings: Some(template),
        color,
        fill_enabled,
        fill_color,
        font_size,
        ..
    } = props
    else {
        return;
    };
    let settings = &mut template.settings;
    let note = settings.tool_type == DrawingToolType::PriceNote;
    let name = |label: &'static str, on_note: &'static str| if note { on_note } else { label };
    // The swatches show the colours the chart draws with the edits so far.
    let mut shown = settings.clone();
    shown.color = color.to_srgba_unmultiplied();
    shown.fill_color = fill_color
        .filter(|_| *fill_enabled)
        .map(|fill| fill.to_srgba_unmultiplied());
    let (text, fill, border) = if note {
        let text = shown.text_box_colors(ui.ctx()).text;
        // A box switched on this frame shows its colours at once.
        (shown.background, shown.border) = (true, true);
        let colors = shown.text_box_colors(ui.ctx());
        (text, colors.fill, colors.border.color)
    } else {
        let colors = shown.price_label_colors();
        // A label without a border shows its fill, which is what it draws there.
        let border = if colors.border.is_empty() {
            colors.fill
        } else {
            colors.border.color
        };
        (colors.text, colors.fill, border)
    };
    let [text_picker, background_picker, border_picker] = pickers;

    ui.label(l10n::text(ui.ctx(), name("Text color", "Label text")));
    if let Some(picked) = color_swatch(ui, text_picker, text, "drawing_props_badge_text") {
        settings.text_color =
            Some(keep_opacity(picked, text, text_picker.custom_mode).to_srgba_unmultiplied());
    }
    ui.end_row();

    let label = ui.label(l10n::text(ui.ctx(), name("Font size", "Label font size")));
    ui.label_inputs_by(label.id, |ui| {
        ui.combo_select(
            "badge_font_size",
            font_size,
            super::text::FONT_SIZES.iter().copied(),
            |v| format!("{}px", *v as i32),
        );
    });
    ui.end_row();

    if note {
        ui.label(l10n::text(ui.ctx(), "Label style"));
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
    }

    let label = ui.label(l10n::text(ui.ctx(), name("Background", "Label background")));
    ui.horizontal(|ui| {
        if note {
            ui.checkbox(
                &mut settings.background,
                l10n::text(ui.ctx(), "Show background"),
            )
            .labelled_by(label.id);
        }
        if (!note || settings.background)
            && let Some(picked) = color_swatch(
                ui,
                background_picker,
                fill,
                "drawing_props_badge_background",
            )
        {
            *fill_enabled = true;
            *fill_color = Some(keep_opacity(picked, fill, background_picker.custom_mode));
        }
    });
    ui.end_row();

    let label = ui.label(l10n::text(ui.ctx(), name("Border", "Label border")));
    ui.horizontal(|ui| {
        if note {
            ui.checkbox(&mut settings.border, l10n::text(ui.ctx(), "Show border"))
                .labelled_by(label.id);
        }
        if (!note || settings.border)
            && let Some(picked) =
                color_swatch(ui, border_picker, border, "drawing_props_badge_border")
        {
            settings.border = true;
            settings.border_color = Some(
                keep_opacity(picked, border, border_picker.custom_mode).to_srgba_unmultiplied(),
            );
        }
    });
    ui.end_row();
}

/// Rows for a range's border, readout label and card, and individual readouts, with its edge
/// extensions while the border is shown. Palette colours keep the current opacity; the custom
/// editor sets it.
fn range_rows(
    ui: &mut Ui,
    tool: DrawingToolType,
    settings: &mut RangeSettings,
    (extend_left, extend_right): (&mut bool, &mut bool),
    (color, width, style): ([u8; 4], f32, LineStyle),
    pickers: &mut [ColorPickerState; 3],
    shadow_picker: &mut ColorPickerState,
) {
    let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
    let [text_picker, background_picker, border_picker] = pickers;
    let label = ui.label(l10n::text(ui.ctx(), "Border"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut settings.border, l10n::text(ui.ctx(), "Show border"))
            .labelled_by(label.id);
        let shown = rgba(settings.border_color.unwrap_or(color));
        if settings.border
            && let Some(picked) = color_swatch(ui, border_picker, shown, "range_border_color")
        {
            settings.border_color = Some(
                keep_opacity(picked, shown, border_picker.custom_mode).to_srgba_unmultiplied(),
            );
        }
    });
    ui.end_row();
    if settings.border {
        ui.label(l10n::text(ui.ctx(), "Border width"));
        let shown = settings.border_width.unwrap_or(width);
        let mut edited = shown;
        line_width_row(ui, &mut edited);
        if edited != shown {
            settings.border_width = Some(edited);
        }
        ui.end_row();
        let label = ui.label(l10n::text(ui.ctx(), "Border style"));
        let shown = settings.border_style.unwrap_or(style);
        let mut edited = shown;
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                "range_border_style",
                &mut edited,
                LineStyle::all().iter().copied(),
                |v| v.name().to_string(),
            );
        });
        if edited != shown {
            settings.border_style = Some(edited);
        }
        ui.end_row();
    }
    // The switches extend the border's edges, so they show with it.
    if settings.border {
        ui.label(l10n::text(ui.ctx(), "Extend"));
        ui.horizontal(|ui| {
            let gap = ui.active_theme().spacing.xxl - ui.spacing().item_spacing.x;
            if tool == DrawingToolType::DateAndPriceRange {
                ui.checkbox(extend_left, l10n::text(ui.ctx(), "Left"));
                ui.add_space(gap);
                ui.checkbox(extend_right, l10n::text(ui.ctx(), "Right"));
                ui.add_space(gap);
            }
            ui.checkbox(&mut settings.extend_top, l10n::text(ui.ctx(), "Top"));
            ui.add_space(gap);
            ui.checkbox(&mut settings.extend_bottom, l10n::text(ui.ctx(), "Bottom"));
        });
        ui.end_row();
    }
    ui.label(l10n::text(ui.ctx(), "Label"));
    ui.horizontal(|ui| {
        let shown = settings
            .text_color
            .map_or(ui.active_theme().chart.axis_text, rgba);
        if let Some(picked) = color_swatch(ui, text_picker, shown, "range_label_color") {
            settings.text_color =
                Some(keep_opacity(picked, shown, text_picker.custom_mode).to_srgba_unmultiplied());
        }
        ui.combo_select(
            "range_label_font_size",
            &mut settings.font_size,
            super::text::FONT_SIZES.iter().copied(),
            |v| format!("{}px", *v as i32),
        );
    });
    ui.end_row();
    let label = ui.label(l10n::text(ui.ctx(), "Label background"));
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut settings.background,
            l10n::text(ui.ctx(), "Show background"),
        )
        .labelled_by(label.id);
        let shown = settings
            .background_color
            .map_or(ui.active_theme().chart.bg_tooltip, rgba);
        if settings.background
            && let Some(picked) =
                color_swatch(ui, background_picker, shown, "range_label_background")
        {
            settings.background_color = Some(
                keep_opacity(picked, shown, background_picker.custom_mode).to_srgba_unmultiplied(),
            );
        }
    });
    ui.end_row();
    ui.label(l10n::text(ui.ctx(), "Label shadow"));
    let shown = rgba(settings.shadow_color);
    if let Some(picked) = color_swatch(ui, shadow_picker, shown, "range_label_shadow") {
        settings.shadow_color =
            keep_opacity(picked, shown, shadow_picker.custom_mode).to_srgba_unmultiplied();
    }
    ui.end_row();
    for (label, value) in [
        ("Price range", &mut settings.price_change),
        ("Percent change", &mut settings.percent_change),
        ("Change in pips or ticks", &mut settings.pip_change),
        ("Bar range", &mut settings.bar_count),
        ("Date and time range", &mut settings.elapsed_time),
        ("Volume", &mut settings.volume),
    ]
    .into_iter()
    .skip(if tool == DrawingToolType::DateRange {
        3
    } else {
        0
    }) {
        ui.label(l10n::text(ui.ctx(), label));
        ui.checkbox(value, l10n::text(ui.ctx(), "Show"))
            .accessible_name(l10n::text(ui.ctx(), label));
        ui.end_row();
    }
}

/// The ghost feed's candle rows: the body colours, the borders and the wick, each with a switch,
/// then the transparency. A ghost feed shows no colour, fill or badge rows, so its five swatches
/// use those rows' pickers. A palette colour keeps the opacity in use; the custom editor sets it.
fn ghost_feed_rows(
    ui: &mut Ui,
    feed: &mut Option<GhostFeedConfig>,
    [body_up, body_down, wick]: [&mut ColorPickerState; 3],
    [border_up, border_down, _]: &mut [ColorPickerState; 3],
) {
    let mut config = feed.unwrap_or_default();
    let original = config;
    let swatch = |ui: &mut Ui, picker: &mut ColorPickerState, color: &mut [u8; 4], id: &str| {
        let [r, g, b, a] = *color;
        let shown = Color32::from_rgba_unmultiplied(r, g, b, a);
        if let Some(picked) = color_swatch(ui, picker, shown, id) {
            *color = keep_opacity(picked, shown, picker.custom_mode).to_srgba_unmultiplied();
        }
    };
    ui.label(l10n::text(ui.ctx(), "Body"));
    ui.horizontal(|ui| {
        swatch(ui, body_up, &mut config.body_up, "ghost_feed_body_up");
        swatch(ui, body_down, &mut config.body_down, "ghost_feed_body_down");
    });
    ui.end_row();
    let label = ui.label(l10n::text(ui.ctx(), "Borders"));
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut config.show_border,
            l10n::text(ui.ctx(), "Show borders"),
        )
        .labelled_by(label.id);
        if config.show_border {
            swatch(ui, border_up, &mut config.border_up, "ghost_feed_border_up");
            swatch(
                ui,
                border_down,
                &mut config.border_down,
                "ghost_feed_border_down",
            );
        }
    });
    ui.end_row();
    let label = ui.label(l10n::text(ui.ctx(), "Wick"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.show_wick, l10n::text(ui.ctx(), "Show wick"))
            .labelled_by(label.id);
        if config.show_wick {
            swatch(ui, wick, &mut config.wick_color, "ghost_feed_wick");
        }
    });
    ui.end_row();
    let label = ui.label(l10n::text(ui.ctx(), "Transparency"));
    ui.label_inputs_by(label.id, |ui| {
        ui.add(
            egui::DragValue::new(&mut config.transparency)
                .range(0..=100)
                .suffix("%"),
        )
    });
    ui.end_row();
    // Written only when edited, so that an untouched feed keeps `None` and the apply sees no edit.
    if config != original {
        *feed = Some(config);
    }
}

/// The forecast's card and badge rows: the background and text colours of the start card, the
/// target card and the success, failure and pending badges, a border switch and colour for each
/// card, the anchor dots and the background transparency. Unset card colours show the active
/// theme's, and unset anchor dots the line colour. A palette colour keeps the opacity in use;
/// the custom editor sets it.
fn forecast_rows(ui: &mut Ui, settings: &mut Drawing, pickers: &mut [ColorPickerState; 13]) {
    let mut style = settings.forecast_style();
    let original = style;
    let [start, target, success, failure, pending] = style.cards(&ui.active_theme().chart);
    let anchor_shown = style
        .anchor_color
        .map_or(settings.color32(), |[r, g, b, a]| {
            Color32::from_rgba_unmultiplied(r, g, b, a)
        });
    // A border row switches its colour on and off; switched on, it starts in the text colour.
    let rows = [
        (
            "Start background",
            &mut style.start_background,
            start.background,
            false,
        ),
        ("Start text", &mut style.start_text, start.text, false),
        ("Start border", &mut style.start_border, start.text, true),
        (
            "Target background",
            &mut style.target_background,
            target.background,
            false,
        ),
        ("Target text", &mut style.target_text, target.text, false),
        ("Target border", &mut style.target_border, target.text, true),
        (
            "Success background",
            &mut style.success_background,
            success.background,
            false,
        ),
        ("Success text", &mut style.success_text, success.text, false),
        (
            "Failure background",
            &mut style.failure_background,
            failure.background,
            false,
        ),
        ("Failure text", &mut style.failure_text, failure.text, false),
        (
            "Pending background",
            &mut style.pending_background,
            pending.background,
            false,
        ),
        ("Pending text", &mut style.pending_text, pending.text, false),
        (
            "Anchor points",
            &mut style.anchor_color,
            anchor_shown,
            false,
        ),
    ];
    for ((name, color, shown, border), picker) in rows.into_iter().zip(pickers) {
        let label = ui.label(l10n::text(ui.ctx(), name));
        ui.horizontal(|ui| {
            if border {
                let mut show = color.is_some();
                if ui
                    .checkbox(&mut show, l10n::text(ui.ctx(), "Show border"))
                    .labelled_by(label.id)
                    .changed()
                {
                    *color = show.then(|| shown.to_srgba_unmultiplied());
                }
                if color.is_none() {
                    return;
                }
            }
            // The exact bytes are edited, so that a translucent colour keeps its hue.
            let current = color.unwrap_or_else(|| shown.to_srgba_unmultiplied());
            let [r, g, b, a] = current;
            picker.sel_color = Color32::from_rgba_unmultiplied(r, g, b, a);
            if let Some([r, g, b, a]) = ColorPicker::new(picker, ("forecast", name))
                .show_unmultiplied(ui, current)
                .inner
            {
                let a = if picker.custom_mode || current[3] == 0 {
                    a
                } else {
                    current[3]
                };
                *color = Some([r, g, b, a]);
            }
        });
        ui.end_row();
    }
    let label = ui.label(l10n::text(ui.ctx(), "Transparency"));
    ui.label_inputs_by(label.id, |ui| {
        ui.add(
            egui::DragValue::new(&mut style.transparency)
                .range(0..=100)
                .suffix("%"),
        )
    });
    ui.end_row();
    // Written only when edited, so that an untouched forecast keeps `None` and the apply sees no
    // edit.
    if style != original {
        settings.forecast = Some(style);
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

/// The regression trend's base, upper and lower line rows and its correlation switch. The base
/// line takes the drawing's colour and width; a band shows the drawing's width until one is set.
fn regression_rows(ui: &mut Ui, props: &mut DrawingProps, pickers: [&mut ColorPickerState; 3]) {
    let Some(template) = &props.tool_settings else {
        return;
    };
    let original = template.settings.regression_trend.unwrap_or_default();
    let mut config = original;
    let opened = props.color_rgba();
    let mut base = opened;
    let mut base_width = props.line_width;
    let [base_picker, upper_picker, lower_picker] = pickers;
    regression_line_row(
        ui,
        "Base line",
        base_picker,
        &mut config.show_base_line,
        &mut base,
        &mut config.line_style,
        &mut base_width,
    );
    for (name, line, picker) in [
        ("Upper line", &mut config.upper_line, upper_picker),
        ("Lower line", &mut config.lower_line, lower_picker),
    ] {
        let shown = line.width().unwrap_or(base_width);
        let mut width = shown;
        regression_line_row(
            ui,
            name,
            picker,
            &mut line.visible,
            &mut line.color,
            &mut line.line_style,
            &mut width,
        );
        if width != shown
            && let Some(set) = line.with_width(Some(width))
        {
            *line = set;
        }
    }
    let label = ui.label(l10n::text(ui.ctx(), "Correlation"));
    ui.checkbox(
        &mut config.show_correlation,
        l10n::text(ui.ctx(), "Show correlation"),
    )
    .labelled_by(label.id);
    ui.end_row();
    props.line_width = base_width;
    if base != opened {
        let [r, g, b, a] = base;
        props.color = Color32::from_rgba_unmultiplied(r, g, b, a);
    }
    if let Some(template) = &mut props.tool_settings {
        if base != opened {
            template.settings.color = base;
        }
        if config != original {
            template.settings.regression_trend = Some(config);
        }
    }
}

/// The projection's outline row, which takes the drawing's colour and width, and its background
/// switch with the colours at the sector's centre and arc. A palette colour keeps the
/// opacity in use; the custom editor sets it.
fn projection_rows(ui: &mut Ui, props: &mut DrawingProps, pickers: [&mut ColorPickerState; 3]) {
    let Some(template) = &props.tool_settings else {
        return;
    };
    let original = template.settings.projection_config().into_owned();
    let mut config = original.clone();
    let opened = props.color_rgba();
    let mut leg = opened;
    let mut width = props.line_width;
    let [leg_picker, near_picker, far_picker] = pickers;
    regression_line_row(
        ui,
        "Trend line",
        leg_picker,
        &mut config.trend_visible,
        &mut leg,
        &mut config.trend_line_style,
        &mut width,
    );
    let label = ui.checkbox(
        &mut config.show_background,
        l10n::text(ui.ctx(), "Background"),
    );
    ui.label_inputs_by(label.id, |ui| {
        ui.horizontal(|ui| {
            for (color, picker, id) in [
                (0, &mut *near_picker, "projection_background_near"),
                (1, &mut *far_picker, "projection_background_far"),
            ] {
                let [r, g, b, a] = config.background_colors[color];
                let shown = Color32::from_rgba_unmultiplied(r, g, b, a);
                if let Some(picked) = color_swatch(ui, picker, shown, id) {
                    config.background_colors[color] =
                        keep_opacity(picked, shown, picker.custom_mode).to_srgba_unmultiplied();
                }
            }
        });
    });
    ui.end_row();
    props.line_width = width;
    if leg != opened {
        let [r, g, b, a] = leg;
        props.color = Color32::from_rgba_unmultiplied(r, g, b, a);
    }
    if let Some(template) = &mut props.tool_settings {
        if leg != opened {
            template.settings.color = leg;
        }
        if config != original {
            template.settings.projection = Some(config);
        }
    }
}

/// One line's row, for the regression trend's lines and the projection's outline: its switch named
/// `name` in the label column, then its colour (a palette swatch keeps the opacity, the custom
/// editor sets it), pattern and width.
fn regression_line_row(
    ui: &mut Ui,
    name: &str,
    picker: &mut ColorPickerState,
    visible: &mut bool,
    color: &mut [u8; 4],
    line_style: &mut LineStyle,
    width: &mut f32,
) {
    let ctx = ui.ctx().clone();
    let label = ui.checkbox(visible, l10n::text(&ctx, name));
    ui.label_inputs_by(label.id, |ui| {
        ui.horizontal(|ui| {
            let [r, g, b, a] = *color;
            picker.sel_color = Color32::from_rgba_unmultiplied(r, g, b, a);
            if let Some([r, g, b, a]) = ColorPicker::new(picker, (name, "regression_line_color"))
                .show_unmultiplied(ui, *color)
                .inner
            {
                let a = if picker.custom_mode { a } else { color[3] };
                *color = [r, g, b, a];
            }
            ui.combo_select(
                (name, "regression_line_style"),
                line_style,
                LineStyle::all().iter().copied(),
                |v| l10n::text(&ctx, v.name()),
            );
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
    });
    ui.end_row();
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
                |ui| {
                    StyleTab::show(
                        ui,
                        &mut props,
                        &mut Default::default(),
                        &mut Default::default(),
                        &mut Default::default(),
                        None,
                    )
                },
            );
            output.drop_without_applying_deltas();
            assert_eq!(props.fill_color, Some(fill), "{tool:?}");
        }
    }

    /// Each of a forecast's thirteen card, badge and anchor wells sets its own colour; a palette
    /// colour keeps the opacity in use.
    #[test]
    fn forecast_wells_set_their_own_colours() {
        use crate::drawings::ForecastStyle;
        type Member = fn(&mut ForecastStyle) -> &mut Option<[u8; 4]>;
        let members: [Member; 13] = [
            |s| &mut s.start_background,
            |s| &mut s.start_text,
            |s| &mut s.start_border,
            |s| &mut s.target_background,
            |s| &mut s.target_text,
            |s| &mut s.target_border,
            |s| &mut s.success_background,
            |s| &mut s.success_text,
            |s| &mut s.failure_background,
            |s| &mut s.failure_text,
            |s| &mut s.pending_background,
            |s| &mut s.pending_text,
            |s| &mut s.anchor_color,
        ];
        // Every colour set, so that both border wells show and no well paints the swatch.
        let mut saved = ForecastStyle::default();
        for (i, member) in members.iter().enumerate() {
            *member(&mut saved) = Some([10 + i as u8, 20, 30, 128]);
        }
        let swatch = MAIN_COLORS[3];
        let [r, g, b, _] = swatch.to_srgba_unmultiplied();
        for (i, member) in members.iter().enumerate() {
            let ctx = Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let chart = Chart::new(BarData::new());
            let mut drawing = Drawing::new(1, DrawingToolType::Forecast);
            drawing.forecast = Some(saved);
            let mut props = DrawingProps::from_drawing(&drawing, &chart);
            let mut card_pickers: [ColorPickerState; 13] = Default::default();
            card_pickers[i].is_open = true;
            let mut frame = |events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1440.0, 1400.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        StyleTab::show(
                            ui,
                            &mut props,
                            &mut Default::default(),
                            &mut Default::default(),
                            &mut card_pickers,
                            None,
                        )
                    },
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
            frame(click(pos));
            let mut want = saved;
            *member(&mut want) = Some([r, g, b, 128]);
            assert_eq!(
                props.tool_settings.as_ref().unwrap().settings.forecast,
                Some(want),
                "well {i}"
            );
        }
    }

    /// Palette swatches retain each line's opacity, including shown and spare Fibonacci levels.
    #[test]
    fn palette_colours_keep_the_line_opacity() {
        for (tool, picker, alpha, fill) in [
            (DrawingToolType::Highlighter, 0, 51, None),
            (DrawingToolType::TextLabel, 0, 255, None),
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
            if tool == DrawingToolType::TextLabel {
                drawing.text_color = Some([242, 54, 69, 128]);
            }
            if let Some(config) = &mut drawing.fib_config {
                for (i, level) in config.levels.iter_mut().enumerate() {
                    level.color[3] = if i % 2 == 0 { 51 } else { 153 };
                }
            }
            let mut props = DrawingProps::from_drawing(&drawing, &chart);
            let mut pickers: [ColorPickerState; 3] = Default::default();
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
                    |ui| {
                        StyleTab::show(
                            ui,
                            &mut props,
                            &mut pickers,
                            &mut Default::default(),
                            &mut Default::default(),
                            None,
                        )
                    },
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
            if tool == DrawingToolType::TextLabel {
                assert_eq!(
                    props.tool_settings.as_ref().unwrap().settings.text_color,
                    Some([r, g, b, 128])
                );
            }

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
    /// highlighter stays at 51. Text with its own colour also propagates a pick equal to its
    /// drawing RGB, while accepting unchanged properties leaves every colour alone. Only an
    /// opacity edited in the custom editor changes the other Texts' text opacity. Text-tab picks
    /// reach only same-tool texts, including a pick equal to the drawing RGB.
    #[test]
    fn palette_colour_on_a_selection_keeps_each_line_opacity() {
        use DrawingToolType::{AnchoredText, TextLabel};

        let green = Some([8, 153, 129, 200]);
        let green_pick = Some(MAIN_COLORS[3]);
        let blue_pick = Some(MAIN_COLORS[6]);
        let mut observed = Vec::new();
        for (opened, alpha, text_tool, (peer_tool, peer_text), swatch, text_tab) in [
            (1, 51, TextLabel, (TextLabel, green), green_pick, false),
            (2, 51, TextLabel, (TextLabel, green), green_pick, false),
            (1, 0, TextLabel, (TextLabel, green), green_pick, false),
            (2, 0, TextLabel, (TextLabel, green), green_pick, false),
            (3, 51, TextLabel, (TextLabel, green), blue_pick, false),
            (3, 51, AnchoredText, (AnchoredText, green), blue_pick, false),
            (3, 51, TextLabel, (TextLabel, green), green_pick, false),
            (3, 51, TextLabel, (AnchoredText, green), green_pick, false),
            (3, 51, AnchoredText, (TextLabel, green), green_pick, false),
            (
                3,
                51,
                AnchoredText,
                (AnchoredText, green),
                green_pick,
                false,
            ),
            (3, 51, TextLabel, (AnchoredText, green), None, false),
            (3, 51, AnchoredText, (TextLabel, green), None, false),
            (3, 51, TextLabel, (TextLabel, None), green_pick, false),
            (3, 51, TextLabel, (TextLabel, None), None, false),
            (3, 51, TextLabel, (TextLabel, green), blue_pick, true),
            (3, 51, TextLabel, (TextLabel, green), green_pick, true),
            (3, 51, AnchoredText, (AnchoredText, green), blue_pick, true),
            (3, 51, AnchoredText, (AnchoredText, green), green_pick, true),
            (3, 51, TextLabel, (AnchoredText, green), blue_pick, true),
            (3, 51, TextLabel, (TextLabel, None), blue_pick, true),
        ] {
            let ctx = Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut chart = Chart::new(BarData::new());
            let mut highlighter = Drawing::new(1, DrawingToolType::Highlighter);
            let mut line = Drawing::new(2, DrawingToolType::TrendLine);
            assert_eq!((highlighter.color[3], line.color[3]), (51, 255));
            highlighter.color[3] = alpha;
            if opened == 3 {
                line.color = [242, 54, 69, 255];
            }
            let mut text = Drawing::new(3, text_tool);
            text.text_color = Some([242, 54, 69, 128]);
            let mut peer = Drawing::new(4, peer_tool);
            peer.text_color = peer_text;
            chart.drawings_mut().drawings = vec![highlighter, line, text, peer];
            chart.drawings_mut().select_drawings([1, 2, 3, 4]);
            let target = chart.drawings().drawings[opened - 1].clone();
            let mut props = DrawingProps::from_drawing(&target, &chart);
            let original: Vec<_> = chart
                .drawings()
                .drawings
                .iter()
                .map(|d| (d.color, d.text_color))
                .collect();
            props
                .apply_to_chart(
                    crate::ui::dialogs::drawing_properties::actions::DrawingId(opened),
                    &mut chart,
                )
                .unwrap();
            assert_eq!(
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| (d.color, d.text_color))
                    .collect::<Vec<_>>(),
                original,
                "unchanged properties on {text_tool:?}, drawing {opened}"
            );
            let mut pickers: [ColorPickerState; 3] = Default::default();
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
                    |ui| {
                        if text_tab {
                            super::super::TextTab::show(ui, &mut props, &mut pickers);
                        } else {
                            StyleTab::show(
                                ui,
                                &mut props,
                                &mut pickers,
                                &mut Default::default(),
                                &mut Default::default(),
                                None,
                            );
                        }
                    },
                );
                let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
                output.drop_without_applying_deltas();
                shapes
            };
            if let Some(swatch) = swatch {
                frame(vec![]);
                let pos = frame(vec![])
                    .iter()
                    .find_map(|s| match s {
                        Shape::Rect(r) if r.fill == swatch => Some(r.rect.center()),
                        _ => None,
                    })
                    .expect("the palette shows the swatch");
                frame(click(pos));
            } else {
                let shapes = open_custom_editor(
                    &mut frame,
                    Color32::from_rgba_unmultiplied(242, 54, 69, 128),
                );
                drag_alpha(&mut frame, &shapes, &[128, 64]);
            }
            props
                .apply_to_chart(
                    crate::ui::dialogs::drawing_properties::actions::DrawingId(opened),
                    &mut chart,
                )
                .unwrap();
            let want = if let Some(swatch) = swatch {
                let [r, g, b, _] = swatch.to_srgba_unmultiplied();
                if text_tab {
                    vec![
                        original[0].0,
                        original[1].0,
                        [r, g, b, 128],
                        if peer_tool == text_tool {
                            [r, g, b, 128]
                        } else {
                            peer_text.unwrap_or(original[3].0)
                        },
                    ]
                } else {
                    vec![
                        [r, g, b, alpha],
                        [r, g, b, 255],
                        [r, g, b, 128],
                        [r, g, b, peer_text.map_or(255, |color| color[3])],
                    ]
                }
            } else {
                let [r, g, b, _] = peer_text.unwrap_or([41, 98, 255, 255]);
                vec![
                    [242, 54, 69, alpha],
                    [242, 54, 69, 255],
                    [242, 54, 69, 64],
                    [r, g, b, 64],
                ]
            };
            observed.push((
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| {
                        if d.id >= 3 {
                            d.text_color.unwrap_or(d.color)
                        } else {
                            d.color
                        }
                    })
                    .collect::<Vec<_>>(),
                want,
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
                chart.drawings().drawings.iter().all(|d| {
                    d.color == [255, 255, 255, 128]
                        && (d.id < 3 || d.text_color.unwrap_or(d.color) == [255, 255, 255, 128])
                }),
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
            let mut pickers: [ColorPickerState; 3] = Default::default();
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
                    |ui| {
                        StyleTab::show(
                            ui,
                            &mut props,
                            &mut pickers,
                            &mut Default::default(),
                            &mut Default::default(),
                            None,
                        )
                    },
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

    /// Badges and ranges offer their border, label and readout settings on the Style tab, a
    /// range its extensions only once its border is shown; a palette colour keeps the border's
    /// opacity.
    #[test]
    fn price_label_and_note_style_their_badge() {
        let chart = Chart::new(BarData::new());
        let green = MAIN_COLORS[3];
        for (tool, offered, absent) in [
            (
                DrawingToolType::PriceLabel,
                &["Text color", "Font size", "Background", "Border"][..],
                &["Color", "Fill", "Label style"][..],
            ),
            (
                DrawingToolType::PriceNote,
                &[
                    "Color",
                    "Label text",
                    "Label font size",
                    "Label style",
                    "Label background",
                    "Label border",
                ][..],
                &["Fill"][..],
            ),
            (
                DrawingToolType::DateRange,
                &[
                    "Border",
                    "Border width",
                    "Border style",
                    "Extend",
                    "Top",
                    "Bottom",
                    "Label",
                    "Label background",
                    "Bar range",
                    "Label shadow",
                    "Date and time range",
                    "Volume",
                ][..],
                &[
                    "Left",
                    "Right",
                    "Price range",
                    "Percent change",
                    "Change in pips or ticks",
                ][..],
            ),
            (
                DrawingToolType::DateAndPriceRange,
                &[
                    "Border",
                    "Border width",
                    "Border style",
                    "Label",
                    "Label background",
                    "Price range",
                    "Label shadow",
                    "Percent change",
                    "Change in pips or ticks",
                    "Bar range",
                    "Date and time range",
                    "Volume",
                    "Left",
                    "Right",
                    "Top",
                    "Bottom",
                ][..],
                &[][..],
            ),
        ] {
            let ctx = Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut drawing = Drawing::new(0, tool);
            let range = matches!(
                tool,
                DrawingToolType::DateRange | DrawingToolType::DateAndPriceRange
            );
            if range {
                drawing.range_settings.border = false;
                drawing.range_settings.border_color = Some([10, 20, 30, 128]);
            } else {
                drawing.border_color = Some([10, 20, 30, 128]);
            }
            let mut props = DrawingProps::from_drawing(&drawing, &chart);
            let original = (drawing.text_color, props.fill_color);
            let mut badge_pickers: [ColorPickerState; 3] = Default::default();
            // A range's border swatch shows only with its border; the test opens it there.
            badge_pickers[2].is_open = !range;
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
                    |ui| {
                        StyleTab::show(
                            ui,
                            &mut props,
                            &mut Default::default(),
                            &mut badge_pickers,
                            &mut Default::default(),
                            None,
                        )
                    },
                );
                let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
                output.drop_without_applying_deltas();
                shapes
            };
            if range {
                frame(vec![]);
                let shapes = frame(vec![]);
                for row in ["Left", "Right", "Top", "Bottom", "Border width"] {
                    assert!(
                        !shapes.iter().any(|shape| matches!(
                            shape,
                            Shape::Text(text) if text.galley.job.text == row
                        )),
                        "{tool:?} hides {row} without a border"
                    );
                }
                frame(click(text_pos(&shapes, "Show border")));
                let shapes = frame(vec![]);
                let swatch = Color32::from_rgba_unmultiplied(10, 20, 30, 128);
                let swatch = shapes
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Rect(r) if r.fill == swatch => Some(r.rect.center()),
                        _ => None,
                    })
                    .expect("the border shows its swatch");
                frame(click(swatch));
            }
            frame(vec![]);
            let shapes = frame(vec![]);
            let rows: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            for row in offered {
                assert!(rows.contains(row), "{tool:?} offers {row}");
            }
            for row in absent {
                assert!(!rows.contains(row), "{tool:?} hides {row}");
            }
            let well = shapes
                .iter()
                .find_map(|shape| match shape {
                    Shape::Rect(r) if r.fill == green => Some(r.rect.center()),
                    _ => None,
                })
                .expect("the palette shows the swatch");
            frame(click(well));
            if range {
                // Close the palette before clicking the extension row beneath it.
                frame(click(Pos2::new(1300.0, 850.0)));
                let shapes = frame(vec![]);
                let top = shapes
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Text(text) if text.galley.job.text == "Top" => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .expect("the range offers Top");
                frame(click(top));
            }
            let settings = &props.tool_settings.as_ref().unwrap().settings;
            if range {
                assert!(
                    settings.range_settings.extend_top,
                    "{tool:?} extends at the top"
                );
            }
            assert_eq!(
                if range {
                    settings.range_settings.border_color
                } else {
                    settings.border_color
                },
                Some([76, 175, 80, 128]),
                "{tool:?}"
            );
            assert_eq!((settings.text_color, props.fill_color), original);
        }
    }

    /// An arrow's Style tab lists the trend line's statistics, and its Middle point, switched on
    /// there, gives the selected arrow a middle handle halfway between its anchors.
    #[test]
    fn arrow_offers_statistics_and_a_middle_handle() {
        use crate::drawings::services::HandleService;
        use crate::drawings::{ChartPoint, HandlePos};
        use crate::ui::dialogs::{DrawingId, DrawingPropertiesDialog, PropertiesTab};
        use egui_kittest::{Harness, kittest::Queryable};
        let mut chart = crate::widget::test_frames::chart(&[90.0; 20], &[210.0; 20]);
        let mut drawing = Drawing::new(7, DrawingToolType::Arrow);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(2.0, 100.0), ChartPoint::new(8.0, 200.0)];
        drawing.points = vec![Pos2::new(100.0, 300.0), Pos2::new(300.0, 100.0)];
        chart.drawings_mut().drawings.push(drawing);
        chart.drawings_mut().select_drawings([7]);
        let mut dialog = DrawingPropertiesDialog::new();
        // Tall enough to show the statistics rows, which come last, without scrolling.
        dialog.config.height = 1300.0;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 1400.0))
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, (chart, dialog): &mut (Chart, DrawingPropertiesDialog)| {
                    dialog.show_and_apply(ui.ctx(), chart);
                },
                (chart, dialog),
            );
        let (chart, dialog) = h.state_mut();
        dialog.open_for_chart(chart, DrawingId(7));
        dialog.active_tab = PropertiesTab::Style;
        h.run();
        for label in [
            "Price labels",
            "Keep statistics visible",
            "Price change",
            "Percentage change",
            "Pip change",
            "Bar count",
            "Elapsed time",
            "Length in periods",
            "Inclination",
        ] {
            h.get_by_role_and_label(Role::CheckBox, label);
        }
        h.get_by_label("Statistics placement");
        let handles = |h: &Harness<'_, (Chart, DrawingPropertiesDialog)>| {
            HandleService::get_handles(&h.state().0.drawings().drawings[0])
        };
        assert_eq!(handles(&h).len(), 2);
        h.get_by_role_and_label(Role::CheckBox, "Middle point")
            .click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Ok").click();
        h.run();
        assert_eq!(
            handles(&h).last(),
            Some(&(HandlePos::Middle, Pos2::new(200.0, 200.0)))
        );
    }

    /// Each switch of the regression trend's Style tab turns off its own line or the correlation
    /// and leaves the others shown.
    #[test]
    fn regression_trend_rows_switch_their_own_line() {
        let chart = Chart::new(BarData::new());
        for (row, shown) in [
            ("Base line", [false, true, true, true]),
            ("Upper line", [true, false, true, true]),
            ("Lower line", [true, true, false, true]),
            ("Show correlation", [true, true, true, false]),
        ] {
            let ctx = Context::default();
            let drawing = Drawing::new(1, DrawingToolType::RegressionTrend);
            let mut props = DrawingProps::from_drawing(&drawing, &chart);
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
                    |ui| {
                        StyleTab::show(
                            ui,
                            &mut props,
                            &mut Default::default(),
                            &mut Default::default(),
                            &mut Default::default(),
                            None,
                        )
                    },
                );
                let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
                output.drop_without_applying_deltas();
                shapes
            };
            frame(vec![]);
            let shapes = frame(vec![]);
            frame(click(text_pos(&shapes, row)));
            let config = props
                .tool_settings
                .as_ref()
                .and_then(|template| template.settings.regression_trend)
                .unwrap_or_default();
            assert_eq!(
                [
                    config.show_base_line,
                    config.upper_line.visible,
                    config.lower_line.visible,
                    config.show_correlation,
                ],
                shown,
                "{row}"
            );
        }
    }
}
