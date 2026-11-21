//! Replay toolbar UI
//!
//! Provide playback controls for market replay functionality including
//! play/pause/stop buttons, speed selector, progress bar, and time display.

use super::model::{ReplayCommand, ReplayProgress, ToolbarReplayState as ReplayState};
use super::{PlaybackState, ReplayAction, ReplayController, ReplaySpeed};
use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::ext::UiExt;
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
    /// Current speed, as the replay controller holds it
    pub speed: ReplaySpeed,
    /// Current progress (if available)
    pub progress: Option<ReplayProgress>,
}

impl ReplayToolbarState {
    /// Sync toolbar replay state from external values.
    ///
    /// `active` / `playing` map to the toolbar-level replay state enum. `speed_multiplier` is
    /// read as [`ReplayAction::SetSpeed`](super::ReplayAction::SetSpeed) reads it: clamped to
    /// 0.1 to 100, 1 when not finite, and the preset within 0.01 when there is one.
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

        self.speed = ReplaySpeed::from(speed_multiplier);
    }
}

/// The toolbar's view of a replay controller: its playback state, speed and position. The
/// progress counts the bars revealed, the current one included, as the docked controls'
/// counter does; a controller outside replay mode has none.
impl From<&ReplayController> for ReplayToolbarState {
    fn from(controller: &ReplayController) -> Self {
        let state = &controller.state;
        let replay_state = match state.playback_state {
            PlaybackState::Inactive => ReplayState::Stopped,
            PlaybackState::Paused => ReplayState::Paused,
            PlaybackState::Playing => ReplayState::Playing,
            PlaybackState::Finished => ReplayState::Finished,
        };
        let progress = state
            .is_active()
            .then(|| controller.curr_ts())
            .flatten()
            .map(|&current_time| {
                let bars_played = state.curr_bar + 1;
                ReplayProgress {
                    current_time,
                    bars_played,
                    total_bars: state.total_bars,
                    ticks_processed: 0,
                    progress_pct: bars_played as f32 / state.total_bars.max(1) as f32 * 100.0,
                }
            });
        Self {
            replay_state,
            speed: state.speed,
            progress,
        }
    }
}

/// Action emitted by the replay toolbar
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ReplayToolbarAction {
    /// Play was clicked
    Play,
    /// Pause was clicked
    Pause,
    /// Stop was clicked
    Stop,
    /// Speed was changed
    SetSpeed(ReplaySpeed),
    /// Step forward
    StepForward,
    /// Step backward
    StepBackward,
    /// Open replay settings dialog
    OpenSettings,
}

/// The controller action each toolbar action stands for, as the docked
/// [`ReplayControls`](super::ReplayControls) send it: Play toggles playback, so it plays a
/// paused replay and restarts a finished one from its first bar; Stop leaves replay, as Exit
/// replay does; Settings is left to the host and converts to [`ReplayAction::None`].
impl From<ReplayToolbarAction> for ReplayAction {
    fn from(action: ReplayToolbarAction) -> Self {
        match action {
            ReplayToolbarAction::Play => ReplayAction::TogglePlayPause,
            ReplayToolbarAction::Pause => ReplayAction::Pause,
            ReplayToolbarAction::Stop => ReplayAction::Stop,
            ReplayToolbarAction::SetSpeed(speed) => ReplayAction::SetSpeed(speed.multiplier()),
            ReplayToolbarAction::StepForward => ReplayAction::StepForward,
            ReplayToolbarAction::StepBackward => ReplayAction::StepBackward,
            ReplayToolbarAction::OpenSettings => ReplayAction::None,
        }
    }
}

