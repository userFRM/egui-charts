//! Scales and lines tab of the chart settings dialog.

use super::super::SettingsDialog;
use super::canvas::line_style_preview;
use super::{
    pick_color, price_line_dropdown, price_line_dropdown_pinning, styled_checkbox, styled_dropdown,
    trading_option,
};
use crate::localization as l10n;
use crate::model::{LineStyle, PriceLineSource};
use crate::scales::WeekdayNames;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use crate::widget::settings::{DateFormat, ThousandsSeparator};
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
                self.draw_setting_row_with_controls(
                    ui,
                    "Thousands separator",
                    theme,
                    |ui, dialog| {
                        use ThousandsSeparator::{Apostrophe, Comma, Off, Other, Space};
                        let separator = &mut dialog.settings.scales_and_lines.thousands_sep;
                        let ctx = ui.ctx().clone();
                        let name = |separator| match separator {
                            Off => "None".to_owned(),
                            Comma => "Comma".to_owned(),
                            Space => "Space".to_owned(),
                            Apostrophe => "Apostrophe".to_owned(),
                            Other(character) => l10n::format_text(
                                &ctx,
                                "Other ({character})",
                                &[("character", character.to_string())],
                            ),
                        };
                        // A separator the host set stays listed while it is the chart's.
                        let host = separator.filter(|separator| matches!(separator, Other(_)));
                        styled_dropdown(
                            ui,
                            "thousands_separator",
                            150.0,
                            theme.ui.text,
                            &name(separator.unwrap_or(Comma)),
                            |ui| {
                                for option in
                                    [Off, Comma, Space, Apostrophe].into_iter().chain(host)
                                {
                                    let label = l10n::text(ui.ctx(), name(option));
                                    ui.selectable_value(separator, Some(option), label);
                                }
                            },
                        );
                    },
                );
                self.draw_setting_row_with_controls(ui, "Pane axes", theme, |ui, dialog| {
                    use crate::model::AxisSide;
                    let side = &mut dialog.settings.scales_and_lines.axis_layout.side;
                    styled_dropdown(
                        ui,
                        "pane_axis_side",
                        150.0,
                        theme.ui.text,
                        match side {
                            None => "By assignment",
                            Some(AxisSide::Left) => "Left edge",
                            Some(AxisSide::Right) => "Right edge",
                        },
                        |ui| {
                            for (label, value) in [
                                ("By assignment", None),
                                ("Left edge", Some(AxisSide::Left)),
                                ("Right edge", Some(AxisSide::Right)),
                            ] {
                                ui.selectable_value(side, value, l10n::text(ui.ctx(), label));
                            }
                        },
                    );
                });
                styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.axis_layout.hide_unused,
                    "Hide unused axes",
                    theme.ui.text,
                );
                ui.add_space(theme.spacing.xxxl);
                self.draw_setting_row_with_controls(
                    ui,
                    "Currency and unit",
                    theme,
                    |ui, dialog| {
                        let on_hover = &mut dialog.settings.scales_and_lines.currency_unit_on_hover;
                        let name = |on_hover| {
                            if on_hover {
                                "Visible on mouse over"
                            } else {
                                "Always visible"
                            }
                        };
                        styled_dropdown(
                            ui,
                            "currency_unit_visibility",
                            180.0,
                            theme.ui.text,
                            name(*on_hover),
                            |ui| {
                                for option in [false, true] {
                                    let label = l10n::text(ui.ctx(), name(option));
                                    ui.selectable_value(on_hover, option, label);
                                }
                            },
                        );
                    },
                );
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
                self.draw_setting_row_with_controls(
                    ui,
                    "Last price follows",
                    theme,
                    |ui, dialog| {
                        let source = &mut dialog.settings.scales_and_lines.price_line_source;
                        let name = |source| match source {
                            PriceLineSource::LastBar => "Newest bar",
                            PriceLineSource::LastVisible => "Newest bar in view",
                        };
                        styled_dropdown(
                            ui,
                            "last_price_source",
                            180.0,
                            theme.ui.text,
                            name(source.unwrap_or_default()),
                            |ui| {
                                for option in
                                    [PriceLineSource::LastBar, PriceLineSource::LastVisible]
                                {
                                    let label = l10n::text(ui.ctx(), name(option));
                                    ui.selectable_value(source, Some(option), label);
                                }
                            },
                        );
                    },
                );
                self.draw_setting_row_with_controls(ui, "Symbol color", theme, |ui, dialog| {
                    let state = &mut dialog.color_picker_states.symbol_value;
                    let mut color = state.sel_color;
                    if pick_color(ui, state, "symbol_value_color", &mut color) {
                        dialog.settings.scales_and_lines.symbol_price_color = Some(color);
                    }
                    if ui.button(l10n::text(ui.ctx(), "Automatic")).clicked() {
                        dialog.settings.scales_and_lines.symbol_price_color = None;
                    }
                });
                self.draw_setting_row_with_controls(ui, "Last price line", theme, |ui, dialog| {
                    let scales = &mut dialog.settings.scales_and_lines;
                    let default = crate::model::SeriesStyle::default();
                    let current = scales.price_line_style.unwrap_or(default.price_line_style);
                    styled_dropdown(
                        ui,
                        "last_price_line_style",
                        DESIGN_TOKENS.sizing.settings_dialog.small_dropdown_width,
                        theme.ui.text,
                        line_style_preview(current),
                        |ui| {
                            for style in LineStyle::all() {
                                let label = l10n::text(ui.ctx(), line_style_preview(*style));
                                if ui.selectable_label(current == *style, label).clicked() {
                                    scales.price_line_style = Some(*style);
                                }
                            }
                        },
                    );
                    ui.add_space(theme.spacing.lg);
                    // A width the host set outside the offered range stays until it is dragged.
                    let mut width = scales.price_line_width.unwrap_or(default.price_line_width);
                    if ui
                        .add(
                            egui::DragValue::new(&mut width)
                                .range(1.0..=4.0)
                                .clamp_existing_to_range(false)
                                .speed(0.5)
                                .suffix(l10n::text(ui.ctx(), " px")),
                        )
                        .accessible_name(l10n::text(ui.ctx(), "Last price line width"))
                        .changed()
                    {
                        scales.price_line_width = Some(width);
                    }
                });
                self.draw_setting_row_with_controls(ui, "Last price tag", theme, |ui, dialog| {
                    let change = &mut dialog.settings.scales_and_lines.show_last_price_change;
                    let name = |change| {
                        if change { "Value and change" } else { "Value" }
                    };
                    styled_dropdown(
                        ui,
                        "last_price_tag",
                        180.0,
                        theme.ui.text,
                        name(*change),
                        |ui| {
                            for option in [false, true] {
                                let label = l10n::text(ui.ctx(), name(option));
                                ui.selectable_value(change, option, label);
                            }
                        },
                    );
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
                    let state = &mut dialog.color_picker_states.study_value;
                    let mut color = state.sel_color;
                    if pick_color(ui, state, "study_value_color", &mut color) {
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
                        pick_color(
                            ui,
                            &mut dialog.color_picker_states.extended_value,
                            "extended_value_color",
                            &mut dialog
                                .settings
                                .scales_and_lines
                                .extended_session_price_color,
                        );
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
                self.draw_setting_row_with_controls(
                    ui,
                    "Previous close line",
                    theme,
                    |ui, dialog| {
                        let mut line = dialog
                            .settings
                            .scales_and_lines
                            .prev_close_line
                            .unwrap_or_default();
                        let before = line;
                        let state = &mut dialog.color_picker_states.prev_close_line;
                        let mut color = state.sel_color;
                        if pick_color(ui, state, "prev_close_line_color", &mut color) {
                            line.color = Some(color);
                        }
                        ui.add_space(theme.spacing.lg);
                        styled_dropdown(
                            ui,
                            "prev_close_line_style",
                            DESIGN_TOKENS.sizing.settings_dialog.small_dropdown_width,
                            theme.ui.text,
                            line_style_preview(line.style),
                            |ui| {
                                for style in LineStyle::all() {
                                    let label = l10n::text(ui.ctx(), line_style_preview(*style));
                                    if ui.selectable_label(line.style == *style, label).clicked() {
                                        line.style = *style;
                                    }
                                }
                            },
                        );
                        ui.add_space(theme.spacing.lg);
                        // A width the host set outside the offered range stays until it is dragged.
                        ui.add(
                            egui::DragValue::new(&mut line.width)
                                .range(1.0..=4.0)
                                .clamp_existing_to_range(false)
                                .speed(0.5)
                                .suffix(l10n::text(ui.ctx(), " px")),
                        )
                        .accessible_name(l10n::text(ui.ctx(), "Previous close line width"));
                        if line != before {
                            dialog.settings.scales_and_lines.prev_close_line = Some(line);
                        }
                    },
                );
                self.draw_setting_row_with_controls(ui, "Bid and ask", theme, |ui, dialog| {
                    let scales = &mut dialog.settings.scales_and_lines;
                    price_line_dropdown_pinning(
                        ui,
                        "bid_ask_mode",
                        theme.ui.text,
                        &mut scales.show_bid_ask,
                        &mut scales.show_bid_ask_lines,
                    );
                });
                self.draw_setting_row_with_controls(ui, "High and low", theme, |ui, dialog| {
                    let scales = &mut dialog.settings.scales_and_lines;
                    price_line_dropdown_pinning(
                        ui,
                        "high_low_mode",
                        theme.ui.text,
                        &mut scales.show_high_low,
                        &mut scales.show_high_low_lines,
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
                    self.settings.scales_and_lines.indexed_to_100 = Some(false);
                }
                if styled_checkbox(
                    ui,
                    &mut self.settings.scales_and_lines.percentage_scale,
                    "Percentage scale",
                    theme.ui.text,
                ) && self.settings.scales_and_lines.percentage_scale
                {
                    self.settings.scales_and_lines.log_scale = false;
                    self.settings.scales_and_lines.indexed_to_100 = Some(false);
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
                ui.add_space(theme.spacing.xxxl);
                self.draw_section_header(ui, "TIME SCALE", theme);
                self.draw_setting_row_with_controls(ui, "Date format", theme, |ui, dialog| {
                    let date_format = &mut dialog.settings.scales_and_lines.date_format;
                    // Keep a host pattern listed while it is the chart's choice.
                    let host = date_format.clone().filter(|format| {
                        matches!(format, DateFormat::Pattern(pattern)
                            if !DATE_FORMATS.contains(&pattern.as_str()))
                    });
                    let selected =
                        date_format_name(date_format.as_ref().unwrap_or(&DateFormat::Default));
                    styled_dropdown(ui, "date_format", 150.0, theme.ui.text, &selected, |ui| {
                        let presets =
                            DATE_FORMATS.map(|pattern| DateFormat::Pattern(pattern.into()));
                        for format in std::iter::once(DateFormat::Default)
                            .chain(presets)
                            .chain(host)
                        {
                            let label = l10n::text(ui.ctx(), date_format_name(&format));
                            ui.selectable_value(date_format, Some(format), label);
                        }
                    });
                });
                self.draw_setting_row_with_controls(ui, "Clock", theme, |ui, dialog| {
                    let use_24_hour = &mut dialog.settings.scales_and_lines.use_24_hour;
                    let name = |use_24_hour| if use_24_hour { "24-hour" } else { "12-hour" };
                    styled_dropdown(
                        ui,
                        "clock",
                        150.0,
                        theme.ui.text,
                        name(use_24_hour.unwrap_or(true)),
                        |ui| {
                            for option in [true, false] {
                                let label = l10n::text(ui.ctx(), name(option));
                                ui.selectable_value(use_24_hour, Some(option), label);
                            }
                        },
                    );
                });
                self.draw_setting_row_with_controls(ui, "Day of week", theme, |ui, dialog| {
                    let weekday_names = &mut dialog.settings.scales_and_lines.weekday_names;
                    let name = |names| match names {
                        WeekdayNames::Default => "Default",
                        WeekdayNames::Always => "Always",
                        WeekdayNames::Never => "Never",
                    };
                    styled_dropdown(
                        ui,
                        "weekday_names",
                        150.0,
                        theme.ui.text,
                        name(weekday_names.unwrap_or_default()),
                        |ui| {
                            for option in [
                                WeekdayNames::Default,
                                WeekdayNames::Always,
                                WeekdayNames::Never,
                            ] {
                                let label = l10n::text(ui.ctx(), name(option));
                                ui.selectable_value(weekday_names, Some(option), label);
                            }
                        },
                    );
                });
                let keep = &mut self
                    .settings
                    .scales_and_lines
                    .keep_left_edge_on_interval_change;
                let mut on = keep.unwrap_or_default();
                if styled_checkbox(
                    ui,
                    &mut on,
                    "Keep left edge on interval change",
                    theme.ui.text,
                ) {
                    *keep = Some(on);
                }
            });
        });
    }
}

/// Date patterns offered alongside the built-in dates.
const DATE_FORMATS: [&str; 6] = [
    "%d %b %Y",
    "%b %d, %Y",
    "%Y-%m-%d",
    "%d.%m.%Y",
    "%d/%m/%Y",
    "%m/%d/%Y",
];

/// A sample past the twelfth distinguishes day-first and month-first dates.
fn date_format_name(format: &DateFormat) -> String {
    use chrono::TimeZone;
    use std::fmt::Write;
    let pattern = match format {
        DateFormat::Default => return "Default".into(),
        DateFormat::Pattern(pattern) => pattern,
    };
    let sample = chrono::Utc
        .with_ymd_and_hms(2024, 3, 13, 14, 30, 0)
        .unwrap();
    let mut name = String::new();
    // Formatting into a string reports invalid patterns instead of panicking.
    match write!(name, "{}", sample.format(pattern)) {
        Ok(()) => name,
        Err(_) => pattern.clone(),
    }
}
