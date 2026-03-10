//! Right Edge Icon Bar
//!
//! Vertical strip of icon btns on the far right edge of the screen.

use crate::ext::HasDesignTokens;
use crate::icons::{Icon, icons as embedded_icons};
use crate::styles::typography;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Sense, Ui, Vec2};

/// Section grouping for widget bar icons
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetBarSection {
    /// Top section: Alerts, ObjectTree
    Top,
    /// Bottom section: Help (separated by line)
    Bottom,
}

/// Available icons in the right edge bar
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RightBarIcon {
    Alerts,
    ObjectTree,
    Help,
}

impl RightBarIcon {
    pub fn icon(&self) -> &'static Icon {
        match self {
            RightBarIcon::Alerts => &embedded_icons::WIDGET_BAR_ALERTS,
            RightBarIcon::ObjectTree => &embedded_icons::OBJECT_TREE,
            RightBarIcon::Help => &embedded_icons::WIDGET_BAR_HELP,
        }
    }

    pub fn tooltip(&self) -> &'static str {
        match self {
            RightBarIcon::Alerts => "Alerts",
            RightBarIcon::ObjectTree => "Object Tree",
            RightBarIcon::Help => "Help",
        }
    }

    /// Get the section this icon belongs to
    pub fn section(&self) -> WidgetBarSection {
        match self {
            RightBarIcon::Alerts | RightBarIcon::ObjectTree => WidgetBarSection::Top,
            RightBarIcon::Help => WidgetBarSection::Bottom,
        }
    }
}

/// Configuration for the right icon bar (exact dimensions)
#[derive(Clone, Debug)]
pub struct WidgetBarConfig {
    /// Bar width - 52px
    pub width: f32,
    /// Button size - 44x44 buttons
    pub btn_size: f32,
    /// Icon size - 44x44 native viewBox
    pub icon_size: f32,
    /// Button spacing - 2px gap between buttons
    pub btn_spacing: f32,
    /// Hover margin - 3px inset for hover rect
    pub hover_margin: f32,
    /// Hover rounding - 8px corners on hover
    pub hover_rounding: f32,
    /// Top padding
    pub top_padding: f32,
}

impl Default for WidgetBarConfig {
    fn default() -> Self {
        Self {
            // Exact dimensions
            width: DESIGN_TOKENS.sizing.toolbar.right_sidebar_width, // 52px
            btn_size: DESIGN_TOKENS.sizing.toolbar.right_btn_size,   // 44px
            icon_size: DESIGN_TOKENS.sizing.toolbar.right_icon_size, // 44px
            btn_spacing: DESIGN_TOKENS.sizing.toolbar.right_btn_spacing, // 2px
            hover_margin: DESIGN_TOKENS.sizing.toolbar.right_hover_margin, // 3px
            hover_rounding: DESIGN_TOKENS.sizing.toolbar.right_hover_rounding, // 8px
            top_padding: DESIGN_TOKENS.spacing.md,
        }
    }
}

/// Action returned by the right icon bar
#[derive(Clone, Debug, PartialEq)]
pub enum WidgetBarAction {
    None,
    Clicked(RightBarIcon),
}

/// Right Edge Icon Bar
pub struct WidgetBar {
    pub config: WidgetBarConfig,
    pub icons: Vec<RightBarIcon>,
    pub active_icon: Option<RightBarIcon>,
    pub notification_counts: std::collections::HashMap<RightBarIcon, u32>,
}

impl Default for WidgetBar {
    fn default() -> Self {
        Self {
            config: WidgetBarConfig::default(),
            icons: vec![
                // Top section
                RightBarIcon::Alerts,
                RightBarIcon::ObjectTree,
                // Bottom section
                RightBarIcon::Help,
            ],
            active_icon: None,
            notification_counts: std::collections::HashMap::new(),
        }
    }
}

