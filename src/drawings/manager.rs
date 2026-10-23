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

use crate::drawings::domain::{ChartPoint, Drawing, DrawingToolType, HandlePos};
use crate::drawings::rendering::ChartView;
use crate::drawings::services::{
    DrawingInteraction, HandleConfig, HandleService, HistoryService, SelectionService, SnapOptions,
    SnapService, ZOrderService, in_z_order,
};
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect};
use std::sync::{Arc, Mutex};

/// A change to the stored drawings, queued in [`DrawingManager::pending_sync`].
///
/// Every [`DrawingManager`] method that changes a field a drawing is stored with queues one
/// action per drawing it changes: creating, moving, styling, locking, hiding, layering,
/// deleting, undo and redo. Restoring a stored drawing with
/// [`DrawingManager::add_synced_drawing`] queues nothing. The chart never drains the queue;
/// the host does, and saves or deletes each drawing it names, for example with
/// [`drawing_to_stored`](crate::drawings::persistence::drawing_to_stored). A host that writes
/// to [`DrawingManager::drawings`] directly queues nothing.
#[derive(Debug, Clone)]
pub enum DrawingSyncAction {
    /// A new drawing was created and should be saved. Carries the drawing ID.
    SaveDrawing(usize),
    /// An existing drawing was modified (moved, resized, styled). Carries the drawing ID.
    UpdateDrawing(usize),
    /// A drawing was deleted. Carries the stored drawing ID as a string.
    Delete(String),
    /// A batch of drawings should be restored from backend storage. The manager never
    /// queues it.
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

/// Colour of a new drawing: a pink that no default study uses, at 4:1 or better on the dark,
/// light and high contrast chart backgrounds.
const DEFAULT_DRAWING_COLOR: [u8; 4] = [233, 30, 99, 255];

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
    /// Snap to the nearest OHLC value of the bar under the pointer regardless of distance.
    /// Requires `magnet_mode`; `false` retains the pixel threshold and nearby overlay targets.
    pub strong_magnet: bool,
    /// Configuration for selection handle appearance and hit testing.
    pub handle_config: HandleConfig,
}

impl Default for DrawingManagerOptions {
    fn default() -> Self {
        Self {
            default_color: DEFAULT_DRAWING_COLOR,
            snap_to_price: true,
            snap_to_time: true,
            snap_distance: DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs,
            magnet_mode: false,
            magnet_distance: DESIGN_TOKENS.sizing.drawing.magnet_distance,
            strong_magnet: false,
            handle_config: HandleConfig::default(),
        }
    }
}

/// Last painted geometry, invalid once its anchors have moved.
struct PaintedDrawing {
    id: usize,
    points: Vec<Pos2>,
    shapes: Vec<egui::epaint::ClippedShape>,
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
/// ```
/// # use egui_charts::drawings::{DrawingManager, DrawingState};
/// # use egui_charts::drawings::rendering::ChartView;
/// # fn frame(
/// #     manager: &mut DrawingManager,
/// #     drawing_state: &DrawingState,
/// #     painter: &egui::Painter,
/// #     price_rect: egui::Rect,
/// #     bars: &[egui_charts::model::Bar],
/// #     revision: u64,
/// #     bar_to_x: impl Fn(f32) -> f32 + Copy,
/// #     price_to_y: impl Fn(f64) -> f32 + Copy,
/// # ) {
/// // 1. Sync state from app
/// manager.sync_from_app_state(drawing_state);
///
/// // 2. Update screen coordinates from chart coordinates
/// manager.update_all_screen_coords(bar_to_x, price_to_y);
///
/// // 3. Handle user input (clicks, drags, keyboard)
/// // ... (start_drawing_with_coords, add_point_with_coords, etc.)
///
/// // 4. Render against the chart's bars and coordinate mapping
/// let view = ChartView { instrument: None, bars, revision, bar_to_x: &bar_to_x, price_to_y: &price_to_y };
/// manager.render_all(painter, price_rect, &view);
/// # }
/// ```
pub struct DrawingManager {
    /// All completed drawings on the chart.
    pub drawings: Vec<Drawing>,
    /// Currently active drawing tool, or `None` for selection/pointer mode.
    pub active_tool: Option<DrawingToolType>,
    /// The drawing currently being created (not yet completed).
    pub curr_drawing: Option<Drawing>,
    /// Whether the last point of `curr_drawing` follows the pointer rather than a click.
    preview_point: bool,
    /// Next available drawing ID (monotonically increasing).
    next_id: usize,

    // Services
    selection: SelectionService,
    history: HistoryService,
    snap_service: SnapService,
    /// Geometry from the last paint, including laid-out text and data-driven shapes.
    painted: Mutex<Vec<PaintedDrawing>>,
    /// Nearby bar and overlay points, refreshed by the chart before pointer handling.
    pub(crate) snap_targets: Vec<Pos2>,

    /// Manager configuration (colors, snapping, handles).
    pub options: DrawingManagerOptions,

    /// Currently dragged handle, if any: `(drawing_id, handle_position)`.
    pub dragging_handle: Option<(usize, HandlePos)>,
    /// Snapshot of the drawing state before the current drag began (for undo).
    drag_old_state: Option<Drawing>,
    /// Pointer origin of a body or centre-handle translation.
    drag_origin: Option<Pos2>,

    /// Whether to stay in drawing mode after completing a drawing.
    pub stay_in_drawing_mode: bool,
    /// Current chart timeframe string (e.g., `"1D"`, `"1h"`): drawings hidden on it are
    /// neither drawn nor hit. [`Chart::set_timeframe`](crate::widget::Chart::set_timeframe)
    /// keeps it in step with the chart.
    pub curr_timeframe: String,
    /// Whether a click deletes the drawing under the pointer instead of selecting it. Set it
    /// with [`Self::set_eraser_mode`].
    pub eraser_mode: bool,
    /// Drawing ID currently hovered by the eraser cursor (for visual feedback).
    pub eraser_hover_drawing: Option<usize>,

