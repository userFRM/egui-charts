//! Pan, zoom, and scroll interaction logic for the chart widget.
//!
//! This module implements all viewport manipulation interactions:
//!
//! - **Mouse wheel** -- vertical scroll zooms the time axis (or the price axis
//!   when hovering over it); horizontal scroll pans; Shift+wheel pans.
//! - **Pinch-to-zoom** -- trackpad / mobile two-finger pinch gesture.
//! - **Drag-to-pan** -- left-click drag scrolls the chart; dragging on the
//!   price axis scales prices; dragging on the time axis zooms time.
//! - **Kinetic scrolling** -- momentum-based animation after a flick gesture.
//! - **Box zoom** -- left-click drag (in zoom mode) zooms into the selected
//!   rectangular region.
//! - **Double-click reset** -- double-click on the time axis jumps to latest;
//!   double-click on the price axis re-enables auto-scale.
//! - **Timescale config** -- applies pending visible-bar and width settings.

use egui::{Pos2, Rect, Response, Ui};
use web_time::Instant;

use super::helpers::{apply_price_zoom, y_to_price};
use super::state::BoxZoomMode;
use crate::widget::Chart;

impl Chart {
    /// Handle mouse wheel events for time-axis zoom, price-axis zoom, and horizontal pan.
    ///
    /// Returns `Some(delta_y)` when the wheel was over the price axis, signaling
    /// that price zoom should be applied downstream (deferred to avoid borrow conflicts).
    pub fn handle_mouse_wheel(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_width: f32,
        chart_rect_min_x: f32,
        price_axis_rect: Rect,
    ) -> Option<f32> {
        let mut pending_price_zoom = None;

        if !response.hovered()
            || (!self.chart_options.handle_scale.mouse_wheel
                && !self.chart_options.handle_scroll.mouse_wheel)
        {
            return pending_price_zoom;
        }

        let scroll_input = ui.input(|i| {
            let mut dx = if i.smooth_scroll_delta.x.abs() > 0.0 {
                i.smooth_scroll_delta.x
            } else {
                i.raw_scroll_delta.x
            };
            let mut dy = if i.smooth_scroll_delta.y.abs() > 0.0 {
                i.smooth_scroll_delta.y
            } else {
                i.raw_scroll_delta.y
            };

            // Treat Shift+Wheel as horizontal scroll for standard mice
            if i.modifiers.shift && dx.abs() < 0.1 && dy.abs() > 0.1 {
                dx = dy;
                dy = 0.0;
            }
            (
                dy,
                dx,
                i.pointer.hover_pos(),
                i.modifiers.command || i.modifiers.ctrl,
            )
        });
        let (delta_y, delta_x, hover_pos, _is_modifier_held) = scroll_input;

        let mut did_zoom = false;
        if delta_y.abs() > 0.1 {
            let shift_held = ui.input(|i| i.modifiers.shift);

            if shift_held && self.chart_options.handle_scroll.mouse_wheel {
                let pan_amount = -delta_y * 2.0;
                self.state.time_scale_mut().scroll_pixels(pan_amount);
            } else if self.chart_options.handle_scale.mouse_wheel
                && let Some(hp) = hover_pos.or_else(|| response.hover_pos())
            {
                if price_axis_rect.contains(hp) {
                    pending_price_zoom = Some(delta_y);
                } else {
                    let zoom_scale = (-delta_y / 100.0).clamp(-0.5, 0.5);
                    // Zoom centered on mouse position by default
                    // With Ctrl/Cmd held: anchor at right edge (for "zoom to latest" behavior)
                    let zoom_point_x = hp.x - chart_rect_min_x; // Always use mouse position

                    log::debug!(
                        "[ZOOM INPUT] mouse_x={:.1}, chart_min_x={:.1}, anchor_x={:.1}, chart_width={:.1}",
                        hp.x,
                        chart_rect_min_x,
                        zoom_point_x,
                        chart_width
                    );

                    self.state.time_scale_mut().zoom(
                        zoom_scale,
                        zoom_point_x,
                        chart_rect_min_x,
                        chart_width,
                    );
                    did_zoom = true;

                    // CRITICAL: Reset drag state after zoom to prevent jump
                    // If user was dragging while zooming, the old start_offset is now invalid
                    if self.scroll_start_offset.is_some() {
                        self.scroll_start_offset = Some(self.state.time_scale().right_offset());
                        self.scroll_start_pos = response.interact_pointer_pos();
                    }
                }
            }
        }

        if !did_zoom
            && delta_y.abs() < 0.1
            && delta_x.abs() > 0.1
            && self.chart_options.handle_scroll.mouse_wheel
        {
            self.state.time_scale_mut().scroll_pixels(delta_x);
        }

        pending_price_zoom
    }

