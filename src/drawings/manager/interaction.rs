//! Pointer interaction: handles and drags, tool selection and drawing creation.

use super::{DrawingManager, DrawingSyncAction};
use crate::drawings::domain::{ChartPoint, Drawing, DrawingToolType, HandlePos};
use crate::drawings::services::HandleService;
use egui::Pos2;

impl DrawingManager {
    // === Handle delegation ===

    /// Returns the selection handle positions for a drawing (tool-type-aware).
    pub fn get_handles(&self, drawing: &Drawing) -> Vec<(HandlePos, Pos2)> {
        HandleService::get_handles(drawing)
    }

    /// Hit-tests a point against the handles of a drawing.
    ///
    /// Returns the [`HandlePos`] if a handle was hit, `None` otherwise.
    pub fn hit_test_handle(&self, drawing: &Drawing, point: Pos2) -> Option<HandlePos> {
        if !drawing.selectable
            || !drawing.is_visible_on_timeframe(&self.curr_timeframe)
            || (!self.pane_clips.is_empty()
                && !self
                    .pane_clips
                    .get(&drawing.pane)
                    .is_some_and(|rect| rect.contains(point)))
        {
            return None;
        }
        HandleService::hit_test_handle(drawing, point, self.options.handle_config.size)
    }

    /// Begins a handle drag operation, saving the drawing's current state for undo. A locked
    /// drawing is not dragged.
    pub fn start_drag_handle(&mut self, drawing_id: usize, handle: HandlePos) {
        if let Some(drawing) = self.drawings.iter().find(|d| {
            d.id == drawing_id
                && d.selectable
                && !d.locked
                && d.is_visible_on_timeframe(&self.curr_timeframe)
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
            self.drag_group = self
                .drawings
                .iter()
                .filter(|d| {
                    d.id != drawing_id
                        && self.pane_members.contains(&d.pane)
                        && d.selectable
                        && self.selection.is_selected(d.id)
                        && !d.locked
                        && d.is_visible_on_timeframe(&self.curr_timeframe)
                })
                .cloned()
                .collect();
        }
    }

    /// Updates the position of the currently dragged handle to `new_pos`,
    /// applying snap behavior and coordinate conversion.
    /// `translate_price(price, from_y, to_y)` shifts whole-drawing prices in scale units;
    /// `y_to_price` retains the pointer bounds for individual handles.
    pub fn update_drag_handle<F, G, H>(
        &mut self,
        new_pos: Pos2,
        x_to_bar: F,
        y_to_price: G,
        translate_price: H,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
        H: Fn(f64, f32, f32) -> f64,
    {
        if let Some((id, handle)) = self.dragging_handle {
            // Compute snapped position before mutable borrow
            let snapped = self.snap_point(new_pos, &self.drawings);
            if let Some(origin) = self.drag_origin {
                let bars = x_to_bar(snapped.x) - x_to_bar(origin.x);
                for old in &self.drag_group {
                    if let Some(drawing) = self
                        .drawings
                        .iter_mut()
                        .find(|d| d.id == old.id && !d.locked)
                    {
                        for (point, before) in
                            drawing.chart_points.iter_mut().zip(&old.chart_points)
                        {
                            *point = ChartPoint::new(
                                before.bar_idx + bars,
                                translate_price(before.price, origin.y, snapped.y),
                            );
                        }
                        for (point, before) in drawing.points.iter_mut().zip(&old.points) {
                            *point = *before + (snapped - origin);
                        }
                        if let Some([x, y]) = old.screen_anchor {
                            drawing.screen_anchor = Some([
                                x + (snapped.x - origin.x) / self.pane_rect.width(),
                                y + (snapped.y - origin.y) / self.pane_rect.height(),
                            ]);
                        }
                    }
                }
            }
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id && !d.locked) {
                if let (Some(origin), Some(old)) = (self.drag_origin, &self.drag_old_state) {
                    let bars = x_to_bar(snapped.x) - x_to_bar(origin.x);
                    for (point, before) in drawing.chart_points.iter_mut().zip(&old.chart_points) {
                        *point = ChartPoint::new(
                            before.bar_idx + bars,
                            translate_price(before.price, origin.y, snapped.y),
                        );
                    }
                    for (point, before) in drawing.points.iter_mut().zip(&old.points) {
                        *point = *before + (snapped - origin);
                    }
                } else {
                    HandleService::update_handle(
                        drawing,
                        handle,
                        snapped,
                        x_to_bar,
                        y_to_price,
                        translate_price,
                    );
                }
                if drawing.screen_anchor.is_some()
                    && let Some(pos) = drawing.points.first()
                {
                    drawing.screen_anchor = Some([
                        (pos.x - self.pane_rect.left()) / self.pane_rect.width(),
                        (pos.y - self.pane_rect.top()) / self.pane_rect.height(),
                    ]);
                }
            }
        }
    }

    /// Ends the current handle drag or [`Self::drag_edit_drawing`] gesture, pushing one modify
    /// command to the undo history and queuing one update for the whole gesture.
    pub fn end_drag_handle(&mut self) {
        if let Some(old_state) = self.drag_old_state.take() {
            self.queue_edit(&old_state);
            if self.drag_group.is_empty() {
                self.history.push_modify(old_state.id, old_state);
            } else {
                let mut before = std::mem::take(&mut self.drag_group);
                for drawing in &before {
                    self.queue_update(drawing.id);
                }
                before.push(old_state);
                let snapshot = self
                    .drawings
                    .iter()
                    .enumerate()
                    .filter_map(|(index, drawing)| {
                        let old = before.iter().find(|old| old.id == drawing.id)?;
                        Some((index, old.clone()))
                    })
                    .collect();
                self.history.push_snapshot(snapshot);
            }
        }
        self.dragging_handle = None;
        self.drag_origin = None;
    }

    // === Tool management ===

    /// Sets the active drawing tool. Pass `None` to exit drawing mode and
    /// return to selection/pointer mode. A tool cancels any in-progress drawing and leaves
    /// eraser mode.
    pub fn set_active_tool(&mut self, tool: Option<DrawingToolType>) {
        self.active_custom_tool = None;
        let tool = tool.filter(|tool| self.registry.availability(*tool).enabled);
        self.active_tool = tool;
        self.cancel_curr_drawing();
        if tool.is_some() {
            self.eraser_mode = false;
        }
    }

    /// The registered tool selected for the next placement, or `None` for a built-in tool.
    pub fn active_custom_tool(&self) -> Option<&str> {
        self.active_custom_tool.as_deref()
    }

    /// Starts a registered tool's multi-click construction. One-anchor tools complete on the
    /// first click. The host picker uses `registry.custom_tools()` for its ids and names.
    pub fn set_custom_tool(&mut self, id: &str) -> Result<(), &'static str> {
        if self.registry.get(id).is_none() {
            return Err("Unknown drawing tool");
        }
        self.cancel_curr_drawing();
        self.eraser_mode = false;
        self.active_tool = Some(DrawingToolType::Polyline);
        self.active_custom_tool = Some(id.into());
        Ok(())
    }

