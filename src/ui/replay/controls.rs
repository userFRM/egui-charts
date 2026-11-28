//! Replay controls UI widget
//!
//! Playback controls for historical data replay.

use super::{ReplayAction, ReplayColors, ReplayConfig, ReplaySpeed, ReplayState};
use crate::icons::icons;
use crate::localization as l10n;
use crate::styles::stroke;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};
use crate::ui_kit::ext::UiExt;
use egui::{RichText, Ui, Vec2};

/// The configured colours, or the active theme's while they are the defaults.
fn colors(config: &ReplayConfig, ui: &Ui) -> ReplayColors {
    if config.colors == ReplayColors::default() {
        ReplayColors::from_theme(&ui.active_theme())
    } else {
        config.colors.clone()
    }
}

/// The kit button size closest to a configured size in points.
fn kit_size(points: f32) -> ButtonSize {
    [
        ButtonSize::SM,
        ButtonSize::MD,
        ButtonSize::LG,
        ButtonSize::XL,
    ]
    .into_iter()
    .min_by(|a, b| {
        (a.pixels() - points)
            .abs()
            .total_cmp(&(b.pixels() - points).abs())
    })
    .unwrap_or_default()
}

/// Replay control bar widget
///
/// Provides play/pause, step, speed control, and progress bar for
/// replaying historical market data.
///
/// # Example
///
/// ```rust
/// use egui_charts::ui::replay::{ReplayConfig, ReplayControls, ReplayState};
///
/// let mut state = ReplayState::new();
/// let config = ReplayConfig::default();
/// let controls = ReplayControls::new(&config);
///
/// // In your UI loop:
/// # egui::__run_test_ui(|ui| {
/// let action = controls.show(ui, &mut state);
/// # });
/// ```
#[derive(Debug)]
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

        let layout = self.config.layout.resolved(&ui.active_theme());
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
                    RichText::new(l10n::text(ui.ctx(), "Click Replay to enter replay mode"))
                        .color(colors(self.config, ui).disabled_color),
                );
            }

            ui.add_space(layout.padding);
        });

        result_action
    }

    /// Render the replay mode toggle button
    fn render_mode_toggle(&self, ui: &mut Ui, state: &ReplayState) -> ReplayAction {
        let btn_text = if state.is_active() {
            "Exit replay"
        } else {
            "Replay"
        };
        let button = Button::new(btn_text)
            .secondary()
            .size(kit_size(self.config.layout.btn_size));

        if button.show(ui).clicked() {
            ReplayAction::ToggleReplayMode
        } else {
            ReplayAction::None
        }
    }

    /// Render playback control btns
    fn render_playback_controls(&self, ui: &mut Ui, state: &ReplayState) -> ReplayAction {
        let layout = self.config.layout.resolved(&ui.active_theme());
        let small = kit_size(layout.small_button_size);
        let mut action = ReplayAction::None;

        if IconButton::new(&icons::UI_ARROW_BACK)
            .size(small)
            .tooltip("Step backward (Left arrow)")
            .enabled(!state.at_start())
            .show(ui)
            .clicked()
        {
            action = ReplayAction::StepBackward;
        }

        ui.add_space(layout.btn_spacing);

        let (play_pause_icon, play_pause_hint) = if state.is_playing() {
            (&icons::MEDIA_PAUSE, "Pause (Space)")
        } else {
            (&icons::MEDIA_PLAY, "Play (Space)")
        };
        if IconButton::new(play_pause_icon)
            .size(kit_size(layout.btn_size))
            .tooltip(play_pause_hint)
            .show(ui)
            .clicked()
        {
            action = ReplayAction::TogglePlayPause;
        }

        ui.add_space(layout.btn_spacing);

        if IconButton::new(&icons::UI_ARROW_FORWARD)
            .size(small)
            .tooltip("Step forward (Right arrow)")
            .enabled(!state.at_end())
            .show(ui)
            .clicked()
        {
            action = ReplayAction::StepForward;
        }

        ui.add_space(layout.btn_spacing);

        if IconButton::new(&icons::MEDIA_STOP)
            .size(small)
            .tooltip("Reset to start")
            .show(ui)
            .clicked()
        {
            action = ReplayAction::Reset;
        }

        action
    }

    /// Render the progress bar
    fn render_progress_bar(&self, ui: &mut Ui, state: &mut ReplayState) -> ReplayAction {
        let layout = self.config.layout.resolved(&ui.active_theme());
        let colors = colors(self.config, ui);
        let mut action = ReplayAction::None;

        let progress = state.progress();
        let width = layout
            .progress_bar_width
            .unwrap_or(DESIGN_TOKENS.sizing.replay.progress_bar_width);

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, layout.progress_bar_height),
            egui::Sense::click_and_drag(),
        );
        crate::styles::focus::draw_focus_ring(ui, &response, ui.active_theme().radius.xs);
        response.widget_info(|| {
            egui::WidgetInfo::slider(
                response.enabled(),
                f64::from(progress) * 100.0,
                "Replay position",
            )
        });

        if ui.is_rect_visible(rect) {
            // Background
            ui.painter()
                .rect_filled(rect, ui.active_theme().radius.xs, colors.progress_bg);

            // Filled portion
            let filled_width = rect.width() * progress;
            let filled_rect =
                egui::Rect::from_min_size(rect.min, Vec2::new(filled_width, rect.height()));
            ui.painter().rect_filled(
                filled_rect,
                ui.active_theme().radius.xs,
                colors.progress_fill,
            );

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
        response.on_hover_text(l10n::format_text(
            ui.ctx(),
            "Bar {0} / {1} ({2}%)",
            &[
                ("0", format!("{}", state.curr_bar + 1)),
                ("1", format!("{}", state.total_bars)),
                ("2", format!("{:.1}", progress * 100.0)),
            ],
        ));

        action
    }

    /// Render speed control
    fn render_speed_control(&self, ui: &mut Ui, state: &mut ReplayState) -> ReplayAction {
        let layout = self.config.layout.resolved(&ui.active_theme());
        let colors = colors(self.config, ui);
        let small = kit_size(layout.small_button_size);
        let mut action = ReplayAction::None;

        if IconButton::new(&icons::MINUS)
            .size(small)
            .tooltip("Slow down")
            .show(ui)
            .clicked()
        {
            action = ReplayAction::SlowDown;
        }

        // Speed dropdown using ComboBox
        let speed_label = state.speed.label();
        let combo_id = ui.scope_id().with("speed_combo");

        egui::ComboBox::from_id_salt(combo_id)
            .selected_text(
                RichText::new(l10n::text(ui.ctx(), &speed_label)).color(colors.speed_color),
            )
            .width(layout.speed_dropdown_width)
            .show_ui(ui, |ui| {
                for preset in ReplaySpeed::presets() {
                    let label = preset.label();
                    if ui
                        .selectable_label(*preset == state.speed, l10n::text(ui.ctx(), &label))
                        .clicked()
                    {
                        action = ReplayAction::SetSpeed(preset.multiplier());
                    }
                }
            });

        if IconButton::new(&icons::PLUS)
            .size(small)
            .tooltip("Speed up")
            .show(ui)
            .clicked()
        {
            action = ReplayAction::SpeedUp;
        }

        action
    }

    /// Render info display (bar counter, time, etc.)
    fn render_info_display(&self, ui: &mut Ui, state: &ReplayState) {
        let behavior = &self.config.behavior;
        let layout = self.config.layout.resolved(&ui.active_theme());

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
                ui.label(
                    RichText::new(l10n::text(
                        ui.ctx(),
                        date.format("%Y-%m-%d %H:%M").to_string(),
                    ))
                    .small(),
                );
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
#[derive(Debug)]
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
        let colors = colors(self.config, ui);

        // Update playback
        let _bars = state.update();

        ui.horizontal(|ui| {
            let (icon, hint) = if state.is_playing() {
                (&icons::MEDIA_PAUSE, "Pause (Space)")
            } else {
                (&icons::MEDIA_PLAY, "Play (Space)")
            };
            if IconButton::new(icon)
                .size(ButtonSize::SM)
                .tooltip(hint)
                .show(ui)
                .clicked()
            {
                action = ReplayAction::TogglePlayPause;
            }

            // Simple progress bar, tinted with the configured fill color.
            let progress = state.progress();
            let progress_bar = egui::ProgressBar::new(progress)
                .show_percentage()
                .fill(colors.progress_fill);
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
            ui.label(
                RichText::new(l10n::text(ui.ctx(), state.speed.label()))
                    .small()
                    .color(colors.speed_color),
            );
        });

        action
    }
}

