//! Cursors category for left toolbar.
//!
//! Contains cursor/pointer tools:
//! - Cross (crosshair cursor)
//! - Dot (simple dot cursor)
//! - Arrow (pointer arrow)
//! - Eraser

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::Ui;

use super::{DrawingToolbarAction, ToolCategory};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::submenu_builder::{FlyoutRow, flyout_row, row_width};
use crate::ui::drawing_toolbar::toolbar::CursorStateData;

/// Cursor tool types (not drawing tools, but cursor modes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Cross,
    Dot,
    Arrow,
    Demonstration,
    Eraser,
}

impl CursorType {
    pub fn all() -> &'static [CursorType] {
        &[
            CursorType::Cross,
            CursorType::Dot,
            CursorType::Arrow,
            CursorType::Demonstration,
            CursorType::Eraser,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            CursorType::Cross => "Cross",
            CursorType::Dot => "Dot",
            CursorType::Arrow => "Arrow",
            CursorType::Demonstration => "Demonstration",
            CursorType::Eraser => "Eraser",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            CursorType::Cross => &icons::CROSSHAIR,
            CursorType::Dot => &icons::DOT,
            CursorType::Arrow => &icons::ARROW,
            CursorType::Demonstration => &icons::DEMONSTRATION,
            CursorType::Eraser => &icons::ERASER,
        }
    }

    pub fn shortcut(&self) -> Option<&'static str> {
        match self {
            CursorType::Cross => None,
            CursorType::Dot => None,
            CursorType::Arrow => None,
            CursorType::Demonstration => None,
            CursorType::Eraser => None,
        }
    }
}

/// Cursors category implementation
pub struct CursorsCategory;

impl ToolCategory for CursorsCategory {
    fn name(&self) -> &'static str {
        "Cursors"
    }

    fn tooltip(&self) -> &'static str {
        "Cursors"
    }

    fn icon(&self) -> &'static Icon {
        &icons::CROSSHAIR
    }

    fn curr_tool_icon(&self, _sel: Option<DrawingToolType>) -> &'static Icon {
        // Cursors don't have drawing tools, always show Cross
        &icons::CROSSHAIR
    }

    fn all_tools(&self) -> Vec<DrawingToolType> {
        // Cursors category has no drawing tools
        vec![]
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        // Cursors don't use drawing tools
        vec![]
    }

    fn render_submenu(
        &self,
        ui: &mut Ui,
        _sel_tool: Option<DrawingToolType>,
        _favorites: &mut Vec<DrawingToolType>,
    ) -> DrawingToolbarAction {
        // The toolbar stores its cursor state for the frame before it shows this menu.
        let Some(state) = ui
            .ctx()
            .data(|d| d.get_temp::<CursorStateData>(CursorStateData::id()))
        else {
            return DrawingToolbarAction::None;
        };
        let values_label = "Values tooltip on long press";

        let rows: Vec<FlyoutRow> = CursorType::all()
            .iter()
            .map(|cursor| FlyoutRow {
                icon: Some(cursor.icon()),
                label: cursor.name(),
                selected: state.is_active(*cursor),
                check: state.is_active(*cursor),
                ..Default::default()
            })
            .collect();
        let values_row = FlyoutRow {
            label: values_label,
            check: state.values_tooltip,
            ..Default::default()
        };
        let width = rows
            .iter()
            .chain([&values_row])
            .map(|row| row_width(ui, row))
            .fold(
                DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_xl,
                f32::max,
            );
        ui.set_min_width(width);

        let mut action = DrawingToolbarAction::None;
        for (cursor, row) in CursorType::all().iter().zip(&rows) {
            if flyout_row(ui, width, row).row.clicked() {
                action = match cursor {
                    CursorType::Eraser => DrawingToolbarAction::ToggleEraserMode,
                    _ => DrawingToolbarAction::SetCursorType(*cursor),
                };
            }
        }
        ui.separator();
        if flyout_row(ui, width, &values_row).row.clicked() {
            action = DrawingToolbarAction::ToggleValuesTooltip;
        }
        action
    }
}
