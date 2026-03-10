//! Drawing template management UI.
//!
//! Provides the template button and dropdown menu for saving,
//! loading, and deleting drawing templates from the toolbar.

use crate::ext::UiExt;
use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::DrawingTemplate;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Rect, Ui};

/// Drawing template management button and dropdown.
pub struct TemplatesButton;

impl TemplatesButton {
    /// Render the templates button with a save icon.
    ///
    /// Returns tuple of (icon_clicked, arrow_clicked, button_rect).
    pub fn show(ui: &mut Ui) -> (bool, bool, Rect) {
        let response = draw_tool_button(ui, &icons::SETTINGS, "Drawing Templates", false);

        let arrow_clicked = response.clicked();

        (false, arrow_clicked, response.rect)
    }

    /// Render the templates dropdown submenu.
    ///
    /// Shows save template option, followed by existing templates
    /// that can be loaded or deleted.
    pub fn show_submenu(
        ui: &mut Ui,
        sidebar_rect: Rect,
        category_rect: Option<Rect>,
        templates: &[DrawingTemplate],
        _template_name_input: &mut String,
        _show_name_input: bool,
    ) -> Option<DrawingToolbarAction> {
        let mut builder = SubmenuBuilder::new(ui, sidebar_rect)
            .with_width(DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_lg);

        if let Some(cat_rect) = category_rect {
            builder = builder.with_category_rect(cat_rect);
        }

        // "Save as Template..." option
        builder = builder.add_text_item_with_action(
            "Save as Template...",
            "Save current drawing properties as a reusable template",
            || Some(DrawingToolbarAction::OpenTemplateMenu),
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
    /// or `None` if canceled or still editing.
    pub fn show_name_dialog(
        ui: &mut Ui,
        sidebar_rect: Rect,
        name_input: &mut String,
    ) -> Option<DrawingToolbarAction> {
        let popup_width = DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_lg;
        let popup_pos = egui::Pos2::new(
            sidebar_rect.right() + DESIGN_TOKENS.spacing.sm,
            sidebar_rect.center().y - DESIGN_TOKENS.sizing.button_xxl,
        );

        let mut action = None;

        egui::Area::new(egui::Id::new("template_name_dialog"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(ui.style().visuals.window_fill())
                    .stroke(ui.style().visuals.window_stroke)
                    .corner_radius(DESIGN_TOKENS.rounding.lg)
                    .show(ui, |ui| {
                        ui.set_min_width(popup_width);
                        ui.space_lg();

                        ui.label(
                            egui::RichText::new("Save Template")
                                .size(DESIGN_TOKENS.typography.body)
                                .strong(),
                        );
                        ui.space_md();

                        ui.label("Template name:");
                        ui.space_sm();

                        let text_response = ui.text_edit_singleline(name_input);

                        // Auto-focus the text input
                        if text_response.gained_focus()
                            || ui.memory(|m| !m.has_focus(text_response.id))
                        {
                            text_response.request_focus();
                        }

                        ui.space_lg();

                        ui.horizontal(|ui| {
                            let can_save = !name_input.trim().is_empty();

                            if ui
                                .add_enabled(can_save, egui::Button::new("Save"))
                                .clicked()
                            {
                                action = Some(DrawingToolbarAction::SaveTemplate {
                                    name: name_input.trim().to_string(),
                                });
                            }

                            if ui.button("Cancel").clicked() {
                                // Return None to signal close without saving
                                name_input.clear();
                            }
                        });

                        // Enter key to confirm
                        if ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !name_input.trim().is_empty()
                        {
                            action = Some(DrawingToolbarAction::SaveTemplate {
                                name: name_input.trim().to_string(),
                            });
                        }

                        // Escape key to cancel
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            name_input.clear();
                        }

                        ui.space_md();
                    });
            });

        action
    }
}
