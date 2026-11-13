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
                        crate::model::ChartType::HighLow
                            | crate::model::ChartType::HlcArea
                            | crate::model::ChartType::TimePriceOpportunity
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

    /// Bars colour by their own open or the previous close, and columns reach the selected price
    /// while their colour still compares closes, through the dialog and the painter after
    /// acceptance only, in both themes. Undo and saved charts keep these values apart from the
    /// candle and line settings.
    #[test]
    fn bars_and_columns_dialog_edits_reach_the_painter_and_keep_independent_saved_values() {
        use crate::model::{BarsStyle, ColumnsStyle, PriceSource};
        use crate::widget::ChartSnapshot;
        use egui::Color32;

        let (up, down) = (Color32::from_rgb(1, 100, 2), Color32::from_rgb(101, 2, 3));
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
            // Line charts keep their own source, which the Columns control must leave alone.
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
                    let expected = ColumnsStyle {
                        price_source: if accept { source } else { PriceSource::Close },
                    };
                    let prices = if accept { prices } else { [14.0, 12.0] };
                    assert_eq!(chart.chart_type_params().styles.columns, expected);
                    assert_eq!(chart.config().price_source, PriceSource::OHLC4);
                    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    let mapping = chart.get_chart_mapping();
                    let drawn: Vec<_> = painted
                        .iter()
                        .filter_map(|s| match s {
                            Shape::Rect(r)
                                if plot.contains(r.rect.center())
                                    && [up, down].contains(&r.fill) =>
                            {
                                Some((r.fill, r.rect.top()))
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(drawn.len(), 2, "{source:?}");
                    for ((fill, top), (colour, price)) in
                        drawn.into_iter().zip([up, down].into_iter().zip(prices))
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
                    }
                }
            }
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

    /// Choosing a palette colour for an HLC area fill keeps the fill's opacity, which has no
    /// control of its own. Expected: white at the seeded alpha of 51.
    #[test]
    fn palette_colour_keeps_the_hlc_fill_opacity() {
        use crate::model::{ChartTypeParams, HlcAreaStyle, LinePlotStyle};
        use egui::accesskit::Role;
        use egui::{Color32, Vec2};
        use egui_kittest::{Harness, kittest::Queryable};

        let line = |r| LinePlotStyle {
            color: Color32::from_rgb(r, 2, 3),
            ..Default::default()
        };
        let fill = Color32::from_rgba_unmultiplied(0, 50, 200, 51);
        let mut params = ChartTypeParams::default();
        params.styles.hlc_area = HlcAreaStyle {
            high_line: line(101),
            low_line: line(102),
            close_line: line(103),
            high_to_close_fill: fill,
            close_to_low_fill: Color32::from_rgba_unmultiplied(200, 50, 0, 51),
            ..Default::default()
        };
        let mut dialog = SettingsDialog::new();
        dialog.set_control_visible(crate::ui::Control::Templates, false);
        dialog.open(ChartSettingsState {
            chart_type: ChartType::HlcArea,
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
        harness
            .get_by_role_and_label(Role::ColorWell, "#0032C8")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(Role::ColorWell, "#FFFFFF")
            .click();
        harness.run();
        harness.get_by_label("Ok").click();
        harness.run();
        let (_, accepted) = harness.state();
        let accepted: &ChartSettingsState = accepted.as_ref().expect("accepted");
        let picked = accepted
            .chart_type_params
            .as_ref()
            .unwrap()
            .styles
            .hlc_area
            .high_to_close_fill;
        assert_eq!(picked.to_srgba_unmultiplied(), [255, 255, 255, 51]);
    }
}
