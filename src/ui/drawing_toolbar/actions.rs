//! Actions that can be returned from drawing toolbar interactions.

use super::categories::cursors::CursorType;
use super::state::MagnetType;
use crate::drawings::DrawingToolType;

/// Actions returned from toolbar interactions
///
/// # Examples
///
/// ```
/// use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
/// use egui_charts::drawings::DrawingToolType;
///
/// let action = DrawingToolbarAction::SelectTool(DrawingToolType::TrendLine);
/// match action {
///     DrawingToolbarAction::SelectTool(tool) => {
///         println!("Selected tool: {:?}", tool);
///     }
///     _ => {}
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DrawingToolbarAction {
    /// No action taken
    None,
    /// Show or hide the left tools. Hosts save `ToolbarState::preferences` and omit the
    /// containing side panel when hidden, keeping a restore control outside that panel.
    /// `show_with_action` still renders floating favourites without reserving side-panel space.
    SetToolbarVisible(bool),
    /// Save whether subsequent bulk removal includes locked drawings.
    SetLockedDrawingRemoval(bool),
    /// Resolved bulk removal after the host calls `RemoveMenu::request` and, if needed,
    /// `RemoveMenu::show_confirmation`. Apply both groups in one chart history operation.
    RemoveDrawings {
        /// Include locked drawings only after the user explicitly chooses it.
        include_locked: bool,
        /// Also remove editable studies, leaving price series and host account data intact.
        include_indicators: bool,
    },
    /// A drawing tool was selected
    SelectTool(DrawingToolType),
    /// The user clicked a drawing tool that the host disabled through
    /// [`ToolAvailability::enabled`](crate::drawings::ToolAvailability::enabled). The toolbar
    /// selects nothing, so the host can tell the user why the tool is unavailable.
    ToolUnavailable(DrawingToolType),
    /// A registered tool was chosen by id in `DrawingToolbar::registry`. Start construction
    /// with [`DrawingManager::set_custom_tool`](crate::drawings::DrawingManager::set_custom_tool).
    SelectCustomTool(String),
    /// A registered tool disabled by the host was clicked; the toolbar selects nothing.
    CustomToolUnavailable(String),
    /// Set cursor type (Cross, Dot, Arrow, Magic)
    SetCursorType(CursorType),
    /// Magnet mode toggled, from a click on the magnet or its Off row
    ToggleMagnet,
    /// Stay-in-drawing mode toggled
    ToggleStayInDrawingMode,
    /// Clear all drawings requested
    ClearAllDrawings,
    /// Drawing color changed (RGBA)
    ColorChanged([u8; 4]),
    /// Eraser mode toggled
    ToggleEraserMode,
    /// Hide or show the drawing group, keeping each drawing's own visibility. Apply the
    /// requested `ToolbarState::hide_drawings` with
    /// [`DrawingManager::set_hidden`](crate::drawings::DrawingManager::set_hidden).
    HideAllDrawings,
    /// Hide or show the study group, keeping each study's own visibility. Apply the requested
    /// `ToolbarState::hide_indicators` with
    /// [`Chart::set_indicators_hidden`](crate::widget::Chart::set_indicators_hidden).
    HideAllIndicators,
    /// Hide or show positions. Apply the requested `ToolbarState::hide_positions` with
    /// [`Trading::apply_hide_action`](crate::ui::trading::Trading::apply_hide_action).
    HideAllPoss,
    /// Hide or show working orders. Apply the requested `ToolbarState::hide_orders` with
    /// [`Trading::apply_hide_action`](crate::ui::trading::Trading::apply_hide_action).
    HideAllOrders,
    /// Hide or show execution marks. Apply the requested `ToolbarState::hide_executions` with
    /// [`Trading::apply_hide_action`](crate::ui::trading::Trading::apply_hide_action).
    HideAllExecutions,
    /// Remove all drawings
    RemoveAllDrawings,
    /// Remove all indicators
    RemoveAllIndicators,
    /// Remove all studies (indicators + overlays)
    RemoveAllStudies,
    /// Hide drawings, indicators, positions, orders and executions at once. A host applies it
    /// as it applies `HideAllDrawings` and `HideAllIndicators` with the visibility selected in
    /// `ToolbarState`, and passes it to
    /// [`Trading::apply_hide_action`](crate::ui::trading::Trading::apply_hide_action) for the
    /// trading groups, in one undo step.
    HideAll,
    /// Remove all drawings and all indicators at once. A host applies it as it applies
    /// `RemoveAllDrawings` and `RemoveAllIndicators`, in one undo step.
    RemoveDrawingsAndIndicators,
    /// Change magnet type (Weak/Strong)
    SetMagnetType(MagnetType),
    /// Include or exclude visible study plots from drawing magnet targets.
    /// Hosts set `DrawingManagerOptions::snap_to_indicators` to this value.
    SetSnapToIndicators(bool),
    /// Zoom in (save current state to history)
    ZoomIn,
    /// Zoom out (restore previous state from history)
    ZoomOut,
    /// Toggle values tooltip on long press
    ToggleValuesTooltip,
    /// Switch close-only crosshair snapping on or off, from the cursor menu. Hosts set
    /// [`CrosshairMode::MagnetClose`](crate::config::CrosshairMode::MagnetClose) while it is on,
    /// and the mode their magnet implies when it is off.
    SetCrosshairCloseSnap(bool),
    /// Lock or unlock every drawing as a group, keeping each drawing's own lock. Apply the
    /// requested `ToolbarState::lock_all` with
    /// [`DrawingManager::set_locked`](crate::drawings::DrawingManager::set_locked).
    LockAllDrawings,
    /// Save current drawing properties as a template
    SaveTemplate {
        /// Template name provided by the user
        name: String,
    },
    /// Load a previously saved drawing template
    LoadTemplate {
        /// Template ID to load
        template_id: String,
    },
    /// Open the template name input.
    OpenTemplateMenu,
    /// Open the host's drawing template catalog for loading and deletion.
    ManageTemplates,
    /// Place a bundled icon, emoji or sticker at its independent pixel size.
    InsertGlyph {
        /// Bundled glyph id or Unicode emoji.
        icon_name: &'static str,
        /// Size in logical pixels, saved as `Drawing::font_size`.
        size: f32,
    },
    /// Legacy host intent for an emoji at the configured default size.
    InsertEmoji {
        /// The icon or emoji to place
        icon_name: &'static str,
    },
}
