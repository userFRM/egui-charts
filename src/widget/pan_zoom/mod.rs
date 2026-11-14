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

mod box_zoom;
pub(super) mod state;

pub(super) use box_zoom::render_box_zoom;

use egui::{Event, MouseWheelUnit, Pos2, Rect, Response, Ui, Vec2, vec2};

use crate::scales::PriceScale;
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

/// What the pointer asks of one price axis in a frame, read by [`Chart::price_axis_gestures`].
#[derive(Default)]
pub(in crate::widget) struct PriceAxisGestures {
    /// The wheel and the drag: the points each moved and the height whose price stays in place.
    zooms: [Option<(f32, f32)>; 2],
    /// Whether a double-click returns the scale to automatic.
    pub(in crate::widget) reset: bool,
}

impl PriceAxisGestures {
    /// Zooms `scale`, drawn from `top` down to `bottom`, and returns whether there was a zoom.
    ///
    /// A movement of `dy` points scales the range by `exp(dy / height)`, so a gesture zooms by
    /// the same amount however many frames it spans.
    pub(in crate::widget) fn zoom(&self, scale: &mut PriceScale, top: f32, bottom: f32) -> bool {
        let mut zoomed = false;
        for (dy, anchor_y) in self.zooms.into_iter().flatten().filter(|_| bottom > top) {
            let anchor = scale
                .map(scale.transform(), top, bottom)
                .y_to_scale(anchor_y);
            scale.zoom((f64::from(dy) / f64::from(bottom - top)).exp(), anchor);
            zoomed = true;
        }
        zoomed
    }
}

impl Chart {
    /// Reads the gestures of a price axis from `response`, which may cover more than the axis:
    /// only what the pointer does within `axis` counts.
    ///
    /// A drag that began on the axis zooms around the price where it began, the wheel zooms
    /// around the pointer, and a double-click returns the scale to automatic. Each obeys its
    /// `handle_scale` permission and the group switch. Positive wheel points and an upward drag
    /// narrow the range; Shift leaves the wheel to horizontal scrolling.
    pub(in crate::widget) fn price_axis_gestures(
        &self,
        ui: &Ui,
        response: &Response,
        axis: Rect,
    ) -> PriceAxisGestures {
        let options = self.chart_options.handle_scale;
        let mut gestures = PriceAxisGestures::default();
        if !options.enabled {
            return gestures;
        }
        let on_axis = |pos: Option<Pos2>| pos.filter(|pos| axis.contains(*pos));
        let (origin, hover, shift, contacts) = ui.input(|i| {
            (
                on_axis(i.pointer.press_origin()),
                on_axis(i.pointer.hover_pos()),
                i.modifiers.shift,
                i.multi_touch().is_some(),
            )
        });
        let wheel = raw_wheel_delta(ui).y;
        if options.mouse_wheel
            && response.hovered()
            && !shift
            && wheel.is_finite()
            && wheel.abs() > 0.1
            && let Some(pos) = hover
        {
            gestures.zooms[0] = Some((-wheel, pos.y));
        }
        // Two contacts are a pinch, whichever of them began on the axis.
        if options.axis_pressed_mouse_move.price
            && response.dragged()
            && !contacts
            && let Some(origin) = origin
        {
            gestures.zooms[1] = Some((response.drag_delta().y, origin.y));
        }
        gestures.reset = options.axis_double_click_reset.price
            && response.double_clicked()
            && !self.pane_collapse_click(ui.ctx())
            && on_axis(response.interact_pointer_pos()).is_some();
        gestures
    }

    /// Whether `pos` is in a pane other than the price pane. A gesture there moves the shared
    /// time scale and leaves the prices of the price pane alone.
    fn in_other_pane(&self, pos: Pos2) -> bool {
        let main = self.main_series_pane();
        self.pane_layout
            .panes
            .iter()
            .any(|pane| pane.id != main && pane.rect.contains(pos))
    }

    /// Handle mouse wheel events for time-axis zoom and horizontal pan. The wheel over the
    /// price axis is left to [`Self::price_axis_gestures`].
    pub(crate) fn handle_mouse_wheel(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_width: f32,
        chart_rect_min_x: f32,
        price_axis_rect: Rect,
    ) {
        let scale = self.chart_options.handle_scale;
        let scroll = self.chart_options.handle_scroll;
        let scale_wheel = scale.enabled && scale.mouse_wheel;
        let scroll_wheel = scroll.enabled && scroll.mouse_wheel;

        if !response.hovered() || (!scale_wheel && !scroll_wheel) {
            return;
        }

        // Apply this frame's wheel events; smoothing can outlive the pointer's axis or modifiers.
        let raw = raw_wheel_delta(ui);
        if raw != Vec2::ZERO {
            // The wheel takes over from the momentum of an earlier flick.
            self.kinetic_scroll.is_active = false;
        }
        let scroll_input = ui.input(|i| {
            let mut dx = raw.x;
            let mut dy = raw.y;

            // Treat Shift+Wheel as horizontal scroll for standard mice
            if i.modifiers.shift && dx.abs() < 0.1 && dy.abs() > 0.1 {
                dx = dy;
                dy = 0.0;
            }
            (dy, dx, i.pointer.hover_pos())
        });
        let (delta_y, delta_x, hover_pos) = scroll_input;

        let mut did_zoom = false;
        if delta_y.is_finite() && delta_y.abs() > 0.1 {
            let shift_held = ui.input(|i| i.modifiers.shift);

            if shift_held && scroll_wheel {
                let pan_amount = -delta_y * 2.0;
                self.state.time_scale_mut().scroll_pixels(pan_amount);
            } else if scale_wheel
                && let Some(hp) = hover_pos.or_else(|| response.hover_pos())
                && !price_axis_rect.contains(hp)
            {
                // Integrate raw points multiplicatively, independently of frame batching.
                let zoom_scale = (delta_y / 1000.0).min(80.0).exp_m1() * 10.0;
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
                    self.scroll_start_offset = Some(self.state.time_scale().right_offset_precise());
                    self.scroll_start_pos = response.interact_pointer_pos();
                    self.scroll_start_spacing = None;
                }
            }
        }

