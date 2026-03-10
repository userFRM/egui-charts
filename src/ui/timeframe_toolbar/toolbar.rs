//! Bottom panel UI rendering
//!
//! Bottom toolbar with date range selection, timezone display,
//! session controls, and dividend adjustment.
//! On mobile: horizontally scrollable to access all controls.

use super::{DateRange, SessionType, TimeframeToolbarAction, TimeframeToolbarState};
use crate::ext::UiExt;
use crate::icons::icons as embedded_icons;
use crate::styles::{icons as icon_sizes, stroke, typography};
use crate::theme::Theme;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::toolbar::ResponsiveToolbar;
use egui::{Color32, FontId, Response, Sense, Stroke, Ui, Vec2};

/// Bottom panel component
pub struct TimeframeToolbar {
    /// State
    pub state: TimeframeToolbarState,
    /// Configuration
    pub config: TimeframeToolbarConfig,
}

/// Configuration for the bottom panel appearance
#[derive(Debug, Clone)]
pub struct TimeframeToolbarConfig {
    /// Background color
    pub background_color: Color32,
    /// Border color (top border)
    pub border_color: Color32,
    /// Text color (inactive)
    pub text_color: Color32,
    /// Text color (hover)
    pub hover_text_color: Color32,
    /// Text color (active/selected)
    pub active_text_color: Color32,
    /// Separator color
    pub separator_color: Color32,
    /// Button hover background
    pub hover_bg: Color32,
    /// Button active background
    pub active_bg: Color32,
    /// Font size
    pub font_size: f32,
    /// Panel height
    pub panel_height: f32,
    /// Button padding horizontal
    pub btn_padding_h: f32,
    /// Button padding vertical
    pub btn_padding_v: f32,
}

impl TimeframeToolbarConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.semantic.ui;
        Self {
            background_color: Color32::TRANSPARENT, // Let parent Frame show through
            border_color: ui.border,
            text_color: ui.text_secondary,
            hover_text_color: ui.text,
            active_text_color: ui.text,
            separator_color: ui.border_subtle,
            hover_bg: ui.btn_bg_hover,
            active_bg: ui.btn_bg_active,
            font_size: typography::LG,
            panel_height: DESIGN_TOKENS.sizing.toolbar.bottom_height,
            btn_padding_h: DESIGN_TOKENS.spacing.lg,
            btn_padding_v: DESIGN_TOKENS.spacing.sm,
        }
    }
}

impl Default for TimeframeToolbarConfig {
    fn default() -> Self {
        // Default uses light UI chrome theme
        Self::from_theme(&Theme::dark())
    }
}

impl Default for TimeframeToolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeframeToolbar {
    /// Create new bottom panel
    pub fn new() -> Self {
        Self {
            state: TimeframeToolbarState::default(),
            config: TimeframeToolbarConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: TimeframeToolbarConfig) -> Self {
        Self {
            state: TimeframeToolbarState::default(),
            config,
        }
    }

    /// Update the current time display
    pub fn update_time(&mut self) {
        self.state.update_time();
    }

    /// Show bottom panel and return action
    pub fn show(&mut self, ui: &mut Ui) -> TimeframeToolbarAction {
        // Update time on each frame
        self.state.update_time();

        // Background and border are handled by parent Frame in platform.rs
        ResponsiveToolbar::horizontal("timeframe_toolbar_scroll")
            .show(ui, |ui, _ctx| self.show_contents(ui))
    }

    /// Internal: render toolbar contents
    fn show_contents(&mut self, ui: &mut Ui) -> TimeframeToolbarAction {
        let mut result_action = TimeframeToolbarAction::None;

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.space_sm();

            // Left section: Date range btns
            let ranges = DateRange::default_presets();
            for range in ranges {
                let is_sel = self.state.sel_range == range;
                if self
                    .date_range_btn(ui, range.label(), range.tooltip(), is_sel)
                    .clicked()
                    && !is_sel
                {
                    self.state.sel_range = range;
                    result_action = TimeframeToolbarAction::DateRangeChanged(range);
                }
            }

            // Separator
            ui.space_lg();
            self.separator(ui);
            ui.space_sm();

            // Go to date button with calendar icon
            if self.icon_button_calendar(ui, "Go to").clicked() {
                result_action = TimeframeToolbarAction::OpenDatePicker;
            }

            // Flexible spacer - push right section to the right
            let remaining =
                ui.available_width() - DESIGN_TOKENS.sizing.timeframe_toolbar.right_section_width;
            if remaining > 0.0 {
                ui.add_space(remaining);
            }

            // Right section: Timezone, Session, Dividends
            // Timezone button with time
            let time_text = self.state.formatted_time();
            if self.text_btn(ui, &time_text, "Timezone", false).clicked() {
                result_action = TimeframeToolbarAction::OpenTimezoneMenu;
            }

            ui.space_lg();

            // RTH/Session button
            let session_label = self.state.session_type.label();
            let session_tooltip = self.state.session_type.tooltip();
            if self
                .text_btn(ui, session_label, session_tooltip, false)
                .clicked()
            {
                result_action = TimeframeToolbarAction::OpenSessionMenu;
            }

            // Separator before ADJ
            ui.space_sm();
            self.separator(ui);
            ui.space_sm();

            // ADJ button
            let adj_active = self.state.adjust_for_dividends;
            if self
                .text_btn(ui, "ADJ", "Adjust data for dividends", adj_active)
                .clicked()
            {
                self.state.adjust_for_dividends = !adj_active;
                result_action =
                    TimeframeToolbarAction::AdjustDividendsToggled(self.state.adjust_for_dividends);
            }

            ui.space_lg();
        });

