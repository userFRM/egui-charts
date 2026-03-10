//! Drawing manager -- thin coordinator that delegates to specialized services.
//!
//! `DrawingManager` is the primary entry point for managing drawings on a
//! chart. It owns the drawing list, delegates selection to
//! `SelectionService`, undo/redo to `HistoryService`, snapping to
//! `SnapService`, and handle manipulation to `HandleService`.
//!
//! At the start of each frame, call [`DrawingManager::sync_from_app_state`] to
//! synchronize the active tool, magnet mode, and selection from the central
//! application state. Then call [`DrawingManager::update_all_screen_coords`] to
//! recompute screen positions from chart coordinates before rendering.

use crate::drawings::domain::{Drawing, DrawingToolType, HandlePos};
use crate::drawings::services::{
    HandleConfig, HandleService, HistoryService, SelectionService, SnapOptions, SnapService,
    SnapTargets,
};
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect};
use std::sync::{Arc, Mutex};

/// Actions queued for backend/cloud synchronization.
///
/// These are produced by [`DrawingManager`] operations (create, update, delete)
/// and consumed by the platform integration layer to persist changes to a
/// remote backend or local storage.
#[derive(Debug, Clone)]
pub enum DrawingSyncAction {
    /// A new drawing was created and should be saved. Carries the drawing ID.
    SaveDrawing(usize),
    /// An existing drawing was modified (moved, resized, styled). Carries the drawing ID.
    UpdateDrawing(usize),
    /// A drawing was deleted. Carries the stored drawing ID as a string.
    Delete(String),
    /// A batch of drawings should be restored from backend storage.
    RestoreDrawings(Vec<crate::drawings::persistence::StoredDrawing>),
}

/// Snapshot of drawing-related state from the central application state.
///
/// This struct is passed to [`DrawingManager::sync_from_app_state`] at the start
/// of each frame to ensure the manager stays in sync with toolbar selections,
/// keyboard shortcuts, and other UI actions that modify drawing state.
#[derive(Debug, Clone, Default)]
pub struct DrawingState {
    /// Currently selected drawing tool. `None` means selection/pointer mode.
    pub active_tool: Option<DrawingToolType>,
    /// ID of the currently selected (highlighted) drawing on the chart.
    pub sel_drawing: Option<usize>,
    /// Whether magnet mode is enabled (snap to OHLC prices and existing drawing points).
    pub magnet_mode: bool,
    /// Whether to stay in drawing mode after completing a drawing (lock mode).
    pub stay_in_drawing_mode: bool,
    /// Current default drawing color as RGBA bytes.
    pub curr_color: [u8; 4],
    /// Whether eraser mode is active (click to delete drawings).
    pub eraser_mode: bool,
}

/// Configuration options for the [`DrawingManager`].
///
/// Controls default colors, snapping behavior, magnet mode thresholds, and
/// selection handle appearance.
#[derive(Clone, Debug)]
pub struct DrawingManagerOptions {
    /// Default color for new drawings as RGBA bytes.
    pub default_color: [u8; 4],
    /// Enable snap-to-price (Y-axis snapping to OHLC levels).
    pub snap_to_price: bool,
    /// Enable snap-to-time (X-axis snapping to candle timestamps).
    pub snap_to_time: bool,
    /// Snap distance threshold in screen pixels.
    pub snap_distance: f32,
    /// Enable magnet mode (snap to existing drawing anchor points).
    pub magnet_mode: bool,
    /// Magnet snap distance threshold in screen pixels.
    pub magnet_distance: f32,
    /// Configuration for selection handle appearance and hit testing.
    pub handle_config: HandleConfig,
}

impl Default for DrawingManagerOptions {
    fn default() -> Self {
        Self {
            default_color: [41, 98, 255, 255],
            snap_to_price: true,
            snap_to_time: true,
            snap_distance: DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs,
            magnet_mode: false,
            magnet_distance: DESIGN_TOKENS.sizing.drawing.magnet_distance,
            handle_config: HandleConfig::default(),
        }
    }
}

