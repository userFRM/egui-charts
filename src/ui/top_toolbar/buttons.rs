//! Reusable button components for the top toolbar.
//!
//! Eliminates ~180 lines of repetitive button rendering code.

use crate::icons::Icon;
use crate::styles::{icons as icon_sizes, stroke, typography};
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

use super::TopToolbarConfig;
use crate::ext::UiExt;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;

/// Small icon-only button (config-driven size)
///
/// Used for: undo, redo, settings, theme, fullscreen, screenshot, save, etc.
/// This component eliminates ~161 lines of duplication (used 7 times in show()).
///
/// Note: This is the top-toolbar-specific variant that uses [`TopToolbarConfig`]
/// for sizing. For a generic icon button, see [`crate::ui_kit::buttons::IconButton`].
pub struct ToolbarIconButton<'a> {
    pub icon: &'a Icon,
    pub tooltip: &'static str,
    pub config: TopToolbarConfig,
}

impl<'a> ToolbarIconButton<'a> {
    pub fn new(icon: &'a Icon, tooltip: &'static str, config: &TopToolbarConfig) -> Self {
        Self {
            icon,
            tooltip,
            config: config.clone(),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let icon_size = self.config.small_icon_size;
        let btn_size = self.config.btn_size;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(btn_size), Sense::click());

        // Background with theme-aware colors
        let bg_color = if response.is_pointer_button_down_on() {
            theming::btn_bg_pressed(ui)
        } else if response.hovered() {
            theming::btn_bg_hover(ui)
        } else {
            theming::btn_bg_normal(ui)
        };

        ui.painter()
            .rect_filled(rect, self.config.rounding, bg_color);

        // Icon with hover/pressed color change
        let is_hovered = response.hovered();
        let is_active = response.is_pointer_button_down_on();

        // Render icon at exact position (with visibility check)
        let icon_pos = rect.center() - Vec2::splat(icon_size / 2.0);
        let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
        if ui.is_rect_visible(icon_rect) {
            let icon_color = if is_active {
                theming::icon_active(ui)
            } else if is_hovered {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);
        }

        response.on_hover_text(self.tooltip)
    }
}

/// Icon button with text label
///
/// Used for: Indicators, Alert, Replay btns.
/// This component eliminates ~117 lines of duplication (used 3 times in show()).
pub struct IconTextButton<'a> {
    pub icon: &'a Icon,
    pub label: &'static str,
    pub config: TopToolbarConfig,
}

impl<'a> IconTextButton<'a> {
    pub fn new(icon: &'a Icon, label: &'static str, config: &TopToolbarConfig) -> Self {
        Self {
            icon,
            label,
            config: config.clone(),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let icon_size = self.config.small_icon_size;
        let padding = self.config.padding;
        // Measure actual text width instead of approximation
        let font_id = egui::FontId::proportional(typography::MD);
        let galley = ui.painter().layout_no_wrap(
            self.label.to_string(),
            font_id,
            DESIGN_TOKENS.semantic.ui.text_light,
        );
        let text_width = galley.rect.width();
        let total_width = icon_size + padding * 3.0 + text_width;
        let height = self.config.btn_size;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

        // All paint operations wrapped with visibility check
        if ui.is_rect_visible(rect) {
            // Background
            let bg_color = if response.is_pointer_button_down_on() {
                theming::btn_bg_pressed(ui)
            } else if response.hovered() {
                theming::btn_bg_hover(ui)
            } else {
                theming::btn_bg_normal(ui)
            };

            ui.painter()
                .rect_filled(rect, self.config.rounding, bg_color);

            // Hover/pressed state
            let is_hovered = response.hovered();
            let is_active = response.is_pointer_button_down_on();

            // Icon - using ui.put() for precise positioning
            let icon_pos = Pos2::new(rect.min.x + padding, rect.center().y - icon_size / 2.0);
            let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
            let icon_color = if is_active {
                theming::icon_active(ui)
            } else if is_hovered {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);

            // Text color
            let color = if is_hovered || is_active {
                theming::icon_active(ui)
            } else {
                theming::icon_normal(ui)
            };

            // Text
            ui.painter().text(
                Pos2::new(rect.min.x + padding + icon_size + padding, rect.center().y),
                egui::Align2::LEFT_CENTER,
                self.label,
                egui::FontId::proportional(typography::MD),
                color,
            );
        }

        response
    }
}

/// Text-only button (used for interval dropdown)
///
/// Displays text with hover/pressed background.
pub struct TextButton {
    pub text: String,
    pub tooltip: Option<&'static str>,
    pub config: TopToolbarConfig,
}

impl TextButton {
    pub fn new(text: String, config: &TopToolbarConfig) -> Self {
        Self {
            text,
            tooltip: None,
            config: config.clone(),
        }
    }

    pub fn with_tooltip(mut self, tooltip: &'static str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let padding = self.config.padding;
        // Measure actual text width
        let font_id = egui::FontId::proportional(typography::LG);
        let galley = ui.painter().layout_no_wrap(
            self.text.clone(),
            font_id,
            DESIGN_TOKENS.semantic.ui.text_light,
        );
        let text_width = galley.rect.width();
        let total_width = text_width + padding * 2.0;
        let height = self.config.btn_size;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

        // Background
        let bg_color = if response.is_pointer_button_down_on() {
            theming::btn_bg_pressed(ui)
        } else if response.hovered() {
            theming::btn_bg_hover(ui)
        } else {
            theming::btn_bg_normal(ui)
        };

        ui.painter()
            .rect_filled(rect, self.config.rounding, bg_color);

        // Text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &self.text,
            egui::FontId::proportional(typography::LG),
            theming::text_color(ui),
        );