        result_action
    }

    /// Render a date range button (compact style)
    fn date_range_btn(&self, ui: &mut Ui, text: &str, tooltip: &str, is_sel: bool) -> Response {
        let font_id = FontId::proportional(self.config.font_size);

        // Calculate size (~0.6 * font_size per character for proportional font)
        let char_width = self.config.font_size * 0.6;
        let text_size = char_width * text.len() as f32;
        let btn_width = text_size + self.config.btn_padding_h * 2.0;
        let btn_height = self.config.panel_height - DESIGN_TOKENS.spacing.sm;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(btn_width, btn_height), Sense::click());

        // Colors based on state
        let (bg_color, text_color) = if is_sel {
            (self.config.active_bg, self.config.active_text_color)
        } else if response.hovered() {
            (self.config.hover_bg, self.config.hover_text_color)
        } else {
            (Color32::TRANSPARENT, self.config.text_color)
        };

        // Draw background on hover/active
        if bg_color != Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);
        }

        // Draw text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            text_color,
        );

        response.on_hover_text(tooltip)
    }

    /// Render a text button
    fn text_btn(&self, ui: &mut Ui, text: &str, tooltip: &str, is_active: bool) -> Response {
        let font_id = FontId::proportional(self.config.font_size);

        // Calculate size (~0.6 * font_size per character for proportional font)
        let char_width = self.config.font_size * 0.6;
        let text_size = char_width * text.len() as f32;
        let btn_width = text_size + self.config.btn_padding_h * 2.0;
        let btn_height = self.config.panel_height - DESIGN_TOKENS.spacing.sm;

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(btn_width.max(DESIGN_TOKENS.sizing.button_xl), btn_height),
            Sense::click(),
        );

        // Colors based on state
        let (bg_color, text_color) = if is_active {
            (self.config.active_bg, self.config.active_text_color)
        } else if response.hovered() {
            (self.config.hover_bg, self.config.hover_text_color)
        } else {
            (Color32::TRANSPARENT, self.config.text_color)
        };

        // Draw background
        if bg_color != Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);
        }

        // Draw text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            text_color,
        );

        response.on_hover_text(tooltip)
    }

    /// Render the calendar icon button (Go to date)
    /// Uses the GoToDate SVG icon from the app's icon system
    fn icon_button_calendar(&self, ui: &mut Ui, tooltip: &str) -> Response {
        let icon_size = icon_sizes::BOTTOM_TOOLBAR;
        let btn_size = Vec2::new(
            DESIGN_TOKENS.sizing.button_md,
            self.config.panel_height - DESIGN_TOKENS.spacing.sm,
        );
        let (rect, response) = ui.allocate_exact_size(btn_size, Sense::click());

        // Draw hover background
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.md, self.config.hover_bg);
        }

        // Center the icon in the button
        let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));

        // Render the icon with theme-aware coloring
        let icon_color = if response.hovered() {
            theming::icon_hover_color(ui)
        } else {
            theming::icon_normal(ui)
        };
        embedded_icons::GO_TO_DATE
            .as_image_tinted(Vec2::splat(icon_size), icon_color)
            .paint_at(ui, icon_rect);

        response.on_hover_text(tooltip)
    }

    /// Render a vertical separator
    fn separator(&self, ui: &mut Ui) {
        let height = self.config.panel_height - DESIGN_TOKENS.spacing.xl;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(stroke::HAIRLINE, height), Sense::hover());
        ui.painter().vline(
            rect.center().x,
            rect.y_range(),
            Stroke::new(stroke::HAIRLINE, self.config.separator_color),
        );
    }

    /// Show bottom panel (without action)
    pub fn ui(&mut self, ui: &mut Ui) -> Response {
        let _ = self.show(ui);
        ui.response()
    }

    /// Get the current state
    pub fn state(&self) -> &TimeframeToolbarState {
        &self.state
    }

    /// Get mutable state
    pub fn state_mut(&mut self) -> &mut TimeframeToolbarState {
        &mut self.state
    }

    /// Set the selected date range
    pub fn set_date_range(&mut self, range: DateRange) {
        self.state.sel_range = range;
    }

    /// Set the timezone
    pub fn set_timezone(&mut self, timezone: String) {
        self.state.timezone = timezone;
    }

    /// Set the session type
    pub fn set_session(&mut self, session: SessionType) {
        self.state.session_type = session;
    }

    /// Toggle dividend adjustment
    pub fn toggle_dividends(&mut self) {
        self.state.adjust_for_dividends = !self.state.adjust_for_dividends;
    }

    /// Sync from application state.
    pub fn sync_from_app_state(&mut self, app_state: &dyn crate::ui::app_state::ChartAppState) {
        self.state.set_timeframe(*app_state.active_timeframe());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_toolbar_creation() {
        let panel = TimeframeToolbar::new();
        assert_eq!(panel.state.sel_range, DateRange::Month1);
        assert_eq!(panel.state.session_type, SessionType::RTH);
        assert!(!panel.state.adjust_for_dividends);
    }

    #[test]
    fn test_config_defaults() {
        let config = TimeframeToolbarConfig::default();
        assert_eq!(
            config.panel_height,
            DESIGN_TOKENS.sizing.toolbar.bottom_height
        );
        assert_eq!(config.font_size, typography::LG);
    }
}
