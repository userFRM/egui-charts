//! Main Object Tree panel widget
//!
//! Ties together all subcomponents into the complete Object Tree panel.

use crate::icons::icons;
use crate::styles::icons as icon_sizes;
use crate::ui_kit::PanelHeader;
use egui::{Frame, RichText, TextEdit, Ui, Vec2, vec2};

use super::actions::ObjectTreeAction;
use super::config::ObjectTreeConfig;
use super::context_menu::{handle_keyboard_shortcuts, render_context_menu};
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
/// use egui_open_trading_charts_rs::ui::widget_bar::panels::object_tree::{
///     ObjectTreePanel, ObjectTreeAction, SourceItem, DataWindowInfo
/// };
///
/// let mut panel = ObjectTreePanel::new();
/// let mut sources = vec![
///     SourceItem::indicator(1, "SMA(20)", egui::Color32::YELLOW),
/// ];
///
/// let action = panel.show(ui, None, &mut sources);
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

        // Collect source IDs for keyboard shortcuts
        let source_ids: Vec<usize> = sources.iter().map(|s| s.id).collect();

        // Handle keyboard shortcuts
        if self.config.enable_keyboard_shortcuts {
            let kb_action = handle_keyboard_shortcuts(ui, &mut self.state, &source_ids);
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
            let tree_action = render_source_tree(ui, sources, &self.config, &mut self.state);
            if !tree_action.is_none() {
                action = tree_action;
            }

            // Footer
            ui.separator();
            self.show_footer(ui, sources);
        });

        // Context menu (rendered on top)
        if self.state.context_menu_open {
            let menu_action = render_context_menu(ui, sources, &mut self.state);
            if !menu_action.is_none() {
                action = menu_action;
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
                ui.menu_button("S", |ui| {
                    ui.checkbox(&mut self.config.show_data_window, "Show Data Window");
                    ui.checkbox(&mut self.config.group_by_type, "Group by Type");
                    ui.checkbox(&mut self.config.show_color_indicators, "Show Colors");
                    ui.checkbox(&mut self.config.show_item_counts, "Show Counts");
                });

                // Expand all
                if ui.small_button("+").on_hover_text("Expand all").clicked() {
                    self.state.expand_all_groups();
                }

                // Collapse all
                if ui.small_button("-").on_hover_text("Collapse all").clicked() {
                    self.state.collapse_all_groups();
                }

                // Bulk actions menu
                ui.menu_button(
                    icons::UI_MORE_HORIZONTAL.as_image(Vec2::splat(icon_sizes::SM)),
                    |ui| {
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
                        if self.state.selection_count() > 0
                            && ui.button("Delete Selected").clicked()
                        {
                            *action = ObjectTreeAction::DeleteSelected;
                            ui.close();
                        }
                    },
                );
            });
    }

    /// Render filter controls
    fn show_filter_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            // Search input
            let _search_response = ui.add_sized(
                vec2(120.0, 20.0),
                TextEdit::singleline(&mut self.state.filter_text).hint_text("Filter..."),
            );

            // Type filter dropdown
            ui.combo_select_width(
                "object_tree_type_filter",
                &mut self.state.filter_type,
                [
                    None,
                    Some(SourceType::DataSource),
                    Some(SourceType::Indicator),
                    Some(SourceType::Drawing),
                    Some(SourceType::Template),
                ],
                |v| match v {
                    None => "All".to_string(),
                    Some(st) => st.display_name().to_string(),
                },
                95.0,
            );

            // Clear filter button
            if self.state.has_filter()
                && ui.small_button("X").on_hover_text("Clear filter").clicked()
            {
                self.state.clear_filter();
            }
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

            ui.label(
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
                .weak()
                .small(),
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
