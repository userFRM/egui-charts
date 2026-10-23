//! Main Object Tree panel widget
//!
//! Ties together all subcomponents into the complete Object Tree panel.

use crate::icons::icons;
use crate::ui_kit::{IconButton, PanelHeader, filter::filter_input};
use egui::{Frame, RichText, Ui, Vec2, vec2};

use super::actions::ObjectTreeAction;
use super::config::ObjectTreeConfig;
use super::context_menu::handle_keyboard_shortcuts;
use super::data_window::show_data_window;
use super::source_tree::render_source_tree;
use super::state::ObjectTreeState;
use super::types::{DataWindowInfo, SourceItem, SourceType};
use crate::ext::UiExt;

/// Object Tree panel widget
///
/// Complete object management panel with:
/// - Data Window (OHLCV at cursor)
/// - Hierarchical source tree (indicators, drawings)
/// - Full object management (visibility, lock, delete, etc.)
/// - Multi-selection and keyboard shortcuts
/// - Context menu with all options
///
/// # Example
///
/// ```no_run
/// use egui_charts::ui::widget_bar::panels::object_tree::{ObjectTreePanel, SourceItem};
///
/// # fn side_panel(ui: &mut egui::Ui) {
/// let mut panel = ObjectTreePanel::new();
/// let mut sources = vec![
///     SourceItem::indicator(1, "SMA(20)", egui::Color32::YELLOW),
/// ];
///
/// let action = panel.show(ui, None, &mut sources);
/// # }
/// ```
pub struct ObjectTreePanel {
    /// Configuration
    config: ObjectTreeConfig,
    /// UI state
    state: ObjectTreeState,
}

impl Default for ObjectTreePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectTreePanel {
    /// Create a new Object Tree panel
    pub fn new() -> Self {
        Self {
            config: ObjectTreeConfig::default(),
            state: ObjectTreeState::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ObjectTreeConfig) -> Self {
        Self {
            config,
            state: ObjectTreeState::new(),
        }
    }

    /// Get mutable reference to state (for external manipulation)
    pub fn state_mut(&mut self) -> &mut ObjectTreeState {
        &mut self.state
    }

    /// Get reference to state
    pub fn state(&self) -> &ObjectTreeState {
        &self.state
    }

    /// Get reference to config
    pub fn config(&self) -> &ObjectTreeConfig {
        &self.config
    }

    /// Set configuration
    pub fn set_config(&mut self, config: ObjectTreeConfig) {
        self.config = config;
    }

    /// Main show method
    ///
    /// Renders the complete Object Tree panel and returns any action.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        data_window: Option<&DataWindowInfo>,
        sources: &mut [SourceItem],
    ) -> ObjectTreeAction {
        let mut action = ObjectTreeAction::None;
        for source in sources.iter_mut() {
            if let Some(label) = self.state.labels.get(&source.id) {
                source.label = Some(label.clone());
            }
        }

        if self.config.group_by_type {
            sources.sort_by_key(|s| {
                let group = match s.source_type {
                    SourceType::DataSource => 0,
                    SourceType::Indicator => 1,
                    SourceType::Drawing => 2,
                    SourceType::Template => 3,
                };
                (group, std::cmp::Reverse(s.z_index))
            });
        } else {
            sources.sort_by_key(|s| std::cmp::Reverse(s.z_index));
        }

        // Handle keyboard shortcuts
        if self.config.enable_keyboard_shortcuts {
            let kb_action = handle_keyboard_shortcuts(ui, &mut self.state, sources);
            if !kb_action.is_none() {
                action = kb_action;
            }
        }

        // Main panel frame
        Frame::new().fill(ui.visuals().panel_fill).show(ui, |ui| {
            // Header
            self.show_header(ui, sources, &mut action);

            ui.separator();

            // Data Window section (collapsible)
            if self.config.show_data_window
                && let Some(data) = data_window
            {
                let expanded = show_data_window(ui, data, self.state.data_window_expanded);
                self.state.data_window_expanded = expanded;
                ui.separator();
            }

            // Filter controls
            self.show_filter_controls(ui);

            ui.space_sm();

            // Source tree
            let mut tree_config = self.config.clone();
            tree_config.max_height = tree_config
                .max_height
                .min((ui.available_height() - 42.0).max(0.0));
            let tree_action = render_source_tree(ui, sources, &tree_config, &mut self.state);
            if !tree_action.is_none() {
                action = tree_action;
            }

            // Keep the footer at the panel bottom even when the tree has a height cap.
            ui.add_space((ui.available_height() - 42.0).max(0.0));
            ui.separator();
            self.show_footer(ui, sources);
        });

        if let Some(id) = self.state.renaming_id {
            use crate::ui_kit::modal::dialogs::{PromptDialog, PromptResult};
            let mut open = true;
            match PromptDialog::new(ui.scope_id().with(("rename_object", id)), "Rename object")
                .initial_value(&self.state.rename_text)
                .show(ui.ctx(), &mut open)
            {
                PromptResult::Submitted(name) if !name.trim().is_empty() => {
                    self.state.labels.insert(id, name.clone());
                    self.state.cancel_rename();
                    action = ObjectTreeAction::Rename(id, name);
                }
                PromptResult::Cancelled | PromptResult::Submitted(_) => self.state.cancel_rename(),
                PromptResult::None => {}
            }
        }

        // Handle selection state updates
        self.handle_selection_action(&action, sources);

        action
    }

