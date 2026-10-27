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
    /// A drawing tool was selected
    SelectTool(DrawingToolType),
    /// Set cursor type (Cross, Dot, Arrow, Magic)
    SetCursorType(CursorType),
    /// Magnet mode toggled
    ToggleMagnet,
    /// Stay-in-drawing mode toggled
    ToggleStayInDrawingMode,
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
    /// Include or exclude visible study plots from drawing magnet targets.
    /// Hosts set `DrawingManagerOptions::snap_to_indicators` to this value.
    SetSnapToIndicators(bool),
    /// Zoom in (save current state to history)
    ZoomIn,
    /// Zoom out (restore previous state from history)
    ZoomOut,
    /// Toggle values tooltip on long press
    ToggleValuesTooltip,
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