    /// Handles pinch-to-zoom for touch/trackpad gestures
    ///
    /// This uses egui's multi-touch zoom_delta which is provided by trackpad
    /// pinch gestures and mobile two-finger pinch.
    pub fn handle_pinch_zoom(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_width: f32,
        chart_rect_min_x: f32,
    ) {
        if !response.hovered() || !self.chart_options.handle_scale.pinch {
            return;
        }

        // Get multi-touch zoom delta (1.0 = no zoom, >1.0 = zoom in, <1.0 = zoom out)
        let zoom_info = ui.input(|i| {
            i.multi_touch()
                .map(|mt| (mt.zoom_delta, mt.translation_delta, i.pointer.hover_pos()))
        });

        if let Some((zoom_delta, translation_delta, hover_pos)) = zoom_info {
            // Handle pinch zoom
            if (zoom_delta - 1.0).abs() > 0.001 {
                // Convert zoom_delta to our zoom scale format
                // zoom_delta > 1.0 means fingers spreading (zoom in)
                // zoom_delta < 1.0 means fingers pinching (zoom out)
                let zoom_scale = (zoom_delta - 1.0) * 2.0;

                // Use hover position or center of chart as zoom anchor
                let zoom_point_x = hover_pos
                    .map(|p| p.x - chart_rect_min_x)
                    .unwrap_or(chart_width / 2.0);

                log::debug!(
                    "[PINCH ZOOM] zoom_delta={:.4}, zoom_scale={:.4}, anchor_x={:.1}",
                    zoom_delta,
                    zoom_scale,
                    zoom_point_x
                );

                self.state.time_scale_mut().zoom(
                    zoom_scale,
                    zoom_point_x,
                    chart_rect_min_x,
                    chart_width,
                );

                // Reset drag state to prevent jump after pinch
                if self.scroll_start_offset.is_some() {
                    self.scroll_start_offset = Some(self.state.time_scale().right_offset());
                    self.scroll_start_pos = response.interact_pointer_pos();
                }
            }

            // Handle two-finger pan (translation during pinch)
            if translation_delta.x.abs() > 0.5 {
                self.state
                    .time_scale_mut()
                    .scroll_pixels(translation_delta.x);
            }
        }
    }

