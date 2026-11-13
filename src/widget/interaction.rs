//! User interaction handling for the chart widget.
//!
//! Implements tracking mode (auto-scroll to latest bar) and keyboard shortcuts
//! through the chart's shared, configurable shortcut map.
//! These methods are called by the main widget paint loop.

use egui::{Event, Id, Pos2, Rect, Response, TouchDeviceId, TouchId, TouchPhase, Ui};

use crate::widget::Chart;
use crate::widget::pan_zoom::raw_wheel_delta;

#[derive(Clone)]
struct TouchContact {
    id: (TouchDeviceId, TouchId),
    origin: Pos2,
    position: Pos2,
    started: f64,
    tracking: bool,
}

/// UI state belongs to the chart response, like egui's own pointer gesture state.
#[derive(Clone, Default)]
pub(super) struct TouchCrosshair {
    contact: Option<TouchContact>,
    pub(super) position: Option<Pos2>,
    pub(super) touch_input: bool,
    pub(super) owns_gesture: bool,
}

pub(super) fn touch_crosshair(ui: &Ui, id: Id) -> TouchCrosshair {
    ui.ctx().data(|data| {
        data.get_temp::<TouchCrosshair>(id.with("touch_crosshair"))
            .unwrap_or_default()
    })
}

impl Chart {
    /// Returns whether a held readout owns this frame's drag, including its release.
    pub(super) fn handle_touch_crosshair(
        &mut self,
        ui: &Ui,
        id: Id,
        plot: Rect,
        price_button: bool,
        blocked: bool,
    ) -> bool {
        use crate::config::TouchTrackingExitMode;

        let options = &self.chart_options.tooltip;
        let mut state = touch_crosshair(ui, id);
        let owned_gesture = state
            .contact
            .as_ref()
            .is_some_and(|contact| contact.tracking);
        if !owned_gesture && state.position.is_some_and(|pos| !plot.contains(pos)) {
            state.position = None;
            state.contact = None;
        }
        let was_active = state.position.is_some();
        let pointer_in_plot = ui.rect_contains_pointer(plot);
        let mut repaint_after = None;
        let slop = ui
            .ctx()
            .options(|options| options.input_options.max_click_dist);
        ui.input(|input| {
            let touch_event = input
                .events
                .iter()
                .any(|event| matches!(event, Event::Touch { .. }));
            if touch_event {
                state.touch_input = true;
            } else if !input.any_touches()
                && input.events.iter().any(|event| {
                    matches!(
                        event,
                        Event::PointerMoved(_) | Event::PointerButton { pressed: true, .. }
                    )
                })
            {
                state = TouchCrosshair::default();
            }
            if !options.enabled
                || !options.on_long_press
                || blocked
                || input.events.contains(&Event::WindowFocused(false))
                || input.multi_touch().is_some()
            {
                state.contact = None;
                state.position = None;
                return;
            }
            // Keep the latched tag registered until its button consumes the release.
            if price_button && state.position.is_some() && state.contact.is_none() {
                return;
            }
            if options.touch_exit_mode == TouchTrackingExitMode::OnTouchEnd
                && state.contact.is_none()
            {
                state.position = None;
            }
            for event in &input.events {
                let Event::Touch {
                    device_id,
                    id,
                    phase,
                    pos,
                    ..
                } = event
                else {
                    continue;
                };
                let contact_id = (*device_id, *id);
                match phase {
                    TouchPhase::Start => {
                        if plot.contains(*pos) && pointer_in_plot {
                            state.contact = Some(TouchContact {
                                id: contact_id,
                                origin: *pos,
                                position: *pos,
                                started: input.time,
                                tracking: false,
                            });
                        } else {
                            state.contact = None;
                            state.position = None;
                        }
                    }
                    TouchPhase::Move => {
                        if let Some(contact) = &mut state.contact
                            && contact.id == contact_id
                        {
                            if !contact.tracking && contact.origin.distance(*pos) > slop {
                                if state.position.is_some() {
                                    contact.tracking = true;
                                } else {
                                    state.contact = None;
                                    continue;
                                }
                            }
                            contact.position = plot.clamp(*pos);
                            if contact.tracking {
                                state.position = Some(contact.position);
                            }
                        }
                    }
                    TouchPhase::End => {
                        if let Some(contact) = &state.contact
                            && contact.id == contact_id
                        {
                            if !contact.tracking
                                || options.touch_exit_mode == TouchTrackingExitMode::OnTouchEnd
                            {
                                state.position = None;
                            } else {
                                state.position = Some(plot.clamp(*pos));
                            }
                            state.contact = None;
                        }
                    }
                    TouchPhase::Cancel => {
                        state.contact = None;
                        state.position = None;
                    }
                }
            }
            if let Some(contact) = &mut state.contact {
                if !input.any_touches() {
                    state.contact = None;
                    state.position = None;
                } else if !contact.tracking {
                    let elapsed =
                        std::time::Duration::try_from_secs_f64(input.time - contact.started)
                            .unwrap_or_default();
                    let remaining = options.long_press_delay.saturating_sub(elapsed);
                    if remaining.is_zero() {
                        contact.tracking = true;
                        state.position = Some(contact.position);
                    } else {
                        repaint_after = Some(remaining);
                    }
                }
            }
        });
        if let Some(delay) = repaint_after {
            ui.ctx().request_repaint_after(delay);
        }
        state.owns_gesture = owned_gesture
            || state
                .contact
                .as_ref()
                .is_some_and(|contact| contact.tracking);
        let owns_drag = state.touch_input
            && (was_active || state.position.is_some() || state.contact.is_some());
        if owns_drag {
            self.kinetic_scroll.is_active = false;
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(id.with("touch_crosshair"), state));
        owns_drag
    }

