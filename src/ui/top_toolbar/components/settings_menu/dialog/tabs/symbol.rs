//! Symbol and status line tabs of the chart settings dialog.

use super::super::SettingsDialog;
use super::{styled_checkbox, styled_dropdown};
use crate::localization as l10n;
use crate::theme::Theme;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::ColorPicker;
use crate::widget::settings::PrecisionMode;
use egui::{RichText, Ui};

/// Height of a candle part row, giving a 50 point pitch with the spacing below it.
const CANDLE_ROW_HEIGHT: f32 = 38.0;

impl SettingsDialog {
    pub(in super::super) fn draw_symbol_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                styled_checkbox(
                    ui,
                    &mut self.settings.show_price_targets,
                    "Analyst price targets",
                    theme.ui.text,
                );
                let kind = self.settings.chart_type;
                if kind.is_time_independent()
                    || !kind.uses_ohlc()
                    || kind == crate::model::ChartType::TimePriceOpportunity
                {
                    let mut series = crate::chart::series::SeriesSettings {
                        chart_type: kind,
                        chart_type_params: self
                            .settings
                            .chart_type_params
                            .clone()
                            .unwrap_or_default(),
                        price_source: self.settings.price_source.unwrap_or_default(),
                        bullish_color: self.settings.candle_colors.body_up,
                        bearish_color: self.settings.candle_colors.body_down,
                        ..Default::default()
                    };
                    crate::ui::dialogs::series_settings::SymbolTab::show(ui, &mut series);
                    self.settings.chart_type_params = Some(series.chart_type_params);
                    self.settings.price_source = Some(series.price_source);
                    self.settings.candle_colors.body_up = series.bullish_color;
                    self.settings.candle_colors.body_down = series.bearish_color;
                } else {
                    let candles = kind == crate::model::ChartType::Candles;
                    self.draw_section_header(ui, if candles { "CANDLES" } else { "COLORS" }, theme);
                    if candles {
                        styled_checkbox(
                            ui,
                            &mut self.settings.candle_colors.color_by_previous_close,
                            "Color bars based on previous close",
                            theme.ui.text,
                        );
                    }
                    let label_width = Self::label_width(ui, theme);
                    let colors = &mut self.settings.candle_colors;
                    let pickers = &mut self.color_picker_states;
                    for (label, shown, [up, down], [up_picker, down_picker], [up_id, down_id]) in [
                        (
                            "Body",
                            &mut colors.show_body,
                            [&mut colors.body_up, &mut colors.body_down],
                            [&mut pickers.body_up, &mut pickers.body_down],
                            ["body_up", "body_down"],
                        ),
                        (
                            "Borders",
                            &mut colors.show_border,
                            [&mut colors.border_up, &mut colors.border_down],
                            [&mut pickers.border_up, &mut pickers.border_down],
                            ["border_up", "border_down"],
                        ),
                        (
                            "Wick",
                            &mut colors.show_wick,
                            [&mut colors.wick_up, &mut colors.wick_down],
                            [&mut pickers.wick_up, &mut pickers.wick_down],
                            ["wick_up", "wick_down"],
                        ),
                    ] {
                        ui.horizontal(|ui| {
                            let checkbox = ui
                                .allocate_ui_with_layout(
                                    egui::vec2(label_width, CANDLE_ROW_HEIGHT),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_width(label_width);
                                        let label = RichText::new(l10n::text(ui.ctx(), label))
                                            .size(theme.text.title)
                                            .color(theme.ui.text);
                                        if candles {
                                            ui.checkbox(shown, label)
                                        } else {
                                            ui.label(label)
                                        }
                                    },
                                )
                                .inner;
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.spacing_mut().interact_size.y = 32.0;
                            ui.add_enabled_ui(!candles || *shown, |ui| {
                                ui.label_inputs_by(checkbox.id, |ui| {
                                    for (color, picker, id) in
                                        [(up, up_picker, up_id), (down, down_picker, down_id)]
                                    {
                                        if let Some(picked) =
                                            ColorPicker::new(picker, id).show(ui).inner
                                        {
                                            *color = picked;
                                        }
                                    }
                                });
                            });
                        });
                        ui.add_space(theme.spacing.sm);
                    }

                    ui.add_space(ui.active_theme().spacing.xxl);
                }

                // DATA MODIFICATION
                self.draw_section_header(ui, "DATA MODIFICATION", theme);
                styled_checkbox(
                    ui,
                    &mut self.settings.adjust_for_dividends,
                    "Adjust data for dividends",
                    theme.ui.text,
                );
                ui.add_space(ui.active_theme().spacing.xxl);

                // PRECISION
                self.draw_section_header(ui, "PRECISION", theme);

                self.draw_setting_row_with_controls(ui, "Mode", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "precision_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.precision.name(),
                        |ui| {
                            for pm in PrecisionMode::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.precision,
                                    *pm,
                                    l10n::text(ui.ctx(), pm.name()),
                                );
                            }
                        },
                    );
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // TIMEZONE
                self.draw_section_header(ui, "TIMEZONE", theme);

                self.draw_setting_row_with_controls(ui, "Timezone", theme, |ui, dialog| {
                    let curr_tz = dialog.settings.timezone.clone();
                    styled_dropdown(
                        ui,
                        "timezone",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        &curr_tz,
                        |ui| {
                            for tz in &dialog.timezones {
                                ui.selectable_value(
                                    &mut dialog.settings.timezone,
                                    tz.clone(),
                                    l10n::text(ui.ctx(), tz.as_str()),
                                );
                            }
                        },
                    );
                });
            });
        });
    }

    pub(in super::super) fn draw_status_line_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "INSTRUMENT", theme);
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_logo,
                    "Logo",
                    theme.ui.text,
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().interact_size.y = 34.0;
                    ui.allocate_ui(egui::vec2(112.0, 42.0), |ui| {
                        styled_checkbox(
                            ui,
                            &mut self.settings.status_line.show_symbol,
                            "Title",
                            theme.ui.text,
                        );
                    });
                    styled_dropdown(
                        ui,
                        "title_format",
                        150.0,
                        theme.ui.text,
                        if self.settings.status_line.title_as_description {
                            "Description"
                        } else {
                            "Ticker"
                        },
                        |ui| {
                            ui.selectable_value(
                                &mut self.settings.status_line.title_as_description,
                                true,
                                l10n::text(ui.ctx(), "Description"),
                            );
                            ui.selectable_value(
                                &mut self.settings.status_line.title_as_description,
                                false,
                                l10n::text(ui.ctx(), "Ticker"),
                            );
                        },
                    );
                });
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_ohlc,
                    "Chart values",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_change,
                    "Bar change values",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_change_percent,
                    "Bar change percent",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_volume,
                    "Volume",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_last_day_change,
                    "Last day change values",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_quote_boxes,
                    "Buy/sell buttons",
                    theme.ui.text,
                );
                ui.add_space(theme.spacing.xxxl);
                self.draw_section_header(ui, "INDICATORS", theme);
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_indicator_titles,
                    "Titles",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_indicator_inputs,
                    "Inputs",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.status_line.show_indicator_values,
                    "Values",
                    theme.ui.text,
                );
                self.draw_setting_row_with_controls(ui, "Background", theme, |ui, dialog| {
                    ui.visuals_mut().widgets.inactive.bg_fill = theme.ui.border_control;
                    ui.add(
                        egui::Slider::new(
                            &mut dialog.settings.status_line.indicator_background_opacity,
                            0..=100,
                        )
                        .suffix("%"),
                    )
                    .accessible_name(l10n::text(ui.ctx(), "Background opacity"));
                });
            });
        });
    }
}