    /// Handle drag-to-pan, price-axis scaling, and time-axis drag-zoom.
    ///
    /// When the drag starts on the main chart area, horizontal movement pans the
    /// time axis. Starting on the price axis enables price scale dragging.
    /// Starting on the time axis enables drag-to-zoom the time scale. Also
    /// tracks velocity for kinetic scrolling continuation after release.
    pub fn handle_drag_pan(
        &mut self,
        ui: &Ui,
        response: &Response,
        price_axis_rect: Rect,
        time_axis_rect: Rect,
        chart_rect_min_x: f32,
        has_active_drawing_tool: bool,
    ) {
        // CRITICAL: Skip panning if box zoom is active (right-click drag)
        let box_zoom_active = self.box_zoom.active;

        if !response.dragged()
            || !self.chart_options.handle_scroll.pressed_mouse_move
            || has_active_drawing_tool
            || box_zoom_active
        {
            // Drag ended - handle kinetic scrolling
            if !has_active_drawing_tool
                && self.chart_options.kinetic_scroll.enabled
                && self.scroll_start_pos.is_some()
            {
                let is_trackpad_gesture =
                    ui.input(|i| i.multi_touch().is_some() || i.any_touches());

                if is_trackpad_gesture {
                    let move_distance = self
                        .kinetic_scroll
                        .last_pos
                        .and_then(|last| {
                            self.scroll_start_pos.map(|start| (last.x - start.x).abs())
                        })
                        .unwrap_or(0.0);

                    let min_v_px_s = self.chart_options.kinetic_scroll.min_scroll_speed * 60.0;
                    let max_v_px_s = self.chart_options.kinetic_scroll.max_scroll_speed * 60.0;

                    if move_distance >= self.chart_options.kinetic_scroll.scroll_min_move
                        && self.kinetic_scroll.velocity.abs() >= min_v_px_s
                    {
                        self.kinetic_scroll.is_active = true;
                        self.kinetic_scroll.velocity =
                            self.kinetic_scroll.velocity.clamp(-max_v_px_s, max_v_px_s);
                        self.kinetic_scroll.anim_last_time = Some(Instant::now());
                    }
                }
            }

            self.scroll_start_pos = None;
            self.scroll_start_offset = None;
            self.kinetic_scroll.last_pos = None;
            self.kinetic_scroll.last_time = None;
            return;
        }

        // Stop kinetic animation when user starts dragging
        self.kinetic_scroll.is_active = false;

        if self.scroll_start_pos.is_none() {
            self.scroll_start_pos = response.interact_pointer_pos();
            self.scroll_start_offset = Some(self.state.time_scale().right_offset());
        }

        if let (Some(start_pos), Some(start_offset), Some(curr_pos)) = (
            self.scroll_start_pos,
            self.scroll_start_offset,
            response.interact_pointer_pos(),
        ) {
            if price_axis_rect.contains(start_pos)
                && self
                    .chart_options
                    .handle_scale
                    .axis_pressed_mouse_move
                    .price
            {
                self.price_scale_drag_start = Some(start_pos);
            } else if time_axis_rect.contains(start_pos)
                && self.chart_options.handle_scale.axis_pressed_mouse_move.time
            {
                let dx = curr_pos.x - start_pos.x;
                if dx.abs() > 0.1 {
                    let zoom_point_x = curr_pos.x - chart_rect_min_x;
                    let zoom_scale = (dx / 100.0).signum() * (dx / 100.0).abs().min(1.0);
                    let chart_width = time_axis_rect.width();
                    self.state.time_scale_mut().zoom(
                        zoom_scale,
                        zoom_point_x,
                        chart_rect_min_x,
                        chart_width,
                    );
                    self.scroll_start_pos = Some(curr_pos);
                }
            } else {
                let drag_delta_x = curr_pos.x - start_pos.x;
                let bar_spacing = self.state.time_scale().bar_spacing();
                let drag_in_bars = drag_delta_x / bar_spacing;
                let new_offset = start_offset - drag_in_bars;

                self.state.time_scale_mut().set_right_offset(new_offset);
            }

            // Track velocity for kinetic scrolling
            if self.chart_options.kinetic_scroll.enabled {
                let now = Instant::now();
                if let (Some(last_pos), Some(last_time)) =
                    (self.kinetic_scroll.last_pos, self.kinetic_scroll.last_time)
                {
                    let dt = now.duration_since(last_time).as_secs_f32();
                    if dt > 0.0 {
                        let dx = curr_pos.x - last_pos.x;
                        self.kinetic_scroll.velocity = dx / dt;
                    }
                }
                self.kinetic_scroll.last_pos = Some(curr_pos);
                self.kinetic_scroll.last_time = Some(now);
            }
        }
    }

    /// Advance the kinetic scrolling animation by one frame.
    ///
    /// Applies the current velocity to the right-offset, then damps the velocity
    /// using the configured damping coefficient. Stops when velocity drops below
    /// the minimum threshold. Requests a repaint to keep the animation running.
    pub fn apply_kinetic_scroll(&mut self, ui: &Ui) {
        if !self.chart_options.kinetic_scroll.enabled || !self.kinetic_scroll.is_active {
            return;
        }

        let now = Instant::now();
        let dt = if let Some(t0) = self.kinetic_scroll.anim_last_time {
            now.duration_since(t0).as_secs_f32().max(0.0)
        } else {
            self.kinetic_scroll.anim_last_time = Some(now);
            0.0
        };
        self.kinetic_scroll.anim_last_time = Some(now);

        if dt > 0.0 {
            let velocity_in_bars_per_s =
                self.kinetic_scroll.velocity / self.state.time_scale().bar_spacing();
            let delta_offset = velocity_in_bars_per_s * dt;
            let curr_offset = self.state.time_scale().right_offset();
            let new_offset = curr_offset - delta_offset;

            self.state.time_scale_mut().set_right_offset(new_offset);

            let frames = (dt * 60.0).max(0.0);
            let damping = self.chart_options.kinetic_scroll.dumping_coeff.powf(frames);
            self.kinetic_scroll.velocity *= damping;

            let min_v_px_s = self.chart_options.kinetic_scroll.min_scroll_speed * 60.0;
            if self.kinetic_scroll.velocity.abs() < min_v_px_s {
                self.kinetic_scroll.is_active = false;
                self.kinetic_scroll.velocity = 0.0;
                self.kinetic_scroll.anim_last_time = None;
            }
        }

        ui.ctx().request_repaint();
    }

