//! Right Panel with Tab Navigation
//!
//! Displays a tabbed interface for Alerts and Object Tree panels.

use crate::icons::{Icon, icons as embedded_icons};
use crate::styles::{icons, stroke, typography};
use crate::theme::Theme;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Available tabs in the right panel
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightPanelTab {
    Alerts,
    ObjectTree,
}

impl TryFrom<u8> for RightPanelTab {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(RightPanelTab::Alerts),
            1 => Ok(RightPanelTab::ObjectTree),
            _ => Err(()),
        }
    }
}

impl RightPanelTab {
    pub fn icon(&self) -> &'static Icon {
        match self {
            RightPanelTab::Alerts => &embedded_icons::BELL,
            RightPanelTab::ObjectTree => &embedded_icons::OBJECT_TREE,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RightPanelTab::Alerts => "Alerts",
            RightPanelTab::ObjectTree => "Object Tree",
        }
    }
}

/// Configuration for the right panel tabs
#[derive(Clone, Debug)]
pub struct RightPanelTabsConfig {
    pub tab_bar_width: f32,
    pub tab_button_size: f32,
    pub bg_color: Color32,
    pub tab_bar_bg: Color32,
    pub active_tab_bg: Color32,
    pub hover_bg: Color32,
    pub pressed_bg: Color32,
    pub icon_color: Color32,
    pub active_icon_color: Color32,
    pub border_color: Color32,
}

impl RightPanelTabsConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.semantic.ui;
        Self {
            tab_bar_width: DESIGN_TOKENS.sizing.toolbar.right_icon_width,
            tab_button_size: DESIGN_TOKENS.sizing.button_lg,
            bg_color: ui.panel_bg,
            tab_bar_bg: ui.panel_bg_secondary,
            active_tab_bg: ui.panel_bg,
            hover_bg: ui.btn_bg_hover,
            pressed_bg: ui.btn_bg_active,
            icon_color: ui.icon,
            active_icon_color: ui.accent,
            border_color: ui.border_subtle,
        }
    }
}

impl Default for RightPanelTabsConfig {
    fn default() -> Self {
        Self::from_theme(&Theme::dark())
    }
}

/// Action returned by the right panel tabs
#[derive(Clone, Debug, PartialEq)]
pub enum RightPanelAction {
    None,
    TabChanged(RightPanelTab),
    Close,
}

/// Right Panel with Tab Navigation
pub struct RightPanelTabs {
    pub active_tab: RightPanelTab,
    pub config: RightPanelTabsConfig,
    pub available_tabs: Vec<RightPanelTab>,
}

impl Default for RightPanelTabs {
    fn default() -> Self {
        Self {
            active_tab: RightPanelTab::Alerts,
            config: RightPanelTabsConfig::default(),
            available_tabs: vec![RightPanelTab::Alerts, RightPanelTab::ObjectTree],
        }
    }
}

impl RightPanelTabs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: RightPanelTabsConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_active_tab(mut self, tab: RightPanelTab) -> Self {
        self.active_tab = tab;
        self
    }

    /// Select and activate a tab programmatically
    pub fn select_tab(&mut self, tab: RightPanelTab) {
        self.active_tab = tab;
    }

    /// Show the tab bar on the right edge
    /// Returns the action and consumed width for the tab bar
    pub fn show_tab_bar(&mut self, ui: &mut Ui) -> RightPanelAction {
        let mut action = RightPanelAction::None;

        let available_height = ui.available_height();
        let (response, painter) = ui.allocate_painter(
            Vec2::new(self.config.tab_bar_width, available_height),
            Sense::hover(),
        );
        let rect = response.rect;

        // Tab bar background
        painter.rect_filled(rect, 0.0, self.config.tab_bar_bg);

        // Left border
        painter.line_segment(
            [rect.left_top(), rect.left_bottom()],
            Stroke::new(stroke::HAIRLINE, self.config.border_color),
        );

        let mut y = rect.min.y + DESIGN_TOKENS.spacing.lg;

        // Draw each tab button
        for tab in &self.available_tabs {
            let is_active = *tab == self.active_tab;
            let btn_rect = Rect::from_min_size(
                Pos2::new(rect.min.x + DESIGN_TOKENS.spacing.sm, y),
                Vec2::new(self.config.tab_button_size, self.config.tab_button_size),
            );

            // Check for clicks
            let btn_res = ui.allocate_rect(btn_rect, Sense::click());
            let is_hovered = btn_res.hovered();
            let is_pressed = btn_res.is_pointer_button_down_on();

            // Button background (with press feedback)
            if is_active {
                painter.rect_filled(
                    btn_rect,
                    DESIGN_TOKENS.rounding.md,
                    self.config.active_tab_bg,
                );
                // Active indicator on left edge
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(rect.min.x, btn_rect.min.y),
                        Vec2::new(
                            DESIGN_TOKENS.sizing.settings_dialog.indicator_width,
                            self.config.tab_button_size,
                        ),
                    ),
                    DESIGN_TOKENS.rounding.none,
                    self.config.active_icon_color,
                );
            } else if is_pressed {
                painter.rect_filled(btn_rect, DESIGN_TOKENS.rounding.md, self.config.pressed_bg);
            } else if is_hovered {
                painter.rect_filled(btn_rect, DESIGN_TOKENS.rounding.md, self.config.hover_bg);
            }

            // Draw icon
            let icon_size = icons::SM;
            let icon_rect = Rect::from_center_size(btn_rect.center(), Vec2::splat(icon_size));

            // Get icon color based on state
            let icon_color = if is_active {
                theming::icon_active(ui)
            } else if is_hovered {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };

            tab.icon()
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);

            // Handle click
            if btn_res.clicked() {
                action = RightPanelAction::TabChanged(*tab);
            }

            // Tooltip
            if is_hovered {
                btn_res.on_hover_text(tab.label());
            }

            y += self.config.tab_button_size + DESIGN_TOKENS.spacing.sm;
        }

        action
    }

    /// Show the header for the active tab content area
    pub fn show_content_header(&self, ui: &mut Ui) -> Response {
        let (response, painter) = ui.allocate_painter(
            Vec2::new(
                ui.available_width(),
                DESIGN_TOKENS.sizing.context_menu.item_height,
            ),
            Sense::hover(),
        );
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, DESIGN_TOKENS.rounding.none, self.config.bg_color);

        // Bottom border
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(stroke::HAIRLINE, self.config.border_color),
        );

        // Tab title
        let title_galley = painter.layout_no_wrap(
            self.active_tab.label().to_string(),
            egui::FontId::proportional(typography::SM),
            ui.visuals().text_color(),
        );
        painter.galley(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.xl,
                rect.center().y - title_galley.size().y / 2.0,
            ),
            title_galley,
            Color32::TRANSPARENT,
        );

        response
    }

    pub fn set_active_tab(&mut self, tab: RightPanelTab) {
        self.active_tab = tab;
    }

    pub fn active_tab(&self) -> RightPanelTab {
        self.active_tab
    }
}
