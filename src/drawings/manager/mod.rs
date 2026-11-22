//! Drawing state, selection, editing and undo history.
//!
//! Use [`DrawingManager`] through the chart's drawing accessors, or directly when
//! composing your own renderer. Direct users synchronize screen coordinates with
//! [`DrawingManager::update_all_screen_coords`] after changing the viewport.

use crate::drawings::domain::{ChartPoint, Drawing, DrawingToolType, HandlePos, TableConfig};
use crate::drawings::services::{
    HandleConfig, HistoryService, SelectionService, SnapOptions, SnapService,
};
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::ClippedShape;
use egui::{Pos2, Rect};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

mod edit;
mod interaction;
mod paint;

use interaction::PartialErase;
pub(crate) use paint::DrawingAxisTag;
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
///
/// These actions say what to store. [`ChartEvent::Drawing`](crate::widget::ChartEvent::Drawing)
/// says what happened to each drawing, by its id and without coalescing.
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

/// What happened to a drawing, reported by
/// [`ChartEvent::Drawing`](crate::widget::ChartEvent::Drawing).
///
/// One edit of one drawing can report several kinds, in the order of this list: `Hidden` or
/// `Shown`, then `Moved` or `AnchorsChanged`, then `PropertiesChanged`. An edit that leaves the
/// drawing as it was reports nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DrawingEventKind {
    /// The drawing joined [`DrawingManager::drawings`]: completed with a tool, pasted,
    /// duplicated, copied by a drag, added by the host, left over by a partial erase, or brought
    /// back by undo or redo. Drawings restored from storage are not reported.
    Created,
    /// The drawing left [`DrawingManager::drawings`]: deleted, erased, removed with its pane, or
    /// taken out by undo or redo.
    Removed,
    /// [`Drawing::visible`] turned off. Hiding every drawing with
    /// [`DrawingManager::set_hidden`] changes no drawing and reports nothing.
    Hidden,
    /// [`Drawing::visible`] turned on.
    Shown,
    /// The drawing was moved as a whole: its body or middle handle dragged, the arrow keys pressed
    /// while it was selected, or such a move undone or redone. Anchors that stay at the point they
    /// annotate, such as a callout's, keep their place.
    Moved,
    /// Its anchors changed in any other way, such as a dragged handle, a partial erase or a host
    /// edit of [`Drawing::chart_points`].
    AnchorsChanged,
    /// Any other saved setting changed, such as its style, text, lock, layer, pane, group,
    /// folder, name, notes or the intervals it shows on.
    PropertiesChanged,
}

/// Snapshot of drawing-related state from the central application state.
///
/// This struct is passed to [`DrawingManager::sync_from_app_state`] at the start
/// of each frame to ensure the manager stays in sync with toolbar selections,
/// keyboard shortcuts, and other UI actions that modify drawing state.
///
/// ```
/// use egui_charts::drawings::services::HandleConfig;
/// use egui_charts::drawings::{
///     DrawingManager, DrawingManagerOptions, DrawingState, DrawingToolType, HandlePos,
/// };
///
/// let mut state = DrawingState::default();
/// state.active_tool = Some(DrawingToolType::TrendLine);
/// let mut handles = HandleConfig::default();
/// handles.size = 10.0;
/// let mut options = DrawingManagerOptions::default();
/// options.snap_to_time = false;
/// options.handle_config = handles;
/// let mut manager = DrawingManager::new();
/// manager.options = options;
/// manager.sync_from_app_state(&state);
/// manager.start_drag_handle(1, HandlePos::End);
/// ```
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DrawingState {
    /// Currently selected drawing tool. `None` means selection/pointer mode.
    pub active_tool: Option<DrawingToolType>,
    /// ID of the currently selected (highlighted) drawing on the chart.
    pub sel_drawing: Option<usize>,
    /// Whether magnet mode is enabled (snap to OHLC prices and existing drawing points).
    pub magnet_mode: bool,
    /// Whether to stay in drawing mode after completing a drawing (lock mode).
    pub stay_in_drawing_mode: bool,
    /// Line colour every new drawing takes instead of its tool's own, as RGBA bytes; `None`
    /// keeps each tool's default.
    pub curr_color: Option<[u8; 4]>,
    /// Whether eraser mode is active (click to delete drawings).
    pub eraser_mode: bool,
}

/// Configuration options for the [`DrawingManager`].
///
/// Controls default colors, snapping behavior, magnet mode thresholds, and
/// selection handle appearance.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DrawingManagerOptions {
    /// Line colour every new drawing takes instead of its tool's own, as RGBA bytes. `None`
    /// (the default) keeps each tool's colour; a default saved for a tool wins over both.
    pub default_color: Option<[u8; 4]>,
    /// Put drawings placed, pasted or cloned on this chart in a top-level
    /// [chart-only folder](super::FolderSync::Chart) of this name. `None` (the default) leaves
    /// new drawings at the root and copies in their original's folder. If the name is already
    /// used by a non-local folder, use a numbered folder instead of changing its scope.
    /// Move a drawing out of the folder to share it under the usual rules. Existing
    /// drawings, including those read from storage, are unaffected.
    pub local_placement_folder: Option<String>,
    /// Place and move anchors on whole bars (the default). `false` keeps the fractional bar
    /// under the pointer, as freeform tools do; a magnet target keeps its own bar either way.
    pub snap_to_time: bool,
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
            default_color: None,
            local_placement_folder: None,
            snap_to_time: true,
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
    start: usize,
    points: Vec<Pos2>,
    shapes: Vec<ClippedShape>,
    /// Where the handles of a drawing that fits itself to the bars sit, or a table's grid edges.
    handle_points: Option<Vec<Pos2>>,
    /// The tessellation of `shapes`, made once a second frame paints the same shapes.
    tessellated: Option<Tessellated>,
}

/// Draft text stays separate from the drawing until the editor commits it.
pub(crate) struct TextEditState {
    pub id: usize,
    pub draft: String,
    pub focus: bool,
    pub opened_frame: Option<u64>,
    /// Placed just before editing, so empty content can be removed without an undo step.
    pub placed: bool,
    /// Paint the draft on the drawing and put the editor over its text.
    pub inline: bool,
    /// A retracement level whose custom text is being edited.
    pub fib_level: Option<usize>,
    /// The table settings being edited with a table's cells; the defaults for other tools.
    pub table: TableConfig,
}

impl TextEditState {
    /// The draft, or a localised placeholder while it is empty.
    pub(crate) fn shown_text(&self, ctx: &egui::Context) -> String {
        if self.draft.is_empty() {
            crate::localization::text(ctx, "Add text")
        } else {
            self.draft.clone()
        }
    }

    /// The drawing with its draft, or a neutral placeholder while it is empty.
    pub(crate) fn preview(&self, drawing: &Drawing, ctx: &egui::Context) -> Drawing {
        let mut shown = drawing.clone();
        if let Some(index) = self.fib_level {
            let mut config = drawing.fibonacci_config().into_owned();
            if let Some(level) = config.levels.get_mut(index) {
                level.text = self.shown_text(ctx);
            }
            shown.fib_config = Some(config);
            return shown;
        }
        if self.draft.is_empty() {
            shown.text_color = Some(
                ctx.global_style()
                    .visuals
                    .weak_text_color()
                    .to_srgba_unmultiplied(),
            );
        }
        shown.text = Some(self.shown_text(ctx));
        // A pin whose popup is switched off still shows the text being written.
        shown.marker.show_text = true;
        shown
    }
}

