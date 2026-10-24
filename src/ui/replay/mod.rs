//! Replay/playback UI module
//!
//! Historical data replay system for stepping through market data bar-by-bar.
//! This module provides:
//!
//! - Play/pause/step controls for navigating historical data
//! - Variable playback speeds (0.1x to 100x)
//! - Progress bar with seek functionality
//! - Markers for annotating specific bars
//! - Trading simulation integration
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ReplayController                         │
//! │  (Orchestrates replay state, data feeding, and events)      │
//! └─────────────────────────┬───────────────────────────────────┘
//!                           │
//!          ┌────────────────┼────────────────┐
//!          ▼                ▼                ▼
//! ┌────────────────┐ ┌─────────────┐ ┌──────────────────┐
//! │  ReplayState   │ │ ReplayConfig│ │ ReplayControls   │
//! │ (Current pos,  │ │ (Colors,    │ │ (UI widget for   │
//! │  playback,     │ │  layout,    │ │  play/pause/     │
//! │  markers)      │ │  behavior)  │ │  progress)       │
//! └────────────────┘ └─────────────┘ └──────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..100).map(|i| { let p = 100.0 + (i as f64 * 0.2).sin() * 5.0; egui_charts::model::Bar::new(chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::ui::replay::{ReplayController, ReplayControls};
//!
//! // Keep the full history in the chart and drive its window with the source cursor.
//! use egui_charts::{Chart, model::{BarData, BarClock, Timeframe}};
//! let mut chart = Chart::new(BarData::from_bars(bars.clone()));
//! chart.set_timeframe(Timeframe::Min1);
//! // Create replay controller with data
//! # let ts = bars.iter().map(|bar| bar.time).collect();
//! let mut controller = ReplayController::new(bars, ts);
//!
//! // In your UI loop:
//! # egui::__run_test_ui(|ui| {
//! egui::Panel::bottom("replay").show(ui, |ui| {
//!     let controls = ReplayControls::new(&controller.config);
//!     let action = controls.show(ui, &mut controller.state);
//!     controller.handle_action(action);
//! });
//! # });
//!
//! chart.set_replay_time(controller.cutoff(&BarClock::new(Timeframe::Min1)));
//! // Supply finer history with chart.set_replay_source when it is available.
//! // Store controller.session() in host storage and restore it after fetching source history.
//! ```

mod actions;
mod config;
mod controls;
mod session;
mod start;
mod state;
mod toolbar;

pub use actions::{ReplayAction, ReplaySpeed};
pub use config::{ReplayBehavior, ReplayColors, ReplayConfig, ReplayLayout};
pub use controls::{CompactReplayControls, ReplayControls, ReplayKeyboardHandler};
pub use session::ReplaySession;
pub use start::{ReplayStartAction, ReplayStartPicker};
pub use state::{PlaybackState, ReplayMarker, ReplayState, TradingSimulationState};
pub use toolbar::{ReplayToolbar, ReplayToolbarAction, ReplayToolbarConfig, ReplayToolbarState};

use crate::model::Bar;
use chrono::{DateTime, Utc};

/// Replay controller
///
/// Orchestrates the replay system, managing state, data feeding, and action handling.
/// This is the main entry point for integrating replay functionality into your application.
#[derive(Default)]
pub struct ReplayController {
    /// Replay state
    pub state: ReplayState,
    /// Replay configuration
    pub config: ReplayConfig,
    /// Historical bar data
    bars: Vec<Bar>,
    /// Bar ts
    ts: Vec<DateTime<Utc>>,
}

impl ReplayController {
    /// Create a new replay controller with bar data. Timestamps are read from the bars;
    /// the separate timestamp argument is retained for source compatibility.
    pub fn new(bars: Vec<Bar>, _ts: Vec<DateTime<Utc>>) -> Self {
        let ts: Vec<_> = bars.iter().map(|bar| bar.time).collect();
        let mut state = ReplayState::new();

        // Initialize state if we have data
        if !bars.is_empty() && !ts.is_empty() {
            let start = ts.first().copied().unwrap_or_else(Utc::now);
            let end = ts.last().copied().unwrap_or_else(Utc::now);
            state.init(bars.len(), start, end, String::new());
        }

        Self {
            state,
            config: ReplayConfig::default(),
            bars,
            ts,
        }
    }

    /// Create with configuration
    pub fn with_config(mut self, config: ReplayConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the symbol name
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.state.symbol = Some(symbol.into());
        self
    }

