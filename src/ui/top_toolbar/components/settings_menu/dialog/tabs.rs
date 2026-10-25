//! Tab rendering implementations for SettingsDialog
//!
//! This module contains all the tab-specific UI rendering code,
//! keeping the main dialog module focused on core structure.

use egui::{Color32, FontId, RichText, Ui};

use super::SettingsDialog;
use crate::theme::components::SettingsDialogStyle;
use crate::ui_kit::ColorPicker;

use crate::styles::typography;

use super::super::data::LineStyle;
use super::super::types::{
    BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode,
};
use crate::tokens::DESIGN_TOKENS;

impl SettingsDialog {
    pub(super) fn draw_canvas_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                // CHART BASIC STYLES
                self.draw_section_header(ui, "CHART BASIC STYLES", style);

                // Background
                self.draw_setting_row_with_controls(ui, "Background", style, |ui, dialog| {
                    // Type dropdown
                    styled_dropdown(
                        ui,
                        "bg_type",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        dialog.settings.chart_basic_styles.background_type.name(),
                        |ui| {
                            for bt in BackgroundType::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.chart_basic_styles.background_type,
                                    *bt,
                                    bt.name(),
                                );
                            }
                        },
                    );

                    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);

                    // Color swatch(es)
                    match dialog.settings.chart_basic_styles.background_type {
                        BackgroundType::Solid => {
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background,
                                "bg_color",
                            )
                            .show(ui)
                            .inner
                            {
                                dialog.settings.chart_basic_styles.background_color = color;
                            }
                        }
                        _ => {
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background_gradient_top,
                                "bg_grad_top",
                            )
                            .show(ui)
                            .inner
                            {
                                dialog.settings.chart_basic_styles.background_gradient_top = color;
                            }
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background_gradient_bottom,
                                "bg_grad_bot",
                            )
                            .show(ui)
                            .inner
                            {
                                dialog
                                    .settings
                                    .chart_basic_styles
                                    .background_gradient_bottom = color;
                            }
                        }
                    }
                });

                // Grid lines
                self.draw_setting_row_with_controls(ui, "Grid lines", style, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "grid_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        dialog.settings.grid_lines.mode.name(),
                        |ui| {
                            for mode in GridLinesMode::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.grid_lines.mode,
                                    *mode,
                                    mode.name(),
                                );
                            }
                        },
                    );

                    // Color picker(s) based on mode
                    if matches!(
                        dialog.settings.grid_lines.mode,
                        GridLinesMode::Horizontal | GridLinesMode::Both
                    ) {
                        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);
                        if let Some(color) =
                            ColorPicker::new(&mut dialog.color_picker_states.grid_h, "grid_h")
                                .show(ui)
                                .inner
                        {
                            dialog.settings.grid_lines.horizontal_color = color;
                        }
                    }
                    if matches!(
                        dialog.settings.grid_lines.mode,
                        GridLinesMode::Vertical | GridLinesMode::Both
                    ) && let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.grid_v, "grid_v")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.grid_lines.vertical_color = color;
                    }
                });

                // Crosshair
                self.draw_setting_row_with_controls(ui, "Crosshair", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.crosshair, "crosshair")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.crosshair.color = color;
                    }

                    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);

                    // Line style with dashes preview
                    styled_dropdown(
                        ui,
                        "crosshair_style",
                        DESIGN_TOKENS.sizing.settings_dialog.small_dropdown_width,
                        style.val_text,
                        line_style_preview(dialog.settings.crosshair.line_style),
                        |ui| {
                            for ls in LineStyle::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.crosshair.line_style,
                                    *ls,
                                    line_style_preview(*ls),
                                );
                            }
                        },
                    );

                    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);

                    // Line width slider
                    let mut lw = dialog.settings.crosshair.line_width;
                    ui.add(
                        egui::DragValue::new(&mut lw)
                            .range(1.0..=4.0)
                            .speed(0.5)
                            .suffix(" px"),
                    );
                    dialog.settings.crosshair.line_width = lw;
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

                // WATERMARK
                self.draw_section_header(ui, "WATERMARK", style);

                self.draw_setting_row_with_controls(ui, "Mode", style, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "watermark_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        dialog.settings.watermark.mode.name(),
                        |ui| {
                            for wm in &[
                                WatermarkMode::ReplayMode,
                                WatermarkMode::Symbol,
                                WatermarkMode::SymbolAndDescription,
                            ] {
                                ui.selectable_value(
                                    &mut dialog.settings.watermark.mode,
                                    *wm,
                                    wm.name(),
                                );
                            }
                        },
                    );

                    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);

                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.watermark, "watermark")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.watermark.color = color;
                    }
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

                // SCALES APPEARANCE
                self.draw_section_header(ui, "SCALES APPEARANCE", style);

                self.draw_setting_row_with_controls(ui, "Text", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.scales_text, "scales_text")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.scales_appearance.text_color = color;
                    }
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

                // MARGINS (Pixel input fields)
                self.draw_section_header(ui, "MARGINS", style);

                self.draw_setting_row_with_controls(ui, "Top margin (%)", style, |ui, dialog| {
                    styled_text_input(
                        ui,
                        &mut dialog.margin_top_str,
                        DESIGN_TOKENS.sizing.settings_dialog.input_width_margin,
                    );
                    if let Ok(v) = dialog.margin_top_str.parse::<f32>() {
                        dialog.settings.margins.top_percent = v;
                    }
                });

                self.draw_setting_row_with_controls(
                    ui,
                    "Bottom margin (%)",
                    style,
                    |ui, dialog| {
                        styled_text_input(
                            ui,
                            &mut dialog.margin_bottom_str,
                            DESIGN_TOKENS.sizing.settings_dialog.input_width_margin,
                        );
                        if let Ok(v) = dialog.margin_bottom_str.parse::<f32>() {
                            dialog.settings.margins.bottom_percent = v;
                        }
                    },
                );

                self.draw_setting_row_with_controls(
                    ui,
                    "Right margin (bars)",
                    style,
                    |ui, dialog| {
                        styled_text_input(
                            ui,
                            &mut dialog.margin_right_str,
                            DESIGN_TOKENS.sizing.settings_dialog.input_width_margin,
                        );
                        if let Ok(v) = dialog.margin_right_str.parse::<u32>() {
                            dialog.settings.margins.right_bars = v;
                        }
                    },
                );

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

                // NAVIGATION BUTTONS
                self.draw_section_header(ui, "NAVIGATION BUTTONS", style);

                self.draw_setting_row_with_controls(ui, "Visibility", style, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "nav_visibility",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        dialog.settings.btns.navigation.name(),
                        |ui| {
                            for vis in ButtonVisibility::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.btns.navigation,
                                    *vis,
                                    vis.name(),
                                );
                            }
                        },
                    );
                });
            });
        });
    }

    pub(super) fn draw_symbol_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                // CANDLE COLORS
                self.draw_section_header(ui, "CANDLE COLORS", style);

                self.draw_setting_row_with_controls(ui, "Body", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.body_up, "body_up")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.candle_colors.body_up = color;
                    }
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.body_down, "body_down")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.candle_colors.body_down = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Border", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.border_up, "border_up")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.candle_colors.border_up = color;
                    }
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.border_down, "border_down")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.candle_colors.border_down = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Wick", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.wick_up, "wick_up")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.candle_colors.wick_up = color;
                    }
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.wick_down, "wick_down")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.candle_colors.wick_down = color;
                    }
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

                // PRECISION
                self.draw_section_header(ui, "PRECISION", style);

                self.draw_setting_row_with_controls(ui, "Mode", style, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "precision_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        dialog.settings.precision.name(),
                        |ui| {
                            for pm in PrecisionMode::all() {
                                ui.selectable_value(&mut dialog.settings.precision, *pm, pm.name());
                            }
                        },
                    );
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

                // TIMEZONE
                self.draw_section_header(ui, "TIMEZONE", style);

                self.draw_setting_row_with_controls(ui, "Timezone", style, |ui, dialog| {
                    let curr_tz = dialog.settings.timezone.clone();
                    styled_dropdown(
                        ui,
                        "timezone",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        &curr_tz,
                        |ui| {
                            for tz in &dialog.timezones {
                                ui.selectable_value(
                                    &mut dialog.settings.timezone,
                                    tz.clone(),
                                    tz.as_str(),
                                );
                            }
                        },
                    );
                });
            });
        });
    }

    pub(super) fn draw_status_line_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "INSTRUMENT", style);
                ui.horizontal(|ui| {
                    ui.spacing_mut().interact_size.y = 34.0;
                    ui.allocate_ui(egui::vec2(112.0, 42.0), |ui| {
                        styled_checkbox(
                            ui,
                            &mut self.settings.status_line.show_symbol,
                            "Title",
                            style.val_text,
                        );
                    });
                    styled_dropdown(
                        ui,
                        "title_format",
                        150.0,
                        style.val_text,
                        if self.settings.status_line.title_as_description {
                            "Description"
                        } else {
                            "Ticker"
                        },
                        |ui| {
                            ui.selectable_value(
                                &mut self.settings.status_line.title_as_description,
                                true,
                                "Description",
                            );
                            ui.selectable_value(
                                &mut self.settings.status_line.title_as_description,
                                false,
                                "Ticker",
                            );
                        },
                    );
                });
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_ohlc,
                    "Chart values",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_change,
                    "Bar change values",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_change_percent,
                    "Bar change percent",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_volume,
                    "Volume",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_quote_boxes,
                    "Buy/sell buttons",
                    style.val_text,
                );
                ui.add_space(24.0);
                self.draw_section_header(ui, "INDICATORS", style);
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_indicator_titles,
                    "Titles",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_indicator_inputs,
                    "Inputs",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_indicator_values,
                    "Values",
                    style.val_text,
                );
            });
        });
    }

    pub(super) fn draw_scales_and_lines_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "PRICE SCALE", style);
                self.draw_setting_row_with_controls(ui, "Placement", style, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "scale_placement",
                        150.0,
                        style.val_text,
                        if dialog.settings.scales_and_lines.price_scale_left {
                            "Left"
                        } else {
                            "Right"
                        },
                        |ui| {
                            ui.selectable_value(
                                &mut dialog.settings.scales_and_lines.price_scale_left,
                                false,
                                "Right",
                            );
                            ui.selectable_value(
                                &mut dialog.settings.scales_and_lines.price_scale_left,
                                true,
                                "Left",
                            );
                        },
                    );
                });
                ui.add_space(24.0);
                self.draw_section_header(ui, "PRICE LABELS & LINES", style);
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_countdown,
                    "Countdown to bar close",
                    style.val_text,
                );
                self.draw_setting_row_with_controls(ui, "Last price", style, |ui, dialog| {
                    price_line_dropdown(
                        ui,
                        "last_price_mode",
                        style.val_text,
                        &mut dialog.settings.scales_and_lines.show_last_price_label,
                        &mut dialog.settings.scales_and_lines.show_price_line,
                    );
                });
                self.draw_setting_row_with_controls(ui, "Previous close", style, |ui, dialog| {
                    price_line_dropdown(
                        ui,
                        "previous_close_mode",
                        style.val_text,
                        &mut dialog.settings.scales_and_lines.show_prev_close_label,
                        &mut dialog.settings.scales_and_lines.show_prev_close,
                    );
                });
                ui.add_space(24.0);
                self.draw_section_header(ui, "SCALE MODES", style);
                if styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.log_scale,
                    "Log scale",
                    style.val_text,
                ) && self.settings.scales_and_lines.log_scale
                {
                    self.settings.scales_and_lines.percentage_scale = false;
                }
                if styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.percentage_scale,
                    "Percentage scale",
                    style.val_text,
                ) && self.settings.scales_and_lines.percentage_scale
                {
                    self.settings.scales_and_lines.log_scale = false;
                }
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.invert_scale,
                    "Invert scale",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.auto_scale,
                    "Auto scale",
                    style.val_text,
                );
            });
        });
    }

    pub(super) fn draw_events_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        self.draw_section_header(ui, "EVENT VISIBILITY", style);
        let events = &mut self.settings.events;
        for (value, label) in [
            (&mut events.show_dividends, "Dividends"),
            (&mut events.show_splits, "Splits"),
            (&mut events.show_earnings, "Earnings"),
            (&mut events.show_economic_events, "Economic events"),
            (&mut events.show_news, "News"),
        ] {
            styled_checkbox(ui, value, label, style.val_text);
        }
    }

    pub(super) fn draw_alerts_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "ALERT SETTINGS", style);

                self.draw_setting_row_with_controls(ui, "Color", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.alert, "alert")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.alerts.alert_color = color;
                    }
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);
                self.draw_section_header(ui, "VISIBILITY", style);

                styled_checkbox(
                    ui,
                    &mut self.settings.alerts.show_alerts,
                    "Show alerts",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.alerts.show_alert_labels,
                    "Show alert labels",
                    style.val_text,
                );
            });
        });
    }

    pub(super) fn draw_section_header(
        &self,
        ui: &mut Ui,
        title: &str,
        style: &SettingsDialogStyle,
    ) {
        ui.label(
            RichText::new(title)
                .size(11.0)
                .strong()
                .color(style.section_header_text),
        );
        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.row_spacing);
    }

    pub(super) fn draw_setting_row_with_controls<F>(
        &mut self,
        ui: &mut Ui,
        label: &str,
        style: &SettingsDialogStyle,
        add_content: F,
    ) where
        F: FnOnce(&mut Ui, &mut SettingsDialog),
    {
        let label_width = [
            "Panel layout",
            "Background",
            "Grid lines",
            "Crosshair",
            "Mode",
            "Text",
            "Lines",
            "Top margin (%)",
            "Bottom margin (%)",
            "Right margin (bars)",
            "Visibility",
            "Timezone",
            "Body",
            "Border",
            "Wick",
        ]
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    (*label).into(),
                    FontId::proportional(typography::LG),
                    style.label_text,
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
        ui.horizontal(|ui| {
            // All tabs share the longest label's width.
            let label = ui.allocate_ui_with_layout(
                egui::vec2(label_width, 42.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(label_width);
                    ui.set_max_width(label_width);
                    ui.add(
                        egui::Label::new(
                            RichText::new(label)
                                .size(typography::LG)
                                .color(style.label_text),
                        )
                        .wrap(),
                    )
                },
            );

            // Controls column - starts at fixed offset
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.spacing_mut().interact_size.y = 32.0;
                ui.label_inputs_by(label.inner.id, |ui| add_content(ui, self));
            });
        });
        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.row_spacing);
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn price_line_dropdown(ui: &mut Ui, id: &str, color: Color32, label: &mut bool, line: &mut bool) {
    let mut mode = (*label, *line);
    let name = |mode| match mode {
        (true, true) => "Value and line",
        (true, false) => "Value",
        (false, true) => "Line",
        _ => "Hidden",
    };
    styled_dropdown(ui, id, 180.0, color, name(mode), |ui| {
        for option in [(true, true), (true, false), (false, true), (false, false)] {
            ui.selectable_value(&mut mode, option, name(option));
        }
    });
    (*label, *line) = mode;
}

