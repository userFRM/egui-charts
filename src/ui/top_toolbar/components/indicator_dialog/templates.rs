//! Indicator template management UI
//!
//! Provides the template panel within the indicator dialog for saving,
//! loading, and managing indicator parameter templates.

use crate::ext::UiExt;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::IndicatorTemplate;
use crate::ui_kit::{EmptyState, LoadingIndicator};
use egui::{Color32, Pos2, Rect, RichText, Sense, Ui, Vec2};

// =============================================================================
// Actions
// =============================================================================

/// Actions emitted by the indicator template panel
#[derive(Debug, Clone)]
pub enum IndicatorTemplateAction {
    /// No action taken
    None,
    /// Save current indicator parameters as a new template
    Save { name: String, description: String },
    /// Load a template's parameters into the current indicator
    Load { template_id: String },
    /// Delete a template
    Delete { template_id: String },
    /// Rename a template
    Rename {
        template_id: String,
        new_name: String,
    },
    /// Toggle shared/personal visibility
    ToggleShared {
        template_id: String,
        is_shared: bool,
    },
}

// =============================================================================
// State
// =============================================================================

/// State for the indicator template panel
#[derive(Debug, Clone, Default)]
pub struct IndicatorTemplateState {
    /// Available templates for the current indicator type
    pub templates: Vec<IndicatorTemplate>,
    /// Currently selected template (for preview/actions)
    pub selected_template: Option<String>,
    /// Whether the template list is loading from the API
    pub is_loading: bool,
    /// Whether the save form is visible
    pub show_save_form: bool,
    /// Name input for saving a new template
    pub save_name: String,
    /// Description input for saving a new template
    pub save_description: String,
    /// Template pending deletion (for confirmation)
    pub pending_delete: Option<String>,
    /// Whether to show shared templates
    pub show_shared: bool,
}

impl IndicatorTemplateState {
    /// Reset the save form fields
    pub fn reset_save_form(&mut self) {
        self.show_save_form = false;
        self.save_name.clear();
        self.save_description.clear();
    }

    /// Cancel pending deletion
    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Set templates from API response
    pub fn set_templates(&mut self, templates: Vec<IndicatorTemplate>) {
        self.templates = templates;
        self.is_loading = false;
    }
}

// =============================================================================
// Panel
// =============================================================================

/// Indicator template management panel
///
/// Renders inside the indicator dialog's configuration area.
/// Shows template list, save form, and action buttons.
pub struct IndicatorTemplatePanel<'a> {
    state: &'a mut IndicatorTemplateState,
    indicator_type: &'a str,
}

impl<'a> IndicatorTemplatePanel<'a> {
    /// Create a new template panel for the given indicator type
    pub fn new(state: &'a mut IndicatorTemplateState, indicator_type: &'a str) -> Self {
        Self {
            state,
            indicator_type,
        }
    }

    /// Show the template panel and return any action
    pub fn show(&mut self, ui: &mut Ui) -> IndicatorTemplateAction {
        let mut action = IndicatorTemplateAction::None;

        self.draw_header(ui);
        ui.space_sm();

        // Shared/personal toggle
        action = self.draw_filter_toggle(ui, action);
        ui.spaced_separator();

        // Save current button
        action = self.draw_save_section(ui, action);
        ui.spaced_separator();

        // Template list
        action = self.draw_template_list(ui, action);

        action
    }

    // =========================================================================
    // Header
    // =========================================================================

    fn draw_header(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Templates")
                    .size(DESIGN_TOKENS.typography.body)
                    .strong(),
            );
            ui.label(
                RichText::new(format!("({})", self.indicator_type))
                    .size(DESIGN_TOKENS.typography.small)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }

    // =========================================================================
    // Filter toggle
    // =========================================================================

    fn draw_filter_toggle(
        &mut self,
        ui: &mut Ui,
        current_action: IndicatorTemplateAction,
    ) -> IndicatorTemplateAction {
        ui.horizontal(|ui| {
            let personal_label = if self.state.show_shared {
                "Personal"
            } else {
                "> Personal"
            };
            let shared_label = if self.state.show_shared {
                "> Shared"
            } else {
                "Shared"
            };

            if ui
                .selectable_label(!self.state.show_shared, personal_label)
                .clicked()
            {
                self.state.show_shared = false;
            }
            if ui
                .selectable_label(self.state.show_shared, shared_label)
                .clicked()
            {
                self.state.show_shared = true;
            }
        });
        current_action
    }

    // =========================================================================
    // Save section
    // =========================================================================

    fn draw_save_section(
        &mut self,
        ui: &mut Ui,
        current_action: IndicatorTemplateAction,
    ) -> IndicatorTemplateAction {
        let mut action = current_action;

        if !self.state.show_save_form {
            if ui.button("Save Current as Template...").clicked() {
                self.state.show_save_form = true;
                self.state.save_name.clear();
                self.state.save_description.clear();
            }
            return action;
        }

        action = self.draw_save_form(ui);
        action
    }

