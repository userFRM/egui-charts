//! Main Object Tree panel widget
//!
//! Ties together all subcomponents into the complete Object Tree panel.

use crate::icons::icons;
use crate::localization as l10n;
use crate::ui_kit::{IconButton, PanelHeader, filter::filter_input};
use egui::{Frame, RichText, Ui, vec2};

use super::super::controls::dropdown;
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
            self.show_header(ui, &mut action);

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

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                self.show_footer(ui, sources);
                ui.separator();
                let mut tree_config = self.config.clone();
                tree_config.max_height = tree_config.max_height.min(ui.available_height());
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    let tree_action =
                        render_source_tree(ui, sources, &tree_config, &mut self.state);
                    if !tree_action.is_none() {
                        action = tree_action;
                    }
                });
            });
        });

        if let Some(id) = self.state.renaming_id {
            use crate::ui_kit::modal::dialogs::{PromptDialog, PromptResult};
            let mut open = true;
            match PromptDialog::new(ui.scope_id().with(("rename_object", id)), "Rename object")
                .initial_value(&self.state.rename_text)
                .show(ui.ctx(), &mut open)
            {
                PromptResult::Submitted(name) if !name.trim().is_empty() => {
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
    fn show_header(&mut self, ui: &mut Ui, action: &mut ObjectTreeAction) {
        PanelHeader::new("Objects")
            .strong()
            .no_separator()
            .show(ui, |ui| {
                // Settings menu
                let settings = IconButton::new(&icons::SETTINGS)
                    .tooltip("Object tree settings")
                    .show(ui);
                egui::Popup::menu(&settings).show(|ui| {
                    ui.checkbox(
                        &mut self.config.show_data_window,
                        l10n::text(ui.ctx(), "Show Data Window"),
                    );
                    ui.checkbox(
                        &mut self.config.group_by_type,
                        l10n::text(ui.ctx(), "Group by Type"),
                    );
                    ui.checkbox(
                        &mut self.config.show_color_indicators,
                        l10n::text(ui.ctx(), "Show Colors"),
                    );
                    ui.checkbox(
                        &mut self.config.show_item_counts,
                        l10n::text(ui.ctx(), "Show Counts"),
                    );
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
                    if ui.button(l10n::text(ui.ctx(), "Show all")).clicked() {
                        *action = ObjectTreeAction::ShowAll;
                        ui.close();
                    }
                    if ui.button(l10n::text(ui.ctx(), "Hide all")).clicked() {
                        *action = ObjectTreeAction::HideAll;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(l10n::text(ui.ctx(), "Lock all")).clicked() {
                        *action = ObjectTreeAction::LockAll;
                        ui.close();
                    }
                    if ui.button(l10n::text(ui.ctx(), "Unlock all")).clicked() {
                        *action = ObjectTreeAction::UnlockAll;
                        ui.close();
                    }
                    ui.separator();
                    if self.state.selection_count() > 0
                        && ui.button(l10n::text(ui.ctx(), "Delete selected")).clicked()
                    {
                        *action = ObjectTreeAction::DeleteSelected;
                        ui.close();
                    }
                });
            });
    }

    /// Render filter controls
    fn show_filter_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let type_width = 108.0;
            let field_width =
                (ui.available_width() - type_width - ui.spacing().item_spacing.x).max(0.0);
            ui.allocate_ui(vec2(field_width, 28.0), |ui| {
                ui.spacing_mut().interact_size.y = 28.0;
                filter_input(ui, &mut self.state.filter_text);
            });
            ui.spacing_mut().interact_size.y = 28.0;
            ui.allocate_ui(vec2(type_width, 28.0), |ui| {
                dropdown(
                    ui,
                    "object_tree_type_filter",
                    self.state.filter_type.map_or("All", |t| t.display_name()),
                    |ui| {
                        for source_type in [
                            None,
                            Some(SourceType::DataSource),
                            Some(SourceType::Indicator),
                            Some(SourceType::Drawing),
                            Some(SourceType::Template),
                        ] {
                            let label = source_type.map_or("All", |t| t.display_name());
                            ui.selectable_value(
                                &mut self.state.filter_type,
                                source_type,
                                l10n::text(ui.ctx(), label),
                            );
                        }
                    },
                )
                .accessible_name(l10n::text(ui.ctx(), "Object type"));
            });
        });
    }

    /// Render footer with stats
    fn show_footer(&self, ui: &mut Ui, sources: &[SourceItem]) {
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
                RichText::new(l10n::format_text(
                    ui.ctx(),
                    "{0} objects • {1} indicators • {2} drawings • {3} visible{4}",
                    &[
                        ("0", format!("{}", total)),
                        ("1", format!("{}", indicators)),
                        ("2", format!("{}", drawings)),
                        ("3", format!("{}", visible)),
                        (
                            "4",
                            (if selected > 0 {
                                format!(" • {selected} selected")
                            } else {
                                String::new()
                            })
                            .to_string(),
                        ),
                    ],
                ))
                .color(ui.visuals().weak_text_color())
                .size(11.0),
            )
            .wrap(),
        );
    }

    /// Handle selection state updates based on action
    fn handle_selection_action(&mut self, action: &ObjectTreeAction, sources: &[SourceItem]) {
        let filter = self.state.filter_text.to_lowercase();
        let source_ids: Vec<usize> = sources
            .iter()
            .filter(|s| super::source_tree::matches_filter(s, &filter, &self.state))
            .map(|s| s.id)
            .collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_all_and_range_selection_respect_the_filter() {
        let mut panel = ObjectTreePanel::new();
        let sources = [
            SourceItem::indicator(1, "Fast", egui::Color32::BLUE),
            SourceItem::data_source(2, "Other"),
            SourceItem::indicator(3, "Slow", egui::Color32::BLUE),
        ];
        panel.state.filter_type = Some(SourceType::Indicator);
        for action in [
            ObjectTreeAction::SelectAll,
            ObjectTreeAction::RangeSelect(3),
        ] {
            panel.state.select(1);
            panel.handle_selection_action(&action, &sources);
            assert!(panel.state.is_selected(1));
            assert!(panel.state.is_selected(3));
            assert!(!panel.state.is_selected(2));
        }
    }

    #[test]
    fn filter_controls_align_and_footer_stays_inside_a_narrow_panel() {
        for (theme, width) in [
            (crate::theme::Theme::light(), 220.0),
            (crate::theme::Theme::light(), 280.0),
            (crate::theme::Theme::dark(), 220.0),
            (crate::theme::Theme::dark(), 280.0),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut panel = ObjectTreePanel::new();
            let mut sources: Vec<_> = (0..30)
                .map(|id| SourceItem::indicator(id, "SMA", egui::Color32::BLUE))
                .collect();
            panel.state.select_all(&(0..30).collect::<Vec<_>>());
            for _ in 0..2 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 360.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        panel.show(ui, None, &mut sources);
                    },
                );
                output.textures_delta.clear();
                let controls: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.rect.width() > 70.0
                                && (20.0..32.0).contains(&rect.rect.height()) =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!controls.is_empty(), "{controls:?}");
                assert_eq!(controls[0].height(), 28.0, "filter at {width}");
                let dropdown = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "All" => {
                            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(
                    (dropdown.center().y - controls[0].center().y).abs() < 2.0,
                    "dropdown and filter must align at {width}"
                );
                assert!(dropdown.right() <= width, "dropdown at {width}");
                let footer = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text().starts_with("30 objects") => {
                            Some(text)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(
                    footer.pos.y + footer.galley.rect.bottom() <= 360.0,
                    "footer at {width}: bottom {}",
                    footer.pos.y + footer.galley.rect.bottom()
                );
                output.drop_without_applying_deltas();
            }
        }
    }
}