    /// Load new bar data
    ///
    /// During an active replay the position moves to the last bar that opens at or before the
    /// current replay time and playback carries on, so a change of interval continues the replay
    /// where it was. On a coarser interval the bar that contains the replay time is shown whole; a
    /// host that wants it partial passes that bar built from the finer bars. Otherwise the replay
    /// starts paused on the first bar.
    pub fn load_data(&mut self, bars: Vec<Bar>, _ts: Vec<DateTime<Utc>>, symbol: String) {
        let resume = self.state.is_active().then(|| {
            (
                self.ts.get(self.state.curr_bar).copied(),
                self.state.is_playing(),
            )
        });
        self.bars = bars;
        self.ts = self.bars.iter().map(|bar| bar.time).collect();

        if !self.bars.is_empty() && !self.ts.is_empty() {
            let start = self.ts.first().copied().unwrap_or_else(Utc::now);
            let end = self.ts.last().copied().unwrap_or_else(Utc::now);
            self.state.init(self.bars.len(), start, end, symbol);
            if let Some((Some(time), playing)) = resume {
                let bar = self.ts.partition_point(|&t| t <= time).saturating_sub(1);
                self.state.jump_to_bar(bar);
                self.update_curr_date();
                if playing {
                    self.state.play();
                }
            }
        } else {
            self.state.stop();
            self.state.total_bars = 0;
            self.state.curr_date = None;
            self.state.start_date = None;
            self.state.end_date = None;
        }
    }

    /// Unload current data
    pub fn unload_data(&mut self) {
        self.bars.clear();
        self.ts.clear();
        self.state.stop();
        self.state.total_bars = 0;
    }

    /// Check if data is loaded
    pub fn has_data(&self) -> bool {
        !self.bars.is_empty()
    }

    /// Get the current bar
    pub fn curr_bar(&self) -> Option<&Bar> {
        self.bars.get(self.state.curr_bar)
    }

    /// Get the current ts
    pub fn curr_ts(&self) -> Option<&DateTime<Utc>> {
        self.ts.get(self.state.curr_bar)
    }

    /// Get visible bars up to current position
    pub fn visible_bars(&self) -> &[Bar] {
        let end = (self.state.curr_bar + 1).min(self.bars.len());
        &self.bars[..end]
    }

    /// Get visible ts up to current position
    pub fn visible_ts(&self) -> &[DateTime<Utc>] {
        let end = (self.state.curr_bar + 1).min(self.ts.len());
        &self.ts[..end]
    }

    /// Get bars in a window around current position
    pub fn bars_in_window(&self, window_size: usize) -> &[Bar] {
        let end = (self.state.curr_bar + 1).min(self.bars.len());
        let start = end.saturating_sub(window_size);
        &self.bars[start..end]
    }

