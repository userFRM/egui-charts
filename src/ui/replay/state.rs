//! Replay state
//!
//! Mutable state for replay mode UI and playback control.

use super::actions::ReplaySpeed;
use chrono::{DateTime, Utc};
use web_time::Instant;

/// Replay marker for annotating specific bars
#[derive(Debug, Clone)]
pub struct ReplayMarker {
    /// Bar index where marker is placed
    pub bar_idx: usize,
    /// Optional label text
    pub label: Option<String>,
    /// Marker color (RGBA)
    pub color: [u8; 4],
    /// Ts when marker was created
    pub created_at: DateTime<Utc>,
}

impl ReplayMarker {
    /// Create a new marker at the given bar index
    pub fn new(bar_idx: usize) -> Self {
        Self {
            bar_idx,
            label: None,
            color: [255, 165, 0, 255], // Orange default
            created_at: Utc::now(),
        }
    }

    /// Set the label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the color
    pub fn with_color(mut self, color: [u8; 4]) -> Self {
        self.color = color;
        self
    }
}

/// Playback state for the replay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    /// Not in replay mode
    #[default]
    Inactive,
    /// In replay mode but paused
    Paused,
    /// Currently playing
    Playing,
    /// Reached end of data
    Finished,
}

impl PlaybackState {
    /// Check if replay mode is active
    pub fn is_active(&self) -> bool {
        !matches!(self, PlaybackState::Inactive)
    }

    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        matches!(self, PlaybackState::Playing)
    }

    /// Check if paused (active but not playing)
    pub fn is_paused(&self) -> bool {
        matches!(self, PlaybackState::Paused)
    }
}

/// Trading simulation state during replay
#[derive(Debug, Clone, Default)]
pub struct TradingSimulationState {
    /// Whether trading simulation is enabled
    pub enabled: bool,
    /// Simulated cash balance
    pub cash: f64,
    /// Simulated position quantity
    pub pos_qty: f64,
    /// Simulated entry price
    pub entry_price: Option<f64>,
    /// Total simulated P&L
    pub total_pnl: f64,
    /// Number of trades executed
    pub trade_cnt: u32,
    /// Win rate percentage
    pub win_rate: f64,
}

impl TradingSimulationState {
    /// Create with initial capital
    pub fn new(initial_capital: f64) -> Self {
        Self {
            enabled: false,
            cash: initial_capital,
            pos_qty: 0.0,
            entry_price: None,
            total_pnl: 0.0,
            trade_cnt: 0,
            win_rate: 0.0,
        }
    }

    /// Reset to initial state
    pub fn reset(&mut self, initial_capital: f64) {
        self.cash = initial_capital;
        self.pos_qty = 0.0;
        self.entry_price = None;
        self.total_pnl = 0.0;
        self.trade_cnt = 0;
        self.win_rate = 0.0;
    }

    /// Calculate current equity given current price
    pub fn equity(&self, curr_price: f64) -> f64 {
        self.cash + (self.pos_qty * curr_price)
    }

    /// Calculate unrealized P&L
    pub fn unrealized_pnl(&self, curr_price: f64) -> f64 {
        if let Some(entry) = self.entry_price {
            (curr_price - entry) * self.pos_qty
        } else {
            0.0
        }
    }
}

/// Main replay state
#[derive(Debug, Clone)]
pub struct ReplayState {
    // Playback state
    /// Current playback state
    pub playback_state: PlaybackState,
    /// Current playback speed
    pub speed: ReplaySpeed,

    // Pos tracking
    /// Current bar index (0-based)
    pub curr_bar: usize,
    /// Total number of bars available
    pub total_bars: usize,
    /// Visible bar count (how many bars to show on chart)
    pub visible_bars: usize,

    // Time tracking
    /// Start date of replay data
    pub start_date: Option<DateTime<Utc>>,
    /// End date of replay data
    pub end_date: Option<DateTime<Utc>>,
    /// Current date/time at playback position
    pub curr_date: Option<DateTime<Utc>>,

    // Symbol info
    /// Symbol being replayed
    pub symbol: Option<String>,

    // UI state
    /// Whether speed dropdown is open
    pub speed_dropdown_open: bool,
    /// Whether date picker is open
    pub date_picker_open: bool,
    /// Whether settings panel is open
    pub settings_open: bool,
    /// Whether to show trading simulation panel
    pub show_trading_panel: bool,

    // Markers
    /// User-placed markers
    pub markers: Vec<ReplayMarker>,

    // Trading simulation
    /// Trading simulation state
    pub trading_sim: TradingSimulationState,

    // Internal timing
    /// Last update instant (for frame timing)
    last_update: Option<Instant>,
    /// Accumulated time since last bar advance
    accumulated_time: f64,
    /// Target time per bar in seconds (based on timeframe)
    bar_duration_secs: f64,
}

