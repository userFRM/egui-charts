//! Replay configuration
//!
//! Visual and behavioral configuration for replay mode.

use super::actions::ReplaySpeed;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Colors for replay UI elements
#[derive(Debug, Clone)]
pub struct ReplayColors {
    /// Play button color
    pub play_color: Color32,
    /// Pause button color
    pub pause_color: Color32,
    /// Stop button color
    pub stop_color: Color32,
    /// Progress bar background
    pub progress_bg: Color32,
    /// Progress bar filled portion
    pub progress_fill: Color32,
    /// Progress bar cursor/handle
    pub progress_cursor: Color32,
    /// Marker default color
    pub marker_color: Color32,
    /// Speed indicator color
    pub speed_color: Color32,
    /// Replay active indicator
    pub active_indicator: Color32,
    /// Disabled button color
    pub disabled_color: Color32,
}

impl Default for ReplayColors {
    fn default() -> Self {
        Self {
            play_color: DESIGN_TOKENS.semantic.extended.success,
            pause_color: DESIGN_TOKENS.semantic.extended.favorite_gold,
            stop_color: DESIGN_TOKENS.semantic.extended.error,
            progress_bg: DESIGN_TOKENS.semantic.extended.chart_axis_bg,
            progress_fill: DESIGN_TOKENS.semantic.extended.info,
            progress_cursor: Color32::WHITE,
            marker_color: DESIGN_TOKENS.semantic.extended.warning,
            speed_color: DESIGN_TOKENS.semantic.extended.purple,
            active_indicator: DESIGN_TOKENS.semantic.extended.success,
            disabled_color: DESIGN_TOKENS.semantic.extended.disabled,
        }
    }
}

/// Layout configuration for replay controls
#[derive(Debug, Clone)]
pub struct ReplayLayout {
    /// Height of the replay control bar
    pub control_bar_height: f32,
    /// Button size
    pub btn_size: f32,
    /// Small button size (for step btns)
    pub small_button_size: f32,
    /// Spacing between btns
    pub btn_spacing: f32,
    /// Progress bar height
    pub progress_bar_height: f32,
    /// Progress bar width (if fixed, None for flexible)
    pub progress_bar_width: Option<f32>,
    /// Padding inside the control bar
    pub padding: f32,
    /// Speed dropdown width
    pub speed_dropdown_width: f32,
    /// Date display width
    pub date_display_width: f32,
}

impl Default for ReplayLayout {
    fn default() -> Self {
        Self {
            control_bar_height: DESIGN_TOKENS.sizing.replay.control_bar_height,
            btn_size: DESIGN_TOKENS.sizing.replay.button_size,
            small_button_size: DESIGN_TOKENS.sizing.button_sm,
            btn_spacing: DESIGN_TOKENS.spacing.sm,
            progress_bar_height: DESIGN_TOKENS.spacing.lg,
            progress_bar_width: None,
            padding: DESIGN_TOKENS.spacing.lg,
            speed_dropdown_width: DESIGN_TOKENS.sizing.replay.speed_dropdown_width,
            date_display_width: DESIGN_TOKENS.sizing.replay.date_display_width,
        }
    }
}

/// Behavior configuration for replay
#[derive(Debug, Clone)]
pub struct ReplayBehavior {
    /// Default playback speed
    pub default_speed: ReplaySpeed,
    /// Whether to loop when reaching end
    pub loop_playback: bool,
    /// Whether to show trading simulation panel by default
    pub show_trading_panel: bool,
    /// Initial capital for trading simulation
    pub initial_capital: f64,
    /// Whether to auto-scroll chart during playback
    pub auto_scroll: bool,
    /// Number of bars to keep visible ahead of current position
    pub lookahead_bars: usize,
    /// Whether to pause on user interaction (e.g., clicking chart)
    pub pause_on_interaction: bool,
    /// Min time between bar updates (in seconds)
    pub min_bar_duration: f64,
    /// Max time between bar updates (in seconds)
    pub max_bar_duration: f64,
    /// Whether to show bar counter
    pub show_bar_counter: bool,
    /// Whether to show elapsed time
    pub show_elapsed_time: bool,
    /// Whether to show remaining time
    pub show_remaining_time: bool,
}

impl Default for ReplayBehavior {
    fn default() -> Self {
        Self {
            default_speed: ReplaySpeed::Normal,
            loop_playback: false,
            show_trading_panel: false,
            initial_capital: 10000.0,
            auto_scroll: true,
            lookahead_bars: 10,
            pause_on_interaction: false,
            min_bar_duration: 0.01,
            max_bar_duration: 10.0,
            show_bar_counter: true,
            show_elapsed_time: true,
            show_remaining_time: true,
        }
    }
}

/// Complete replay configuration
#[derive(Debug, Clone, Default)]
pub struct ReplayConfig {
    /// Color configuration
    pub colors: ReplayColors,
    /// Layout configuration
    pub layout: ReplayLayout,
    /// Behavior configuration
    pub behavior: ReplayBehavior,
}

impl ReplayConfig {
    /// Create new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set colors
    pub fn with_colors(mut self, colors: ReplayColors) -> Self {
        self.colors = colors;
        self
    }

    /// Set layout
    pub fn with_layout(mut self, layout: ReplayLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Set behavior
    pub fn with_behavior(mut self, behavior: ReplayBehavior) -> Self {
        self.behavior = behavior;
        self
    }

    /// Create a compact layout variant
    pub fn compact() -> Self {
        Self {
            layout: ReplayLayout {
                control_bar_height: DESIGN_TOKENS.sizing.icon_xxl,
                btn_size: DESIGN_TOKENS.sizing.icon_lg,
                small_button_size: DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
                btn_spacing: DESIGN_TOKENS.spacing.xs,
                progress_bar_height: DESIGN_TOKENS.spacing.md,
                padding: DESIGN_TOKENS.spacing.sm,
                speed_dropdown_width: DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                    - DESIGN_TOKENS.spacing.xxxl,
                date_display_width: DESIGN_TOKENS.sizing.replay.date_display_width
                    - DESIGN_TOKENS.spacing.section_lg,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Create a minimal layout (just play/pause and progress)
    pub fn minimal() -> Self {
        Self {
            layout: ReplayLayout {
                control_bar_height: DESIGN_TOKENS.sizing.replay.button_size,
                btn_size: DESIGN_TOKENS.sizing.button_sm,
                small_button_size: DESIGN_TOKENS.sizing.icon_md,
                btn_spacing: DESIGN_TOKENS.spacing.xs,
                progress_bar_height: DESIGN_TOKENS.spacing.sm,
                padding: DESIGN_TOKENS.spacing.sm,
                speed_dropdown_width: DESIGN_TOKENS.sizing.technical_labels.line_label_width,
                date_display_width: DESIGN_TOKENS.sizing.replay.date_display_width
                    - DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                    + DESIGN_TOKENS.spacing.section_lg,
                ..Default::default()
            },
            behavior: ReplayBehavior {
                show_bar_counter: false,
                show_elapsed_time: false,
                show_remaining_time: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = ReplayConfig::new();
        assert_eq!(
            config.layout.control_bar_height,
            DESIGN_TOKENS.sizing.replay.control_bar_height
        );
        assert!(!config.behavior.loop_playback);
    }

    #[test]
    fn test_compact_layout() {
        let config = ReplayConfig::compact();
        assert!(config.layout.control_bar_height < DESIGN_TOKENS.sizing.replay.control_bar_height);
    }

    #[test]
    fn test_minimal_layout() {
        let config = ReplayConfig::minimal();
        assert!(!config.behavior.show_bar_counter);
    }
}
