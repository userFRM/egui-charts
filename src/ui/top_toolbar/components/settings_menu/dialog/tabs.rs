//! Tab rendering implementations for SettingsDialog
//!
//! This module contains all the tab-specific UI rendering code,
//! keeping the main dialog module focused on core structure.

use egui::{Color32, FontId, RichText, Stroke, Ui};

use super::SettingsDialog;
use crate::theme::components::SettingsDialogStyle;
use crate::ui::stubs::LayoutStyle;
use crate::ui_kit::ColorPicker;

use crate::styles::{stroke, typography};

use super::super::data::LineStyle;
use super::super::types::{
    BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode,
};
use crate::tokens::DESIGN_TOKENS;

impl SettingsDialog {
    pub(super) fn draw_canvas_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                // LAYOUT STYLE
                self.draw_section_header(ui, "LAYOUT STYLE", style);

                self.draw_setting_row_with_controls(ui, "Panel layout", style, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "layout_style",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        style.val_text,
                        match dialog.layout_style {
                            LayoutStyle::Modern => "Modern",
                            LayoutStyle::Classic => "Classic",
                        },
                        |ui| {
                            ui.selectable_value(
                                &mut dialog.layout_style,
                                LayoutStyle::Modern,
                                "Modern",
                            );
                            ui.selectable_value(
                                &mut dialog.layout_style,
                                LayoutStyle::Classic,
                                "Classic",
                            );
                        },
                    );
                });

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);

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
                            {
                                dialog.settings.chart_basic_styles.background_gradient_top = color;
                            }
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background_gradient_bottom,
                                "bg_grad_bot",
                            )
                            .show(ui)
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
                    if dialog.settings.grid_lines.mode != GridLinesMode::None {
                        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);
                        if let Some(color) =
                            ColorPicker::new(&mut dialog.color_picker_states.grid_h, "grid_h")
                                .show(ui)
                        {
                            dialog.settings.grid_lines.horizontal_color = color;
                        }
                    }
                    if dialog.settings.grid_lines.mode == GridLinesMode::Both
                        && let Some(color) =
                            ColorPicker::new(&mut dialog.color_picker_states.grid_v, "grid_v")
                                .show(ui)
                    {
                        dialog.settings.grid_lines.vertical_color = color;
                    }
                });

                // Crosshair
                self.draw_setting_row_with_controls(ui, "Crosshair", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.crosshair, "crosshair")
                            .show(ui)
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
                    ui.add(egui::Slider::new(&mut lw, 1.0..=4.0).show_value(false));
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
                            for wm in WatermarkMode::all() {
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
                    {
                        dialog.settings.scales_appearance.text_color = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Lines", style, |ui, dialog| {
                    if let Some(color) = ColorPicker::new(
                        &mut dialog.color_picker_states.scales_lines,
                        "scales_lines",
                    )
                    .show(ui)
                    {
                        dialog.settings.scales_appearance.lines_color = color;
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
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                // CANDLE COLORS
                self.draw_section_header(ui, "CANDLE COLORS", style);

                self.draw_setting_row_with_controls(ui, "Body", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.body_up, "body_up")
                            .show(ui)
                    {
                        dialog.settings.candle_colors.body_up = color;
                    }
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.body_down, "body_down")
                            .show(ui)
                    {
                        dialog.settings.candle_colors.body_down = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Border", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.border_up, "border_up")
                            .show(ui)
                    {
                        dialog.settings.candle_colors.border_up = color;
                    }
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.border_down, "border_down")
                            .show(ui)
                    {
                        dialog.settings.candle_colors.border_down = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Wick", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.wick_up, "wick_up")
                            .show(ui)
                    {
                        dialog.settings.candle_colors.wick_up = color;
                    }
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.wick_down, "wick_down")
                            .show(ui)
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

                self.draw_setting_row_with_controls(ui, "Chart timezone", style, |ui, dialog| {
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
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "VISIBILITY", style);

                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_symbol,
                    "Symbol",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_ohlc,
                    "OHLC values",
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
                    &mut self.settings.status_line.show_volume,
                    "Volume",
                    style.val_text,
                );
            });
        });
    }

    pub(super) fn draw_scales_and_lines_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "SCALE MODES", style);

                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.log_scale,
                    "Log scale",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.percentage_scale,
                    "Percentage scale",
                    style.val_text,
                );
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

                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.section_spacing);
                self.draw_section_header(ui, "PRICE LINES", style);

                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_countdown,
                    "Countdown to bar close",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_high_low,
                    "High/Low price lines",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_prev_close,
                    "Previous day close",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_bid_ask,
                    "Bid/Ask price lines",
                    style.val_text,
                );
            });
        });
    }

    pub(super) fn draw_trading_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "TRADING OPTIONS", style);

                styled_checkbox(
                    ui,
                    &mut self.settings.trading.show_poss,
                    "Show positions",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.trading.show_orders,
                    "Show orders",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.trading.show_executions,
                    "Show executions",
                    style.val_text,
                );
            });
        });
    }

    pub(super) fn draw_alerts_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "ALERT SETTINGS", style);

                self.draw_setting_row_with_controls(ui, "Color", style, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.alert, "alert").show(ui)
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

    pub(super) fn draw_events_tab(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_h);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "EVENT MARKERS", style);

                styled_checkbox(
                    ui,
                    &mut self.settings.events.show_dividends,
                    "Dividends",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.events.show_splits,
                    "Splits",
                    style.val_text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.events.show_earnings,
                    "Earnings",
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
                .size(typography::SM)
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
        ui.horizontal(|ui| {
            // Label column with fixed width
            ui.allocate_ui_with_layout(
                egui::vec2(
                    DESIGN_TOKENS.sizing.settings_dialog.label_width,
                    DESIGN_TOKENS.sizing.settings_dialog.row_height,
                ),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(label)
                            .size(typography::LG)
                            .color(style.label_text),
                    );
                },
            );

            // Controls column - starts at fixed offset
            ui.horizontal(|ui| {
                add_content(ui, self);
            });
        });
        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.row_spacing);
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