    /// Shows a registered tool's settings and records a changed draft as one undoable edit.
    /// Unknown tools and locked drawings show no editor and return false.
    pub fn show_custom_settings(&mut self, ui: &mut egui::Ui, id: usize) -> bool {
        let Some(drawing) = self.drawings.iter().find(|d| d.id == id && !d.locked) else {
            return false;
        };
        let Some(tool) = drawing
            .custom_tool
            .as_deref()
            .and_then(|id| self.registry.get(id))
        else {
            return false;
        };
        let mut settings = drawing.custom_settings.clone();
        if !tool.implementation.settings(ui, &mut settings) || settings == drawing.custom_settings {
            return false;
        }
        self.edit_drawing(id, |drawing| drawing.custom_settings = settings);
        true
    }

    /// Switches eraser mode on or off. In eraser mode a click on the chart deletes the
    /// drawing under the pointer, unless it is locked. Switching it on leaves the active tool
    /// and cancels any in-progress drawing; choosing a tool with [`Self::set_active_tool`]
    /// switches it off.
    pub fn set_eraser_mode(&mut self, on: bool) {
        self.eraser_mode = on;
        if on {
            self.active_custom_tool = None;
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
    /// Prepares a bundled icon, Unicode emoji or `sticker:` glyph,
    /// allowing the user to click on the chart to place it.
    pub fn start_icon_insertion(&mut self, icon_name: String) {
        let tool = if icon_name.starts_with("sticker:") {
            DrawingToolType::Sticker
        } else {
            DrawingToolType::FontIcon
        };
        if !self.registry.availability(tool).enabled {
            return;
        }
        self.set_active_tool(Some(tool));

        // Create a new drawing with the icon name
        let id = self.take_id();
        let mut drawing = Drawing::with_color(id, tool, self.options.default_color);

        if let Some(defaults) = self.tool_defaults.get(&drawing.tool_type) {
            let _ = defaults.apply_to(&mut drawing);
        }
        drawing.text = Some(icon_name);
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
        if self.active_custom_tool.is_none() && !self.registry.availability(tool_type).enabled {
            return;
        }
        let point = self.snap_point(point, &self.drawings);
        // A drawing prepared before its first anchor, such as a picked icon, starts here.
        let mut drawing = match self.curr_drawing.take() {
            Some(prepared) if prepared.tool_type == tool_type && prepared.points.is_empty() => {
                prepared
            }
            _ => {
                let mut drawing =
                    Drawing::with_color(self.take_id(), tool_type, self.options.default_color);
                if let Some(defaults) = self.tool_defaults.get(&tool_type) {
                    let _ = defaults.apply_to(&mut drawing);
                }
                drawing
            }
        };
        if let Some(id) = &self.active_custom_tool {
            drawing.custom_tool = Some(id.clone());
        }
        drawing.pane = self.pane;
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

        if let Some(tool) = drawing
            .custom_tool
            .as_deref()
            .and_then(|id| self.registry.get(id))
        {
            drawing.completed = drawing.chart_points.len() == tool.anchors;
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
            if let Some(tool) = drawing
                .custom_tool
                .as_deref()
                .and_then(|id| self.registry.get(id))
            {
                drawing.completed = drawing.chart_points.len() == tool.anchors;
            }
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
            && drawing.custom_tool.as_deref().map_or_else(
                || drawing.points.len() >= drawing.tool_type.anchors_needed().max(2),
                |id| {
                    self.registry
                        .get(id)
                        .is_some_and(|tool| drawing.points.len() == tool.anchors)
                },
            )
        {
            drawing.completed = true;
            self.finish_curr_drawing();
        }
    }

    /// Moves the completed current drawing to the drawing list and queues it to be saved.
    /// In keep-drawing mode the icon tool goes on placing the icon that was picked.
    fn finish_curr_drawing(&mut self) {
        if let Some(mut drawing) = self.curr_drawing.take() {
            if drawing.custom_tool.is_none()
                && !self.registry.availability(drawing.tool_type).enabled
            {
                return;
            }
            drawing.update_pane_coords(self.pane_rect);
            let icon = matches!(
                drawing.tool_type,
                DrawingToolType::FontIcon | DrawingToolType::Sticker
            )
            .then(|| drawing.text.clone())
            .flatten();
            let glyph_size = drawing.font_size;
            self.history.push_add(drawing.clone());
            let id = drawing.id;
            self.drawings.push(drawing);
            self.queue(DrawingSyncAction::SaveDrawing(id));
            if !self.stay_in_drawing_mode {
                self.active_tool = None;
                self.active_custom_tool = None;
            } else if let Some(icon) = icon {
                self.start_icon_insertion(icon);
                if let Some(prepared) = &mut self.curr_drawing {
                    prepared.font_size = glyph_size;
                }
            }
        }
    }

    /// Cancels the in-progress drawing without adding it to the drawing list.
    pub fn cancel_curr_drawing(&mut self) {
        self.curr_drawing = None;
        self.preview_point = false;
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

    /// Alias for [`Self::update_drag_handle`], with the same pointer mapping and price translation.
    pub fn update_drag_handle_with_coords<F, G, H>(
        &mut self,
        new_pos: Pos2,
        x_to_bar: F,
        y_to_price: G,
        translate_price: H,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
        H: Fn(f64, f32, f32) -> f64,
    {
        self.update_drag_handle(new_pos, x_to_bar, y_to_price, translate_price);
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
}
