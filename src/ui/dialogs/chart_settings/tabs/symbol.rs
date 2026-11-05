//! Symbol and status line tabs of the chart settings dialog.

use super::super::SettingsDialog;
use super::{checkbox, styled_checkbox, styled_dropdown};
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
                    || matches!(
                        kind,
                        crate::model::ChartType::TimePriceOpportunity
                            | crate::model::ChartType::VolumeFootprint
                            | crate::model::ChartType::SessionVolumeProfile
                    )
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
                    let heikin = kind == crate::model::ChartType::Heikin;
                    let hollow = kind == crate::model::ChartType::HollowCandles;
                    let candles = kind == crate::model::ChartType::Candles || heikin || hollow;
                    self.draw_section_header(
                        ui,
                        if heikin {
                            "HEIKIN ASHI"
                        } else if hollow {
                            "HOLLOW CANDLES"
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
                    if candles && !hollow {
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

    /// Real pointer edits reach the painter only after acceptance; Cancel keeps the original
    /// component and scale settings in both themes.
    #[test]
    fn heikin_dialog_accepts_and_cancels_component_and_scale_edits() {
        for theme in [Theme::light(), Theme::dark()] {
            for label in [
                "Body",
                "Borders",
                "Wick",
                "Use original prices on scale",
                "Reset to defaults",
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
                    chart.set_chart_type(ChartType::Heikin);
                    chart.config_mut().show_volume = false;
                    let mut params = chart.chart_type_params().clone();
                    // A border in the colour of an opaque body paints nothing.
                    params.styles.heikin.border_up = egui::Color32::from_rgb(3, 4, 102);
                    if label == "Reset to defaults" {
                        params.styles.heikin.body_visible = false;
                        params.styles.heikin.border_visible = false;
                        params.styles.heikin.wick_visible = false;
                        params.styles.heikin.real_price_only = true;
                    }
                    chart.set_chart_type_params(params);
                    let before = ChartSettingsState::from_chart(&chart);
                    let mut dialog = SettingsDialog::new();
                    dialog.set_control_visible(crate::ui::Control::Templates, false);
                    dialog.open(before.clone());
                    for _ in 0..3 {
                        frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[]);
                    }
                    for control in [label, if accept { "Ok" } else { "Cancel" }] {
                        let pos =
                            frame(&ctx, &mut chart, &mut dialog, &theme, vec![], &[control])[0];
                        frame(
                            &ctx,
                            &mut chart,
                            &mut dialog,
                            &theme,
                            vec![Event::PointerMoved(pos)],
                            &[],
                        );
                        for pressed in [true, false] {
                            frame(
                                &ctx,
                                &mut chart,
                                &mut dialog,
                                &theme,
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
                    assert!(!dialog.is_open);
                    let after = ChartSettingsState::from_chart(&chart);
                    let style = chart.chart_type_params().styles.heikin.clone();
                    let mut expected = before
                        .chart_type_params
                        .as_ref()
                        .unwrap()
                        .styles
                        .heikin
                        .clone();
                    if accept {
                        match label {
                            "Body" => expected.body_visible = false,
                            "Borders" => expected.border_visible = false,
                            "Wick" => expected.wick_visible = false,
                            "Use original prices on scale" => expected.real_price_only = true,
                            "Reset to defaults" => expected = Default::default(),
                            _ => unreachable!(),
                        }
                    }
                    assert_eq!(style, expected);
                    if !accept {
                        // Theme colours only apply once a frame has run, so compare the
                        // candle settings rather than the whole state.
                        assert_eq!(after.candle_colors, before.candle_colors);
                    }
                    // Paint the accepted state after the modal has closed.
                    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    let body = painted.iter().any(|s| {
                        matches!(s, Shape::Rect(r)
                        if plot.contains(r.rect.center()) && r.fill == style.body_up)
                    });
                    assert_eq!(body, style.body_visible);
                    let border = painted.iter().any(|s| {
                        matches!(s, Shape::Rect(r)
                        if plot.contains(r.rect.center()) && r.stroke.color == style.border_up)
                    });
                    let wick = painted.iter().any(|s| matches!(s, Shape::LineSegment { points, stroke }
                        if plot.contains(points[0]) && points[0].x == points[1].x && stroke.color == style.wick_up));
                    // Only the defaults restored by the reset share the body's colour.
                    assert_eq!(
                        border,
                        style.border_visible && style.border_up != style.body_up
                    );
                    assert_eq!(wick, style.wick_visible);
                }
            }
        }
    }
}