    /// Reset axes on double-click: time axis jumps to latest, price axis re-enables auto-scale.
    pub fn handle_double_click(
        &mut self,
        response: &Response,
        price_axis_rect: Rect,
        time_axis_rect: Rect,
    ) {
        if !response.double_clicked() {
            return;
        }

        if let Some(pos) = response.interact_pointer_pos() {
            if self.chart_options.handle_scale.axis_double_click_reset.time
                && time_axis_rect.contains(pos)
            {
                self.state.time_scale_mut().jump_to_latest();
                self.state
                    .time_scale_mut()
                    .set_bar_spacing(self.chart_options.time_scale.bar_spacing);
            }
            if self
                .chart_options
                .handle_scale
                .axis_double_click_reset
                .price
                && price_axis_rect.contains(pos)
            {
                self.state.set_price_auto_scale(true);
            }
        }
    }

    /// Handle box-zoom interaction (left-click drag when zoom mode is active).
    ///
    /// On press, starts tracking the selection rectangle. On release, either
    /// zooms into the rectangle (Zoom mode) or measures price/bar delta (Measure mode).
    /// Returns `true` if a zoom was successfully applied this frame.
    pub fn handle_box_zoom(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_rect: Rect,
        chart_width: f32,
        zoom_mode_active: bool,
    ) -> bool {
        // Use left-click (primary) when zoom mode is active, otherwise don't trigger
        let btn_pressed = zoom_mode_active && ui.input(|i| i.pointer.primary_pressed());
        let btn_down = zoom_mode_active && ui.input(|i| i.pointer.primary_down());
        let btn_released = zoom_mode_active && ui.input(|i| i.pointer.primary_released());

        if btn_pressed
            && let Some(pos) = response.interact_pointer_pos()
            && chart_rect.contains(pos)
        {
            self.box_zoom.active = true;
            self.box_zoom.start_pos = Some(pos);
            self.box_zoom.curr_pos = Some(pos);
        }

        if btn_down
            && self.box_zoom.active
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.box_zoom.curr_pos = Some(pos);
        }

        let mut zoom_applied = false;
        if btn_released && self.box_zoom.active {
            if let (Some(start), Some(end)) = (self.box_zoom.start_pos, self.box_zoom.curr_pos)
                && (chart_rect.contains(start) || chart_rect.contains(end))
            {
                match self.box_zoom.mode {
                    BoxZoomMode::Zoom => {
                        zoom_applied = self.execute_box_zoom(start, end, chart_rect, chart_width);
                    }
                    BoxZoomMode::Measure => {
                        // TODO(P1): Display measurement overlay showing price delta, percentage
                        // change, and bar count between start/end of the box-select region.
                        // Render as a floating tooltip anchored to the selection rectangle,
                        // similar to a Measure tool (Alt+click drag).
                    }
                }
            }
            self.box_zoom.reset();
        }