/// Central coordinator for chart drawings.
///
/// `DrawingManager` owns the list of completed drawings and the in-progress
/// drawing, and delegates business logic to specialized services:
///
/// - **Selection** via [`SelectionService`]
/// - **Undo/redo** via [`HistoryService`]
/// - **Snapping** via [`SnapService`]
/// - **Handle manipulation** via [`HandleService`]
///
/// # Frame loop integration
///
/// ```ignore
/// // 1. Sync state from app
/// manager.sync_from_app_state(&drawing_state);
///
/// // 2. Update screen coordinates from chart coordinates
/// manager.update_all_screen_coords(bar_to_x, price_to_y);
///
/// // 3. Handle user input (clicks, drags, keyboard)
/// // ... (start_drawing_with_coords, add_point_with_coords, etc.)
///
/// // 4. Render
/// manager.render_all(&painter, price_rect);
/// ```
pub struct DrawingManager {
    /// All completed drawings on the chart.
    pub drawings: Vec<Drawing>,
    /// Currently active drawing tool, or `None` for selection/pointer mode.
    pub active_tool: Option<DrawingToolType>,
    /// The drawing currently being created (not yet completed).
    pub curr_drawing: Option<Drawing>,
    /// Next available drawing ID (monotonically increasing).
    next_id: usize,

    // Services
    selection: SelectionService,
    history: HistoryService,
    snap_service: SnapService,

    /// Manager configuration (colors, snapping, handles).
    pub options: DrawingManagerOptions,

    /// Currently dragged handle, if any: `(drawing_id, handle_position)`.
    pub dragging_handle: Option<(usize, HandlePos)>,
    /// Snapshot of the drawing state before the current drag began (for undo).
    drag_old_state: Option<Drawing>,

    /// Whether to stay in drawing mode after completing a drawing.
    pub stay_in_drawing_mode: bool,
    /// Current chart timeframe string (e.g., `"1D"`, `"1h"`), used for
    /// per-timeframe drawing visibility filtering.
    pub curr_timeframe: String,
    /// Whether eraser mode is active.
    pub eraser_mode: bool,
    /// Drawing ID currently hovered by the eraser cursor (for visual feedback).
    pub eraser_hover_drawing: Option<usize>,

    /// Queue of sync actions for backend/cloud persistence.
    pub pending_sync: Arc<Mutex<Vec<DrawingSyncAction>>>,
}