fn styled_dropdown<R>(
    ui: &mut Ui,
    id: &str,
    width: f32,
    text_color: Color32,
    selected: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) {
    egui::ComboBox::from_id_salt(id)
        .icon(crate::ui_kit::buttons::combo_chevron)
        .width(width)
        .selected_text(
            RichText::new(selected)
                .size(typography::LG)
                .color(text_color),
        )
        .show_ui(ui, add_contents);
}

fn styled_checkbox(ui: &mut Ui, checked: &mut bool, label: &str, text_color: Color32) -> bool {
    let changed = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 42.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(42.0);
                ui.checkbox(checked, RichText::new(label).size(14.0).color(text_color))
                    .changed()
            },
        )
        .inner;
    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.row_spacing);
    changed
}

fn styled_text_input(ui: &mut Ui, text: &mut String, width: f32) {
    ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(width)
            .font(FontId::proportional(typography::MD)),
    );
}

fn line_style_preview(ls: LineStyle) -> &'static str {
    match ls {
        LineStyle::Solid => "Solid",
        LineStyle::Dashed => "\u{2013} \u{2013} \u{2013}",
        LineStyle::Dotted => "· · · ·",
        LineStyle::LargeDashed => "\u{2014}  \u{2014}  \u{2014}",
        LineStyle::SparseDotted => "·   ·   ·",
    }
}
