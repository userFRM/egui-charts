//! Crosshair options for the chart cursor overlay.
//!
//! Controls the crosshair mode (Normal vs Magnet-snap), visual style
//! (Full / Dot / Arrow), line style (Solid / Dashed / Dotted), and colors.

/// Crosshair Options.
///
/// Controls crosshair behavior and appearance.
use crate::theme::ThemeExt;
use egui::Color32;

/// Crosshair mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrosshairMode {
    /// Normal crosshair (follows mouse exactly)
    #[default]
    Normal,
    /// Magnet crosshair (snaps to nearest OHLC data point)
    Magnet,
}

/// Crosshair visual style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrosshairStyle {
    /// Full crosshair with vertical and horizontal lines (default)
    #[default]
    Full,
    /// Only a dot at the intersection point
    Dot,
    /// No crosshair, just standard arrow cursor
    Arrow,
}

/// Line style for crosshair rendering (Solid, Dashed, Dotted)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrosshairLineStyle {
    /// Solid line
    Solid,
    /// Dashed line
    Dashed,
    /// Dotted line (default)
    #[default]
    Dotted,
}

/// Crosshair options
#[derive(Debug, Clone, Copy)]
pub struct CrosshairOptions {
    /// Crosshair mode
    pub mode: CrosshairMode,

    /// Crosshair visual style (Full/Dot/Arrow)
    pub style: CrosshairStyle,

    /// Line style for crosshair (Solid/Dashed/Dotted)
    pub line_style: CrosshairLineStyle,

    /// Show vertical line
    pub vert_line_visible: bool,

    /// Show horizontal line
    pub horz_line_visible: bool,

    /// Vertical line color (`Color32::PLACEHOLDER`: active theme)
    pub vert_line_color: Color32,

    /// Horizontal line color (`Color32::PLACEHOLDER`: active theme)
    pub horz_line_color: Color32,

    /// Vertical line width
    pub vert_line_width: f32,

    /// Horizontal line width
    pub horz_line_width: f32,

    /// Show labels
    pub label_visible: bool,

    /// Label background color (`Color32::PLACEHOLDER`: active theme)
    pub label_background_color: Color32,

    /// Label text color (`Color32::PLACEHOLDER`: active theme)
    pub label_text_color: Color32,
}

impl Default for CrosshairOptions {
    fn default() -> Self {
        Self {
            mode: CrosshairMode::Normal,
            style: CrosshairStyle::Full,
            line_style: CrosshairLineStyle::Dotted,
            vert_line_visible: true,
            horz_line_visible: true,
            vert_line_color: Color32::PLACEHOLDER,
            horz_line_color: Color32::PLACEHOLDER,
            vert_line_width: 1.0,
            horz_line_width: 1.0,
            label_visible: true,
            label_background_color: Color32::BLACK,
            label_text_color: Color32::WHITE,
        }
    }
}

impl CrosshairOptions {
    /// These options with `Color32::PLACEHOLDER` colours taken from the theme applied to `ctx`.
    pub(crate) fn resolved(&self, ctx: &egui::Context) -> Self {
        let chart = ctx.active_theme().chart;
        let or = |color: Color32, themed: Color32| {
            if color == Color32::PLACEHOLDER {
                themed
            } else {
                color
            }
        };
        Self {
            vert_line_color: or(self.vert_line_color, chart.crosshair_line),
            horz_line_color: or(self.horz_line_color, chart.crosshair_line),
            label_background_color: or(self.label_background_color, chart.crosshair_label_bg),
            label_text_color: or(self.label_text_color, chart.crosshair_label_text),
            ..*self
        }
    }
}