    /// Render header with title and controls
    fn show_header(&mut self, ui: &mut Ui, _sources: &[SourceItem], action: &mut ObjectTreeAction) {
        PanelHeader::new("Objects")
            .strong()
            .no_separator()
            .show(ui, |ui| {
                // Settings menu
                let settings = IconButton::new(&icons::SETTINGS)
                    .tooltip("Object tree settings")
                    .show(ui);
                egui::Popup::menu(&settings).show(|ui| {
                    ui.checkbox(&mut self.config.show_data_window, "Show Data Window");
                    ui.checkbox(&mut self.config.group_by_type, "Group by Type");
                    ui.checkbox(&mut self.config.show_color_indicators, "Show Colors");
                    ui.checkbox(&mut self.config.show_item_counts, "Show Counts");
                });

                // Expand all
                if IconButton::new(&icons::CHEVRON_DOWN)
                    .tooltip("Expand all")
                    .show(ui)
                    .clicked()
                {
                    self.state.expand_all_groups();
                }

                // Collapse all
                if IconButton::new(&icons::CHEVRON_UP)
                    .tooltip("Collapse all")
                    .show(ui)
                    .clicked()
                {
                    self.state.collapse_all_groups();
                }

                // Bulk actions menu
                let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                    .tooltip("Object actions")
                    .show(ui);
                egui::Popup::menu(&more).show(|ui| {
                    if ui.button("Show All").clicked() {
                        *action = ObjectTreeAction::ShowAll;
                        ui.close();
                    }
                    if ui.button("Hide All").clicked() {
                        *action = ObjectTreeAction::HideAll;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Lock All").clicked() {
                        *action = ObjectTreeAction::LockAll;
                        ui.close();
                    }
                    if ui.button("Unlock All").clicked() {
                        *action = ObjectTreeAction::UnlockAll;
                        ui.close();
                    }
                    ui.separator();
                    if self.state.selection_count() > 0 && ui.button("Delete Selected").clicked() {
                        *action = ObjectTreeAction::DeleteSelected;
                        ui.close();
                    }
                });
            });
    }

    /// Render filter controls
    fn show_filter_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let combo_width = 108.0;
            let field_width =
                (ui.available_width() - combo_width - ui.spacing().item_spacing.x).max(0.0);
            ui.allocate_ui(vec2(field_width, 28.0), |ui| {
                ui.spacing_mut().interact_size.y = 24.0;
                filter_input(ui, &mut self.state.filter_text);
            });
            ui.spacing_mut().interact_size.y = 28.0;
            egui::ComboBox::from_id_salt("object_tree_type_filter")
                .width(combo_width)
                .selected_text(self.state.filter_type.map_or("All", |t| t.display_name()))
                .icon(|ui, rect, visuals, _| {
                    icons::CHEVRON_DOWN
                        .as_image_tinted(Vec2::splat(12.0), visuals.fg_stroke.color)
                        .paint_at(ui, rect);
                })
                .show_ui(ui, |ui| {
                    ui.set_min_width(combo_width - 2.0 * ui.spacing().menu_margin.leftf());
                    for source_type in [
                        None,
                        Some(SourceType::DataSource),
                        Some(SourceType::Indicator),
                        Some(SourceType::Drawing),
                        Some(SourceType::Template),
                    ] {
                        let label = source_type.map_or("All", |t| t.display_name());
                        ui.selectable_value(&mut self.state.filter_type, source_type, label);
                    }
                });
        });
    }

    /// Render footer with stats
    fn show_footer(&self, ui: &mut Ui, sources: &[SourceItem]) {
        ui.horizontal(|ui| {
            let total = sources.len();
            let visible = sources.iter().filter(|s| s.visible).count();
            let selected = self.state.selection_count();
            let indicators = sources
                .iter()
                .filter(|s| s.source_type == SourceType::Indicator)
                .count();
            let drawings = sources
                .iter()
                .filter(|s| s.source_type == SourceType::Drawing)
                .count();

            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} objects • {} indicators • {} drawings • {} visible{}",
                        total,
                        indicators,
                        drawings,
                        visible,
                        if selected > 0 {
                            format!(" • {selected} selected")
                        } else {
                            String::new()
                        }
                    ))
                    .color(ui.visuals().weak_text_color())
                    .size(11.0),
                )
                .wrap(),
            );
        });
    }

    /// Handle selection state updates based on action
    fn handle_selection_action(&mut self, action: &ObjectTreeAction, sources: &[SourceItem]) {
        let source_ids: Vec<usize> = sources.iter().map(|s| s.id).collect();

        match action {
            ObjectTreeAction::Select(id) => {
                self.state.select(*id);
            }
            ObjectTreeAction::AddToSelection(id) => {
                self.state.toggle_selection(*id);
            }
            ObjectTreeAction::RangeSelect(id) => {
                self.state.range_select(*id, &source_ids);
            }
            ObjectTreeAction::ClearSelection => {
                self.state.clear_selection();
            }
            ObjectTreeAction::SelectAll => {
                self.state.select_all(&source_ids);
            }
            _ => {}
        }
    }

    /// Get selected item IDs
    pub fn selected_ids(&self) -> Vec<usize> {
        self.state.selected_ids.iter().copied().collect()
    }

    /// Clear all state
    pub fn reset(&mut self) {
        self.state = ObjectTreeState::new();
    }
}
