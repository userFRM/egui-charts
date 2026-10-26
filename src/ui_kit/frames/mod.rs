//! Styled Panel Frames
//!
//! Builder pattern for creating panel frames with consistent gaps and selective corner rounding.
//!
//! Uses a "gap-based" layout where panels are separated by a background gap color,
//! and inner corners are rounded where panels meet.
//!
//! # Example
//! ```
//! # use egui_charts::ui_kit::frames::StyledPanelFrame;
//! # egui::__run_test_ui(|ui| {
//! StyledPanelFrame::left(ui).show(ui, |ui| {
//!     // Panel content
//! });
//! # });
//! ```

use egui::{Color32, CornerRadius, Frame, InnerResponse, Margin, Ui};

use crate::theme::ThemeExt;

/// Panel position in the layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPosition {
    /// Left sidebar (drawing toolbar)
    Left,
    /// Right widget bar (icons)
    RightWidgetBar,
    /// Right content panel (alerts, object tree, etc.)
    RightContent,
    /// Top panel (toolbar, symbol header)
    Top,
    /// Bottom panel (timeline, status)
    Bottom,
    /// Central chart area
    Chart,
}

/// Styled panel frame builder
///
/// Creates consistent outer (gap) and inner (content) frames for panels.
/// Handles selective corner rounding patterns where panels meet.
///
/// # Example
/// ```
/// # use egui_charts::ui_kit::frames::StyledPanelFrame;
/// # egui::__run_test_ui(|ui| {
/// // Method 1: Show both frames with content
/// egui::CentralPanel::default().show(ui, |ui| {
///     StyledPanelFrame::left(ui).show(ui, |ui| {
///         ui.label("Panel content");
///     });
/// });
///
/// // Method 2: Get frames separately (for a side panel)
/// let frame = StyledPanelFrame::left(ui);
/// egui::Panel::left("left")
///     .frame(frame.outer_frame())
///     .show(ui, |ui| {
///         frame.inner_frame().show(ui, |ui| {
///             ui.label("Panel content");
///         });
///     });
/// # });
/// ```
pub struct StyledPanelFrame {
    position: PanelPosition,
    gap_color: Color32,
    panel_bg: Color32,
    rounding: u8,
    gap: i8,
}

impl StyledPanelFrame {
    /// Create a left panel frame (drawing toolbar)
    ///
    /// Rounded: top-right corner only
    pub fn left(ui: &Ui) -> Self {
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        Self {
            position: PanelPosition::Left,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
            rounding: CornerRadius::from(theme.radius.sm).nw,
            gap: theme.spacing.sm as i8,
        }
    }

    /// Create a right widget bar frame (icon tabs)
    ///
    /// Rounded: top-left corner only
    pub fn right_widget_bar(ui: &Ui) -> Self {
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        Self {
            position: PanelPosition::RightWidgetBar,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
            rounding: CornerRadius::from(theme.radius.sm).nw,
            gap: theme.spacing.sm as i8,
        }
    }

    /// Create a right content panel frame (alerts, object tree, etc.)
    ///
    /// Rounded: top-left corner only
    pub fn right_content(ui: &Ui) -> Self {
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        Self {
            position: PanelPosition::RightContent,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
            rounding: CornerRadius::from(theme.radius.sm).nw,
            gap: theme.spacing.sm as i8,
        }
    }

    /// Create a top panel frame (toolbar)
    ///
    /// No rounding - spans full width
    pub fn top(ui: &Ui) -> Self {
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        Self {
            position: PanelPosition::Top,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
            rounding: CornerRadius::from(theme.radius.sm).nw,
            gap: theme.spacing.sm as i8,
        }
    }

    /// Create a bottom panel frame (timeline)
    ///
    /// No rounding
    pub fn bottom(ui: &Ui) -> Self {
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        Self {
            position: PanelPosition::Bottom,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
            rounding: CornerRadius::from(theme.radius.sm).nw,
            gap: theme.spacing.sm as i8,
        }
    }

    /// Create a central chart area frame
    ///
    /// Rounded: top-left and top-right corners
    pub fn chart(ui: &Ui, chart_bg: Color32) -> Self {
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        Self {
            position: PanelPosition::Chart,
            gap_color: visuals.faint_bg_color,
            panel_bg: chart_bg,
            rounding: CornerRadius::from(theme.radius.sm).nw,
            gap: theme.spacing.sm as i8,
        }
    }

    /// Override the gap color
    #[must_use]
    pub fn gap_color(mut self, color: Color32) -> Self {
        self.gap_color = color;
        self
    }

    /// Override the panel background color
    #[must_use]
    pub fn panel_bg(mut self, color: Color32) -> Self {
        self.panel_bg = color;
        self
    }