        zoom_applied
    }

    /// Execute box-zoom: compute new bar spacing and right-offset to fill the viewport
    /// with the selected region, and apply price zoom to match the vertical selection.
    ///
    /// Returns `false` if the selection rectangle is too small (< 20px in either dimension).
    pub fn execute_box_zoom(
        &mut self,
        start: Pos2,
        end: Pos2,
        chart_rect: Rect,
        chart_width: f32,
    ) -> bool {
        let min_x = start.x.min(end.x);
        let max_x = start.x.max(end.x);
        let min_y = start.y.min(end.y);
        let max_y = start.y.max(end.y);

        if (max_x - min_x).abs() <= 20.0 || (max_y - min_y).abs() <= 20.0 {
            return false; // Zoom box too small
        }

        // Save current zoom state to history before applying new zoom
        self.state.push_zoom_state();

        let left_x_relative = min_x - chart_rect.min.x;
        let right_x_relative = max_x - chart_rect.min.x;

        let left_idx = self
            .state
            .time_scale()
            .coord_to_idx(min_x, chart_rect.min.x, chart_width);
        let right_idx = self
            .state
            .time_scale()
            .coord_to_idx(max_x, chart_rect.min.x, chart_width);

        let num_bars = right_idx.max(left_idx) - right_idx.min(left_idx) + 1.0;
        if num_bars > 0.0 {
            let new_spacing = (right_x_relative - left_x_relative) / num_bars;

            let min_spacing = self.chart_options.time_scale.min_bar_spacing;
            let max_spacing = self.chart_options.time_scale.max_bar_spacing;
            let clamped_spacing = if max_spacing > 0.0 {
                new_spacing.clamp(min_spacing, max_spacing)
            } else {
                new_spacing.max(min_spacing)
            };

            self.state.time_scale_mut().set_bar_spacing(clamped_spacing);

            let center_idx = (left_idx + right_idx) / 2.0;
            let base_idx = self.state.time_scale().base_idx() as f32;
            let visible_bars = chart_width / clamped_spacing;
            let target_right_offset = center_idx + (visible_bars / 2.0) - base_idx;
            self.state
                .time_scale_mut()
                .set_right_offset(target_right_offset);
        }

        // Handle vertical (price) zoom
        let price_min_y = max_y;
        let price_max_y = min_y;

        let price_range_height = chart_rect.height();
        let price_min_ratio = (chart_rect.max.y - price_min_y) / price_range_height;
        let price_max_ratio = (chart_rect.max.y - price_max_y) / price_range_height;

        let (curr_min, curr_max) = self.state.price_range();
        let curr_range = curr_max - curr_min;

        let sel_min_price = curr_min + (price_min_ratio as f64 * curr_range);
        let sel_max_price = curr_min + (price_max_ratio as f64 * curr_range);

        self.state.set_price_range(sel_min_price, sel_max_price);

        true // Zoom successfully applied
    }

    /// Apply deferred price-axis zoom from mouse wheel or price-axis drag.
    ///
    /// Called after all input processing to avoid mutable borrow conflicts.
    /// Updates the chart's price range and returns the new `(min, max)`.
    pub fn apply_price_zoom(
        &mut self,
        pending_price_zoom: Option<f32>,
        response: &Response,
        chart_rect: Rect,
        adjusted_min: f64,
        adjusted_max: f64,
    ) -> (f64, f64) {
        let mut new_min = adjusted_min;
        let mut new_max = adjusted_max;

        if let Some(delta_y) = pending_price_zoom
            && let Some(hp) = response.hover_pos()
        {
            let anchor_price = y_to_price(hp.y, adjusted_min, adjusted_max, chart_rect);
            let (min, max) = apply_price_zoom(
                (adjusted_min, adjusted_max),
                anchor_price,
                delta_y,
                chart_rect.height(),
            );
            self.state.set_price_range(min, max);
            new_min = min;
            new_max = max;
        }

        if let Some(start_pos) = self.price_scale_drag_start.take()
            && let Some(curr_pos) = response.interact_pointer_pos()
        {
            let dy = curr_pos.y - start_pos.y;
            let anchor_price = y_to_price(start_pos.y, new_min, new_max, chart_rect);
            let (min, max) =
                apply_price_zoom((new_min, new_max), anchor_price, dy, chart_rect.height());
            self.state.set_price_range(min, max);
            new_min = min;
            new_max = max;
        }

        (new_min, new_max)
    }

    /// Apply pending timescale configuration: width, initial visible bars,
    /// lock-on-resize behavior, and pending start-index jumps.
    pub fn apply_timescale_config(&mut self, chart_width: f32) {
        self.state.time_scale_mut().set_width(chart_width);

        if self.apply_visible_bars_once {
            if let Some(desired) = self.desired_visible_bars
                && desired > 0
            {
                let mut spacing = self.calculate_bar_spacing(chart_width, desired);
                let min = self.chart_options.time_scale.min_bar_spacing;
                let max = self.chart_options.time_scale.max_bar_spacing;
                spacing = if max > 0.0 {
                    spacing.clamp(min, max)
                } else {
                    spacing.max(min)
                };
                self.state.time_scale_mut().set_bar_spacing(spacing);
            }
            self.apply_visible_bars_once = false;
        }

        // lockVisibleTimeRangeOnResize
        if self
            .chart_options
            .time_scale
            .lock_visible_time_range_on_resize
        {
            if let Some(prev_width) = self.prev_width
                && (chart_width - prev_width).abs() > 1.0
            {
                let prev_visible_bars = self.calculate_visible_bars(prev_width);
                if prev_visible_bars > 0 {
                    let mut spacing = chart_width / prev_visible_bars as f32;
                    let min = self.chart_options.time_scale.min_bar_spacing;
                    let max = self.chart_options.time_scale.max_bar_spacing;
                    spacing = if max > 0.0 {
                        spacing.clamp(min, max)
                    } else {
                        spacing.max(min)
                    };
                    self.state.time_scale_mut().set_bar_spacing(spacing);
                }
            }
            self.prev_width = Some(chart_width);
        }

        // Apply pending start index
        if let Some(target_start) = self.pending_start_idx.take() {
            let bar_spacing = self.state.time_scale().bar_spacing();
            if bar_spacing > 0.0 {
                let visible_len = chart_width / bar_spacing;
                let base_idx = self.state.time_scale().base_idx() as f32;
                let desired_right_border = (target_start as f32) + visible_len - 1.0;
                let desired_offset = desired_right_border - base_idx;
                self.state.time_scale_mut().set_right_offset(desired_offset);
            }
        }
    }
}
