//! Replay toolbar UI
//!
//! Provide playback controls for market replay functionality including
//! play/pause/stop buttons, speed selector, progress bar, and time display.

use crate::ext::HasDesignTokens;
use crate::ext::UiExt;
use crate::icons::icons;
use crate::styles::{icons as icon_sizes, typography};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::{
    PlaybackSpeed, ReplayCommand, ReplayProgress, ToolbarReplayState as ReplayState,
};
use egui::{Response, Ui, Vec2, Widget};
use std::sync::mpsc::Sender;

/// Replay toolbar configuration
#[derive(Debug, Clone)]
pub struct ReplayToolbarConfig {
    /// Show speed selector
    pub show_speed_selector: bool,
    /// Show progress bar
    pub show_progress: bool,
    /// Show time display
    pub show_time: bool,
    /// Compact mode (icons only)
    pub compact: bool,
}

impl Default for ReplayToolbarConfig {
    fn default() -> Self {
        Self {
            show_speed_selector: true,
            show_progress: true,
            show_time: true,
            compact: false,
        }
    }
}

/// Replay toolbar state
#[derive(Debug, Clone, Default)]
pub struct ReplayToolbarState {
    /// Current replay state
    pub replay_state: ReplayState,
    /// Current speed
    pub speed: PlaybackSpeed,
    /// Current progress (if available)
    pub progress: Option<ReplayProgress>,
}

impl ReplayToolbarState {
    /// Sync toolbar replay state from external values.
    ///
    /// `active` / `playing` map to the toolbar-level replay state enum,
    /// and `speed_multiplier` is converted to the closest `PlaybackSpeed`.
    pub fn sync_from(&mut self, active: bool, playing: bool, speed_multiplier: f32) {
        if active {
            if playing {
                self.replay_state = ReplayState::Playing;
            } else {
                self.replay_state = ReplayState::Paused;
            }
        } else {
            self.replay_state = ReplayState::Stopped;
        }

        self.speed = match speed_multiplier {
            s if s <= 1.0 => PlaybackSpeed::RealTime,
            s if s <= 4.0 => PlaybackSpeed::Fast,
            s if s <= 100.0 => PlaybackSpeed::VeryFast,
            _ => PlaybackSpeed::UltraFast,
        };
    }
}

/// Action emitted by the replay toolbar
#[derive(Debug, Clone)]
pub enum ReplayToolbarAction {
    /// Play was clicked
    Play,
    /// Pause was clicked
    Pause,
    /// Stop was clicked
    Stop,
    /// Speed was changed
    SetSpeed(PlaybackSpeed),
    /// Step forward
    StepForward,
    /// Step backward
    StepBackward,
    /// Open replay settings dialog
    OpenSettings,
}

/// Replay toolbar widget
pub struct ReplayToolbar<'a> {
    state: &'a ReplayToolbarState,
    config: ReplayToolbarConfig,
    command_sender: Option<&'a Sender<ReplayCommand>>,
}

impl<'a> ReplayToolbar<'a> {
    /// Create a new replay toolbar
    pub fn new(state: &'a ReplayToolbarState) -> Self {
        Self {
            state,
            config: ReplayToolbarConfig::default(),
            command_sender: None,
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: ReplayToolbarConfig) -> Self {
        self.config = config;
        self
    }

    /// Set command sender for direct control
    pub fn with_command_sender(mut self, sender: &'a Sender<ReplayCommand>) -> Self {
        self.command_sender = Some(sender);
        self
    }

    /// Enable compact mode
    pub fn compact(mut self) -> Self {
        self.config.compact = true;
        self
    }

    /// Show the toolbar and return any action
    pub fn show(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let action = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.sm;

                let mut action = None;

                // Play/Pause button
                action = self.show_play_pause_button(ui).or(action);

                // Stop button
                action = self.show_stop_button(ui).or(action);

                ui.space_sm();

                // Step buttons
                action = self.show_step_buttons(ui).or(action);

                ui.space_md();

                // Speed selector
                if self.config.show_speed_selector {
                    action = self.show_speed_selector(ui).or(action);
                }

                ui.space_md();

                // Progress bar
                if self.config.show_progress {
                    self.show_progress_bar(ui);
                }

                // Time display
                if self.config.show_time {
                    self.show_time_display(ui);
                }

                // Settings button
                if ui
                    .add(icons::SETTINGS.as_button(Vec2::splat(icon_sizes::COMPACT)))
                    .clicked()
                {
                    action = Some(ReplayToolbarAction::OpenSettings);
                }

                action
            })
            .inner;

        // Send command if we have a sender
        if let (Some(sender), Some(act)) = (&self.command_sender, &action) {
            let cmd = match act {
                ReplayToolbarAction::Play => Some(ReplayCommand::Resume),
                ReplayToolbarAction::Pause => Some(ReplayCommand::Pause),
                ReplayToolbarAction::Stop => Some(ReplayCommand::Stop),
                ReplayToolbarAction::SetSpeed(s) => Some(ReplayCommand::SetSpeed(*s)),
                ReplayToolbarAction::StepForward => Some(ReplayCommand::StepForward),
                ReplayToolbarAction::StepBackward => Some(ReplayCommand::StepBackward),
                ReplayToolbarAction::OpenSettings => None,
            };

            if let Some(cmd) = cmd {
                let _ = sender.send(cmd);
            }
        }