    /// Manage tracking mode: auto-scroll to the latest bar while active.
    ///
    /// Tracking mode keeps the chart viewport anchored to real-time data.
    /// It exits automatically based on the configured
    /// [`TrackingModeExitMode`](crate::config::TrackingModeExitMode): on mouse leave,
    /// on next tap, or on touch end. Any manual scroll/zoom/drag also deactivates it.
    pub(crate) fn handle_tracking_mode(&mut self, ui: &Ui, response: &Response) {
        // Track mouse entering/leaving chart area for tracking mode
        let was_in_chart = self.mouse_in_chart;
        self.mouse_in_chart = response.hovered();

        // Manual interaction (scroll, zoom, drag) cancels a pending `scroll_to_time`
        let manual_interaction = response.dragged() || raw_wheel_delta(ui).length_sq() > 0.0;
        if manual_interaction {
            self.pending_scroll_time = None;
        }

        // Handle tracking mode exit conditions
        use crate::config::TrackingModeExitMode;
        if self.tracking_mode_active {
            match self.chart_options.tracking_mode.exit_mode {
                TrackingModeExitMode::OnMouseLeave => {
                    // Exit when mouse leaves the chart
                    if was_in_chart && !self.mouse_in_chart {
                        self.tracking_mode_active = false;
                    }
                }
                // A click or tap is both the next tap and a touch that ended. A press that
                // moved has ended tracking as a drag, below, and a contact that a touch
                // readout owns is not the chart's to end on.
                TrackingModeExitMode::OnNextTap | TrackingModeExitMode::OnTouchEnd => {
                    if response.clicked() {
                        self.tracking_mode_active = false;
                    }
                }
            }

            // Exit tracking mode on any manual interaction (scroll, zoom, drag)
            if manual_interaction {
                self.tracking_mode_active = false;
            }
        }

        // If tracking mode is active, keep scrolling to latest
        if self.tracking_mode_active {
            self.state.time_scale_mut().scroll_to_realtime();
        }
    }

