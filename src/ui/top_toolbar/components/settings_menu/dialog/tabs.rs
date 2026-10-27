//! Tab rendering implementations for SettingsDialog
//!
//! This module contains all the tab-specific UI rendering code,
//! keeping the main dialog module focused on core structure.

use crate::localization as l10n;
use crate::model::LineStyle;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, RichText, Ui};

use super::SettingsDialog;
use crate::theme::Theme;
use crate::ui_kit::ColorPicker;

use crate::chart::PnlUnit;
use crate::tokens::DESIGN_TOKENS;
use crate::widget::settings::TradingNotifications;
use crate::widget::settings::{
    BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode,
};

impl SettingsDialog {
    pub(super) fn draw_canvas_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                // CHART BASIC STYLES
                self.draw_section_header(ui, "CHART BASIC STYLES", theme);

                // Background
                self.draw_setting_row_with_controls(ui, "Background", theme, |ui, dialog| {
                    // Type dropdown
                    styled_dropdown(
                        ui,
                        "bg_type",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.chart_basic_styles.background_type.name(),
                        |ui| {
                            for bt in BackgroundType::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.chart_basic_styles.background_type,
                                    *bt,
                                    l10n::text(ui.ctx(), bt.name()),
                                );
                            }
                        },
                    );

                    ui.add_space(ui.active_theme().spacing.lg);

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
                self.draw_setting_row_with_controls(ui, "Grid lines", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "grid_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.grid_lines.mode.name(),
                        |ui| {
                            for mode in GridLinesMode::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.grid_lines.mode,
                                    *mode,
                                    l10n::text(ui.ctx(), mode.name()),
                                );
                            }
                        },
                    );

                    // Color picker(s) based on mode
                    if matches!(
                        dialog.settings.grid_lines.mode,
                        GridLinesMode::Horizontal | GridLinesMode::Both
                    ) {
                        ui.add_space(ui.active_theme().spacing.lg);
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
                self.draw_setting_row_with_controls(ui, "Crosshair", theme, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.crosshair, "crosshair")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.crosshair.color = color;
                    }

                    ui.add_space(ui.active_theme().spacing.lg);

                    // Line style with dashes preview
                    styled_dropdown(
                        ui,
                        "crosshair_style",
                        DESIGN_TOKENS.sizing.settings_dialog.small_dropdown_width,
                        theme.ui.text,
                        line_style_preview(dialog.settings.crosshair.line_style),
                        |ui| {
                            for ls in LineStyle::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.crosshair.line_style,
                                    *ls,
                                    l10n::text(ui.ctx(), line_style_preview(*ls)),
                                );
                            }
                        },
                    );

                    ui.add_space(ui.active_theme().spacing.lg);

                    // Line width slider
                    let mut lw = dialog.settings.crosshair.line_width;
                    ui.add(
                        egui::DragValue::new(&mut lw)
                            .range(1.0..=4.0)
                            .speed(0.5)
                            .suffix(l10n::text(ui.ctx(), " px")),
                    );
                    dialog.settings.crosshair.line_width = lw;
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // WATERMARK
                self.draw_section_header(ui, "WATERMARK", theme);

                self.draw_setting_row_with_controls(ui, "Mode", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "watermark_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
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
                                    l10n::text(ui.ctx(), wm.name()),
                                );
                            }
                        },
                    );

                    ui.add_space(ui.active_theme().spacing.lg);

                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.watermark, "watermark")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.watermark.color = color;
                    }
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // SCALES APPEARANCE
                self.draw_section_header(ui, "SCALES APPEARANCE", theme);

                self.draw_setting_row_with_controls(ui, "Text", theme, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.scales_text, "scales_text")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.scales_appearance.text_color = color;
                    }
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // MARGINS (Pixel input fields)
                self.draw_section_header(ui, "MARGINS", theme);

                self.draw_setting_row_with_controls(ui, "Top margin (%)", theme, |ui, dialog| {
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
                    theme,
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
                    theme,
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

                ui.add_space(ui.active_theme().spacing.xxl);

                // NAVIGATION BUTTONS
                self.draw_section_header(ui, "NAVIGATION BUTTONS", theme);

                self.draw_setting_row_with_controls(ui, "Visibility", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "nav_visibility",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.btns.navigation.name(),
                        |ui| {
                            for vis in ButtonVisibility::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.btns.navigation,
                                    *vis,
                                    l10n::text(ui.ctx(), vis.name()),
                                );
                            }
                        },
                    );
                });
            });
        });
    }

    pub(super) fn draw_symbol_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                styled_checkbox(
                    ui,
                    &mut self.settings.show_price_targets,
                    "Analyst price targets",
                    theme.ui.text,
                );
                // CANDLES
                self.draw_section_header(ui, "CANDLES", theme);
                styled_checkbox(
                    ui,
                    &mut self.settings.candle_colors.color_by_previous_close,
                    "Color bars based on previous close",
                    theme.ui.text,
                );
                for (label, part) in [
                    ("Body", CandlePart::Body),
                    ("Borders", CandlePart::Border),
                    ("Wick", CandlePart::Wick),
                ] {
                    self.draw_candle_row(ui, label, part, theme);
                }

                ui.add_space(ui.active_theme().spacing.xxl);

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

    pub(super) fn draw_status_line_tab(&mut self, ui: &mut Ui, theme: &Theme) {
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
                let status_line = &mut self.settings.status_line;
                let shown = status_line.show_indicator_background;
                let opacity = &mut status_line.indicator_background_opacity;
                trading_option(
                    ui,
                    0.0,
                    &mut status_line.show_indicator_background,
                    "Background",
                    theme.ui.text,
                    |ui| {
                        ui.add_enabled(shown, egui::Slider::new(opacity, 0..=100).suffix("%"))
                            .accessible_name(l10n::text(ui.ctx(), "Background opacity"));
                    },
                );
            });
        });
    }

    pub(super) fn draw_scales_and_lines_tab(&mut self, ui: &mut Ui, theme: &Theme) {
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

    pub(super) fn draw_events_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        self.draw_section_header(ui, "EVENT VISIBILITY", theme);
        let events = &mut self.settings.events;
        for (value, label) in [
            (&mut events.show_dividends, "Dividends"),
            (&mut events.show_splits, "Splits"),
            (&mut events.show_earnings, "Earnings"),
            (&mut events.show_earnings_breaks, "Earnings breaks"),
            (&mut events.show_economic_events, "Economic events"),
            (&mut events.show_news, "News"),
            (&mut events.show_news_notifications, "News notifications"),
            (&mut events.show_ideas, "Ideas"),
        ] {
            styled_checkbox(ui, value, label, theme.ui.text);
        }
    }

    pub(super) fn draw_trading_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        let text = theme.ui.text;
        self.draw_section_header(ui, "GENERAL", theme);
        styled_checkbox(
            ui,
            &mut self.settings.status_line.show_quote_boxes,
            "Buy/sell buttons",
            text,
        );
        let trading = &mut self.settings.trading;
        styled_checkbox(
            ui,
            &mut trading.instant_orders,
            "Instant orders placement",
            text,
        );
        let sounds = &self.execution_sounds;
        let play = trading.play_execution_sound;
        trading_option(
            ui,
            0.0,
            &mut trading.play_execution_sound,
            "Play sound for executions",
            text,
            |ui| {
                if let Some(first) = sounds.first() {
                    let sound = &mut trading.execution_sound;
                    let selected = if sound.is_empty() { first } else { &*sound }.clone();
                    ui.add_enabled_ui(play, |ui| {
                        styled_dropdown(ui, "execution_sound", 140.0, text, &selected, |ui| {
                            for name in sounds {
                                if ui
                                    .selectable_label(selected == *name, l10n::text(ui.ctx(), name))
                                    .clicked()
                                {
                                    sound.clone_from(name);
                                }
                            }
                        });
                    });
                }
            },
        );
        ui.horizontal(|ui| {
            ui.set_min_height(42.0);
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Notifications"))
                    .size(ui.active_theme().text.title)
                    .color(text),
            );
            ui.add_space((TRADING_LABEL_WIDTH - ui.min_rect().width()).max(0.0));
            let level = &mut trading.notifications;
            styled_dropdown(
                ui,
                "trading_notifications",
                180.0,
                text,
                level.label(),
                |ui| {
                    for option in TradingNotifications::ALL {
                        ui.selectable_value(level, option, l10n::text(ui.ctx(), option.label()));
                    }
                },
            );
        });
        ui.add_space(ui.active_theme().spacing.sm);

        ui.add_space(theme.spacing.xxxl);
        self.draw_section_header(ui, "APPEARANCE", theme);
        let trading = &mut self.settings.trading;
        styled_checkbox(ui, &mut trading.bid_ask_labels, "Bid and ask labels", text);
        styled_checkbox(ui, &mut trading.show_positions, "Positions", text);
        ui.add_enabled_ui(trading.show_positions, |ui| {
            let unit = &mut trading.position_pnl_unit;
            let shown = trading.position_pnl;
            trading_option(
                ui,
                TRADING_INDENT,
                &mut trading.position_pnl,
                "Profit & loss",
                text,
                |ui| pnl_unit_dropdown(ui, "position_pnl_unit", shown, unit, text),
            );
            trading_option(
                ui,
                TRADING_INDENT,
                &mut trading.reverse_button,
                "Reverse button on hover",
                text,
                |_| {},
            );
        });
        styled_checkbox(ui, &mut trading.show_orders, "Orders", text);
        ui.add_enabled_ui(trading.show_orders, |ui| {
            let unit = &mut trading.bracket_pnl_unit;
            let shown = trading.bracket_pnl;
            trading_option(
                ui,
                0.0,
                &mut trading.bracket_pnl,
                "Brackets profit & loss",
                text,
                |ui| pnl_unit_dropdown(ui, "bracket_pnl_unit", shown, unit, text),
            );
        });
        styled_checkbox(ui, &mut trading.show_executions, "Executions", text);
        ui.add_enabled_ui(trading.show_executions, |ui| {
            trading_option(
                ui,
                TRADING_INDENT,
                &mut trading.show_execution_labels,
                "Executions labels",
                text,
                |_| {},
            );
        });
    }

    pub(super) fn draw_alerts_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "ALERT SETTINGS", theme);

                self.draw_setting_row_with_controls(ui, "Color", theme, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.alert, "alert")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.alerts.alert_color = color;
                    }
                });

                ui.add_space(ui.active_theme().spacing.xxl);
                self.draw_section_header(ui, "VISIBILITY", theme);

                styled_checkbox(
                    ui,
                    &mut self.settings.alerts.show_alerts,
                    "Show alerts",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.alerts.show_alert_labels,
                    "Show alert labels",
                    theme.ui.text,
                );
                styled_checkbox(
                    ui,
                    &mut self.settings.alerts.show_active_only,
                    "Show active alerts only",
                    theme.ui.text,
                );

                ui.add_space(ui.active_theme().spacing.xxl);
                self.draw_section_header(ui, "NOTIFICATIONS", theme);

                self.draw_setting_row_with_controls(ui, "Sound volume", theme, |ui, dialog| {
                    ui.add(
                        egui::Slider::new(&mut dialog.settings.alerts.sound_volume, 0..=100)
                            .suffix("%"),
                    );
                });
                self.draw_setting_row_with_controls(ui, "Lifetime", theme, |ui, dialog| {
                    ui.add(
                        egui::DragValue::new(&mut dialog.settings.alerts.notification_seconds)
                            .range(1..=300)
                            .suffix(l10n::text(ui.ctx(), " s")),
                    );
                });
            });
        });
    }

    pub(super) fn draw_section_header(&self, ui: &mut Ui, title: &str, theme: &Theme) {
        ui.label(
            RichText::new(l10n::text(ui.ctx(), title))
                .size(theme.text.small)
                .strong()
                .color(theme.ui.text_secondary),
        );
        ui.add_space(ui.active_theme().spacing.sm);
    }

    /// The width of the label column, which every tab shares: the longest label's width.
    fn label_width(ui: &Ui, theme: &Theme) -> f32 {
        [
            "Panel layout",
            "Extended-session price",
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
        ]
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    l10n::text(ui.ctx(), *label),
                    FontId::proportional(theme.text.title),
                    theme.ui.text,
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
    }

    pub(super) fn draw_setting_row_with_controls<F>(
        &mut self,
        ui: &mut Ui,
        label: &str,
        theme: &Theme,
        add_content: F,
    ) where
        F: FnOnce(&mut Ui, &mut SettingsDialog),
    {
        let label_width = Self::label_width(ui, theme);
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
                            RichText::new(l10n::text(ui.ctx(), label))
                                .size(theme.text.title)
                                .color(theme.ui.text),
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
        ui.add_space(ui.active_theme().spacing.sm);
    }
}