impl Default for DrawingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingManager {
    /// Creates a new `DrawingManager` with default options and empty drawing list.
    pub fn new() -> Self {
        Self {
            drawings: Vec::new(),
            active_tool: None,
            curr_drawing: None,
            next_id: 0,
            selection: SelectionService::new(),
            history: HistoryService::new(),
            snap_service: SnapService::new(),
            options: DrawingManagerOptions::default(),
            dragging_handle: None,
            drag_old_state: None,
            stay_in_drawing_mode: false,
            curr_timeframe: String::from("1D"),
            eraser_mode: false,
            eraser_hover_drawing: None,
            pending_sync: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Sync drawing manager state from central AppState
    /// Called at the start of each frame to ensure consistency
    pub fn sync_from_app_state(&mut self, drawing_state: &DrawingState) {
        self.active_tool = drawing_state.active_tool;
        self.options.magnet_mode = drawing_state.magnet_mode;
        self.options.default_color = drawing_state.curr_color;
        self.stay_in_drawing_mode = drawing_state.stay_in_drawing_mode;
        self.eraser_mode = drawing_state.eraser_mode;

        // Sync selection
        match (drawing_state.sel_drawing, self.selection.primary()) {
            (Some(id), current) if current != Some(id) => {
                self.selection.select(id);
            }
            (None, Some(_)) => {
                self.selection.deselect_all();
            }
            _ => {} // Already in sync
        }
    }

    // === Selection delegation ===

    /// Returns the ID of the currently selected (primary) drawing, if any.
    pub fn sel_drawing(&self) -> Option<usize> {
        self.selection.primary()
    }

    /// Selects the drawing with the given ID (deselects all others).
    pub fn select(&mut self, id: usize) {
        self.selection.select(id);
    }

    /// Deselects all drawings.
    pub fn deselect(&mut self) {
        self.selection.deselect_all();
    }

    /// Hit-tests at `point` and selects the topmost drawing found.
    ///
    /// Returns `true` if a drawing was selected, `false` if the click hit empty space
    /// (which deselects all drawings).
    pub fn select_at(&mut self, point: Pos2) -> bool {
        if let Some(id) = self.hit_test(point) {
            self.selection.select(id);
            true
        } else {
            self.selection.deselect_all();
            false
        }
    }

    // === History delegation ===

    /// Undoes the last drawing operation. Returns `true` if an operation was undone.
    pub fn undo(&mut self) -> bool {
        self.history.undo(&mut self.drawings).is_some()
    }

    /// Redoes the last undone operation. Returns `true` if an operation was redone.
    pub fn redo(&mut self) -> bool {
        self.history.redo(&mut self.drawings).is_some()
    }

    /// Returns `true` if there are operations available to undo.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Returns `true` if there are operations available to redo.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    // === Snap delegation ===

    /// Rebuilds the internal [`SnapService`] from the current manager options.
    /// Call this after changing snap-related options.
    pub fn update_snap_options(&mut self) {
        self.snap_service = SnapService::with_options(SnapOptions {
            snap_to_price: self.options.snap_to_price,
            snap_to_time: self.options.snap_to_time,
            snap_distance: self.options.snap_distance,
            magnet_mode: self.options.magnet_mode,
            magnet_distance: self.options.magnet_distance,
        });
    }

    /// Applies snap behavior to a screen point, returning the snapped position.
    ///
    /// Snaps to price levels, time markers, and existing drawing anchor points
    /// depending on the current snap/magnet configuration.
    pub fn snap_point(&self, point: Pos2, drawings: &[Drawing]) -> Pos2 {
        let targets = SnapTargets {
            prices: vec![], // Would be populated from chart
            times: vec![],
            drawing_points: drawings.iter().flat_map(|d| d.points.clone()).collect(),
        };
        self.snap_service.snap_point(point, &targets)
    }

    // === Handle delegation ===

    /// Returns the selection handle positions for a drawing (tool-type-aware).
    pub fn get_handles(&self, drawing: &Drawing) -> Vec<(HandlePos, Pos2)> {
        HandleService::get_handles(drawing)
    }

    /// Hit-tests a point against the handles of a drawing.
    ///
    /// Returns the [`HandlePos`] if a handle was hit, `None` otherwise.
    pub fn hit_test_handle(&self, drawing: &Drawing, point: Pos2) -> Option<HandlePos> {
        HandleService::hit_test_handle(drawing, point, self.options.handle_config.size)
    }

    /// Begins a handle drag operation, saving the drawing's current state for undo.
    pub fn start_drag_handle(&mut self, drawing_id: usize, handle: HandlePos) {
        if let Some(drawing) = self.drawings.iter().find(|d| d.id == drawing_id) {
            self.dragging_handle = Some((drawing_id, handle));
            self.drag_old_state = Some(drawing.clone());
        }
    }

    /// Updates the position of the currently dragged handle to `new_pos`,
    /// applying snap behavior and coordinate conversion.
    pub fn update_drag_handle<F, G>(&mut self, new_pos: Pos2, x_to_bar: F, y_to_price: G)
    where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if let Some((id, handle)) = self.dragging_handle {
            // Compute snapped position before mutable borrow
            let snapped = self.snap_point(new_pos, &[]);
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id) {
                HandleService::update_handle(drawing, handle, snapped, x_to_bar, y_to_price);
            }
        }
    }

    /// Ends the current handle drag operation, pushing a modify command to the
    /// undo history and queuing a cloud sync update.
    pub fn end_drag_handle(&mut self) {
        if let Some(old_state) = self.drag_old_state.take() {
            let drawing_id = old_state.id;
            self.history.push_modify(old_state.id, old_state);

            // Queue cloud sync for the updated drawing
            if let Ok(mut sync) = self.pending_sync.lock() {
                sync.push(DrawingSyncAction::UpdateDrawing(drawing_id));
            }
        }
        self.dragging_handle = None;
    }

    /// Renders selection handles for a drawing, highlighting the currently
    /// dragged handle (if any).
    pub fn render_handles(&self, painter: &egui::Painter, drawing: &Drawing) {
        let dragging = self
            .dragging_handle
            .filter(|(id, _)| *id == drawing.id)
            .map(|(_, h)| h);
        HandleService::render_handles(painter, drawing, &self.options.handle_config, dragging);
    }

    // === Tool management ===

    /// Sets the active drawing tool. Pass `None` to exit drawing mode and
    /// return to selection/pointer mode. Cancels any in-progress drawing.
    pub fn set_active_tool(&mut self, tool: Option<DrawingToolType>) {
        self.active_tool = tool;
        if tool.is_some() {
            self.curr_drawing = None;
        }
    }

    /// Start text annotation mode
    ///
    /// Activates the Note drawing tool, allowing the user to click
    /// on the chart to place a text note annotation.
    pub fn start_text_annotation(&mut self) {
        self.set_active_tool(Some(DrawingToolType::Note));
    }

