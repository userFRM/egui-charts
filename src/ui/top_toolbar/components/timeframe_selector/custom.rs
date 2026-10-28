//! Custom interval dialog of the timeframe selector.

use super::{TimeframeUnit, interval};
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::Button;
use egui::{RichText, Ui, vec2};

/// Custom timeframe dialog state
#[derive(Debug, Clone)]
pub struct CustomTimeframeDialog {
    /// The numeric value input
    pub value_input: String,
    /// The selected unit
    pub unit: TimeframeUnit,
    /// Validation error message
    pub error: Option<String>,
}

impl Default for CustomTimeframeDialog {
    fn default() -> Self {
        Self {
            value_input: "1".to_string(),
            unit: TimeframeUnit::Minute,
            error: None,
        }
    }
}

impl CustomTimeframeDialog {
    /// Validate and return the custom Timeframe, or None if invalid
    pub fn validate(&mut self) -> Option<Timeframe> {
        let trimmed = self.value_input.trim();
        if trimmed.is_empty() {
            self.error = Some("Enter a number".to_string());
            return None;
        }
        match trimmed.parse::<u32>() {
            Ok(0) => {
                self.error = Some("Value must be > 0".to_string());
                None
            }
            Ok(v) if v > 10_000 => {
                self.error = Some("Value too large (max 10000)".to_string());
                None
            }
            Ok(v) if self.unit == TimeframeUnit::Millisecond && ![100, 250, 500].contains(&v) => {
                self.error = Some("Use 100, 250 or 500 milliseconds".to_owned());
                None
            }
            Ok(v) => {
                self.error = None;
                Some(interval(v, self.unit))
            }
            Err(_) => {
                self.error = Some("Enter a valid number".to_string());
                None
            }
        }
    }

    /// Available units for the custom dialog
    fn available_units() -> &'static [(TimeframeUnit, &'static str)] {
        &[
            (TimeframeUnit::Tick, "Ticks"),
            (TimeframeUnit::Volume, "Volume"),
            (TimeframeUnit::Range, "Range"),
            (TimeframeUnit::Delta, "Delta"),
            (TimeframeUnit::Second, "Seconds"),
            (TimeframeUnit::Minute, "Minutes"),
            (TimeframeUnit::Hour, "Hours"),
            (TimeframeUnit::Day, "Days"),
            (TimeframeUnit::Week, "Weeks"),
            (TimeframeUnit::Month, "Months"),
        ]
    }

    /// Show the custom timeframe dialog inside the dropdown.
    /// Returns Some(Timeframe) if the user confirms, None otherwise.
    pub fn show(&mut self, ui: &mut Ui) -> Option<Timeframe> {
        let theme = ui.active_theme();
        let mut result = None;

        ui.add_space(theme.spacing.lg);
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), DESIGN_TOKENS.sizing.button_md),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(theme.spacing.lg);
                // The controls share the height of the button.
                ui.spacing_mut().interact_size.y = DESIGN_TOKENS.sizing.button_md;
                // The menu style draws inactive controls without a frame; the form's fields need one.
                let inactive = &mut ui.visuals_mut().widgets.inactive;
                inactive.bg_stroke = egui::Stroke::new(1.0, theme.ui.border_control);
                inactive.weak_bg_fill = theme.ui.btn_bg_hover;
                let width_id = ui.scope_id().with("interval_actions_width");
                let actions_width = ui.data(|d| d.get_temp::<f32>(width_id)).unwrap_or(0.0);
                let field_width =
                    (ui.available_width() - actions_width - 2.0 * theme.spacing.lg).max(0.0);
                ui.add_sized(
                    vec2(field_width, DESIGN_TOKENS.sizing.button_md),
                    egui::TextEdit::singleline(&mut self.value_input)
                        .desired_width(0.0)
                        .align(egui::Align2::LEFT_CENTER)
                        .font(egui::FontId::proportional(theme.text.body)),
                )
                .accessible_name(l10n::text(ui.ctx(), "Interval value"));
                let width = ui
                    .horizontal(|ui| {
                        let unit = Self::available_units()
                            .iter()
                            .find(|(u, _)| *u == self.unit)
                            .map_or_else(|| self.unit.name(), |(_, label)| label);
                        // A submenu keeps the interval menu open when a unit is picked.
                        ui.menu_button(l10n::text(ui.ctx(), unit), |ui| {
                            for (u, label) in Self::available_units() {
                                if ui
                                    .selectable_label(self.unit == *u, l10n::text(ui.ctx(), *label))
                                    .clicked()
                                {
                                    self.unit = *u;
                                }
                            }
                        });
                        if Button::new("Add").primary().show(ui).clicked() {
                            result = self.validate();
                        }
                    })
                    .response
                    .rect
                    .width();
                if (width - actions_width).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(width_id, width));
                    ui.ctx().request_discard("Interval actions changed width");
                }
            },
        );

        if let Some(err) = &self.error {
            ui.horizontal(|ui| {
                ui.add_space(theme.spacing.lg);
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), err))
                        .size(theme.text.small)
                        .color(ui.visuals().error_fg_color),
                );
            });
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::top_toolbar::components::timeframe_selector::parts;
    use egui::Rect;

    #[test]
    fn custom_interval_tabs_from_value_to_unit_to_add() {
        for unit in [TimeframeUnit::Week, TimeframeUnit::Month] {
            assert!(
                CustomTimeframeDialog::available_units()
                    .iter()
                    .any(|(u, _)| *u == unit)
            );
            let mut dialog = CustomTimeframeDialog {
                value_input: "3".into(),
                unit,
                error: None,
            };
            assert_eq!(parts(dialog.validate().unwrap()), (3, unit));
            dialog.value_input = "0".into();
            assert!(dialog.validate().is_none());
        }
        for width in [320.0, 500.0] {
            let ctx = egui::Context::default();
            let mut dialog = CustomTimeframeDialog::default();
            let mut frame = |events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        dialog.show(ui);
                    },
                )
                .drop_without_applying_deltas();
                ctx.memory(|m| m.focused())
                    .and_then(|id| ctx.read_response(id))
                    .map(|r| r.rect)
            };
            frame(vec![]);
            let mut right = 0.0;
            for _ in 0..3 {
                let rect = frame(vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }])
                .unwrap();
                assert!(rect.left() >= right, "{rect:?} follows {right}");
                assert!(rect.right() <= width, "{rect:?}");
                right = rect.right();
            }
        }
    }
}