/// A part of the candles on the Symbol tab.
#[derive(Clone, Copy)]
enum CandlePart {
    Body,
    Border,
    Wick,
}

/// Height of a candle part row, which with the spacing below it gives a 50 point pitch.
const CANDLE_ROW_HEIGHT: f32 = 38.0;

impl SettingsDialog {
    /// A Symbol tab row for one candle part: its visibility checkbox in the label column, then
    /// its rising and falling colours, which are disabled while the part is hidden.
    fn draw_candle_row(&mut self, ui: &mut Ui, label: &str, part: CandlePart, theme: &Theme) {
        let label_width = Self::label_width(ui, theme);
        let colors = &mut self.settings.candle_colors;
        let pickers = &mut self.color_picker_states;
        let (shown, [up, down], [up_picker, down_picker], [up_id, down_id]) = match part {
            CandlePart::Body => (
                &mut colors.show_body,
                [&mut colors.body_up, &mut colors.body_down],
                [&mut pickers.body_up, &mut pickers.body_down],
                ["body_up", "body_down"],
            ),
            CandlePart::Border => (
                &mut colors.show_border,
                [&mut colors.border_up, &mut colors.border_down],
                [&mut pickers.border_up, &mut pickers.border_down],
                ["border_up", "border_down"],
            ),
            CandlePart::Wick => (
                &mut colors.show_wick,
                [&mut colors.wick_up, &mut colors.wick_down],
                [&mut pickers.wick_up, &mut pickers.wick_down],
                ["wick_up", "wick_down"],
            ),
        };
        ui.horizontal(|ui| {
            let checkbox = ui
                .allocate_ui_with_layout(
                    egui::vec2(label_width, CANDLE_ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(label_width);
                        ui.checkbox(
                            shown,
                            RichText::new(l10n::text(ui.ctx(), label))
                                .size(theme.text.title)
                                .color(theme.ui.text),
                        )
                    },
                )
                .inner;
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().interact_size.y = 32.0;
            ui.add_enabled_ui(*shown, |ui| {
                ui.label_inputs_by(checkbox.id, |ui| {
                    for (color, picker, id) in
                        [(up, up_picker, up_id), (down, down_picker, down_id)]
                    {
                        if let Some(picked) = ColorPicker::new(picker, id).show(ui).inner {
                            *color = picked;
                        }
                    }
                });
            });
        });
        ui.add_space(theme.spacing.sm);
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
            ui.selectable_value(&mut mode, option, l10n::text(ui.ctx(), name(option)));
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
            RichText::new(l10n::text(ui.ctx(), selected))
                .size(ui.active_theme().text.title)
                .color(text_color),
        )
        .show_ui(ui, add_contents);
}

