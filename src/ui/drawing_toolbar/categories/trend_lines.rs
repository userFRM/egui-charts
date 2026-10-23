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
use crate::ui::drawing_toolbar::DrawingToolShortcuts;
use crate::ui::drawing_toolbar::icons as toolbar_icons;
use crate::ui::drawing_toolbar::submenu_builder::{
    FlyoutRow, flyout_row, row_width, section_header,
};

/// Trend Lines category implementation
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
            let response = flyout_row(
                ui,
                width,
                &FlyoutRow {
                    icon: Some(toolbar_icons::get_icon(tool)),
                    label: tool.as_str(),
                    shortcut: hint.as_deref(),
                    selected: sel_tool == Some(tool),
                    star: Some(favorites.contains(&tool)),
                    ..Default::default()
                },
            );
            if response.star.as_ref().is_some_and(Response::clicked) {
                match favorites.iter().position(|&t| t == tool) {
                    Some(index) => {
                        favorites.remove(index);
                    }
                    None => favorites.push(tool),
                }
            } else if response.row.clicked() {
                action = DrawingToolbarAction::SelectTool(tool);
            }
        }
    }
    action
}
