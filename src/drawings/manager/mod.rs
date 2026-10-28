//! Drawing state, selection, editing and undo history.
//!
//! Use [`DrawingManager`] through the chart's drawing accessors, or directly when
//! composing your own renderer. Direct users synchronize screen coordinates with
//! [`DrawingManager::update_all_screen_coords`] after changing the viewport.

use crate::drawings::domain::{ChartPoint, Drawing, DrawingToolType, HandlePos};
use crate::drawings::services::{
    HandleConfig, HistoryService, SelectionService, SnapOptions, SnapService,
};
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::ClippedShape;
use egui::{Pos2, Rect};
use std::sync::{Arc, Mutex};

mod edit;
mod interaction;
mod paint;

use paint::Tessellated;

/// A change to the stored drawings, queued in [`DrawingManager::pending_sync`].
///
/// When enabled, every [`DrawingManager`] method that changes a stored drawing field queues
/// an action per drawing it changes: creating, moving, styling, locking, hiding, layering,
/// deleting, undo and redo. Restoring a stored drawing with
/// [`DrawingManager::add_synced_drawing`] queues nothing. Repeated saves and updates coalesce
/// until that drawing is deleted. The host drains the queue and saves or deletes each drawing it names,
/// skipping saves of drawings already deleted, for example with
/// [`drawing_to_stored`](crate::drawings::persistence::drawing_to_stored). A host that writes
/// to [`DrawingManager::drawings`] directly queues nothing. Automatic chart storage disables
/// these notifications; use [`DrawingManager::set_sync_enabled`] to consume both paths.
#[derive(Debug, Clone)]
#[non_exhaustive]
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
    /// Include visible study plots in magnet targets. Independent of OHLC and drawing anchors.
    pub snap_to_indicators: bool,
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
            snap_to_indicators: true,
            handle_config: HandleConfig::default(),
        }
    }
}

/// Last painted geometry, invalid once its anchors have moved.
struct PaintedDrawing {
    id: usize,
    points: Vec<Pos2>,
    shapes: Vec<ClippedShape>,
    /// The tessellation of `shapes`, made once a second frame paints the same shapes.
    tessellated: Option<Tessellated>,
}

/// Draft text stays separate from the drawing until the editor commits it.
pub(crate) struct TextEditState {
    pub id: usize,
    pub draft: String,
    pub focus: bool,
    pub opened_frame: Option<u64>,
}

/// Central coordinator for chart drawings.
///
/// `DrawingManager` owns the list of completed drawings and the in-progress
/// drawing, and delegates business logic to specialized services:
///
/// - **Selection** via [`SelectionService`]
/// - **Undo/redo** via [`HistoryService`]
/// - **Snapping** via [`SnapService`]
/// - **Handle manipulation** via [`HandleService`](crate::drawings::services::HandleService)
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
/// let view = ChartView { instrument: None, bars, revision, bar_to_x: &bar_to_x, price_to_y: &price_to_y, time_axis: true, executions: None };
/// manager.render_all(painter, price_rect, &view);
/// # }
/// ```
pub struct DrawingManager {
    /// Host tools and built-in availability. Register tools before restoring their drawings.
    pub registry: super::DrawingToolRegistry,
    active_custom_tool: Option<String>,
    /// Per-tool defaults, exportable with serde for host-owned application storage.
    /// New drawings use these properties without copying geometry, identity or layer.
    pub tool_defaults: std::collections::HashMap<DrawingToolType, super::DrawingTemplate>,
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

    /// Pane used to project and drag screen-anchored annotations.
    pane_rect: Rect,
    pane: Option<usize>,
    pane_members: Vec<Option<usize>>,
    pane_clips: std::collections::HashMap<Option<usize>, Rect>,
    pub(crate) text_editor: Option<TextEditState>,

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
    /// Other selected drawings before a body translation.
    drag_group: Vec<Drawing>,

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
    /// Whether edits emit per-drawing storage notifications.
    pub(crate) sync_enabled: bool,
}

impl Default for DrawingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingManager {
    /// Enables or disables per-drawing storage notifications. Disabling clears pending actions.
    /// Automatic chart storage disables them; enable them again to also consume notifications.
    /// Changes made while disabled are not reported.
    pub fn set_sync_enabled(&mut self, enabled: bool) {
        self.sync_enabled = enabled;
        if !enabled {
            self.pending_sync
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clear();
        }
    }

