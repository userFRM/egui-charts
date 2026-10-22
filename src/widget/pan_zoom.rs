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

use crate::chart::state::BoxZoomMode;
use crate::scales::PriceTransform;
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
                // Zoom by this frame's movement around the point where the drag began.
                let anchor_y = ui
                    .input(|i| i.pointer.press_origin())
                    .unwrap_or(start_pos)
                    .y;
                self.price_axis_drag = Some((anchor_y, response.drag_delta().y));
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
                self.state.price_scale_mut().reset();
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

        // Vertical zoom: the box's prices, in the units of the window it selects, whose first
        // visible close (the base of the relative modes) can differ from the one it was drawn on.
        let (p, q) = (
            self.last_price_map.y_to_price(min_y),
            self.last_price_map.y_to_price(max_y),
        );
        let transform = self.state.fit_price_scale();
        let (a, b) = (transform.to_scale(p), transform.to_scale(q));
        self.state.price_scale_mut().set_range(a.min(b), a.max(b));

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

#[cfg(test)]
mod tests {
    use crate::chart::series::SeriesSettings;
    use crate::scales::PriceScaleMode;
    use crate::widget::test_frames::{chart, frame};
    use egui::{Event, Modifiers, PointerButton, Pos2};

    /// Drags the price axis down by 100 points in `steps` frames and returns the resulting range.
    fn drag_price_axis(steps: usize) -> (f64, f64) {
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        let rect = chart.last_rendered_price_rect;
        let start = Pos2::new(rect.max.x + 10.0, rect.center().y);
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
        step(&mut chart, vec![Event::PointerMoved(start)]);
        step(&mut chart, vec![button(start, true)]);
        let mut pos = start;
        for _ in 0..steps {
            pos.y += 100.0 / steps as f32;
            step(&mut chart, vec![Event::PointerMoved(pos)]);
        }
        step(&mut chart, vec![button(pos, false)]);
        let range = chart.state.price_scale().range();
        (range.min, range.max)
    }

    /// Zoom speed must not depend on how many frames a drag spans.
    #[test]
    fn price_axis_drag_zoom_is_frame_independent() {
        let (one, ten) = (drag_price_axis(1), drag_price_axis(10));
        let span = one.1 - one.0;
        // Dragging down zooms out from the autoscaled 93.5..111.5 (95..110 plus 10% margins).
        assert!(span > 18.0, "1 frame: {one:?}");
        assert!(
            (one.0 - ten.0).abs() < span * 1e-9 && (one.1 - ten.1).abs() < span * 1e-9,
            "1 frame: {one:?}, 10 frames: {ten:?}"
        );
    }

    /// The prices inside a zoom box fill the pane on the next frame. The box re-centres the time
    /// window, so the relative modes rebase on another first visible close than the box was drawn on.
    #[test]
    fn box_zoom_fills_the_pane_with_the_selected_prices() {
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
            assert!(chart.execute_box_zoom(start, end, rect, rect.width()));
            frame(&ctx, &mut chart, 0.1, Vec::new());
            let after = chart.get_chart_mapping().price;
            let (top, bottom) = after.extent();
            let (y_hi, y_lo) = (after.price_to_y(hi), after.price_to_y(lo));
            assert!(
                (y_hi - top).abs() < 0.5 && (y_lo - bottom).abs() < 0.5,
                "{mode:?}: {lo}..{hi} at y {y_lo}..{y_hi}, pane {top}..{bottom}"
            );
        }
    }
}