impl ReplayState {
    /// Create new replay state
    pub fn new() -> Self {
        Self {
            playback_state: PlaybackState::Inactive,
            speed: ReplaySpeed::Normal,
            curr_bar: 0,
            total_bars: 0,
            visible_bars: 100,
            start_date: None,
            end_date: None,
            curr_date: None,
            symbol: None,
            speed_dropdown_open: false,
            date_picker_open: false,
            settings_open: false,
            show_trading_panel: false,
            markers: Vec::new(),
            trading_sim: TradingSimulationState::default(),
            last_update: None,
            accumulated_time: 0.0,
            bar_duration_secs: 1.0, // Default 1 second per bar at 1x
        }
    }

    /// Initialize replay with data params
    pub fn init(
        &mut self,
        total_bars: usize,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        symbol: String,
    ) {
        self.total_bars = total_bars;
        self.start_date = Some(start_date);
        self.end_date = Some(end_date);
        self.curr_date = Some(start_date);
        self.symbol = Some(symbol);
        self.curr_bar = 0;
        self.playback_state = PlaybackState::Paused;
        self.markers.clear();
    }

    /// Set the bar duration (time between bars at 1x speed)
    pub fn set_bar_duration(&mut self, seconds: f64) {
        self.bar_duration_secs = seconds.max(0.001);
    }