    /// Handle a replay action
    pub fn handle_action(&mut self, action: ReplayAction) {
        match action {
            ReplayAction::None => {}

            // Playback controls
            ReplayAction::Play => self.state.play(),
            ReplayAction::Pause => self.state.pause(),
            ReplayAction::TogglePlayPause => self.state.toggle_play_pause(),
            ReplayAction::Stop => self.state.stop(),
            ReplayAction::Reset => self.state.reset(),

            // Navigation
            ReplayAction::StepForward => {
                self.state.pause();
                self.state.step_forward();
                self.update_curr_date();
            }
            ReplayAction::StepBackward => {
                self.state.pause();
                self.state.step_backward();
                self.update_curr_date();
            }
            ReplayAction::StepForwardN(n) => {
                self.state.pause();
                for _ in 0..n {
                    if !self.state.at_end() {
                        self.state.step_forward();
                    }
                }
                self.update_curr_date();
            }
            ReplayAction::StepBackwardN(n) => {
                self.state.pause();
                for _ in 0..n {
                    if !self.state.at_start() {
                        self.state.step_backward();
                    }
                }
                self.update_curr_date();
            }
            ReplayAction::JumpToBar(bar) => {
                self.state.jump_to_bar(bar);
                self.update_curr_date();
            }
            ReplayAction::JumpToDate(date) => {
                // Find the nearest bar to the given date
                if let Some(idx) = self.find_bar_at_date(&date) {
                    self.state.jump_to_bar(idx);
                    self.update_curr_date();
                }
            }
            ReplayAction::JumpToStart => {
                self.state.jump_to_bar(0);
                self.update_curr_date();
            }
            ReplayAction::JumpToEnd => {
                self.state.jump_to_bar(self.bars.len().saturating_sub(1));
                self.update_curr_date();
            }
            ReplayAction::JumpToPercent(pct) => {
                self.state.jump_to_percent(pct);
                self.update_curr_date();
            }

            // Speed control
            ReplayAction::SetSpeed(speed) => self.state.set_speed(ReplaySpeed::from(speed)),
            ReplayAction::SpeedUp => self.state.speed_up(),
            ReplayAction::SlowDown => self.state.slow_down(),

            // Mode control
            ReplayAction::EnterReplayMode => self.state.enter_replay_mode(),
            ReplayAction::ExitReplayMode => self.state.exit_replay_mode(),
            ReplayAction::ToggleReplayMode => {
                if self.state.is_active() {
                    self.state.exit_replay_mode();
                } else {
                    self.state.enter_replay_mode();
                }
            }

            // Data loading (handled externally, this just updates state)
            ReplayAction::LoadData { symbol, start, end } => {
                // Data loading would be handled by the application
                // This action signals intent to load data
                if let (Some(s), Some(e)) = (start, end) {
                    self.state.start_date = Some(s);
                    self.state.end_date = Some(e);
                }
                self.state.symbol = Some(symbol);
            }
            ReplayAction::UnloadData => {
                self.unload_data();
            }

            // Range selection
            ReplayAction::SetStartDate(date) => {
                self.state.start_date = Some(date);
            }
            ReplayAction::SetEndDate(date) => {
                self.state.end_date = Some(date);
            }
            ReplayAction::SetDateRange { start, end } => {
                self.state.start_date = Some(start);
                self.state.end_date = Some(end);
            }

            // Markers
            ReplayAction::AddMarker { label, color } => {
                self.state.add_marker(label, color);
            }
            ReplayAction::RemoveMarker(bar_idx) => {
                self.state.remove_marker(bar_idx);
            }
            ReplayAction::ClearMarkers => {
                self.state.clear_markers();
            }

            // Trading simulation
            ReplayAction::EnableTradingSimulation => {
                self.state.trading_sim.enabled = true;
                self.state.show_trading_panel = true;
            }
            ReplayAction::DisableTradingSimulation => {
                self.state.trading_sim.enabled = false;
            }
            ReplayAction::ResetTradingSimulation => {
                self.state
                    .trading_sim
                    .reset(self.config.behavior.initial_capital);
            }
        }
    }

    /// Update state each frame (call in your render loop)
    pub fn update(&mut self) -> usize {
        let bars_advanced = self.state.update();
        if bars_advanced > 0 {
            self.update_curr_date();
        }
        bars_advanced
    }

    /// Find the bar index closest to the given date
    fn find_bar_at_date(&self, target: &DateTime<Utc>) -> Option<usize> {
        if self.ts.is_empty() {
            return None;
        }

        // Binary search for the closest ts
        let result = self.ts.binary_search(target);
        match result {
            Ok(idx) => Some(idx),
            Err(idx) => {
                if idx == 0 {
                    Some(0)
                } else if idx >= self.ts.len() {
                    Some(self.ts.len() - 1)
                } else {
                    // Compare distances to adjacent ts
                    let before_diff = *target - self.ts[idx - 1];
                    let after_diff = self.ts[idx] - *target;
                    if before_diff < after_diff {
                        Some(idx - 1)
                    } else {
                        Some(idx)
                    }
                }
            }
        }
    }