    /// Start icon/emoji insertion mode
    ///
    /// Prepares a FontIcon drawing with the specified icon name,
    /// allowing the user to click on the chart to place it.
    pub fn start_icon_insertion(&mut self, icon_name: String) {
        self.set_active_tool(Some(DrawingToolType::FontIcon));

        // Create a new drawing with the icon name
        let mut drawing = Drawing::with_color(
            self.next_id,
            DrawingToolType::FontIcon,
            self.options.default_color,
        );
        self.next_id += 1;

        // Store the icon name in the text field
        drawing.text = Some(icon_name);
        drawing.font_size = 32.0; // Larger size for icons/emojis
        drawing.completed = false; // Not complete until user clicks

        // Set as current drawing (will be placed on next click)
        self.curr_drawing = Some(drawing);
    }

    // === Drawing lifecycle ===

    /// Starts a new drawing of the given tool type at the given screen position.
    ///
    /// The coordinate conversion closures (`x_to_bar`, `y_to_price`) convert
    /// screen coordinates to chart coordinates for persistent storage. If the
    /// tool requires only one point, the drawing is immediately completed and
    /// added to the drawing list.
    pub fn start_drawing_with_coords<F, G>(
        &mut self,
        tool_type: DrawingToolType,
        point: Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        let mut drawing = Drawing::with_color(self.next_id, tool_type, self.options.default_color);
        let drawing_id = self.next_id;
        self.next_id += 1;
        drawing.add_point_with_chart_coords(point, x_to_bar, y_to_price);

        if drawing.completed {
            self.history.push_add(drawing.clone());
            self.drawings.push(drawing);

            // Queue cloud sync
            if let Ok(mut sync) = self.pending_sync.lock() {
                sync.push(DrawingSyncAction::SaveDrawing(drawing_id));
            }

            self.curr_drawing = None;
            if !self.stay_in_drawing_mode {
                self.active_tool = None;
            }
        } else {
            self.curr_drawing = Some(drawing);
        }
    }