    /// Check if replay mode is active
    pub fn is_active(&self) -> bool {
        self.playback_state.is_active()
    }

    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        self.playback_state.is_playing()
    }

    /// Get progress as percentage (0.0 - 1.0)
    pub fn progress(&self) -> f32 {
        if self.total_bars == 0 {
            0.0
        } else {
            self.curr_bar as f32 / (self.total_bars - 1).max(1) as f32
        }
    }

    /// Get remaining bars
    pub fn remaining_bars(&self) -> usize {
        self.total_bars.saturating_sub(self.curr_bar + 1)
    }

    /// Check if at the end
    pub fn at_end(&self) -> bool {
        self.curr_bar >= self.total_bars.saturating_sub(1)
    }

    /// Check if at the beginning
    pub fn at_start(&self) -> bool {
        self.curr_bar == 0
    }

    // Playback control methods

    /// Start playing
    pub fn play(&mut self) {
        if self.total_bars > 0 && !self.at_end() {
            self.playback_state = PlaybackState::Playing;
            self.last_update = Some(Instant::now());
            self.accumulated_time = 0.0;
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        if self.playback_state.is_playing() {
            self.playback_state = PlaybackState::Paused;
            self.last_update = None;
        }
    }

    /// Toggle play/pause
    pub fn toggle_play_pause(&mut self) {
        match self.playback_state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.play(),
            PlaybackState::Finished => {
                // Restart from beginning
                self.curr_bar = 0;
                self.play();
            }
            PlaybackState::Inactive => {}
        }
    }

    /// Stop and exit replay mode
    pub fn stop(&mut self) {
        self.playback_state = PlaybackState::Inactive;
        self.curr_bar = 0;
        self.last_update = None;
        self.accumulated_time = 0.0;
    }

    /// Reset to beginning without exiting replay mode
    pub fn reset(&mut self) {
        self.pause();
        self.curr_bar = 0;
        self.curr_date = self.start_date;
        self.accumulated_time = 0.0;
    }

    /// Enter replay mode
    pub fn enter_replay_mode(&mut self) {
        if self.total_bars > 0 {
            self.playback_state = PlaybackState::Paused;
            self.curr_bar = 0;
            self.curr_date = self.start_date;
        }
    }

    /// Exit replay mode
    pub fn exit_replay_mode(&mut self) {
        self.stop();
    }

    // Navigation methods

    /// Step forward one bar
    pub fn step_forward(&mut self) {
        if self.curr_bar < self.total_bars.saturating_sub(1) {
            self.curr_bar += 1;
        } else {
            self.playback_state = PlaybackState::Finished;
        }
    }

    /// Step backward one bar
    pub fn step_backward(&mut self) {
        if self.curr_bar > 0 {
            self.curr_bar -= 1;
            // If we were finished, go back to paused
            if self.playback_state == PlaybackState::Finished {
                self.playback_state = PlaybackState::Paused;
            }
        }
    }

    /// Jump to specific bar
    pub fn jump_to_bar(&mut self, bar: usize) {
        self.curr_bar = bar.min(self.total_bars.saturating_sub(1));
        if self.at_end() && self.playback_state == PlaybackState::Playing {
            self.playback_state = PlaybackState::Finished;
        }
    }

    /// Jump to percentage position
    pub fn jump_to_percent(&mut self, percent: f32) {
        let bar = ((self.total_bars.saturating_sub(1)) as f32 * percent.clamp(0.0, 1.0)) as usize;
        self.jump_to_bar(bar);
    }

    // Speed control

    /// Set playback speed
    pub fn set_speed(&mut self, speed: ReplaySpeed) {
        self.speed = speed;
    }

    /// Increase speed
    pub fn speed_up(&mut self) {
        self.speed = self.speed.faster();
    }

    /// Decrease speed
    pub fn slow_down(&mut self) {
        self.speed = self.speed.slower();
    }

    // Marker management

    /// Add a marker at current position
    pub fn add_marker(&mut self, label: Option<String>, color: Option<[u8; 4]>) {
        let mut marker = ReplayMarker::new(self.curr_bar);
        if let Some(l) = label {
            marker = marker.with_label(l);
        }
        if let Some(c) = color {
            marker = marker.with_color(c);
        }
        self.markers.push(marker);
    }

    /// Remove marker at bar index
    pub fn remove_marker(&mut self, bar_idx: usize) {
        self.markers.retain(|m| m.bar_idx != bar_idx);
    }

    /// Clear all markers
    pub fn clear_markers(&mut self) {
        self.markers.clear();
    }

    /// Get markers in visible range
    pub fn visible_markers(&self, start_bar: usize, end_bar: usize) -> Vec<&ReplayMarker> {
        self.markers
            .iter()
            .filter(|m| m.bar_idx >= start_bar && m.bar_idx <= end_bar)
            .collect()
    }

    // Update method for frame-based playback

    /// Update playback state. Call this each frame.
    /// Returns the number of bars to advance (usually 0 or 1).
    pub fn update(&mut self) -> usize {
        if !self.playback_state.is_playing() {
            return 0;
        }

        let now = Instant::now();
        let delta = if let Some(last) = self.last_update {
            now.duration_since(last).as_secs_f64()
        } else {
            0.0
        };
        self.last_update = Some(now);

        // Accumulate time adjusted by speed
        self.accumulated_time += delta * self.speed.multiplier() as f64;

        // Calculate how many bars to advance
        let bars_to_advance = (self.accumulated_time / self.bar_duration_secs) as usize;

        if bars_to_advance > 0 {
            self.accumulated_time -= bars_to_advance as f64 * self.bar_duration_secs;

            // Advance bars
            let new_bar = self.curr_bar + bars_to_advance;
            if new_bar >= self.total_bars.saturating_sub(1) {
                self.curr_bar = self.total_bars.saturating_sub(1);
                self.playback_state = PlaybackState::Finished;
                return self.total_bars.saturating_sub(1) - (self.curr_bar - bars_to_advance);
            } else {
                self.curr_bar = new_bar;
                return bars_to_advance;
            }
        }

        0
    }

    /// Update current date based on bar index
    pub fn update_curr_date(&mut self, bar_ts: &[DateTime<Utc>]) {
        if let Some(ts) = bar_ts.get(self.curr_bar) {
            self.curr_date = Some(*ts);
        }
    }

    /// Sync replay state from external values.
    ///
    /// `active` / `playing` drive the playback state machine,
    /// `current_bar` / `total_bars` sync position, and `speed` sets playback rate.
    pub fn sync_from(
        &mut self,
        active: bool,
        playing: bool,
        current_bar: usize,
        total_bars: usize,
        speed: ReplaySpeed,
    ) {
        // Update playback state based on central state
        if active {
            if playing {
                if self.playback_state != PlaybackState::Playing {
                    self.playback_state = PlaybackState::Playing;
                    self.last_update = Some(Instant::now());
                }
            } else if self.playback_state == PlaybackState::Playing {
                self.playback_state = PlaybackState::Paused;
            }
        } else if self.playback_state != PlaybackState::Inactive {
            self.playback_state = PlaybackState::Inactive;
        }

        // Sync position
        self.curr_bar = current_bar;
        self.total_bars = total_bars;

        // Sync speed
        self.speed = speed;
    }
}

impl Default for ReplayState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_state_creation() {
        let state = ReplayState::new();
        assert!(!state.is_active());
        assert!(!state.is_playing());
        assert_eq!(state.curr_bar, 0);
    }

    #[test]
    fn test_replay_progress() {
        let mut state = ReplayState::new();
        state.total_bars = 100;
        state.curr_bar = 50;
        assert!((state.progress() - 0.505).abs() < 0.01);
    }

    #[test]
    fn test_navigation() {
        let mut state = ReplayState::new();
        state.total_bars = 10;
        state.playback_state = PlaybackState::Paused;

        state.step_forward();
        assert_eq!(state.curr_bar, 1);

        state.jump_to_bar(5);
        assert_eq!(state.curr_bar, 5);

        state.step_backward();
        assert_eq!(state.curr_bar, 4);

        state.jump_to_percent(1.0);
        assert_eq!(state.curr_bar, 9);
    }

    #[test]
    fn test_markers() {
        let mut state = ReplayState::new();
        state.curr_bar = 5;

        state.add_marker(Some("Test".to_string()), None);
        assert_eq!(state.markers.len(), 1);
        assert_eq!(state.markers[0].bar_idx, 5);

        state.remove_marker(5);
        assert!(state.markers.is_empty());
    }
}