        if !did_zoom && delta_y.abs() < 0.1 && delta_x.abs() > 0.1 && scroll_wheel {
            self.state.time_scale_mut().scroll_pixels(delta_x);
        }
    }

    /// Handles pinch-to-zoom for touch/trackpad gestures
    ///
    /// A contact pinch zooms around the midpoint between the fingers and pans by its movement,
    /// so the bars under the fingers stay under them; a native zoom event zooms around the
    /// pointer. Contact gestures take precedence when a backend reports both.
    pub(crate) fn handle_pinch_zoom(
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
                // The pointer follows the first finger; the gesture turns about their midpoint.
                .map(|mt| (mt.zoom_delta, mt.translation_delta, Some(mt.center_pos)))
                .or_else(|| {
                    // Wheel zoom is handled separately, including its Ctrl/Cmd variant.
                    let native_zoom: f32 = i
                        .events
                        .iter()
                        .filter_map(|event| {
                            if let Event::Zoom(delta) = event
                                && delta.is_finite()
                                && *delta > 0.0
                            {
                                Some(*delta)
                            } else {
                                None
                            }
                        })
                        .product();
                    (native_zoom != 1.0).then_some((native_zoom, Vec2::ZERO, i.pointer.hover_pos()))
                })
        });

        if let Some((zoom_delta, translation_delta, anchor)) = zoom_info {
            // Handle two-finger pan (translation during pinch) first: the zoom below then keeps
            // the bar under the midpoint where the fingers have taken it.
            if self.chart_options.handle_scroll.enabled
                && self.chart_options.handle_scroll.horz_touch_drag
                && translation_delta.x != 0.0
            {
                self.pending_scroll_time = None;
                self.state
                    .time_scale_mut()
                    .scroll_pixels(translation_delta.x);
            }

            // Handle pinch zoom
            if self.chart_options.handle_scale.enabled
                && self.chart_options.handle_scale.pinch
                && zoom_delta.is_finite()
                && zoom_delta > 0.0
                && zoom_delta != 1.0
            {
                // TimeScale::zoom applies one tenth of its delta.
                let zoom_scale = (zoom_delta - 1.0) * 10.0;
                self.pending_scroll_time = None;
                self.tracking_mode_active = false;
                self.kinetic_scroll.is_active = false;

                // Use the gesture's anchor or center of chart as zoom anchor
                let zoom_point_x = self.zoom_anchor(
                    anchor.map_or(chart_width / 2.0, |p| p.x - chart_rect_min_x),
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
                    self.scroll_start_spacing = None;
                }
            }

            // The vertical part of the two-finger pan moves a manual price range.
            if self.chart_options.handle_scroll.enabled
                && self.chart_options.handle_scroll.vert_touch_drag
                && self.last_rendered_price_rect.height() > 0.0
                && self.state.price_scale().is_manual()
                && translation_delta.y != 0.0
                && anchor.is_none_or(|pos| !self.in_other_pane(pos))
            {
                let y = anchor.map_or(0.0, |pos| pos.y);
                let map = self.last_price_map;
                let shift = map.y_to_scale(y - translation_delta.y) - map.y_to_scale(y);
                let range = self.state.price_scale().range();
                self.state
                    .price_scale_mut()
                    .set_range(range.min + shift, range.max + shift);
            }
        }
    }

    /// Handle drag-to-pan and time-axis drag-zoom.
    ///
    /// When the drag starts on a plot, horizontal movement pans the time axis
    /// and, in the price pane while auto-scale is off, vertical movement pans
    /// the price axis. A drag that starts on the price axis is left to
    /// [`Self::price_axis_gestures`]. Starting on the time axis enables
    /// drag-to-zoom the time scale. Also tracks velocity for kinetic scrolling
    /// continuation after release.
    pub(crate) fn handle_drag_pan(
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
        // The frame clock times the samples, as it times the input they come from.
        let (is_touch, multi_touch, now) = ui.input(|i| {
            (
                i.any_touches()
                    || i.events
                        .iter()
                        .any(|event| matches!(event, Event::Touch { .. })),
                i.multi_touch().is_some(),
                i.time,
            )
        });

        // Touch and mouse drags pan and continue under their own options.
        let kinetic = self.chart_options.handle_scroll.enabled
            && if is_touch {
                self.chart_options.handle_scroll.horz_touch_drag
                    && self.chart_options.kinetic_scroll.enabled
            } else {
                self.chart_options.handle_scroll.pressed_mouse_move
                    && self.chart_options.kinetic_scroll.mouse
            };

        // A press, a click or a touch on the chart stops the momentum of an earlier flick.
        if response.is_pointer_button_down_on() || response.clicked() {
            self.kinetic_scroll.is_active = false;
        }

        if !response.dragged() || multi_touch || has_active_drawing_tool || box_zoom_active {
            // Drag ended - handle kinetic scrolling
            if response.drag_stopped()
                && !has_active_drawing_tool
                && !multi_touch
                && kinetic
                && self.scroll_start_pos.is_some()
            {
                // A pointer that came to rest before the release throws nothing.
                let moving = |last: Option<f64>| last.is_some_and(|last| now - last <= 0.05);
                // Frames slower than that rest leave the release to carry the pointer's last
                // movement itself. Faster frames have measured the speed over a whole frame,
                // which a release that arrives part way through its own would understate.
                if !moving(self.kinetic_scroll.last_time)
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    self.sample_kinetic_speed(pos, now);
                }
                let move_distance = self
                    .kinetic_scroll
                    .last_pos
                    .and_then(|last| self.scroll_start_pos.map(|start| (last.x - start.x).abs()))
                    .unwrap_or(0.0);

                let min_v_px_s = self.chart_options.kinetic_scroll.min_scroll_speed * 60.0;
                let max_v_px_s = self.chart_options.kinetic_scroll.max_scroll_speed * 60.0;

                if moving(self.kinetic_scroll.last_time)
                    && move_distance >= self.chart_options.kinetic_scroll.scroll_min_move
                    && self.kinetic_scroll.velocity.abs() >= min_v_px_s
                {
                    self.kinetic_scroll.is_active = true;
                    self.kinetic_scroll.velocity =
                        self.kinetic_scroll.velocity.clamp(-max_v_px_s, max_v_px_s);
                    self.kinetic_scroll.anim_last_time = Some(now);
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
            self.scroll_start_spacing = None;
            self.kinetic_scroll.last_pos = None;
            self.kinetic_scroll.last_time = None;
            if !self.kinetic_scroll.is_active {
                // Only a coast carries the speed of a drag past its release.
                self.kinetic_scroll.velocity = 0.0;
            }
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
                // The price axis reads its own drag.
            } else if time_axis_rect.contains(origin) {
                let scale = self.chart_options.handle_scale;
                if !scale.enabled || !scale.axis_pressed_mouse_move.time {
                    // Rebase while disabled so re-enabling does not replay the blocked movement.
                    self.scroll_start_pos = Some(curr_pos);
                    self.scroll_start_offset = Some(self.state.time_scale().right_offset_precise());
                    self.scroll_start_spacing = None;
                    return;
                }
                // Zoom the view the drag began with by the whole movement since, around the bar
                // that was pressed: the spacing grows e times per 1000 points whatever frames
                // the drag spans, and coming back restores the view.
                let time_scale = self.state.time_scale_mut();
                let spacing = *self
                    .scroll_start_spacing
                    .get_or_insert(time_scale.bar_spacing());
                time_scale.set_bar_spacing(spacing);
                time_scale.set_right_offset(start_offset);
                // TimeScale::zoom applies one tenth of its delta.
                time_scale.zoom(
                    ((curr_pos.x - start_pos.x) / 1000.0).min(80.0).exp_m1() * 10.0,
                    origin.x - chart_rect_min_x,
                    chart_rect_min_x,
                    time_axis_rect.width(),
                );
            } else {
                let scroll = self.chart_options.handle_scroll;
                if !scroll.enabled {
                    // Rebase while disabled so re-enabling does not replay the blocked movement.
                    self.scroll_start_pos = Some(curr_pos);
                    self.scroll_start_offset = Some(self.state.time_scale().right_offset_precise());
                    return;
                }
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
                        self.sample_kinetic_speed(curr_pos, now);
                    }
                }

                // Manual prices follow only the movement enabled for this input device.
                if vertical
                    && self.last_rendered_price_rect.height() > 0.0
                    && self.state.price_scale().is_manual()
                    && !self.in_other_pane(origin)
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

    /// Measures the drag speed from the pointer at `pos` at frame time `now`. A resting pointer
    /// adds no sample, so the age of the last one tells a release how long ago it moved.
    fn sample_kinetic_speed(&mut self, pos: Pos2, now: f64) {
        if self.kinetic_scroll.last_pos == Some(pos) {
            return;
        }
        if let (Some(last_pos), Some(last_time)) =
            (self.kinetic_scroll.last_pos, self.kinetic_scroll.last_time)
        {
            let dt = (now - last_time) as f32;
            if dt > 0.0 {
                self.kinetic_scroll.velocity = (pos.x - last_pos.x) / dt;
            }
        }
        self.kinetic_scroll.last_pos = Some(pos);
        self.kinetic_scroll.last_time = Some(now);
    }

    /// Advance the kinetic scrolling animation by one frame.
    ///
    /// Applies the current velocity to the right-offset, then damps the velocity
    /// using the configured damping coefficient. Stops when velocity drops below
    /// the minimum threshold or motion becomes non-finite. Requests a repaint
    /// while the animation is running.
    pub(crate) fn apply_kinetic_scroll(&mut self, ui: &Ui) {
        if !self.chart_options.handle_scroll.enabled {
            self.kinetic_scroll = state::KineticScrollState::new();
            return;
        }
        let options = self.chart_options.kinetic_scroll;
        if !(options.enabled || options.mouse) || !self.kinetic_scroll.is_active {
            return;
        }
        if !self.kinetic_scroll.velocity.is_finite() {
            self.kinetic_scroll = state::KineticScrollState::new();
            return;
        }

        let now = ui.input(|i| i.time);
        let dt = self
            .kinetic_scroll
            .anim_last_time
            .map_or(0.0, |t0| (now - t0).max(0.0) as f32);
        self.kinetic_scroll.anim_last_time = Some(now);

        if dt > 0.0 {
            let velocity_in_bars_per_s =
                self.kinetic_scroll.velocity / self.state.time_scale().bar_spacing();
            let delta_offset = velocity_in_bars_per_s * dt;
            let curr_offset = self.state.time_scale().right_offset_precise();
            let new_offset = curr_offset - f64::from(delta_offset);

            if !new_offset.is_finite() {
                self.kinetic_scroll = state::KineticScrollState::new();
                return;
            }
            self.state.time_scale_mut().set_right_offset(new_offset);

            let frames = (dt * 60.0).max(0.0);
            let damping = self.chart_options.kinetic_scroll.damping_coeff.powf(frames);
            self.kinetic_scroll.velocity *= damping;

            let min_v_px_s = self.chart_options.kinetic_scroll.min_scroll_speed * 60.0;
            if self.kinetic_scroll.velocity.abs() < min_v_px_s {
                self.kinetic_scroll = state::KineticScrollState::new();
                return;
            }
        }

        ui.ctx().request_repaint();
    }

    /// Reset the time axis on double-click: it returns to the configured spacing at the latest
    /// bar. The price axis is left to [`Self::price_axis_gestures`].
    pub(crate) fn handle_double_click(&mut self, response: &Response, time_axis_rect: Rect) {
        if !self.chart_options.handle_scale.enabled || !response.double_clicked() {
            return;
        }

        if let Some(pos) = response.interact_pointer_pos()
            && self.chart_options.handle_scale.axis_double_click_reset.time
            && time_axis_rect.contains(pos)
        {
            // Spacing first: a pixel right offset is converted at the restored spacing.
            self.state
                .time_scale_mut()
                .set_bar_spacing(self.chart_options.time_scale.bar_spacing);
            self.state.time_scale_mut().jump_to_latest();
            self.pending_scroll_time = None;
        }
    }

    /// Handle box-zoom interaction (left-click drag when zoom mode is active).
    ///
    /// On press, starts tracking the selection rectangle. On release, zooms into the rectangle.
    /// Returns `true` if a zoom was successfully applied this frame.
    pub(crate) fn handle_box_zoom(
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

    /// Zoom programmatically to a screen rectangle from `start` to `end`, filling the
    /// viewport with the selected time and price ranges. [`Self::zoom_back`] undoes it.
    ///
    /// Returns `false` if either dimension of the selection is 20 points or less, or if a
    /// corner or the width is non-finite.
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
        let center = time_scale.position(center_idx);
        let old = time_scale.bar_spacing();
        time_scale.set_bar_spacing(
            (old * (chart_width - 1.0) / (max_x - min_x)).min((chart_width - 1.0).max(old)),
        );
        // The inverse of `coord_to_idx` at the middle of the plot's pixel span.
        let spacing = time_scale.bar_spacing();
        let base_idx = time_scale.base_idx() as f64;
        time_scale.set_right_offset(
            center - base_idx + (f64::from(chart_width) - 1.0) / 2.0 / f64::from(spacing) - 0.5,
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

    /// Apply pending timescale configuration: width, initial visible bars,
    /// lock-on-resize behavior, and pending start-index jumps.
    pub(crate) fn apply_timescale_config(&mut self, chart_width: f32) {
        // Hidden or collapsed layouts cannot establish a zoom or a resize baseline.
        if !chart_width.is_finite() || chart_width < 2.0 {
            return;
        }
        if self.apply_visible_bars_once {
            self.state.time_scale_mut().set_width(chart_width);
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
            && chart_width != prev_width
        {
            // Sets the new width with the rescaled spacing, so a fixed edge never clamps
            // against the new width at the old spacing.
            self.state
                .time_scale_mut()
                .keep_slots_on_resize(prev_width, chart_width);
        } else {
            self.state.time_scale_mut().set_width(chart_width);
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
            let half_window = ((chart_width - 1.0) / scale.bar_spacing() - 1.0) / 2.0;
            scale.set_right_offset(
                scale.position(index as f64) - scale.base_idx() as f64 + f64::from(half_window),
            );
            if !needs_history {
                self.pending_scroll_time = None;
            }
        }

        // Apply pending start index
        if let Some(target_start) = self.pending_start_idx.take() {
            let bar_spacing = self.state.time_scale().bar_spacing();
            if bar_spacing > 0.0 {
                let visible_len = (chart_width - 1.0) / bar_spacing;
                let base_idx = self.state.time_scale().base_idx() as f64;
                let desired_right_border = self.state.time_scale().position(target_start as f64)
                    + f64::from(visible_len)
                    - 1.0;
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

    /// Invalid damping falls back at both option entry points and the animation still ends;
    /// under the defaults a flick keeps its speed and settles in under two seconds.
    #[test]
    fn kinetic_damping_options_decay_to_a_stop() {
        for runtime in [false, true] {
            for (coefficient, expected) in [
                (f32::NAN, 0.95),
                (f32::INFINITY, 0.95),
                (f32::NEG_INFINITY, 0.95),
                (-0.5, 0.95),
                (1.0, 0.95),
                (1.5, 0.95),
                (0.0, 0.0),
                (0.5, 0.5),
                (0.997, 0.997),
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0], &[105.0]);
                let mut options = chart.chart_options().clone();
                options.kinetic_scroll.enabled = true;
                options.kinetic_scroll.damping_coeff = coefficient;
                if runtime {
                    chart.set_chart_options(options);
                } else {
                    chart = chart.with_chart_options(options);
                }
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.kinetic_scroll.is_active = true;
                chart.kinetic_scroll.velocity = 600.0;
                chart.kinetic_scroll.anim_last_time = Some(0.0);
                // At the slowest damping here, 600 * 0.997^(40 * 60) < 12 points per second.
                for i in 1..=40 {
                    frame(&ctx, &mut chart, f64::from(i), vec![]);
                    assert!(chart.state.time_scale().right_offset_precise().is_finite());
                }
                assert!(
                    !chart.kinetic_scroll.is_active,
                    "runtime={runtime}, damping={coefficient}"
                );
                assert_eq!(chart.kinetic_scroll.velocity, 0.0);
                assert_eq!(chart.chart_options().kinetic_scroll.damping_coeff, expected);
            }
        }

        // A drag of 20 points every 10 ms is released at 2000 points per second, under the
        // default limit of 3000. At 60 frames per second each frame keeps 0.95 of the speed,
        // and 2000 * 0.95^100 = 11.8 is the first below the minimum of 12 points per second.
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0], &[105.0]);
        chart.chart_options.kinetic_scroll.mouse = true;
        frame(&ctx, &mut chart, 0.0, vec![]);
        let start = chart.last_rendered_price_rect.center();
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut chart, 0.01, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut chart, 0.02, vec![button(start, true)]);
        let mut end = start;
        for i in 1..=4 {
            end.x = start.x + 20.0 * i as f32;
            let time = 0.02 + f64::from(i) * 0.01;
            frame(&ctx, &mut chart, time, vec![Event::PointerMoved(end)]);
        }
        frame(&ctx, &mut chart, 0.07, vec![button(end, false)]);
        assert!(chart.kinetic_scroll.is_active);
        let speed = chart.kinetic_scroll.velocity;
        assert!((speed - 2000.0).abs() < 1.0, "released at {speed}");
        let frames = (1..=1200).find(|&i| {
            frame(&ctx, &mut chart, 0.07 + f64::from(i) / 60.0, vec![]);
            !chart.kinetic_scroll.is_active
        });
        assert_eq!(frames, Some(100));
    }

    /// Invalid motion and a disabled scroll group stop momentum without changing the viewport.
    #[test]
    fn kinetic_invalid_or_disabled_motion_stops_before_scrolling() {
        for (velocity, elapsed, enabled) in [
            (f32::NAN, None, true),
            (f32::INFINITY, Some(0.05), true),
            (f32::NEG_INFINITY, Some(0.05), true),
            (f32::MAX, Some(0.05), true),
            (600.0, Some(1.0), false),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0], &[105.0]);
            chart.chart_options.kinetic_scroll.enabled = true;
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.state.time_scale_mut().set_bar_spacing(0.5);
            let offset = chart.state.time_scale().right_offset_precise();
            chart.kinetic_scroll.is_active = true;
            chart.kinetic_scroll.velocity = velocity;
            chart.kinetic_scroll.anim_last_time = elapsed.map(|seconds| 0.05 - seconds);
            chart.chart_options.handle_scroll.enabled = enabled;
            frame(&ctx, &mut chart, 0.05, vec![]);
            assert!(!chart.kinetic_scroll.is_active, "velocity={velocity}");
            assert_eq!(chart.kinetic_scroll.velocity, 0.0);
            assert!(chart.kinetic_scroll.anim_last_time.is_none());
            assert_eq!(chart.state.time_scale().right_offset_precise(), offset);
            chart.chart_options.handle_scroll.enabled = true;
            frame(&ctx, &mut chart, 0.1, vec![]);
            assert_eq!(chart.state.time_scale().right_offset_precise(), offset);
        }
    }

    /// Native zoom and contact pinch each scale once: a native zoom keeps the bar under the
    /// pointer, and a pinch keeps the bars under both fingers.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn native_and_contact_pinch_keep_the_zoom_anchor() {
        // Contacts are how far each finger moves while they spread from 100 to 150 points
        // apart: the first rests, both move or the second rests.
        let (first_rests, both_move, second_rests) = ([0.0, 50.0], [-25.0, 25.0], [-50.0, 0.0]);
        for (contacts, native, enabled, tracking, kinetic, maximized, group_enabled) in [
            (None, true, true, false, false, false, true),
            (Some(first_rests), false, true, false, false, false, true),
            (Some(both_move), false, true, false, false, false, true),
            (Some(second_rests), false, true, false, false, false, true),
            (Some(both_move), true, true, false, false, false, true),
            (None, true, false, false, false, false, true),
            (None, true, true, true, false, false, true),
            (None, true, true, false, true, false, true),
            (None, true, true, false, false, true, true),
            (Some(first_rests), false, true, false, false, true, true),
            (None, true, true, false, false, false, false),
            (Some(second_rests), false, true, false, false, false, false),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            chart.chart_options.handle_scale.pinch = enabled;
            chart.chart_options.handle_scale.enabled = group_enabled;
            let enabled = enabled && group_enabled;
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
            // A backend keeps the pointer on the first finger.
            if contacts.is_some() {
                frame(
                    &ctx,
                    &mut chart,
                    0.04,
                    vec![
                        Event::PointerButton {
                            pos: pointer,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                        touch(1, egui::TouchPhase::Start, pointer.x),
                        touch(2, egui::TouchPhase::Start, pointer.x + 100.0),
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
                chart.kinetic_scroll.anim_last_time = Some(0.04);
            }
            let (spacing, offset) = chart.get_time_scale_state();
            let mut events = Vec::new();
            if let Some([first, second]) = contacts {
                events.extend([
                    Event::PointerMoved(Pos2::new(pointer.x + first, pointer.y)),
                    touch(1, egui::TouchPhase::Move, pointer.x + first),
                    touch(2, egui::TouchPhase::Move, pointer.x + 100.0 + second),
                ]);
            }
            if native {
                events.push(Event::Zoom(1.5));
            }
            frame(&ctx, &mut chart, 0.06, events);
            assert!(!chart.kinetic_scroll.is_active);
            let (zoomed, moved) = chart.get_time_scale_state();
            // A pinch follows its reported ratio, whether native or derived from contacts.
            let expected_spacing = if enabled { spacing * 1.5 } else { spacing };
            assert!(
                (zoomed - expected_spacing).abs() < 1e-5,
                "contacts={contacts:?}, native={native}, enabled={enabled}: {zoomed}"
            );
            // The bar under the fingers' midpoint follows it, and the other bars spread around
            // it by the pinch ratio; a native zoom turns about the pointer, which stays.
            let (from, to) = contacts.map_or((pointer.x, pointer.x), |[first, second]| {
                (pointer.x + 50.0, pointer.x + 50.0 + (first + second) / 2.0)
            });
            let expected_offset =
                offset + (rect.right() - to - 1.0) / zoomed - (rect.right() - from - 1.0) / spacing;
            assert!(
                (moved - expected_offset).abs() < 1e-4,
                "contacts={contacts:?}, native={native}, enabled={enabled}: {moved}, expected \
                 {expected_offset}"
            );
            frame(&ctx, &mut chart, 0.08, vec![]);
            assert_eq!(chart.get_time_scale_state(), (zoomed, moved));
            if tracking {
                assert!(!chart.is_tracking_mode_active());
            }
        }
    }

    /// Wheel zoom applies the complete gesture immediately, independently of event batching.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn wheel_zoom_follows_input_independently_of_frame_batching() {
        for modifiers in [Modifiers::NONE, Modifiers::COMMAND] {
            for total in [-240.0_f32, 240.0] {
                for steps in [1, 2, 8] {
                    for together in [false, true] {
                        let case = format!(
                            "modifiers={modifiers:?}, total={total}, steps={steps}, together={together}"
                        );
                        let ctx = egui::Context::default();
                        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                        frame(&ctx, &mut chart, 0.0, vec![]);
                        let rect = chart.last_rendered_price_rect;
                        let pointer = Pos2::new(rect.left() + rect.width() * 0.25, rect.center().y);
                        frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pointer)]);
                        let (spacing, offset) = chart.get_time_scale_state();
                        let wheel = || Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, total / steps as f32),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Wheel,
                            modifiers,
                        };
                        if together {
                            frame(
                                &ctx,
                                &mut chart,
                                0.04,
                                (0..steps).map(|_| wheel()).collect(),
                            );
                        } else {
                            for step in 0..steps {
                                frame(&ctx, &mut chart, 0.04 + step as f64 * 0.02, vec![wheel()]);
                            }
                        }
                        let (zoomed, moved) = chart.get_time_scale_state();
                        let expected = spacing * (total / 1000.0).exp();
                        assert!(
                            (zoomed - expected).abs() < 1e-5,
                            "{case}: spacing={zoomed}, expected={expected}"
                        );
                        let expected_offset = offset
                            + (rect.right() - pointer.x - 1.0) * (1.0 / zoomed - 1.0 / spacing);
                        assert!(
                            (moved - expected_offset).abs() < 1e-4,
                            "{case}: offset={moved}, expected={expected_offset}"
                        );
                        for step in 0..20 {
                            frame(&ctx, &mut chart, 1.0 + step as f64 * 0.1, vec![]);
                            assert_eq!(
                                chart.get_time_scale_state(),
                                (zoomed, moved),
                                "{case}, idle frame={step}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Applied wheel zoom cannot replay as pan or price zoom after the pointer or modifiers move.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn wheel_zoom_does_not_replay_on_other_axes() {
        use egui::MouseWheelSource::{Momentum, Wheel};
        for (dx, destination, fresh, source) in [
            (30.0, "plot", false, Wheel),
            (0.0, "price", false, Wheel),
            (0.0, "shift", false, Wheel),
            (0.0, "price", true, Wheel),
            (0.0, "shift", true, Wheel),
            (0.0, "price", true, Momentum),
            (0.0, "shift", true, Momentum),
        ] {
            let case =
                format!("source={source:?}, dx={dx}, destination={destination}, fresh={fresh}");
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            chart.state.price_scale_mut().set_range(90.0, 110.0);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let rect = chart.last_rendered_price_rect;
            let pointer = Pos2::new(rect.left() + rect.width() * 0.25, rect.center().y);
            frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pointer)]);
            let wheel = |x, y, modifiers| Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(x, y),
                phase: egui::TouchPhase::Move,
                source,
                modifiers,
            };
            frame(
                &ctx,
                &mut chart,
                0.04,
                vec![wheel(dx, -240.0, Modifiers::NONE)],
            );
            let before = chart.get_time_scale_state();
            let range = chart.state.price_scale().range();
            let price = destination == "price";
            let shift = destination == "shift";
            let modifiers = if shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            let pos = if price {
                Pos2::new(rect.right() + 10.0, pointer.y)
            } else {
                pointer
            };
            let mut events = vec![Event::PointerMoved(pos), Event::ModifiersChanged(modifiers)];
            if fresh {
                events.push(wheel(0.0, -30.0, modifiers));
            }
            frame(&ctx, &mut chart, 0.06, events);
            let after = chart.get_time_scale_state();
            let expected_offset = before.1 + if fresh && shift { 30.0 / before.0 } else { 0.0 };
            let expected_span = (range.max - range.min)
                * if fresh && price {
                    (30.0 / f64::from(rect.height())).exp()
                } else {
                    1.0
                };
            let shown = chart.state.price_scale().range();
            assert!(
                (after.0 - before.0).abs() < 1e-5,
                "{case}: spacing={} -> {}",
                before.0,
                after.0
            );
            assert!(
                (after.1 - expected_offset).abs() < 1e-4,
                "{case}: offset={}, expected={expected_offset}",
                after.1
            );
            assert!(
                (shown.max - shown.min - expected_span).abs() < 1e-8,
                "{case}: price span={}, expected={expected_span}",
                shown.max - shown.min
            );
            for step in 0..20 {
                frame(&ctx, &mut chart, 0.08 + step as f64 * 0.02, vec![]);
                assert_eq!(
                    chart.get_time_scale_state(),
                    after,
                    "{case}, idle frame={step}"
                );
                assert_eq!(
                    chart.state.price_scale().range(),
                    shown,
                    "{case}, idle frame={step}"
                );
            }
        }
    }

    /// Reciprocal pinches cancel; invalid scale ratios leave the viewport untouched.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn native_pinch_ratios_compose_and_reject_invalid_values() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let rect = chart.last_rendered_price_rect;
        frame(
            &ctx,
            &mut chart,
            0.02,
            vec![Event::PointerMoved(rect.center())],
        );
        let before = chart.get_time_scale_state();
        frame(&ctx, &mut chart, 0.04, vec![Event::Zoom(1.25)]);
        frame(&ctx, &mut chart, 0.06, vec![Event::Zoom(0.8)]);
        let after = chart.get_time_scale_state();
        assert!((after.0 - before.0).abs() < 1e-5);
        assert!((after.1 - before.1).abs() < 1e-4);
        for (i, ratio) in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY]
            .into_iter()
            .enumerate()
        {
            frame(
                &ctx,
                &mut chart,
                0.1 + i as f64 * 0.02,
                vec![Event::Zoom(ratio)],
            );
            assert_eq!(chart.get_time_scale_state(), after, "ratio={ratio}");
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn extreme_finite_wheel_zoom_reaches_spacing_limits() {
        for (delta, expected) in [(86_500.0, 50.0), (f32::MAX, 50.0), (-f32::MAX, 0.5)] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0], &[105.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.state.time_scale_mut().set_min_bar_spacing(0.5);
            chart.state.time_scale_mut().set_max_bar_spacing(50.0);
            let pointer = chart.last_rendered_price_rect.center();
            frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pointer)]);
            frame(
                &ctx,
                &mut chart,
                0.04,
                vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, delta),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert_eq!(
                chart.state.time_scale().bar_spacing(),
                expected,
                "delta={delta}"
            );
            assert!(
                chart.state.time_scale().right_offset_precise().is_finite(),
                "delta={delta}"
            );
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
                let spacing = (width - 1.0) / 100.0;
                assert!((scale.bar_spacing() - spacing).abs() < 1e-5);
                let offset = pixels.map_or(2.5, |px| px / spacing);
                assert!((scale.right_offset() - offset).abs() < 1e-5);
                chart.state.time_scale_mut().set_right_offset(-20.0);
                show(&mut chart, 0.0);
                show(&mut chart, 1.0);
                show(&mut chart, 1064.0);
                assert!((chart.state.time_scale().bar_spacing() - spacing).abs() < 1e-5);
                assert_eq!(chart.state.time_scale().right_offset(), -20.0);
                // A locked resize keeps the `width - 1` pixels of slots the plot spans, also when
                // the window grows by less than a pixel per frame.
                for step in 0..=20 {
                    show(&mut chart, 1564.0 + 0.5 * step as f32);
                    let expected = if lock {
                        (chart.last_rendered_price_rect.width() - 1.0) / 100.0
                    } else {
                        spacing
                    };
                    let actual = chart.state.time_scale().bar_spacing();
                    assert!((actual - expected).abs() < 1e-5, "step={step}");
                    assert_eq!(chart.state.time_scale().right_offset(), -20.0);
                }
                chart.set_visible_bars(80);
                show(&mut chart, 1264.0);
                assert!(
                    (chart.state.time_scale().bar_spacing()
                        - (chart.last_rendered_price_rect.width() - 1.0) / 80.0)
                        .abs()
                        < 1e-5,
                    "a pending view must take precedence over a locked resize"
                );
            }
        }
    }

    /// A locked resize at the fixed left edge's stop keeps the first bar at the left edge.
    #[test]
    fn locked_resize_at_the_left_edge_stop_keeps_the_first_bar() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0], &[105.0]).visible_bars(50);
        let options = &mut chart.chart_options.time_scale;
        options.lock_visible_time_range_on_resize = true;
        options.fix_left_edge = true;
        let options = chart.chart_options.clone();
        chart = chart.with_chart_options(options);
        let show = |chart: &mut Chart, width| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            )
            .drop_without_applying_deltas();
        };
        show(&mut chart, 1064.0);
        chart.scroll_to_position(-1000.0, false);
        for width in [1064.0, 1364.0, 1364.5, 864.0] {
            show(&mut chart, width);
            let scale = chart.state.time_scale();
            let left = scale.visible_logical_range().left;
            assert!(left.abs() < 1e-4, "width={width}: left={left}");
            let spacing = (chart.last_rendered_price_rect.width() - 1.0) / 50.0;
            assert!(
                (scale.bar_spacing() - spacing).abs() < 1e-4,
                "width={width}"
            );
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

    /// Positive wheel input zooms in on the time scale or on the price axis under the pointer,
    /// retaining its anchor and permissions and leaving every other scale alone.
    /// Ctrl/Cmd wheel and point-based trackpad input follow the same direction.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn wheel_zooms_both_axes_with_and_without_the_zoom_modifier() {
        for (theme, width) in [
            (crate::theme::Theme::light(), 800.0),
            (crate::theme::Theme::dark(), 800.0),
            (crate::theme::Theme::light(), 360.0),
            (crate::theme::Theme::dark(), 360.0),
        ] {
            let ctx = egui::Context::default();
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
            for (modifiers, right_bar_stays, fraction, group_enabled, wheel_enabled) in [
                (Modifiers::NONE, false, 0.25, true, true),
                (Modifiers::NONE, false, 0.25, false, true),
                (Modifiers::NONE, false, 0.25, true, false),
                (Modifiers::CTRL, false, 0.75, true, true),
                (Modifiers::COMMAND, false, 0.75, true, true),
                (Modifiers::COMMAND, false, 0.75, false, true),
                (Modifiers::NONE, true, 0.25, true, true),
            ] {
                // The plot, the main price axis, a study pane's axis and an added series' axis.
                for target in ["plot", "price", "pane", "series"] {
                    let price = target != "plot";
                    let enabled = group_enabled && wheel_enabled;
                    let key = crate::model::SeriesScale::Separate(1);
                    for delta in [-1.0_f32, 1.0] {
                        for (unit, source) in [
                            (egui::MouseWheelUnit::Line, egui::MouseWheelSource::Wheel),
                            (
                                egui::MouseWheelUnit::Point,
                                egui::MouseWheelSource::Momentum,
                            ),
                        ] {
                            let case = format!(
                                "{modifiers:?}, target={target}, delta={delta}, unit={unit:?}, enabled={enabled}, width={width}"
                            );
                            let time = ctx.input(|input| input.time) + 1.0;
                            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                            match target {
                                "pane" => {
                                    chart.add_indicator(Box::new(
                                        crate::studies::CustomIndicator::new("Pane", |bars| {
                                            vec![
                                                crate::studies::IndicatorValue::Single(50.0);
                                                bars.len()
                                            ]
                                        })
                                        .with_overlay(false),
                                    ));
                                }
                                "series" => {
                                    let series = chart.add_series(crate::widget::SeriesOptions {
                                        scale: key,
                                        ..Default::default()
                                    });
                                    chart.set_data(series, chart.state.data().bars.clone());
                                }
                                _ => {}
                            }
                            chart.set_theme(&ctx, &theme);
                            chart.chart_options.handle_scale.enabled = group_enabled;
                            chart.chart_options.handle_scale.mouse_wheel = wheel_enabled;
                            chart.chart_options.handle_scroll.enabled = !group_enabled;
                            chart.chart_options.time_scale.right_bar_stays_on_scroll =
                                right_bar_stays;
                            chart.state.price_scale_mut().set_range(90.0, 110.0);
                            frame(&ctx, &mut chart, time, Vec::new());
                            // The pointed scale shows a manual range 20 wide from `low`.
                            let low = match target {
                                "pane" => {
                                    chart.pane_layout.panes[1].scale.set_range(40.0, 60.0);
                                    40.0
                                }
                                "series" => {
                                    let pane = chart.main_series_pane();
                                    chart.pane_price_scale_mut(pane, key).set_range(90.0, 110.0);
                                    90.0
                                }
                                _ => 90.0,
                            };
                            let pointed = |chart: &Chart| match target {
                                "pane" => chart.pane_layout.panes[1].scale.range(),
                                "series" => chart.series_scales[&key].range(),
                                _ => chart.state.price_scale().range(),
                            };
                            let (spacing, offset) = chart.get_time_scale_state();
                            let rect = chart.last_rendered_price_rect;
                            // The height the pointed scale is drawn over.
                            let axis = if target == "pane" {
                                chart.pane_layout.panes[1].rect
                            } else {
                                rect
                            };
                            let pointer = Pos2::new(
                                match target {
                                    "plot" => rect.left() + rect.width() * fraction,
                                    "series" => rect.left() - 10.0,
                                    _ => rect.right() + 10.0,
                                },
                                axis.top() + axis.height() * fraction,
                            );
                            frame(
                                &ctx,
                                &mut chart,
                                time + 0.02,
                                vec![Event::PointerMoved(pointer)],
                            );
                            let anchor_price = low + 20.0
                                - 20.0 * f64::from((pointer.y - axis.top()) / axis.height());
                            let wheel = Event::MouseWheel {
                                unit,
                                delta: egui::vec2(0.0, delta),
                                phase: egui::TouchPhase::Move,
                                source,
                                modifiers,
                            };
                            frame(&ctx, &mut chart, time + 0.04, vec![wheel]);
                            for i in 3..10 {
                                frame(&ctx, &mut chart, time + i as f64 * 0.02, Vec::new());
                            }
                            let (zoomed, moved) = chart.get_time_scale_state();
                            let range = pointed(&chart);
                            let main = chart.state.price_scale().range();
                            if price && enabled {
                                let span = range.max - range.min;
                                assert_eq!(span < 20.0, delta > 0.0, "{case}: {range:?}");
                                assert!((span - 20.0).abs() > 1e-6, "{case}");
                                // Affine range scaling keeps the pointed value at its original fraction.
                                let anchored = range.max
                                    - span * f64::from((pointer.y - axis.top()) / axis.height());
                                assert!(
                                    (anchored - anchor_price).abs() < 1e-5,
                                    "{case}: anchor {anchored}, expected {anchor_price}"
                                );
                                assert_eq!((zoomed, moved), (spacing, offset), "{case}");
                                if target != "price" {
                                    assert_eq!((main.min, main.max), (90.0, 110.0), "{case}");
                                }
                            } else {
                                assert_eq!((range.min, range.max), (low, low + 20.0), "{case}");
                                assert_eq!((main.min, main.max), (90.0, 110.0), "{case}");
                                if enabled {
                                    assert_eq!(
                                        zoomed > spacing,
                                        delta > 0.0,
                                        "{case}: spacing {spacing} -> {zoomed}"
                                    );
                                    assert_ne!(zoomed, spacing, "{case}");
                                } else {
                                    assert_eq!((zoomed, moved), (spacing, offset), "{case}");
                                }
                                assert_eq!(moved == offset, right_bar_stays || !enabled, "{case}");
                                let anchor = if right_bar_stays {
                                    rect.right() - 1.0
                                } else {
                                    pointer.x
                                };
                                let expected = offset
                                    + (rect.right() - anchor - 1.0)
                                        * (1.0 / zoomed - 1.0 / spacing);
                                assert!((moved - expected).abs() < 1e-4, "{case}: anchor moved");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Wheel pan and time/price zoom follow only their group, including after re-enabling it.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn wheel_groups_toggle_without_losing_individual_permissions() {
        for (scroll_wheel, scale_wheel) in [(true, true), (false, true), (true, false)] {
            for (horizontal, shift, price) in [
                (true, false, false),
                (false, true, false),
                (false, false, false),
                (false, false, true),
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                let mut options = chart.chart_options().clone();
                options.handle_scroll.mouse_wheel = scroll_wheel;
                options.handle_scroll.vert_touch_drag = false;
                options.handle_scale.mouse_wheel = scale_wheel;
                options.handle_scale.pinch = false;
                chart.set_chart_options(options.clone());
                chart.state.price_scale_mut().set_range(90.0, 110.0);
                frame(&ctx, &mut chart, 0.0, vec![]);
                let rect = chart.last_rendered_price_rect;
                let pos = if price {
                    Pos2::new(rect.right() + 10.0, rect.center().y)
                } else {
                    rect.center()
                };
                let mut t = 0.0;
                for (scroll, scale) in [(false, false), (true, false), (false, true), (true, true)]
                {
                    options.handle_scroll.enabled = scroll;
                    options.handle_scale.enabled = scale;
                    chart.set_chart_options(options.clone());
                    t += 1.0;
                    frame(&ctx, &mut chart, t, vec![Event::PointerMoved(pos)]);
                    let before = chart.get_time_scale_state();
                    let range = chart.state.price_scale().range();
                    let modifiers = if shift {
                        Modifiers::SHIFT
                    } else {
                        Modifiers::NONE
                    };
                    t += 0.02;
                    frame(
                        &ctx,
                        &mut chart,
                        t,
                        vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: if horizontal {
                                egui::vec2(30.0, 0.0)
                            } else {
                                egui::vec2(0.0, -30.0)
                            },
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Wheel,
                            modifiers,
                        }],
                    );
                    for _ in 0..20 {
                        t += 0.1;
                        frame(&ctx, &mut chart, t, vec![]);
                        if !ctx.input(|input| input.is_scrolling()) {
                            break;
                        }
                    }
                    assert!(
                        !ctx.input(|input| input.is_scrolling()),
                        "wheel smoothing must finish before toggling groups"
                    );
                    let after = chart.get_time_scale_state();
                    let pan = horizontal || shift;
                    assert_eq!(after.0 != before.0, !pan && !price && scale && scale_wheel);
                    if pan {
                        assert_eq!(after.1 != before.1, scroll && scroll_wheel);
                    }
                    assert_eq!(
                        chart.state.price_scale().range() != range,
                        !pan && price && scale && scale_wheel
                    );
                    assert_eq!(chart.chart_options().handle_scroll, options.handle_scroll);
                    assert_eq!(chart.chart_options().handle_scale, options.handle_scale);
                }
            }
        }
    }

    /// Axis resets follow both permissions; the time reset restores spacing and a pixel offset.
    #[test]
    fn axis_resets_respect_permissions_and_configured_right_offset() {
        let key = crate::model::SeriesScale::Separate(1);
        // The time axis, the main price axis, a study pane's axis and an added series' axis.
        for target in ["time", "price", "pane", "series"] {
            for individual in [false, true] {
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
                if target == "pane" {
                    chart.add_indicator(Box::new(
                        crate::studies::CustomIndicator::new("Pane", |bars| {
                            vec![crate::studies::IndicatorValue::Single(50.0); bars.len()]
                        })
                        .with_overlay(false),
                    ));
                }
                if target == "series" {
                    let series = chart.add_series(crate::widget::SeriesOptions {
                        scale: key,
                        ..Default::default()
                    });
                    chart.set_data(series, chart.state.data().bars.clone());
                }
                chart.chart_options.handle_scroll.enabled = false;
                chart
                    .chart_options
                    .handle_scale
                    .axis_double_click_reset
                    .time = individual;
                chart
                    .chart_options
                    .handle_scale
                    .axis_double_click_reset
                    .price = individual;
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.state.time_scale_mut().set_bar_spacing(12.0);
                chart.state.time_scale_mut().set_right_offset(-10.0);
                chart.state.price_scale_mut().set_range(90.0, 110.0);
                let rect = chart.last_rendered_price_rect;
                let pos = match target {
                    "time" => Pos2::new(
                        rect.center().x,
                        rect.bottom().max(chart.last_rendered_volume_rect.bottom()) + 5.0,
                    ),
                    "price" => Pos2::new(rect.right() + 10.0, rect.center().y),
                    "series" => {
                        let pane = chart.main_series_pane();
                        chart.pane_price_scale_mut(pane, key).set_range(90.0, 110.0);
                        Pos2::new(rect.left() - 10.0, rect.center().y)
                    }
                    _ => {
                        chart.pane_layout.panes[1].scale.set_range(40.0, 60.0);
                        let pane = &chart.get_rendered_indicator_panes()[0];
                        Pos2::new(pane.chart_rect.right() + 10.0, pane.panel_rect.center().y)
                    }
                };
                let mut t = 0.0;
                for enabled in [false, true] {
                    chart.chart_options.handle_scale.enabled = enabled;
                    // Separate the two double-clicks so egui does not count a triple-click.
                    t += 1.0;
                    frame(&ctx, &mut chart, t, vec![Event::PointerMoved(pos)]);
                    for pressed in [true, false, true, false] {
                        t += 0.02;
                        frame(
                            &ctx,
                            &mut chart,
                            t,
                            vec![Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                    }
                    let reset = enabled && individual;
                    match target {
                        "time" => {
                            assert_eq!(
                                chart.state.time_scale().bar_spacing(),
                                if reset { 6.0 } else { 12.0 }
                            );
                            assert_eq!(
                                chart.state.time_scale().right_offset(),
                                if reset { 10.0 } else { -10.0 }
                            );
                        }
                        "price" => assert_eq!(chart.state.price_scale().is_manual(), !reset),
                        "series" => assert_eq!(chart.series_scales[&key].is_manual(), !reset),
                        _ => assert_eq!(chart.pane_layout.panes[1].scale.is_manual(), !reset),
                    }
                    let reset = chart.chart_options.handle_scale.axis_double_click_reset;
                    assert_eq!(reset.time, individual);
                    assert_eq!(reset.price, individual);
                }
            }
        }
    }

    /// Study-pane and secondary price-axis drags obey the same scale group as the main axis.
    #[test]
    fn scale_group_controls_other_price_axis_drags() {
        for pane in [false, true] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            let key = crate::model::SeriesScale::Separate(1);
            if pane {
                chart.add_indicator(Box::new(
                    crate::studies::CustomIndicator::new("Pane", |bars| {
                        vec![crate::studies::IndicatorValue::Single(50.0); bars.len()]
                    })
                    .with_overlay(false),
                ));
            } else {
                let series = chart.add_series(crate::widget::SeriesOptions {
                    scale: key,
                    ..Default::default()
                });
                chart.set_data(series, chart.state.data().bars.clone());
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let rect = if pane {
                chart.get_rendered_indicator_panes()[0].chart_rect
            } else {
                chart.last_rendered_price_rect
            };
            let start = Pos2::new(
                if pane {
                    rect.right() + 10.0
                } else {
                    rect.left() - 10.0
                },
                rect.center().y,
            );
            let mut t = 0.0;
            for enabled in [false, true] {
                chart.chart_options.handle_scale.enabled = enabled;
                let range = |chart: &Chart| {
                    if pane {
                        chart.pane_layout.panes[1].scale.range()
                    } else {
                        chart.series_scales[&key].range()
                    }
                };
                let before = range(&chart);
                let time_scale = chart.get_time_scale_state();
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                for events in [
                    vec![Event::PointerMoved(start)],
                    vec![button(start, true)],
                    vec![Event::PointerMoved(start + egui::vec2(30.0, 30.0))],
                    vec![button(start + egui::vec2(30.0, 30.0), false)],
                ] {
                    t += 0.02;
                    frame(&ctx, &mut chart, t, events);
                }
                assert_eq!(chart.get_time_scale_state(), time_scale);
                let after = range(&chart);
                assert_eq!(
                    after.max - after.min > before.max - before.min,
                    enabled,
                    "pane={pane}, enabled={enabled}"
                );
            }
        }
    }

    /// Re-enabling during a drag applies only the movement since the last disabled frame.
    #[test]
    fn reenabled_drag_does_not_replay_disabled_movement() {
        for time_axis in [false, true] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let rect = chart.last_rendered_price_rect;
            let start = if time_axis {
                Pos2::new(
                    rect.center().x,
                    rect.bottom().max(chart.last_rendered_volume_rect.bottom()) + 5.0,
                )
            } else {
                rect.center()
            };
            let button = Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            };
            frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(start)]);
            frame(&ctx, &mut chart, 0.04, vec![button]);
            let initial = chart.get_time_scale_state();
            chart.chart_options.handle_scroll.enabled = false;
            chart.chart_options.handle_scale.enabled = false;
            frame(
                &ctx,
                &mut chart,
                0.06,
                vec![Event::PointerMoved(start + egui::vec2(100.0, 0.0))],
            );
            let before = chart.get_time_scale_state();
            assert_eq!(before, initial);
            chart.chart_options.handle_scroll.enabled = true;
            chart.chart_options.handle_scale.enabled = true;
            frame(
                &ctx,
                &mut chart,
                0.08,
                vec![Event::PointerMoved(start + egui::vec2(110.0, 0.0))],
            );
            let after = chart.get_time_scale_state();
            if time_axis {
                // Ten points on the time axis scale the spacing by e^0.01 = 1.010050.
                assert!((after.0 - before.0 * 1.010_05).abs() < 1e-5);
            } else {
                assert_eq!(after.0, before.0);
                assert!((after.1 - (before.1 - 10.0 / before.0)).abs() < 1e-5);
            }
        }
    }

    /// Zoom speed must not depend on how many frames a drag spans, and the price that was
    /// grabbed stays at its height.
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
        // The axis of an added series showing 90..110, grabbed a quarter of the way down at 105:
        // 100 points down widen the range by e^(100 / height), a quarter of it above 105.
        let key = crate::model::SeriesScale::Separate(1);
        for steps in [1, 10] {
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            let series = chart.add_series(crate::widget::SeriesOptions {
                scale: key,
                ..Default::default()
            });
            chart.set_data(series, chart.state.data().bars.clone());
            let pane = chart.main_series_pane();
            chart.pane_price_scale_mut(pane, key).set_range(90.0, 110.0);
            drag_down(
                &mut chart,
                |rect| Pos2::new(rect.left() - 10.0, rect.top() + rect.height() * 0.25),
                steps,
                false,
            );
            let range = chart.series_scales[&key].range();
            let span = 20.0 * (100.0 / f64::from(chart.last_rendered_price_rect.height())).exp();
            assert!(
                (range.max - range.min - span).abs() < 1e-4
                    && (range.max - 105.0 - span * 0.25).abs() < 1e-4,
                "{steps} frames: {range:?}, expected a span of {span}"
            );
        }
    }

    /// A drag on the time axis scales the bar spacing by e per 1000 points of its whole movement
    /// around the bar that was pressed, however many frames it spans, and returning restores the view.
    #[test]
    fn time_axis_drag_zoom_is_frame_independent() {
        // 300 points to the right in one frame and in ten, then out and back.
        let mut views = Vec::new();
        for (steps, back) in [(1, false), (10, false), (10, true)] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let rect = chart.last_rendered_price_rect;
            let start = Pos2::new(
                rect.left() + rect.width() * 0.25,
                rect.bottom().max(chart.last_rendered_volume_rect.bottom()) + 5.0,
            );
            let (spacing, offset) = chart.get_time_scale_state();
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let out = (1..=steps).map(|step| 300.0 * step as f32 / steps as f32);
            let path: Vec<_> = if back {
                out.clone().chain(out.rev().skip(1)).chain([0.0]).collect()
            } else {
                out.collect()
            };
            let mut events = vec![vec![Event::PointerMoved(start)], vec![button(start, true)]];
            events.extend(
                path.iter()
                    .map(|dx| vec![Event::PointerMoved(start + egui::vec2(*dx, 0.0))]),
            );
            let end = start + egui::vec2(*path.last().unwrap(), 0.0);
            events.push(vec![button(end, false)]);
            for (at, events) in events.into_iter().enumerate() {
                frame(&ctx, &mut chart, 0.02 * (at + 1) as f64, events);
            }
            let (zoomed, moved) = chart.get_time_scale_state();
            if back {
                assert_eq!((zoomed, moved), (spacing, offset));
            } else {
                // e^0.3 = 1.349859.
                assert!(
                    (zoomed - spacing * 1.349_859).abs() < 1e-5,
                    "{steps} frames: spacing {spacing} -> {zoomed}"
                );
                let expected =
                    offset + (rect.right() - start.x - 1.0) * (1.0 / zoomed - 1.0 / spacing);
                assert!(
                    (moved - expected).abs() < 1e-4,
                    "{steps} frames: offset {moved}, expected {expected}"
                );
                views.push((zoomed, moved));
            }
        }
        assert_eq!(views[0], views[1], "1 frame against 10");
    }

    /// With auto-scale off, the prices follow a drag on the chart: 100 points down moves the
    /// 20-wide range up by 20 * 100 / height, or down by as much on an inverted scale.
    #[test]
    fn chart_drag_pans_a_manual_price_range() {
        for (invert_scale, sign) in [(false, 1.0), (true, -1.0)] {
            for (touch, mouse, vertical, group_enabled) in [
                (false, true, false, true),
                (false, false, true, true),
                (true, false, true, true),
                (true, true, false, true),
                (false, true, true, false),
                (true, true, true, false),
            ] {
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                chart.apply_series_settings(&SeriesSettings {
                    invert_scale,
                    ..SeriesSettings::default()
                });
                chart.chart_options.handle_scroll.pressed_mouse_move = mouse;
                chart.chart_options.handle_scroll.enabled = group_enabled;
                chart.chart_options.handle_scale.enabled = false;
                chart.chart_options.handle_scroll.vert_touch_drag = vertical;
                chart.chart_options.handle_scroll.horz_touch_drag = false;
                chart.chart_options.kinetic_scroll.enabled = false;
                chart.state.price_scale_mut().set_range(90.0, 110.0);
                drag_down(&mut chart, |rect| rect.center(), 10, touch);
                let enabled = group_enabled && if touch { vertical } else { mouse };
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
            for (touch, mouse, horizontal, group_enabled) in [
                (false, true, false, true),
                (false, false, true, true),
                (true, false, true, true),
                (true, true, false, true),
                (false, true, true, false),
                (true, true, true, false),
            ] {
                for distance in [-60.0, 60.0] {
                    let ctx = egui::Context::default();
                    let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    chart.chart_options.handle_scroll.pressed_mouse_move = mouse;
                    chart.chart_options.handle_scroll.enabled = group_enabled;
                    chart.chart_options.handle_scale.enabled = false;
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
                    let enabled = group_enabled && if touch { horizontal } else { mouse };
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

    /// A fast horizontal drag continues after release, at its speed, only when inertia is
    /// enabled for its device and the pointer was still moving, and a new drag stops it.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn released_drag_continues_under_its_device_inertia() {
        // The seconds from the last movement frame to the release, whether frames are drawn
        // meanwhile, and how far the pointer has moved on when the release arrives.
        for (touch, rest, frames, slide) in [
            (false, 0.02, false, 0.0),
            (true, 0.02, false, 0.0),
            (false, 0.04, true, 0.0),
            (true, 0.04, true, 0.0),
            (false, 0.5, false, 0.0),
            (true, 0.5, false, 0.0),
            (false, 0.5, true, 0.0),
            (true, 0.5, true, 0.0),
            (false, 0.1, false, 15.0),
            (true, 0.1, false, 15.0),
            (true, 0.02, false, 2.0),
        ] {
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
                let released = 0.12 + rest;
                if frames {
                    for time in (1..).map(|i| 0.12 + f64::from(i) * 0.02) {
                        if time > released - 0.01 {
                            break;
                        }
                        frame(&ctx, &mut chart, time, vec![]);
                    }
                }
                end.x -= slide;
                let release = input(button(end, false), egui::TouchPhase::End, end);
                frame(&ctx, &mut chart, released, release);
                let inertia = if touch { touch_inertia } else { mouse_inertia };
                // Only a rest of more than 50 ms ends the motion, whether or not frames were
                // drawn, and a release that brings movement with it is no rest, however slow
                // the frames.
                assert_eq!(
                    chart.kinetic_scroll.is_active,
                    inertia && (rest <= 0.05 || slide > 0.0),
                    "touch={touch}, rest={rest}, frames={frames}, slide={slide}, touch \
                     inertia={touch_inertia}, mouse inertia={mouse_inertia}"
                );
                // The drag moved 15 points every 20 ms. A release within 50 ms continues at that
                // speed, whatever part of its own frame's movement it brings, and a later one
                // at the speed of the movement it brings.
                let speed = if rest <= 0.05 {
                    -750.0
                } else {
                    -slide / rest as f32
                };
                assert!(
                    !chart.kinetic_scroll.is_active
                        || (chart.kinetic_scroll.velocity - speed).abs() < 1.0,
                    "touch={touch}, rest={rest}, slide={slide}: {}",
                    chart.kinetic_scroll.velocity
                );

                let press = input(button(end, true), egui::TouchPhase::Start, end);
                frame(&ctx, &mut chart, released + 0.02, press);
                end.x += 20.0;
                let events = input(Event::PointerMoved(end), egui::TouchPhase::Move, end);
                frame(&ctx, &mut chart, released + 0.04, events);
                assert!(!chart.kinetic_scroll.is_active);
            }
        }
    }

    /// A press, a click, a touch, the wheel and the pan and zoom keys each end a coast in the
    /// frame they arrive, before it moves the view again.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn press_wheel_and_keys_stop_a_coast() {
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let wheel = |delta| Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        };
        let key = |key, modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        for (input, stops) in [
            ("none", false),
            ("press", true),
            ("click", true),
            ("touch", true),
            ("wheel zoom", true),
            ("wheel scroll", true),
            ("pan key", true),
            ("zoom key", true),
        ] {
            // The same input reaches a coasting chart and one at rest.
            let [(coasting, moved), (_, rest)] = [true, false].map(|coast| {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                chart.chart_options.kinetic_scroll.enabled = true;
                frame(&ctx, &mut chart, 0.0, vec![]);
                let pos = chart.last_rendered_price_rect.center();
                frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pos)]);
                if coast {
                    chart.kinetic_scroll.is_active = true;
                    chart.kinetic_scroll.velocity = 600.0;
                    chart.kinetic_scroll.anim_last_time = Some(0.02);
                }
                let events = match input {
                    "none" => vec![],
                    "press" => vec![button(pos, true)],
                    "click" => vec![button(pos, true), button(pos, false)],
                    "touch" => vec![
                        button(pos, true),
                        Event::Touch {
                            device_id: egui::TouchDeviceId(1),
                            id: egui::TouchId(1),
                            phase: egui::TouchPhase::Start,
                            pos,
                            force: None,
                        },
                    ],
                    "wheel zoom" => vec![wheel(egui::vec2(0.0, -240.0))],
                    "wheel scroll" => vec![wheel(egui::vec2(120.0, 0.0))],
                    "pan key" => vec![key(egui::Key::ArrowLeft, Modifiers::NONE)],
                    _ => vec![key(egui::Key::ArrowUp, Modifiers::COMMAND)],
                };
                frame(&ctx, &mut chart, 0.04, events);
                (chart.kinetic_scroll.is_active, chart.get_time_scale_state())
            });
            assert_eq!(coasting, !stops, "{input}");
            assert_eq!(moved == rest, stops, "{input}: {moved:?}, at rest {rest:?}");
        }
    }

    /// Two contacts translate each enabled axis once, independently of pinch scaling.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn two_finger_translation_respects_touch_permissions() {
        for (pinch, group_enabled) in [(false, true), (true, true), (true, false)] {
            for (horizontal, vertical) in
                [(false, false), (false, true), (true, false), (true, true)]
            {
                let ctx = egui::Context::default();
                let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                chart.chart_options.handle_scroll.horz_touch_drag = horizontal;
                chart.chart_options.handle_scroll.vert_touch_drag = vertical;
                chart.chart_options.handle_scale.pinch = pinch;
                chart.chart_options.handle_scale.enabled = false;
                chart.chart_options.handle_scroll.enabled = group_enabled;
                let horizontal = horizontal && group_enabled;
                let vertical = vertical && group_enabled;
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
                // The second step is under half a point: the fingers are followed however
                // slowly they move.
                for (step, moved) in [(1, 30.0), (2, 30.4)] {
                    let pos = start + egui::vec2(moved, moved);
                    frame(
                        &ctx,
                        &mut chart,
                        0.02 + f64::from(step) * 0.02,
                        vec![
                            Event::PointerMoved(pos),
                            contact(1, egui::TouchPhase::Move, pos),
                            contact(2, egui::TouchPhase::Move, pos + egui::vec2(100.0, 0.0)),
                        ],
                    );
                }
                let pos = start + egui::vec2(30.4, 30.4);
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
                let pos = start + egui::vec2(60.4, 60.4);
                frame(
                    &ctx,
                    &mut chart,
                    0.10,
                    vec![
                        Event::PointerMoved(pos),
                        contact(1, egui::TouchPhase::Move, pos),
                    ],
                );
                let expected_offset = offset - if horizontal { 60.4 / spacing } else { 0.0 };
                let shift = if vertical {
                    20.0 * 60.4 / rect.height() as f64
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
            // Keep the font atlas across cases, as a host does across chart replacements.
            let ctx = egui::Context::default();
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
                    for (axis, group_enabled) in [(true, true), (false, true), (true, false)] {
                        let time = ctx.input(|input| input.time) + 1.0;
                        let mut chart = chart(&[95.0, 100.0], &[105.0, 110.0]);
                        chart.set_theme(&ctx, &theme);
                        chart.chart_options.handle_scroll.pressed_mouse_move = plot;
                        chart.chart_options.handle_scroll.enabled = plot;
                        chart.chart_options.handle_scale.enabled = group_enabled;
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
                        let axis = axis && group_enabled;
                        frame(&ctx, &mut chart, time, vec![]);
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
                        frame(
                            &ctx,
                            &mut chart,
                            time + 0.02,
                            vec![Event::PointerMoved(start)],
                        );
                        frame(&ctx, &mut chart, time + 0.04, vec![button(start, true)]);
                        frame(
                            &ctx,
                            &mut chart,
                            time + 0.06,
                            vec![Event::PointerMoved(start.lerp(end, 0.5))],
                        );
                        frame(
                            &ctx,
                            &mut chart,
                            time + 0.08,
                            vec![Event::PointerMoved(end)],
                        );
                        frame(&ctx, &mut chart, time + 0.10, vec![button(end, false)]);
                        let expected_span = if price && axis {
                            before * (100.0 / rect.height() as f64).exp()
                        } else {
                            before
                        };
                        // 100 points on the time axis scale the spacing by e^0.1 = 1.105171.
                        let expected_spacing = if !price && axis {
                            spacing * 1.105_171
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
                (x_left - rect.left()).abs() < 0.01
                    && (x_right - (rect.right() - 1.0)).abs() < 0.01,
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
        assert_eq!(one_bar.state.time_scale().bar_spacing(), rect.width() - 1.0);

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
