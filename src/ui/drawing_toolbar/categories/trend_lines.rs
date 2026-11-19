//! Trend Lines category for left toolbar.
//!
//! Contains three sub-sections:
//! - LINES: Trend Line, Ray, Info Line, Extended Line, Trend Angle, etc.
//! - CHANNELS: Parallel Channel, Regression Trend, Flat Top/Bottom, Disjoint Channel
//! - PITCHFORKS: Pitchfork, Schiff Pitchfork, Modified Schiff, Inside Pitchfork

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::{Response, Ui};

use super::{DrawingToolbarAction, ToolCategory};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::icons as toolbar_icons;
use crate::ui::drawing_toolbar::submenu_builder::{
    FlyoutRow, flyout_row, row_width, section_header,
};
use crate::ui::drawing_toolbar::{DrawingToolShortcuts, ToolbarState};

/// Trend Lines category implementation
#[derive(Debug)]
pub struct TrendLinesCategory;

impl ToolCategory for TrendLinesCategory {
    fn name(&self) -> &'static str {
        "Lines"
    }

    fn tooltip(&self) -> &'static str {
        "Trend line tools"
    }

    fn icon(&self) -> &'static Icon {
        &icons::TREND_LINE
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::TREND_LINE
    }
}

/// Draw a tool list into the flyout: a header per section, one row per tool with its shortcut
/// and favourite star. A click on a row selects the tool; a click on its star toggles the
/// favourite in `favorites` and selects nothing.
pub fn render_tool_submenu(
    ui: &mut Ui,
    sections: Vec<(&'static str, Vec<DrawingToolType>)>,
    sel_tool: Option<DrawingToolType>,
    favorites: &mut Vec<DrawingToolType>,
) -> DrawingToolbarAction {
    let mut state = ToolbarState {
        sel_tool,
        favorites: std::mem::take(favorites),
        ..Default::default()
    };
    let action =
        render_registered_tool_submenu(ui, None, sections, &mut state, &Default::default());
    *favorites = state.favorites;
    action
}

pub(crate) fn render_registered_tool_submenu(
    ui: &mut Ui,
    category: Option<&str>,
    mut sections: Vec<(&'static str, Vec<DrawingToolType>)>,
    state: &mut ToolbarState,
    registry: &crate::drawings::DrawingToolRegistry,
) -> DrawingToolbarAction {
    for (_, tools) in &mut sections {
        tools.retain(|tool| registry.availability(*tool).visible);
    }
    sections.retain(|(_, tools)| !tools.is_empty());
    let custom: Vec<_> = registry
        .custom_tools()
        .filter(|(id, tool)| {
            category.is_some()
                && tool.category.as_deref() == category
                && registry.custom_availability(id).visible
        })
        .collect();
    let mut action = DrawingToolbarAction::None;
    let shortcuts = DrawingToolShortcuts::default();
    let hint = |ui: &Ui, tool| shortcuts.hint(ui.ctx(), tool);

    // The widest row sets the width, so no label meets the shortcut or the card edge.
    let width = sections
        .iter()
        .flat_map(|(_, tools)| tools)
        .map(|&tool| {
            let hint = hint(ui, tool);
            row_width(
                ui,
                &FlyoutRow {
                    icon: Some(toolbar_icons::get_icon(tool)),
                    label: tool.as_str(),
                    shortcut: hint.as_deref(),
                    star: Some(false),
                    ..Default::default()
                },
            )
        })
        .chain(custom.iter().map(|(id, tool)| {
            row_width(
                ui,
                &FlyoutRow {
                    icon: Some(
                        tool.icon
                            .as_ref()
                            .unwrap_or(toolbar_icons::get_icon(DrawingToolType::Polyline)),
                    ),
                    label: registry.custom_name(id),
                    star: Some(false),
                    ..Default::default()
                },
            )
        }))
        .fold(
            DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_xl,
            f32::max,
        );
    ui.set_min_width(width);

    for (index, (section, tools)) in sections.iter().enumerate() {
        if index > 0 {
            ui.separator();
        }
        section_header(ui, section, width);
        for &tool in tools {
            let hint = hint(ui, tool);
            let enabled = registry.availability(tool).enabled;
            let response = flyout_row(
                ui,
                width,
                &FlyoutRow {
                    icon: Some(toolbar_icons::get_icon(tool)),
                    label: tool.as_str(),
                    shortcut: hint.as_deref(),
                    selected: state.sel_builtin() == Some(tool),
                    star: Some(state.favorites.contains(&tool)),
                    disabled: !enabled,
                    ..Default::default()
                },
            );
            if response.star.as_ref().is_some_and(Response::clicked) {
                match state.favorites.iter().position(|&t| t == tool) {
                    Some(index) => {
                        state.favorites.remove(index);
                    }
                    None => state.favorites.push(tool),
                }
            } else if response.row.clicked() {
                action = if enabled {
                    DrawingToolbarAction::SelectTool(tool)
                } else {
                    DrawingToolbarAction::ToolUnavailable(tool)
                };
            }
        }
    }
    if !custom.is_empty() {
        if !sections.is_empty() {
            ui.separator();
        }
        section_header(ui, "CUSTOM", width);
        for (id, tool) in custom {
            let enabled = registry.custom_availability(id).enabled;
            let response = flyout_row(
                ui,
                width,
                &FlyoutRow {
                    icon: Some(
                        tool.icon
                            .as_ref()
                            .unwrap_or(toolbar_icons::get_icon(DrawingToolType::Polyline)),
                    ),
                    label: registry.custom_name(id),
                    selected: state.sel_custom_tool.as_deref() == Some(id),
                    star: Some(state.custom_favorites.iter().any(|tool| tool == id)),
                    disabled: !enabled,
                    ..Default::default()
                },
            );
            if response.star.as_ref().is_some_and(Response::clicked) {
                match state.custom_favorites.iter().position(|tool| tool == id) {
                    Some(index) => {
                        state.custom_favorites.remove(index);
                    }
                    None => state.custom_favorites.push(id.to_owned()),
                }
            } else if response.row.clicked() {
                action = if enabled {
                    DrawingToolbarAction::SelectCustomTool(id.to_owned())
                } else {
                    DrawingToolbarAction::CustomToolUnavailable(id.to_owned())
                };
            }
        }
    }
    action
}
