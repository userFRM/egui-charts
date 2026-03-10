//! Replay controls UI widget
//!
//! Playback controls for historical data replay.

use super::{ReplayAction, ReplayConfig, ReplaySpeed, ReplayState};
use crate::ext::UiExt;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{RichText, Ui, Vec2};

/// Replay control bar widget
///
/// Provides play/pause, step, speed control, and progress bar for
/// replaying historical market data.
///
/// # Example
///
/// ```rust,ignore
/// use egui_open_trading_charts::ui::replay::{ReplayControls, ReplayState, ReplayConfig};
///
/// let mut state = ReplayState::new();
/// let config = ReplayConfig::default();
/// let controls = ReplayControls::new(&config);
///
/// // In your UI loop:
/// let action = controls.show(ui, &mut state);
/// ```
pub struct ReplayControls<'a> {
    config: &'a ReplayConfig,
}

impl<'a> ReplayControls<'a> {
    /// Create new replay controls
    pub fn new(config: &'a ReplayConfig) -> Self {
        Self { config }
    }

    /// Show the replay control bar
    pub fn show(&self, ui: &mut Ui, state: &mut ReplayState) -> ReplayAction {
        // Update playback timing
        let _bars_advanced = state.update();

        let layout = &self.config.layout;
        let mut result_action = ReplayAction::None;

        ui.horizontal(|ui| {
            ui.set_height(layout.control_bar_height);
            ui.add_space(layout.padding);

            // Replay mode toggle
            if let action @ ReplayAction::ToggleReplayMode = self.render_mode_toggle(ui, state) {
                result_action = action;
            }

            ui.add_space(layout.btn_spacing);
            ui.separator();
            ui.add_space(layout.btn_spacing);

            // Only show full controls when replay is active
            if state.is_active() {
                // Playback controls
                let playback_action = self.render_playback_controls(ui, state);
                if !matches!(playback_action, ReplayAction::None) {
                    result_action = playback_action;
                }

                ui.add_space(layout.btn_spacing);
                ui.separator();
                ui.add_space(layout.btn_spacing);

                // Progress bar
                let progress_action = self.render_progress_bar(ui, state);
                if !matches!(progress_action, ReplayAction::None) {
                    result_action = progress_action;
                }

                ui.add_space(layout.btn_spacing);
                ui.separator();
                ui.add_space(layout.btn_spacing);

                // Speed control
                let speed_action = self.render_speed_control(ui, state);
                if !matches!(speed_action, ReplayAction::None) {
                    result_action = speed_action;
                }

                ui.add_space(layout.btn_spacing);
                ui.separator();
                ui.add_space(layout.btn_spacing);

                // Info display
                self.render_info_display(ui, state);
            } else {
                // Show hint when not in replay mode
                ui.label(
                    RichText::new("Click 'Replay' to enter replay mode")
                        .color(self.config.colors.disabled_color),
                );
            }

            ui.add_space(layout.padding);
        });

        result_action
    }

    /// Render the replay mode toggle button
    fn render_mode_toggle(&self, ui: &mut Ui, state: &ReplayState) -> ReplayAction {
        let is_active = state.is_active();
        let btn_text = if is_active { "Exit" } else { "Replay" };
        let btn_color = if is_active {
            self.config.colors.stop_color
        } else {
            self.config.colors.active_indicator
        };

        let button =
            egui::Button::new(RichText::new(btn_text).color(btn_color)).min_size(Vec2::new(
                DESIGN_TOKENS.sizing.replay.speed_dropdown_width,
                self.config.layout.btn_size,
            ));

        if ui.add(button).clicked() {
            ReplayAction::ToggleReplayMode
        } else {
            ReplayAction::None
        }
    }

