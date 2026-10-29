//! Events, trading and alerts tabs of the chart settings dialog.

use super::super::SettingsDialog;
use super::{
    TRADING_INDENT, TRADING_LABEL_WIDTH, styled_checkbox, styled_dropdown, trading_option,
};
use crate::chart::PnlUnit;
use crate::localization as l10n;
use crate::theme::{Theme, ThemeExt};
use crate::ui_kit::ColorPicker;
use crate::ui_kit::ext::UiExt;
use crate::widget::settings::TradingNotifications;
use egui::{Color32, RichText, Ui};

impl SettingsDialog {
    pub(in super::super) fn draw_events_tab(&mut self, ui: &mut Ui, theme: &Theme) {
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

    pub(in super::super) fn draw_trading_tab(&mut self, ui: &mut Ui, theme: &Theme) {
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
        sound_controls(
            ui,
            "Play sound for executions",
            &mut trading.play_execution_sound,
            &mut trading.execution_sound,
            &mut trading.execution_sound_volume,
            &self.execution_sounds,
            &mut self.sound_preview,
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
        styled_checkbox(ui, &mut trading.extend_lines, "Extend lines left", text);
        ui.horizontal(|ui| {
            ui.set_min_height(42.0);
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Label alignment"))
                    .size(ui.active_theme().text.title)
                    .color(text),
            );
            ui.add_space((TRADING_LABEL_WIDTH - ui.min_rect().width()).max(0.0));
            let alignment = &mut trading.label_alignment;
            use crate::chart::TradingLabelAlignment::{Center, Left, Right};
            let label = match alignment {
                Left => "Left",
                Center => "Center",
                Right => "Right",
            };
            styled_dropdown(ui, "trading_label_alignment", 140.0, text, label, |ui| {
                for (option, label) in [(Left, "Left"), (Center, "Center"), (Right, "Right")] {
                    ui.selectable_value(alignment, option, l10n::text(ui.ctx(), label));
                }
            });
        });
        ui.add_space(ui.active_theme().spacing.sm);
        styled_checkbox(
            ui,
            &mut trading.include_in_image,
            "Include trading overlays in images",
            text,
        );
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

    pub(in super::super) fn draw_alerts_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                self.draw_section_header(ui, "ALERT SETTINGS", theme);
                let alerts = &mut self.settings.alerts;
                sound_controls(
                    ui,
                    "Play sound for alerts",
                    &mut alerts.play_sound,
                    &mut alerts.sound,
                    &mut alerts.sound_volume,
                    &self.execution_sounds,
                    &mut self.sound_preview,
                );

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
}

fn sound_controls(
    ui: &mut Ui,
    label: &str,
    enabled: &mut bool,
    sound: &mut String,
    volume: &mut f32,
    sounds: &[String],
    preview: &mut Option<(String, f32)>,
) {
    styled_checkbox(ui, enabled, label, ui.active_theme().ui.text);
    let Some(first) = sounds.first() else { return };
    if sound.is_empty() {
        sound.clone_from(first);
    }
    ui.add_enabled_ui(*enabled, |ui| {
        ui.horizontal(|ui| {
            ui.combo_select(label, sound, sounds.iter().cloned(), Clone::clone);
            ui.visuals_mut().widgets.inactive.bg_fill = ui.active_theme().ui.border_control;
            ui.add(egui::Slider::new(volume, 0.0..=1.0).text(l10n::text(ui.ctx(), "Volume")));
        });
        if ui.button(l10n::text(ui.ctx(), "Test sound")).clicked() {
            *preview = Some((sound.clone(), *volume));
        }
        if cfg!(target_arch = "wasm32") {
            ui.label(l10n::text(
                ui.ctx(),
                "Click Test sound to enable browser audio.",
            ));
        }
    });
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
