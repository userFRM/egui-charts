//! Tab rendering implementations for SettingsDialog
//!
//! This module contains all the tab-specific UI rendering code,
//! keeping the main dialog module focused on core structure.

mod canvas;
mod scales;
mod symbol;
mod trading;

use super::SettingsDialog;
use crate::localization as l10n;
use crate::theme::Theme;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, RichText, Ui};

impl SettingsDialog {
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
