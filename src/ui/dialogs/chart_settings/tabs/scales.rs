//! Scales and lines tab of the chart settings dialog.

use super::super::SettingsDialog;
use super::{price_line_dropdown, styled_checkbox, styled_dropdown, trading_option};
use crate::localization as l10n;
use crate::theme::Theme;
use crate::ui_kit::ColorPicker;
use egui::Ui;

impl SettingsDialog {
    pub(in super::super) fn draw_scales_and_lines_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "PRICE SCALE", theme);
                self.draw_setting_row_with_controls(ui, "Placement", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "scale_placement",
                        150.0,
                        theme.ui.text,
                        if dialog.settings.scales_and_lines.price_scale_left {
                            "Left"
                        } else {
                            "Right"
                        },
                        |ui| {
                            ui.selectable_value(
                                &mut dialog.settings.scales_and_lines.price_scale_left,
                                false,
                                l10n::text(ui.ctx(), "Right"),
                            );
                            ui.selectable_value(
                                &mut dialog.settings.scales_and_lines.price_scale_left,
                                true,
                                l10n::text(ui.ctx(), "Left"),
                            );
                        },
                    );
                });
                ui.add_space(theme.spacing.xxxl);
                for (value, label) in [
                    (
                        &mut self.settings.scales_and_lines.show_currency,
                        "Currency",
                    ),
                    (&mut self.settings.scales_and_lines.show_unit, "Unit"),
                    (
                        &mut self.settings.scales_and_lines.show_scale_mode_buttons,
                        "Scale mode buttons",
                    ),
                    (
                        &mut self.settings.scales_and_lines.show_plus_button,
                        "Plus button",
                    ),
                    (
                        &mut self.settings.scales_and_lines.allow_label_overlap,
                        "Allow labels to overlap",
                    ),
                ] {
                    styled_checkbox(ui, value, label, theme.ui.text);
                }
                let scales = &mut self.settings.scales_and_lines;
                let locked = scales.lock_price_to_bar_ratio;
                trading_option(
                    ui,
                    0.0,
                    &mut scales.lock_price_to_bar_ratio,
                    "Lock price-to-bar ratio",
                    theme.ui.text,
                    |ui| {
                        ui.add_enabled(
                            locked,
                            egui::DragValue::new(&mut scales.price_to_bar_ratio)
                                .speed(0.01)
                                .custom_formatter(|value, _| value.to_string())
                                .range(0.00000001..=f64::MAX),
                        )
                        .accessible_name(l10n::text(ui.ctx(), "Price-to-bar ratio"));
                    },
                );
                if scales.lock_price_to_bar_ratio {
                    scales.auto_scale = false;
                }
                ui.add_space(theme.spacing.xl);
                self.draw_section_header(ui, "PRICE LABELS & LINES", theme);
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_countdown,
                    "Countdown to bar close",
                    theme.ui.text,
                );
                self.draw_setting_row_with_controls(ui, "Last price", theme, |ui, dialog| {
                    price_line_dropdown(
                        ui,
                        "last_price_mode",
                        theme.ui.text,
                        &mut dialog.settings.scales_and_lines.show_last_price_label,
                        &mut dialog.settings.scales_and_lines.show_price_line,
                    );
                });
                self.draw_setting_row_with_controls(ui, "Symbol color", theme, |ui, dialog| {
                    if let Some(color) = ColorPicker::new(
                        &mut dialog.color_picker_states.symbol_value,
                        "symbol_value_color",
                    )
                    .show(ui)
                    .inner
                    {
                        dialog.settings.scales_and_lines.symbol_price_color = Some(color);
                    }
                    if ui.button(l10n::text(ui.ctx(), "Automatic")).clicked() {
                        dialog.settings.scales_and_lines.symbol_price_color = None;
                    }
                });
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_symbol_name,
                    "Symbol name on scale",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.show_study_names,
                    "Study names on scale",
                    theme.ui.text,
                );
                self.draw_setting_row_with_controls(ui, "Study values", theme, |ui, dialog| {
                    styled_checkbox(
                        ui,
                        &mut dialog.settings.scales_and_lines.show_study_values,
                        "Show",
                        theme.ui.text,
                    );
                    if let Some(color) = ColorPicker::new(
                        &mut dialog.color_picker_states.study_value,
                        "study_value_color",
                    )
                    .show(ui)
                    .inner
                    {
                        dialog.settings.scales_and_lines.study_value_color = Some(color);
                    }
                    if ui.button(l10n::text(ui.ctx(), "Automatic")).clicked() {
                        dialog.settings.scales_and_lines.study_value_color = None;
                    }
                });
                self.draw_setting_row_with_controls(
                    ui,
                    "Extended-session price",
                    theme,
                    |ui, dialog| {
                        styled_checkbox(
                            ui,
                            &mut dialog.settings.scales_and_lines.show_extended_session_price,
                            "Show",
                            theme.ui.text,
                        );
                        if let Some(color) = ColorPicker::new(
                            &mut dialog.color_picker_states.extended_value,
                            "extended_value_color",
                        )
                        .show(ui)
                        .inner
                        {
                            dialog
                                .settings
                                .scales_and_lines
                                .extended_session_price_color = color;
                        }
                    },
                );
                self.draw_setting_row_with_controls(ui, "Previous close", theme, |ui, dialog| {
                    price_line_dropdown(
                        ui,
                        "previous_close_mode",
                        theme.ui.text,
                        &mut dialog.settings.scales_and_lines.show_prev_close_label,
                        &mut dialog.settings.scales_and_lines.show_prev_close,
                    );
                });
                ui.add_space(theme.spacing.xxxl);
                self.draw_section_header(ui, "SCALE MODES", theme);
                if styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.log_scale,
                    "Log scale",
                    theme.ui.text,
                ) && self.settings.scales_and_lines.log_scale
                {
                    self.settings.scales_and_lines.percentage_scale = false;
                }
                if styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.percentage_scale,
                    "Percentage scale",
                    theme.ui.text,
                ) && self.settings.scales_and_lines.percentage_scale
                {
                    self.settings.scales_and_lines.log_scale = false;
                }
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.invert_scale,
                    "Invert scale",
                    theme.ui.text,
                );
                if styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.auto_scale,
                    "Auto scale",
                    theme.ui.text,
                ) && self.settings.scales_and_lines.auto_scale
                {
                    self.settings.scales_and_lines.lock_price_to_bar_ratio = false;
                }
            });
        });
    }
}
