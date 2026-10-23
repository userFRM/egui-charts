//! User interaction handling for the chart widget.
//!
//! Implements tracking mode (auto-scroll to latest bar) and keyboard shortcuts
//! (arrow keys for pan, +/- for zoom, Home for jump-to-latest, F for fit-all).
//! These methods are called by the main widget paint loop.

use egui::{Response, Ui};

use crate::widget::Chart;
use crate::widget::pan_zoom::raw_wheel_delta;

impl Chart {
    /// Manage tracking mode: auto-scroll to the latest bar while active.
    ///
    /// Tracking mode keeps the chart viewport anchored to real-time data.
    /// It exits automatically based on the configured
    /// [`TrackingModeExitMode`](crate::config::TrackingModeExitMode): on mouse leave,
    /// on next tap, or on touch end. Any manual scroll/zoom/drag also deactivates it.
    pub fn handle_tracking_mode(&mut self, ui: &Ui, response: &Response) {
        // Track mouse entering/leaving chart area for tracking mode
        let was_in_chart = self.mouse_in_chart;
        self.mouse_in_chart = response.hovered();

        // Manual interaction (scroll, zoom, drag) cancels a pending `scroll_to_time`
        let manual_interaction = response.dragged()
            || raw_wheel_delta(ui).length_sq() > 0.0
            || ui.input(|i| i.smooth_scroll_delta.length_sq() > 0.0);
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
                TrackingModeExitMode::OnNextTap => {
                    // Exit on any click
                    if response.clicked() {
                        self.tracking_mode_active = false;
                    }
                }
                TrackingModeExitMode::OnTouchEnd => {
                    // Exit when drag/touch ends
                    if !response.dragged() && self.scroll_start_pos.is_some() {
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
    pub fn handle_keyboard_shortcuts(
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
    pub fn request_focus_if_needed(&self, response: &mut Response) {
        if self.chart_options.keyboard.enabled
            && (response.clicked() || response.dragged() || response.is_pointer_button_down_on())
        {
            response.request_focus();
        }
    }

    /// Display a grabbing cursor while the user is actively dragging (panning) the chart.
    pub fn set_panning_cursor(&self, ui: &Ui, response: &Response) {
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::widget::test_frames::chart;
    use egui::{Context, Event, Key, Modifiers, RawInput, Rect, Vec2};

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