impl WidgetBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: WidgetBarConfig) -> Self {
        self.config = config;
        self
    }

    pub fn set_notification_cnt(&mut self, icon: RightBarIcon, count: u32) {
        self.notification_counts.insert(icon, count);
    }

    /// Show the icon bar - colors fetched from ui context at render time
    /// Uses section grouping and flexible spacer
    pub fn show(&mut self, ui: &mut Ui) -> WidgetBarAction {
        let mut action = WidgetBarAction::None;

        // Get colors from theme at render time (not cached)
        let bg_color = ui.style().visuals.panel_fill;
        let icon_color = ui.icon_color();
        let icon_hover_color = ui.icon_hover();
        let active_color = ui.accent_color();
        let hover_bg = ui.hover_bg();
        let pressed_bg = ui.active_bg();
        let error_color = ui.style().visuals.error_fg_color;
        let separator_color = ui.style().visuals.widgets.noninteractive.bg_fill;

        let available_height = ui.available_height();
        let (response, painter) = ui.allocate_painter(
            Vec2::new(self.config.width, available_height),
            Sense::hover(),
        );
        let rect = response.rect;

        // Background with panel rounding to match outer frame (with visibility check)
        if ui.is_rect_visible(rect) {
            painter.rect_filled(rect, DESIGN_TOKENS.rounding.panel, bg_color);
        }

        let mut y = rect.min.y + self.config.top_padding;
        let center_x = rect.center().x;

        // Separate icons into sections
        let top_icons: Vec<_> = self
            .icons
            .iter()
            .filter(|i| i.section() == WidgetBarSection::Top)
            .copied()
            .collect();
        let bottom_icons: Vec<_> = self
            .icons
            .iter()
            .filter(|i| i.section() == WidgetBarSection::Bottom)
            .copied()
            .collect();

        // Calculate space needed for bottom section
        let bottom_section_height = (bottom_icons.len() as f32)
            * (self.config.btn_size + self.config.btn_spacing)
            + DESIGN_TOKENS.spacing.lg; // separator + padding

        // Render top section icons (Alerts, ObjectTree)
        for icon in &top_icons {
            y = self.render_icon_button(
                ui,
                &painter,
                icon,
                center_x,
                y,
                &mut action,
                icon_color,
                icon_hover_color,
                active_color,
                hover_bg,
                pressed_bg,
                error_color,
            );
        }

        // Flexible spacer BETWEEN top and bottom sections
        let remaining_space = rect.max.y - y - bottom_section_height;
        if remaining_space > 0.0 {
            y += remaining_space;
        }

        // Draw separator before bottom section (with visibility check)
        let sep_margin = DESIGN_TOKENS.sizing.toolbar.separator_margin;
        let sep_width = self.config.width - sep_margin * 2.0;
        let sep_height = DESIGN_TOKENS.sizing.toolbar.separator_width;
        let sep_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + sep_margin, y),
            Vec2::new(sep_width, sep_height),
        );
        if ui.is_rect_visible(sep_rect) {
            painter.rect_filled(sep_rect, DESIGN_TOKENS.rounding.none, separator_color);
        }
        y += sep_height + DESIGN_TOKENS.spacing.md;

        // Render bottom section icons (Help)
        for icon in &bottom_icons {
            y = self.render_icon_button(
                ui,
                &painter,
                icon,
                center_x,
                y,
                &mut action,
                icon_color,
                icon_hover_color,
                active_color,
                hover_bg,
                pressed_bg,
                error_color,
            );
        }

        action
    }

    /// Render a single icon button with hover rect
    fn render_icon_button(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        icon: &RightBarIcon,
        center_x: f32,
        y: f32,
        action: &mut WidgetBarAction,
        icon_color: egui::Color32,
        icon_hover_color: egui::Color32,
        active_color: egui::Color32,
        hover_bg: egui::Color32,
        pressed_bg: egui::Color32,
        error_color: egui::Color32,
    ) -> f32 {
        let btn_rect = Rect::from_center_size(
            Pos2::new(center_x, y + self.config.btn_size / 2.0),
            Vec2::splat(self.config.btn_size),
        );

        // Inner hover rect with margins
        let hover_rect = Rect::from_min_max(
            Pos2::new(
                btn_rect.min.x + self.config.hover_margin,
                btn_rect.min.y + self.config.hover_margin,
            ),
            Pos2::new(
                btn_rect.max.x - self.config.hover_margin,
                btn_rect.max.y - self.config.hover_margin,
            ),
        );

        // Check for clicks
        let btn_res = ui.allocate_rect(btn_rect, Sense::click());
        let is_hovered = btn_res.hovered();
        let is_pressed = btn_res.is_pointer_button_down_on();
        let is_active = self.active_icon == Some(*icon);

        // Button background on inner rect only - with visibility check
        if ui.is_rect_visible(hover_rect) {
            if is_active {
                painter.rect_filled(hover_rect, self.config.hover_rounding, hover_bg);
            } else if is_pressed {
                painter.rect_filled(hover_rect, self.config.hover_rounding, pressed_bg);
            } else if is_hovered {
                painter.rect_filled(hover_rect, self.config.hover_rounding, hover_bg);
            }
        }

        // Icon color based on state
        let curr_icon_color = if is_active {
            active_color
        } else if is_pressed || is_hovered {
            icon_hover_color
        } else {
            icon_color
        };

        // Render icon centered in button rect (with visibility check)
        let icon_rect =
            Rect::from_center_size(btn_rect.center(), Vec2::splat(self.config.icon_size));
        if ui.is_rect_visible(icon_rect) {
            icon.icon()
                .as_image_tinted(Vec2::splat(self.config.icon_size), curr_icon_color)
                .paint_at(ui, icon_rect);
        }

        // Notification badge (with visibility check)
        if let Some(&count) = self.notification_counts.get(icon)
            && count > 0
        {
            let badge_radius = DESIGN_TOKENS.spacing.md;
            let badge_pos = Pos2::new(btn_rect.max.x - badge_radius, btn_rect.min.y + badge_radius);
            let badge_rect = Rect::from_center_size(badge_pos, Vec2::splat(badge_radius * 2.0));

            if ui.is_rect_visible(badge_rect) {
                painter.circle_filled(badge_pos, badge_radius, error_color);

                let cnt_text = if count > 9 {
                    "9+".to_string()
                } else {
                    count.to_string()
                };
                let badge_galley = painter.layout_no_wrap(
                    cnt_text,
                    egui::FontId::proportional(typography::XS),
                    theming::badge_text_color(ui),
                );
                painter.galley(
                    Pos2::new(
                        badge_pos.x - badge_galley.size().x / 2.0,
                        badge_pos.y - badge_galley.size().y / 2.0,
                    ),
                    badge_galley,
                    egui::Color32::TRANSPARENT,
                );
            }
        }

        // Handle click
        if btn_res.clicked() {
            self.active_icon = if is_active { None } else { Some(*icon) };
            *action = WidgetBarAction::Clicked(*icon);
        }

        // Tooltip
        if is_hovered {
            btn_res.on_hover_text(icon.tooltip());
        }

        y + self.config.btn_size + self.config.btn_spacing
    }
}
