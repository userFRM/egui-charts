//! Actions that can be returned from drawing toolbar interactions.

use super::categories::cursors::CursorType;
use super::state::MagnetType;
use crate::drawings::DrawingToolType;

/// Actions returned from toolbar interactions
///
/// # Examples
///
/// ```
/// use egui_open_trading_charts_rs::ui::drawing_toolbar::DrawingToolbarAction;
/// use egui_open_trading_charts_rs::drawing::DrawingToolType;
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
pub enum DrawingToolbarAction {
    /// No action taken
    None,
    /// A drawing tool was selected
    SelectTool(DrawingToolType),
    /// Selection cleared (cursor mode)
    ClearSelection,
    /// Set cursor type (Cross, Dot, Arrow, Magic)
    SetCursorType(CursorType),
    /// Magnet mode toggled
    ToggleMagnet,
    /// Stay-in-drawing mode toggled
    ToggleStayInDrawingMode,
    /// Tool added to favorites
    AddFavorite(DrawingToolType),
    /// Tool removed from favorites
    RemoveFavorite(DrawingToolType),
    /// Settings requested
    OpenSettings,
    /// Clear all drawings requested
    ClearAllDrawings,
    /// Drawing color changed (RGBA)
    ColorChanged([u8; 4]),
    /// Eraser mode toggled
    ToggleEraserMode,
    /// Hide all drawings
    HideAllDrawings,
    /// Hide all indicators
    HideAllIndicators,
    /// Hide all positions
    HideAllPoss,
    /// Hide all orders
    HideAllOrders,
    /// Remove all drawings
    RemoveAllDrawings,
    /// Remove all indicators
    RemoveAllIndicators,
    /// Remove all studies (indicators + overlays)
    RemoveAllStudies,
    /// Change magnet type (Weak/Strong/OHLC)
    SetMagnetType(MagnetType),
    /// Zoom in (save current state to history)
    ZoomIn,
    /// Zoom out (restore previous state from history)
    ZoomOut,
    /// Toggle values tooltip on long press
    ToggleValuesTooltip,
    /// Toggle favorites toolbar visibility
    ToggleFavoritesToolbar,
    /// Lock all drawings
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
    /// Delete a saved drawing template
    DeleteTemplate {
        /// Template ID to delete
        template_id: String,
    },
    /// Open the template management menu
    OpenTemplateMenu,
    /// Insert an emoji/icon as a text annotation
    InsertEmoji {
        /// Icon reference to insert
        icon_name: &'static str,
    },
}