    /// Render playback control btns
    fn render_playback_controls(&self, ui: &mut Ui, state: &ReplayState) -> ReplayAction {
        let layout = &self.config.layout;
        let colors = &self.config.colors;
        let mut action = ReplayAction::None;

        // Step backward
        let step_back_enabled = !state.at_start();
        let step_back_btn = egui::Button::new("|<").min_size(Vec2::splat(layout.small_button_size));
        let step_back_res = ui.add_enabled(step_back_enabled, step_back_btn);
        if step_back_res.clicked() {
            action = ReplayAction::StepBackward;
        }
        step_back_res.on_hover_text("Step backward (Left)");

        ui.add_space(layout.btn_spacing);

        // Play/Pause toggle
        let (play_pause_icon, play_pause_color, play_pause_hint) = if state.is_playing() {
            ("||", colors.pause_color, "Pause (Space)")
        } else {
            (">", colors.play_color, "Play (Space)")
        };

        let play_pause_btn =
            egui::Button::new(RichText::new(play_pause_icon).color(play_pause_color))
                .min_size(Vec2::splat(layout.btn_size));
        if ui
            .add(play_pause_btn)
            .on_hover_text(play_pause_hint)
            .clicked()
        {
            action = ReplayAction::TogglePlayPause;
        }

        ui.add_space(layout.btn_spacing);

        // Step forward
        let step_fwd_enabled = !state.at_end();
        let step_fwd_btn = egui::Button::new(">|").min_size(Vec2::splat(layout.small_button_size));
        let step_fwd_res = ui.add_enabled(step_fwd_enabled, step_fwd_btn);
        if step_fwd_res.clicked() {
            action = ReplayAction::StepForward;
        }
        step_fwd_res.on_hover_text("Step forward (Right)");

        ui.add_space(layout.btn_spacing);

        // Stop/Reset
        let stop_btn = egui::Button::new(RichText::new("[]").color(colors.stop_color))
            .min_size(Vec2::splat(layout.small_button_size));
        if ui.add(stop_btn).on_hover_text("Reset to start").clicked() {
            action = ReplayAction::Reset;
        }

        action
    }

    /// Render the progress bar
    fn render_progress_bar(&self, ui: &mut Ui, state: &mut ReplayState) -> ReplayAction {
        let layout = &self.config.layout;
        let colors = &self.config.colors;
        let mut action = ReplayAction::None;

        let progress = state.progress();
        let width = layout
            .progress_bar_width
            .unwrap_or(DESIGN_TOKENS.sizing.replay.progress_bar_width);

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, layout.progress_bar_height),
            egui::Sense::click_and_drag(),
        );

        if ui.is_rect_visible(rect) {
            // Background
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.xs, colors.progress_bg);

            // Filled portion
            let filled_width = rect.width() * progress;
            let filled_rect =
                egui::Rect::from_min_size(rect.min, Vec2::new(filled_width, rect.height()));
            ui.painter()
                .rect_filled(filled_rect, DESIGN_TOKENS.rounding.xs, colors.progress_fill);

            // Cursor/handle
            let cursor_x = rect.min.x + filled_width;
            let cursor_center = egui::pos2(cursor_x, rect.center().y);
            ui.painter().circle_filled(
                cursor_center,
                layout.progress_bar_height * 0.8,
                colors.progress_cursor,
            );

            // Markers
            for marker in &state.markers {
                if state.total_bars > 1 {
                    let marker_progress = marker.bar_idx as f32 / (state.total_bars - 1) as f32;
                    let marker_x = rect.min.x + rect.width() * marker_progress;
                    let marker_color = egui::Color32::from_rgba_unmultiplied(
                        marker.color[0],
                        marker.color[1],
                        marker.color[2],
                        marker.color[3],
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(marker_x, rect.min.y),
                            egui::pos2(marker_x, rect.max.y),
                        ],
                        egui::Stroke::new(stroke::MEDIUM, marker_color),
                    );
                }
            }
        }

        // Handle interaction
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            let relative_x = (pos.x - rect.min.x) / rect.width();
            let new_progress = relative_x.clamp(0.0, 1.0);
            action = ReplayAction::JumpToPercent(new_progress);
        }

        // Show tooltip with position info
        response.on_hover_text(format!(
            "Bar {} / {} ({:.1}%)",
            state.curr_bar + 1,
            state.total_bars,
            progress * 100.0
        ));

        action
    }

    /// Render speed control
    fn render_speed_control(&self, ui: &mut Ui, state: &mut ReplayState) -> ReplayAction {
        let layout = &self.config.layout;
        let colors = &self.config.colors;
        let mut action = ReplayAction::None;

        // Slow down button
        let slow_btn = egui::Button::new("-").min_size(Vec2::splat(layout.small_button_size));
        if ui.add(slow_btn).on_hover_text("Slow down").clicked() {
            action = ReplayAction::SlowDown;
        }

        // Speed dropdown using ComboBox
        let speed_label = state.speed.label();
        let combo_id = ui.id().with("speed_combo");

        egui::ComboBox::from_id_salt(combo_id)
            .selected_text(RichText::new(&speed_label).color(colors.speed_color))
            .width(layout.speed_dropdown_width)
            .show_ui(ui, |ui| {
                for preset in ReplaySpeed::presets() {
                    let label = preset.label();
                    if ui
                        .selectable_label(*preset == state.speed, &label)
                        .clicked()
                    {
                        action = ReplayAction::SetSpeed(preset.multiplier());
                    }
                }
            });

        // Speed up button
        let fast_btn = egui::Button::new("+").min_size(Vec2::splat(layout.small_button_size));
        if ui.add(fast_btn).on_hover_text("Speed up").clicked() {
            action = ReplayAction::SpeedUp;
        }

        action
    }

    /// Render info display (bar counter, time, etc.)
    fn render_info_display(&self, ui: &mut Ui, state: &ReplayState) {
        let behavior = &self.config.behavior;
        let layout = &self.config.layout;

        ui.right_aligned(|ui| {
            // Bar counter
            if behavior.show_bar_counter {
                ui.label(
                    RichText::new(format!("{}/{}", state.curr_bar + 1, state.total_bars)).small(),
                );
            }

            // Current date/time
            if let Some(date) = state.curr_date {
                ui.add_space(layout.btn_spacing);
                ui.label(RichText::new(date.format("%Y-%m-%d %H:%M").to_string()).small());
            }

            // Symbol
            if let Some(ref symbol) = state.symbol {
                ui.add_space(layout.btn_spacing);
                ui.strong_label(symbol);
            }
        });
    }
}

