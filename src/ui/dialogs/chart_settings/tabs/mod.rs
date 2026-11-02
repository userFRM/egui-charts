//! Tab rendering implementations for SettingsDialog
//!
//! This module contains all the tab-specific UI rendering code,
//! keeping the main dialog module focused on core structure.

mod canvas;
mod order_flow;
mod scales;
mod symbol;
mod trading;

use super::SettingsDialog;
use crate::localization as l10n;
use crate::theme::Theme;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Response, RichText, Ui};

impl SettingsDialog {
    pub(super) fn draw_section_header(&self, ui: &mut Ui, title: &str, theme: &Theme) {
        ui.label(
            RichText::new(l10n::text(ui.ctx(), title))
                .size(theme.text.small)
                .strong()
                .color(theme.ui.text_muted),
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
                ui.spacing_mut().interact_size.y =
                    DESIGN_TOKENS.sizing.settings_dialog.button_height;
                ui.label_inputs_by(label.inner.id, |ui| add_content(ui, self));
            });
        });
        ui.add_space(ui.active_theme().spacing.sm);
    }
}

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
        checkbox(
            ui,
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

fn styled_checkbox(ui: &mut Ui, checked: &mut bool, label: &str, text_color: Color32) -> bool {
    let changed = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 42.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(42.0);
                checkbox(
                    ui,
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

/// Keep egui's input and accessibility behaviour while painting the settings checkbox.
pub(crate) fn checkbox(ui: &mut Ui, checked: &mut bool, label: RichText) -> Response {
    let theme = ui.active_theme();
    let size = DESIGN_TOKENS.sizing.settings_dialog.checkbox_size;
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().icon_width = size;
            let widgets = &mut ui.visuals_mut().widgets;
            for visuals in [
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
            ] {
                visuals.bg_fill = Color32::TRANSPARENT;
                visuals.bg_stroke = egui::Stroke::NONE;
                visuals.fg_stroke.color = Color32::TRANSPARENT;
            }
            ui.checkbox(checked, label)
        })
        .inner;
    let rect = egui::Rect::from_min_size(
        egui::pos2(response.rect.left(), response.rect.center().y - size / 2.0),
        egui::Vec2::splat(size),
    );
    let fill = if *checked {
        theme.ui.text
    } else {
        theme.ui.panel_bg
    };
    ui.painter().rect(
        rect,
        theme.radius.sm,
        fill,
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, theme.ui.text),
        egui::StrokeKind::Inside,
    );
    if *checked {
        let tick = rect.shrink(theme.spacing.sm);
        ui.painter().add(egui::Shape::line(
            vec![tick.left_center(), tick.center_bottom(), tick.right_top()],
            egui::Stroke::new(DESIGN_TOKENS.stroke.medium, theme.ui.panel_bg),
        ));
    }
    crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.sm);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_checkboxes_fill_immediately_and_keep_native_input() {
        for (_, theme) in crate::theme::presets() {
            for (enabled, keyboard) in [(true, false), (true, true), (false, false)] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let native_icon_width = ctx.global_style().spacing.icon_width;
                let mut checked = false;
                let mut response = None;
                for step in 0..5 {
                    let events = match (step, keyboard) {
                        (2, true) => vec![egui::Event::Key {
                            key: egui::Key::Space,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        (2 | 3, false) => {
                            let pos = response
                                .as_ref()
                                .map(|r: &Response| r.rect.center())
                                .unwrap();
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton {
                                    pos,
                                    pressed: step == 2,
                                    button: egui::PointerButton::Primary,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ]
                        }
                        _ => vec![],
                    };
                    let output = ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            response = Some(
                                ui.add_enabled_ui(enabled, |ui| {
                                    checkbox(
                                        ui,
                                        &mut checked,
                                        RichText::new("Visible").color(theme.ui.text),
                                    )
                                })
                                .inner,
                            );
                            // A settings checkbox must not change the following native control.
                            ui.checkbox(&mut false, "Native");
                        },
                    );
                    let boxes: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Rect(rect)
                                if rect.rect.size() == egui::Vec2::splat(18.0)
                                    && rect.stroke.color == theme.ui.text =>
                            {
                                Some((rect.stroke, rect.fill))
                            }
                            _ => None,
                        })
                        .collect();
                    let ticks = output.shapes.iter().filter(|shape| matches!(&shape.shape,
                        egui::Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(theme.ui.panel_bg)
                            && path.points.len() == 3)).count();
                    let native_shown = output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Rect(rect) if rect.rect.size() == egui::Vec2::splat(native_icon_width)
                            && rect.stroke.color == theme.ui.border_control));
                    output.drop_without_applying_deltas();
                    if keyboard && step == 1 {
                        response.as_ref().unwrap().request_focus();
                    }
                    assert_eq!(checked, enabled && step >= if keyboard { 2 } else { 3 });
                    if enabled {
                        assert_eq!(boxes.len(), 1);
                        assert_eq!(boxes[0].0.width, 1.0);
                        assert_eq!(
                            boxes[0].1,
                            if checked {
                                theme.ui.text
                            } else {
                                theme.ui.panel_bg
                            }
                        );
                        assert_eq!(ticks, usize::from(checked));
                    }
                    assert!(
                        native_shown,
                        "settings checkbox styling leaked to a native control"
                    );
                    assert_eq!(ctx.global_style().spacing.icon_width, native_icon_width);
                }
            }
        }
    }
}
