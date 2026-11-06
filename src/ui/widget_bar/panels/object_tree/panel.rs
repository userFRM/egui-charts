//! Main Object Tree panel widget
//!
//! Ties together all subcomponents into the complete Object Tree panel.

use crate::config::DrawingSort;
use crate::icons::icons;
use crate::localization as l10n;
use crate::ui_kit::{IconButton, PanelHeader, buttons::ButtonSize, filter::filter_input};
use crate::widget::SymbolDrawings;
use egui::{Frame, RichText, ScrollArea, Ui, vec2};

use super::super::controls::dropdown;
use super::actions::ObjectTreeAction;
use super::config::ObjectTreeConfig;
use super::context_menu::handle_keyboard_shortcuts;
use super::data_window::show_data_window;
use super::source_tree::render_source_tree;
use super::state::ObjectTreeState;
use super::types::{DataWindowInfo, SourceItem, SourceType};
use crate::ui_kit::ext::UiExt;

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
        self.state.folders = sources
            .iter()
            .filter(|s| s.is_drawing())
            .flat_map(|s| (1..=s.folder.len()).map(|depth| s.folder[..depth].to_vec()))
            .collect();
        let folders = &self.state.folders;
        self.state
            .collapsed_folders
            .retain(|path| folders.contains(path));

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
        if self.config.enable_keyboard_shortcuts && !self.state.showing_symbol_drawings {
            let kb_action = handle_keyboard_shortcuts(ui, &mut self.state, sources);
            if !kb_action.is_none() {
                action = kb_action;
            }
        }

        // Main panel frame
        Frame::new().fill(ui.visuals().panel_fill).show(ui, |ui| {
            // Header
            let can_group = sources
                .iter()
                .any(|s| s.is_drawing() && self.state.is_selected(s.id));
            self.show_header(ui, &mut action, can_group);

            ui.separator();

            if self.state.showing_symbol_drawings {
                self.show_symbol_drawings(ui, &mut action);
                return;
            }

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

        if let Some(parent) = self.state.creating_folder.clone() {
            use crate::ui_kit::modal::dialogs::{PromptDialog, PromptResult};
            let mut open = true;
            match PromptDialog::new(ui.scope_id().with("new_folder"), "New folder")
                .placeholder("Folder name")
                .show(ui.ctx(), &mut open)
            {
                PromptResult::Submitted(name) => {
                    self.state.creating_folder = None;
                    if !name.trim().is_empty() {
                        action = ObjectTreeAction::CreateFolder(parent, name);
                    }
                }
                PromptResult::Cancelled => self.state.creating_folder = None,
                PromptResult::None => {}
            }
        }

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
    fn show_header(&mut self, ui: &mut Ui, action: &mut ObjectTreeAction, can_group: bool) {
        PanelHeader::new("Objects")
            .strong()
            .no_separator()
            .show(ui, |ui| {
                if IconButton::new(&icons::PLUS)
                    .tooltip("New folder with selected drawings")
                    .enabled(can_group)
                    .show(ui)
                    .clicked()
                {
                    self.state.creating_folder = Some(Vec::new());
                }
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
                    if self.state.symbol_drawings.is_some() {
                        if ui
                            .button(l10n::text(ui.ctx(), "Drawings by symbol"))
                            .clicked()
                        {
                            self.state.showing_symbol_drawings = true;
                            *action = ObjectTreeAction::ReadSymbolDrawings;
                            ui.close();
                        }
                        ui.separator();
                    }
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

    /// Lists the layout's drawings by symbol in place of the objects.
    fn show_symbol_drawings(&mut self, ui: &mut Ui, action: &mut ObjectTreeAction) {
        ui.horizontal(|ui| {
            if IconButton::new(&icons::CHEVRON_LEFT)
                .size(ButtonSize::SM)
                .tooltip(l10n::text(ui.ctx(), "Back to objects"))
                .show(ui)
                .clicked()
            {
                self.state.showing_symbol_drawings = false;
            }
            ui.strong_label(l10n::text(ui.ctx(), "Drawings by symbol"));
        });
        ui.space_sm();
        let inventory = self.state.symbol_drawings.clone().unwrap_or_default();
        if inventory.is_empty() {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), "No saved drawings"))
                        .weak()
                        .italics(),
                );
            });
            return;
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for mut symbol in inventory {
                    if let Some(sort) = self.state.drawing_sort {
                        symbol.sort(sort);
                    }
                    ui.push_id(&symbol.symbol, |ui| {
                        self.show_symbol_row(ui, &symbol, action);
                    });
                }
            });
    }

    /// One symbol's row and, when expanded, its drawings. The row offers sorting and deleting
    /// while the pointer is over it, and while expanded for input without a pointer.
    fn show_symbol_row(
        &mut self,
        ui: &mut Ui,
        symbol: &SymbolDrawings,
        action: &mut ObjectTreeAction,
    ) {
        let name = symbol.symbol.as_str();
        let expanded = self.state.expanded_symbols.contains(name);
        let row = egui::Rect::from_min_size(
            ui.cursor().min,
            vec2(ui.available_width(), self.config.row_height),
        );
        let offered =
            expanded || ui.rect_contains_pointer(row) || egui::Popup::is_any_open(ui.ctx());
        ui.horizontal(|ui| {
            ui.set_min_height(self.config.row_height);
            let caret = if expanded {
                &icons::CHEVRON_DOWN
            } else {
                &icons::CHEVRON_RIGHT
            };
            if IconButton::new(caret)
                .size(ButtonSize::SM)
                .tooltip(l10n::format_text(
                    ui.ctx(),
                    "Toggle {symbol} drawings",
                    &[("symbol", name.to_owned())],
                ))
                .show(ui)
                .clicked()
            {
                if expanded {
                    self.state.expanded_symbols.remove(name);
                } else {
                    self.state.expanded_symbols.insert(name.to_owned());
                }
            }
            ui.strong_label(name);
            if self.config.show_item_counts {
                ui.label(
                    RichText::new(format!("({})", symbol.drawings.len()))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
            if offered {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if IconButton::new(&icons::TRASH)
                        .size(ButtonSize::SM)
                        .tooltip(l10n::format_text(
                            ui.ctx(),
                            "Delete {symbol} drawings",
                            &[("symbol", name.to_owned())],
                        ))
                        .show(ui)
                        .clicked()
                    {
                        *action = ObjectTreeAction::DeleteSymbolDrawings(name.to_owned());
                    }
                    let sort = IconButton::new(&icons::SORT)
                        .size(ButtonSize::SM)
                        .tooltip(l10n::format_text(
                            ui.ctx(),
                            "Sort {symbol} drawings",
                            &[("symbol", name.to_owned())],
                        ))
                        .show(ui);
                    egui::Popup::menu(&sort).show(|ui| {
                        for (order, title) in [
                            (DrawingSort::Name, "Sort by name"),
                            (DrawingSort::LastModified, "Sort by last modified"),
                        ] {
                            let chosen = self.state.drawing_sort == Some(order);
                            if ui
                                .selectable_label(chosen, l10n::text(ui.ctx(), title))
                                .clicked()
                            {
                                self.state.drawing_sort = (!chosen).then_some(order);
                                ui.close();
                            }
                        }
                    });
                });
            }
        });
        if expanded {
            ui.indent("drawings", |ui| {
                for drawing in &symbol.drawings {
                    ui.horizontal(|ui| {
                        ui.add(
                            crate::ui::drawing_toolbar::icons::get_icon(drawing.tool_type)
                                .as_image_tinted(
                                    vec2(16.0, 16.0),
                                    ui.visuals().widgets.noninteractive.fg_stroke.color,
                                ),
                        );
                        ui.label(drawing.name());
                    });
                }
            });
        }
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
                let ordered = if self.state.visible_ids.is_empty() {
                    source_ids
                } else {
                    self.state.visible_ids.clone()
                };
                self.state.range_select(*id, &ordered);
            }
            ObjectTreeAction::ClearSelection => {
                self.state.clear_selection();
            }
            ObjectTreeAction::SelectAll => {
                self.state.select_all(&source_ids);
            }
            ObjectTreeAction::SelectFolder(path) | ObjectTreeAction::AddFolderToSelection(path) => {
                let members: Vec<_> = sources
                    .iter()
                    .filter(|s| s.is_drawing() && s.folder.starts_with(path))
                    .map(|s| s.id)
                    .collect();
                if matches!(action, ObjectTreeAction::SelectFolder(_)) {
                    self.state.select_all(&members);
                } else if members.iter().all(|id| self.state.is_selected(*id)) {
                    self.state.selected_ids.retain(|id| !members.contains(id));
                } else {
                    self.state.selected_ids.extend(members);
                }
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
    fn selected_drawings_can_be_nested_hidden_locked_and_moved_without_replacing_them() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use egui_kittest::{Harness, kittest::Queryable};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let mut chart = crate::widget::ChartBuilder::new().build();
            for name in ["Support", "Resistance", "Outside"] {
                let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
                drawing.label = Some(name.into());
                drawing.stored_id = Some(name.into());
                drawing.z_order = match name {
                    "Resistance" => 3,
                    "Outside" => 2,
                    _ => 0,
                };
                drawing.chart_points =
                    vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)];
                drawing.completed = true;
                chart.drawings_mut().add_synced_drawing(drawing).unwrap();
            }
            let ids: Vec<_> = chart.drawings().drawings.iter().map(|d| d.id).collect();
            let mut panel = ObjectTreePanel::with_config(ObjectTreeConfig {
                show_data_window: false,
                ..Default::default()
            });
            panel.state_mut().select_all(&ids[..2]);
            let mut h = Harness::builder()
                .allow_missing_glyphs()
                .with_size(vec2(220.0, 600.0))
                .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
                .build_ui_state(
                    |ui, (panel, chart): &mut (ObjectTreePanel, crate::Chart)| {
                        let mut sources: Vec<_> = chart
                            .drawings()
                            .drawings
                            .iter()
                            .map(|d| {
                                let mut item = SourceItem::drawing(d.id, d.tool_type, d.color32());
                                item.label = d.label.clone();
                                item.folder = d.folder.clone();
                                item.visible = d.visible;
                                item.locked = d.locked;
                                item.z_index = d.z_order;
                                item
                            })
                            .collect();
                        let action = panel.show(ui, None, &mut sources);
                        assert!(panel.apply_to_chart(chart, &action, &sources));
                    },
                    (panel, chart),
                );
            h.run();
            h.get_by_label("New folder with selected drawings").click();
            h.run();
            h.event(egui::Event::Text("Analysis / one".into()));
            h.key_press(egui::Key::Enter);
            h.run();
            let caret_right = h.get_by_label("Toggle folder").rect().right();
            let name_left = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Analysis / one" => {
                        Some(text.pos.x)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(
                name_left - caret_right <= 12.0,
                "folder name must follow its expand control"
            );
            h.get_by_label("Analysis / one").click_secondary();
            h.run();
            h.get_by_label("New subfolder with selected drawings")
                .click();
            h.run();
            h.event(egui::Event::Text("Levels".into()));
            h.key_press(egui::Key::Enter);
            h.run();
            let saved = |h: &Harness<'_, (ObjectTreePanel, crate::Chart)>| {
                h.state()
                    .1
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| serde_json::to_value(d).unwrap())
                    .collect::<Vec<_>>()
            };
            h.get_by_label("Resistance").click();
            h.run();
            h.get_by_label("Support").click_modifiers(egui::Modifiers {
                shift: true,
                ..Default::default()
            });
            h.run();
            assert_eq!(h.state().0.state().selection_count(), 2);
            for filter_type in [None, Some(SourceType::Drawing)] {
                h.state_mut().0.state_mut().filter_type = filter_type;
                h.run();
                h.query_all_by_label("Toggle folder")
                    .next()
                    .unwrap()
                    .click();
                h.run();
                assert!(h.query_by_label("Support").is_none());
                h.state_mut().0.state_mut().filter_text = "Support".into();
                h.run();
                h.get_by_label("Support");
                assert!(h.query_by_label("Outside").is_none());
                h.state_mut().0.state_mut().filter_text.clear();
                h.run();
                h.query_all_by_label("Toggle folder")
                    .next()
                    .unwrap()
                    .click();
                h.run();
            }
            h.state_mut().0.state_mut().filter_type = None;
            let nested = saved(&h);
            for (index, name) in ["Support", "Resistance", "Outside"].iter().enumerate() {
                assert_eq!(nested[index]["label"], *name);
                assert_eq!(
                    h.state().1.drawings().drawings[index].stored_id.as_deref(),
                    Some(*name)
                );
                assert_eq!(
                    nested[index]["folder"],
                    if index < 2 {
                        serde_json::json!(["Analysis / one", "Levels"])
                    } else {
                        serde_json::json!([])
                    }
                );
            }
            h.query_all_by_label("Hide folder").next().unwrap().click();
            h.run();
            assert_eq!(
                saved(&h)
                    .iter()
                    .map(|d| d["visible"].as_bool().unwrap())
                    .collect::<Vec<_>>(),
                [false, false, true]
            );
            h.query_all_by_label("Lock folder").next().unwrap().click();
            h.run();
            assert_eq!(
                saved(&h)
                    .iter()
                    .map(|d| d["locked"].as_bool().unwrap())
                    .collect::<Vec<_>>(),
                [true, true, false]
            );
            assert!(h.state_mut().1.drawings_mut().undo());
            assert_eq!(
                saved(&h)
                    .iter()
                    .map(|d| d["locked"].as_bool().unwrap())
                    .collect::<Vec<_>>(),
                [false, false, false]
            );
            assert!(h.state_mut().1.drawings_mut().redo());
            h.run();
            h.get_by_label("Support").click_secondary();
            h.run();
            h.get_by_label("Move to folder").click();
            h.run();
            h.get_by_label("Root").click();
            h.run();
            assert!(
                saved(&h)
                    .iter()
                    .all(|d| d["folder"] == serde_json::json!([]))
            );
            assert!(h.state_mut().1.drawings_mut().undo());
            h.run();
            assert_eq!(saved(&h)[0]["folder"], nested[0]["folder"]);
            assert_eq!(saved(&h)[1]["folder"], nested[1]["folder"]);
            assert_eq!(
                h.state().1.drawings().drawings[0].chart_points,
                [ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)]
            );
            let path = vec!["Analysis / one".into(), "Levels".into()];
            let sources: Vec<_> = ids[..2]
                .iter()
                .map(|&id| SourceItem::drawing(id, DrawingToolType::TrendLine, egui::Color32::RED))
                .collect();
            let (panel, chart) = h.state_mut();
            for action in [
                ObjectTreeAction::MoveToFolder(ids[..2].to_vec(), path.clone()),
                ObjectTreeAction::SetFolderVisible(path.clone(), false),
                ObjectTreeAction::SetFolderLocked(path, true),
            ] {
                assert!(panel.apply_to_chart(chart, &action, &sources));
                assert!(
                    chart.drawings().can_redo(),
                    "unchanged {action:?} cleared redo"
                );
            }
        }
    }

    /// Through the tree: a folder row selects its descendants, drags into another folder and
    /// back out, nests with a new folder, and leaves with its contents in one undo step.
    #[test]
    fn folder_rows_select_nest_delete_and_clean_up() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        type App = (ObjectTreePanel, crate::Chart);
        let mut chart = crate::widget::ChartBuilder::new().build();
        for (name, folder) in [
            ("Line", vec!["Levels"]),
            ("Deep", vec!["Zones", "Inner"]),
            ("Free", vec![]),
        ] {
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.label = Some(name.into());
            drawing.folder = folder.into_iter().map(str::to_owned).collect();
            drawing.chart_points = vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)];
            drawing.completed = true;
            chart.drawings_mut().add_synced_drawing(drawing).unwrap();
        }
        let ids: Vec<_> = chart.drawings().drawings.iter().map(|d| d.id).collect();
        let panel = ObjectTreePanel::with_config(ObjectTreeConfig {
            show_data_window: false,
            ..Default::default()
        });
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(320.0, 600.0))
            .build_ui_state(
                |ui, (panel, chart): &mut App| {
                    let mut sources: Vec<_> = chart
                        .drawings()
                        .drawings
                        .iter()
                        .map(|d| {
                            let mut item = SourceItem::drawing(d.id, d.tool_type, d.color32());
                            item.label = d.label.clone();
                            item.folder = d.folder.clone();
                            item.folder_sync = d.folder_sync_mode();
                            item
                        })
                        .collect();
                    let action = panel.show(ui, None, &mut sources);
                    assert!(panel.apply_to_chart(chart, &action, &sources));
                },
                (panel, chart),
            );
        h.run();
        let folder_of = |h: &Harness<'_, App>, index: usize| {
            h.state().1.drawings().drawings[index].folder.join("/")
        };
        let drag = |h: &mut Harness<'_, App>, from: &str, to: &str| {
            let from = h.get_by_label(from).rect().center();
            h.drag_at(from);
            h.run();
            h.hover_at(from + vec2(0.0, 12.0));
            h.run();
            let to = h.get_by_label(to).rect().center();
            h.hover_at(to);
            h.run();
            h.drop_at(to);
            h.run();
        };

        // No selection means nothing to group: a folder never starts empty.
        assert!(
            h.get_by_label("New folder with selected drawings")
                .accesskit_node()
                .is_disabled()
        );

        // Selecting a folder takes every descendant, here through its nested folder.
        h.get_by_label("Zones").click();
        h.run();
        assert_eq!(
            h.state().1.drawings().selected_ids().collect::<Vec<_>>(),
            [ids[1]]
        );
        h.get_by_label("Levels").click_modifiers(egui::Modifiers {
            ctrl: true,
            ..Default::default()
        });
        h.run();
        assert_eq!(
            h.state().1.drawings().selected_ids().collect::<Vec<_>>(),
            [ids[0], ids[1]]
        );

        // Grouping the selected folders keeps their structure.
        h.get_by_label("New folder with selected drawings").click();
        h.run();
        h.event(egui::Event::Text("All".into()));
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(folder_of(&h, 0), "All/Levels");
        assert_eq!(folder_of(&h, 1), "All/Zones/Inner");
        assert_eq!(folder_of(&h, 2), "");

        // A folder drags out to the root and into another; never into its own descendant.
        drag(&mut h, "Zones", "Levels");
        assert_eq!(folder_of(&h, 1), "All/Levels/Zones/Inner");
        drag(&mut h, "Levels", "Inner");
        assert_eq!(folder_of(&h, 1), "All/Levels/Zones/Inner");
        drag(&mut h, "Inner", "Move to root");
        assert_eq!(folder_of(&h, 1), "Inner");
        assert!(h.query_by_label("Zones").is_none(), "emptied folder stays");
        assert!(h.state_mut().1.drawings_mut().undo());
        h.run();
        assert_eq!(folder_of(&h, 1), "All/Levels/Zones/Inner");
        h.get_by_label("Zones");

        // Deleting from the row or by key removes the descendants in one undo step each.
        let delete: [fn(&mut Harness<'_, App>); 2] = [
            |h| {
                h.get_by_label("All").click_secondary();
                h.run();
                h.get_by_label("Delete folder").click();
                h.run();
            },
            |h| {
                h.get_by_label("All").click();
                h.run();
                h.key_press(egui::Key::Delete);
                h.run();
            },
        ];
        for delete in delete {
            delete(&mut h);
            assert_eq!(h.state().1.drawings().drawings.len(), 1);
            assert!(h.query_by_label("All").is_none());
            assert!(h.state_mut().1.drawings_mut().undo());
            h.run();
            assert_eq!(h.state().1.drawings().drawings.len(), 3);
            assert_eq!(folder_of(&h, 1), "All/Levels/Zones/Inner");
            h.get_by_label("All");
        }
    }

    /// The list of drawings by symbol: reached from the actions menu when the host supplies it,
    /// sorted from a row's controls, which appear under the pointer or when expanded, and deleting
    /// one symbol only asks for that symbol.
    #[test]
    fn symbol_drawings_list_sorts_and_asks_to_delete_one_symbol() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use crate::widget::{Chart, ChartLayout};
        use egui_kittest::{Harness, kittest::Queryable};
        type App = (ObjectTreePanel, Vec<ObjectTreeAction>, Chart);
        let mut chart = crate::widget::ChartBuilder::new().build();
        chart.set_symbol("AAA");
        for name in ["beta", "Alpha"] {
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.label = Some(name.into());
            drawing.chart_points = vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)];
            drawing.completed = true;
            chart.drawings_mut().add_synced_drawing(drawing).unwrap();
        }
        let panel = ObjectTreePanel::with_config(ObjectTreeConfig {
            show_data_window: false,
            ..Default::default()
        });
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(320.0, 600.0))
            .build_ui_state(
                |ui, (panel, actions, chart): &mut App| {
                    let inventory = ChartLayout::default().drawing_inventory(&[&mut *chart]);
                    panel.state_mut().symbol_drawings = Some(inventory);
                    let action = panel.show(ui, None, &mut []);
                    if !action.is_none() {
                        actions.push(action);
                    }
                },
                (panel, Vec::new(), chart),
            );
        h.run();
        let offered = |h: &Harness<'_, App>| h.query_by_label("Delete AAA drawings").is_some();
        let rows = |h: &Harness<'_, App>| {
            let top = |name: &str| h.get_by_label(name).rect().top();
            top("beta") < top("Alpha")
        };
        h.get_by_label("Object actions").click();
        h.run();
        h.get_by_label("Drawings by symbol").click();
        h.run();
        assert_eq!(h.state().1, [ObjectTreeAction::ReadSymbolDrawings]);
        assert!(!offered(&h), "controls wait for the pointer");
        h.hover_at(h.get_by_label("AAA").rect().center());
        h.run();
        assert!(offered(&h), "controls under the pointer");
        h.hover_at(egui::pos2(160.0, 590.0));
        h.run();
        assert!(!offered(&h));

        h.get_by_label("Toggle AAA drawings").click();
        h.run();
        assert!(offered(&h), "controls while expanded");
        assert!(rows(&h), "the order of the chart until sorted");
        h.get_by_label("Sort AAA drawings").click();
        h.run();
        h.get_by_label("Sort by name").click();
        h.run();
        assert_eq!(
            h.state().0.state().drawing_sort,
            Some(crate::config::DrawingSort::Name)
        );
        assert!(!rows(&h), "alphabetical, ignoring case");

        h.get_by_label("Delete AAA drawings").click();
        h.run();
        assert_eq!(
            h.state().1.last(),
            Some(&ObjectTreeAction::DeleteSymbolDrawings("AAA".into()))
        );
        h.get_by_label("Back to objects").click();
        h.run();
        assert!(h.query_by_label("AAA").is_none());
    }

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