    /// Dispatch configured shortcuts, retaining arrow-key focus on the chart.
    /// Hosts call [`Self::dispatch_shortcuts`] before their toolbars and after dialogs.
    pub(crate) fn handle_keyboard_shortcuts(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_width: f32,
        chart_rect_min_x: f32,
    ) {
        self.keyboard.chart_id = Some(response.id);
        if response.has_focus() {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: self.chart_options.keyboard.enabled,
                        vertical_arrows: self.chart_options.keyboard.enabled,
                        ..Default::default()
                    },
                );
            });
        }
        // These bounds are current even on the first frame or after resizing.
        self.last_rendered_price_rect.min.x = chart_rect_min_x;
        self.last_rendered_price_rect.max.x = chart_rect_min_x + chart_width;
        self.dispatch_shortcuts(ui.ctx());
    }

    /// Request keyboard focus when the user actually interacts with the chart
    /// (click, pointer-down, or drag), so keyboard shortcuts become active.
    ///
    /// Focus is deliberately *not* taken on bare hover: doing so would paint the
    /// keyboard focus ring whenever the pointer merely passes over the chart,
    /// which reads as a spurious selection. Tab/keyboard navigation still routes
    /// focus here through egui's own focus machinery, so the focus ring remains a
    /// genuine keyboard affordance.
    pub(crate) fn request_focus_if_needed(&self, response: &mut Response) {
        if self.chart_options.keyboard.enabled
            && (response.clicked() || response.dragged() || response.is_pointer_button_down_on())
        {
            response.request_focus();
        }
    }

    /// Display a grabbing cursor while the user is actively dragging (panning) the chart.
    pub(crate) fn set_panning_cursor(&self, ui: &Ui, response: &Response) {
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::widget::test_frames::{chart, frame};
    use egui::{Context, Event, Key, Modifiers, PointerButton, RawInput, Rect, Vec2};

    /// Live-edge following ends on the gesture its exit mode names, and on a drag or the wheel
    /// under every mode; a press that is still held ends it under none.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn tracking_exit_modes_follow_their_documented_gesture() {
        use crate::config::TrackingModeExitMode::{OnMouseLeave, OnNextTap, OnTouchEnd};
        // Whether following survives a press, a click, a drag, the pointer leaving and the wheel.
        for (mode, survives) in [
            (OnTouchEnd, [true, false, false, true, false]),
            (OnMouseLeave, [true, true, false, false, false]),
            (OnNextTap, [true, false, false, true, false]),
        ] {
            for (gesture, survives) in ["press", "click", "drag", "leave", "wheel"]
                .into_iter()
                .zip(survives)
            {
                let ctx = Context::default();
                let mut chart = chart(&[100.0], &[101.0]);
                chart.chart_options.tracking_mode.exit_mode = mode;
                frame(&ctx, &mut chart, 0.0, vec![]);
                let pos = chart.last_rendered_price_rect.center();
                let end = pos + egui::vec2(40.0, 0.0);
                frame(&ctx, &mut chart, 0.02, vec![Event::PointerMoved(pos)]);
                chart.enable_tracking_mode();
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                let steps = match gesture {
                    "press" => vec![button(pos, true)],
                    "click" => vec![button(pos, true), button(pos, false)],
                    "drag" => vec![
                        button(pos, true),
                        Event::PointerMoved(end),
                        button(end, false),
                    ],
                    "leave" => vec![Event::PointerGone],
                    _ => vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -240.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: Modifiers::NONE,
                    }],
                };
                let settled = 0.04 + steps.len() as f64 * 0.02;
                for (step, event) in steps.into_iter().enumerate() {
                    frame(&ctx, &mut chart, 0.04 + step as f64 * 0.02, vec![event]);
                }
                // A pointer that has gone stops hovering the chart in the following frame.
                frame(&ctx, &mut chart, settled, vec![]);
                assert_eq!(
                    chart.is_tracking_mode_active(),
                    survives,
                    "{mode:?}, {gesture}"
                );
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn pending_scroll_survives_smoothed_wheel_remainder() {
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[101.0]);
        let target = chart.state.data().bars[0].time;
        for (step, fresh) in [true, false, true].into_iter().enumerate() {
            chart.scroll_to_time(target);
            ctx.run_ui(
                RawInput {
                    time: Some(step as f64 * 0.02),
                    events: if fresh {
                        vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -240.0),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Wheel,
                            modifiers: Modifiers::NONE,
                        }]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ui| {
                    let response = ui.allocate_response(Vec2::splat(100.0), egui::Sense::hover());
                    if !fresh {
                        assert!(ui.input(|i| i.smooth_scroll_delta.length_sq() > 0.0));
                    }
                    chart.handle_tracking_mode(ui, &response);
                },
            )
            .drop_without_applying_deltas();
            assert_eq!(
                chart.pending_scroll_time,
                (!fresh).then_some(target),
                "step={step}, fresh={fresh}"
            );
        }
    }

    #[test]
    fn horizontal_arrows_pan_repeatedly_and_tab_leaves_the_chart() {
        for enabled in [true, false] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[101.0]);
            chart.chart_options.keyboard.enabled = enabled;
            let mut chart_id = egui::Id::NULL;
            chart.config.show_right_axis = false;
            chart.config.show_ohlc_info = false;
            let mut next_id = egui::Id::NULL;
            for (pass, key) in [
                None,
                None,
                Some(Key::ArrowLeft),
                Some(Key::ArrowLeft),
                Some(Key::ArrowRight),
                Some(Key::Tab),
            ]
            .into_iter()
            .enumerate()
            {
                let before = chart.state.time_scale().right_offset();
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1000.0, 500.0),
                        )),
                        events: key
                            .map(|key| Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            })
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    },
                    |ui| {
                        ui.horizontal(|ui| {
                            let _ = ui.button("Before");
                            ui.allocate_ui(Vec2::new(700.0, 400.0), |ui| {
                                let response = chart.show(ui);
                                chart_id = response.id;
                                if pass == 0 {
                                    response.request_focus();
                                }
                            });
                            next_id = ui.button("After").id;
                        });
                    },
                );
                output.drop_without_applying_deltas();
                if pass >= 2 && key != Some(Key::Tab) {
                    let expected = if !enabled {
                        0.0
                    } else if key == Some(Key::ArrowLeft) {
                        -chart.chart_options.keyboard.pan_amount
                    } else {
                        chart.chart_options.keyboard.pan_amount
                    };
                    assert_eq!(
                        chart.state.time_scale().right_offset(),
                        before - expected,
                        "pass {pass}"
                    );
                    if enabled {
                        assert!(
                            ctx.memory(|memory| memory.has_focus(chart_id)),
                            "pass {pass}"
                        );
                    } else if pass == 2 {
                        assert!(
                            !ctx.memory(|memory| memory.has_focus(chart_id)),
                            "disabled arrows must keep native focus navigation"
                        );
                    }
                }
            }
            assert!(!ctx.memory(|memory| memory.has_focus(chart_id)));
            assert!(
                ctx.memory(|memory| memory.focused().is_some()),
                "expected keyboard focus after Tab, next {next_id:?}"
            );
        }
    }
}