        if let Some(tooltip) = self.tooltip {
            response.on_hover_text(tooltip)
        } else {
            response
        }
    }
}

/// Vertical separator line
pub fn separator(ui: &mut Ui) {
    ui.space_lg();
    let rect = ui
        .allocate_exact_size(Vec2::new(stroke::HAIRLINE, icon_sizes::MD), Sense::hover())
        .0;
    ui.painter().rect_filled(
        rect,
        DESIGN_TOKENS.rounding.none,
        theming::separator_color(ui),
    );
    ui.space_lg();
}

/// Filled pill button (Trade button style)
///
/// Blue-filled rounded pill with white text
pub struct PillButtonFilled {
    pub text: &'static str,
    pub tooltip: &'static str,
}

impl PillButtonFilled {
    pub fn new(text: &'static str, tooltip: &'static str) -> Self {
        Self { text, tooltip }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let padding_h = DESIGN_TOKENS.spacing.lg;
        let font_id = egui::FontId::proportional(typography::SM);
        let galley = ui.painter().layout_no_wrap(
            self.text.to_string(),
            font_id,
            theming::publish_button_text(ui),
        );
        let text_width = galley.rect.width();
        let total_width = text_width + padding_h * 2.0;
        let height = DESIGN_TOKENS.sizing.button_sm;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

        if ui.is_rect_visible(rect) {
            // Blue background
            let bg_color = if response.is_pointer_button_down_on() {
                theming::publish_button_bg_pressed(ui)
            } else if response.hovered() {
                theming::publish_button_bg_hover(ui)
            } else {
                theming::publish_button_bg(ui)
            };

            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.pill, bg_color);

            // White text
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                self.text,
                egui::FontId::proportional(typography::SM),
                theming::publish_button_text(ui),
            );
        }

        response.on_hover_text(self.tooltip)
    }
}

/// Outlined pill button (Publish button style)
///
/// Transparent with border and text
pub struct PillButtonOutlined {
    pub text: &'static str,
    pub tooltip: &'static str,
}

impl PillButtonOutlined {
    pub fn new(text: &'static str, tooltip: &'static str) -> Self {
        Self { text, tooltip }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let padding_h = DESIGN_TOKENS.spacing.lg;
        let font_id = egui::FontId::proportional(typography::SM);
        let galley =
            ui.painter()
                .layout_no_wrap(self.text.to_string(), font_id, theming::text_color(ui));
        let text_width = galley.rect.width();
        let total_width = text_width + padding_h * 2.0;
        let height = DESIGN_TOKENS.sizing.button_sm;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

        if ui.is_rect_visible(rect) {
            // Background: transparent or subtle hover
            let bg_color = if response.is_pointer_button_down_on() {
                theming::btn_bg_pressed(ui)
            } else if response.hovered() {
                theming::btn_bg_hover(ui)
            } else {
                egui::Color32::TRANSPARENT
            };

            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.pill, bg_color);

            // Border
            let border_color = if response.hovered() {
                theming::icon_hover_color(ui)
            } else {
                theming::separator_color(ui)
            };
            ui.painter().rect_stroke(
                rect,
                DESIGN_TOKENS.rounding.pill,
                egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
                egui::StrokeKind::Inside,
            );

            // Text
            let text_color = if response.hovered() {
                theming::icon_hover_color(ui)
            } else {
                theming::text_color(ui)
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                self.text,
                egui::FontId::proportional(typography::SM),
                text_color,
            );
        }

        response.on_hover_text(self.tooltip)
    }
}

/// Text + icon dropdown button (Save dropdown, etc.)
pub struct TextIconButton<'a> {
    pub text: &'static str,
    pub icon: &'a crate::icons::Icon,
    pub tooltip: &'static str,
    pub config: TopToolbarConfig,
}

impl<'a> TextIconButton<'a> {
    pub fn new(
        text: &'static str,
        icon: &'a crate::icons::Icon,
        tooltip: &'static str,
        config: &TopToolbarConfig,
    ) -> Self {
        Self {
            text,
            icon,
            tooltip,
            config: config.clone(),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let padding = self.config.padding;
        let icon_size = icon_sizes::XS;
        let font_id = egui::FontId::proportional(typography::SM);
        let galley = ui.painter().layout_no_wrap(
            self.text.to_string(),
            font_id.clone(),
            theming::text_color(ui),
        );
        let text_width = galley.rect.width();
        let total_width = text_width + padding * 2.0 + icon_size + padding;
        let height = self.config.btn_size;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

        if ui.is_rect_visible(rect) {
            // Background
            let bg_color = if response.is_pointer_button_down_on() {
                theming::btn_bg_pressed(ui)
            } else if response.hovered() {
                theming::btn_bg_hover(ui)
            } else {
                theming::btn_bg_normal(ui)
            };

            ui.painter()
                .rect_filled(rect, self.config.rounding, bg_color);

            // Text
            let text_color = if response.hovered() {
                theming::icon_hover_color(ui)
            } else {
                theming::text_color(ui)
            };
            let text_x = rect.min.x + padding;
            ui.painter().text(
                Pos2::new(text_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                self.text,
                font_id,
                text_color,
            );

            // Icon (chevron)
            let icon_x = rect.max.x - padding - icon_size;
            let icon_pos = Pos2::new(icon_x, rect.center().y - icon_size / 2.0);
            let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
            let icon_color = if response.hovered() {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);
        }

        response.on_hover_text(self.tooltip)
    }
}
