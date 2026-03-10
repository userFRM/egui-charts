//! User interaction handling for the chart widget.
//!
//! Implements tracking mode (auto-scroll to latest bar) and keyboard shortcuts
//! (arrow keys for pan, +/- for zoom, Home for jump-to-latest, F for fit-all).
//! These methods are called by the main widget paint loop.

use egui::{Response, Ui};

use crate::widget::Chart;

impl Chart {
    /// Manage tracking mode — auto-scroll to the latest bar while active.
    ///
    /// Tracking mode keeps the chart viewport anchored to real-time data.
    /// It exits automatically based on the configured
    /// [`TrackingModeExitMode`](crate::config::TrackingModeExitMode): on mouse leave,
    /// on next tap, or on touch end. Any manual scroll/zoom/drag also deactivates it.
    pub fn handle_tracking_mode(&mut self, ui: &Ui, response: &Response) {
        // Track mouse entering/leaving chart area for tracking mode
        let was_in_chart = self.mouse_in_chart;
        self.mouse_in_chart = response.hovered();

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
            if response.dragged()
                || ui.input(|i| {
                    i.raw_scroll_delta.length_sq() > 0.0 || i.smooth_scroll_delta.length_sq() > 0.0
                })
            {
                self.tracking_mode_active = false;
            }
        }

        // If tracking mode is active, keep scrolling to latest
        if self.tracking_mode_active {
            self.state.time_scale_mut().scroll_to_realtime();
        }
    }

    /// Process keyboard shortcuts when the chart has focus.
    ///
    /// | Key | Action |
    /// |-----|--------|
    /// | Left/Right | Pan by configured `pan_amount` bars |
    /// | +/- | Zoom in/out by `zoom_step` centered on chart |
    /// | Home | Scroll to latest (real-time) data |
    /// | F | Fit all data into the viewport |
    /// | PageUp/PageDown | Zoom in/out by `3x zoom_step` |
    ///
    /// Shortcuts are only active when `chart_options.keyboard.enabled` is `true`
    /// and the chart response has focus.
    pub fn handle_keyboard_shortcuts(
        &mut self,
        ui: &Ui,
        response: &Response,
        chart_width: f32,
        chart_rect_min_x: f32,
    ) {
        if !self.chart_options.keyboard.enabled || !response.has_focus() {
            return;
        }

        ui.input(|i| {
            use egui::Key;

            // Pan left (Left arrow)
            if i.key_pressed(Key::ArrowLeft) {
                self.state
                    .time_scale_mut()
                    .scroll_bars(-self.chart_options.keyboard.pan_amount);
            }

            // Pan right (Right arrow)
            if i.key_pressed(Key::ArrowRight) {
                self.state
                    .time_scale_mut()
                    .scroll_bars(self.chart_options.keyboard.pan_amount);
            }

            // Zoom in (+)
            if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                let zoom_point_x = chart_width / 2.0;
                self.state.time_scale_mut().zoom(
                    self.chart_options.keyboard.zoom_step,
                    zoom_point_x,
                    chart_rect_min_x,
                    chart_width,
                );
            }

            // Zoom out (-)
            if i.key_pressed(Key::Minus) {
                let zoom_point_x = chart_width / 2.0;
                self.state.time_scale_mut().zoom(
                    -self.chart_options.keyboard.zoom_step,
                    zoom_point_x,
                    chart_rect_min_x,
                    chart_width,
                );
            }

            // Scroll to real-time / latest data (Home)
            if i.key_pressed(Key::Home) {
                self.state.time_scale_mut().scroll_to_realtime();
            }

            // Fit content (F) - zoom to show all data
            if i.key_pressed(Key::F) {
                self.state.time_scale_mut().fit_content();
            }

            // Page Up (zoom in more)
            if i.key_pressed(Key::PageUp) {
                let zoom_point_x = chart_width / 2.0;
                self.state.time_scale_mut().zoom(
                    self.chart_options.keyboard.zoom_step * 3.0,
                    zoom_point_x,
                    chart_rect_min_x,
                    chart_width,
                );
            }

            // Page Down (zoom out more)
            if i.key_pressed(Key::PageDown) {
                let zoom_point_x = chart_width / 2.0;
                self.state.time_scale_mut().zoom(
                    -self.chart_options.keyboard.zoom_step * 3.0,
                    zoom_point_x,
                    chart_rect_min_x,
                    chart_width,
                );
            }
        });
    }

    /// Automatically request keyboard focus when the pointer hovers over the chart,
    /// so keyboard shortcuts work without an explicit click.
    pub fn request_focus_if_needed(&self, response: &mut Response) {
        if self.chart_options.keyboard.enabled && response.hovered() {
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
