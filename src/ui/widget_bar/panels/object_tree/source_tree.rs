//! Source tree rendering
//!
//! Renders the hierarchical tree of indicators and drawings.

use egui::{RichText, ScrollArea, Ui};

use egui::Vec2;

use super::actions::ObjectTreeAction;
use super::config::ObjectTreeConfig;
use super::item_row::render_item_row;
use super::state::ObjectTreeState;
use super::types::{SourceItem, SourceType};
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;

/// Render the source tree section
pub fn render_source_tree(
    ui: &mut Ui,
    sources: &mut [SourceItem],
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    // Filter sources
    let filter_lower = state.filter_text.to_lowercase();

    ScrollArea::vertical()
        .max_height(config.max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if config.group_by_type {
                // Grouped view: Indicators, Drawings, Templates
                action = render_grouped_tree(ui, sources, config, state, &filter_lower);
            } else {
                // Flat view: all items in z-order
                action = render_flat_tree(ui, sources, config, state, &filter_lower);
            }
        });

    action
}

/// Render tree grouped by type
fn render_grouped_tree(
    ui: &mut Ui,
    sources: &mut [SourceItem],
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    filter: &str,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    // Count items per type
    let indicator_count = sources
        .iter()
        .filter(|s| s.source_type == SourceType::Indicator && matches_filter(s, filter, state))
        .count();
    let drawing_count = sources
        .iter()
        .filter(|s| s.source_type == SourceType::Drawing && matches_filter(s, filter, state))
        .count();
    let template_count = sources
        .iter()
        .filter(|s| s.source_type == SourceType::Template && matches_filter(s, filter, state))
        .count();

    // Indicators group
    if indicator_count > 0 {
        let group_action = render_group(
            ui,
            "Indicators",
            SourceType::Indicator,
            indicator_count,
            sources,
            config,
            state,
            filter,
        );
        if !group_action.is_none() {
            action = group_action;
        }
    }

    // Drawings group
    if drawing_count > 0 {
        let group_action = render_group(
            ui,
            "Drawings",
            SourceType::Drawing,
            drawing_count,
            sources,
            config,
            state,
            filter,
        );
        if !group_action.is_none() {
            action = group_action;
        }
    }

    // Templates group
    if template_count > 0 {
        let group_action = render_group(
            ui,
            "Templates",
            SourceType::Template,
            template_count,
            sources,
            config,
            state,
            filter,
        );
        if !group_action.is_none() {
            action = group_action;
        }
    }

    // Empty state
    if indicator_count == 0 && drawing_count == 0 && template_count == 0 {
        render_empty_state(ui, state.has_filter());
    }

    action
}

/// Render a group with header and items
fn render_group(
    ui: &mut Ui,
    name: &str,
    source_type: SourceType,
    count: usize,
    sources: &mut [SourceItem],
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    filter: &str,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;
    let is_expanded = state.is_group_expanded(name);

    // Group header
    ui.horizontal(|ui| {
        // Expand/collapse button
        let icon = if is_expanded { "v" } else { ">" };
        if ui.small_button(icon).clicked() {
            state.toggle_group(name);
        }

        // Group icon and name
        let icon_rect = ui
            .allocate_space(Vec2::splat(DESIGN_TOKENS.sizing.icon_sm))
            .1;
        source_type
            .icon()
            .as_image(Vec2::splat(DESIGN_TOKENS.sizing.icon_sm))
            .paint_at(ui, icon_rect);
        ui.strong_label(name);

        // Item count badge
        if config.show_item_counts {
            ui.hint_label(format!("({count})"));
        }
    });

    // Group content
    if is_expanded {
        ui.indent(name, |ui| {
            // Sort by z-index for drawings
            let items: Vec<_> = sources
                .iter_mut()
                .filter(|s| s.source_type == source_type && matches_filter(s, filter, state))
                .collect();

            for item in items {
                let item_action = render_item_row(ui, item, config, state, 0);
                if !item_action.is_none() {
                    action = item_action;
                }
            }
        });
    }

    action
}

/// Render flat list (no groups)
fn render_flat_tree(
    ui: &mut Ui,
    sources: &mut [SourceItem],
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    filter: &str,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    let visible_items: Vec<_> = sources
        .iter_mut()
        .filter(|s| matches_filter(s, filter, state))
        .collect();

    if visible_items.is_empty() {
        render_empty_state(ui, state.has_filter());
        return action;
    }

    for item in visible_items {
        let item_action = render_item_row(ui, item, config, state, 0);
        if !item_action.is_none() {
            action = item_action;
        }
    }

    action
}

/// Check if item matches current filter
fn matches_filter(item: &SourceItem, filter: &str, state: &ObjectTreeState) -> bool {
    // Type filter
    if let Some(filter_type) = state.filter_type
        && item.source_type != filter_type
    {
        return false;
    }

    // Hidden objects filter
    if !item.visible {
        // Could add a config option for this
        // For now, always show
    }

    // Text filter
    if !filter.is_empty() {
        let name_matches = item.name.to_lowercase().contains(filter);
        let label_matches = item
            .label
            .as_ref()
            .map(|l| l.to_lowercase().contains(filter))
            .unwrap_or(false);
        let params_matches = item
            .parameters
            .as_ref()
            .map(|p| p.to_lowercase().contains(filter))
            .unwrap_or(false);

        if !name_matches && !label_matches && !params_matches {
            return false;
        }
    }

    true
}

/// Render empty state message
fn render_empty_state(ui: &mut Ui, has_filter: bool) {
    ui.space_xl();
    ui.vertical_centered(|ui| {
        if has_filter {
            ui.label(RichText::new("No matching objects").weak().italics());
            ui.hint_label("Try a different search");
        } else {
            ui.label(RichText::new("No objects").weak().italics());
            ui.hint_label("Add indicators or drawings to see them here");
        }
    });
    ui.space_xl();
}
