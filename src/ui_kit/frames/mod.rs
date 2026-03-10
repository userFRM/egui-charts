//! Styled Panel Frames
//!
//! Builder pattern for creating panel frames with consistent gaps and selective corner rounding.
//!
//! Uses a "gap-based" layout where panels are separated by a background gap color,
//! and inner corners are rounded where panels meet.
//!
//! # Example
//! ```ignore
//! let visuals = ui.style().visuals.clone();
//! StyledPanelFrame::left(&visuals).show(ui, |ui| {
//!     // Panel content
//! });
//! ```

use egui::{Color32, CornerRadius, Frame, InnerResponse, Margin, Ui, Visuals};

use crate::tokens::DESIGN_TOKENS;

/// Panel corner radius - sourced from DESIGN_TOKENS.rounding.sm
fn panel_corner_radius() -> u8 {
    DESIGN_TOKENS.rounding.sm as u8
}

/// Gap between panels - sourced from DESIGN_TOKENS.spacing.panel_gap
fn panel_gap() -> i8 {
    DESIGN_TOKENS.spacing.panel_gap as i8
}

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
/// ```ignore
/// let visuals = ui.style().visuals.clone();
///
/// // Method 1: Show both frames with content
/// StyledPanelFrame::left(&visuals).show(ui, |ui| {
///     ui.label("Panel content");
/// });
///
/// // Method 2: Get frames separately (for SidePanel usage)
/// let frame = StyledPanelFrame::left(&visuals);
/// egui::SidePanel::left("left")
///     .frame(frame.outer_frame())
///     .show(ctx, |ui| {
///         frame.inner_frame().show(ui, |ui| {
///             ui.label("Panel content");
///         });
///     });
/// ```
pub struct StyledPanelFrame {
    position: PanelPosition,
    gap_color: Color32,
    panel_bg: Color32,
}

impl StyledPanelFrame {
    /// Create a left panel frame (drawing toolbar)
    ///
    /// Rounded: top-right corner only
    pub fn left(visuals: &Visuals) -> Self {
        Self {
            position: PanelPosition::Left,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
        }
    }

    /// Create a right widget bar frame (icon tabs)
    ///
    /// Rounded: top-left corner only
    pub fn right_widget_bar(visuals: &Visuals) -> Self {
        Self {
            position: PanelPosition::RightWidgetBar,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
        }
    }

    /// Create a right content panel frame (alerts, object tree, etc.)
    ///
    /// Rounded: top-left corner only
    pub fn right_content(visuals: &Visuals) -> Self {
        Self {
            position: PanelPosition::RightContent,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
        }
    }

    /// Create a top panel frame (toolbar)
    ///
    /// No rounding - spans full width
    pub fn top(visuals: &Visuals) -> Self {
        Self {
            position: PanelPosition::Top,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
        }
    }

    /// Create a bottom panel frame (timeline)
    ///
    /// No rounding
    pub fn bottom(visuals: &Visuals) -> Self {
        Self {
            position: PanelPosition::Bottom,
            gap_color: visuals.faint_bg_color,
            panel_bg: visuals.panel_fill,
        }
    }

    /// Create a central chart area frame
    ///
    /// Rounded: top-left and top-right corners
    pub fn chart(visuals: &Visuals, chart_bg: Color32) -> Self {
        Self {
            position: PanelPosition::Chart,
            gap_color: visuals.faint_bg_color,
            panel_bg: chart_bg,
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
                right: panel_gap(),
                top: 0, // NO top margin - toolbar provides gap via its bottom margin
                bottom: 0,
            },
            // Right widget bar: left margin only (gap to chart)
            PanelPosition::RightWidgetBar => Margin {
                left: panel_gap(),
                right: 0,
                top: 0, // NO top margin
                bottom: 0,
            },
            // Right content panel: left margin only
            PanelPosition::RightContent => Margin {
                left: panel_gap(),
                right: 0,
                top: 0, // NO top margin
                bottom: 0,
            },
            // Top panel: bottom margin for gap below
            PanelPosition::Top => Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: panel_gap(),
            },
            // Bottom panel: top margin for gap above
            PanelPosition::Bottom => Margin {
                left: 0,
                right: 0,
                top: panel_gap(),
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
                ne: panel_corner_radius(),
                sw: 0,
                se: 0,
            },
            PanelPosition::RightWidgetBar | PanelPosition::RightContent => CornerRadius {
                nw: panel_corner_radius(),
                ne: 0,
                sw: 0,
                se: 0,
            },
            PanelPosition::Top | PanelPosition::Bottom => CornerRadius::ZERO,
            PanelPosition::Chart => CornerRadius {
                nw: panel_corner_radius(),
                ne: panel_corner_radius(),
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
// Card — faint background container
// =============================================================================

/// Subtle card container with faint background, rounding, and padding.
///
/// Replaces the common pattern:
/// ```ignore
/// egui::Frame::NONE
///     .fill(ui.style().visuals.faint_bg_color)
///     .inner_margin(DESIGN_TOKENS.spacing.sm)
///     .corner_radius(DESIGN_TOKENS.rounding.sm)
///     .show(ui, |ui| { ... });
/// ```
///
/// Usage:
/// ```ignore
/// Card::new().show(ui, |ui| {
///     ui.label("Content in a subtle card");
/// });
/// ```
pub struct Card {
    rounding: f32,
    padding: f32,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    /// Create a card with default styling (sm rounding, sm padding)
    pub fn new() -> Self {
        Self {
            rounding: DESIGN_TOKENS.rounding.sm,
            padding: DESIGN_TOKENS.spacing.sm,
        }
    }

    /// Override the corner rounding
    #[must_use]
    pub fn rounding(mut self, r: f32) -> Self {
        self.rounding = r;
        self
    }

    /// Override the inner padding
    #[must_use]
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }

    /// Show the card with content
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        Frame::new()
            .fill(ui.style().visuals.faint_bg_color)
            .corner_radius(self.rounding)
            .inner_margin(self.padding)
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
