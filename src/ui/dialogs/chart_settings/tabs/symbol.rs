//! Symbol and status line tabs of the chart settings dialog.

use super::super::SettingsDialog;
use super::{checkbox, pick_color, styled_checkbox, styled_dropdown};
use crate::localization as l10n;
use crate::scales::TimezoneMode;
use crate::theme::Theme;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::timeframe_toolbar::SessionType;
use crate::widget::settings::PrecisionMode;
use egui::{Color32, RichText, Ui};

/// Height of a candle part row, giving a 50 point pitch with the spacing below it.
const CANDLE_ROW_HEIGHT: f32 = 38.0;

impl SettingsDialog {
    pub(in super::super) fn draw_symbol_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                if self.price_targets_offered {
                    styled_checkbox(
                        ui,
                        &mut self.settings.show_price_targets,
                        "Analyst price targets",
                        theme.ui.text,
                    );
                    let targets_shown = self.settings.show_price_targets;
                    ui.add_enabled_ui(targets_shown, |ui| {
                        self.draw_setting_row_with_controls(ui, "Positive", theme, |ui, dialog| {
                            let mut color = dialog
                                .settings
                                .price_target_positive_color
                                .unwrap_or(Color32::PLACEHOLDER);
                            if pick_color(
                                ui,
                                &mut dialog.color_picker_states.price_target_positive,
                                "price_target_positive",
                                &mut color,
                            ) {
                                dialog.settings.price_target_positive_color = Some(color);
                            }
                        });
                        self.draw_setting_row_with_controls(ui, "Negative", theme, |ui, dialog| {
                            let mut color = dialog
                                .settings
                                .price_target_negative_color
                                .unwrap_or(Color32::PLACEHOLDER);
                            if pick_color(
                                ui,
                                &mut dialog.color_picker_states.price_target_negative,
                                "price_target_negative",
                                &mut color,
                            ) {
                                dialog.settings.price_target_negative_color = Some(color);
                            }
                        });
                    });
                }
                let kind = self.settings.chart_type;
                if kind.is_time_independent()
                    || !kind.uses_ohlc()
                    || matches!(
                        kind,
                        crate::model::ChartType::HighLow
                            | crate::model::ChartType::HlcArea
                            | crate::model::ChartType::TimePriceOpportunity
                            | crate::model::ChartType::VolumeFootprint
                            | crate::model::ChartType::SessionVolumeProfile
                    )
                {
                    // Settings without a shared source take the one the dialog shows, so Ok
                    // applies the source shown.
                    let shared = *self.settings.price_source.get_or_insert_default();
                    let params = self.settings.chart_type_params.clone().unwrap_or_default();
                    let mut series = crate::chart::series::SeriesSettings {
                        chart_type: kind,
                        price_source: params.styles.line_source(kind, shared),
                        chart_type_params: params,
                        bullish_color: self.settings.candle_colors.body_up,
                        bearish_color: self.settings.candle_colors.body_down,
                        ..Default::default()
                    };
                    crate::ui::dialogs::series_settings::SymbolTab::show(ui, &mut series);
                    series.chart_type_params.styles.set_line_source(
                        kind,
                        series.price_source,
                        shared,
                    );
                    self.settings.chart_type_params = Some(series.chart_type_params);
                    self.settings.candle_colors.body_up = series.bullish_color;
                    self.settings.candle_colors.body_down = series.bearish_color;
                    ui.add_space(ui.active_theme().spacing.xxl);
                } else {
                    let heikin = kind == crate::model::ChartType::Heikin;
                    let hollow = kind == crate::model::ChartType::HollowCandles;
                    let volume = kind == crate::model::ChartType::VolumeCandles;
                    let bars = kind == crate::model::ChartType::Bars;
                    let candles =
                        kind == crate::model::ChartType::Candles || heikin || hollow || volume;
                    self.draw_section_header(
                        ui,
                        if heikin {
                            "HEIKIN ASHI"
                        } else if hollow {
                            "HOLLOW CANDLES"
                        } else if volume {
                            "VOLUME CANDLES"
                        } else if bars {
                            "BARS"
                        } else if candles {
                            "CANDLES"
                        } else {
                            "COLORS"
                        },
                        theme,
                    );
                    if heikin {
                        let style = &mut self
                            .settings
                            .chart_type_params
                            .get_or_insert_with(Default::default)
                            .styles
                            .heikin;
                        styled_checkbox(
                            ui,
                            &mut style.real_price_only,
                            "Use original prices on scale",
                            theme.ui.text,
                        );
                    }
                    // Hollow candles always compare the close with the previous close.
                    if (candles && !hollow) || bars {
                        let previous_close = if bars || volume || heikin {
                            let styles = &mut self
                                .settings
                                .chart_type_params
                                .get_or_insert_with(Default::default)
                                .styles;
                            if bars {
                                &mut styles.bars.color_by_previous_close
                            } else if volume {
                                &mut styles.volume_candles.color_by_previous_close
                            } else {
                                &mut styles.heikin.color_by_previous_close
                            }
                        } else {
                            &mut self.settings.candle_colors.color_by_previous_close
                        };
                        if styled_checkbox(
                            ui,
                            previous_close,
                            "Color bars based on previous close",
                            theme.ui.text,
                        ) && kind == crate::model::ChartType::Candles
                        {
                            // Templates can be saved before Ok applies the edit to the chart.
                            self.settings
                                .chart_type_params
                                .get_or_insert_with(Default::default)
                                .styles
                                .candles
                                .color_by_previous_close =
                                self.settings.candle_colors.color_by_previous_close;
                        }
                    }
                    if bars {
                        styled_checkbox(
                            ui,
                            &mut self
                                .settings
                                .chart_type_params
                                .get_or_insert_with(Default::default)
                                .styles
                                .bars
                                .open_visible,
                            "Show open",
                            theme.ui.text,
                        );
                    }
                    let label_width = Self::label_width(ui, theme);
                    let colors = &mut self.settings.candle_colors;
                    let pickers = &mut self.color_picker_states;
                    for (label, shown, [up, down], [up_picker, down_picker], [up_id, down_id]) in [
                        (
                            if bars { "Up and down colors" } else { "Body" },
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
                    ]
                    .into_iter()
                    // A bar is one stroke, so it has no separate border or wick colours.
                    .take(if bars { 1 } else { 3 })
                    {
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
                                            checkbox(ui, shown, label)
                                        } else {
                                            ui.label(label)
                                        }
                                    },
                                )
                                .inner;
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.spacing_mut().interact_size.y =
                                DESIGN_TOKENS.sizing.settings_dialog.button_height;
                            ui.add_enabled_ui(!candles || *shown, |ui| {
                                ui.label_inputs_by(checkbox.id, |ui| {
                                    for (color, picker, id) in
                                        [(up, up_picker, up_id), (down, down_picker, down_id)]
                                    {
                                        pick_color(ui, picker, id, color);
                                    }
                                });
                            });
                        });
                        ui.add_space(theme.spacing.sm);
                    }
                    if bars {
                        styled_checkbox(
                            ui,
                            &mut self
                                .settings
                                .chart_type_params
                                .get_or_insert_with(Default::default)
                                .styles
                                .bars
                                .thin_bars,
                            "Thin bars",
                            theme.ui.text,
                        );
                    }

                    ui.add_space(ui.active_theme().spacing.xxl);
                }

                // DATA MODIFICATION
                self.draw_section_header(ui, "DATA MODIFICATION", theme);
                if self.hours_offered.0 {
                    self.draw_setting_row_with_controls(ui, "Session", theme, |ui, dialog| {
                        let current = SessionType::from_hours(
                            dialog.settings.extended_hours.unwrap_or(false),
                            dialog.settings.overnight_hours.unwrap_or(false),
                        );
                        let overnight = dialog.hours_offered.1 || current == SessionType::Overnight;
                        styled_dropdown(
                            ui,
                            "session",
                            DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                            theme.ui.text,
                            current.choice_label(),
                            |ui| {
                                for &choice in SessionType::choices(overnight) {
                                    if ui
                                        .selectable_label(
                                            choice == current,
                                            l10n::text(ui.ctx(), choice.choice_label()),
                                        )
                                        .clicked()
                                        && let Some((extended, overnight)) = choice.hours()
                                    {
                                        dialog.settings.extended_hours = Some(extended);
                                        dialog.settings.overnight_hours = Some(overnight);
                                    }
                                }
                            },
                        );
                    });
                }
                styled_checkbox(
                    ui,
                    &mut self.settings.adjust_for_dividends,
                    "Adjust data for dividends",
                    theme.ui.text,
                );
                if self.futures_data {
                    styled_checkbox(
                        ui,
                        &mut self.settings.back_adjust_futures,
                        "Back-adjust continuous futures",
                        theme.ui.text,
                    );
                    styled_checkbox(
                        ui,
                        &mut self.settings.settlement_as_close,
                        "Settlement as close",
                        theme.ui.text,
                    );
                }
                ui.add_space(ui.active_theme().spacing.xxl);

                if self.hours_offered.0 {
                    self.draw_section_header(ui, "SESSION BACKGROUNDS", theme);
                    let mut background =
                        self.settings.session_background.clone().unwrap_or_default();
                    let before = background.clone();
                    styled_checkbox(
                        ui,
                        &mut background.show_extended_hours,
                        "Highlight extended sessions",
                        theme.ui.text,
                    );
                    let fallback = |color, opacity| {
                        if color == Color32::PLACEHOLDER {
                            theme.ui.accent.linear_multiply(opacity)
                        } else {
                            color
                        }
                    };
                    let shared = background.extended_hours_color;
                    let label_width = Self::label_width(ui, theme);
                    ui.add_enabled_ui(background.show_extended_hours, |ui| {
                        for (index, label, mut color, shown) in [
                            (
                                0,
                                "Pre-market",
                                background.premarket_color.unwrap_or(shared),
                                &mut background.shade_premarket,
                            ),
                            (
                                1,
                                "Post-market",
                                background.postmarket_color.unwrap_or(shared),
                                &mut background.shade_postmarket,
                            ),
                            (
                                2,
                                "Overnight",
                                background.overnight_hours_color,
                                &mut background.shade_overnight,
                            ),
                        ] {
                            ui.horizontal(|ui| {
                                let label = ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(label_width, 42.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(label_width);
                                            ui.set_max_width(label_width);
                                            let label = RichText::new(l10n::text(ui.ctx(), label))
                                                .size(theme.text.title)
                                                .color(theme.ui.text);
                                            checkbox(ui, shown, label)
                                        },
                                    )
                                    .inner;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    ui.spacing_mut().interact_size.y =
                                        DESIGN_TOKENS.sizing.settings_dialog.button_height;
                                    ui.add_enabled_ui(*shown, |ui| {
                                        ui.label_inputs_by(label.id, |ui| {
                                            let picker = &mut self
                                                .color_picker_states
                                                .session_background[index];
                                            picker.sel_color = fallback(
                                                color,
                                                if index == 2 { 0.16 } else { 0.08 },
                                            );
                                            if pick_color(
                                                ui,
                                                picker,
                                                &format!("session_background_{index}"),
                                                &mut color,
                                            ) {
                                                match index {
                                                    0 => background.premarket_color = Some(color),
                                                    1 => background.postmarket_color = Some(color),
                                                    _ => background.overnight_hours_color = color,
                                                }
                                            }
                                        });
                                    });
                                });
                            });
                            ui.add_space(ui.active_theme().spacing.sm);
                        }
                    });
                    if background != before {
                        self.settings.session_background = Some(background);
                    }
                    ui.add_space(ui.active_theme().spacing.xxl);
                }

                // PRECISION
                self.draw_section_header(ui, "PRECISION", theme);

                self.draw_setting_row_with_controls(ui, "Mode", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "precision_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        &dialog.settings.precision.name(),
                        |ui| {
                            // A precision the list lacks keeps its own row while the chart holds
                            // it, so that it can be chosen again.
                            let held = Some(dialog.opened_precision)
                                .filter(|mode| matches!(mode, PrecisionMode::Other(_)));
                            for pm in PrecisionMode::all().iter().copied().chain(held) {
                                ui.selectable_value(
                                    &mut dialog.settings.precision,
                                    pm,
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
                    // A zone this build cannot read is shown as saved, with no row selected.
                    let current = dialog.settings.timezone_mode().ok();
                    let rows = crate::ui::timeframe_toolbar::toolbar::zone_rows(
                        &dialog.timezones,
                        current.as_ref().unwrap_or(&TimezoneMode::Utc),
                    );
                    let shown = rows
                        .iter()
                        .find(|(_, mode)| Some(mode) == current.as_ref())
                        .map_or_else(
                            || dialog.settings.timezone.clone(),
                            |(label, _)| label.clone(),
                        );
                    styled_dropdown(
                        ui,
                        "timezone",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        &shown,
                        |ui| {
                            for (label, mode) in rows {
                                let selected = Some(&mode) == current.as_ref();
                                if ui
                                    .selectable_label(selected, l10n::text(ui.ctx(), &label))
                                    .clicked()
                                {
                                    dialog.settings.set_timezone_mode(&mode);
                                }
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
                    &mut self.settings.status_line.show_market_status,
                    "Market status",
                    theme.ui.text,
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, ChartType};
    use crate::ui::top_toolbar::components::settings_menu::actions::SettingsAction;
    use crate::widget::{Chart, settings::ChartSettingsState};
    use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};

    fn frame(
        ctx: &Context,
        chart: &mut Chart,
        dialog: &mut SettingsDialog,
        theme: &Theme,
        events: Vec<Event>,
        labels: &[&str],
    ) -> Vec<Pos2> {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
                if let SettingsAction::Apply(settings) = dialog.show(ui.ctx(), theme) {
                    chart.begin_user_action();
                    settings.apply_to_chart(chart).unwrap();
                    chart.end_user_action();
                }
            },
        );
        let positions = labels
            .iter()
            .map(|label| {
                output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        Shape::Text(text) if text.galley.text() == *label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing control: {label}"))
            })
            .collect();
        output.drop_without_applying_deltas();
        positions
    }

    fn click(
        ctx: &Context,
        chart: &mut Chart,
        dialog: &mut SettingsDialog,
        theme: &Theme,
        label: &str,
    ) {
        let pos = frame(ctx, chart, dialog, theme, vec![], &[label])[0];
        frame(
            ctx,
            chart,
            dialog,
            theme,
            vec![Event::PointerMoved(pos)],
            &[],
        );
        for pressed in [true, false] {
            frame(
                ctx,
                chart,
                dialog,
                theme,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
                &[],
            );
        }
    }

    /// Accepting a session choice reloads its history and undoes in one step; continuously
    /// traded instruments have no Session row.
    #[test]
    fn session_row_applies_reloads_and_undoes_the_hours() {
        use crate::model::{Instrument, Session, SessionWindow, Timeframe};
        struct Hours {
            overnight_windows: bool,
            requests: std::sync::Mutex<Vec<(bool, bool)>>,
        }
        impl crate::data::Datafeed for Hours {
            fn resolve(&self, symbol: &str, reply: crate::data::Reply<Instrument>) {
                let at = |hour| chrono::NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
                let mut session =
                    Session::new([SessionWindow::new([chrono::Weekday::Mon], at(9), at(16))]);
                if self.overnight_windows {
                    session = session.with_overnight_windows([SessionWindow::new(
                        [chrono::Weekday::Mon],
                        at(20),
                        at(4),
                    )]);
                }
                reply.send(Ok(Instrument::new(symbol).with_session(session)));
            }
            fn history(
                &self,
                request: crate::data::HistoryRequest,
                reply: crate::data::Reply<crate::data::HistoryPage>,
            ) {
                self.requests
                    .lock()
                    .unwrap()
                    .push((request.extended_hours, request.overnight_hours));
                reply.send(Ok(crate::data::HistoryPage::new(
                    vec![Bar::new(
                        chrono::DateTime::UNIX_EPOCH,
                        100.0,
                        110.0,
                        90.0,
                        105.0,
                        1.0,
                    )],
                    true,
                )));
            }
            fn subscribe(&self, _: crate::data::Subscription) {}
        }
        for (overnight_windows, label, expected) in [
            (false, "Extended trading hours", (true, false)),
            (true, "Overnight trading hours", (true, true)),
        ] {
            let feed = std::sync::Arc::new(Hours {
                overnight_windows,
                requests: Default::default(),
            });
            let mut chart = crate::ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("A")
                .with_timeframe(Timeframe::Min1)
                .build();
            let ctx = Context::default();
            let theme = Theme::dark();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut dialog = SettingsDialog::new();
            for _ in 0..4 {
                frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
            }
            assert_eq!(feed.requests.lock().unwrap().last(), Some(&(false, false)));
            dialog.set_control_visible(
                crate::ui::dialogs::chart_settings::Control::Templates,
                false,
            );
            dialog.set_feed_capabilities(chart.feed_capabilities(), chart.instrument());
            dialog.open(ChartSettingsState::from_chart(&chart));
            for _ in 0..3 {
                frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
            }
            click(
                &ctx,
                &mut chart,
                &mut dialog,
                &theme,
                "Regular trading hours",
            );
            click(&ctx, &mut chart, &mut dialog, &theme, label);
            click(&ctx, &mut chart, &mut dialog, &theme, "Ok");
            assert_eq!((chart.extended_hours(), chart.overnight_hours()), expected);
            frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
            assert_eq!(feed.requests.lock().unwrap().last(), Some(&expected));
            assert!(chart.undo());
            frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
            assert_eq!(
                (chart.extended_hours(), chart.overnight_hours()),
                (false, false)
            );
            assert_eq!(feed.requests.lock().unwrap().last(), Some(&(false, false)));
            assert!(!chart.can_undo());

            dialog.set_feed_capabilities(None, Some(&Instrument::new("BTC")));
            dialog.open(ChartSettingsState::from_chart(&chart));
            for _ in 0..2 {
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    dialog.show(ui.ctx(), &theme);
                });
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect();
                assert!(!texts.contains(&"Session"));
                assert!(texts.contains(&"Adjust data for dividends"));
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Real pointer edits reach the painter only after acceptance; Cancel keeps the original
    /// component and scale settings in both themes. Templates saved before Ok retain each
    /// chart type's independent previous-close rule.
    #[test]
    fn candle_dialogs_accept_and_cancel_component_and_scale_edits() {
        for theme in [Theme::light(), Theme::dark()] {
            for (kind, label) in [
                (ChartType::Candles, "Color bars based on previous close"),
                (ChartType::Heikin, "Body"),
                (ChartType::Heikin, "Borders"),
                (ChartType::Heikin, "Wick"),
                (ChartType::Heikin, "Color bars based on previous close"),
                (ChartType::Heikin, "Use original prices on scale"),
                (ChartType::Heikin, "Reset to defaults"),
                (ChartType::VolumeCandles, "Body"),
                (ChartType::VolumeCandles, "Borders"),
                (ChartType::VolumeCandles, "Wick"),
                (
                    ChartType::VolumeCandles,
                    "Color bars based on previous close",
                ),
                (ChartType::VolumeCandles, "Reset to defaults"),
            ] {
                for accept in [false, true] {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
                        chrono::DateTime::UNIX_EPOCH,
                        10.0,
                        18.0,
                        8.0,
                        14.0,
                        0.0,
                    )]));
                    chart.set_chart_type(kind);
                    chart.config_mut().show_volume = false;
                    let mut params = chart.chart_type_params().clone();
                    // A border in the colour of an opaque body paints nothing.
                    params.styles.heikin.border_up = egui::Color32::from_rgb(3, 4, 102);
                    params.styles.volume_candles.border_up = egui::Color32::from_rgb(3, 4, 102);
                    if label == "Reset to defaults" {
                        params.styles.heikin.body_visible = false;
                        params.styles.heikin.border_visible = false;
                        params.styles.heikin.wick_visible = false;
                        params.styles.heikin.real_price_only = true;
                        params.styles.volume_candles.body_visible = false;
                        params.styles.volume_candles.border_visible = false;
                        params.styles.volume_candles.wick_visible = false;
                    }
                    chart.set_chart_type_params(params);
                    let before = ChartSettingsState::from_chart(&chart);
                    let mut dialog = SettingsDialog::new();
                    dialog.set_control_visible(crate::ui::Control::Templates, false);
                    dialog.open(before.clone());
                    for _ in 0..3 {
                        frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
                    }
                    click(&ctx, &mut chart, &mut dialog, &theme, label);
                    if label == "Color bars based on previous close" {
                        // Saving before Ok must carry the edit even when loaded on another type.
                        let saved = serde_json::to_string(dialog.settings()).unwrap();
                        for destination in [
                            ChartType::Candles,
                            ChartType::Bars,
                            ChartType::Heikin,
                            ChartType::VolumeCandles,
                        ] {
                            let mut loaded: ChartSettingsState =
                                serde_json::from_str(&saved).unwrap();
                            loaded.chart_type = destination;
                            let mut restored = Chart::new(BarData::new());
                            restored.set_chart_type(destination);
                            loaded.apply_to_chart(&mut restored).unwrap();
                            let styles = &restored.chart_type_params().styles;
                            assert_eq!(
                                styles.candles.color_by_previous_close,
                                kind == ChartType::Candles,
                                "{kind:?} to {destination:?}"
                            );
                            assert_eq!(
                                styles.heikin.color_by_previous_close,
                                kind == ChartType::Heikin
                            );
                            assert_eq!(
                                styles.volume_candles.color_by_previous_close,
                                kind == ChartType::VolumeCandles
                            );
                            assert!(!styles.bars.color_by_previous_close);
                        }
                    }
                    click(
                        &ctx,
                        &mut chart,
                        &mut dialog,
                        &theme,
                        if accept { "Ok" } else { "Cancel" },
                    );
                    assert!(!dialog.is_open);
                    let after = ChartSettingsState::from_chart(&chart);
                    // Theme colours only apply once a frame has run, so compare the candle
                    // settings rather than the whole state.
                    let style = after.candle_colors.clone();
                    let mut expected = before.candle_colors.clone();
                    if accept {
                        match label {
                            "Body" => expected.show_body = false,
                            "Borders" => expected.show_border = false,
                            "Wick" => expected.show_wick = false,
                            "Color bars based on previous close" => {
                                expected.color_by_previous_close = kind == ChartType::Candles;
                            }
                            "Use original prices on scale" => {}
                            "Reset to defaults" => {
                                expected = crate::widget::settings::CandleColorConfig {
                                    show_border: true,
                                    ..Default::default()
                                }
                            }
                            _ => unreachable!(),
                        }
                    }
                    assert_eq!(style, expected);
                    let styles = &chart.chart_type_params().styles;
                    assert_eq!(
                        if kind == ChartType::Heikin {
                            styles.heikin.color_by_previous_close
                        } else if kind == ChartType::VolumeCandles {
                            styles.volume_candles.color_by_previous_close
                        } else {
                            styles.candles.color_by_previous_close
                        },
                        accept && label == "Color bars based on previous close"
                    );
                    assert_eq!(
                        styles.candles.color_by_previous_close,
                        accept && kind == ChartType::Candles
                    );
                    assert_eq!(
                        chart.chart_type_params().styles.heikin.real_price_only,
                        match label {
                            "Use original prices on scale" => accept,
                            "Reset to defaults" => !accept,
                            _ => false,
                        }
                    );
                    // Paint the accepted state after the modal has closed.
                    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    let body = painted.iter().any(|s| {
                        matches!(s, Shape::Rect(r)
                        if plot.contains(r.rect.center()) && r.fill == style.body_up)
                    });
                    assert_eq!(body, style.show_body);
                    let border = painted.iter().any(|s| {
                        matches!(s, Shape::Rect(r)
                        if plot.contains(r.rect.center()) && r.stroke.color == style.border_up)
                    });
                    let wick = painted.iter().any(|s| matches!(s, Shape::LineSegment { points, stroke }
                        if plot.contains(points[0]) && points[0].x == points[1].x && stroke.color == style.wick_up));
                    // Only the defaults restored by the reset share the body's colour.
                    assert_eq!(
                        border,
                        style.show_border && style.border_up != style.body_up
                    );
                    assert_eq!(wick, style.show_wick);
                }
            }
        }
    }

    /// Each switch of the HLC area hides its own line or fill through the dialog and the painter
    /// after acceptance only, in both themes, while undo and saved charts keep every value apart.
    #[test]
    fn hlc_area_dialog_edits_reach_the_painter_and_keep_independent_saved_values() {
        use crate::model::{HlcAreaStyle, LinePlotStyle};
        use crate::widget::ChartSnapshot;
        use egui::Color32;
        use egui::epaint::ColorMode;

        let line = |r, width| LinePlotStyle {
            color: Color32::from_rgb(r, 2, 3),
            width,
            ..Default::default()
        };
        let edited = HlcAreaStyle {
            high_line: line(101, 1.0),
            low_line: line(102, 2.5),
            close_line: line(103, 4.0),
            high_to_close_fill: Color32::from_rgba_unmultiplied(104, 5, 6, 77),
            close_to_low_fill: Color32::from_rgba_unmultiplied(7, 105, 8, 128),
            ..Default::default()
        };
        type Edit = fn(&mut HlcAreaStyle);
        let cases: [(&str, Edit); 5] = [
            ("High line", |s| s.high_line_visible = false),
            ("Low line", |s| s.low_line_visible = false),
            ("High to close fill", |s| {
                s.high_to_close_fill_visible = false
            }),
            ("Close to low fill", |s| s.close_to_low_fill_visible = false),
            ("Reset to defaults", |s| *s = HlcAreaStyle::default()),
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for (label, edit) in cases {
                for accept in [false, true] {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    // High, close and low sit at unequal distances.
                    let mut chart = Chart::new(BarData::from_bars(
                        (0..3)
                            .map(|i| {
                                Bar::new(
                                    chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i),
                                    10.0,
                                    18.0,
                                    8.0,
                                    16.0,
                                    0.0,
                                )
                            })
                            .collect(),
                    ));
                    chart.set_chart_type(ChartType::HlcArea);
                    chart.config_mut().show_volume = false;
                    let mut params = chart.chart_type_params().clone();
                    params.styles.hlc_area = edited.clone();
                    chart.set_chart_type_params(params);
                    let mut dialog = SettingsDialog::new();
                    dialog.set_control_visible(crate::ui::Control::Templates, false);
                    dialog.open(ChartSettingsState::from_chart(&chart));
                    for _ in 0..3 {
                        frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
                    }
                    for control in [label, if accept { "Ok" } else { "Cancel" }] {
                        click(&ctx, &mut chart, &mut dialog, &theme, control);
                    }
                    assert!(!dialog.is_open);
                    let mut expected = edited.clone();
                    if accept {
                        edit(&mut expected);
                    }
                    let style = || chart.chart_type_params().styles.hlc_area.clone();
                    assert_eq!(style(), expected, "{label}");
                    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
                    let stroked = |color| {
                        painted.iter().any(|s| {
                            matches!(s, Shape::Path(p)
                                if p.stroke.width > 0.0 && p.stroke.color == ColorMode::Solid(color))
                        })
                    };
                    let filled = |color| {
                        painted
                            .iter()
                            .any(|s| matches!(s, Shape::Path(p) if p.fill == color))
                    };
                    assert_eq!(
                        stroked(expected.high_line.color),
                        expected.high_line_visible
                    );
                    assert_eq!(stroked(expected.low_line.color), expected.low_line_visible);
                    assert!(stroked(expected.close_line.color));
                    assert_eq!(
                        filled(expected.high_to_close_fill),
                        expected.high_to_close_fill_visible
                    );
                    assert_eq!(
                        filled(expected.close_to_low_fill),
                        expected.close_to_low_fill_visible
                    );
                    if accept {
                        let saved: ChartSnapshot = serde_json::from_str(
                            &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                        )
                        .unwrap();
                        assert!(chart.undo());
                        assert_eq!(chart.chart_type_params().styles.hlc_area, edited);
                        assert!(chart.redo());
                        chart.restore(&saved).unwrap();
                        assert_eq!(chart.chart_type_params().styles.hlc_area, expected);
                    }
                }
            }
        }
    }

    /// Choosing a gradient line in the Symbol tab reaches the painter only after Ok, in both
    /// themes: the leftmost segment takes the start colour, the rightmost the end colour and the
    /// two between them the blends a third and two thirds of the way. Undo, saved layouts and
    /// switching chart type keep the colour type and both colours, and documents saved without
    /// them draw a solid line.
    #[test]
    fn line_gradient_dialog_edits_reach_the_painter_and_survive_undo_and_saves() {
        use crate::widget::ChartSnapshot;
        use egui::Color32;
        use egui::epaint::ColorMode;

        let start = Color32::from_rgb(210, 0, 0);
        let end = Color32::from_rgba_premultiplied(0, 0, 120, 120);
        // (1 - t) * start + t * end per premultiplied channel, at t = 1/3 and t = 2/3.
        let gradient = vec![
            start,
            Color32::from_rgba_premultiplied(140, 0, 40, 210),
            Color32::from_rgba_premultiplied(70, 0, 80, 165),
            end,
        ];
        // The colours of the line's strokes, from left to right.
        let line_colors = |ctx: &Context, chart: &mut Chart| {
            let mut strokes: Vec<(f32, Color32)> =
                crate::widget::test_frames::shapes(ctx, chart, 1.0, vec![])
                    .into_iter()
                    .filter_map(|shape| match shape {
                        Shape::Path(path) if path.stroke.width == 3.25 => match path.stroke.color {
                            ColorMode::Solid(color) => Some((path.points[0].x, color)),
                            ColorMode::UV(_) => None,
                        },
                        _ => None,
                    })
                    .collect();
            strokes.sort_by(|a, b| a.0.total_cmp(&b.0));
            strokes
                .into_iter()
                .map(|(_, color)| color)
                .collect::<Vec<_>>()
        };
        let kept = |chart: &Chart| {
            let styles = &chart.chart_type_params().styles;
            (
                styles.line_gradient,
                styles.line_gradient_start,
                styles.line_gradient_end,
            )
        };
        // Saved without the setting: removes its keys wherever a document holds the styles.
        fn forget(value: &mut serde_json::Value) -> usize {
            match value {
                serde_json::Value::Object(map) => {
                    let removed = usize::from(map.remove("line_gradient").is_some());
                    map.remove("line_gradient_start");
                    map.remove("line_gradient_end");
                    removed + map.values_mut().map(forget).sum::<usize>()
                }
                serde_json::Value::Array(items) => items.iter_mut().map(forget).sum(),
                _ => 0,
            }
        }
        for theme in [Theme::light(), Theme::dark()] {
            for accept in [false, true] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                // Five rising closes: four segments.
                let mut chart = Chart::new(BarData::from_bars(
                    (0..5)
                        .map(|i| {
                            Bar::new(
                                chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i),
                                10.0,
                                20.0,
                                5.0,
                                10.0 + i as f64,
                                0.0,
                            )
                        })
                        .collect(),
                ));
                chart.set_chart_type(ChartType::Line);
                chart.config_mut().show_volume = false;
                let mut params = chart.chart_type_params().clone();
                params.styles.line.width = 3.25;
                params.styles.line_gradient_start = start;
                params.styles.line_gradient_end = end;
                chart.set_chart_type_params(params);
                let solid = vec![chart.chart_type_params().styles.line.color];
                let mut dialog = SettingsDialog::new();
                dialog.set_control_visible(crate::ui::Control::Templates, false);
                dialog.open(ChartSettingsState::from_chart(&chart));
                for _ in 0..3 {
                    frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
                }
                // The colour type's "Solid" is painted before the line style's.
                for control in ["Solid", "Gradient", if accept { "Ok" } else { "Cancel" }] {
                    click(&ctx, &mut chart, &mut dialog, &theme, control);
                }
                assert!(!dialog.is_open);
                assert_eq!(kept(&chart), (accept, start, end));
                assert_eq!(
                    line_colors(&ctx, &mut chart),
                    if accept {
                        gradient.clone()
                    } else {
                        solid.clone()
                    }
                );
                if accept {
                    let saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
                    assert!(chart.undo());
                    assert_eq!(kept(&chart), (false, start, end));
                    assert_eq!(line_colors(&ctx, &mut chart), solid);
                    // Restored over a solid line, then switched to candles and back.
                    let restored: ChartSnapshot = serde_json::from_value(saved.clone()).unwrap();
                    chart.restore(&restored).unwrap();
                    chart.set_chart_type(ChartType::Candles);
                    chart.set_chart_type(ChartType::Line);
                    assert_eq!(kept(&chart), (true, start, end));
                    assert_eq!(line_colors(&ctx, &mut chart), gradient);
                    // The snapshot's own styles and those of its chart settings.
                    let mut legacy = saved;
                    assert!(forget(&mut legacy) >= 2);
                    let legacy: ChartSnapshot = serde_json::from_value(legacy).unwrap();
                    chart.restore(&legacy).unwrap();
                    assert!(!chart.chart_type_params().styles.line_gradient);
                    assert_eq!(line_colors(&ctx, &mut chart), solid);
                }
            }
        }
    }

    /// Bars colour by their own open or the previous close, and columns reach the selected price
    /// in their own colours while their colour still compares closes, through the dialog and the
    /// painter after acceptance only, in both themes. Undo and saved charts keep these values
    /// apart from the candle and line settings, and a layout saved before columns had colours
    /// draws them in its candle body colours.
    #[test]
    fn bars_and_columns_dialog_edits_reach_the_painter_and_keep_independent_saved_values() {
        use crate::model::{BarsStyle, ColumnsStyle, PriceSource};
        use crate::widget::ChartSnapshot;
        use egui::Color32;

        let (up, down) = (Color32::from_rgb(1, 100, 2), Color32::from_rgb(101, 2, 3));
        // The default column colours, rgb(8, 153, 129) and rgb(242, 54, 69) at 50% opacity.
        const COLUMN_UP: Color32 = Color32::from_rgba_premultiplied(4, 77, 65, 128);
        const COLUMN_DOWN: Color32 = Color32::from_rgba_premultiplied(121, 27, 35, 128);
        let chart_of = |kind, bars: &[(f64, f64, f64, f64)]| {
            let mut chart = Chart::new(BarData::from_bars(
                bars.iter()
                    .enumerate()
                    .map(|(i, &(o, h, l, c))| {
                        Bar::new(
                            chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64),
                            o,
                            h,
                            l,
                            c,
                            0.0,
                        )
                    })
                    .collect(),
            ));
            chart.set_chart_type(kind);
            let config = chart.config_mut();
            config.show_volume = false;
            config.show_price_line = false;
            config.bullish_color = up;
            config.bearish_color = down;
            // The line charts' shared source, which the Columns control must leave alone.
            config.price_source = PriceSource::OHLC4;
            chart
        };
        let edit = |ctx: &Context, chart: &mut Chart, theme: &Theme, clicks: &[&str], ok: bool| {
            let mut dialog = SettingsDialog::new();
            dialog.set_control_visible(crate::ui::Control::Templates, false);
            dialog.open(ChartSettingsState::from_chart(chart));
            for _ in 0..3 {
                frame(ctx, chart, &mut dialog, theme, vec![], &[]);
            }
            for control in clicks {
                click(ctx, chart, &mut dialog, theme, control);
            }
            click(
                ctx,
                chart,
                &mut dialog,
                theme,
                if ok { "Ok" } else { "Cancel" },
            );
            assert!(!dialog.is_open);
        };

        // Against the bar's own open the last two colours swap: 12.5 is below its open of 13 but
        // above the last close of 12, and 10 is above its open of 9 but below the last close.
        let bars = [
            (10.0, 18.0, 8.0, 14.0),
            (13.0, 17.0, 8.5, 11.0),
            (11.0, 14.0, 6.0, 12.0),
            (13.0, 15.0, 10.0, 12.5),
            (9.0, 13.0, 8.0, 10.0),
        ];
        type BarsEdit = fn(&mut BarsStyle);
        let bars_cases: [(&str, BarsEdit); 3] = [
            ("Color bars based on previous close", |s| {
                s.color_by_previous_close = true
            }),
            ("Show open", |s| s.open_visible = false),
            ("Thin bars", |s| s.thin_bars = false),
        ];
        // The first close is above the second, whose every other price is above the first's.
        let columns = [(10.0, 18.0, 8.0, 14.0), (15.0, 20.0, 9.0, 12.0)];
        let sources = [
            (PriceSource::Open, [10.0, 15.0]),
            (PriceSource::High, [18.0, 20.0]),
            (PriceSource::Low, [8.0, 9.0]),
            (PriceSource::HL2, [13.0, 14.5]),
            (PriceSource::HLC3, [40.0 / 3.0, 41.0 / 3.0]),
            (PriceSource::OHLC4, [12.5, 14.0]),
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for accept in [false, true] {
                for (label, change) in bars_cases {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut chart = chart_of(ChartType::Bars, &bars);
                    edit(&ctx, &mut chart, &theme, &[label], accept);
                    let mut expected = BarsStyle::default();
                    if accept {
                        change(&mut expected);
                    }
                    let params = chart.chart_type_params().clone();
                    assert_eq!(params.styles.bars, expected, "{label}");
                    assert!(!params.styles.candles.color_by_previous_close);
                    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    let segments: Vec<_> = painted
                        .iter()
                        .filter_map(|s| match s {
                            Shape::LineSegment { points, stroke }
                                if plot.contains(points[0])
                                    && [up, down].contains(&stroke.color) =>
                            {
                                Some((points[0].x == points[1].x, *stroke))
                            }
                            _ => None,
                        })
                        .collect();
                    let colours: Vec<_> = segments
                        .iter()
                        .filter(|(vertical, _)| *vertical)
                        .map(|(_, stroke)| stroke.color)
                        .collect();
                    assert_eq!(
                        colours,
                        if expected.color_by_previous_close {
                            [up, down, up, up, down]
                        } else {
                            [up, down, up, down, up]
                        }
                    );
                    let ticks = segments.len() - colours.len();
                    assert_eq!(ticks, if expected.open_visible { 10 } else { 5 });
                    let wick = chart.config().wick_width;
                    assert_eq!(
                        segments.iter().all(|(_, stroke)| stroke.width == wick),
                        expected.thin_bars
                    );
                    if accept {
                        let saved: ChartSnapshot = serde_json::from_str(
                            &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                        )
                        .unwrap();
                        assert!(chart.undo());
                        assert_eq!(chart.chart_type_params().styles.bars, BarsStyle::default());
                        assert!(chart.redo());
                        chart.restore(&saved).unwrap();
                        assert_eq!(chart.chart_type_params().styles.bars, expected);
                    }
                }
                for (source, prices) in sources {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut chart = chart_of(ChartType::Columns, &columns);
                    edit(&ctx, &mut chart, &theme, &["Close", source.label()], accept);
                    let mut expected = ColumnsStyle::default();
                    if accept {
                        expected.price_source = source;
                    }
                    let prices = if accept { prices } else { [14.0, 12.0] };
                    assert_eq!(chart.chart_type_params().styles.columns, expected);
                    let config = chart.config();
                    assert_eq!(config.price_source, PriceSource::OHLC4);
                    assert_eq!((config.bullish_color, config.bearish_color), (up, down));
                    // The columns of `chart` filled in one of `colours`, as (fill, top).
                    let columns_of = |chart: &mut Chart, colours: [Color32; 2]| {
                        let painted = crate::widget::test_frames::shapes(&ctx, chart, 1.0, vec![]);
                        let plot = chart.get_rendered_price_rect();
                        painted
                            .iter()
                            .filter_map(|s| match s {
                                Shape::Rect(r)
                                    if plot.contains(r.rect.center())
                                        && colours.contains(&r.fill) =>
                                {
                                    Some((r.fill, r.rect.top()))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    };
                    let drawn = columns_of(&mut chart, [COLUMN_UP, COLUMN_DOWN]);
                    let mapping = chart.get_chart_mapping();
                    assert_eq!(drawn.len(), 2, "{source:?}");
                    for ((fill, top), (colour, price)) in drawn
                        .into_iter()
                        .zip([COLUMN_UP, COLUMN_DOWN].into_iter().zip(prices))
                    {
                        assert_eq!(fill, colour);
                        assert!((top - mapping.price_to_y(price)).abs() < 0.01, "{source:?}");
                    }
                    if accept {
                        let saved: ChartSnapshot = serde_json::from_str(
                            &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                        )
                        .unwrap();
                        assert!(chart.undo());
                        assert_eq!(
                            chart.chart_type_params().styles.columns,
                            ColumnsStyle::default()
                        );
                        assert!(chart.redo());
                        chart.restore(&saved).unwrap();
                        assert_eq!(chart.chart_type_params().styles.columns, expected);

                        // The entry as saved before its colours.
                        let mut former = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
                        for params in ["/chart_type_params", "/settings/chart_type_params"] {
                            former.pointer_mut(params).unwrap()["styles"]["columns"] =
                                serde_json::json!({ "price_source": source });
                        }
                        let former: ChartSnapshot = serde_json::from_value(former).unwrap();
                        let mut restored = chart_of(ChartType::Columns, &columns);
                        restored.restore(&former).unwrap();
                        crate::widget::test_frames::frame(&ctx, &mut restored, 1.0, vec![]);
                        let fills: Vec<_> = columns_of(&mut restored, [up, down])
                            .into_iter()
                            .map(|(fill, _)| fill)
                            .collect();
                        assert_eq!(fills, [up, down], "{source:?}");
                    }
                }
            }
        }
    }

    /// Each line-based chart type keeps the price source chosen for it in the chart or series
    /// settings: switching to another of them draws that type's own source, and switching back
    /// draws the chosen one again. Settings saved while one source served them all draw it on
    /// every one of them.
    #[test]
    fn line_chart_types_keep_their_own_price_source() {
        use crate::model::PriceSource;
        use egui::epaint::ColorMode;

        // Every high is above every close, so the two sources never share a height.
        let bars: Vec<Bar> = [
            (10.0, 18.0, 8.0, 14.0),
            (13.0, 17.0, 8.5, 11.0),
            (11.0, 16.0, 6.0, 12.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c))| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64);
            Bar::new(time, o, h, l, c, 0.0)
        })
        .collect();
        let (highs, closes) = ([18.0, 17.0, 16.0], [14.0, 11.0, 12.0]);
        let theme = Theme::dark();
        type Edit = fn(&Context, &mut Chart, &Theme);
        let edits: [(&str, Edit); 2] = [
            ("chart settings", |ctx, chart, theme| {
                let mut dialog = SettingsDialog::new();
                dialog.set_control_visible(crate::ui::Control::Templates, false);
                dialog.open(ChartSettingsState::from_chart(chart));
                for _ in 0..3 {
                    frame(ctx, chart, &mut dialog, theme, vec![], &[]);
                }
                for control in ["Close", "High", "Ok"] {
                    click(ctx, chart, &mut dialog, theme, control);
                }
                assert!(!dialog.is_open);
            }),
            ("series settings", |_, chart, _| {
                let mut settings = chart.series_settings();
                settings.price_source = PriceSource::High;
                chart.apply_series_settings(&settings);
            }),
        ];
        for (via, edit) in edits {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.set_chart_type(ChartType::Line);
            let config = chart.config_mut();
            config.show_volume = false;
            config.show_price_line = false;
            edit(&ctx, &mut chart, &theme);
            for (t, (kind, source, drawn, other)) in [
                (ChartType::StepLine, PriceSource::Close, closes, highs),
                (ChartType::Line, PriceSource::High, highs, closes),
            ]
            .into_iter()
            .enumerate()
            {
                let row = format!("{via}: {kind:?}");
                chart.set_chart_type(kind);
                assert_eq!(chart.series_settings().price_source, source, "{row}");
                let painted =
                    crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0 + t as f64, vec![]);
                let plot = chart.get_rendered_price_rect();
                let styles = &chart.chart_type_params().styles;
                let color = match kind {
                    ChartType::Line => styles.line.color,
                    _ => styles.step_line.color,
                };
                let ys: Vec<f32> = painted
                    .iter()
                    .flat_map(|shape| match shape {
                        Shape::Path(path) if path.stroke.color == ColorMode::Solid(color) => {
                            path.points.clone()
                        }
                        Shape::LineSegment { points, stroke } if stroke.color == color => {
                            points.to_vec()
                        }
                        _ => Vec::new(),
                    })
                    .filter(|point| plot.x_range().contains(point.x))
                    .map(|point| point.y)
                    .collect();
                let mapping = chart.get_chart_mapping();
                let near = |price: f64| {
                    ys.iter()
                        .any(|y| (y - mapping.price_to_y(price)).abs() < 0.5)
                };
                assert!(drawn.into_iter().all(near), "{row} must draw {source:?}");
                assert!(!other.into_iter().any(near), "{row} draws another source");
            }
        }
        // Each line-based chart type keeps a source of its own. Settings saved while one source
        // served them all hold none, so they draw the shared one on every one of them.
        let kinds = [
            ChartType::Line,
            ChartType::StepLine,
            ChartType::LineWithMarkers,
            ChartType::Area,
            ChartType::Baseline,
        ];
        let sources = [
            PriceSource::Open,
            PriceSource::High,
            PriceSource::HL2,
            PriceSource::HLC3,
            PriceSource::OHLC4,
        ];
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        for (kind, source) in kinds.into_iter().zip(sources) {
            chart.set_chart_type(kind);
            let mut settings = chart.series_settings();
            settings.price_source = source;
            chart.apply_series_settings(&settings);
        }
        for (kind, source) in kinds.into_iter().zip(sources) {
            chart.set_chart_type(kind);
            assert_eq!(chart.series_settings().price_source, source, "{kind:?}");
        }
        let mut saved = Chart::new(BarData::from_bars(bars));
        saved.config_mut().price_source = PriceSource::Low;
        let mut json = serde_json::to_value(ChartSettingsState::from_chart(&saved)).unwrap();
        let styles = json["chart_type_params"]["styles"].as_object_mut().unwrap();
        for key in [
            "line_price_source",
            "step_line_price_source",
            "line_with_markers_price_source",
            "area_price_source",
            "baseline_price_source",
        ] {
            assert!(styles.remove(key).is_some(), "{key}");
        }
        let older: ChartSettingsState = serde_json::from_value(json).unwrap();
        older.apply_to_chart(&mut chart).unwrap();
        for kind in kinds {
            chart.set_chart_type(kind);
            assert_eq!(
                chart.series_settings().price_source,
                PriceSource::Low,
                "saved {kind:?}"
            );
        }
    }

    /// Pointer edits of each high-low control reach the painter only after acceptance, and
    /// Reset to defaults restores the single colour with its labels.
    #[test]
    fn high_low_dialog_edits_reach_the_painter_only_when_accepted() {
        use crate::model::{ChartTypeParams, HighLowStyle};
        use crate::widget::test_frames::shapes;
        use egui::accesskit::Role;
        use egui::{Color32, Vec2};
        use egui_kittest::{Harness, kittest::Queryable};

        let seeded = HighLowStyle {
            bar_color: Color32::from_rgb(3, 4, 102),
            labels_visible: true,
            label_color: Color32::from_rgb(5, 6, 104),
            label_font_size: 9.0,
        };
        let white = Color32::from_rgb(255, 255, 255);
        for theme in [Theme::light(), Theme::dark()] {
            for accept in [false, true] {
                for edit in ["Bar", "Label", "Price labels", "Label size", "Reset"] {
                    let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
                        chrono::DateTime::UNIX_EPOCH,
                        10.0,
                        18.0,
                        8.0,
                        14.0,
                        0.0,
                    )]));
                    chart.set_chart_type(ChartType::HighLow);
                    chart.config_mut().show_volume = false;
                    let mut params = ChartTypeParams::default();
                    params.styles.high_low = seeded;
                    chart.set_chart_type_params(params);
                    let mut dialog = SettingsDialog::new();
                    dialog.set_control_visible(crate::ui::Control::Templates, false);
                    dialog.open(ChartSettingsState::from_chart(&chart));
                    let harness_theme = theme.clone();
                    let setup_theme = theme.clone();
                    let mut harness = Harness::builder()
                        .with_context_setup(move |ctx| {
                            egui_extras::install_image_loaders(ctx);
                            crate::theme::apply_to_egui(ctx, &setup_theme);
                        })
                        .with_size(Vec2::new(1440.0, 900.0))
                        .build_ui_state(
                            move |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                                if let SettingsAction::Apply(settings) =
                                    dialog.show(ui.ctx(), &harness_theme)
                                {
                                    *accepted = Some(settings);
                                }
                            },
                            (dialog, None),
                        );
                    harness.run();
                    match edit {
                        "Bar" | "Label" => {
                            let color = if edit == "Bar" { "#030466" } else { "#050668" };
                            harness
                                .get_by_role_and_label(Role::ColorWell, color)
                                .click();
                            harness.run();
                            harness
                                .get_by_role_and_label(Role::ColorWell, "#FFFFFF")
                                .click();
                        }
                        "Price labels" => {
                            harness.get_by_role_and_label(Role::CheckBox, edit).click();
                        }
                        "Label size" => {
                            harness.get_by_role_and_label(Role::ComboBox, edit).click();
                            harness.run();
                            harness.get_by_role_and_label(Role::Button, "12").click();
                        }
                        _ => harness.get_by_label("Reset to defaults").click(),
                    }
                    harness.run();
                    harness
                        .get_by_label(if accept { "Ok" } else { "Cancel" })
                        .click();
                    harness.run();
                    let (dialog, accepted) = harness.state();
                    assert!(!dialog.is_open);
                    if let Some(settings) = accepted {
                        settings.apply_to_chart(&mut chart).unwrap();
                    }
                    assert_eq!(accepted.is_some(), accept);

                    let mut expected = seeded;
                    if accept {
                        match edit {
                            "Bar" => expected.bar_color = white,
                            "Label" => expected.label_color = white,
                            "Price labels" => expected.labels_visible = false,
                            "Label size" => expected.label_font_size = 12.0,
                            _ => expected = HighLowStyle::default(),
                        }
                    }
                    assert_eq!(chart.chart_type_params().styles.high_low, expected);
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let painted = shapes(&ctx, &mut chart, 1.0, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    assert!(painted.iter().any(|s| matches!(s, Shape::Rect(r)
                        if plot.contains(r.rect.center()) && r.fill == expected.bar_color)));
                    // Reset's default label colour prints in the price scale's text colour.
                    let label_color = if expected.label_color == Color32::PLACEHOLDER {
                        chart.config().text_color
                    } else {
                        expected.label_color
                    };
                    let label = painted.iter().any(|s| {
                        matches!(s, Shape::Text(t)
                        if t.galley.text() == "18.00"
                            && t.galley.job.sections[0].format.color == label_color
                            && t.galley.job.sections[0].format.font_id.size
                                == expected.label_font_size)
                    });
                    assert_eq!(label, expected.labels_visible);
                }
            }
        }
    }

    /// Every TPO label control changes only its own saved field, after Ok; Cancel discards it.
    #[test]
    fn tpo_label_switches_apply_on_ok_only() {
        use crate::model::ChartTypeParams;
        use egui::Vec2;
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        let statistics = serde_json::json!({"tpo_statistics": true});
        // (settings before the dialog opens, switch, its saved field)
        for (seed, label, field) in [
            (serde_json::json!({}), "POC", "/tpo_labels/poc"),
            (serde_json::json!({}), "VAH", "/tpo_labels/value_area_high"),
            (serde_json::json!({}), "VAL", "/tpo_labels/value_area_low"),
            (serde_json::json!({}), "Mid", "/tpo_labels/midpoint"),
            (
                serde_json::json!({}),
                "IBH",
                "/tpo_labels/initial_balance_high",
            ),
            (
                serde_json::json!({}),
                "IBL",
                "/tpo_labels/initial_balance_low",
            ),
            (serde_json::json!({}), "Open", "/tpo_labels/opening_price"),
            (serde_json::json!({}), "Close", "/tpo_labels/closing_price"),
            (serde_json::json!({}), "PH", "/tpo_labels/poor_high"),
            (serde_json::json!({}), "PL", "/tpo_labels/poor_low"),
            (serde_json::json!({}), "SP", "/tpo_labels/single_prints"),
            (serde_json::json!({}), "Label prices", "/tpo_labels/prices"),
            (serde_json::json!({}), "Price scale tags", "/tpo_level_tags"),
            (
                statistics.clone(),
                "Statistics: Total volume",
                "/tpo_statistics_rows/total_volume",
            ),
            (
                statistics.clone(),
                "Statistics: POC",
                "/tpo_statistics_rows/poc",
            ),
        ] {
            let mut seeded = serde_json::to_value(ChartTypeParams::default()).unwrap();
            for (name, value) in seed.as_object().unwrap() {
                seeded[name] = value.clone();
            }
            for accept in [false, true] {
                let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
                    chrono::DateTime::UNIX_EPOCH,
                    100.0,
                    102.0,
                    99.0,
                    101.0,
                    1.0,
                )]));
                chart.set_chart_type(ChartType::TimePriceOpportunity);
                chart.set_chart_type_params(serde_json::from_value(seeded.clone()).unwrap());
                let mut dialog = SettingsDialog::new();
                dialog.set_control_visible(crate::ui::Control::Templates, false);
                dialog.open(ChartSettingsState::from_chart(&chart));
                let mut harness = Harness::builder()
                    .with_context_setup(|ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &Theme::light());
                    })
                    .with_size(Vec2::new(1440.0, 900.0))
                    .build_ui_state(
                        |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                            if let SettingsAction::Apply(settings) =
                                dialog.show(ui.ctx(), &Theme::light())
                            {
                                *accepted = Some(settings);
                            }
                        },
                        (dialog, None),
                    );
                harness.run();
                harness
                    .get_by_role_and_label(Role::CheckBox, label)
                    .scroll_to_me();
                harness.run();
                harness.get_by_role_and_label(Role::CheckBox, label).click();
                harness.run();
                harness
                    .get_by_label(if accept { "Ok" } else { "Cancel" })
                    .click();
                harness.run();
                let (dialog, accepted) = harness.state();
                assert!(!dialog.is_open);
                assert_eq!(accepted.is_some(), accept);
                if let Some(settings) = accepted {
                    settings.apply_to_chart(&mut chart).unwrap();
                }
                let mut expected = seeded.clone();
                if accept {
                    let value = expected.pointer_mut(field).unwrap();
                    *value = serde_json::Value::Bool(!value.as_bool().unwrap());
                }
                assert_eq!(
                    chart.chart_type_params(),
                    &serde_json::from_value::<ChartTypeParams>(expected).unwrap(),
                    "{label}, accept={accept}"
                );
            }
        }
    }

    /// Renko bricks and Line Break lines without border colours of their own are outlined in the
    /// rising and falling body colours at full opacity, and the Border row starts from them, so a
    /// palette colour picked after ticking Custom up is stored and painted opaque. Flat bars
    /// closing at 100, 102 and 99 build Renko bricks of 1 from 100 up to 102 and from 101 down
    /// to 99, two rising and two falling, and the Line Break lines 100 to 100, 100 to 102 and
    /// 100 to 99, two rising and one falling. Expected: the opaque default body colours #089981
    /// and #F23645, then opaque black on the rising elements.
    #[test]
    fn brick_borders_are_opaque_by_default_and_as_picked() {
        use egui::Color32;
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        for (kind, rising, falling) in [(ChartType::Renko, 2, 2), (ChartType::LineBreak, 2, 1)] {
            let bars = [100.0, 102.0, 99.0]
                .into_iter()
                .enumerate()
                .map(|(i, close)| {
                    let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i as i64);
                    Bar::new(time, close, close, close, close, 0.0)
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars));
            chart.set_chart_type(kind);
            let ctx = Context::default();
            // The stroke of the outline drawn at the centre of each element's slot.
            let borders = |chart: &mut Chart, time: f64| {
                crate::widget::test_frames::frame(&ctx, chart, time, vec![]);
                crate::widget::test_frames::frame(&ctx, chart, time + 0.1, vec![]);
                let painted = crate::widget::test_frames::shapes(&ctx, chart, time + 0.2, vec![]);
                let map = chart.get_chart_mapping();
                (0..rising + falling)
                    .map(|i| {
                        let x = map.idx_to_x(i);
                        painted.iter().find_map(|shape| match shape {
                            Shape::Rect(r)
                                if r.stroke.width > 0.0 && (r.rect.center().x - x).abs() < 0.5 =>
                            {
                                Some(r.stroke.color)
                            }
                            _ => None,
                        })
                    })
                    .collect::<Vec<_>>()
            };
            let (up, down) = (chart.config().bullish_color, chart.config().bearish_color);
            let expected = |top| [vec![Some(top); rising], vec![Some(down); falling]].concat();
            assert_eq!(borders(&mut chart, 0.0), expected(up), "{kind:?} unset");

            let mut dialog = SettingsDialog::new();
            dialog.set_control_visible(crate::ui::Control::Templates, false);
            dialog.open(ChartSettingsState::from_chart(&chart));
            let theme = Theme::light();
            let setup_theme = theme.clone();
            let mut harness = Harness::builder()
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &setup_theme);
                })
                .with_size(Vec2::new(1440.0, 900.0))
                .build_ui_state(
                    move |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                        if let SettingsAction::Apply(settings) = dialog.show(ui.ctx(), &theme) {
                            *accepted = Some(settings);
                        }
                    },
                    (dialog, None),
                );
            harness.run();
            // The Border row comes before Renko's Wick row.
            harness
                .get_all_by_role_and_label(Role::CheckBox, "Custom up")
                .next()
                .unwrap()
                .click();
            harness.run();
            // The rising Body well comes first, then the Border well that Custom up enabled.
            let [r, g, b, _] = up.to_srgba_unmultiplied();
            harness
                .get_all_by_role_and_label(Role::ColorWell, &format!("#{r:02X}{g:02X}{b:02X}"))
                .nth(1)
                .expect("the rising Border well shows the rising body colour")
                .click();
            harness.run();
            harness
                .get_by_role_and_label(Role::ColorWell, "#000000")
                .click();
            harness.run();
            harness.get_by_label("Ok").click();
            harness.run();
            let (_, accepted) = harness.state();
            let accepted: &ChartSettingsState = accepted.as_ref().expect("accepted");
            accepted.apply_to_chart(&mut chart).unwrap();
            assert_eq!(
                borders(&mut chart, 1.0),
                expected(Color32::BLACK),
                "{kind:?} black picked"
            );
        }
    }

    /// Choosing a palette colour for an HLC area fill, a column colour or a Baseline fill at the
    /// base keeps its opacity, which has no control of its own, and leaves the other colours
    /// alone. Expected: white at the seeded alpha.
    #[test]
    fn palette_colour_keeps_the_fill_and_column_opacity() {
        use crate::model::{ChartTypeParams, ChartTypeStyles, HlcAreaStyle, LinePlotStyle};
        use egui::accesskit::Role;
        use egui::{Color32, Vec2};
        use egui_kittest::{Harness, kittest::Queryable};

        // Chart type, seeded colours, the well to click by its colour, the colour it edits and
        // that colour afterwards.
        type Row = (
            ChartType,
            fn(&mut ChartTypeStyles),
            &'static str,
            fn(&ChartTypeStyles) -> Color32,
            [u8; 4],
        );
        let rows: [Row; 3] = [
            (
                ChartType::HlcArea,
                |styles| {
                    let line = |r| LinePlotStyle {
                        color: Color32::from_rgb(r, 2, 3),
                        ..Default::default()
                    };
                    styles.hlc_area = HlcAreaStyle {
                        high_line: line(101),
                        low_line: line(102),
                        close_line: line(103),
                        high_to_close_fill: Color32::from_rgba_unmultiplied(0, 50, 200, 51),
                        close_to_low_fill: Color32::from_rgba_unmultiplied(200, 50, 0, 51),
                        ..Default::default()
                    };
                },
                "#0032C8",
                |styles| styles.hlc_area.high_to_close_fill,
                [255, 255, 255, 51],
            ),
            (
                ChartType::Columns,
                |styles| {
                    styles.columns.up_color = Color32::from_rgba_unmultiplied(0, 50, 100, 128);
                    styles.columns.down_color = Color32::from_rgba_unmultiplied(100, 50, 0, 128);
                },
                "#003264",
                |styles| styles.columns.up_color,
                [255, 255, 255, 128],
            ),
            (
                ChartType::Baseline,
                |styles| {
                    let style = &mut styles.baseline;
                    style.above_fill = Color32::from_rgba_unmultiplied(200, 50, 0, 51);
                    style.above_fill_at_base =
                        Some(Color32::from_rgba_unmultiplied(0, 50, 200, 51));
                    style.below_fill = Color32::from_rgba_unmultiplied(50, 200, 0, 51);
                    style.below_fill_at_base =
                        Some(Color32::from_rgba_unmultiplied(0, 200, 50, 51));
                },
                "#0032C8",
                // What the fill draws at the base.
                |styles| {
                    let style = &styles.baseline;
                    style.above_fill_at_base.unwrap_or(style.above_fill)
                },
                [255, 255, 255, 51],
            ),
        ];
        for (chart_type, seed, well, read, expected) in rows {
            let mut params = ChartTypeParams::default();
            seed(&mut params.styles);
            let mut dialog = SettingsDialog::new();
            dialog.set_control_visible(crate::ui::Control::Templates, false);
            dialog.open(ChartSettingsState {
                chart_type,
                chart_type_params: Some(params),
                ..Default::default()
            });
            let theme = Theme::light();
            let setup_theme = theme.clone();
            let mut harness = Harness::builder()
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &setup_theme);
                })
                .with_size(Vec2::new(1440.0, 900.0))
                .build_ui_state(
                    move |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                        if let SettingsAction::Apply(settings) = dialog.show(ui.ctx(), &theme) {
                            *accepted = Some(settings);
                        }
                    },
                    (dialog, None),
                );
            harness.run();
            harness.get_by_role_and_label(Role::ColorWell, well).click();
            harness.run();
            harness
                .get_by_role_and_label(Role::ColorWell, "#FFFFFF")
                .click();
            harness.run();
            harness.get_by_label("Ok").click();
            harness.run();
            let (_, accepted) = harness.state();
            let accepted: &ChartSettingsState = accepted.as_ref().expect("accepted");
            let styles = &accepted.chart_type_params.as_ref().unwrap().styles;
            assert_eq!(
                read(styles).to_srgba_unmultiplied(),
                expected,
                "{chart_type:?}"
            );
        }
    }

    /// Every footprint edit is staged until Ok; Cancel leaves the opening settings intact.
    #[test]
    fn footprint_symbol_settings_apply_only_when_accepted() {
        use crate::model::{ChartTypeParams, FootprintColor, FootprintIntensity};
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        let seeded = ChartTypeParams {
            footprint_intensity: FootprintIntensity::Volume(500.0),
            footprint_naked_poc: true,
            footprint_extend_bars: Some(5),
            ..Default::default()
        };
        for accept in [false, true] {
            for edit in [
                "Shading scale",
                "Full shade volume",
                "Automatic row height",
                "Smallest row with numbers",
                "Expiry value",
                "Expiry off",
                "Keep host values",
                "Fixed volume default",
                "Expiry on",
                "Zones end only when broken",
                "Larger side",
                "Side colours",
                "Shade wells",
            ] {
                let mut seeded = seeded.clone();
                if edit == "Shade wells" {
                    seeded.footprint_step_colors[1][0] = Some(Color32::from_rgb(4, 5, 6));
                }
                if edit == "Fixed volume default" {
                    seeded.footprint_intensity = FootprintIntensity::Bar;
                }
                if edit == "Expiry on" {
                    seeded.footprint_extend_bars = None;
                }
                if edit == "Zones end only when broken" {
                    seeded.footprint_stacked_imbalances = true;
                }
                if edit == "Keep host values" {
                    seeded.footprint_intensity = FootprintIntensity::Volume(-1.0);
                    seeded.footprint_auto_row_height = 0.0;
                    seeded.footprint_min_text_height = 100.0;
                    seeded.footprint_extend_bars = Some(0);
                }
                let mut dialog = SettingsDialog::new();
                dialog.set_control_visible(crate::ui::Control::Templates, false);
                dialog.open(ChartSettingsState {
                    chart_type: ChartType::VolumeFootprint,
                    chart_type_params: Some(seeded.clone()),
                    ..Default::default()
                });
                let theme = Theme::light();
                let setup_theme = theme.clone();
                let mut harness = Harness::builder()
                    .with_context_setup(move |ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &setup_theme);
                    })
                    .with_size(Vec2::new(1440.0, 900.0))
                    .build_ui_state(
                        move |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                            if let SettingsAction::Apply(settings) = dialog.show(ui.ctx(), &theme) {
                                *accepted = Some(settings);
                            }
                        },
                        (dialog, None),
                    );
                harness.run();
                let mut expected = seeded.clone();
                match edit {
                    "Larger side" | "Side colours" => {
                        harness
                            .get_by_role_and_label(Role::ComboBox, "Colouring")
                            .scroll_to_me();
                        harness.run();
                        harness
                            .get_by_role_and_label(Role::ComboBox, "Colouring")
                            .click();
                        harness.run();
                        harness.get_by_role_and_label(Role::Button, edit).click();
                        harness.run();
                        expected.footprint_color_by = if edit == "Larger side" {
                            FootprintColor::LargerSide
                        } else {
                            FootprintColor::Side
                        };
                        assert!(
                            harness
                                .query_by_role_and_label(Role::ColorWell, "#045144")
                                .is_none()
                        );
                    }
                    "Shade wells" => {
                        harness.get_by_role_and_label(Role::ColorWell, "#040506");
                        harness.get_by_role_and_label(Role::ColorWell, "#045144");
                    }
                    "Shading scale" | "Fixed volume default" => {
                        let label = "Shading scale";
                        harness
                            .get_by_role_and_label(Role::ComboBox, label)
                            .scroll_to_me();
                        harness.run();
                        harness.get_by_role_and_label(Role::ComboBox, label).click();
                        harness.run();
                        let fixed = edit == "Fixed volume default";
                        harness
                            .get_by_role_and_label(
                                Role::Button,
                                if fixed {
                                    "Fixed volume"
                                } else {
                                    "Bars in view"
                                },
                            )
                            .click();
                        expected.footprint_intensity = if fixed {
                            FootprintIntensity::Volume(100.0)
                        } else {
                            FootprintIntensity::Visible
                        };
                    }
                    "Automatic row height" | "Smallest row with numbers" => {
                        let value = if edit == "Automatic row height" {
                            24.0
                        } else {
                            10.0
                        };
                        harness
                            .get_by_role_and_label(Role::SpinButton, edit)
                            .set_value_accesskit(value);
                        if edit == "Automatic row height" {
                            expected.footprint_auto_row_height = value as f32;
                        } else {
                            expected.footprint_min_text_height = value as f32;
                        }
                    }
                    "Keep host values" => {
                        harness.get_by_label("Annotations").scroll_to_me();
                        harness.run();
                        harness.get_by_label("Annotations").click();
                    }
                    "Zones end only when broken" => {
                        harness.get_by_label("Annotations").scroll_to_me();
                        harness.run();
                        harness.get_by_label("Annotations").click();
                        harness.run();
                        harness
                            .get_by_role_and_label(Role::CheckBox, edit)
                            .scroll_to_me();
                        harness.run();
                        harness.get_by_role_and_label(Role::CheckBox, edit).click();
                        expected.footprint_zones_until_broken = true;
                    }
                    "Expiry value" | "Expiry off" | "Expiry on" => {
                        harness.get_by_label("Annotations").scroll_to_me();
                        harness.run();
                        harness.get_by_label("Annotations").click();
                        harness.run();
                        if edit == "Expiry value" {
                            harness
                                .get_by_role_and_label(Role::SpinButton, "Expire after bars")
                                .set_value_accesskit(8.0);
                            expected.footprint_extend_bars = Some(8);
                        } else {
                            harness
                                .get_by_role_and_label(Role::CheckBox, "Expire after bars")
                                .scroll_to_me();
                            harness.run();
                            harness
                                .get_by_role_and_label(Role::CheckBox, "Expire after bars")
                                .click();
                            expected.footprint_extend_bars =
                                if edit == "Expiry on" { Some(50) } else { None };
                        }
                    }
                    _ => {
                        harness
                            .get_by_role_and_label(Role::SpinButton, edit)
                            .set_value_accesskit(2000.0);
                        expected.footprint_intensity = FootprintIntensity::Volume(2000.0);
                    }
                }
                harness.run();
                harness
                    .get_by_label(if accept { "Ok" } else { "Cancel" })
                    .click();
                harness.run();
                let (_, accepted) = harness.state();
                assert_eq!(
                    accepted.as_ref().and_then(|s| s.chart_type_params.as_ref()),
                    accept.then_some(&expected),
                    "{edit}, accept {accept}"
                );
            }
        }
    }
}