/// Replay toolbar widget
#[derive(Debug)]
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
                ui.spacing_mut().item_spacing.x = ui.active_theme().spacing.sm;

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
                if IconButton::new(&icons::SETTINGS)
                    .size(self.button_size())
                    .tooltip("Settings")
                    .show(ui)
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

    fn button_size(&self) -> ButtonSize {
        if self.config.compact {
            ButtonSize::SM
        } else {
            ButtonSize::MD
        }
    }

    fn show_play_pause_button(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let (icon, tooltip, action) = match self.state.replay_state {
            ReplayState::Playing => (&icons::MEDIA_PAUSE, "Pause", ReplayToolbarAction::Pause),
            ReplayState::Paused | ReplayState::Stopped | ReplayState::Finished => {
                (&icons::MEDIA_PLAY, "Play", ReplayToolbarAction::Play)
            }
            ReplayState::Buffering => (
                &icons::UI_BUFFERING,
                "Buffering…",
                ReplayToolbarAction::Pause,
            ),
            ReplayState::Error => (&icons::STATUS_ERROR, "Error", ReplayToolbarAction::Stop),
        };

        // Outside replay mode Play toggles nothing; the docked controls offer no Play there.
        let response = IconButton::new(icon)
            .size(self.button_size())
            .tooltip(tooltip)
            .enabled(self.state.replay_state != ReplayState::Stopped)
            .show(ui);

        if response.clicked() {
            Some(action)
        } else {
            None
        }
    }

    fn show_stop_button(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let response = IconButton::new(&icons::MEDIA_STOP)
            .size(self.button_size())
            .tooltip("Stop")
            .enabled(self.state.replay_state != ReplayState::Stopped)
            .show(ui);

        if response.clicked() {
            Some(ReplayToolbarAction::Stop)
        } else {
            None
        }
    }

    fn show_step_buttons(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let can_step = matches!(
            self.state.replay_state,
            ReplayState::Paused | ReplayState::Finished
        );

        let mut action = None;

        if IconButton::new(&icons::UI_ARROW_BACK)
            .size(self.button_size())
            .tooltip("Step backward")
            .enabled(can_step)
            .show(ui)
            .clicked()
        {
            action = Some(ReplayToolbarAction::StepBackward);
        }

        if IconButton::new(&icons::UI_ARROW_FORWARD)
            .size(self.button_size())
            .tooltip("Step forward")
            .enabled(can_step)
            .show(ui)
            .clicked()
        {
            action = Some(ReplayToolbarAction::StepForward);
        }

        action
    }

    fn show_speed_selector(&self, ui: &mut Ui) -> Option<ReplayToolbarAction> {
        let mut action = None;
        let current = self.state.speed;
        let spacing = ui.spacing();
        let row_height = (ui.text_style_height(&egui::TextStyle::Button)
            + 2.0 * spacing.button_padding.y)
            .max(spacing.interact_size.y)
            + spacing.item_spacing.y;

        egui::ComboBox::from_id_salt("replay_speed")
            .selected_text(l10n::text(ui.ctx(), current.label()))
            .width(DESIGN_TOKENS.sizing.replay.speed_dropdown_width)
            .height(row_height * ReplaySpeed::presets().len() as f32)
            .show_ui(ui, |ui| {
                for speed in ReplaySpeed::presets().iter().copied() {
                    if ui
                        .selectable_value(
                            &mut self.state.speed.clone(),
                            speed,
                            l10n::text(ui.ctx(), speed.label()),
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
            let theme = ui.active_theme();

            // Progress bar
            let desired_size = Vec2::new(
                DESIGN_TOKENS.sizing.replay.progress_bar_width / 2.0,
                theme.spacing.sm,
            );
            let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
            let translated_label = l10n::text(ui.ctx(), "Replay progress");
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(
                    egui::Role::ProgressIndicator,
                    response.enabled(),
                    &translated_label,
                );
                info.value = Some(f64::from(pct) * 100.0);
                info
            });

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let (bg, fg) = (theme.chart.bg, theme.ui.accent);

                // Background
                painter.rect_filled(rect, ui.active_theme().radius.xs, bg);

                // Progress fill
                let fill_width = rect.width() * pct;
                let fill_rect =
                    egui::Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));
                painter.rect_filled(fill_rect, ui.active_theme().radius.xs, fg);
            }

            // Beside the bar, never over the fill.
            ui.label(
                egui::RichText::new(format!("{:.0}%", pct * 100.0))
                    .size(theme.text.small)
                    .color(theme.ui.text_secondary),
            );
        }
    }

    fn show_time_display(&self, ui: &mut Ui) {
        if let Some(ref progress) = self.state.progress {
            let time_str = progress.current_time.format("%H:%M:%S").to_string();
            ui.label(
                egui::RichText::new(l10n::text(ui.ctx(), time_str))
                    .monospace()
                    .small(),
            );

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
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    /// The speed text is the controller's own, also for speeds between the old four.
    #[test]
    fn toolbar_shows_the_speed_it_syncs() {
        let shown: Vec<String> = [0.25_f32, 0.5, 1.0, 4.0, 10.0, 50.0, 100.0]
            .iter()
            .map(|&speed| {
                let mut state = ReplayToolbarState::default();
                state.sync_from(true, false, speed);
                let mut harness = Harness::builder()
                    .allow_missing_glyphs()
                    .with_accessibility_check(false)
                    .build_ui(move |ui| {
                        ReplayToolbar::new(&state).show(ui);
                    });
                harness.run();
                harness
                    .get_by_role(egui::Role::ComboBox)
                    .accesskit_node()
                    .value()
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(shown, ["0.25x", "0.5x", "1x", "4x", "10x", "50x", "100x"]);
    }

    /// Each toolbar action moves the controller as the docked controls' button for it does.
    #[test]
    fn toolbar_actions_drive_the_controller_as_the_docked_controls_do() {
        use PlaybackState::*;
        let controller = || {
            let bars = (0..10)
                .map(|minute| {
                    crate::model::Bar::new(
                        chrono::DateTime::from_timestamp(1_704_067_200 + minute * 60, 0).unwrap(),
                        10.0,
                        12.0,
                        9.0,
                        11.0,
                        1.0,
                    )
                })
                .collect();
            let mut replay = ReplayController::new(bars, vec![]);
            replay.handle_action(ReplayAction::JumpToBar(4));
            replay
        };
        let harness = |replay: ReplayController| {
            Harness::builder()
                .allow_missing_glyphs()
                .with_accessibility_check(false)
                .build_ui_state(
                    |ui, replay: &mut ReplayController| {
                        let state = ReplayToolbarState::from(&*replay);
                        if let Some(action) = ReplayToolbar::new(&state).show(ui) {
                            replay.handle_action(action.into());
                        }
                    },
                    replay,
                )
        };
        let rows = [
            (vec![], "Play", (4, Playing)),
            (vec![ReplayAction::Play], "Pause", (4, Paused)),
            (vec![], "Step forward", (5, Paused)),
            (vec![], "Step backward", (3, Paused)),
            // Exit replay on the docked controls.
            (vec![], "Stop", (0, Inactive)),
            // A finished replay plays again from its first bar.
            (vec![ReplayAction::JumpToEnd], "Play", (0, Playing)),
            (vec![], "Settings", (4, Paused)),
        ];
        let expected: Vec<_> = rows.iter().map(|row| row.2).collect();
        let moved: Vec<_> = rows
            .into_iter()
            .map(|(setup, label, _)| {
                let mut replay = controller();
                for action in setup {
                    replay.handle_action(action);
                }
                let mut harness = harness(replay);
                harness.run();
                harness.get_by_label(label).click();
                harness.run();
                let state = &harness.state().state;
                (state.curr_bar, state.playback_state)
            })
            .collect();
        assert_eq!(moved, expected);

        // The speed menu offers the controller's presets, 50x among them.
        let mut harness = harness(controller());
        harness.run();
        harness.get_by_role(egui::Role::ComboBox).click();
        harness.run();
        harness.get_by_label("50x").click();
        harness.run();
        assert_eq!(harness.state().state.speed, ReplaySpeed::Turbo);

        harness.get_by_role(egui::Role::ComboBox).click();
        harness.run();
        let last_preset = harness.output().shapes.iter().find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == "100x"
            {
                Some(shape.clip_rect.contains_rect(text.visual_bounding_rect()))
            } else {
                None
            }
        });
        assert_eq!(
            last_preset,
            Some(true),
            "the last preset must be fully visible"
        );
        harness.get_by_label("100x").click();
        harness.run();
        assert_eq!(harness.state().state.speed, ReplaySpeed::Max);

        // Outside replay mode the toolbar offers no button that would do nothing.
        harness.state_mut().handle_action(ReplayAction::Stop);
        harness.run();
        for label in ["Play", "Step forward"] {
            let node = harness.get_by_label(label);
            assert!(node.accesskit_node().is_disabled(), "{label}");
        }
    }
}
