//! Drawing interaction mode.
//!
//! Defines how mouse/pointer input is translated into point capture for each
//! drawing tool type. The [`DrawingInteractionMode`] determines the UX flow
//! for creating a drawing, and the [`DrawingManager`](crate::drawings::DrawingManager)
//! uses it to decide when a drawing is considered complete.

/// How a drawing tool captures points from mouse/pointer input.
///
/// Every [`DrawingToolType`](super::DrawingToolType) maps to exactly one
/// interaction mode via
/// [`DrawingToolType::interaction_mode()`](super::DrawingToolType::interaction_mode).
/// The mode governs the full creation flow: how many clicks are needed, whether
/// dragging is involved, and when the drawing is marked as complete.
///
/// # Example
///
/// ```ignore
/// use egui_charts::drawings::{DrawingToolType, DrawingInteractionMode};
///
/// assert_eq!(
///     DrawingToolType::TrendLine.interaction_mode(),
///     DrawingInteractionMode::ClickClick,
/// );
/// assert_eq!(
///     DrawingToolType::Rect.interaction_mode(),
///     DrawingInteractionMode::DragToDraw,
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawingInteractionMode {
    /// Click once to place (e.g., `HorizontalLine`, `TextLabel`, arrow markers).
    ///
    /// The drawing is created and completed with a single click. Only one
    /// [`ChartPoint`](crate::drawings::ChartPoint) is captured.
    SingleClick,
    /// Click to set the first point, move, then click to set the second point
    /// (e.g., `TrendLine`, `Ray`, `ExtendedLine`).
    ///
    /// A preview line follows the cursor between the first and second clicks.
    ClickClick,
    /// Press the mouse button to set the first point, drag to the second point,
    /// and release to complete (e.g., `Rect`, `Circle`, `FibonacciRetracement`).
    DragToDraw,
    /// Hold the mouse button and draw a freehand path; release to complete
    /// (e.g., `Brush`, `Highlighter`).
    ///
    /// Points are continuously appended while the button is held.
    ContinuousDraw,
    /// Click to add points one at a time; double-click or press Enter to finish
    /// (e.g., `Polyline`, `XABCDPattern`, `ElliottImpulse`).
    ///
    /// The number of required points varies by tool (see
    /// [`DrawingToolType::required_points()`](super::DrawingToolType::required_points)).
    /// Some tools (like `Polyline`) have no fixed limit.
    MultiPoint,
    /// Not a drawing tool -- cursor/interaction modes only
    /// (e.g., `CrossCursor`, `Eraser`).
    ///
    /// These tools modify the cursor or provide utility functions but do not
    /// create [`Drawing`](crate::drawings::Drawing) objects.
    NotDrawing,
}