/// The width of the Trading tab's labels, where its dropdowns start.
const TRADING_LABEL_WIDTH: f32 = 264.0;
/// How far the Trading tab's sub-options sit right of their group's checkbox.
const TRADING_INDENT: f32 = 32.0;

/// A Trading tab row: a checkbox `indent` from the left and, from
/// [`TRADING_LABEL_WIDTH`], the controls `add_controls` adds.
fn trading_option(
    ui: &mut Ui,
    indent: f32,
    checked: &mut bool,
    label: &str,
    text_color: Color32,
    add_controls: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_height(42.0);
        ui.add_space(indent);
        ui.checkbox(
            checked,
            RichText::new(l10n::text(ui.ctx(), label))
                .size(ui.active_theme().text.title)
                .color(text_color),
        );
        ui.add_space((TRADING_LABEL_WIDTH - ui.min_rect().width()).max(0.0));
        add_controls(ui);
    });
    ui.add_space(ui.active_theme().spacing.sm);
}

/// The profit and loss unit dropdown of a Trading tab row, enabled while `shown`.
fn pnl_unit_dropdown(ui: &mut Ui, id: &str, shown: bool, unit: &mut PnlUnit, text_color: Color32) {
    ui.add_enabled_ui(shown, |ui| {
        styled_dropdown(ui, id, 140.0, text_color, unit.label(), |ui| {
            for option in PnlUnit::ALL {
                ui.selectable_value(unit, option, l10n::text(ui.ctx(), option.label()));
            }
        });
    });
}

fn styled_checkbox(ui: &mut Ui, checked: &mut bool, label: &str, text_color: Color32) -> bool {
    let changed = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 42.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(42.0);
                ui.checkbox(
                    checked,
                    RichText::new(l10n::text(ui.ctx(), label))
                        .size(ui.active_theme().text.title)
                        .color(text_color),
                )
                .changed()
            },
        )
        .inner;
    ui.add_space(ui.active_theme().spacing.sm);
    changed
}

fn styled_text_input(ui: &mut Ui, text: &mut String, width: f32) {
    ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(width)
            .font(FontId::proportional(ui.active_theme().text.body)),
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