pub(super) fn configure_dialog_visuals(ui: &mut Ui, style: &SettingsDialogStyle) {
    let visuals = ui.visuals_mut();

    // Widget backgrounds and rounding
    visuals.widgets.noninteractive.bg_fill = style.dropdown_bg;
    visuals.widgets.noninteractive.weak_bg_fill = style.dropdown_bg;
    visuals.widgets.noninteractive.bg_stroke = style.dropdown_border;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(stroke::HAIRLINE, style.val_text);

    visuals.widgets.inactive.bg_fill = style.dropdown_bg;
    visuals.widgets.inactive.weak_bg_fill = style.dropdown_bg;
    visuals.widgets.inactive.bg_stroke = style.dropdown_border;
    visuals.widgets.inactive.fg_stroke = Stroke::new(stroke::HAIRLINE, style.val_text);

    visuals.widgets.hovered.bg_fill = style.dropdown_bg_hover;
    visuals.widgets.hovered.weak_bg_fill = style.dropdown_bg_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(stroke::HAIRLINE, style.val_text);
    visuals.widgets.hovered.fg_stroke = Stroke::new(stroke::HAIRLINE, style.val_text);

    visuals.widgets.active.bg_fill = style.input_bg_focus;
    visuals.widgets.active.weak_bg_fill = style.input_bg_focus;
    visuals.widgets.active.bg_stroke = style.input_border_focus;
    visuals.widgets.active.fg_stroke = Stroke::new(stroke::HAIRLINE, style.val_text);

    // Selection colors for dropdowns
    visuals.selection.bg_fill = style.tab_bg_active;
    visuals.selection.stroke = Stroke::new(0.0, Color32::TRANSPARENT);

    // Text input (TextEdit)
    visuals.extreme_bg_color = style.input_bg;

    // Checkbox styling
    visuals.widgets.inactive.expansion = 0.0;
    visuals.widgets.hovered.expansion = 0.0;
    visuals.widgets.active.expansion = 0.0;

    // Popup background (for ComboBox dropdown)
    visuals.window_fill = style.sidebar_bg;
    visuals.window_stroke = style.dropdown_border;
    visuals.popup_shadow = egui::Shadow::NONE;
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
        .width(width)
        .selected_text(
            RichText::new(selected)
                .size(typography::LG)
                .color(text_color),
        )
        .show_ui(ui, add_contents);
}

fn styled_checkbox(ui: &mut Ui, checked: &mut bool, label: &str, text_color: Color32) {
    ui.horizontal(|ui| {
        ui.checkbox(
            checked,
            RichText::new(label).size(typography::LG).color(text_color),
        );
    });
    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.row_spacing);
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
        LineStyle::Solid => "———",
        LineStyle::Dashed => "– – –",
        LineStyle::Dotted => "· · · ·",
    }
}