    /// Changes to the stored drawings, oldest first, for the host to drain, for example
    /// with `std::mem::take(&mut *manager.pending_sync.lock().unwrap())`. See
    /// [`DrawingSyncAction`].
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
            preview_point: false,
            next_id: 0,
            selection: SelectionService::new(),
            history: HistoryService::new(),
            snap_service: SnapService::new(),
            painted: Mutex::new(Vec::new()),
            snap_targets: Vec::new(),
            options: DrawingManagerOptions::default(),
            dragging_handle: None,
            drag_old_state: None,
            drag_origin: None,
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
        self.step_history(HistoryService::undo)
    }

    /// Redoes the last undone operation. Returns `true` if an operation was redone.
    pub fn redo(&mut self) -> bool {
        self.step_history(HistoryService::redo)
    }

    /// Runs an undo or redo step and queues the change it made to its drawing.
    fn step_history(
        &mut self,
        step: fn(&mut HistoryService, &mut Vec<Drawing>) -> Option<usize>,
    ) -> bool {
        let before: Vec<_> = self.drawings.iter().map(|d| (d.id, sync_id(d))).collect();
        let Some(id) = step(&mut self.history, &mut self.drawings) else {
            return false;
        };
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| p.id != id);
        let was = before.into_iter().find(|&(i, _)| i == id);
        match (was, self.drawings.iter().any(|d| d.id == id)) {
            (Some(_), true) => self.queue(DrawingSyncAction::UpdateDrawing(id)),
            (Some((_, stored_id)), false) => self.queue(DrawingSyncAction::Delete(stored_id)),
            (None, true) => self.queue(DrawingSyncAction::SaveDrawing(id)),
            (None, false) => {}
        }
        true
    }

    /// Queues a change for the host to save.
    fn queue(&self, action: DrawingSyncAction) {
        if let Ok(mut sync) = self.pending_sync.lock() {
            sync.push(action);
        }
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
    /// With magnet mode on, snaps to nearby chart values and visible anchors of `drawings`.
    /// Targets keep their time and price paired. A dragged drawing cannot snap to itself.
    /// Drawing anchors are borrowed without flattening or cloning their geometry.
    pub fn snap_point(&self, point: Pos2, drawings: &[Drawing]) -> Pos2 {
        if self.options.magnet_mode
            && self.options.strong_magnet
            && let Some(target) = self
                .snap_targets
                .iter()
                .filter(|p| p.is_finite())
                .min_by(|a, b| (a.y - point.y).abs().total_cmp(&(b.y - point.y).abs()))
        {
            return *target;
        }
        self.snap_service.snap_point_with_drawing_points(
            point,
            &[],
            &[],
            self.snap_targets.iter().copied().chain(
                drawings
                    .iter()
                    .filter(|d| {
                        d.is_visible_on_timeframe(&self.curr_timeframe)
                            && self.dragging_handle.is_none_or(|(id, _)| id != d.id)
                    })
                    .flat_map(|d| d.points.iter().copied()),
            ),
        )
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
        if !drawing.is_visible_on_timeframe(&self.curr_timeframe) {
            return None;
        }
        HandleService::hit_test_handle(drawing, point, self.options.handle_config.size)
    }

    /// Begins a handle drag operation, saving the drawing's current state for undo. A locked
    /// drawing is not dragged.
    pub fn start_drag_handle(&mut self, drawing_id: usize, handle: HandlePos) {
        if let Some(drawing) = self.drawings.iter().find(|d| {
            d.id == drawing_id && !d.locked && d.is_visible_on_timeframe(&self.curr_timeframe)
        }) {
            self.dragging_handle = Some((drawing_id, handle));
            self.drag_old_state = Some(drawing.clone());
            self.drag_origin = (handle == HandlePos::Middle)
                .then(|| {
                    HandleService::get_handles(drawing)
                        .into_iter()
                        .find(|(h, _)| *h == handle)
                        .map(|(_, p)| p)
                })
                .flatten();
        }
    }

    /// Begins moving every anchor from a body grab at `origin`, retaining the grab offset.
    /// Locked drawings ignore the grab. The move shares the handle drag's undo and sync lifecycle.
    pub fn start_drag_body(&mut self, drawing_id: usize, origin: Pos2) {
        self.start_drag_handle(drawing_id, HandlePos::Middle);
        if self.dragging_handle == Some((drawing_id, HandlePos::Middle)) {
            self.drag_origin = Some(origin);
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
            let snapped = self.snap_point(new_pos, &self.drawings);
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id && !d.locked) {
                if let (Some(origin), Some(old)) = (self.drag_origin, &self.drag_old_state) {
                    let bars = x_to_bar(snapped.x) - x_to_bar(origin.x);
                    let price = y_to_price(snapped.y) - y_to_price(origin.y);
                    for (point, before) in drawing.chart_points.iter_mut().zip(&old.chart_points) {
                        *point = ChartPoint::new(before.bar_idx + bars, before.price + price);
                    }
                    for (point, before) in drawing.points.iter_mut().zip(&old.points) {
                        *point = *before + (snapped - origin);
                    }
                } else {
                    HandleService::update_handle(drawing, handle, snapped, x_to_bar, y_to_price);
                }
            }
        }
    }

    /// Ends the current handle drag or [`Self::drag_edit_drawing`] gesture, pushing one modify
    /// command to the undo history and queuing one update for the whole gesture.
    pub fn end_drag_handle(&mut self) {
        if let Some(old_state) = self.drag_old_state.take() {
            let drawing_id = old_state.id;
            self.history.push_modify(old_state.id, old_state);
            self.queue(DrawingSyncAction::UpdateDrawing(drawing_id));
        }
        self.dragging_handle = None;
        self.drag_origin = None;
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
    /// return to selection/pointer mode. A tool cancels any in-progress drawing and leaves
    /// eraser mode.
    pub fn set_active_tool(&mut self, tool: Option<DrawingToolType>) {
        self.active_tool = tool;
        if tool.is_some() {
            self.cancel_curr_drawing();
            self.eraser_mode = false;
        }
    }

    /// Switches eraser mode on or off. In eraser mode a click on the chart deletes the
    /// drawing under the pointer, unless it is locked. Switching it on leaves the active tool
    /// and cancels any in-progress drawing; choosing a tool with [`Self::set_active_tool`]
    /// switches it off.
    pub fn set_eraser_mode(&mut self, on: bool) {
        self.eraser_mode = on;
        if on {
            self.active_tool = None;
            self.cancel_curr_drawing();
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
        let id = self.take_id();
        let mut drawing =
            Drawing::with_color(id, DrawingToolType::FontIcon, self.options.default_color);

        // Store the icon name in the text field
        drawing.text = Some(icon_name);
        drawing.font_size = 32.0; // Larger size for icons/emojis
        drawing.completed = false; // Not complete until user clicks

        // Set as current drawing (will be placed on next click)
        self.curr_drawing = Some(drawing);
    }

    // === Drawing lifecycle ===

    /// Starts a new drawing of the given tool type at the given screen position, or places the
    /// drawing [`Self::start_icon_insertion`] prepared for it.
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
        let point = self.snap_point(point, &self.drawings);
        // A drawing prepared before its first anchor, such as a picked icon, starts here.
        let mut drawing = match self.curr_drawing.take() {
            Some(prepared) if prepared.tool_type == tool_type && prepared.points.is_empty() => {
                prepared
            }
            _ => Drawing::with_color(self.take_id(), tool_type, self.options.default_color),
        };
        drawing.z_order = self.top_z_order();
        self.preview_point = false;
        drawing.add_point_with_chart_coords(point, &x_to_bar, &y_to_price);
        if matches!(
            tool_type,
            DrawingToolType::LongPos | DrawingToolType::ShortPos
        ) {
            let side = if tool_type == DrawingToolType::LongPos {
                -1.0
            } else {
                1.0
            };
            let target = point + egui::vec2(100.0, side * 50.0);
            let target_price = y_to_price(target.y);
            let entry_price = drawing.chart_points[0].price;
            let stop = Pos2::new(target.x, point.y - side * 50.0);
            drawing.points.extend([target, stop]);
            drawing.chart_points.extend([
                ChartPoint::new(x_to_bar(target.x), target_price),
                ChartPoint::new(x_to_bar(target.x), 2.0 * entry_price - target_price),
            ]);
        }

        let completed = drawing.completed;
        self.curr_drawing = Some(drawing);
        if completed {
            self.finish_curr_drawing();
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
        let point = self.snap_point(point, &self.drawings);
        if let Some(ref mut drawing) = self.curr_drawing {
            // The click takes the place of the pointer preview.
            if std::mem::take(&mut self.preview_point) {
                drawing.points.pop();
                drawing.chart_points.pop();
            }
            drawing.add_point_with_chart_coords(point, &x_to_bar, &y_to_price);
            if drawing.completed {
                self.finish_curr_drawing();
            }
        }
    }

    /// Moves the pointer preview of the in-progress drawing to `point`, adding it after the
    /// placed anchors if there is none yet. The next click or the end of a drag replaces it
    /// with an anchor; finishing a multi-point drawing drops it.
    pub fn update_last_point_with_coords<F, G>(&mut self, point: Pos2, x_to_bar: F, y_to_price: G)
    where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        let point = self.snap_point(point, &self.drawings);
        if let Some(ref mut drawing) = self.curr_drawing
            && !drawing.points.is_empty()
            && !drawing.completed
        {
            if self.preview_point {
                drawing.update_last_point_with_chart_coords(point, x_to_bar, y_to_price);
            } else {
                drawing.add_point_with_chart_coords(point, &x_to_bar, &y_to_price);
                drawing.completed = false;
                self.preview_point = true;
            }
        }
    }

    /// Manually completes the in-progress drawing (e.g., on double-click or Enter
    /// for multi-point tools), without its pointer preview. Requires at least 2 points, and
    /// as many as the tool draws with, so that what it adds can be saved.
    pub fn complete_curr_drawing(&mut self) {
        if let Some(drawing) = &mut self.curr_drawing
            && std::mem::take(&mut self.preview_point)
        {
            drawing.points.pop();
            drawing.chart_points.pop();
        }
        if let Some(ref mut drawing) = self.curr_drawing
            && drawing.points.len() >= drawing.tool_type.anchors_needed().max(2)
        {
            drawing.completed = true;
            self.finish_curr_drawing();
        }
    }

    /// Moves the completed current drawing to the drawing list and queues it to be saved.
    /// In keep-drawing mode the icon tool goes on placing the icon that was picked.
    fn finish_curr_drawing(&mut self) {
        if let Some(drawing) = self.curr_drawing.take() {
            let icon = (drawing.tool_type == DrawingToolType::FontIcon)
                .then(|| drawing.text.clone())
                .flatten();
            self.history.push_add(drawing.clone());
            self.queue(DrawingSyncAction::SaveDrawing(drawing.id));
            self.drawings.push(drawing);
            if !self.stay_in_drawing_mode {
                self.active_tool = None;
            } else if let Some(icon) = icon {
                self.start_icon_insertion(icon);
            }
        }
    }

    /// Cancels the in-progress drawing without adding it to the drawing list.
    pub fn cancel_curr_drawing(&mut self) {
        self.curr_drawing = None;
        self.preview_point = false;
    }

    // === Drawing CRUD ===

    /// Deletes the drawing with the given ID from the drawing list.
    ///
    /// The deleted drawing is pushed to the undo history and a cloud sync
    /// delete action is queued. If the deleted drawing was selected, the
    /// selection is cleared. A locked drawing is not deleted.
    pub fn delete_drawing(&mut self, id: usize) {
        if self.drawings.iter().any(|d| d.id == id && d.locked) {
            return;
        }
        if let Some(drawing) = self.drawings.iter().find(|d| d.id == id).cloned() {
            self.drawings.retain(|d| d.id != id);
            self.queue(DrawingSyncAction::Delete(sync_id(&drawing)));
            self.history.push_delete(drawing);
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

    /// Removes every drawing that is not locked and clears the current drawing and selection.
    ///
    /// Queues a `Delete` for each drawing it removes, so the host deletes their stored records.
    /// To unload drawings without deleting them, clear [`Self::drawings`], then call
    /// [`Self::cancel_curr_drawing`] and [`Self::deselect`]. It does not push to the undo history.
    pub fn clear_all(&mut self) {
        for drawing in self.drawings.iter().filter(|d| !d.locked) {
            self.queue(DrawingSyncAction::Delete(sync_id(drawing)));
        }
        self.drawings.retain(|d| d.locked);
        self.cancel_curr_drawing();
        self.selection.deselect_all();
    }

    // === Layer ordering ===
    //
    // `Drawing::z_order` decides the layering: drawings are drawn in ascending z-order and hit
    // from the highest down, in list order where it is the same.

    /// Bring a drawing to the front (drawn last, on top), above the highest z-order
    pub fn bring_to_front(&mut self, id: usize) {
        ZOrderService::bring_to_front(&mut self.drawings, id);
        self.queue_update(id);
    }

    /// Send a drawing to the back (drawn first, behind others), below the lowest z-order
    pub fn send_to_back(&mut self, id: usize) {
        ZOrderService::send_to_back(&mut self.drawings, id);
        self.queue_update(id);
    }

    /// Applies `edit` to the drawing with the given ID, records it for undo and queues it to
    /// be saved. Edit a drawing through this rather than [`Self::drawings`] so the change is
    /// saved. A locked drawing accepts only an edit that unlocks it.
    /// Does nothing if there is no such drawing.
    pub fn edit_drawing(&mut self, id: usize, edit: impl FnOnce(&mut Drawing)) {
        if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id) {
            let old = drawing.clone();
            edit(drawing);
            if old.locked && drawing.locked {
                *drawing = old;
                return;
            }
            self.history.push_modify(id, old);
            self.painted
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|p| p.id != id);
            self.queue(DrawingSyncAction::UpdateDrawing(id));
        }
    }

    /// Applies `edit` as one frame of a continuous edit, such as dragging a colour picker: the
    /// first frame records the drawing's state and nothing is queued until
    /// [`Self::end_drag_handle`], which makes the whole gesture one undo step and queues one
    /// update. Does nothing if there is no such drawing or it is locked.
    pub fn drag_edit_drawing(&mut self, id: usize, edit: impl FnOnce(&mut Drawing)) {
        if self.drag_old_state.as_ref().is_some_and(|d| d.id != id) {
            self.end_drag_handle();
        }
        if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id && !d.locked) {
            self.drag_old_state.get_or_insert_with(|| drawing.clone());
            edit(drawing);
            self.painted
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|p| p.id != id);
        }
    }

    /// Queues an update of the drawing with the given ID, if there is one.
    fn queue_update(&self, id: usize) {
        if self.drawings.iter().any(|d| d.id == id) {
            self.queue(DrawingSyncAction::UpdateDrawing(id));
        }
    }

    /// An id above every drawing's id and every numeric id a drawing is stored under, so a new
    /// drawing is never saved over a stored one. A stored id too large to leave room above it
    /// is not one this manager hands out, so it cannot collide and is left out.
    fn take_id(&mut self) -> usize {
        let floor = self
            .drawings
            .iter()
            .flat_map(|d| {
                [
                    Some(d.id),
                    d.stored_id
                        .as_deref()
                        .and_then(|s| s.parse().ok())
                        .filter(|&id: &usize| id < usize::MAX / 2),
                ]
            })
            .flatten()
            .max()
            .map_or(0, |id: usize| id.saturating_add(1));
        let id = self.next_id.max(floor);
        self.next_id = id.saturating_add(1);
        id
    }

    /// The z-order that puts a drawing added now on top of the others.
    fn top_z_order(&self) -> i32 {
        self.drawings.iter().map(|d| d.z_order).max().unwrap_or(0)
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

    /// Shifts every bar index at or after `from` by `shift`, across all
    /// drawings, the in-progress drawing, the state a handle drag restores on
    /// undo, and the undo history.
    ///
    /// This is called when bars are inserted in front of existing ones, causing
    /// the bar indices from there on to shift right. Pass `f32::NEG_INFINITY`
    /// for bars inserted in front of the first bar.
    pub fn shift_bar_indices(&mut self, from: f32, shift: f32) {
        if shift.abs() < 0.001 {
            return;
        }
        for drawing in &mut self.drawings {
            drawing.shift_bar_indices(from, shift);
        }
        if let Some(ref mut curr) = self.curr_drawing {
            curr.shift_bar_indices(from, shift);
        }
        if let Some(old) = &mut self.drag_old_state {
            old.shift_bar_indices(from, shift);
        }
        self.history.shift_bar_indices(from, shift);
    }

    // === Hit testing ===

    /// Hit-tests a screen point against all visible drawings (back-to-front).
    ///
    /// Returns the ID of the topmost drawing at `point`, or `None` if no
    /// drawing was hit. Delegates to [`DrawingInteraction::hit_test`], the single
    /// source of truth for per-segment / per-level geometry hit testing, so that
    /// channels, Fibonacci levels, fans, and pitchforks are selectable along
    /// their drawn geometry rather than only near a control point. Honors the
    /// active timeframe so drawings hidden on the current timeframe are not hit.
    /// After [`Self::render_all`], uses the actual clipped paint geometry, including text and
    /// data-driven bodies. Before the first paint, falls back to the tool's geometric outline.
    pub fn hit_test(&self, point: Pos2) -> Option<usize> {
        let painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        in_z_order(&self.drawings)
            .into_iter()
            .rev()
            .filter(|d| d.is_visible_on_timeframe(&self.curr_timeframe))
            .find(|drawing| {
                let paint = painted
                    .iter()
                    .find(|p| p.id == drawing.id && p.points == drawing.points);
                if paint.is_some_and(|p| !p.shapes.iter().any(|s| s.clip_rect.contains(point))) {
                    return false;
                }
                paint.is_some_and(|p| DrawingInteraction::hit_test_painted(point, &p.shapes))
                    || DrawingInteraction::new()
                        .hit_test(point, std::slice::from_ref(*drawing), &self.curr_timeframe)
                        .is_some()
            })
            .map(|d| d.id)
    }

    // === Visibility ===

    /// Toggles the visibility of all drawings. If all are visible, hides them
    /// all; otherwise shows them all.
    pub fn toggle_visibility_all(&mut self) {
        let all_visible = self.drawings.iter().all(|d| d.visible);
        for drawing in self
            .drawings
            .iter_mut()
            .filter(|d| d.visible == all_visible)
        {
            drawing.visible = !all_visible;
            if let Ok(mut sync) = self.pending_sync.lock() {
                sync.push(DrawingSyncAction::UpdateDrawing(drawing.id));
            }
        }
    }

    /// Toggles the lock state of all drawings. If all are locked, unlocks them
    /// all; otherwise locks them all.
    pub fn toggle_lock_all(&mut self) {
        let all_locked = self.drawings.iter().all(|d| d.locked);
        for drawing in self.drawings.iter_mut().filter(|d| d.locked == all_locked) {
            drawing.locked = !all_locked;
            if let Ok(mut sync) = self.pending_sync.lock() {
                sync.push(DrawingSyncAction::UpdateDrawing(drawing.id));
            }
        }
    }

    // === Compatibility methods for old DrawingManager API ===

    /// Complete a drag-to-draw drawing (see [`Self::complete_curr_drawing`])
    pub fn complete_drag_drawing(&mut self) {
        // The pointer preview is where the drag ended: the drawing's last anchor.
        self.preview_point = false;
        // A position's stop starts mirrored around the entry, as an anchor of
        // its own so it can be moved without moving the target.
        if let Some(drawing) = &mut self.curr_drawing
            && matches!(
                drawing.tool_type,
                DrawingToolType::LongPos | DrawingToolType::ShortPos
            )
            && let (&[entry, target], &[entry_at, target_at]) =
                (drawing.chart_points.as_slice(), drawing.points.as_slice())
        {
            let stop = 2.0 * entry.price - target.price;
            drawing
                .chart_points
                .push(ChartPoint::new(target.bar_idx, stop));
            drawing
                .points
                .push(Pos2::new(target_at.x, 2.0 * entry_at.y - target_at.y));
        }
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

    /// Add a drawing from another source (for multi-chart sync, or restored from storage)
    /// under a new id. A drawing restored from storage keeps its stored identity (`stored_id`,
    /// `created_at`, `notes`) and its z-order. A drawing that was never stored goes on top, and
    /// so does a copy, one stored under the id of a drawing already here, which is saved as a
    /// new drawing.
    pub fn add_synced_drawing(&mut self, mut drawing: Drawing) {
        if drawing.stored_id.is_some()
            && self
                .drawings
                .iter()
                .any(|d| d.stored_id == drawing.stored_id)
        {
            (drawing.stored_id, drawing.created_at) = (None, None);
        }
        if drawing.stored_id.is_none() {
            drawing.z_order = self.top_z_order();
        }
        // Assign a new ID to avoid conflicts
        drawing.id = self.take_id();
        if drawing.stored_id.is_none() {
            self.queue(DrawingSyncAction::SaveDrawing(drawing.id));
        }
        self.drawings.push(drawing);
    }

    /// Render all drawings against the chart's bars and coordinate mapping
    pub fn render_all(&self, painter: &egui::Painter, price_rect: Rect, view: &ChartView) {
        let painter = &painter.with_clip_rect(price_rect);
        let mut painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        painted.clear();
        // Render completed drawings, bottom layer first
        for drawing in in_z_order(&self.drawings) {
            if drawing.is_visible_on_timeframe(&self.curr_timeframe) {
                let start = painter.ctx().graphics(|g| {
                    g.get(painter.layer_id())
                        .map_or(0, |list| list.next_idx().0)
                });
                drawing.render(painter, price_rect, view);
                let shapes = painter.ctx().graphics(|g| {
                    g.get(painter.layer_id())
                        .map(|list| list.all_entries().skip(start).cloned().collect())
                        .unwrap_or_default()
                });
                painted.push(PaintedDrawing {
                    id: drawing.id,
                    points: drawing.points.clone(),
                    shapes,
                });
            }
        }

        // Render current drawing being created
        if let Some(ref drawing) = self.curr_drawing {
            drawing.render(painter, price_rect, view);
        }

        // Render handles for selected drawing
        if let Some(sel_id) = self.selection.primary()
            && let Some(drawing) = self
                .drawings
                .iter()
                .find(|d| d.id == sel_id && d.is_visible_on_timeframe(&self.curr_timeframe))
        {
            self.render_handles(painter, drawing);
        }
    }
}

/// The id a drawing is stored under.
fn sync_id(drawing: &Drawing) -> String {
    drawing
        .stored_id
        .clone()
        .unwrap_or_else(|| drawing.id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text extents, position zones, Fibonacci rings and repeating cycle lines are
    /// selectable away from every anchor, using their actual paint geometry.
    #[test]
    fn painted_bodies_are_hit_away_from_anchors() {
        use egui::{Context, RawInput};
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        for (tool, anchors, hit, miss) in [
            (
                DrawingToolType::TrendLine,
                vec![(50., 100.), (550., 100.)],
                (200., 100.),
                (510., 100.),
            ),
            (
                DrawingToolType::TextLabel,
                vec![(50., 100.)],
                (180., 105.),
                (180., 160.),
            ),
            (
                DrawingToolType::LongPos,
                vec![(50., 150.), (250., 50.), (250., 250.)],
                (180., 100.),
                (350., 300.),
            ),
            (
                DrawingToolType::FibonacciCircles,
                vec![(100., 100.), (200., 100.)],
                (100., 200.),
                (400., 350.),
            ),
            (
                DrawingToolType::CyclicLines,
                vec![(50., 100.), (150., 100.)],
                (250., 300.),
                (480., 390.),
            ),
        ] {
            let ctx = Context::default();
            let mut manager = DrawingManager::new();
            let mut drawing = Drawing::new(7, tool);
            drawing.points = anchors.into_iter().map(|(x, y)| Pos2::new(x, y)).collect();
            drawing.chart_points = drawing
                .points
                .iter()
                .map(|p| ChartPoint::new(p.x / 10.0, 500.0 - p.y as f64))
                .collect();
            drawing.text = Some("A long text annotation".into());
            manager.drawings.push(drawing);
            ctx.run_ui(RawInput::default(), |ui| {
                let view = ChartView {
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b * 10.0,
                    price_to_y: &|p| 500.0 - p as f32,
                };
                manager.render_all(ui.painter(), pane, &view);
            })
            .drop_without_applying_deltas();
            assert_eq!(
                manager.hit_test(Pos2::new(hit.0, hit.1)),
                Some(7),
                "{tool:?}"
            );
            assert_eq!(
                manager.hit_test(Pos2::new(miss.0, miss.1)),
                None,
                "{tool:?}"
            );
            if tool == DrawingToolType::TextLabel {
                manager.edit_drawing(7, |d| d.text = Some("x".into()));
                assert_eq!(manager.hit_test(Pos2::new(hit.0, hit.1)), None);
            }
            manager.drawings[0].visible = false;
            assert_eq!(manager.hit_test(Pos2::new(hit.0, hit.1)), None);
        }
    }

    #[test]
    fn test_new_manager() {
        let manager = DrawingManager::new();
        assert!(manager.drawings.is_empty());
        assert!(manager.active_tool.is_none());
    }

    /// Two clicks and Enter finish a multi-point tool with two anchors. Whatever the
    /// manager finishes must save, and a tool that draws nothing with two anchors (the parallel
    /// channel) must stay in progress, or the whole storage would refuse to save.
    #[test]
    fn a_drawing_finished_early_can_be_saved() {
        use crate::drawings::persistence::{DrawingStorage, drawing_to_stored};
        for (tool, finishes) in [
            (DrawingToolType::Triangle, true),
            (DrawingToolType::Pitchfork, true),
            (DrawingToolType::FlatTopBottom, true),
            (DrawingToolType::ParallelChannel, false),
        ] {
            let mut manager = DrawingManager::new();
            let (x_to_bar, y_to_price) = (|x: f32| x, |y: f32| f64::from(y));
            manager.start_drawing_with_coords(tool, Pos2::new(10.0, 10.0), x_to_bar, y_to_price);
            manager.add_point_with_coords(Pos2::new(50.0, 40.0), x_to_bar, y_to_price);
            manager.complete_curr_drawing();
            assert_eq!(manager.drawings.len(), usize::from(finishes), "{tool:?}");
            let mut storage = DrawingStorage::new("AAPL");
            for drawing in &manager.drawings {
                storage.add_drawing(drawing_to_stored(drawing, |bar| 60_000 * bar as i64));
            }
            assert!(
                storage.to_json().is_ok(),
                "{tool:?}: {:?}",
                storage.to_json()
            );
        }
    }

    /// The pointer moves between the clicks that place a drawing's anchors. The drawing must
    /// finish on the click that places its last anchor and keep exactly the clicked positions,
    /// with no anchor left at a hovered position.
    #[test]
    fn hovering_between_clicks_adds_no_anchor() {
        for (tool, clicks) in [
            (DrawingToolType::TrendLine, 2),
            (DrawingToolType::ParallelChannel, 3),
            (DrawingToolType::XABCDPattern, 5),
            (DrawingToolType::Polyline, 3),
        ] {
            let mut manager = DrawingManager::new();
            let (x_to_bar, y_to_price) = (|x: f32| x, |y: f32| f64::from(y));
            let click = |i: usize| Pos2::new(10.0 * (i + 1) as f32, 5.0 * (i % 2) as f32);
            manager.start_drawing_with_coords(tool, click(0), x_to_bar, y_to_price);
            for i in 1..clicks {
                manager.update_last_point_with_coords(Pos2::new(99.0, 99.0), x_to_bar, y_to_price);
                assert!(
                    manager.drawings.is_empty(),
                    "{tool:?} finished after {i} clicks"
                );
                manager.add_point_with_coords(click(i), x_to_bar, y_to_price);
            }
            manager.update_last_point_with_coords(Pos2::new(99.0, 99.0), x_to_bar, y_to_price);
            manager.complete_curr_drawing();
            let want: Vec<_> = (0..clicks)
                .map(|i| ChartPoint::new(click(i).x, f64::from(click(i).y)))
                .collect();
            assert_eq!(manager.drawings.len(), 1, "{tool:?}");
            assert_eq!(manager.drawings[0].chart_points, want, "{tool:?}");
        }
    }

    /// A locked drawing keeps its place when one of its handles is dragged and survives a
    /// delete or style edit, until it is explicitly unlocked.
    #[test]
    fn a_locked_drawing_is_neither_moved_nor_deleted() {
        let mut manager = DrawingManager::new();
        let mut line = Drawing::new(3, DrawingToolType::TrendLine);
        line.chart_points = vec![ChartPoint::new(10.0, 10.0), ChartPoint::new(20.0, 20.0)];
        line.update_screen_coords(|bar| bar, |price| price as f32);
        line.locked = true;
        manager.drawings.push(line);

        manager.start_drag_handle(3, HandlePos::Start);
        manager.update_drag_handle(Pos2::new(50.0, 50.0), |x| x, f64::from);
        manager.end_drag_handle();
        manager.delete_drawing(3);
        manager.edit_drawing(3, |d| d.font_size = 30.0);
        manager.drag_edit_drawing(3, |d| d.font_size = 40.0);
        manager.end_drag_handle();
        assert_eq!(manager.drawings[0].font_size, 12.0);
        assert!(!manager.can_undo());
        assert!(manager.pending_sync.lock().unwrap().is_empty());
        manager.edit_drawing(3, |d| d.locked = false);
        manager.edit_drawing(3, |d| d.font_size = 30.0);
        assert_eq!(manager.drawings[0].font_size, 30.0);

        assert_eq!(manager.drawings.len(), 1);
        assert_eq!(
            manager.drawings[0].chart_points[0],
            ChartPoint::new(10.0, 10.0)
        );
    }

    /// Of two overlapping drawings, the one with the higher `z_order` is hit, whichever was
    /// added last, the manager's layer operations set `z_order`, which is what is saved, and a
    /// drawing a host adds goes on top.
    #[test]
    fn z_order_decides_which_overlapping_drawing_is_hit() {
        use crate::drawings::services::ZOrderService;
        let mut manager = DrawingManager::new();
        for id in [1, 2] {
            let mut line = Drawing::new(id, DrawingToolType::HorizontalLine);
            line.chart_points = vec![ChartPoint::new(10.0, 50.0)];
            line.update_screen_coords(|bar| bar, |price| price as f32);
            manager.drawings.push(line);
        }
        let on_both = Pos2::new(30.0, 50.0);
        let z = |m: &DrawingManager, id| m.drawings.iter().find(|d| d.id == id).unwrap().z_order;
        assert_eq!(manager.hit_test(on_both), Some(2));

        ZOrderService::bring_to_front(&mut manager.drawings, 1);
        assert_eq!(manager.hit_test(on_both), Some(1));

        manager.send_to_back(1);
        assert_eq!(manager.hit_test(on_both), Some(2));
        assert!(z(&manager, 1) < z(&manager, 2));

        manager.bring_to_front(1);
        assert_eq!(manager.hit_test(on_both), Some(1));
        assert!(z(&manager, 1) > z(&manager, 2));

        // A drawing a host adds, never stored, goes on top of one at z-order 5.
        manager
            .drawings
            .iter_mut()
            .find(|d| d.id == 1)
            .unwrap()
            .z_order = 5;
        let mut fresh = Drawing::new(0, DrawingToolType::HorizontalLine);
        fresh.chart_points = vec![ChartPoint::new(10.0, 50.0)];
        fresh.update_screen_coords(|bar| bar, |price| price as f32);
        manager.add_synced_drawing(fresh);
        let fresh_id = manager.drawings.last().unwrap().id;
        assert_eq!(manager.hit_test(on_both), Some(fresh_id));
    }

    /// Every edit of a stored field queues one action per drawing it changes, so a host that
    /// saves from the queue stores what the user sees. Restoring a stored drawing queues nothing,
    /// and deleting it names the record it is stored under.
    #[test]
    fn every_edit_queues_one_sync_action() {
        type Edit = fn(&mut DrawingManager);
        let cases: [(Edit, &[&str]); 11] = [
            (|m| m.clear_all(), &["Delete(a1b2)"]),
            (|m| m.toggle_visibility_all(), &["Update(0)"]),
            (|m| m.toggle_lock_all(), &["Update(0)"]),
            (|m| m.bring_to_front(0), &["Update(0)"]),
            (|m| m.send_to_back(0), &["Update(0)"]),
            (
                |m| {
                    m.delete_drawing(0);
                    m.undo();
                    m.redo();
                },
                &["Delete(a1b2)", "Save(0)", "Delete(a1b2)"],
            ),
            (
                |m| {
                    let rect = DrawingToolType::Rect;
                    m.start_drawing_with_coords(rect, Pos2::new(1.0, 1.0), |x| x, f64::from);
                    m.update_last_point_with_coords(Pos2::new(9.0, 9.0), |x| x, f64::from);
                    m.complete_drag_drawing();
                },
                &["Save(1)"],
            ),
            (
                |m| {
                    let line = DrawingToolType::Polyline;
                    m.start_drawing_with_coords(line, Pos2::new(1.0, 1.0), |x| x, f64::from);
                    m.add_point_with_coords(Pos2::new(9.0, 9.0), |x| x, f64::from);
                    m.complete_curr_drawing();
                },
                &["Save(1)"],
            ),
            (
                |m| m.add_synced_drawing(Drawing::new(7, DrawingToolType::HorizontalLine)),
                &["Save(1)"],
            ),
            (
                |m| m.edit_drawing(0, |d| d.stroke_width = 3.0),
                &["Update(0)"],
            ),
            (
                // A colour picker dragged over 120 frames is one undo step and one update.
                |m| {
                    for red in 0..120 {
                        m.drag_edit_drawing(0, |d| d.color = [red, 0, 0, 255]);
                    }
                    m.end_drag_handle();
                    m.undo();
                    m.undo();
                },
                &["Update(0)", "Update(0)"],
            ),
        ];
        for (edit, expected) in cases {
            let mut manager = DrawingManager::new();
            let mut restored = Drawing::new(5, DrawingToolType::HorizontalLine);
            restored.stored_id = Some("a1b2".into());
            manager.add_synced_drawing(restored);
            edit(&mut manager);
            let queued: Vec<String> = manager
                .pending_sync
                .lock()
                .unwrap()
                .iter()
                .map(|action| match action {
                    DrawingSyncAction::SaveDrawing(id) => format!("Save({id})"),
                    DrawingSyncAction::UpdateDrawing(id) => format!("Update({id})"),
                    DrawingSyncAction::Delete(id) => format!("Delete({id})"),
                    DrawingSyncAction::RestoreDrawings(_) => "Restore".into(),
                })
                .collect();
            assert_eq!(queued, expected);
        }
    }

    /// The click after an icon is picked places that icon, not a default one, and so does every
    /// later click in keep-drawing mode.
    #[test]
    fn a_picked_icon_is_placed_by_the_next_click() {
        for (keep_drawing, clicks) in [(false, 1), (true, 3)] {
            let mut manager = DrawingManager::new();
            manager.stay_in_drawing_mode = keep_drawing;
            manager.start_icon_insertion("🔥".into());
            for i in 0..clicks {
                let tool = manager.active_tool.unwrap();
                let at = Pos2::new(5.0 * (i + 1) as f32, 5.0);
                manager.start_drawing_with_coords(tool, at, |x| x, f64::from);
            }
            let placed: Vec<_> = manager.drawings.iter().map(|d| d.text.as_deref()).collect();
            assert_eq!(
                placed,
                vec![Some("🔥"); clicks],
                "keep drawing {keep_drawing}"
            );
        }
    }

    #[test]
    fn test_selection() {
        let mut manager = DrawingManager::new();
        manager.select(1);
        assert_eq!(manager.sel_drawing(), Some(1));
        manager.deselect();
        assert_eq!(manager.sel_drawing(), None);
    }

    /// A click on a Fibonacci level line, far from every control point, must
    /// select the drawing. This proves selection is routed through the unified
    /// per-level hit testing in the interaction service rather than the old
    /// vertex-proximity fallback that only matched near a control point.
    #[test]
    fn test_hit_test_fibonacci_level_off_control_point() {
        let mut manager = DrawingManager::new();
        let mut fib = Drawing::new(7, DrawingToolType::FibonacciRetracement);
        // Control points: Start (100,100), End (300,200), Middle (200,150).
        fib.chart_points = vec![ChartPoint::new(100.0, 100.0), ChartPoint::new(300.0, 200.0)];
        fib.update_screen_coords(|bar| bar, |price| price as f32);
        fib.completed = true;
        manager.drawings.push(fib);

        // 0.236 level: y = 200 + (100 - 200) * 0.236 = 176.4, sampled at x = 180.
        // This point is more than 30 px from the Start, End and Middle handles.
        let click = Pos2::new(180.0, 176.4);
        for handle in [
            Pos2::new(100.0, 100.0),
            Pos2::new(300.0, 200.0),
            Pos2::new(200.0, 150.0),
        ] {
            let d = ((click.x - handle.x).powi(2) + (click.y - handle.y).powi(2)).sqrt();
            assert!(d > 20.0, "click must be off control points, was {d}px away");
        }

        assert_eq!(manager.hit_test(click), Some(7));
        assert!(manager.select_at(click));
        assert_eq!(manager.sel_drawing(), Some(7));
    }

    /// A click on a parallel-channel segment, far from every control point,
    /// must select the drawing -- again proving per-segment hit testing.
    #[test]
    fn test_hit_test_channel_segment_off_control_point() {
        let mut manager = DrawingManager::new();
        let mut channel = Drawing::new(11, DrawingToolType::ParallelChannel);
        // Main line p1 (100,100) -> p2 (300,100); offset point p3 (200,200).
        channel.points = vec![
            Pos2::new(100.0, 100.0),
            Pos2::new(300.0, 100.0),
            Pos2::new(200.0, 200.0),
        ];
        channel.completed = true;
        manager.drawings.push(channel);

        // Midway along the main line, 50px+ from p1, p2, and p3.
        let click = Pos2::new(150.0, 100.0);
        for handle in [
            Pos2::new(100.0, 100.0),
            Pos2::new(300.0, 100.0),
            Pos2::new(200.0, 200.0),
        ] {
            let d = ((click.x - handle.x).powi(2) + (click.y - handle.y).powi(2)).sqrt();
            assert!(d > 20.0, "click must be off control points, was {d}px away");
        }

        assert_eq!(manager.hit_test(click), Some(11));
    }

    /// Snapping must borrow magnet targets from the live drawings without
    /// allocating a flattened point set on each call.
    #[test]
    fn test_snap_point_borrows_magnet_targets() {
        let mut manager = DrawingManager::new();
        manager.options.magnet_mode = true;
        manager.update_snap_options();

        let mut d = Drawing::new(1, DrawingToolType::TrendLine);
        d.points = vec![Pos2::new(100.0, 100.0), Pos2::new(200.0, 200.0)];
        let drawings = vec![d];

        // A pointer near the first anchor snaps to it under magnet mode.
        let snapped = manager.snap_point(Pos2::new(103.0, 102.0), &drawings);
        assert_eq!(snapped, Pos2::new(100.0, 100.0));
    }

    /// A position's stop is an anchor of its own: it starts mirrored around
    /// the entry, moves without dragging the target along, and its handle
    /// stays at the end of the stop line, which ends where the target line does.
    #[test]
    fn position_stop_moves_independently_of_target() {
        let mut manager = DrawingManager::new();
        let x_to_bar = |x: f32| x;
        let y_to_price = |y: f32| f64::from(200.0 - y);
        manager.start_drawing_with_coords(
            DrawingToolType::LongPos,
            Pos2::new(10.0, 100.0),
            x_to_bar,
            y_to_price,
        );
        assert!(manager.curr_drawing.is_none());
        let id = manager.drawings[0].id;
        assert_eq!(manager.drawings[0].chart_points[2].price, 50.0);

        manager.start_drag_handle(id, HandlePos::Point(2));
        manager.update_drag_handle(Pos2::new(50.0, 105.0), x_to_bar, y_to_price);
        manager.end_drag_handle();
        let points = &manager.drawings[0].chart_points;
        assert_eq!((points[1].price, points[2].price), (150.0, 95.0));
        assert_eq!(points[2].bar_idx, 110.0);

        manager.start_drag_handle(id, HandlePos::Point(1));
        manager.update_drag_handle(Pos2::new(40.0, 85.0), x_to_bar, y_to_price);
        manager.end_drag_handle();
        let points = &manager.drawings[0].chart_points;
        assert_eq!((points[2].bar_idx, points[2].price), (40.0, 95.0));
    }
}