/// Display state of one chart, not part of any drawing or its stored document.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SharedDrawingFilter {
    hidden: bool,
    pub(crate) symbol_storage: bool,
}

impl SharedDrawingFilter {
    pub(crate) fn hides(self, drawing: &Drawing) -> bool {
        self.hidden
            && match drawing.folder_sync_mode() {
                Some(super::FolderSync::AllLayouts) => true,
                Some(_) => false,
                None => self.symbol_storage && drawing.saveable,
            }
    }

    fn selectable(self, drawing: &Drawing) -> bool {
        drawing.selectable && !self.hides(drawing)
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
/// #     y_to_price: impl Fn(f32) -> f64 + Copy,
/// #     bar_spacing: f32,
/// # ) {
/// // 1. Sync state from app
/// manager.sync_from_app_state(drawing_state);
///
/// // 2. Update screen coordinates from chart coordinates
/// manager.update_all_screen_coords(bar_to_x, price_to_y, bar_spacing);
///
/// // 3. Handle user input (clicks, drags, keyboard)
/// // ... (start_drawing_with_coords, add_point_with_coords, etc.)
///
/// // 4. Render against the chart's bars and coordinate mapping
/// let view = ChartView { price_formatter: None, thousands_sep: None, time_formatter: None, bar_seconds: 0, instrument: None, bars, revision, bar_to_x: &bar_to_x, bar_spacing, price_to_y: &price_to_y, y_to_price: &y_to_price, time_axis: true, executions: None, timeframe: None, finer: &[] };
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
    /// Defaults of registered tools by tool id, exported beside [`Self::tool_defaults`].
    /// A template naming another tool in `settings.custom_tool` never applies.
    pub custom_tool_defaults: std::collections::HashMap<String, super::DrawingTemplate>,
    /// All completed drawings on the chart.
    pub drawings: Vec<Drawing>,
    /// Currently active drawing tool, or `None` for selection/pointer mode.
    pub(crate) active_tool: Option<DrawingToolType>,
    /// The drawing currently being created (not yet completed).
    pub(crate) curr_drawing: Option<Drawing>,
    /// Whether the last point of `curr_drawing` follows the pointer rather than a click.
    preview_point: bool,
    /// The last finished measurement, shown until the next click on the chart. It is never
    /// added to `drawings`, saved or recorded for undo.
    pub(crate) measurement: Option<Drawing>,
    /// Next available drawing ID (monotonically increasing).
    next_id: usize,

    /// Pane used to project and drag screen-anchored annotations.
    pane_rect: Rect,
    pane: Option<usize>,
    pane_members: Vec<Option<usize>>,
    /// Removed owners of filtered drawings. Their drawings use the price pane locally until
    /// the owner returns; the layout mirror must never see a change of the drawing's owner.
    pub(crate) removed_owners: Vec<usize>,
    pub(crate) pane_clips: std::collections::HashMap<Option<usize>, Rect>,
    /// Editor-only settings must not grow every idle chart's inline state.
    pub(crate) text_editor: Option<Box<TextEditState>>,
    /// A completed text placement whose add command may already be in chart history.
    pub(crate) finished_text_placement: Option<usize>,
    /// A newly placed image waiting for a host file request at the end of this frame.
    pub(crate) image_request: Option<usize>,
    /// Post and idea cards the user placed before their content was known, with the time of the
    /// anchor they were placed at and that anchor. The chart moves each to the time its content
    /// was published once the content arrives, unless the card was moved or locked first.
    pub(crate) unplaced_posts: Vec<(usize, Option<chrono::DateTime<chrono::Utc>>, ChartPoint)>,
    /// The frame of the last primary click the active tool took. The chart reports no double
    /// click either of whose clicks a tool took, such as the one that finishes a path.
    pub(crate) placement_click: Option<u64>,

    // Services
    selection: SelectionService,
    history: HistoryService,
    snap_service: SnapService,
    /// Geometry from the last paint, including laid-out text and data-driven shapes.
    painted: Mutex<Vec<PaintedDrawing>>,
    /// Points where a series or study is drawn over drawings: no drawing whose `series_layer`
    /// is below the count is hit there. Set by the chart where it hit tests its series.
    covers: Mutex<Vec<(Pos2, usize)>>,
    /// Nearby bar and overlay points, refreshed by the chart before pointer handling.
    pub(crate) snap_targets: Vec<Pos2>,
    /// While a drawing tool is active over the price pane: the frame, the pointer height and the
    /// height a click there gives the next anchor, which the price pane's crosshair shows.
    pub(crate) anchor_crosshair: Option<(u64, f32, f32)>,

    /// Manager configuration (colors, snapping, handles).
    pub options: DrawingManagerOptions,

    /// Currently dragged handle, if any: `(drawing_id, handle_position)`.
    pub(crate) dragging_handle: Option<(usize, HandlePos)>,
    /// Snapshot of the drawing state before the current drag began (for undo).
    drag_old_state: Option<Drawing>,
    /// Pointer origin of a body or centre-handle translation.
    drag_origin: Option<Pos2>,
    /// Other selected drawings before a body translation.
    drag_group: Vec<Drawing>,
    /// The current drag creates copies instead of modifying the original selection.
    drag_copy: bool,
    /// The Ctrl or Command eraser drag, recorded as one undo step when it ends.
    pub(crate) partial_erase: Option<PartialErase>,
    /// Current chart input, separate from the toolbar magnet setting.
    pub(crate) interaction_modifiers: egui::Modifiers,
    /// A line tool chosen by its shortcut this frame, placed by the pane under the pointer.
    pub(crate) place_at_pointer: bool,

    /// Whether to stay in drawing mode after completing a drawing.
    /// A finished measurement always returns to selection mode.
    pub stay_in_drawing_mode: bool,
    /// Current chart timeframe string (e.g., `"1D"`, `"1h"`): drawings hidden on it are
    /// neither drawn nor hit. [`Chart::set_timeframe`](crate::widget::Chart::set_timeframe)
    /// keeps it in step with the chart.
    pub curr_timeframe: String,
    /// Whether a click deletes the drawing under the pointer instead of selecting it, and a
    /// Ctrl or Command drag erases the parts of brush and highlighter strokes it passes over.
    /// Set it with [`Self::set_eraser_mode`].
    pub(crate) eraser_mode: bool,
    /// Whether every drawing is hidden as a group; see [`Self::set_hidden`].
    hidden: bool,
    pub(crate) shared_filter: SharedDrawingFilter,
    /// Whether every drawing is locked as a group; see [`Self::set_locked`].
    locked: bool,

    /// Changes to the stored drawings, oldest first, for the host to drain, for example
    /// with `std::mem::take(&mut *manager.pending_sync.lock().unwrap())`. See
    /// [`DrawingSyncAction`].
    pub pending_sync: Arc<Mutex<Vec<DrawingSyncAction>>>,
    /// Whether edits emit per-drawing storage notifications.
    pub(crate) sync_enabled: bool,
    /// Drawing changes for the chart to report as `ChartEvent::Drawing`, oldest first, at most
    /// 1024: a manager outside a chart keeps only the newest.
    pub(crate) events: std::collections::VecDeque<(usize, DrawingEventKind)>,
    /// Changes left out of `events` once it held 1024.
    pub(crate) events_dropped: usize,
    /// Drawings changed by a series removal, reported once after their owner and layer settle.
    removal: Option<Vec<usize>>,
    /// Tells this manager from one that replaced it, such as `Chart::load_drawings` makes, whose
    /// drawing ids start again: unique to each manager, and kept when the manager moves or reads
    /// its drawings from storage again.
    pub(crate) identity: u64,
    /// Indices the owning chart can date for saving an anchor without its restored time.
    /// `None` outside a chart or while the chart has no bars.
    pub(crate) mapped_bars: Option<std::ops::RangeInclusive<f32>>,
    /// The chart mapping used for `mapped_bars`, to avoid recomputing unchanged bounds.
    pub(crate) mapped_key: Option<crate::drawings::validation::MappingKey>,
}

impl std::fmt::Debug for DrawingManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrawingManager")
            .field("active_tool", &self.active_tool)
            .field("drawings", &self.drawings.len())
            .finish_non_exhaustive()
    }
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
        let mut manager = Self {
            registry: Default::default(),
            active_custom_tool: None,
            drawings: Vec::new(),
            tool_defaults: Default::default(),
            custom_tool_defaults: Default::default(),
            active_tool: None,
            curr_drawing: None,
            preview_point: false,
            measurement: None,
            next_id: 0,
            pane_rect: Rect::NOTHING,
            pane: None,
            pane_members: vec![None],
            removed_owners: Vec::new(),
            pane_clips: std::collections::HashMap::new(),
            text_editor: None,
            finished_text_placement: None,
            image_request: None,
            unplaced_posts: Vec::new(),
            placement_click: None,
            selection: SelectionService::new(),
            history: HistoryService::new(),
            snap_service: SnapService::new(),
            painted: Mutex::new(Vec::new()),
            covers: Mutex::new(Vec::new()),
            snap_targets: Vec::new(),
            anchor_crosshair: None,
            options: DrawingManagerOptions::default(),
            dragging_handle: None,
            drag_old_state: None,
            drag_origin: None,
            drag_group: Vec::new(),
            drag_copy: false,
            partial_erase: None,
            interaction_modifiers: egui::Modifiers::NONE,
            place_at_pointer: false,
            stay_in_drawing_mode: false,
            curr_timeframe: String::from("1D"),
            eraser_mode: false,
            hidden: false,
            shared_filter: SharedDrawingFilter::default(),
            locked: false,
            pending_sync: Arc::new(Mutex::new(Vec::new())),
            sync_enabled: true,
            events: std::collections::VecDeque::new(),
            events_dropped: 0,
            removal: None,
            identity: crate::model::next_revision(),
            mapped_bars: None,
            mapped_key: None,
        };
        manager.update_snap_options();
        manager
    }

    /// Apply the host's tool, snapping, color, eraser and selection state. A state with both
    /// a tool and the eraser leaves only the eraser on, as [`Self::set_eraser_mode`] does.
    pub fn sync_from_app_state(&mut self, drawing_state: &DrawingState) {
        if self.active_tool != drawing_state.active_tool {
            self.set_active_tool(drawing_state.active_tool);
        }
        self.options.magnet_mode = drawing_state.magnet_mode;
        self.options.default_color = drawing_state.curr_color;
        self.stay_in_drawing_mode = drawing_state.stay_in_drawing_mode;
        if self.eraser_mode != drawing_state.eraser_mode {
            self.set_eraser_mode(drawing_state.eraser_mode);
        }

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

    /// Whether drawing `id` exists and the user can edit it; see [`Self::accepts_user_edits`].
    #[cfg(feature = "ui")]
    pub(crate) fn user_can_edit(&self, id: usize) -> bool {
        self.drawings
            .iter()
            .any(|d| d.id == id && self.accepts_user_edits(d))
    }

    /// Selects the drawing with the given ID (deselects all others).
    pub fn select(&mut self, id: usize) {
        self.select_drawings([id]);
    }

    /// Replace the selected drawings with the supplied live ids.
    /// Selects nothing while the drawings are hidden ([`Self::set_hidden`]).
    pub fn select_drawings(&mut self, ids: impl IntoIterator<Item = usize>) {
        self.selection.deselect_all();
        if self.hidden {
            return;
        }
        self.selection.select_all(ids.into_iter().filter(|id| {
            self.drawings
                .iter()
                .any(|d| d.id == *id && self.shared_filter.selectable(d))
        }));
    }

    /// Selected live drawing ids, in drawing-list order.
    pub fn selected_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.drawings
            .iter()
            .filter(|d| self.shared_filter.selectable(d) && self.selection.is_selected(d.id))
            .map(|d| d.id)
    }

    /// Whether a drawing belongs to the current selection.
    pub fn is_selected(&self, id: usize) -> bool {
        self.selection.is_selected(id)
            && self
                .drawings
                .iter()
                .any(|d| d.id == id && self.shared_filter.selectable(d))
    }

    /// Toggle a live drawing in the selection, retaining the other selected drawings.
    pub fn toggle_selection(&mut self, id: usize) {
        if !self.hidden
            && self
                .drawings
                .iter()
                .any(|d| d.id == id && self.shared_filter.selectable(d))
        {
            self.selection.toggle(id);
        }
    }

    /// Apply a style edit to every selected drawing the user can edit
    /// ([`Self::accepts_user_edits`]) as one undo step.
    /// An edit after which saving would refuse one of them, as [`Self::edit_drawing`] describes,
    /// changes none of them, records no undo step and returns the reason.
    pub fn edit_selected(&mut self, edit: impl FnMut(&mut Drawing)) -> Result<(), &'static str> {
        self.edit_selected_as(false, edit)
    }

    /// [`Self::edit_selected`]; `moved` records the edit as a move of the drawings as a whole.
    pub(crate) fn edit_selected_as(
        &mut self,
        moved: bool,
        mut edit: impl FnMut(&mut Drawing),
    ) -> Result<(), &'static str> {
        let before: Vec<_> = self
            .drawings
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                self.shared_filter.selectable(d)
                    && self.selection.is_selected(d.id)
                    && self.accepts_user_edits(d)
            })
            .map(|(index, d)| (index, d.clone()))
            .collect();
        if before.is_empty() {
            return Ok(());
        }
        for (index, old) in &before {
            let drawing = &mut self.drawings[*index];
            edit(drawing);
            if let Err(reason) =
                crate::drawings::validation::validate_edit(old, drawing, self.mapped_bars.as_ref())
            {
                for (index, old) in &before {
                    self.drawings[*index] = old.clone();
                }
                return Err(reason);
            }
        }
        if let Some(reason) = before
            .iter()
            .find_map(|(index, old)| self.check_identity(*index, old).err())
        {
            for (index, old) in &before {
                self.drawings[*index] = old.clone();
            }
            return Err(reason);
        }
        for (_, old) in &before {
            self.queue_change(old, moved);
        }
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !before.iter().any(|(_, d)| d.id == p.id));
        if moved {
            self.history.push_moved_snapshot(before);
        } else {
            self.history.push_snapshot(before);
        }
        Ok(())
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
    /// A drawing keeps its current [`Drawing::user_edit_enabled`].
    pub fn undo(&mut self) -> bool {
        self.step_history(false)
    }

    /// Redoes the last undone operation. Returns `true` if an operation was redone.
    /// A drawing keeps its current [`Drawing::user_edit_enabled`].
    pub fn redo(&mut self) -> bool {
        self.step_history(true)
    }

    /// Runs an undo or redo step and queues the change it made to its drawing.
    fn step_history(&mut self, redo: bool) -> bool {
        let before: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| d.saveable)
            .map(|d| (d.id, sync_id(d)))
            .collect();
        // Editing permission belongs to the host, independently of user history.
        let permissions: Vec<_> = self
            .drawings
            .iter()
            .map(|d| (d.id, d.user_edit_enabled))
            .collect();
        let (ids, moved) = self.history.peek(redo).unwrap_or_default();
        let old: Vec<Drawing> = self
            .drawings
            .iter()
            .filter(|d| ids.contains(&d.id))
            .cloned()
            .collect();
        let stepped = if redo {
            self.history.redo(&mut self.drawings)
        } else {
            self.history.undo(&mut self.drawings)
        };
        let Some(_) = stepped else {
            return false;
        };
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !self.history.affected_ids().contains(&p.id));
        for &id in self.history.affected_ids() {
            let was = before.iter().find(|&&(i, _)| i == id);
            match (was, self.drawings.iter().any(|d| d.id == id && d.saveable)) {
                (Some(_), true) => self.notify(DrawingSyncAction::UpdateDrawing(id)),
                (Some((_, stored_id)), false) => {
                    self.notify(DrawingSyncAction::Delete(stored_id.clone()))
                }
                (None, true) => self.notify(DrawingSyncAction::SaveDrawing(id)),
                (None, false) => {}
            }
        }
        let affected = self.history.affected_ids().to_vec();
        // A state recorded before the drawing's first edit comes back without an edit time.
        let now = chrono::Utc::now().timestamp_millis();
        for drawing in self
            .drawings
            .iter_mut()
            .filter(|d| affected.contains(&d.id))
        {
            drawing.modified_at.get_or_insert(now);
            if let Some(&(_, enabled)) = permissions.iter().find(|(id, _)| *id == drawing.id) {
                drawing.user_edit_enabled = enabled;
            }
        }
        for &id in &affected {
            self.report_change(id, old.iter().find(|d| d.id == id), moved);
        }
        // A step that hides a selected drawing deselects it, as hiding it by an edit does.
        self.selection.prune(
            &self
                .drawings
                .iter()
                .filter(|d| d.visible)
                .map(|d| d.id)
                .collect(),
        );
        true
    }

    /// Puts the time of an edit on the drawing with the given ID.
    fn stamp(&mut self, id: usize) {
        if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id) {
            drawing.modified_at = Some(chrono::Utc::now().timestamp_millis());
        }
    }

    /// Records a change of drawing `id` for the chart to report.
    fn report(&mut self, id: usize, kind: DrawingEventKind) {
        if let Some(removal) = &mut self.removal {
            if !removal.contains(&id) {
                removal.push(id);
            }
            return;
        }
        if self.events.len() == 1024 {
            self.events.pop_front();
            self.events_dropped = self.events_dropped.saturating_add(1);
        }
        self.events.push_back((id, kind));
    }

    /// Records how drawing `id` changed from `old`, `None` when it did not exist, to its current
    /// state. `moved` says the change of its anchors moved it as a whole.
    fn report_change(&mut self, id: usize, old: Option<&Drawing>, moved: bool) {
        let new = self.drawings.iter().find(|d| d.id == id);
        let kinds = match (old, new) {
            (None, Some(_)) => [Some(DrawingEventKind::Created), None, None],
            (Some(_), None) => [Some(DrawingEventKind::Removed), None, None],
            (Some(old), Some(new)) => changes(old, new, moved),
            (None, None) => [None, None, None],
        };
        for kind in kinds.into_iter().flatten() {
            self.report(id, kind);
        }
    }

    /// Records an edit for the host to save and puts its time on the drawing.
    fn queue(&mut self, action: DrawingSyncAction) {
        if let DrawingSyncAction::SaveDrawing(id) | DrawingSyncAction::UpdateDrawing(id) = &action {
            self.stamp(*id);
        }
        self.notify(action);
    }

    /// Queues a change for the host to save.
    fn notify(&self, action: DrawingSyncAction) {
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

    /// Applies `magnet_mode` and `magnet_distance` to the snapping service. Call this after
    /// changing `magnet_distance`; the other options take effect on the next frame.
    pub fn update_snap_options(&mut self) {
        self.snap_service = SnapService::with_options(SnapOptions {
            magnet_mode: self.options.magnet_mode,
            magnet_distance: self.options.magnet_distance,
            ..SnapOptions::default()
        });
    }

    pub(crate) fn magnet_enabled(&self) -> bool {
        self.options.magnet_mode
            ^ (self.interaction_modifiers.ctrl || self.interaction_modifiers.command)
    }

    /// Applies snap behavior to a screen point, returning the snapped position.
    ///
    /// With magnet mode on, snaps to nearby chart values and visible anchors of `drawings`.
    /// Targets keep their time and price paired. A dragged drawing cannot snap to itself.
    /// Drawing anchors are borrowed without flattening or cloning their geometry.
    pub fn snap_point(&self, point: Pos2, drawings: &[Drawing]) -> Pos2 {
        if self.magnet_enabled()
            && self.options.strong_magnet
            && let Some(target) = self
                .snap_targets
                .iter()
                .filter(|p| p.is_finite())
                .min_by(|a, b| (a.y - point.y).abs().total_cmp(&(b.y - point.y).abs()))
        {
            return *target;
        }
        let mut service = self.snap_service.clone();
        service.set_magnet_mode(self.magnet_enabled());
        service.snap_point_with_drawing_points(
            point,
            &[],
            &[],
            self.snap_targets.iter().copied().chain(
                drawings
                    .iter()
                    .filter(|d| {
                        self.shows(d)
                            && self.pane_members.contains(&d.pane)
                            && self.dragging_handle.is_none_or(|(id, _)| id != d.id)
                            && self.drag_group.iter().all(|old| old.id != d.id)
                    })
                    .flat_map(|d| d.points.iter().copied()),
            ),
        )
    }

    fn queue_edit(&mut self, old: &Drawing) {
        self.queue_change(old, false);
    }

    /// Queues the storage action for an edit of `old` and records what it changed.
    fn queue_change(&mut self, old: &Drawing, moved: bool) {
        if let Some(drawing) = self.drawings.iter().find(|d| d.id == old.id) {
            let action = match (old.saveable, drawing.saveable) {
                (true, false) => DrawingSyncAction::Delete(sync_id(old)),
                (false, true) => DrawingSyncAction::SaveDrawing(old.id),
                _ => DrawingSyncAction::UpdateDrawing(old.id),
            };
            self.queue(action);
            self.report_change(old.id, Some(old), moved);
        }
    }

    /// Queues an update of the drawing with the given ID, if there is one.
    fn queue_update(&mut self, id: usize) {
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

    /// What [`Self::keep_across_read`] needs of the drawings about to be replaced. With
    /// `keep_confined`, the saveable drawings in a top-level folder kept on one chart or in one
    /// layout stay too, as drawing storage does not hold them.
    pub(crate) fn before_read(&self, keep_confined: bool) -> BeforeRead {
        let stays = |d: &&Drawing| !d.saveable || (keep_confined && d.confined());
        BeforeRead {
            // A drawing that stays is in no document read, so its id is no stored id.
            ids: self
                .drawings
                .iter()
                .filter(|d| !stays(d))
                .map(|d| (sync_id(d), d.id))
                .collect(),
            next_id: self.next_id,
            unsaved: self.drawings.iter().filter(stays).cloned().collect(),
            identity: self.identity,
        }
    }

    /// Gives drawings just read from storage the local ids they had before, so that what refers
    /// to a drawing by its local id, such as an open properties dialog, keeps referring to it.
    /// Every other read drawing gets an id that no drawing of the manager has or had, so that it
    /// is not taken for one removed elsewhere. Drawings that are never stored stay, since no
    /// document holds them, and so do the confined drawings [`Self::before_read`] kept, unless
    /// one was read again (see [`read_again`]); one that has the stored id of another read
    /// drawing takes a free one, so that a later write keeps both.
    pub(crate) fn keep_across_read(&mut self, previous: BeforeRead) {
        let BeforeRead {
            mut ids,
            next_id,
            unsaved,
            identity,
        } = previous;
        // The same drawings to a layout that mirrors them, as their ids are.
        self.identity = identity;
        self.next_id = self.next_id.max(next_id);
        let read = self.drawings.len();
        let mut taken: HashSet<String> = self.drawings.iter().map(sync_id).collect();
        taken.extend(unsaved.iter().filter(|d| d.saveable).map(sync_id));
        // The drawings that stay are in place before ids are handed out, so none gets theirs.
        for mut drawing in unsaved {
            if drawing.saveable
                && let Some(other) = self.drawings[..read]
                    .iter()
                    .find(|other| other.stored_id.as_deref() == Some(sync_id(&drawing).as_str()))
            {
                if read_again(&drawing, other) {
                    ids.insert(sync_id(&drawing), drawing.id);
                    continue;
                }
                drawing.stored_id = Some(free_stored_id(&drawing, &taken));
                taken.insert(sync_id(&drawing));
            }
            self.drawings.push(drawing);
        }
        let mut fresh = Vec::new();
        for (at, drawing) in self.drawings[..read].iter_mut().enumerate() {
            match drawing.stored_id.as_ref().and_then(|id| ids.get(id)) {
                Some(id) => drawing.id = *id,
                None => fresh.push(at),
            }
        }
        for at in fresh {
            self.drawings[at].id = self.take_id();
        }
    }

    /// A new drawing of `tool` in its default style: the tool's own, recoloured by
    /// [`DrawingManagerOptions::default_color`] when set, then the default saved for the tool.
    pub fn new_drawing(&self, id: usize, tool: DrawingToolType) -> Drawing {
        let mut drawing = self.base_drawing(id, tool);
        if let Some(defaults) = self.tool_defaults.get(&tool) {
            let _ = defaults.apply_to(&mut drawing);
        }
        drawing
    }

    /// A new registered drawing in the polyline's original style, recoloured by
    /// [`DrawingManagerOptions::default_color`] when set, then styled by the default saved for
    /// `tool` in [`Self::custom_tool_defaults`]. The polyline's saved default never applies.
    pub fn new_custom_drawing(&self, id: usize, tool: &str) -> Drawing {
        let mut drawing = self.base_drawing(id, DrawingToolType::Polyline);
        drawing.custom_tool = Some(tool.to_owned());
        if let Some(defaults) = self.custom_tool_defaults.get(tool) {
            let _ = defaults.apply_to(&mut drawing);
        }
        drawing
    }

    fn base_drawing(&self, id: usize, tool: DrawingToolType) -> Drawing {
        let mut drawing = match self.options.default_color {
            Some(color) => Drawing::with_color(id, tool, color),
            None => Drawing::new(id, tool),
        };
        self.place_locally(&mut drawing);
        drawing
    }

    /// Moves a drawing placed on this chart now, by a tool, a paste or a clone, into the
    /// top-level chart-only folder that [`DrawingManagerOptions::local_placement_folder`] names,
    /// numbered when a folder shared beyond this chart already has the name. Without the option
    /// the drawing keeps its folder.
    fn place_locally(&self, drawing: &mut Drawing) {
        use super::FolderSync;
        let Some(name) = &self.options.local_placement_folder else {
            return;
        };
        let shared_names = || {
            self.drawings
                .iter()
                .filter(|d| d.folder_sync_mode() != Some(FolderSync::Chart))
                .filter_map(|d| d.folder.first().map(String::as_str))
        };
        let folder = if shared_names().any(|taken| taken == name) {
            let taken: HashSet<_> = shared_names().collect();
            (2_usize..)
                .map(|n| format!("{name} ({n})"))
                .find(|candidate| !taken.contains(candidate.as_str()))
                .expect("more folder names than drawings")
        } else {
            name.clone()
        };
        drawing.folder = vec![folder];
        drawing.folder_sync = vec![Some(FolderSync::Chart)];
    }

    /// Saves a visible default under its registered tool id or built-in tool type.
    #[cfg(any(feature = "ui", test))]
    pub(crate) fn save_tool_default(&mut self, mut template: super::DrawingTemplate) {
        template.settings.visible = true;
        match template.settings.custom_tool.clone() {
            Some(id) => {
                self.custom_tool_defaults.insert(id, template);
            }
            None => {
                self.tool_defaults
                    .insert(template.settings.tool_type, template);
            }
        }
    }

    /// The z-order that puts a drawing added now on top of the others.
    pub(crate) fn top_z_order(&self) -> i32 {
        self.drawings.iter().map(|d| d.z_order).max().unwrap_or(0)
    }

    pub(crate) fn visit_chart_points(&mut self, visit: &mut impl FnMut(&mut ChartPoint)) {
        for drawing in self
            .drawings
            .iter_mut()
            .chain(&mut self.curr_drawing)
            .chain(&mut self.measurement)
            .chain(&mut self.drag_old_state)
            .chain(&mut self.drag_group)
            .chain(
                self.partial_erase
                    .iter_mut()
                    .flat_map(|erase| &mut erase.before),
            )
        {
            drawing.chart_points.iter_mut().for_each(&mut *visit);
        }
        self.history.visit_chart_points(visit);
    }
}

/// What a manager keeps when drawings read from storage replace its own.
pub(crate) struct BeforeRead {
    /// The local id of each drawing by the id it is stored under.
    ids: std::collections::HashMap<String, usize>,
    next_id: usize,
    /// The drawings that are never stored.
    unsaved: Vec<Drawing>,
    /// The `DrawingManager::identity` of the manager.
    identity: u64,
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
pub(crate) fn sync_id(drawing: &Drawing) -> String {
    drawing
        .stored_id
        .clone()
        .unwrap_or_else(|| drawing.id.to_string())
}

/// What a change from `old` to `new` reports, in the order of `DrawingEventKind`.
fn changes(old: &Drawing, new: &Drawing, moved: bool) -> [Option<DrawingEventKind>; 3] {
    use DrawingEventKind as Kind;
    let visibility = match (old.visible, new.visible) {
        (true, false) => Some(Kind::Hidden),
        (false, true) => Some(Kind::Shown),
        _ => None,
    };
    let anchors = anchors_differ(old, new).then_some(if moved {
        Kind::Moved
    } else {
        Kind::AnchorsChanged
    });
    let properties = properties_differ(old, new).then_some(Kind::PropertiesChanged);
    [visibility, anchors, properties]
}

/// Whether the anchors differ. A NaN matches a NaN, so a drawing that saving already refuses
/// reports no change it did not make.
fn anchors_differ(old: &Drawing, new: &Drawing) -> bool {
    let same = |a: f64, b: f64| a == b || (a.is_nan() && b.is_nan());
    let same_screen = match (old.screen_anchor, new.screen_anchor) {
        (Some(a), Some(b)) => a
            .into_iter()
            .zip(b)
            .all(|(a, b)| same(f64::from(a), f64::from(b))),
        (None, None) => true,
        _ => false,
    };
    !same_screen
        || old.chart_points.len() != new.chart_points.len()
        || old
            .chart_points
            .iter()
            .zip(&new.chart_points)
            .any(|(a, b)| {
                !same(f64::from(a.bar_idx), f64::from(b.bar_idx)) || !same(a.price, b.price)
            })
}

/// Whether a saved setting other than visibility and anchors differs. Compares the saved form,
/// so a setting added to `Drawing` later is covered without a change here.
fn properties_differ(old: &Drawing, new: &Drawing) -> bool {
    let saved = |drawing: &Drawing| {
        let mut value = serde_json::to_value(drawing).ok()?;
        let fields = value.as_object_mut()?;
        fields.remove("visible");
        fields.remove("screen_anchor");
        Some(value)
    };
    old.notes != new.notes
        || match (saved(old), saved(new)) {
            (Some(old), Some(new)) => old != new,
            _ => true,
        }
}

/// Whether `kept`, a drawing that storage does not hold, is `read`, the drawing read under its
/// stored id: one stored in a folder kept on one chart or in one layout, as earlier versions
/// stored such drawings, or one stored with the same creation time.
pub(crate) fn read_again(kept: &Drawing, read: &Drawing) -> bool {
    kept.stored_id.is_some()
        && kept.stored_id == read.stored_id
        && (read.confined() || kept.created_at == read.created_at)
}

/// The stored id of `drawing`, with `-local` appended until no id in `taken` is the same.
pub(crate) fn free_stored_id(drawing: &Drawing, taken: &HashSet<String>) -> String {
    let mut id = sync_id(drawing);
    while taken.contains(&id) {
        id.push_str("-local");
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two clicks and Enter finish a multi-point tool with two anchors. Whatever the
    /// manager finishes must save, and a tool that draws nothing with two anchors (the parallel
    /// channel and the pitchforks) must stay in progress, or the whole storage would refuse to save.
    #[test]
    fn a_drawing_finished_early_can_be_saved() {
        use crate::drawings::persistence::{DrawingStorage, drawing_to_stored};
        for (tool, finishes) in [
            (DrawingToolType::Triangle, true),
            (DrawingToolType::Pitchfork, false),
            (DrawingToolType::SchiffPitchfork, false),
            (DrawingToolType::ModifiedSchiffPitchfork, false),
            (DrawingToolType::InsidePitchfork, false),
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
                    manager.edit_drawings(&[3], change).unwrap();
                } else {
                    manager.edit_drawing(3, change).unwrap();
                }
            };
            let mut manager = DrawingManager::new();
            let mut line = Drawing::new(3, DrawingToolType::TrendLine);
            line.chart_points = vec![ChartPoint::new(10.0, 10.0), ChartPoint::new(20.0, 20.0)];
            line.update_screen_coords(|bar| bar, |price| price as f32);
            line.locked = true;
            manager.drawings.push(line);

            manager.start_drag_handle(3, HandlePos::Start);
            manager.update_drag_handle(
                Pos2::new(50.0, 50.0),
                |x| x,
                f64::from,
                |price, from, to| price + f64::from(to - from),
            );
            manager.end_drag_handle();
            manager.delete_drawing(3);
            edit(&mut manager, |d| d.font_size = 30.0);
            assert_eq!(
                manager.drag_edit_drawing(3, |d| d.font_size = 40.0),
                Err("Drawing is locked")
            );
            manager.end_drag_handle();
            assert_eq!(manager.drawings[0].font_size, 14.0);
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
            assert_eq!(manager.drawings[0].font_size, 14.0);
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

    /// User commands leave protected drawings alone, while id-based host edits and their
    /// history still work. Permissions survive undo, and user copies are editable. Lock all
    /// holds an editable drawing too, except for commands that act on locked drawings.
    #[test]
    fn user_commands_leave_a_drawing_without_user_edits_alone() {
        fn fixture(enabled: bool) -> DrawingManager {
            let mut manager = DrawingManager::new();
            for id in [3, 4] {
                let mut line = Drawing::new(id, DrawingToolType::TrendLine);
                let first = if id == 3 { 10.0 } else { 30.0 };
                line.chart_points = vec![
                    ChartPoint::new(first, f64::from(first)),
                    ChartPoint::new(first + 10.0, f64::from(first + 10.0)),
                ];
                line.update_screen_coords(|bar| bar, |price| price as f32);
                line.completed = true;
                line.user_edit_enabled = id != 3 || enabled;
                manager.drawings.push(line);
            }
            manager.select_drawings([3, 4]);
            manager
        }
        fn state(manager: &DrawingManager, id: usize) -> Option<(String, Vec<ChartPoint>)> {
            manager
                .drawings
                .iter()
                .find(|d| d.id == id)
                .map(|d| (serde_json::to_string(d).unwrap(), d.chart_points.clone()))
        }
        fn drag(manager: &mut DrawingManager) {
            manager.update_drag_handle(
                Pos2::new(50.0, 50.0),
                |x| x,
                f64::from,
                |price, from, to| price + f64::from(to - from),
            );
            manager.end_drag_handle();
        }
        type Command = fn(&mut DrawingManager);
        let commands: &[(&str, Command)] = &[
            ("handle drag", |m| {
                m.start_drag_handle(3, HandlePos::Start);
                drag(m);
            }),
            ("body drag", |m| {
                m.start_drag_body(3, Pos2::new(15.0, 15.0));
                drag(m);
            }),
            ("group drag", |m| {
                m.start_drag_body(4, Pos2::new(35.0, 35.0));
                drag(m);
            }),
            ("edit selected", |m| {
                m.edit_selected(|d| d.font_size = 30.0).unwrap();
            }),
            ("delete selected", |m| m.delete_selected()),
            ("clear all", |m| m.clear_all()),
            ("clear all with locked", |m| m.clear_all_with_locked(true)),
            ("set selected locked", |m| m.set_selected_locked(true)),
        ];
        for &(name, command) in commands {
            for enabled in [true, false] {
                let mut manager = fixture(enabled);
                let before = state(&manager, 3);
                let neighbour = state(&manager, 4);
                command(&mut manager);
                assert_eq!(
                    state(&manager, 3) == before,
                    !enabled,
                    "{name}, enabled {enabled}"
                );
                if matches!(name, "clear all" | "clear all with locked") {
                    assert_ne!(state(&manager, 4), neighbour, "{name}");
                }
            }
            // Lock all holds drawing 3 like its own lock, except for commands on locked drawings.
            let mut manager = fixture(true);
            manager.set_locked(true);
            let before = state(&manager, 3);
            command(&mut manager);
            assert_eq!(
                state(&manager, 3) == before,
                !matches!(name, "clear all with locked" | "set selected locked"),
                "{name}, all locked"
            );
        }
        let mut manager = fixture(false);
        manager.edit_drawing(3, |d| d.font_size = 30.0).unwrap();
        assert_eq!(manager.drawings[0].font_size, 30.0);
        assert!(manager.undo());
        assert_eq!(manager.drawings[0].font_size, 14.0);
        manager.delete_drawing(3);
        assert!(!manager.drawings.iter().any(|d| d.id == 3));
        assert!(manager.undo());
        let original = manager.drawings.iter().find(|d| d.id == 3).unwrap().clone();
        assert!(!original.user_edit_enabled);
        manager.duplicate_drawing(3);
        assert!(manager.drawings.last().unwrap().user_edit_enabled);
        manager.paste_drawings(&[original]);
        assert!(manager.drawings.last().unwrap().user_edit_enabled);

        let mut manager = fixture(true);
        manager.edit_drawing(3, |d| d.font_size = 30.0).unwrap();
        manager.drawings[0].user_edit_enabled = false;
        assert!(manager.undo());
        assert_eq!(
            (
                manager.drawings[0].font_size,
                manager.drawings[0].user_edit_enabled
            ),
            (14.0, false)
        );
        assert!(manager.redo());
        assert_eq!(
            (
                manager.drawings[0].font_size,
                manager.drawings[0].user_edit_enabled
            ),
            (30.0, false)
        );
        // A permission revoked during a gesture takes effect on its next update.
        let mut manager = fixture(true);
        manager.start_drag_handle(3, HandlePos::Start);
        manager.drawings[0].user_edit_enabled = false;
        let before = state(&manager, 3);
        drag(&mut manager);
        assert_eq!(state(&manager, 3), before);
    }

    /// A bulk command on drawing 2 records only that drawing, so its undo and redo keep
    /// changes made to the others outside history.
    #[test]
    fn bulk_edits_preserve_unrelated_host_changes_and_drawing_order() {
        for edit in [
            (|m: &mut DrawingManager| {
                m.edit_drawings(&[2], |d| d.folder = vec!["Analysis".into()])
                    .unwrap();
            }) as fn(&mut DrawingManager),
            |m| m.edit_drawings(&[2], |d| d.visible = false).unwrap(),
            |m| m.edit_drawings(&[2], |d| d.locked = true).unwrap(),
            |m| {
                m.select(2);
                m.edit_selected(|d| d.color = [255, 0, 0, 255]).unwrap();
            },
            |m| m.set_group(&[2], Some("Group".into())),
        ] {
            let mut manager = DrawingManager::new();
            manager.drawings = (0..3)
                .map(|id| Drawing::new(id, DrawingToolType::HorizontalLine))
                .collect();
            edit(&mut manager);
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

    /// Overlapping drawings are hit by their place among series and studies, then `z_order`,
    /// whichever was added last. Manager moves set both, while a host addition or paste lands
    /// on top, including a paste of a drawing sent behind the series.
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
        assert_eq!(manager.drawings[0].series_layer, Some(0));
        manager.drawings[0].z_order = 100;
        assert_eq!(manager.hit_test(on_both), Some(2));

        manager.bring_to_front(1);
        assert_eq!(manager.drawings[0].series_layer, None);
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
        manager.add_synced_drawing(fresh).unwrap();
        let fresh_id = manager.drawings.last().unwrap().id;
        assert_eq!(manager.hit_test(on_both), Some(fresh_id));

        // Pasting a lower layer puts the copy above existing drawings too.
        manager.send_to_back(2);
        manager.paste_drawings(&[manager.drawings[1].clone()]);
        manager.update_all_screen_coords(|bar| bar, |price| price as f32, 1.0);
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
        fn line(id: usize) -> Drawing {
            let mut drawing = Drawing::new(id, DrawingToolType::HorizontalLine);
            drawing.chart_points = vec![ChartPoint::new(1.0, 10.0)];
            drawing
        }
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
                    m.add_synced_drawing(line(1)).unwrap();
                    m.reorder_drawings(0, &[(0, 1), (1, 2)]);
                    m.undo();
                    m.redo();
                },
                &["Save(1)", "Update(0)"],
            ),
            (|m| m.clear_all(), &["Delete(a1b2)"]),
            (|m| m.set_hidden(true), &[]),
            (|m| m.set_locked(true), &[]),
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
                |m| {
                    m.add_synced_drawing(line(7)).unwrap();
                },
                &["Save(1)"],
            ),
            (
                |m| m.edit_drawing(0, |d| d.stroke_width = 3.0).unwrap(),
                &["Update(0)"],
            ),
            (
                // A colour picker dragged over 120 frames is one undo step and one update.
                |m| {
                    for red in 0..120 {
                        m.drag_edit_drawing(0, |d| d.color = [red, 0, 0, 255])
                            .unwrap();
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
                        m.edit_drawing(0, |d| d.stroke_width = 2.0 + (i % 2) as f32)
                            .unwrap();
                    }
                    m.edit_drawings(&[0], |d| d.locked = true).unwrap();
                },
                &["Update(0)"],
            ),
            (
                |m| {
                    m.edit_drawing(0, |d| d.stroke_width = 2.0).unwrap();
                    m.add_synced_drawing(line(1)).unwrap();
                    m.delete_drawing(1);
                    m.edit_drawing(0, |d| d.stroke_width = 3.0).unwrap();
                },
                &["Update(0)", "Save(1)", "Delete(1)"],
            ),
        ];
        for (case, (edit, expected)) in cases.into_iter().enumerate() {
            let mut manager = DrawingManager::new();
            let mut restored = line(5);
            restored.stored_id = Some("a1b2".into());
            manager.add_synced_drawing(restored).unwrap();
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
            if matches!(case, 3 | 4) {
                assert_eq!(
                    before, after,
                    "group hiding and locking are not drawing edits"
                );
                assert!(!manager.can_undo());
            }
            if matches!(case, 0 | 2 | 5 | 6 | 10 | 11) {
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
        manager.update_all_screen_coords(|x| x, |y| y as f32, 1.0);
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
        manager
            .edit_selected(|drawing| {
                if drawing.id == 1 {
                    drawing.selectable = false;
                }
            })
            .unwrap();
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

    #[test]
    fn shared_drawing_filter_removes_only_shared_targets() {
        let mut m = DrawingManager::new();
        m.options.magnet_mode = true;
        let [local_id, shared_id] = [(false, 100.0), (true, 150.0)].map(|(shared, y)| {
            let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
            line.stored_id = Some(format!("line-{y}"));
            line.chart_points.push(ChartPoint::new(50.0, y));
            if shared {
                line.folder.push("Shared".into());
                line.folder_sync
                    .push(Some(crate::drawings::FolderSync::AllLayouts));
            }
            m.add_synced_drawing(line).unwrap()
        });
        m.update_all_screen_coords(|x| x, |y| y as f32, 1.0);
        let local = Pos2::new(50.0, 100.0);
        let shared = Pos2::new(50.0, 150.0);
        let near = shared + egui::vec2(2.0, 1.0);
        let tags = |m: &DrawingManager| {
            m.axis_tags(Rect::EVERYTHING, egui::Color32::WHITE, &[], 0, None)
                .filter_map(|tag| match tag {
                    DrawingAxisTag::Price { price, .. } => Some(price),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(tags(&m), [100.0, 150.0]);
        assert_eq!(m.hit_test(shared), Some(shared_id));
        assert_eq!(m.snap_point(near, &m.drawings), shared);
        m.select_drawings([local_id, shared_id]);
        m.set_shared_drawings_hidden(true);
        m.toggle_selection(shared_id);
        assert_eq!(m.selected_ids().collect::<Vec<_>>(), [local_id]);
        assert_eq!(tags(&m), [100.0]);
        assert_eq!(m.hit_test(local), Some(local_id));
        assert_eq!(m.hit_test(shared), None);
        assert_eq!(m.snap_point(near, &m.drawings), near);
        m.start_drag_body(shared_id, shared);
        m.update_drag_handle(
            local,
            |x| x,
            f64::from,
            |p, from, to| p + f64::from(to - from),
        );
        m.end_drag_handle();
        m.set_shared_drawings_hidden(false);
        assert_eq!(tags(&m), [100.0, 150.0]);
        assert!(m.pending_sync.lock().unwrap().is_empty());
    }

    /// The magnet snaps to a drawing anchor within the default distance, excluding members of
    /// the group being dragged.
    #[test]
    fn magnet_snaps_to_anchors_except_the_group_being_dragged() {
        let mut manager = DrawingManager::new();
        manager.options.magnet_mode = true;

        let mut d = Drawing::new(1, DrawingToolType::TrendLine);
        d.points = vec![Pos2::new(100.0, 100.0), Pos2::new(200.0, 200.0)];
        let drawings = vec![d];

        // A pointer just inside the default distance of the first anchor snaps to it.
        let reach = manager.options.magnet_distance - 1.0;
        let snapped = manager.snap_point(Pos2::new(100.0 + reach, 100.0), &drawings);
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
    /// the entry and moves without dragging the target along. The target and
    /// stop handles change price only; the width handle moves both to its x.
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
        manager.update_drag_handle(
            Pos2::new(50.0, 105.0),
            x_to_bar,
            y_to_price,
            |price, from, to| price - f64::from(to - from),
        );
        manager.end_drag_handle();
        let points = &manager.drawings[0].chart_points;
        assert_eq!((points[1].price, points[2].price), (150.0, 95.0));
        assert_eq!(points[2].bar_idx, 110.0);

        manager.start_drag_handle(id, HandlePos::Point(1));
        manager.update_drag_handle(
            Pos2::new(40.0, 85.0),
            x_to_bar,
            y_to_price,
            |price, from, to| price - f64::from(to - from),
        );
        manager.end_drag_handle();
        let points = &manager.drawings[0].chart_points;
        assert_eq!((points[1].bar_idx, points[1].price), (110.0, 115.0));
        assert_eq!((points[2].bar_idx, points[2].price), (110.0, 95.0));

        manager.start_drag_handle(id, HandlePos::End);
        manager.update_drag_handle(
            Pos2::new(260.0, 30.0),
            x_to_bar,
            y_to_price,
            |price, from, to| price - f64::from(to - from),
        );
        manager.end_drag_handle();
        let points = &manager.drawings[0].chart_points;
        assert_eq!((points[1].bar_idx, points[1].price), (260.0, 115.0));
        assert_eq!((points[2].bar_idx, points[2].price), (260.0, 95.0));
    }

    /// A host state with both a tool and the eraser leaves only the eraser on, as
    /// `set_eraser_mode` does.
    #[test]
    fn app_state_with_a_tool_and_the_eraser_leaves_only_the_eraser() {
        let mut manager = DrawingManager::new();
        manager.sync_from_app_state(&DrawingState {
            active_tool: Some(DrawingToolType::TrendLine),
            eraser_mode: true,
            ..DrawingState::default()
        });
        assert!(manager.is_eraser_mode());
        assert_eq!(manager.active_tool(), None);
    }

    /// Every path that creates drawings adds only drawings that save: a drawing of a disabled
    /// tool, a cursor, a non-finite or unmappable anchor or too few anchors is left out, and the
    /// chart still saves.
    #[test]
    fn creation_paths_add_only_drawings_that_save() {
        use crate::drawings::ToolAvailability;
        fn line(tool: DrawingToolType, anchors: &[(f32, f64)]) -> Drawing {
            let mut drawing = Drawing::new(0, tool);
            drawing.chart_points = anchors
                .iter()
                .map(|&(bar, price)| ChartPoint::new(bar, price))
                .collect();
            drawing.completed = true;
            drawing
        }
        fn place(m: &mut DrawingManager, tool: DrawingToolType, x_to_bar: fn(f32) -> f32) {
            let y_to_price = |y: f32| 200.0 - f64::from(y);
            m.start_drawing_with_coords(tool, Pos2::new(10.0, 95.0), x_to_bar, y_to_price);
            m.add_point_with_coords(Pos2::new(20.0, 94.0), x_to_bar, y_to_price);
            m.complete_curr_drawing();
        }
        const TWO: &[(f32, f64)] = &[(1.0, 105.0), (2.0, 106.0)];
        const FAR: &[(f32, f64)] = &[(1.0, 105.0), (1e9, 106.0)];
        const NAN: &[(f32, f64)] = &[(1.0, 105.0), (2.0, f64::NAN)];
        const ONE: &[(f32, f64)] = &[(1.0, 105.0)];
        type Create = fn(&mut DrawingManager);
        let cases: [(&str, bool, Create); 15] = [
            ("add, anchor off the chart", false, |m| {
                let _ = m.add_drawing(line(DrawingToolType::TrendLine, FAR));
            }),
            ("paste, anchor off the chart", false, |m| {
                m.paste_drawings(&[line(DrawingToolType::TrendLine, FAR)])
            }),
            ("sync, anchor off the chart", false, |m| {
                let _ = m.add_synced_drawing(line(DrawingToolType::TrendLine, FAR));
            }),
            ("duplicate, disabled tool", true, |m| {
                m.duplicate_drawing(m.drawings[0].id)
            }),
            ("copy drag, disabled tool", true, |m| {
                let id = m.drawings[0].id;
                m.start_drag_body(id, Pos2::ZERO);
                m.copy_dragged_drawings();
                m.end_drag_handle();
            }),
            ("paste, disabled tool", true, |m| {
                m.paste_drawings(&m.drawings.clone())
            }),
            ("paste, cursor", false, |m| {
                m.paste_drawings(&[line(DrawingToolType::Eraser, TWO)])
            }),
            ("paste, non-finite anchor", false, |m| {
                m.paste_drawings(&[line(DrawingToolType::TrendLine, NAN)])
            }),
            ("paste, one anchor", false, |m| {
                m.paste_drawings(&[line(DrawingToolType::TrendLine, ONE)])
            }),
            ("sync, cursor", false, |m| {
                let _ = m.add_synced_drawing(line(DrawingToolType::Eraser, TWO));
            }),
            ("sync, non-finite anchor", false, |m| {
                let _ = m.add_synced_drawing(line(DrawingToolType::TrendLine, NAN));
            }),
            ("sync, one anchor", false, |m| {
                let _ = m.add_synced_drawing(line(DrawingToolType::TrendLine, ONE));
            }),
            ("pointer, cursor", false, |m| {
                place(m, DrawingToolType::Eraser, |x| x / 10.0)
            }),
            ("pointer, non-finite anchor", false, |m| {
                place(m, DrawingToolType::TrendLine, |_| f32::NAN)
            }),
            ("pointer, unmappable anchor", false, |m| {
                place(m, DrawingToolType::TrendLine, |x| x * 1e30)
            }),
        ];
        for (case, disable, create) in cases {
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            let id = chart
                .drawings_mut()
                .add_drawing(line(DrawingToolType::TrendLine, TWO))
                .unwrap();
            if disable {
                chart.drawings_mut().registry.set_availability(
                    DrawingToolType::TrendLine,
                    ToolAvailability {
                        visible: true,
                        enabled: false,
                    },
                );
            }
            create(chart.drawings_mut());
            let ids: Vec<_> = chart.drawings().drawings.iter().map(|d| d.id).collect();
            assert_eq!(ids, [id], "{case}");
            assert!(chart.save_drawings().is_ok(), "{case}");
        }
    }
}
