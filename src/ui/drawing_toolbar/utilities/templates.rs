//! Drawing template management UI.
//!
//! Provides the visibility flyout rows and name dialog for saving
//! and loading drawing templates.

use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::DrawingTemplate;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use crate::ui_kit::ext::UiExt;
use egui::{Rect, Ui};

/// Drawing template rows and save dialog.
pub struct TemplatesButton;

impl TemplatesButton {
    /// Render the templates dropdown rows.
    ///
    /// Shows save template option, followed by existing templates
    /// that can be loaded.
    pub fn show_submenu(
        ui: &mut Ui,
        templates: &[DrawingTemplate],
    ) -> Option<DrawingToolbarAction> {
        let mut builder = SubmenuBuilder::new(ui).add_text_item_with_action(
            "Save as template…",
            "Save current drawing properties as a reusable template",
            || Some(DrawingToolbarAction::OpenTemplateMenu),
        );

        builder = builder.add_text_item_with_action(
            "Manage templates…",
            "Load or delete saved drawing templates",
            || Some(DrawingToolbarAction::ManageTemplates),
        );

        // List existing templates
        for template in templates {
            let template_id = template.id.clone();
            let name = if template.is_default {
                format!("{} (default)", template.name)
            } else {
                template.name.clone()
            };
            let tooltip = format!(
                "Load template: {} ({})",
                template.name, template.drawing_type
            );

            builder = builder.add_text_item_with_action(&name, &tooltip, move || {
                Some(DrawingToolbarAction::LoadTemplate {
                    template_id: template_id.clone(),
                })
            });
        }

        builder.show()
    }

    /// Render a standalone save-template name input dialog.
    ///
    /// Returns a `SaveTemplate` action if the user confirms,
    /// [`DrawingToolbarAction::None`] if they cancel with the Cancel button or Escape, which
    /// also clears `name_input`, and `None` while they are still editing.
    pub fn show_name_dialog(
        ui: &mut Ui,
        sidebar_rect: Rect,
        name_input: &mut String,
    ) -> Option<DrawingToolbarAction> {
        let popup_width = DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_lg;
        let popup_pos = egui::Pos2::new(
            sidebar_rect.right() + ui.active_theme().spacing.sm,
            sidebar_rect.center().y - DESIGN_TOKENS.sizing.button_xxl,
        );

        let mut action = None;

        egui::Area::new(egui::Id::unique("template_name_dialog"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(ui.style().visuals.window_fill())
                    .stroke(ui.style().visuals.window_stroke)
                    .corner_radius(ui.active_theme().radius.lg)
                    .show(ui, |ui| {
                        ui.set_min_width(popup_width);
                        ui.space_lg();

                        ui.label(
                            egui::RichText::new(l10n::text(ui.ctx(), "Save Template"))
                                .size(ui.active_theme().text.body)
                                .strong(),
                        );
                        ui.space_md();

                        ui.label(l10n::text(ui.ctx(), "Template name:"));
                        ui.space_sm();

                        let text_response = ui.text_edit_singleline(name_input);

                        let focus_in_dialog = ui
                            .memory(|m| m.focused())
                            .and_then(|id| ui.ctx().read_response(id))
                            .is_some_and(|response| response.layer_id == ui.layer_id());
                        if !focus_in_dialog && !text_response.lost_focus() {
                            text_response.request_focus();
                        }

                        ui.space_lg();

                        ui.horizontal(|ui| {
                            let can_save = !name_input.trim().is_empty();

                            if ui
                                .add_enabled(
                                    can_save,
                                    egui::Button::new(l10n::text(ui.ctx(), "Save")),
                                )
                                .clicked()
                            {
                                action = Some(DrawingToolbarAction::SaveTemplate {
                                    name: name_input.trim().to_string(),
                                });
                            }

                            if ui.button(l10n::text(ui.ctx(), "Cancel")).clicked() {
                                name_input.clear();
                                action = Some(DrawingToolbarAction::None);
                            }
                        });

                        // Enter key to confirm
                        if ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && (text_response.has_focus()
                                || ui.memory(|m| m.had_focus_last_frame(text_response.id)))
                            && !name_input.trim().is_empty()
                        {
                            action = Some(DrawingToolbarAction::SaveTemplate {
                                name: name_input.trim().to_string(),
                            });
                        }

                        // Escape key to cancel
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            name_input.clear();
                            action = Some(DrawingToolbarAction::None);
                        }

                        ui.space_md();
                    });
            });

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_focus_reaches_cancel_without_saving_the_template() {
        let ctx = egui::Context::default();
        let mut name = "Example".to_owned();
        let mut frame = |key: Option<egui::Key>| {
            let mut action = None;
            ctx.run_ui(
                egui::RawInput {
                    events: key
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        })
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    action = TemplatesButton::show_name_dialog(
                        ui,
                        Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(48.0, 200.0)),
                        &mut name,
                    );
                },
            )
            .drop_without_applying_deltas();
            (action, ctx.memory(|m| m.focused()))
        };
        frame(None);
        let name_field = frame(None).1;
        let save = frame(Some(egui::Key::Tab)).1;
        assert_ne!(save, name_field);
        frame(Some(egui::Key::Tab));
        assert!(matches!(
            frame(Some(egui::Key::Enter)).0,
            Some(DrawingToolbarAction::None)
        ));
        assert!(name.is_empty());
    }
}
