//! Replay configuration
//!
//! Visual and behavioral configuration for replay mode.

use super::actions::ReplaySpeed;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Colors for replay UI elements
#[derive(Debug, Clone, PartialEq)]
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

impl ReplayColors {
    /// Colours from a theme's UI palette.
    ///
    /// The replay widgets paint with the active theme's colours while the configured colours
    /// equal [`ReplayColors::default`]; set any other colours with [`ReplayConfig::with_colors`].
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            play_color: ui.success,
            pause_color: ui.warning,
            stop_color: ui.error,
            progress_bg: ui.btn_bg_hover,
            progress_fill: ui.accent,
            progress_cursor: ui.text,
            marker_color: ui.warning,
            speed_color: ui.text,
            active_indicator: ui.success,
            disabled_color: ui.text_muted,
        }
    }
}

impl Default for ReplayColors {
    fn default() -> Self {
        Self::from_theme(&Theme::dark())
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

impl ReplayLayout {
    pub(super) fn resolved(&self, theme: &Theme) -> Self {
        let defaults = Self::default();
        let mut layout = self.clone();
        for (value, default, themed) in [
            (
                &mut layout.btn_spacing,
                defaults.btn_spacing,
                theme.spacing.sm,
            ),
            (
                &mut layout.progress_bar_height,
                defaults.progress_bar_height,
                theme.spacing.lg,
            ),
            (&mut layout.padding, defaults.padding, theme.spacing.lg),
        ] {
            if *value == default {
                *value = themed;
            }
        }
        layout
    }
}

impl Default for ReplayLayout {
    fn default() -> Self {
        Self {
            control_bar_height: DESIGN_TOKENS.sizing.replay.control_bar_height,
            btn_size: DESIGN_TOKENS.sizing.replay.button_size,
            small_button_size: DESIGN_TOKENS.sizing.button_sm,
            btn_spacing: crate::theme::Theme::default().spacing.sm,
            progress_bar_height: crate::theme::Theme::default().spacing.lg,
            progress_bar_width: None,
            padding: crate::theme::Theme::default().spacing.lg,
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
                btn_spacing: crate::theme::Theme::default().spacing.xs,
                progress_bar_height: crate::theme::Theme::default().spacing.md,
                padding: crate::theme::Theme::default().spacing.sm,
                speed_dropdown_width: DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                    - crate::theme::Theme::default().spacing.xxxl,
                date_display_width: DESIGN_TOKENS.sizing.replay.date_display_width
                    - (crate::theme::Theme::default().spacing.xxxl
                        + crate::theme::Theme::default().spacing.xxl),
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
                btn_spacing: crate::theme::Theme::default().spacing.xs,
                progress_bar_height: crate::theme::Theme::default().spacing.sm,
                padding: crate::theme::Theme::default().spacing.sm,
                speed_dropdown_width: DESIGN_TOKENS.sizing.technical_labels.line_label_width,
                date_display_width: DESIGN_TOKENS.sizing.replay.date_display_width
                    - DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                    + crate::theme::Theme::default().spacing.xxxl
                    + crate::theme::Theme::default().spacing.xxl,
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