    /// Get the outer frame (gap color with margins)
    ///
    /// Panel layout: gaps are created by panel margins
    /// - Top toolbar: bottom margin (gap below toolbar)
    /// - Side panels: horizontal margins only (NO top margin)
    /// - Chart: no margins (gaps from surrounding panels)
    pub fn outer_frame(&self) -> Frame {
        let margin = match self.position {
            // Left sidebar: right margin only (gap to chart)
            PanelPosition::Left => Margin {
                left: 0,
                right: self.gap,
                top: 0, // NO top margin - toolbar provides gap via its bottom margin
                bottom: 0,
            },
            // Right widget bar: left margin only (gap to chart)
            PanelPosition::RightWidgetBar => Margin {
                left: self.gap,
                right: 0,
                top: 0, // NO top margin
                bottom: 0,
            },
            // Right content panel: left margin only
            PanelPosition::RightContent => Margin {
                left: self.gap,
                right: 0,
                top: 0, // NO top margin
                bottom: 0,
            },
            // Top panel: bottom margin for gap below
            PanelPosition::Top => Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: self.gap,
            },
            // Bottom panel: top margin for gap above
            PanelPosition::Bottom => Margin {
                left: 0,
                right: 0,
                top: self.gap,
                bottom: 0,
            },
            // Chart: no margins (gaps provided by surrounding panels)
            PanelPosition::Chart => Margin::ZERO,
        };

        Frame::new()
            .fill(self.gap_color)
            .corner_radius(CornerRadius::ZERO)
            .inner_margin(margin)
    }

    /// Get the inner frame (panel bg with selective rounding)
    pub fn inner_frame(&self) -> Frame {
        let rounding = match self.position {
            PanelPosition::Left => CornerRadius {
                nw: 0,
                ne: self.rounding,
                sw: 0,
                se: 0,
            },
            PanelPosition::RightWidgetBar | PanelPosition::RightContent => CornerRadius {
                nw: self.rounding,
                ne: 0,
                sw: 0,
                se: 0,
            },
            PanelPosition::Top | PanelPosition::Bottom => CornerRadius::ZERO,
            PanelPosition::Chart => CornerRadius {
                nw: self.rounding,
                ne: self.rounding,
                sw: 0,
                se: 0,
            },
        };

        Frame::new().fill(self.panel_bg).corner_radius(rounding)
    }

    /// Show both outer and inner frames with content
    ///
    /// This is a convenience method that nests the frames correctly.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.outer_frame()
            .show(ui, |ui| self.inner_frame().show(ui, content).inner)
    }
}

// =============================================================================
// Card: a bordered surface one step above its parent
// =============================================================================

/// Card container: the theme's secondary surface with a 1 px border, rounding and padding.
///
/// ```
/// # use egui_charts::ui_kit::Card;
/// # egui::__run_test_ui(|ui| {
/// Card::new().show(ui, |ui| {
///     ui.label("Content in a card");
/// });
/// # });
/// ```
pub struct Card {
    rounding: Option<f32>,
    padding: Option<f32>,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    /// Create a card with the theme's small radius and large padding
    pub fn new() -> Self {
        Self {
            rounding: None,
            padding: None,
        }
    }

    /// Override the corner rounding
    #[must_use]
    pub fn rounding(mut self, r: f32) -> Self {
        self.rounding = Some(r);
        self
    }

    /// Override the inner padding
    #[must_use]
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = Some(p);
        self
    }

    /// Show the card with content
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let theme = ui.active_theme();
        Frame::new()
            .fill(theme.ui.panel_bg_secondary)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(self.rounding.unwrap_or(theme.radius.sm))
            .inner_margin(self.padding.unwrap_or(theme.spacing.lg))
            .show(ui, content)
    }
}

/// Create a simple modern frame (no gaps, no rounding)
///
/// Used for the Modern layout style where panels are edge-to-edge with separator lines.
pub fn modern_panel_frame(bg: Color32) -> Frame {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(egui::Margin::ZERO)
        .outer_margin(egui::Margin::ZERO)
        .stroke(egui::Stroke::NONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_panel_frames_use_the_applied_gap_and_radius() {
        for mut theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            theme.spacing.sm = 11.0;
            theme.radius.sm = 7.5;
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.run_ui(Default::default(), |ui| {
                for (frame, gap, corners) in [
                    (StyledPanelFrame::left(ui), [0, 11, 0, 0], [0, 8]),
                    (
                        StyledPanelFrame::right_widget_bar(ui),
                        [11, 0, 0, 0],
                        [8, 0],
                    ),
                    (StyledPanelFrame::right_content(ui), [11, 0, 0, 0], [8, 0]),
                    (StyledPanelFrame::top(ui), [0, 0, 0, 11], [0, 0]),
                    (StyledPanelFrame::bottom(ui), [0, 0, 11, 0], [0, 0]),
                    (
                        StyledPanelFrame::chart(ui, theme.chart.bg),
                        [0, 0, 0, 0],
                        [8, 8],
                    ),
                ] {
                    let margin = frame.outer_frame().inner_margin;
                    assert_eq!([margin.left, margin.right, margin.top, margin.bottom], gap);
                    let radius = frame.inner_frame().corner_radius;
                    assert_eq!([radius.nw, radius.ne], corners);
                    assert_eq!([radius.sw, radius.se], [0, 0]);
                }
            })
            .drop_without_applying_deltas();
        }
    }
}