    /// Creates a new `DrawingManager` with default options and empty drawing list.
    pub fn new() -> Self {
        Self {
            registry: Default::default(),
            active_custom_tool: None,
            drawings: Vec::new(),
            tool_defaults: Default::default(),
            active_tool: None,
            curr_drawing: None,
            preview_point: false,
            next_id: 0,
            pane_rect: Rect::NOTHING,
            pane: None,
            pane_members: vec![None],
            pane_clips: std::collections::HashMap::new(),
            text_editor: None,
            selection: SelectionService::new(),
            history: HistoryService::new(),
            snap_service: SnapService::new(),
            painted: Mutex::new(Vec::new()),
            snap_targets: Vec::new(),
            options: DrawingManagerOptions::default(),
            dragging_handle: None,
            drag_old_state: None,
            drag_origin: None,
            drag_group: Vec::new(),
            stay_in_drawing_mode: false,
            curr_timeframe: String::from("1D"),
            eraser_mode: false,
            eraser_hover_drawing: None,
            pending_sync: Arc::new(Mutex::new(Vec::new())),
            sync_enabled: true,
        }
    }

    /// Apply the host's tool, snapping, color, eraser and selection state.
    pub fn sync_from_app_state(&mut self, drawing_state: &DrawingState) {
        if self.active_tool != drawing_state.active_tool {
            self.set_active_tool(drawing_state.active_tool);
        }
        self.options.magnet_mode = drawing_state.magnet_mode;
        self.options.default_color = drawing_state.curr_color;
        self.stay_in_drawing_mode = drawing_state.stay_in_drawing_mode;
        self.eraser_mode = drawing_state.eraser_mode;

        // Sync selection
        match (drawing_state.sel_drawing, self.selection.primary()) {
            (Some(id), current) if current != Some(id) => {
                self.select(id);
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
        self.selection
            .primary()
            .filter(|id| self.is_selected(*id))
            .or_else(|| self.selected_ids().next())
    }

    /// Selects the drawing with the given ID (deselects all others).
    pub fn select(&mut self, id: usize) {
        self.select_drawings([id]);
    }

    /// Replace the selected drawings with the supplied live ids.
    pub fn select_drawings(&mut self, ids: impl IntoIterator<Item = usize>) {
        self.selection.deselect_all();
        self.selection.select_all(
            ids.into_iter()
                .filter(|id| self.drawings.iter().any(|d| d.id == *id && d.selectable)),
        );
    }

    /// Selected live drawing ids, in drawing-list order.
    pub fn selected_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.drawings
            .iter()
            .filter(|d| d.selectable && self.selection.is_selected(d.id))
            .map(|d| d.id)
    }

    /// Whether a drawing belongs to the current selection.
    pub fn is_selected(&self, id: usize) -> bool {
        self.drawings.iter().any(|d| d.id == id && d.selectable) && self.selection.is_selected(id)
    }

    /// Toggle a live drawing in the selection, retaining the other selected drawings.
    pub fn toggle_selection(&mut self, id: usize) {
        if self.drawings.iter().any(|d| d.id == id && d.selectable) {
            self.selection.toggle(id);
        }
    }

    /// Apply a style edit to every unlocked selected drawing as one undo step.
    pub fn edit_selected(&mut self, mut edit: impl FnMut(&mut Drawing)) {
        let before: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| d.selectable && self.selection.is_selected(d.id) && !d.locked)
            .cloned()
            .collect();
        if before.is_empty() {
            return;
        }
        let snapshot = self.drawings.clone();
        for old in &before {
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == old.id) {
                edit(drawing);
            }
            self.queue_edit(old);
        }
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !before.iter().any(|d| d.id == p.id));
        self.history.push_snapshot(snapshot);
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
            self.select(id);
            true
        } else {
            self.selection.deselect_all();
            false
        }
    }

    // === History delegation ===

    /// Exchange the standalone history with a chart user action's drawing commands.
    pub(crate) fn swap_history(&mut self, history: &mut HistoryService) {
        std::mem::swap(&mut self.history, history);
    }

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
        let before: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| d.saveable)
            .map(|d| (d.id, sync_id(d)))
            .collect();
        let Some(_) = step(&mut self.history, &mut self.drawings) else {
            return false;
        };
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !self.history.affected_ids().contains(&p.id));
        for &id in self.history.affected_ids() {
            let was = before.iter().find(|&&(i, _)| i == id);
            match (was, self.drawings.iter().any(|d| d.id == id && d.saveable)) {
                (Some(_), true) => self.queue(DrawingSyncAction::UpdateDrawing(id)),
                (Some((_, stored_id)), false) => {
                    self.queue(DrawingSyncAction::Delete(stored_id.clone()))
                }
                (None, true) => self.queue(DrawingSyncAction::SaveDrawing(id)),
                (None, false) => {}
            }
        }
        self.selection
            .prune(&self.drawings.iter().map(|d| d.id).collect());
        true
    }

    /// Queues a change for the host to save.
    fn queue(&self, action: DrawingSyncAction) {
        if !self.sync_enabled {
            return;
        }
        let drawing = match &action {
            DrawingSyncAction::SaveDrawing(id) | DrawingSyncAction::UpdateDrawing(id) => {
                self.drawings.iter().find(|drawing| drawing.id == *id)
            }
            _ => None,
        };
        if drawing.is_some_and(|drawing| !drawing.saveable) {
            return;
        }
        if let Ok(mut sync) = self.pending_sync.lock() {
            queue_sync(&mut sync, action, drawing.map(sync_id).as_deref());
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
                            && self.pane_members.contains(&d.pane)
                            && self.dragging_handle.is_none_or(|(id, _)| id != d.id)
                            && self.drag_group.iter().all(|old| old.id != d.id)
                    })
                    .flat_map(|d| d.points.iter().copied()),
            ),
        )
    }

    fn queue_edit(&self, old: &Drawing) {
        if let Some(drawing) = self.drawings.iter().find(|d| d.id == old.id) {
            self.queue(match (old.saveable, drawing.saveable) {
                (true, false) => DrawingSyncAction::Delete(sync_id(old)),
                (false, true) => DrawingSyncAction::SaveDrawing(old.id),
                _ => DrawingSyncAction::UpdateDrawing(old.id),
            });
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

    pub(crate) fn visit_chart_points(&mut self, visit: &mut impl FnMut(&mut ChartPoint)) {
        for drawing in self
            .drawings
            .iter_mut()
            .chain(&mut self.curr_drawing)
            .chain(&mut self.drag_old_state)
            .chain(&mut self.drag_group)
        {
            drawing.chart_points.iter_mut().for_each(&mut *visit);
        }
        self.history.visit_chart_points(visit);
    }
}

/// Coalesces repeated saves and updates of a drawing without reordering them across its delete.
fn queue_sync(
    sync: &mut Vec<DrawingSyncAction>,
    action: DrawingSyncAction,
    stored_id: Option<&str>,
) {
    if let DrawingSyncAction::SaveDrawing(id) | DrawingSyncAction::UpdateDrawing(id) = &action {
        for pending in sync.iter_mut().rev() {
            match pending {
                DrawingSyncAction::Delete(deleted) if Some(deleted.as_str()) == stored_id => break,
                DrawingSyncAction::SaveDrawing(other) | DrawingSyncAction::UpdateDrawing(other)
                    if other == id =>
                {
                    if matches!(action, DrawingSyncAction::SaveDrawing(_)) {
                        *pending = action;
                    }
                    return;
                }
                _ => {}
            }
        }
    }
    sync.push(action);
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
        for bulk in [false, true] {
            let edit = |manager: &mut DrawingManager, change: fn(&mut Drawing)| {
                if bulk {
                    manager.edit_drawings(&[3], change);
                } else {
                    manager.edit_drawing(3, change);
                }
            };
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
            edit(&mut manager, |d| d.font_size = 30.0);
            manager.drag_edit_drawing(3, |d| d.font_size = 40.0);
            manager.end_drag_handle();
            assert_eq!(manager.drawings[0].font_size, 12.0);
            assert!(!manager.can_undo());
            assert!(manager.pending_sync.lock().unwrap().is_empty());
            // Tree metadata can change without unlocking geometry or style edits.
            edit(&mut manager, |d| {
                d.visible = false;
                d.label = Some("Support".into());
                d.folder = vec!["Analysis".into()];
                d.font_size = 30.0;
            });
            assert!(!manager.drawings[0].visible);
            assert_eq!(manager.drawings[0].label.as_deref(), Some("Support"));
            assert_eq!(manager.drawings[0].folder, ["Analysis"]);
            assert_eq!(manager.drawings[0].font_size, 12.0);
            assert!(manager.drawings[0].locked);
            assert!(manager.undo());
            assert!(manager.drawings[0].visible);
            assert_eq!(manager.drawings[0].label, None);
            assert!(manager.drawings[0].folder.is_empty());
            assert!(manager.drawings[0].locked);
            edit(&mut manager, |d| d.locked = false);
            edit(&mut manager, |d| d.font_size = 30.0);
            assert_eq!(manager.drawings[0].font_size, 30.0);

            assert_eq!(manager.drawings.len(), 1);
            assert_eq!(
                manager.drawings[0].chart_points[0],
                ChartPoint::new(10.0, 10.0)
            );
        }
    }

    #[test]
    fn bulk_edits_preserve_unrelated_host_changes_and_drawing_order() {
        for edit in [
            (|d: &mut Drawing| d.folder = vec!["Analysis".into()]) as fn(&mut Drawing),
            |d| d.visible = false,
            |d| d.locked = true,
        ] {
            let mut manager = DrawingManager::new();
            manager.drawings = (0..3)
                .map(|id| Drawing::new(id, DrawingToolType::HorizontalLine))
                .collect();
            manager.edit_drawings(&[2], edit);
            manager.pending_sync.lock().unwrap().clear();
            // Host updates outside this edit must survive both history directions.
            manager.drawings[0].label = Some("Host update".into());
            manager.drawings.remove(1);
            manager
                .drawings
                .push(Drawing::new(3, DrawingToolType::HorizontalLine));
            for step in [DrawingManager::undo, DrawingManager::redo] {
                assert!(step(&mut manager));
                assert_eq!(
                    manager.drawings.iter().map(|d| d.id).collect::<Vec<_>>(),
                    [0, 2, 3]
                );
                assert_eq!(manager.drawings[0].label.as_deref(), Some("Host update"));
                assert!(matches!(
                    manager.pending_sync.lock().unwrap().as_slice(),
                    [DrawingSyncAction::UpdateDrawing(2)]
                ));
            }
        }
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

        // Pasting a lower layer puts the copy above existing drawings too.
        manager.paste_drawings(&[manager.drawings[1].clone()]);
        manager.update_all_screen_coords(|bar| bar, |price| price as f32);
        let copy_id = manager.drawings.last().unwrap().id;
        assert_eq!(manager.hit_test(on_both), Some(copy_id));
        assert!(manager.undo());
        assert_eq!(manager.hit_test(on_both), Some(fresh_id));
    }

    /// Stored-field edits coalesce into notifications of the current drawing state.
    /// Restoring a stored drawing queues nothing,
    /// and deleting it names the record it is stored under.
    #[test]
    fn edits_queue_coalesced_sync_actions() {
        type Edit = fn(&mut DrawingManager);
        let cases: [(Edit, &[&str]); 15] = [
            (
                |m| {
                    m.duplicate_drawing(0);
                    m.undo();
                    m.redo();
                },
                &["Save(1)", "Delete(1)", "Save(1)"],
            ),
            (
                |m| {
                    m.add_synced_drawing(Drawing::new(1, DrawingToolType::HorizontalLine));
                    m.reorder_drawings(0, &[(0, 1), (1, 2)]);
                    m.undo();
                    m.redo();
                },
                &["Save(1)", "Update(0)"],
            ),
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
                &["Update(0)"],
            ),
            (
                |m| {
                    for i in 0..10_000 {
                        m.edit_drawing(0, |d| d.stroke_width = 2.0 + (i % 2) as f32);
                    }
                    m.toggle_visibility_all();
                    m.toggle_lock_all();
                },
                &["Update(0)"],
            ),
            (
                |m| {
                    m.edit_drawing(0, |d| d.stroke_width = 2.0);
                    m.add_synced_drawing(Drawing::new(1, DrawingToolType::HorizontalLine));
                    m.delete_drawing(1);
                    m.edit_drawing(0, |d| d.stroke_width = 3.0);
                },
                &["Update(0)", "Save(1)", "Delete(1)"],
            ),
        ];
        for (case, (edit, expected)) in cases.into_iter().enumerate() {
            let mut manager = DrawingManager::new();
            let mut restored = Drawing::new(5, DrawingToolType::HorizontalLine);
            restored.stored_id = Some("a1b2".into());
            manager.add_synced_drawing(restored);
            let before = manager
                .drawings
                .iter()
                .map(|drawing| (drawing.id, serde_json::to_value(drawing).unwrap()))
                .collect::<Vec<_>>();
            edit(&mut manager);
            let after = manager
                .drawings
                .iter()
                .map(|drawing| (drawing.id, serde_json::to_value(drawing).unwrap()))
                .collect::<Vec<_>>();
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
            if matches!(case, 0 | 2..=6 | 10 | 11) {
                assert!(manager.undo(), "case {case} must be undoable");
                assert_eq!(
                    manager
                        .drawings
                        .iter()
                        .map(|drawing| (drawing.id, serde_json::to_value(drawing).unwrap()))
                        .collect::<Vec<_>>(),
                    before,
                    "case {case}: undo"
                );
                assert!(!manager.can_undo(), "case {case}: one command per edit");
                assert!(manager.redo(), "case {case}");
                assert_eq!(
                    manager
                        .drawings
                        .iter()
                        .map(|drawing| (drawing.id, serde_json::to_value(drawing).unwrap()))
                        .collect::<Vec<_>>(),
                    after,
                    "case {case}: redo"
                );
            }
        }
    }

    /// The click after an icon is picked places that icon, not a default one, and so does every
    /// later click in keep-drawing mode.
    #[test]
    fn a_picked_icon_is_placed_by_the_next_click() {
        for glyph in [
            "icon:check",
            "icon:star",
            "😀",
            "👍",
            "🔥",
            "🍎",
            "🚀",
            "📌",
            "⭐",
            "sticker:bull",
        ] {
            for (keep_drawing, clicks) in [(false, 1), (true, 3)] {
                let mut manager = DrawingManager::new();
                manager.stay_in_drawing_mode = keep_drawing;
                manager.start_icon_insertion(glyph.into());
                manager.curr_drawing.as_mut().unwrap().font_size = 48.0;
                for i in 0..clicks {
                    let tool = manager.active_tool.unwrap();
                    assert_eq!(
                        tool,
                        if glyph.starts_with("sticker:") {
                            DrawingToolType::Sticker
                        } else {
                            DrawingToolType::FontIcon
                        }
                    );
                    let at = Pos2::new(5.0 * (i + 1) as f32, 5.0);
                    manager.start_drawing_with_coords(tool, at, |x| x, f64::from);
                }
                let placed: Vec<_> = manager
                    .drawings
                    .iter()
                    .map(|d| (d.text.as_deref(), d.font_size))
                    .collect();
                assert_eq!(
                    placed,
                    vec![(Some(glyph), 48.0); clicks],
                    "{glyph}, keep drawing {keep_drawing}"
                );
            }
        }
    }

    #[test]
    fn test_selection() {
        let mut manager = DrawingManager::new();
        manager.select(1);
        assert_eq!(manager.sel_drawing(), None);
        manager.deselect();
        assert_eq!(manager.sel_drawing(), None);
        manager.drawings = (1..=3)
            .map(|id| Drawing::new(id, DrawingToolType::TrendLine))
            .collect();
        manager.drawings[2].selectable = false;
        manager.drawings[2].chart_points =
            vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(100.0, 10.0)];
        manager.update_all_screen_coords(|x| x, |y| y as f32);
        assert_eq!(manager.hit_test(Pos2::new(50.0, 10.0)), None);
        assert_eq!(
            manager.hit_test_handle(&manager.drawings[2], Pos2::new(0.0, 10.0)),
            None
        );
        manager.select_drawings([1, 2, 3, 99]);
        assert_eq!(manager.selection.selection_count(), 2);
        manager.set_group(&[1, 2], Some("levels".into()));
        manager.select_group("levels");
        assert_eq!(manager.selected_ids().collect::<Vec<_>>(), [1, 2]);
        assert!(manager.undo());
        assert!(manager.group_drawings("levels").next().is_none());
        assert!(manager.redo());
        assert_eq!(manager.group_drawings("levels").count(), 2);
        manager.edit_selected(|drawing| {
            if drawing.id == 1 {
                drawing.selectable = false;
            }
        });
        assert_eq!(manager.sel_drawing(), Some(2));
        assert_eq!(manager.selected_ids().collect::<Vec<_>>(), [2]);
        assert!(manager.undo());
        manager.select_group("levels");
        manager.delete_selected();
        assert_eq!(
            manager.drawings.iter().map(|d| d.id).collect::<Vec<_>>(),
            [3]
        );
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

        // A group being translated cannot magnetize to another member's old position.
        manager.drawings = drawings;
        let mut peer = Drawing::new(2, DrawingToolType::TrendLine);
        peer.points = vec![Pos2::new(300.0, 300.0), Pos2::new(400.0, 400.0)];
        manager.drawings.push(peer);
        manager.select_drawings([1, 2]);
        manager.start_drag_body(1, Pos2::new(150.0, 150.0));
        let pointer = Pos2::new(303.0, 302.0);
        assert_eq!(manager.snap_point(pointer, &manager.drawings), pointer);
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
