//! Pane control buttons that appear at top-right of chart panes.
//!
//! Shows maximize, minimize, close, and settings buttons on hover.

use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Sense, Ui, Vec2};

/// Unique identifier for a pane
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub usize);

/// Actions from pane controls
#[derive(Debug, Clone, PartialEq)]
pub enum PaneControlAction {
    /// No action taken
    None,
    /// Maximize the specified pane
    Maximize(PaneId),
    /// Minimize the specified pane
    Minimize(PaneId),
    /// Close the specified pane
    Close(PaneId),
    /// Open settings for the specified pane
    Settings(PaneId),
}

/// Configuration for pane controls
#[derive(Clone, Debug)]
pub struct PaneControlsConfig {
    /// Button size (square)
    pub button_size: f32,
    /// Gap between buttons
    pub button_gap: f32,
    /// Padding inside the control bar
    pub padding: f32,
    /// Corner radius for the control bar background
    pub rounding: f32,
    /// Offset from pane corner
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for PaneControlsConfig {
    fn default() -> Self {
        Self {
            button_size: DESIGN_TOKENS.sizing.button_sm,
            button_gap: crate::theme::Theme::default().spacing.xs,
            padding: crate::theme::Theme::default().spacing.xs,
            rounding: crate::theme::Theme::default().radius.sm,
            offset_x: crate::theme::Theme::default().spacing.sm,
            offset_y: crate::theme::Theme::default().spacing.sm,
        }
    }
}

/// Pane control buttons (maximize, minimize, close, settings)
pub struct PaneControls {
    pub config: PaneControlsConfig,
}

impl Default for PaneControls {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneControls {
    /// Create a new pane controls widget with default configuration
    pub fn new() -> Self {
        Self {
            config: PaneControlsConfig::default(),
        }
    }

    /// Set a custom configuration
    pub fn with_config(mut self, config: PaneControlsConfig) -> Self {
        self.config = config;
        self
    }

    /// Show pane controls at the top-right of the given pane rect
    /// Only shows if pane is hovered
    /// Returns any action triggered
    pub fn show(
        &self,
        ui: &mut Ui,
        pane_rect: Rect,
        pane_id: PaneId,
        pane_hovered: bool,
    ) -> PaneControlAction {
        if !pane_hovered {
            return PaneControlAction::None;
        }

        let theme = ui.active_theme();
        let defaults = PaneControlsConfig::default();
        let mut config = self.config.clone();
        for (value, default, themed) in [
            (
                &mut config.button_gap,
                defaults.button_gap,
                theme.spacing.xs,
            ),
            (&mut config.padding, defaults.padding, theme.spacing.xs),
            (&mut config.rounding, defaults.rounding, theme.radius.sm),
            (&mut config.offset_x, defaults.offset_x, theme.spacing.sm),
            (&mut config.offset_y, defaults.offset_y, theme.spacing.sm),
        ] {
            if *value == default {
                *value = themed;
            }
        }

        let mut action = PaneControlAction::None;

        // Get colors from theme
        let bg_color = ui.style().visuals.window_fill;
        let border_stroke = ui.style().visuals.window_stroke;
        let icon_color = ui.style().visuals.widgets.inactive.fg_stroke.color;
        let icon_hover_color = ui.style().visuals.widgets.hovered.fg_stroke.color;
        let btn_hover_bg = ui.style().visuals.widgets.hovered.bg_fill;

        // Button definitions: (icon, tooltip, action_fn)
        #[allow(clippy::type_complexity)]
        let buttons: [(&Icon, &str, fn(PaneId) -> PaneControlAction); 4] = [
            (
                &embedded_icons::SETTINGS,
                "Pane Settings",
                PaneControlAction::Settings,
            ),
            (
                &embedded_icons::REMOVE,
                "Minimize",
                PaneControlAction::Minimize,
            ),
            (
                &embedded_icons::FULLSCREEN,
                "Maximize",
                PaneControlAction::Maximize,
            ),
            (
                &embedded_icons::CLOSE,
                "Close Pane",
                PaneControlAction::Close,
            ),
        ];

        let num_buttons = buttons.len() as f32;
        let bar_width = (config.button_size * num_buttons)
            + (config.button_gap * (num_buttons - 1.0))
            + (config.padding * 2.0);
        let bar_height = config.button_size + (config.padding * 2.0);

        // Position at top-right of pane
        let bar_pos = Pos2::new(
            pane_rect.max.x - bar_width - config.offset_x,
            pane_rect.min.y + config.offset_y,
        );
        let bar_rect = Rect::from_min_size(bar_pos, Vec2::new(bar_width, bar_height));

        // Check if bar area is visible
        if !ui.is_rect_visible(bar_rect) {
            return PaneControlAction::None;
        }

        // Draw bar background
        ui.painter().rect(
            bar_rect,
            config.rounding,
            bg_color,
            border_stroke,
            egui::StrokeKind::Inside,
        );

        let mut x = bar_rect.min.x + config.padding;
        let center_y = bar_rect.center().y;
        let icon_size = 14.0;

        for (icon, tooltip, action_fn) in buttons {
            let btn_rect = Rect::from_center_size(
                Pos2::new(x + config.button_size / 2.0, center_y),
                Vec2::splat(config.button_size),
            );

            let btn_response = ui.allocate_rect(btn_rect, Sense::click());
            let translated_label = l10n::text(ui.ctx(), tooltip);
            btn_response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    btn_response.enabled(),
                    &translated_label,
                )
            });
            crate::styles::focus::draw_focus_ring(ui, &btn_response, ui.active_theme().radius.sm);
            let is_hovered = btn_response.hovered();
            let was_clicked = btn_response.clicked();

            // Button background on hover
            if is_hovered {
                ui.painter()
                    .rect_filled(btn_rect, ui.active_theme().radius.sm, btn_hover_bg);
            }

            // Icon
            let icon_rect = Rect::from_center_size(btn_rect.center(), Vec2::splat(icon_size));
            let icon_color_final = if is_hovered {
                icon_hover_color
            } else {
                icon_color
            };
            icon.as_image_tinted(Vec2::splat(icon_size), icon_color_final)
                .paint_at(ui, icon_rect);

            btn_response.on_hover_text(l10n::text(ui.ctx(), tooltip));

            if was_clicked {
                action = action_fn(pane_id);
            }

            x += config.button_size + config.button_gap;
        }

        action
    }
}
