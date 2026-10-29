//! Crosshair options for the chart cursor overlay.
//!
//! Controls the crosshair mode (free, or snapping to values), visual style
//! (Full / Dot / Arrow / Hidden), the style of each line, and colors.

use crate::model::LineStyle;
use crate::theme::ThemeExt;
use egui::Color32;

/// Crosshair mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum CrosshairMode {
    /// Normal crosshair (snaps x to the bar center and follows the pointer value)
    #[default]
    Normal,
    /// Magnet crosshair (snaps to the nearest visible OHLC, study or comparison value)
    Magnet,
    /// Magnet crosshair that snaps the price pane to the bar's close only. Volume and study
    /// panes snap to their values as with `Magnet`.
    MagnetClose,
}

impl CrosshairMode {
    /// Whether the crosshair snaps to values instead of following the pointer.
    pub(crate) fn snaps(self) -> bool {
        matches!(self, Self::Magnet | Self::MagnetClose)
    }
}

/// Crosshair visual style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum CrosshairStyle {
    /// Full crosshair with vertical and horizontal lines (default)
    #[default]
    Full,
    /// Only a dot at the intersection point
    Dot,
    /// Standard arrow cursor and axis readouts, without crosshair lines
    Arrow,
    /// A fading pointer trail for presentations, with axis readouts.
    Demonstration,
    /// A trail that follows the pointer while the primary button is held and fades after it is
    /// released, with axis readouts. It creates no drawings.
    Magic,
    /// No crosshair lines or axis labels. Legends, the data window and tooltips still follow
    /// the pointer.
    Hidden,
}

/// Crosshair options
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CrosshairOptions {
    /// Crosshair mode
    pub mode: CrosshairMode,

    /// Crosshair visual style (Full/Dot/Arrow/Hidden)
    pub style: CrosshairStyle,

    /// Vertical line style
    pub vert_line_style: LineStyle,

    /// Horizontal line style
    pub horz_line_style: LineStyle,

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

    /// Background of the vertical line's label on the time axis (`Color32::PLACEHOLDER`:
    /// active theme)
    pub vert_label_background_color: Color32,

    /// Background of the horizontal line's label on the price axis (`Color32::PLACEHOLDER`:
    /// active theme)
    pub horz_label_background_color: Color32,

    /// Label text color (`Color32::PLACEHOLDER`: active theme)
    pub label_text_color: Color32,
}

impl Default for CrosshairOptions {
    fn default() -> Self {
        Self {
            mode: CrosshairMode::Normal,
            style: CrosshairStyle::Full,
            vert_line_style: LineStyle::Dotted,
            horz_line_style: LineStyle::Dotted,
            vert_line_visible: true,
            horz_line_visible: true,
            vert_line_color: Color32::PLACEHOLDER,
            horz_line_color: Color32::PLACEHOLDER,
            vert_line_width: 1.0,
            horz_line_width: 1.0,
            label_visible: true,
            vert_label_background_color: Color32::PLACEHOLDER,
            horz_label_background_color: Color32::PLACEHOLDER,
            label_text_color: Color32::PLACEHOLDER,
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
            vert_label_background_color: or(
                self.vert_label_background_color,
                chart.crosshair_label_bg,
            ),
            horz_label_background_color: or(
                self.horz_label_background_color,
                chart.crosshair_label_bg,
            ),
            label_text_color: or(self.label_text_color, chart.crosshair_label_text),
            ..*self
        }
    }
}