    /// Adds a subsequent point to the in-progress drawing.
    ///
    /// If the added point completes the drawing (based on
    /// [`DrawingToolType::required_points`]), the drawing is finalized, pushed
    /// to the drawing list, and a cloud sync action is queued.
    pub fn add_point_with_coords<F, G>(&mut self, point: Pos2, x_to_bar: F, y_to_price: G)
    where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if let Some(ref mut drawing) = self.curr_drawing {
            let drawing_id = drawing.id;
            drawing.add_point_with_chart_coords(point, &x_to_bar, &y_to_price);

            if drawing.completed {
                self.history.push_add(drawing.clone());
                self.drawings.push(drawing.clone());

                // Queue cloud sync
                if let Ok(mut sync) = self.pending_sync.lock() {
                    sync.push(DrawingSyncAction::SaveDrawing(drawing_id));
                }

                self.curr_drawing = None;
                if !self.stay_in_drawing_mode {
                    self.active_tool = None;
                }
            }
        }
    }

    /// Updates the last point of the in-progress drawing (used for live preview
    /// as the cursor moves between the first click and the final placement).
    pub fn update_last_point_with_coords<F, G>(&mut self, point: Pos2, x_to_bar: F, y_to_price: G)
    where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if let Some(ref mut drawing) = self.curr_drawing
            && !drawing.points.is_empty()
            && !drawing.completed
        {
            if drawing.points.len() == 1 {
                drawing.add_point_with_chart_coords(point, &x_to_bar, &y_to_price);
                drawing.completed = false;
            } else if drawing.points.len() >= 2 {
                drawing.update_last_point_with_chart_coords(point, x_to_bar, y_to_price);
            }
        }
    }

    /// Manually completes the in-progress drawing (e.g., on double-click or Enter
    /// for multi-point tools). Requires at least 2 points.
    pub fn complete_curr_drawing(&mut self) {
        if let Some(ref mut drawing) = self.curr_drawing
            && drawing.points.len() >= 2
        {
            drawing.completed = true;
            self.history.push_add(drawing.clone());
            self.drawings.push(drawing.clone());
            self.curr_drawing = None;
            if !self.stay_in_drawing_mode {
                self.active_tool = None;
            }
        }
    }

    /// Cancels the in-progress drawing without adding it to the drawing list.
    pub fn cancel_curr_drawing(&mut self) {
        self.curr_drawing = None;
    }

    // === Drawing CRUD ===

    /// Deletes the drawing with the given ID from the drawing list.
    ///
    /// The deleted drawing is pushed to the undo history and a cloud sync
    /// delete action is queued. If the deleted drawing was selected, the
    /// selection is cleared.
    pub fn delete_drawing(&mut self, id: usize) {
        if let Some(drawing) = self.drawings.iter().find(|d| d.id == id).cloned() {
            self.drawings.retain(|d| d.id != id);
            self.history.push_delete(drawing);
            if let Ok(mut sync) = self.pending_sync.lock() {
                sync.push(DrawingSyncAction::Delete(id.to_string()));
            }
        }
        if self.selection.primary() == Some(id) {
            self.selection.deselect_all();
        }
    }

    /// Deletes the currently selected drawing (if any).
    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selection.primary() {
            self.delete_drawing(id);
        }
    }

    /// Removes all drawings and clears the current drawing and selection.
    ///
    /// Note: This does not push to the undo history (it is a destructive reset).
    pub fn clear_all(&mut self) {
        self.drawings.clear();
        self.curr_drawing = None;
        self.selection.deselect_all();
    }

    // === Layer ordering ===

    /// Bring a drawing to the front (rendered last, on top)
    pub fn bring_to_front(&mut self, id: usize) {
        if let Some(idx) = self.drawings.iter().position(|d| d.id == id) {
            let drawing = self.drawings.remove(idx);
            self.drawings.push(drawing);
        }
    }

    /// Send a drawing to the back (rendered first, behind others)
    pub fn send_to_back(&mut self, id: usize) {
        if let Some(idx) = self.drawings.iter().position(|d| d.id == id) {
            let drawing = self.drawings.remove(idx);
            self.drawings.insert(0, drawing);
        }
    }

    // === Coordinate updates ===

    /// Recomputes screen coordinates for all drawings (completed and in-progress)
    /// from their persistent chart coordinates using the current pan/zoom transforms.
    ///
    /// Must be called each frame before rendering.
    pub fn update_all_screen_coords<F, G>(&mut self, bar_to_x: F, price_to_y: G)
    where
        F: Fn(f32) -> f32 + Copy,
        G: Fn(f64) -> f32 + Copy,
    {
        for drawing in &mut self.drawings {
            drawing.update_screen_coords(bar_to_x, price_to_y);
        }
        if let Some(ref mut drawing) = self.curr_drawing {
            drawing.update_screen_coords(bar_to_x, price_to_y);
        }
    }

    /// Shifts all bar indices by `shift` across all drawings, the in-progress
    /// drawing, and the undo history.
    ///
    /// This is called when historical data is prepended to the chart, causing
    /// existing bar indices to shift right.
    pub fn shift_bar_indices(&mut self, shift: f32) {
        if shift.abs() < 0.001 {
            return;
        }
        for drawing in &mut self.drawings {
            for cp in &mut drawing.chart_points {
                cp.bar_idx += shift;
            }
        }
        if let Some(ref mut curr) = self.curr_drawing {
            for cp in &mut curr.chart_points {
                cp.bar_idx += shift;
            }
        }
        self.history.shift_bar_indices(shift);
    }

    // === Hit testing ===

    /// Hit-tests a screen point against all visible drawings (back-to-front).
    ///
    /// Returns the ID of the topmost drawing at `point`, or `None` if no
    /// drawing was hit. Uses a 5-pixel tolerance.
    pub fn hit_test(&self, point: Pos2) -> Option<usize> {
        let hit_distance = 5.0;
        for drawing in self.drawings.iter().rev() {
            if !drawing.visible {
                continue;
            }
            if self.hit_test_drawing(drawing, point, hit_distance) {
                return Some(drawing.id);
            }
        }
        None
    }

    fn hit_test_drawing(&self, drawing: &Drawing, point: Pos2, tolerance: f32) -> bool {
        match drawing.tool_type {
            DrawingToolType::TrendLine | DrawingToolType::Measure => {
                drawing.points.len() >= 2
                    && point_to_line_distance(point, drawing.points[0], drawing.points[1])
                        <= tolerance
            }
            DrawingToolType::HorizontalLine => {
                !drawing.points.is_empty() && (point.y - drawing.points[0].y).abs() <= tolerance
            }
            DrawingToolType::VerticalLine => {
                !drawing.points.is_empty() && (point.x - drawing.points[0].x).abs() <= tolerance
            }
            DrawingToolType::Rect => {
                drawing.points.len() >= 2
                    && Rect::from_two_pos(drawing.points[0], drawing.points[1]).contains(point)
            }
            _ => drawing.points.iter().any(|&p| {
                ((point.x - p.x).powi(2) + (point.y - p.y).powi(2)).sqrt() <= tolerance * 2.0
            }),
        }
    }

    // === Visibility ===

    /// Toggles the visibility of all drawings. If all are visible, hides them
    /// all; otherwise shows them all.
    pub fn toggle_visibility_all(&mut self) {
        let all_visible = self.drawings.iter().all(|d| d.visible);
        for drawing in &mut self.drawings {
            drawing.visible = !all_visible;
        }
    }

    /// Toggles the lock state of all drawings. If all are locked, unlocks them
    /// all; otherwise locks them all.
    pub fn toggle_lock_all(&mut self) {
        let all_locked = self.drawings.iter().all(|d| d.locked);
        for drawing in &mut self.drawings {
            drawing.locked = !all_locked;
        }
    }

    // === Compatibility methods for old DrawingManager API ===

    /// Complete a drag-to-draw drawing (same as complete_curr_drawing)
    pub fn complete_drag_drawing(&mut self) {
        self.complete_curr_drawing();
    }

    /// Update drag handle with coordinate conversion functions
    /// Alias for update_drag_handle for API compatibility
    pub fn update_drag_handle_with_coords<F, G>(
        &mut self,
        new_pos: Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        self.update_drag_handle(new_pos, x_to_bar, y_to_price);
    }

    /// Hit test a handle by drawing ID
    /// Returns the handle position if hit, None otherwise
    pub fn hit_test_handle_by_id(&self, point: Pos2, drawing_id: usize) -> Option<HandlePos> {
        if let Some(drawing) = self.drawings.iter().find(|d| d.id == drawing_id) {
            self.hit_test_handle(drawing, point)
        } else {
            None
        }
    }

    /// Update real-time prices for position drawings (P&L calculation)
    pub fn update_pos_prices(&mut self, price: f64) {
        for drawing in &mut self.drawings {
            drawing.curr_price = Some(price);
        }
    }

    /// Add a drawing from another source (for multi-chart sync)
    pub fn add_synced_drawing(&mut self, mut drawing: Drawing) {
        // Assign a new ID to avoid conflicts
        drawing.id = self.next_id;
        self.next_id += 1;
        self.drawings.push(drawing);
    }

    /// Render all drawings
    pub fn render_all(&self, painter: &egui::Painter, price_rect: Rect) {
        // Render completed drawings
        for drawing in &self.drawings {
            if drawing.visible {
                drawing.render(painter, price_rect);
            }
        }

        // Render current drawing being created
        if let Some(ref drawing) = self.curr_drawing {
            drawing.render(painter, price_rect);
        }

        // Render handles for selected drawing
        if let Some(sel_id) = self.selection.primary()
            && let Some(drawing) = self.drawings.iter().find(|d| d.id == sel_id)
        {
            self.render_handles(painter, drawing);
        }
    }
}

/// Calculates the perpendicular distance from `point` to the closest point on
/// the line segment `[line_start, line_end]`.
///
/// The projection is clamped to the segment, so if the closest point on the
/// infinite line lies outside the segment, the distance to the nearest endpoint
/// is returned instead. Returns the distance to `line_start` if the segment is
/// degenerate (zero-length).
fn point_to_line_distance(point: Pos2, line_start: Pos2, line_end: Pos2) -> f32 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-6 {
        return ((point.x - line_start.x).powi(2) + (point.y - line_start.y).powi(2)).sqrt();
    }

    let t =
        (((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / len_sq).clamp(0.0, 1.0);
    let proj_x = line_start.x + t * dx;
    let proj_y = line_start.y + t * dy;

    ((point.x - proj_x).powi(2) + (point.y - proj_y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manager() {
        let manager = DrawingManager::new();
        assert!(manager.drawings.is_empty());
        assert!(manager.active_tool.is_none());
    }

    #[test]
    fn test_selection() {
        let mut manager = DrawingManager::new();
        manager.select(1);
        assert_eq!(manager.sel_drawing(), Some(1));
        manager.deselect();
        assert_eq!(manager.sel_drawing(), None);
    }
}