/// Replay keyboard handler
#[derive(Debug)]
pub struct ReplayKeyboardHandler;

impl ReplayKeyboardHandler {
    /// The replay action of a key pressed this frame. Left and Right step one bar back or
    /// forward, with or without Shift, as the keyboard help lists them.
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

            // Left arrow, with or without Shift - step backward one bar
            if i.key_pressed(egui::Key::ArrowLeft) {
                return ReplayAction::StepBackward;
            }

            // Right arrow, with or without Shift - step forward one bar
            if i.key_pressed(egui::Key::ArrowRight) {
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
    use super::super::ReplayController;
    use super::*;
    use crate::Chart;
    use crate::model::{Bar, BarClock, BarData, Timeframe};
    use crate::widget::SeriesId;
    use egui::{Event, Key, Modifiers};

    /// Shift+Right and Shift+Left step one bar, as the keyboard help lists them beside the plain
    /// arrows: playback pauses, the chart reveals one bar more or less, and both ends hold.
    #[test]
    fn shift_arrows_step_one_bar_as_the_keyboard_help_lists() {
        let bars: Vec<Bar> = (0..40)
            .map(|i| {
                let time = chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap();
                Bar::new(time, 100.0, 101.0, 99.0, 100.5, 1_000.0)
            })
            .collect();
        let clock = BarClock::new(Timeframe::Min1);
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_timeframe(Timeframe::Min1);
        let mut replay = ReplayController::new(bars, Vec::new());
        replay.handle_action(ReplayAction::JumpToBar(5));
        let ctx = egui::Context::default();
        // (action before the key, arrow pressed with Shift, cursor after, bars the chart reveals)
        for (before, key, cursor, revealed) in [
            (ReplayAction::Play, Key::ArrowRight, 6, 7),
            (ReplayAction::Play, Key::ArrowLeft, 5, 6),
            (ReplayAction::JumpToStart, Key::ArrowLeft, 0, 1),
            (ReplayAction::JumpToEnd, Key::ArrowRight, 39, 40),
        ] {
            replay.handle_action(before.clone());
            // egui reads held modifiers from ModifiersChanged, not from the key event.
            let events = vec![
                Event::ModifiersChanged(Modifiers::SHIFT),
                Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::SHIFT,
                },
            ];
            let mut action = ReplayAction::None;
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| action = ReplayKeyboardHandler::handle(ui, &replay.state),
            )
            .drop_without_applying_deltas();
            replay.handle_action(action);
            chart.set_replay_time(replay.cutoff(&clock));
            assert_eq!(
                (
                    replay.state.curr_bar,
                    chart.bars(SeriesId::MAIN).len(),
                    replay.state.is_playing(),
                ),
                (cursor, revealed, false),
                "Shift+{key:?} after {before:?}"
            );
        }
    }
}