        action
    }

    fn show_play_pause_button(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let (icon, tooltip, action) = match self.state.replay_state {
            ReplayState::Playing => (&icons::MEDIA_PAUSE, "Pause", ReplayToolbarAction::Pause),
            ReplayState::Paused | ReplayState::Stopped | ReplayState::Finished => {
                (&icons::MEDIA_PLAY, "Play", ReplayToolbarAction::Play)
            }
            ReplayState::Buffering => (
                &icons::UI_BUFFERING,
                "Buffering...",
                ReplayToolbarAction::Pause,
            ),
            ReplayState::Error => (&icons::STATUS_ERROR, "Error", ReplayToolbarAction::Stop),
        };

        let icon_size = if self.config.compact {
            icon_sizes::COMPACT
        } else {
            icon_sizes::SM
        };
        let response = ui.add(icon.as_button(Vec2::splat(icon_size)));

        if response.on_hover_text(tooltip).clicked() {
            Some(action)
        } else {
            None
        }
    }

    fn show_stop_button(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let enabled = self.state.replay_state.is_active();
        let response = ui.add_enabled(
            enabled,
            icons::MEDIA_STOP.as_button(Vec2::splat(icon_sizes::COMPACT)),
        );

        if response.on_hover_text("Stop").clicked() {
            Some(ReplayToolbarAction::Stop)
        } else {
            None
        }
    }

    fn show_step_buttons(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let can_step =
            self.state.replay_state == ReplayState::Paused || self.state.replay_state.can_start();

        let mut action = None;

        if ui
            .add_enabled(
                can_step,
                icons::MEDIA_SKIP_BACK.as_button(Vec2::splat(icon_sizes::COMPACT)),
            )
            .on_hover_text("Step backward")
            .clicked()
        {
            action = Some(ReplayToolbarAction::StepBackward);
        }

        if ui
            .add_enabled(
                can_step,
                icons::MEDIA_SKIP_FORWARD.as_button(Vec2::splat(icon_sizes::COMPACT)),
            )
            .on_hover_text("Step forward")
            .clicked()
        {
            action = Some(ReplayToolbarAction::StepForward);
        }

        action
    }

    fn show_speed_selector(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let mut action = None;
        let current = self.state.speed;

        egui::ComboBox::from_id_salt("replay_speed")
            .selected_text(current.display_name())
            .width(DESIGN_TOKENS.sizing.replay.speed_dropdown_width)
            .show_ui(ui, |ui| {
                for speed in [
                    PlaybackSpeed::RealTime,
                    PlaybackSpeed::Fast,
                    PlaybackSpeed::VeryFast,
                    PlaybackSpeed::UltraFast,
                ] {
                    if ui
                        .selectable_value(
                            &mut self.state.speed.clone(),
                            speed,
                            speed.display_name(),
                        )
                        .clicked()
                        && speed != current
                    {
                        action = Some(ReplayToolbarAction::SetSpeed(speed));
                    }
                }
            });

        action
    }

    fn show_progress_bar(&self, ui: &mut Ui) {
        if let Some(ref progress) = self.state.progress {
            let pct = progress.progress_pct() as f32;

            // Progress bar
            let desired_size = Vec2::new(
                DESIGN_TOKENS.sizing.replay.progress_bar_width / 2.0,
                typography::XXL,
            );
            let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let bg = ui.chart_bg();
                let fg = ui.accent_color();

                // Background
                painter.rect_filled(rect, DESIGN_TOKENS.rounding.xs, bg);

                // Progress fill
                let fill_width = rect.width() * pct;
                let fill_rect =
                    egui::Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));
                painter.rect_filled(fill_rect, DESIGN_TOKENS.rounding.xs, fg);

                // Text
                let text = format!("{:.0}%", pct * 100.0);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(typography::XS),
                    ui.text_color(),
                );
            }
        }
    }

    fn show_time_display(&self, ui: &mut Ui) {
        if let Some(ref progress) = self.state.progress {
            let time_str = progress.current_time.format("%H:%M:%S").to_string();
            ui.label(egui::RichText::new(time_str).monospace().small());

            // Bars played
            let bars_str = format!("{}/{}", progress.bars_played, progress.total_bars);
            ui.hint_label(bars_str);
        }
    }
}

impl Widget for ReplayToolbar<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            self.show(ui);
        })
        .response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_state_default() {
        let state = ReplayToolbarState::default();
        assert_eq!(state.replay_state, ReplayState::Stopped);
    }

    #[test]
    fn test_toolbar_config_default() {
        let config = ReplayToolbarConfig::default();
        assert!(config.show_speed_selector);
        assert!(config.show_progress);
    }
}
