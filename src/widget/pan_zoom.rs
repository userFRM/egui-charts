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

use egui::{Event, MouseWheelUnit, Pos2, Rect, Response, Ui, Vec2, vec2};
use web_time::Instant;

use crate::scales::PriceTransform;
use crate::widget::Chart;

/// This frame's wheel scroll in points before egui smooths it: line and page units converted, and
/// the axis forced by egui's horizontal and vertical scroll modifiers. Unlike
/// `smooth_scroll_delta` it includes scrolling with the zoom modifier (Ctrl, or Cmd on macOS),
/// which egui turns into zoom instead.
pub(super) fn raw_wheel_delta(ui: &Ui) -> Vec2 {
    let options = ui.ctx().options(|o| o.input_options);
    ui.input(|i| {
        let mut sum = Vec2::ZERO;
        for event in &i.events {
            if let Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            {
                let mut delta = match unit {
                    MouseWheelUnit::Point => *delta,
                    MouseWheelUnit::Line => options.line_scroll_speed * *delta,
                    MouseWheelUnit::Page => i.viewport_rect().height() * *delta,
                };
                let horizontal = modifiers.matches_any(options.horizontal_scroll_modifier);
                let vertical = modifiers.matches_any(options.vertical_scroll_modifier);
                if horizontal && !vertical {
                    delta = vec2(delta.x + delta.y, 0.0);
                } else if vertical && !horizontal {
                    delta = vec2(0.0, delta.x + delta.y);
                }
                sum += delta;
            }
        }
        sum
    })
}

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

        let raw = raw_wheel_delta(ui);
        let scroll_input = ui.input(|i| {
            let mut dx = if i.smooth_scroll_delta.x.abs() > 0.0 {
                i.smooth_scroll_delta.x
            } else {
                raw.x
            };
            let mut dy = if i.smooth_scroll_delta.y.abs() > 0.0 {
                i.smooth_scroll_delta.y
            } else {
                raw.y
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
                    // Zoom around the pointer, or the right edge while the right bar stays on scroll
                    let zoom_point_x = self.zoom_anchor(hp.x - chart_rect_min_x, chart_width);

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
                        self.scroll_start_offset =
                            Some(self.state.time_scale().right_offset_precise());
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
    /// Native zoom events and contact-based pinches share the pointer anchor.
    /// Contact gestures take precedence when a backend reports both.
    pub fn handle_pinch_zoom(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_width: f32,
        chart_rect_min_x: f32,
    ) {
        if !response.hovered() {
            return;
        }

        // Get multi-touch zoom delta (1.0 = no zoom, >1.0 = zoom in, <1.0 = zoom out)
        let zoom_info = ui.input(|i| {
            i.multi_touch()
                .map(|mt| (mt.zoom_delta, mt.translation_delta, i.pointer.hover_pos()))
                .or_else(|| {
                    // Wheel zoom is handled separately, including its Ctrl/Cmd variant.
                    let native_zoom: f32 = i
                        .events
                        .iter()
                        .filter_map(|event| {
                            if let Event::Zoom(delta) = event {
                                Some(*delta)
                            } else {
                                None
                            }
                        })
                        .product();
                    (native_zoom != 1.0).then_some((native_zoom, Vec2::ZERO, i.pointer.hover_pos()))
                })
        });

        if let Some((zoom_delta, translation_delta, hover_pos)) = zoom_info {
            // Handle pinch zoom
            if self.chart_options.handle_scale.pinch && (zoom_delta - 1.0).abs() > 0.001 {
                // Convert zoom_delta to our zoom scale format
                // zoom_delta > 1.0 means fingers spreading (zoom in)
                // zoom_delta < 1.0 means fingers pinching (zoom out)
                let zoom_scale = (zoom_delta - 1.0) * 2.0;
                self.pending_scroll_time = None;
                self.tracking_mode_active = false;
                self.kinetic_scroll.is_active = false;

                // Use hover position or center of chart as zoom anchor
                let zoom_point_x = self.zoom_anchor(
                    hover_pos.map_or(chart_width / 2.0, |p| p.x - chart_rect_min_x),
                    chart_width,
                );

                log::debug!(
                    "[PINCH ZOOM] zoom_delta={zoom_delta:.4}, zoom_scale={zoom_scale:.4}, anchor_x={zoom_point_x:.1}"
                );

                self.state.time_scale_mut().zoom(
                    zoom_scale,
                    zoom_point_x,
                    chart_rect_min_x,
                    chart_width,
                );

                // Reset drag state to prevent jump after pinch
                if self.scroll_start_offset.is_some() {
                    self.scroll_start_offset = Some(self.state.time_scale().right_offset_precise());
                    self.scroll_start_pos = response.interact_pointer_pos();
                }
            }

            // Handle two-finger pan (translation during pinch)
            if self.chart_options.handle_scroll.horz_touch_drag && translation_delta.x.abs() > 0.5 {
                self.pending_scroll_time = None;
                self.state
                    .time_scale_mut()
                    .scroll_pixels(translation_delta.x);
            }
            if self.chart_options.handle_scroll.vert_touch_drag
                && self.last_rendered_price_rect.height() > 0.0
                && self.state.price_scale().is_manual()
                && translation_delta.y.abs() > 0.5
            {
                let y = hover_pos.map_or(0.0, |pos| pos.y);
                let map = self.last_price_map;
                let shift = map.y_to_scale(y - translation_delta.y) - map.y_to_scale(y);
                let range = self.state.price_scale().range();
                self.state
                    .price_scale_mut()
                    .set_range(range.min + shift, range.max + shift);
            }
        }
    }

    /// Handle drag-to-pan, price-axis scaling, and time-axis drag-zoom.
    ///
    /// When the drag starts on the main chart area, horizontal movement pans the
    /// time axis and, while auto-scale is off, vertical movement pans the price
    /// axis. Starting on the price axis enables price scale dragging.
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
        let (is_touch, multi_touch) = ui.input(|i| {
            (
                i.any_touches()
                    || i.events
                        .iter()
                        .any(|event| matches!(event, Event::Touch { .. })),
                i.multi_touch().is_some(),
            )
        });

        // Touch and mouse drags pan and continue under their own options.
        let kinetic = if is_touch {
            self.chart_options.handle_scroll.horz_touch_drag
                && self.chart_options.kinetic_scroll.enabled
        } else {
            self.chart_options.handle_scroll.pressed_mouse_move
                && self.chart_options.kinetic_scroll.mouse
        };

        if !response.dragged() || multi_touch || has_active_drawing_tool || box_zoom_active {
            // Drag ended - handle kinetic scrolling
            if response.drag_stopped()
                && !has_active_drawing_tool
                && !multi_touch
                && kinetic
                && self.scroll_start_pos.is_some()
            {
                let move_distance = self
                    .kinetic_scroll
                    .last_pos
                    .and_then(|last| self.scroll_start_pos.map(|start| (last.x - start.x).abs()))
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

            if multi_touch {
                self.kinetic_scroll.is_active = false;
                // Rebase the pointer drag while contacts own translation, including finger lifts.
                self.scroll_start_pos = response.interact_pointer_pos();
                self.scroll_start_offset = Some(self.state.time_scale().right_offset_precise());
            } else {
                self.scroll_start_pos = None;
                self.scroll_start_offset = None;
            }
            self.kinetic_scroll.last_pos = None;
            self.kinetic_scroll.last_time = None;
            return;
        }

        // Stop kinetic animation when user starts dragging
        self.kinetic_scroll.is_active = false;

        if self.scroll_start_pos.is_none() {
            self.scroll_start_pos = ui.input(|i| i.pointer.press_origin());
            self.scroll_start_offset = Some(self.state.time_scale().right_offset_precise());
        }

        if let (Some(start_pos), Some(start_offset), Some(curr_pos)) = (
            self.scroll_start_pos,
            self.scroll_start_offset,
            response.interact_pointer_pos(),
        ) {
            let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(start_pos);
            if price_axis_rect.contains(origin) {
                if self
                    .chart_options
                    .handle_scale
                    .axis_pressed_mouse_move
                    .price
                {
                    // Zoom by this frame's movement around the point where the drag began.
                    self.price_axis_drag = Some((origin.y, response.drag_delta().y));
                }
            } else if time_axis_rect.contains(origin) {
                if self.chart_options.handle_scale.axis_pressed_mouse_move.time {
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
                }
            } else {
                let scroll = self.chart_options.handle_scroll;
                let horizontal = if is_touch {
                    scroll.horz_touch_drag
                } else {
                    scroll.pressed_mouse_move
                };
                let vertical = if is_touch {
                    scroll.vert_touch_drag
                } else {
                    scroll.pressed_mouse_move
                };
                if horizontal {
                    let drag_delta_x = curr_pos.x - start_pos.x;
                    let bar_spacing = self.state.time_scale().bar_spacing();
                    let drag_in_bars = drag_delta_x / bar_spacing;
                    let new_offset = start_offset - f64::from(drag_in_bars);
                    self.state.time_scale_mut().set_right_offset(new_offset);

                    if kinetic {
                        let now = Instant::now();
                        if let (Some(last_pos), Some(last_time)) =
                            (self.kinetic_scroll.last_pos, self.kinetic_scroll.last_time)
                        {
                            let dt = now.duration_since(last_time).as_secs_f32();
                            if dt > 0.0 {
                                self.kinetic_scroll.velocity = (curr_pos.x - last_pos.x) / dt;
                            }
                        }
                        self.kinetic_scroll.last_pos = Some(curr_pos);
                        self.kinetic_scroll.last_time = Some(now);
                    }
                }

                // Manual prices follow only the movement enabled for this input device.
                if vertical
                    && self.last_rendered_price_rect.height() > 0.0
                    && self.state.price_scale().is_manual()
                {
                    let map = self.last_price_map;
                    let dy = response.drag_delta().y;
                    let shift = map.y_to_scale(curr_pos.y - dy) - map.y_to_scale(curr_pos.y);
                    let range = self.state.price_scale().range();
                    self.state
                        .price_scale_mut()
                        .set_range(range.min + shift, range.max + shift);
                }
            }
        }
    }

    /// Advance the kinetic scrolling animation by one frame.
    ///
    /// Applies the current velocity to the right-offset, then damps the velocity
    /// using the configured damping coefficient. Stops when velocity drops below
    /// the minimum threshold. Requests a repaint to keep the animation running.
    pub fn apply_kinetic_scroll(&mut self, ui: &Ui) {
        let options = self.chart_options.kinetic_scroll;
        if !(options.enabled || options.mouse) || !self.kinetic_scroll.is_active {
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
            let curr_offset = self.state.time_scale().right_offset_precise();
            let new_offset = curr_offset - f64::from(delta_offset);

            self.state.time_scale_mut().set_right_offset(new_offset);

            let frames = (dt * 60.0).max(0.0);
            let damping = self.chart_options.kinetic_scroll.damping_coeff.powf(frames);
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
                // Spacing first: a pixel right offset is converted at the restored spacing.
                self.state
                    .time_scale_mut()
                    .set_bar_spacing(self.chart_options.time_scale.bar_spacing);
                self.state.time_scale_mut().jump_to_latest();
                self.pending_scroll_time = None;
            }
            if self
                .chart_options
                .handle_scale
                .axis_double_click_reset
                .price
                && price_axis_rect.contains(pos)
            {
                self.state.price_scale_mut().reset();
            }
        }
    }

    /// Handle box-zoom interaction (left-click drag when zoom mode is active).
    ///
    /// On press, starts tracking the selection rectangle. On release, zooms into the rectangle.
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
                zoom_applied = self.execute_box_zoom(start, end, chart_rect, chart_width);
            }
            self.box_zoom.reset();
        }

        zoom_applied
    }

    /// Execute box-zoom: compute new bar spacing and right-offset to fill the viewport
    /// with the selected region, and apply price zoom to match the vertical selection.
    ///
    /// Returns `false` if the selection rectangle is too small or a corner or width is non-finite.
    pub fn execute_box_zoom(
        &mut self,
        start: Pos2,
        end: Pos2,
        chart_rect: Rect,
        chart_width: f32,
    ) -> bool {
        if !(start.is_finite() && end.is_finite() && chart_width.is_finite()) {
            return false;
        }
        let min_x = start.x.min(end.x);
        let max_x = start.x.max(end.x);
        let min_y = start.y.min(end.y);
        let max_y = start.y.max(end.y);

        if (max_x - min_x).abs() <= 20.0 || (max_y - min_y).abs() <= 20.0 {
            return false; // Zoom box too small
        }

        // Save current zoom state to history before applying new zoom
        self.state.push_zoom_state();

        // Horizontal zoom: the box's bars fill the width (up to the maximum spacing), centred.
        // A box inside one bar shows that bar across the width, or keeps a wider spacing.
        let time_scale = self.state.time_scale_mut();
        let center_idx =
            time_scale.coord_to_idx_precise((min_x + max_x) / 2.0, chart_rect.min.x, chart_width);
        let old = time_scale.bar_spacing();
        time_scale.set_bar_spacing((old * chart_width / (max_x - min_x)).min(chart_width.max(old)));
        // The inverse of `coord_to_idx` at the middle of the width.
        let spacing = time_scale.bar_spacing();
        let base_idx = time_scale.base_idx() as f64;
        time_scale.set_right_offset(
            center_idx - base_idx + (f64::from(chart_width) / 2.0 - 1.0) / f64::from(spacing) - 0.5,
        );

        // Vertical zoom: the box's prices, in the units of the window it selects, whose first
        // visible close (the base of the relative modes) can differ from the one it was drawn on.
        let (p, q) = (
            self.last_price_map.y_to_price(min_y),
            self.last_price_map.y_to_price(max_y),
        );
        let transform = self.state.fit_price_scale(
            self.studies
                .indicators()
                .iter()
                .zip(&self.study_autoscale)
                .filter(|(_, autoscale)| **autoscale && !self.chart_type.is_time_independent())
                .map(|(study, _)| study.as_ref()),
        );
        let (a, b) = (transform.to_scale(p), transform.to_scale(q));
        let scale = self.state.price_scale_mut();
        // Widened around its middle to the span zooming stops at.
        let pad = (scale.min_span() - (a - b).abs()).max(0.0) / 2.0;
        scale.set_range(a.min(b) - pad, a.max(b) + pad);
        self.pending_scroll_time = None;

        true // Zoom successfully applied
    }

    /// Apply deferred price-axis zoom from mouse wheel or price-axis drag.
    ///
    /// Called after all input processing to avoid mutable borrow conflicts. A
    /// movement of `dy` points scales the range by `exp(dy / height)`, so a drag
    /// zooms by the same amount however many frames it spans.
    pub(crate) fn apply_price_zoom(
        &mut self,
        pending_price_zoom: Option<f32>,
        response: &Response,
        price_rect: Rect,
        transform: PriceTransform,
    ) {
        let wheel = pending_price_zoom.zip(response.hover_pos().map(|p| p.y));
        let drag = self.price_axis_drag.take().map(|(y, dy)| (dy, y));
        for (dy, anchor_y) in wheel.into_iter().chain(drag) {
            let scale = self.state.price_scale();
            let anchor = scale
                .map(transform, price_rect.min.y, price_rect.max.y)
                .y_to_scale(anchor_y);
            let factor = (dy as f64 / price_rect.height() as f64).exp();
            self.state.price_scale_mut().zoom(factor, anchor);
        }
    }

    /// Apply pending timescale configuration: width, initial visible bars,
    /// lock-on-resize behavior, and pending start-index jumps.
    pub fn apply_timescale_config(&mut self, chart_width: f32) {
        // Hidden or collapsed layouts cannot establish a zoom or a resize baseline.
        if !chart_width.is_finite() || chart_width <= 1.0 {
            return;
        }
        self.state.time_scale_mut().set_width(chart_width);

        if self.apply_visible_bars_once {
            if let Some(desired) = self.desired_visible_bars
                && desired > 0
            {
                let spacing = self.calculate_bar_spacing(chart_width, desired);
                self.state.time_scale_mut().set_bar_spacing(spacing);
                if self.prev_width.is_none() {
                    self.state.time_scale_mut().jump_to_latest();
                }
            }
            self.apply_visible_bars_once = false;
        } else if self
            .chart_options
            .time_scale
            .lock_visible_time_range_on_resize
            && let Some(prev_width) = self.prev_width
            && (chart_width - prev_width).abs() > 1.0
        {
            let spacing = self.state.time_scale().bar_spacing() * chart_width / prev_width;
            self.state.time_scale_mut().set_bar_spacing(spacing);
        }
        self.prev_width = Some(chart_width);

        if let Some(time) = self.pending_scroll_time
            && let Some(first) = self.state.data().bars.first()
        {
            let bars = &self.state.data().bars;
            let next = bars.partition_point(|bar| bar.time < time);
            let index = if next == 0 {
                0
            } else if next == bars.len() || time - bars[next - 1].time <= bars[next].time - time {
                next - 1
            } else {
                next
            };
            let needs_history = time < first.time
                && !self.symbol.is_empty()
                && !self.feed.history_complete
                && self
                    .feed_link
                    .as_ref()
                    .is_some_and(|link| !link.paging_stopped);
            let scale = self.state.time_scale_mut();
            let half_window = (chart_width / scale.bar_spacing() - 1.0) / 2.0;
            scale.set_right_offset(index as f64 - scale.base_idx() as f64 + f64::from(half_window));
            if !needs_history {
                self.pending_scroll_time = None;
            }
        }

        // Apply pending start index
        if let Some(target_start) = self.pending_start_idx.take() {
            let bar_spacing = self.state.time_scale().bar_spacing();
            if bar_spacing > 0.0 {
                let visible_len = chart_width / bar_spacing;
                let base_idx = self.state.time_scale().base_idx() as f64;
                let desired_right_border = target_start as f64 + f64::from(visible_len) - 1.0;
                let desired_offset = desired_right_border - base_idx;
                self.state.time_scale_mut().set_right_offset(desired_offset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::chart::series::SeriesSettings;
    use crate::scales::PriceScaleMode;
    use crate::widget::Chart;
    use crate::widget::test_frames::{chart, frame};
    use egui::{Event, Modifiers, PointerButton, Pos2, Rect};

    /// Native zoom and contact pinch each scale once, retaining the bar under the pointer.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn native_and_contact_pinch_keep_the_zoom_anchor() {
        for (contacts, native, enabled, tracking, kinetic, maximized) in [
            (false, true, true, false, false, false),
            (true, false, true, false, false, false),
            (true, true, true, false, false, false),
            (false, true, false, false, false, false),
            (false, true, true, true, false, false),
            (false, true, true, false, true, false),
            (false, true, true, false, false, true),
            (true, false, true, false, false, true),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            chart.chart_options.handle_scale.pinch = enabled;
            if maximized {
                let study = chart.add_indicator(Box::new(
                    crate::studies::CustomIndicator::new("Pane", |bars| {
                        vec![crate::studies::IndicatorValue::Single(50.0); bars.len()]
                    })
                    .with_overlay(false),
                ));
                chart.sync_panes();
                chart.pane_layout.maximized = chart.series_pane(study);
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let rect = if maximized {
                chart.get_rendered_indicator_panes()[0].chart_rect
            } else {
                chart.last_rendered_price_rect
            };
            let pointer = Pos2::new(rect.left() + rect.width() * 0.25, rect.center().y);
            frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pointer)]);
            let touch = |id, phase, x| Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(id),
                phase,
                pos: Pos2::new(x, pointer.y),
                force: None,
            };
            if contacts {
                frame(
                    &ctx,
                    &mut chart,
                    0.04,
                    vec![
                        touch(1, egui::TouchPhase::Start, pointer.x - 50.0),
                        touch(2, egui::TouchPhase::Start, pointer.x + 50.0),
                    ],
                );
            }
            if tracking {
                chart.enable_tracking_mode();
            }
            if kinetic {
                chart.chart_options.kinetic_scroll.enabled = true;
                chart.kinetic_scroll.is_active = true;
                chart.kinetic_scroll.velocity = 120.0;
                chart.kinetic_scroll.anim_last_time = Some(web_time::Instant::now());
            }
            let (spacing, offset) = chart.get_time_scale_state();
            let mut events = Vec::new();
            if contacts {
                events.extend([
                    touch(1, egui::TouchPhase::Move, pointer.x - 75.0),
                    touch(2, egui::TouchPhase::Move, pointer.x + 75.0),
                ]);
            }
            if native {
                events.push(Event::Zoom(1.5));
            }
            frame(&ctx, &mut chart, 0.06, events);
            assert!(!chart.kinetic_scroll.is_active);
            let (zoomed, moved) = chart.get_time_scale_state();
            // The existing pinch sensitivity maps a 1.5 gesture to a 10% spacing increase.
            let expected_spacing = if enabled { spacing * 1.1 } else { spacing };
            assert!(
                (zoomed - expected_spacing).abs() < 1e-5,
                "contacts={contacts}, native={native}, enabled={enabled}: {zoomed}"
            );
            let expected_offset =
                offset + (rect.right() - pointer.x - 1.0) * (1.0 / zoomed - 1.0 / spacing);
            assert!((moved - expected_offset).abs() < 1e-4);
            frame(&ctx, &mut chart, 0.08, vec![]);
            assert_eq!(chart.get_time_scale_state(), (zoomed, moved));
            if tracking {
                assert!(!chart.is_tracking_mode_active());
            }
        }
    }

    /// Startup and suspended layouts must not consume the requested zoom or move the right edge.
    #[test]
    fn degenerate_width_preserves_pending_view_and_resize_anchor() {
        for lock in [false, true] {
            for pixels in [None, Some(60.0)] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0], &[105.0]).visible_bars(100);
                let options = &mut chart.chart_options.time_scale;
                options.min_bar_spacing = 0.01;
                options.lock_visible_time_range_on_resize = lock;
                options.right_offset_pixels = pixels;
                let options = chart.chart_options.clone();
                chart = chart.with_chart_options(options);
                let show = |chart: &mut Chart, width| {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(width, 600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                        },
                    )
                    .drop_without_applying_deltas();
                };
                show(&mut chart, 0.0);
                show(&mut chart, 1.0);
                assert_eq!(chart.state.time_scale().bar_spacing(), 6.0);
                show(&mut chart, 1064.0);
                let width = chart.last_rendered_price_rect.width();
                let scale = chart.state.time_scale();
                let spacing = width / 100.0;
                assert!((scale.bar_spacing() - spacing).abs() < 1e-5);
                let offset = pixels.map_or(2.5, |px| px / spacing);
                assert!((scale.right_offset() - offset).abs() < 1e-5);
                chart.state.time_scale_mut().set_right_offset(-20.0);
                show(&mut chart, 0.0);
                show(&mut chart, 1.0);
                show(&mut chart, 1064.0);
                assert!((chart.state.time_scale().bar_spacing() - spacing).abs() < 1e-5);
                assert_eq!(chart.state.time_scale().right_offset(), -20.0);
                show(&mut chart, 1564.0);
                let expected = if lock {
                    chart.last_rendered_price_rect.width() / 100.0
                } else {
                    spacing
                };
                assert!((chart.state.time_scale().bar_spacing() - expected).abs() < 1e-5);
                assert_eq!(chart.state.time_scale().right_offset(), -20.0);
                chart.set_visible_bars(80);
                show(&mut chart, 1264.0);
                assert!(
                    (chart.state.time_scale().bar_spacing()
                        - chart.last_rendered_price_rect.width() / 80.0)
                        .abs()
                        < 1e-5,
                    "a pending view must take precedence over a locked resize"
                );
            }
        }
    }

    /// Drags down by 100 points in `steps` frames from the point `start` picks given the price pane.
    fn drag_down(chart: &mut Chart, start: fn(Rect) -> Pos2, steps: usize, touch: bool) {
        let ctx = egui::Context::default();
        frame(&ctx, chart, 0.0, Vec::new());
        let start = start(chart.last_rendered_price_rect);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut t = 0.0;
        let mut step = |chart: &mut _, events| {
            t += 0.02;
            frame(&ctx, chart, t, events);
        };
        step(chart, vec![Event::PointerMoved(start)]);
        let contact = |phase, pos| Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: None,
        };
        let mut events = vec![button(start, true)];
        if touch {
            events.push(contact(egui::TouchPhase::Start, start));
        }
        step(chart, events);
        let mut pos = start;
        for _ in 0..steps {
            pos.y += 100.0 / steps as f32;
            let mut events = vec![Event::PointerMoved(pos)];
            if touch {
                events.push(contact(egui::TouchPhase::Move, pos));
            }
            step(chart, events);
        }
        let mut events = vec![button(pos, false)];
        if touch {
            events.push(contact(egui::TouchPhase::End, pos));
        }
        step(chart, events);
    }

    /// Drags the price axis down by 100 points in `steps` frames and returns the resulting range.
    fn drag_price_axis(steps: usize, left: bool) -> (f64, f64) {
        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
        chart.config.show_left_axis = left;
        chart.config.show_right_axis = !left;
        drag_down(
            &mut chart,
            if left {
                |rect| Pos2::new(rect.left() - 10.0, rect.center().y)
            } else {
                |rect| Pos2::new(rect.right() + 10.0, rect.center().y)
            },
            steps,
            false,
        );
        let range = chart.state.price_scale().range();
        (range.min, range.max)
    }

    /// A double-click on the time axis of a chart zoomed to twice the configured spacing of 6
    /// returns to that spacing and to the live edge with the configured 60 px right offset,
    /// 10 bars at a spacing of 6.
    #[test]
    fn time_axis_double_click_restores_the_configured_right_offset() {
        let ctx = egui::Context::default();
        let options = crate::config::ChartOptions {
            time_scale: crate::config::TimeScaleOptions {
                bar_spacing: 6.0,
                right_offset_pixels: Some(60.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]).with_chart_options(options);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.state.time_scale_mut().set_bar_spacing(12.0);
        let (price, volume) = (
            chart.last_rendered_price_rect,
            chart.last_rendered_volume_rect,
        );
        let pos = Pos2::new(price.center().x, price.max.y.max(volume.max.y) + 5.0);
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut t = 0.0;
        for events in [
            vec![Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
            vec![button(true)],
            vec![button(false)],
            Vec::new(),
        ] {
            t += 0.02;
            frame(&ctx, &mut chart, t, events);
        }
        let time_scale = chart.state.time_scale();
        assert_eq!(time_scale.bar_spacing(), 6.0);
        assert_eq!(time_scale.right_offset(), 10.0);
    }

    /// A wheel notch over the chart zooms the time axis, also with the zoom modifier (Ctrl, or
    /// Cmd on macOS) held, which egui leaves out of its smoothed scroll delta.
    /// While the right bar stays on scroll, the zoom keeps the right offset.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn wheel_zooms_the_time_axis_with_and_without_the_zoom_modifier() {
        for (modifiers, right_bar_stays, fraction) in [
            (Modifiers::NONE, false, 0.25),
            (Modifiers::COMMAND, false, 0.75),
            (Modifiers::NONE, true, 0.25),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            chart.chart_options.time_scale.right_bar_stays_on_scroll = right_bar_stays;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let (spacing, offset) = chart.get_time_scale_state();
            let rect = chart.last_rendered_price_rect;
            let pointer = Pos2::new(rect.left() + rect.width() * fraction, rect.center().y);
            frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pointer)]);
            let wheel = Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers,
            };
            frame(&ctx, &mut chart, 0.04, vec![wheel]);
            for i in 3..10 {
                frame(&ctx, &mut chart, i as f64 * 0.02, Vec::new());
            }
            let (zoomed, moved) = chart.get_time_scale_state();
            assert!(
                zoomed > spacing,
                "{modifiers:?}: spacing {spacing} -> {zoomed}"
            );
            // Zooming around the centre moves the right edge unless the right bar stays.
            assert_eq!(
                moved == offset,
                right_bar_stays,
                "offset {offset} -> {moved}"
            );
            let anchor = if right_bar_stays {
                rect.right() - 1.0
            } else {
                pointer.x
            };
            let expected = offset + (rect.right() - anchor - 1.0) * (1.0 / zoomed - 1.0 / spacing);
            assert!(
                (moved - expected).abs() < 1e-4,
                "the logical bar at the zoom anchor must stay at the same pixel"
            );
        }
    }

    /// Zoom speed must not depend on how many frames a drag spans.
    #[test]
    fn price_axis_drag_zoom_is_frame_independent() {
        for left in [false, true] {
            let (one, ten) = (drag_price_axis(1, left), drag_price_axis(10, left));
            let span = one.1 - one.0;
            // Dragging down zooms out from the autoscaled 93.5..111.5 (95..110 plus 10% margins).
            assert!(span > 18.0, "1 frame: {one:?}");
            assert!(
                (one.0 - ten.0).abs() < span * 1e-9 && (one.1 - ten.1).abs() < span * 1e-9,
                "1 frame: {one:?}, 10 frames: {ten:?}"
            );
        }
    }

    /// With auto-scale off, the prices follow a drag on the chart: 100 points down moves the
    /// 20-wide range up by 20 * 100 / height, or down by as much on an inverted scale.
    #[test]
    fn chart_drag_pans_a_manual_price_range() {
        for (invert_scale, sign) in [(false, 1.0), (true, -1.0)] {
            for (touch, mouse, vertical) in [
                (false, true, false),
                (false, false, true),
                (true, false, true),
                (true, true, false),
            ] {
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                chart.apply_series_settings(&SeriesSettings {
                    invert_scale,
                    ..SeriesSettings::default()
                });
                chart.chart_options.handle_scroll.pressed_mouse_move = mouse;
                chart.chart_options.handle_scroll.vert_touch_drag = vertical;
                chart.chart_options.handle_scroll.horz_touch_drag = false;
                chart.chart_options.kinetic_scroll.enabled = false;
                chart.state.price_scale_mut().set_range(90.0, 110.0);
                drag_down(&mut chart, |rect| rect.center(), 10, touch);
                let enabled = if touch { vertical } else { mouse };
                let shift = if enabled {
                    sign * 20.0 * 100.0 / chart.last_rendered_price_rect.height() as f64
                } else {
                    0.0
                };
                let range = chart.state.price_scale().range();
                assert!(
                    (range.min - (90.0 + shift)).abs() < 1e-4
                        && (range.max - (110.0 + shift)).abs() < 1e-4,
                    "inverted {invert_scale}, touch={touch}, mouse={mouse}, vertical={vertical}: {range:?}, expected a shift of {shift}"
                );
            }
        }
    }

    /// A grabbed bar follows the entire horizontal drag, including its first movement frame.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn chart_drag_keeps_the_grabbed_bar_under_the_pointer() {
        for steps in [1, 10] {
            for (touch, mouse, horizontal) in [
                (false, true, false),
                (false, false, true),
                (true, false, true),
                (true, true, false),
            ] {
                for distance in [-60.0, 60.0] {
                    let ctx = egui::Context::default();
                    let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    chart.chart_options.handle_scroll.pressed_mouse_move = mouse;
                    chart.chart_options.handle_scroll.horz_touch_drag = horizontal;
                    chart.chart_options.handle_scroll.vert_touch_drag = false;
                    chart.chart_options.kinetic_scroll.enabled = false;
                    let contact = |phase, pos| Event::Touch {
                        device_id: egui::TouchDeviceId(1),
                        id: egui::TouchId(1),
                        phase,
                        pos,
                        force: None,
                    };
                    let start = chart.last_rendered_price_rect.center();
                    let (spacing, offset) = chart.get_time_scale_state();
                    let button = |pos, pressed| Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    };
                    frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(start)]);
                    let mut events = vec![button(start, true)];
                    if touch {
                        events.push(contact(egui::TouchPhase::Start, start));
                    }
                    frame(&ctx, &mut chart, 0.04, events);
                    let mut end = start;
                    for i in 1..=steps {
                        end.x = start.x + distance * i as f32 / steps as f32;
                        let mut events = vec![Event::PointerMoved(end)];
                        if touch {
                            events.push(contact(egui::TouchPhase::Move, end));
                        }
                        frame(&ctx, &mut chart, 0.04 + i as f64 * 0.02, events);
                    }
                    let mut events = vec![button(end, false)];
                    if touch {
                        events.push(contact(egui::TouchPhase::End, end));
                    }
                    frame(&ctx, &mut chart, 0.5, events);
                    // Moving 60 pixels at six pixels per bar moves the right offset by ten bars.
                    let enabled = if touch { horizontal } else { mouse };
                    let expected = offset - if enabled { distance / spacing } else { 0.0 };
                    let (after_spacing, after_offset) = chart.get_time_scale_state();
                    assert_eq!(after_spacing, spacing);
                    assert!(
                        (after_offset - expected).abs() < 1e-4,
                        "touch={touch}, mouse={mouse}, horizontal={horizontal}, {steps} steps, {distance} pixels: {after_offset}, expected {expected}"
                    );
                }
            }
        }
    }

    /// A fast horizontal drag continues after release only when inertia is enabled for its
    /// device, and a new drag stops it.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn released_drag_continues_under_its_device_inertia() {
        for touch in [false, true] {
            for (touch_inertia, mouse_inertia) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                chart.chart_options.kinetic_scroll.enabled = touch_inertia;
                chart.chart_options.kinetic_scroll.mouse = mouse_inertia;
                frame(&ctx, &mut chart, 0.0, vec![]);
                let start = chart.last_rendered_price_rect.center();
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                let contact = |phase, pos| Event::Touch {
                    device_id: egui::TouchDeviceId(1),
                    id: egui::TouchId(1),
                    phase,
                    pos,
                    force: None,
                };
                let input = |pointer: Event, phase, pos| {
                    let mut events = vec![pointer];
                    if touch {
                        events.push(contact(phase, pos));
                    }
                    events
                };
                frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(start)]);
                let press = input(button(start, true), egui::TouchPhase::Start, start);
                frame(&ctx, &mut chart, 0.04, press);
                let mut end = start;
                for i in 1..=4 {
                    end.x = start.x - 15.0 * i as f32;
                    let events = input(Event::PointerMoved(end), egui::TouchPhase::Move, end);
                    frame(&ctx, &mut chart, 0.04 + i as f64 * 0.02, events);
                }
                let release = input(button(end, false), egui::TouchPhase::End, end);
                frame(&ctx, &mut chart, 0.2, release);
                let expected = if touch { touch_inertia } else { mouse_inertia };
                assert_eq!(
                    chart.kinetic_scroll.is_active, expected,
                    "touch={touch}, touch inertia={touch_inertia}, mouse inertia={mouse_inertia}"
                );

                let press = input(button(end, true), egui::TouchPhase::Start, end);
                frame(&ctx, &mut chart, 0.22, press);
                end.x += 20.0;
                let events = input(Event::PointerMoved(end), egui::TouchPhase::Move, end);
                frame(&ctx, &mut chart, 0.24, events);
                assert!(!chart.kinetic_scroll.is_active);
            }
        }
    }

    /// Two contacts translate each enabled axis once, independently of pinch scaling.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn two_finger_translation_respects_touch_permissions() {
        for pinch in [false, true] {
            for (horizontal, vertical) in
                [(false, false), (false, true), (true, false), (true, true)]
            {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                chart.chart_options.handle_scroll.horz_touch_drag = horizontal;
                chart.chart_options.handle_scroll.vert_touch_drag = vertical;
                chart.chart_options.handle_scale.pinch = pinch;
                chart.chart_options.kinetic_scroll.enabled = false;
                chart.state.price_scale_mut().set_range(90.0, 110.0);
                frame(&ctx, &mut chart, 0.0, vec![]);
                let rect = chart.last_rendered_price_rect;
                let start = rect.center();
                let (spacing, offset) = chart.get_time_scale_state();
                let contact = |id, phase, pos| Event::Touch {
                    device_id: egui::TouchDeviceId(1),
                    id: egui::TouchId(id),
                    phase,
                    pos,
                    force: None,
                };
                frame(&ctx, &mut chart, 0.01, vec![Event::PointerMoved(start)]);
                frame(
                    &ctx,
                    &mut chart,
                    0.02,
                    vec![
                        Event::PointerMoved(start),
                        Event::PointerButton {
                            pos: start,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                        contact(1, egui::TouchPhase::Start, start),
                        contact(2, egui::TouchPhase::Start, start + egui::vec2(100.0, 0.0)),
                    ],
                );
                for step in 1..=2 {
                    let pos = start + egui::vec2(30.0, 30.0) * step as f32;
                    frame(
                        &ctx,
                        &mut chart,
                        0.02 + step as f64 * 0.02,
                        vec![
                            Event::PointerMoved(pos),
                            contact(1, egui::TouchPhase::Move, pos),
                            contact(2, egui::TouchPhase::Move, pos + egui::vec2(100.0, 0.0)),
                        ],
                    );
                }
                let pos = start + egui::vec2(60.0, 60.0);
                frame(
                    &ctx,
                    &mut chart,
                    0.08,
                    vec![contact(
                        2,
                        egui::TouchPhase::End,
                        pos + egui::vec2(100.0, 0.0),
                    )],
                );
                let pos = start + egui::vec2(90.0, 90.0);
                frame(
                    &ctx,
                    &mut chart,
                    0.10,
                    vec![
                        Event::PointerMoved(pos),
                        contact(1, egui::TouchPhase::Move, pos),
                    ],
                );
                let expected_offset = offset - if horizontal { 90.0 / spacing } else { 0.0 };
                let shift = if vertical {
                    20.0 * 90.0 / rect.height() as f64
                } else {
                    0.0
                };
                let range = chart.state.price_scale().range();
                assert!(
                    (chart.state.time_scale().right_offset() - expected_offset).abs() < 1e-4,
                    "pinch={pinch}, horizontal={horizontal}, vertical={vertical}"
                );
                assert!(
                    (range.min - 90.0 - shift).abs() < 1e-4
                        && (range.max - 110.0 - shift).abs() < 1e-4,
                    "pinch={pinch}, horizontal={horizontal}, vertical={vertical}: {range:?}"
                );
                assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
            }
        }
    }

    /// Axis drags keep their own permission after leaving the axis, including in narrow layouts.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn axis_drag_permissions_are_independent_of_plot_drag() {
        for (theme, width) in [
            (crate::theme::Theme::light(), 800.0),
            (crate::theme::Theme::dark(), 800.0),
            (crate::theme::Theme::light(), 360.0),
            (crate::theme::Theme::dark(), 360.0),
        ] {
            let frame = |ctx: &egui::Context, chart: &mut Chart, time, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            chart.show(ui);
                        });
                    },
                )
                .drop_without_applying_deltas();
            };
            for price in [false, true] {
                for plot in [false, true] {
                    for axis in [true, false] {
                        let ctx = egui::Context::default();
                        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                        chart.set_theme(&ctx, &theme);
                        chart.chart_options.handle_scroll.pressed_mouse_move = plot;
                        chart
                            .chart_options
                            .handle_scale
                            .axis_pressed_mouse_move
                            .price = axis;
                        chart
                            .chart_options
                            .handle_scale
                            .axis_pressed_mouse_move
                            .time = axis;
                        frame(&ctx, &mut chart, 0.0, vec![]);
                        let rect = chart.last_rendered_price_rect;
                        let start = if price {
                            Pos2::new(rect.right() + 10.0, rect.center().y)
                        } else {
                            Pos2::new(
                                rect.center().x,
                                rect.bottom().max(chart.last_rendered_volume_rect.bottom()) + 5.0,
                            )
                        };
                        let end = start
                            + if price {
                                egui::vec2(-100.0, 100.0)
                            } else {
                                egui::vec2(100.0, -100.0)
                            };
                        let before = chart.state.price_scale().range();
                        let before = before.max - before.min;
                        let (spacing, offset) = chart.get_time_scale_state();
                        let button = |pos, pressed| Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        };
                        frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(start)]);
                        frame(&ctx, &mut chart, 0.04, vec![button(start, true)]);
                        frame(
                            &ctx,
                            &mut chart,
                            0.06,
                            vec![Event::PointerMoved(start.lerp(end, 0.5))],
                        );
                        frame(&ctx, &mut chart, 0.08, vec![Event::PointerMoved(end)]);
                        frame(&ctx, &mut chart, 0.10, vec![button(end, false)]);
                        let expected_span = if price && axis {
                            before * (100.0 / rect.height() as f64).exp()
                        } else {
                            before
                        };
                        let expected_spacing = if !price && axis {
                            spacing * 1.05 * 1.05
                        } else {
                            spacing
                        };
                        assert!(
                            ((chart.state.price_scale().range().max
                                - chart.state.price_scale().range().min)
                                - expected_span)
                                .abs()
                                < 1e-5,
                            "price={price}, plot={plot}, axis={axis}"
                        );
                        assert!(
                            (chart.state.time_scale().bar_spacing() - expected_spacing).abs()
                                < 1e-5,
                            "price={price}, plot={plot}, axis={axis}"
                        );
                        if !axis {
                            assert_eq!(chart.state.time_scale().right_offset(), offset);
                        }
                    }
                }
            }
        }
    }

    /// The bars and prices inside a zoom box fill the pane on the next frame. The box re-centres
    /// the time window, so the relative modes rebase on another first visible close than the box
    /// was drawn on.
    #[test]
    fn box_zoom_fills_the_pane_with_the_selected_bars_and_prices() {
        let lows: Vec<f64> = (0..100).map(|i| 100.0 + i as f64).collect();
        let highs: Vec<f64> = lows.iter().map(|low| low + 1.0).collect();
        for mode in [PriceScaleMode::Normal, PriceScaleMode::Percentage] {
            let ctx = egui::Context::default();
            let mut chart = chart(&lows, &highs);
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: mode,
                ..SeriesSettings::default()
            });
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let rect = chart.last_rendered_price_rect;
            let before = chart.get_chart_mapping().price;
            let (y_hi, y_lo) = (rect.top() + 100.0, rect.top() + 200.0);
            let (hi, lo) = (before.y_to_price(y_hi), before.y_to_price(y_lo));
            let (start, end) = (
                Pos2::new(rect.right() - 150.0, y_hi),
                Pos2::new(rect.right() - 50.0, y_lo),
            );
            let idx = |x| {
                let ts = chart.state.time_scale();
                ts.coord_to_idx(x, rect.left(), rect.width())
            };
            let (left, right) = (idx(start.x), idx(end.x));
            assert!(chart.execute_box_zoom(start, end, rect, rect.width()));
            frame(&ctx, &mut chart, 0.1, Vec::new());
            let ts = chart.state.time_scale();
            let x = |idx| ts.idx_to_coord_precise(idx, rect.left(), rect.width());
            let (x_left, x_right) = (x(left), x(right));
            assert!(
                (x_left - rect.left()).abs() < 0.5 && (x_right - rect.right()).abs() < 0.5,
                "{mode:?}: bars {left}..{right} at x {x_left}..{x_right}, pane {rect:?}"
            );
            let after = chart.get_chart_mapping().price;
            let (top, bottom) = after.extent();
            let (y_hi, y_lo) = (after.price_to_y(hi), after.price_to_y(lo));
            assert!(
                (y_hi - top).abs() < 0.5 && (y_lo - bottom).abs() < 0.5,
                "{mode:?}: {lo}..{hi} at y {y_lo}..{y_hi}, pane {top}..{bottom}"
            );
        }

        // A box inside one bar zooms to one bar across the pane, no further.
        let ctx = egui::Context::default();
        let mut one_bar = chart(&lows, &highs);
        frame(&ctx, &mut one_bar, 0.0, Vec::new());
        let rect = one_bar.last_rendered_price_rect;
        one_bar.state.time_scale_mut().set_bar_spacing(40.0);
        let (x, y) = (rect.center().x, rect.top() + 100.0);
        let (start, end) = (Pos2::new(x, y), Pos2::new(x + 25.0, y + 100.0));
        assert!(one_bar.execute_box_zoom(start, end, rect, rect.width()));
        assert_eq!(one_bar.state.time_scale().bar_spacing(), rect.width());

        // Invalid corners or width must leave both scales and zoom history unchanged.
        let view = one_bar.get_time_scale_state();
        let range = one_bar.state.price_scale().range();
        one_bar.state.clear_zoom_history();
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for (start, end, width) in [
                (
                    Pos2::new(invalid, invalid),
                    Pos2::new(invalid, invalid),
                    rect.width(),
                ),
                (Pos2::new(invalid, y), end, rect.width()),
                (start, Pos2::new(x + 25.0, invalid), rect.width()),
                (start, end, invalid),
            ] {
                assert!(!one_bar.execute_box_zoom(start, end, rect, width));
                assert_eq!(one_bar.get_time_scale_state(), view);
                assert_eq!(one_bar.state.price_scale().range(), range);
                assert!(!one_bar.state.has_zoom_history());
            }
        }

        // A box over a range narrower than the zoom floor widens to it: a millionth of the
        // visible data's highest price, 110.
        let ctx = egui::Context::default();
        let mut narrow = chart(&[95.0, 100.0], &[105.0, 110.0]);
        narrow.state.price_scale_mut().set_range(100.0, 100.00001);
        frame(&ctx, &mut narrow, 0.0, Vec::new());
        let rect = narrow.last_rendered_price_rect;
        let (start, end) = (
            Pos2::new(rect.right() - 150.0, rect.top() + 100.0),
            Pos2::new(rect.right() - 50.0, rect.top() + 200.0),
        );
        assert!(narrow.execute_box_zoom(start, end, rect, rect.width()));
        let range = narrow.state.price_scale().range();
        assert!(
            (range.max - range.min - 1.1e-4).abs() < 1e-10,
            "{range:?}, expected a span of 1.1e-4"
        );
    }
}
