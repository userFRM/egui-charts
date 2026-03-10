//! Trading Panel Row - collapsible trading panel row
//!
//! A slim 28px row that sits below the chart toolbar, providing access to
//! the trading panel with expand/collapse and fullscreen controls.

use crate::icons::icons as embedded_icons;
use crate::styles::typography;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Sense, Ui, Vec2};

/// Action returned from trading panel row interactions
#[derive(Debug, Clone, PartialEq)]
pub enum TradingPanelRowAction {
    /// No action taken
    None,
    /// Toggle the trading panel expanded/collapsed state
    ToggleExpanded,
    /// Enter fullscreen mode for trading panel
    Fullscreen,
}

/// Configuration for the trading panel row
#[derive(Debug, Clone)]
pub struct TradingPanelRowConfig {
    /// Row height (28px)
    pub height: f32,
    /// Left padding for text
    pub left_padding: f32,
    /// Icon size
    pub icon_size: f32,
    /// Icon button size
    pub icon_btn_size: f32,
    /// Right padding
    pub right_padding: f32,
}

impl Default for TradingPanelRowConfig {
    fn default() -> Self {
        Self {
            height: DESIGN_TOKENS.sizing.panel.trading_panel_row_height,
            left_padding: DESIGN_TOKENS.spacing.lg,
            icon_size: DESIGN_TOKENS.sizing.panel.trading_panel_row_icon_size,
            icon_btn_size: DESIGN_TOKENS.sizing.panel.trading_panel_row_btn_size,
            right_padding: DESIGN_TOKENS.spacing.md,
        }
    }
}

/// Trading Panel Row component
pub struct TradingPanelRow {
    /// Configuration
    pub config: TradingPanelRowConfig,
    /// Whether the trading panel is expanded
    pub is_expanded: bool,
}

impl Default for TradingPanelRow {
    fn default() -> Self {
        Self::new()
    }
}

impl TradingPanelRow {
    /// Create a new trading panel row
    pub fn new() -> Self {
        Self {
            config: TradingPanelRowConfig::default(),
            is_expanded: false,
        }
    }

    /// Create with custom config
    pub fn with_config(config: TradingPanelRowConfig) -> Self {
        Self {
            config,
            is_expanded: false,
        }
    }

    /// Set expanded state
    pub fn set_expanded(&mut self, expanded: bool) {
        self.is_expanded = expanded;
    }

    /// Toggle expanded state
    pub fn toggle_expanded(&mut self) {
        self.is_expanded = !self.is_expanded;
    }

    /// Show the trading panel row and return action
    pub fn show(&mut self, ui: &mut Ui) -> TradingPanelRowAction {
        let mut action = TradingPanelRowAction::None;

        // Get colors from theme
        let text_color = theming::text_color(ui);
        let text_muted = theming::muted_color(ui);
        let hover_bg = theming::btn_bg_hover(ui);
        let separator_color = theming::separator_color(ui);

        let available_width = ui.available_width();
        let (response, painter) = ui.allocate_painter(
            Vec2::new(available_width, self.config.height),
            Sense::hover(),
        );
        let rect = response.rect;

        // All paint operations wrapped with visibility check
        if ui.is_rect_visible(rect) {
            // Draw top border/separator
            painter.hline(
                rect.x_range(),
                rect.min.y,
                egui::Stroke::new(1.0, separator_color),
            );

            // "Trading Panel" text on the left
            let text_pos = Pos2::new(rect.min.x + self.config.left_padding, rect.center().y);
            painter.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                "Trading Panel",
                egui::FontId::proportional(typography::SM),
                text_muted,
            );
        }

        // Right side buttons: Fullscreen | Chevron
        let btn_size = self.config.icon_btn_size;
        let icon_size = self.config.icon_size;

        // Chevron button (rightmost)
        let chevron_x = rect.max.x - self.config.right_padding - btn_size;
        let chevron_rect = Rect::from_center_size(
            Pos2::new(chevron_x + btn_size / 2.0, rect.center().y),
            Vec2::splat(btn_size),
        );
        let chevron_response = ui.allocate_rect(chevron_rect, Sense::click());

        // Draw chevron hover bg and icon (with visibility check)
        if ui.is_rect_visible(chevron_rect) {
            if chevron_response.hovered() {
                painter.rect_filled(chevron_rect, DESIGN_TOKENS.rounding.sm, hover_bg);
            }

            // Draw chevron icon (rotated based on expanded state)
            let chevron_icon_rect =
                Rect::from_center_size(chevron_rect.center(), Vec2::splat(icon_size));
            let chevron_color = if chevron_response.hovered() {
                text_color
            } else {
                text_muted
            };

            let chevron_icon = &embedded_icons::CHEVRON_DOWN;
            chevron_icon
                .as_image_tinted(Vec2::splat(icon_size), chevron_color)
                .paint_at(ui, chevron_icon_rect);
        }

        if chevron_response.clicked() {
            action = TradingPanelRowAction::ToggleExpanded;
        }
        chevron_response.on_hover_text(if self.is_expanded {
            "Collapse"
        } else {
            "Expand"
        });

        // Fullscreen button (to the left of chevron)
        let fullscreen_x = chevron_x - btn_size - DESIGN_TOKENS.spacing.xs;
        let fullscreen_rect = Rect::from_center_size(
            Pos2::new(fullscreen_x + btn_size / 2.0, rect.center().y),
            Vec2::splat(btn_size),
        );
        let fullscreen_response = ui.allocate_rect(fullscreen_rect, Sense::click());

        // Draw fullscreen hover bg and icon (with visibility check)
        if ui.is_rect_visible(fullscreen_rect) {
            if fullscreen_response.hovered() {
                painter.rect_filled(fullscreen_rect, DESIGN_TOKENS.rounding.sm, hover_bg);
            }

            // Draw fullscreen icon
            let fullscreen_icon_rect =
                Rect::from_center_size(fullscreen_rect.center(), Vec2::splat(icon_size));
            let fullscreen_color = if fullscreen_response.hovered() {
                text_color
            } else {
                text_muted
            };
            embedded_icons::FULLSCREEN
                .as_image_tinted(Vec2::splat(icon_size), fullscreen_color)
                .paint_at(ui, fullscreen_icon_rect);
        }

        if fullscreen_response.clicked() {
            action = TradingPanelRowAction::Fullscreen;
        }
        fullscreen_response.on_hover_text("Fullscreen");

        action
    }
}