    fn draw_save_form(&mut self, ui: &mut Ui) -> IndicatorTemplateAction {
        let mut action = IndicatorTemplateAction::None;

        ui.label(
            RichText::new("Save Template")
                .size(DESIGN_TOKENS.typography.body)
                .strong(),
        );
        ui.space_sm();

        ui.label("Name:");
        let name_response = ui.text_edit_singleline(&mut self.state.save_name);

        // Auto-focus the name field when save form opens
        if !name_response.has_focus() {
            name_response.request_focus();
        }

        ui.space_sm();
        ui.label("Description (optional):");
        ui.text_edit_singleline(&mut self.state.save_description);

        ui.space_lg();

        let can_save = !self.state.save_name.trim().is_empty();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_save, egui::Button::new("Save"))
                .clicked()
            {
                action = IndicatorTemplateAction::Save {
                    name: self.state.save_name.trim().to_string(),
                    description: self.state.save_description.trim().to_string(),
                };
                self.state.reset_save_form();
            }

            if ui.button("Cancel").clicked() {
                self.state.reset_save_form();
            }
        });

        // Enter key to confirm
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && can_save {
            action = IndicatorTemplateAction::Save {
                name: self.state.save_name.trim().to_string(),
                description: self.state.save_description.trim().to_string(),
            };
            self.state.reset_save_form();
        }

        // Escape key to cancel
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.state.reset_save_form();
        }

        action
    }

    // =========================================================================
    // Template list
    // =========================================================================

    fn draw_template_list(
        &mut self,
        ui: &mut Ui,
        current_action: IndicatorTemplateAction,
    ) -> IndicatorTemplateAction {
        let mut action = current_action;

        if self.state.is_loading {
            LoadingIndicator::new()
                .message("Loading templates...")
                .show(ui);
            return action;
        }

        let filtered: Vec<IndicatorTemplate> = self
            .state
            .templates
            .iter()
            .filter(|t| t.is_shared == self.state.show_shared)
            .cloned()
            .collect();

        if filtered.is_empty() {
            let msg = if self.state.show_shared {
                "No shared templates"
            } else {
                "No saved templates"
            };
            EmptyState::new(msg).show(ui);
            return action;
        }

        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .show(ui, |ui| {
                for template in &filtered {
                    if let Some(a) = self.draw_template_item(ui, template) {
                        action = a;
                    }
                }
            });

        action
    }

    fn draw_template_item(
        &mut self,
        ui: &mut Ui,
        template: &IndicatorTemplate,
    ) -> Option<IndicatorTemplateAction> {
        let item_height = DESIGN_TOKENS.sizing.button_lg;
        let desired_size = Vec2::new(ui.available_width() - DESIGN_TOKENS.spacing.lg, item_height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        let is_selected = self
            .state
            .selected_template
            .as_ref()
            .is_some_and(|id| id == &template.id);

        // Background
        self.draw_item_bg(ui, rect, &response, is_selected);

        // Text content
        self.draw_item_content(ui, rect, template);

        // Handle selection and actions
        self.handle_item_interaction(template, &response, is_selected)
    }

    fn draw_item_bg(&self, ui: &mut Ui, rect: Rect, response: &egui::Response, is_selected: bool) {
        if response.hovered() || is_selected {
            let bg = if is_selected {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.hovered.bg_fill
            };
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg);
        }
    }

    fn draw_item_content(&self, ui: &mut Ui, rect: Rect, template: &IndicatorTemplate) {
        let text_x = rect.min.x + DESIGN_TOKENS.spacing.lg;

        // Template name
        ui.painter().text(
            Pos2::new(text_x, rect.min.y + DESIGN_TOKENS.spacing.xl),
            egui::Align2::LEFT_CENTER,
            &template.name,
            egui::FontId::proportional(typography::MD),
            ui.visuals().text_color(),
        );

        // Parameter summary and shared badge
        let summary = template.parameter_summary();
        let suffix = if template.is_shared {
            format!("{summary} [shared]")
        } else {
            summary
        };

        ui.painter().text(
            Pos2::new(text_x, rect.min.y + DESIGN_TOKENS.sizing.button_sm),
            egui::Align2::LEFT_CENTER,
            &suffix,
            egui::FontId::proportional(typography::XS),
            ui.visuals().weak_text_color(),
        );
    }

    fn handle_item_interaction(
        &mut self,
        template: &IndicatorTemplate,
        response: &egui::Response,
        is_selected: bool,
    ) -> Option<IndicatorTemplateAction> {
        if response.clicked() {
            if is_selected {
                // Double-click loads the template
                return Some(IndicatorTemplateAction::Load {
                    template_id: template.id.clone(),
                });
            }
            self.state.selected_template = Some(template.id.clone());
        }

        // Context menu (right-click on desktop)
        let mut action = None;
        response.context_menu(|ui| {
            if ui.button("Load").clicked() {
                action = Some(IndicatorTemplateAction::Load {
                    template_id: template.id.clone(),
                });
                ui.close();
            }

            let share_label = if template.is_shared {
                "Make Personal"
            } else {
                "Share with Team"
            };
            if ui.button(share_label).clicked() {
                action = Some(IndicatorTemplateAction::ToggleShared {
                    template_id: template.id.clone(),
                    is_shared: !template.is_shared,
                });
                ui.close();
            }

            ui.separator();

            // Delete with confirmation
            if self.state.pending_delete.as_ref() == Some(&template.id) {
                ui.colored_label(Color32::from_rgb(239, 83, 80), "Confirm delete?");
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        action = Some(IndicatorTemplateAction::Delete {
                            template_id: template.id.clone(),
                        });
                        self.state.pending_delete = None;
                        ui.close();
                    }
                    if ui.button("No").clicked() {
                        self.state.pending_delete = None;
                        ui.close();
                    }
                });
            } else if ui.button("Delete").clicked() {
                self.state.pending_delete = Some(template.id.clone());
            }
        });

        action
    }
}