/// Compact replay controls (minimal UI)
#[allow(dead_code)]
pub struct CompactReplayControls<'a> {
    config: &'a ReplayConfig,
}

impl<'a> CompactReplayControls<'a> {
    /// Create new compact replay controls
    pub fn new(config: &'a ReplayConfig) -> Self {
        Self { config }
    }

    /// Show compact controls (just play/pause and progress)
    pub fn show(&self, ui: &mut Ui, state: &mut ReplayState) -> ReplayAction {
        let mut action = ReplayAction::None;

        // Update playback
        let _bars = state.update();

        ui.horizontal(|ui| {
            // Play/Pause
            let icon = if state.is_playing() { "||" } else { ">" };
            if ui.small_button(icon).clicked() {
                action = ReplayAction::TogglePlayPause;
            }

            // Simple progress bar
            let progress = state.progress();
            let progress_bar = egui::ProgressBar::new(progress).show_percentage();
            let response = ui.add(progress_bar);

            // Allow clicking on progress bar to seek
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let rect = response.rect;
                let relative_x = (pos.x - rect.min.x) / rect.width();
                action = ReplayAction::JumpToPercent(relative_x.clamp(0.0, 1.0));
            }

            // Speed label
            ui.label(RichText::new(state.speed.label()).small());
        });

        action
    }
}

/// Replay keyboard handler
pub struct ReplayKeyboardHandler;

impl ReplayKeyboardHandler {
    /// Handle keyboard input for replay controls
    pub fn handle(ui: &Ui, state: &ReplayState) -> ReplayAction {
        if !state.is_active() {
            return ReplayAction::None;
        }

        // Only handle when no widget has focus
        if ui.ctx().memory(|mem| mem.focused().is_some()) {
            return ReplayAction::None;
        }

        ui.input(|i| {
            // Space - toggle play/pause
            if i.key_pressed(egui::Key::Space) {
                return ReplayAction::TogglePlayPause;
            }

            // Left arrow - step backward
            if i.key_pressed(egui::Key::ArrowLeft) {
                if i.modifiers.shift {
                    return ReplayAction::StepBackwardN(10);
                }
                return ReplayAction::StepBackward;
            }

            // Right arrow - step forward
            if i.key_pressed(egui::Key::ArrowRight) {
                if i.modifiers.shift {
                    return ReplayAction::StepForwardN(10);
                }
                return ReplayAction::StepForward;
            }

            // Up arrow - speed up
            if i.key_pressed(egui::Key::ArrowUp) {
                return ReplayAction::SpeedUp;
            }

            // Down arrow - slow down
            if i.key_pressed(egui::Key::ArrowDown) {
                return ReplayAction::SlowDown;
            }

            // Home - jump to start
            if i.key_pressed(egui::Key::Home) {
                return ReplayAction::JumpToStart;
            }

            // End - jump to end
            if i.key_pressed(egui::Key::End) {
                return ReplayAction::JumpToEnd;
            }

            // M - add marker
            if i.key_pressed(egui::Key::M) {
                return ReplayAction::AddMarker {
                    label: None,
                    color: None,
                };
            }

            // R - reset
            if i.key_pressed(egui::Key::R) && i.modifiers.ctrl {
                return ReplayAction::Reset;
            }

            // Escape - exit replay mode
            if i.key_pressed(egui::Key::Escape) {
                return ReplayAction::ExitReplayMode;
            }

            ReplayAction::None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controls_creation() {
        let config = ReplayConfig::default();
        let _controls = ReplayControls::new(&config);
    }

    #[test]
    fn test_compact_controls_creation() {
        let config = ReplayConfig::compact();
        let _controls = CompactReplayControls::new(&config);
    }
}