    /// Update current date from bar ts
    fn update_curr_date(&mut self) {
        self.state.update_curr_date(&self.ts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_bars(count: usize) -> (Vec<Bar>, Vec<DateTime<Utc>>) {
        let mut bars = Vec::with_capacity(count);
        let mut timestamps = Vec::with_capacity(count);

        for i in 0..count {
            // Use different days to avoid hour overflow (hours 0-23 only)
            let day = (i / 24) as u32 + 1;
            let hour = (i % 24) as u32;
            let ts = Utc
                .with_ymd_and_hms(2024, 1, day.min(28), hour, 0, 0)
                .unwrap();
            bars.push(Bar {
                time: ts,
                open: 100.0 + i as f64,
                high: 101.0 + i as f64,
                low: 99.0 + i as f64,
                close: 100.5 + i as f64,
                volume: 1000.0,
            });
            timestamps.push(ts);
        }

        (bars, timestamps)
    }

    #[test]
    fn test_controller_creation() {
        let (bars, ts) = create_test_bars(100);
        let controller = ReplayController::new(bars, ts);

        assert!(controller.has_data());
        assert_eq!(controller.state.total_bars, 100);
    }

    #[test]
    fn test_navigation() {
        let (bars, ts) = create_test_bars(100);
        let mut controller = ReplayController::new(bars, ts);

        controller.handle_action(ReplayAction::EnterReplayMode);
        assert!(controller.state.is_active());

        controller.handle_action(ReplayAction::JumpToBar(50));
        assert_eq!(controller.state.curr_bar, 50);

        controller.handle_action(ReplayAction::StepForward);
        assert_eq!(controller.state.curr_bar, 51);

        controller.handle_action(ReplayAction::StepBackward);
        assert_eq!(controller.state.curr_bar, 50);
    }

    fn play_to_end(controller: &mut ReplayController) {
        controller.state.set_bar_duration(0.001);
        controller.handle_action(ReplayAction::SetSpeed(100.0));
        controller.handle_action(ReplayAction::Play);
        while controller.state.is_playing() {
            controller.update();
        }
    }

    #[test]
    fn playback_state_follows_seeks_and_steps() {
        use ReplayAction::*;
        type Setup = fn(&mut ReplayController);
        let rows: [(Setup, Vec<ReplayAction>, (usize, PlaybackState)); 6] = [
            (
                play_to_end,
                vec![JumpToPercent(0.3), TogglePlayPause],
                (29, PlaybackState::Playing),
            ),
            (play_to_end, vec![Reset], (0, PlaybackState::Paused)),
            (|_| {}, vec![JumpToEnd], (99, PlaybackState::Finished)),
            (
                |_| {},
                vec![JumpToPercent(1.0)],
                (99, PlaybackState::Finished),
            ),
            (
                |_| {},
                vec![StepForwardN(10)],
                (99, PlaybackState::Finished),
            ),
            (
                |_| {},
                vec![JumpToBar(98), StepForward],
                (99, PlaybackState::Finished),
            ),
        ];
        for (index, (setup, actions, expected)) in rows.into_iter().enumerate() {
            let (bars, ts) = create_test_bars(100);
            let mut controller = ReplayController::new(bars, ts);
            controller.handle_action(JumpToBar(95));
            setup(&mut controller);
            for action in actions {
                controller.handle_action(action);
            }
            let state = &controller.state;
            assert_eq!(
                (state.curr_bar, state.playback_state),
                expected,
                "row {index}"
            );
        }
    }

    #[test]
    fn interval_change_keeps_the_replay_time() {
        type Row = (
            fn(&mut ReplayController),
            (usize, PlaybackState, DateTime<Utc>),
        );
        let days: Vec<_> = (1..=3)
            .map(|day| Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap())
            .collect();
        let rows: [Row; 2] = [
            // Hourly bar 30 opens 2024-01-02 06:00, inside the second daily bar.
            (
                |c| {
                    c.handle_action(ReplayAction::JumpToBar(30));
                    c.handle_action(ReplayAction::Play);
                },
                (1, PlaybackState::Playing, days[1]),
            ),
            // Toggling play after the end restarts on hourly bar 0, inside the first daily bar.
            (
                |c| {
                    play_to_end(c);
                    c.handle_action(ReplayAction::TogglePlayPause);
                },
                (0, PlaybackState::Playing, days[0]),
            ),
        ];
        for (index, (setup, (bar, playback, date))) in rows.into_iter().enumerate() {
            let (bars, ts) = create_test_bars(48);
            let mut controller = ReplayController::new(bars, ts);
            setup(&mut controller);
            let daily = days
                .iter()
                .map(|&time| Bar::new(time, 100.0, 101.0, 99.0, 100.5, 1000.0))
                .collect();
            controller.load_data(daily, days.clone(), String::new());

            let state = &controller.state;
            assert_eq!(
                (state.curr_bar, state.playback_state, state.curr_date),
                (bar, playback, Some(date)),
                "row {index}"
            );
        }
    }

    #[test]
    fn test_visible_bars() {
        let (bars, ts) = create_test_bars(100);
        let mut controller = ReplayController::new(bars, ts);

        controller.state.curr_bar = 25;
        let visible = controller.visible_bars();
        assert_eq!(visible.len(), 26); // 0 through 25 inclusive
    }

    #[test]
    fn test_find_bar_at_date() {
        let (bars, ts) = create_test_bars(24);
        let controller = ReplayController::new(bars, ts);

        let target = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let idx = controller.find_bar_at_date(&target);
        assert_eq!(idx, Some(12));
    }
}
