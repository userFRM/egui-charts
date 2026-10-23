//! Bottom panel UI rendering
//!
//! Bottom toolbar with date range selection, timezone display,
//! session controls, and dividend adjustment.
//! On mobile: horizontally scrollable to access all controls.

use super::{DateRange, SessionType, TimeframeToolbarAction, TimeframeToolbarState};
use crate::ext::UiExt;
use crate::icons::icons as embedded_icons;
use crate::styles::{icons as icon_sizes, stroke, typography};
use crate::theme::{Theme, follow_active_theme, theme_derived};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui_kit::toolbar::ResponsiveToolbar;
use egui::{Color32, FontId, Response, Sense, Stroke, Ui, Vec2};
use std::collections::HashSet;

/// Bottom panel component
pub struct TimeframeToolbar {
    /// State
    pub state: TimeframeToolbarState,
    /// Configuration
    pub config: TimeframeToolbarConfig,
    /// Theme and config `config` was last derived from; `None` when the caller supplied it.
    config_theme: Option<(Theme, TimeframeToolbarConfig)>,
    /// Controls the host hid.
    hidden: HashSet<Control>,
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
    /// Create config from `theme`
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            background_color: Color32::TRANSPARENT, // Let parent Frame show through
            border_color: ui.border,
            text_color: ui.text_secondary,
            hover_text_color: ui.text,
            active_text_color: ui.text,
            separator_color: ui.border,
            hover_bg: ui.btn_bg_hover,
            active_bg: ui.btn_bg_active,
            font_size: typography::SM_MD,
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

theme_derived!(TimeframeToolbarConfig {
    background_color,
    border_color,
    text_color,
    hover_text_color,
    active_text_color,
    separator_color,
    hover_bg,
    active_bg,
    font_size,
    panel_height,
    btn_padding_h,
    btn_padding_v
});

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
            config_theme: Some((Theme::dark(), TimeframeToolbarConfig::default())),
            hidden: HashSet::new(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: TimeframeToolbarConfig) -> Self {
        Self {
            state: TimeframeToolbarState::default(),
            config,
            config_theme: None,
            hidden: HashSet::new(),
        }
    }

    /// Shows or hides `control`: TimeZone, Session and DividendAdjustment. The toolbar
    /// ignores the other controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.hidden, control, visible);
    }

    /// Update the current time display
    pub fn update_time(&mut self) {
        self.state.update_time();
    }

    /// Show bottom panel and return action
    pub fn show(&mut self, ui: &mut Ui) -> TimeframeToolbarAction {
        follow_active_theme(
            ui.ctx(),
            &mut self.config_theme,
            &mut self.config,
            TimeframeToolbarConfig::from_theme,
        );
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

            // Flexible spacer - push right section to the right. Hidden controls give back
            // their share of the reservation, so the rest stays flush right.
            let spacing = &DESIGN_TOKENS.spacing;
            let mut reserved = spacing.lg;
            if !self.hidden.contains(&Control::TimeZone) {
                reserved += self.text_btn_width(&self.state.formatted_time()) + spacing.lg;
            }
            if !self.hidden.contains(&Control::Session) {
                reserved += self.text_btn_width(self.state.session_type.label());
            }
            if !self.hidden.contains(&Control::DividendAdjustment) {
                reserved += 2.0 * spacing.sm + stroke::HAIRLINE + self.text_btn_width("ADJ");
            }
            let remaining = ui.available_width() - reserved;
            if remaining > 0.0 {
                ui.add_space(remaining);
            }

            // Right section: Timezone, Session, Dividends
            // Timezone button with time
            if !self.hidden.contains(&Control::TimeZone) {
                let time_text = self.state.formatted_time();
                if self.text_btn(ui, &time_text, "Timezone", false).clicked() {
                    result_action = TimeframeToolbarAction::OpenTimezoneMenu;
                }
                ui.space_lg();
            }

            // RTH/Session button
            if !self.hidden.contains(&Control::Session) {
                let session_label = self.state.session_type.label();
                let session_tooltip = self.state.session_type.tooltip();
                if self
                    .text_btn(ui, session_label, session_tooltip, false)
                    .clicked()
                {
                    result_action = TimeframeToolbarAction::OpenSessionMenu;
                }
            }

            // ADJ button, after a separator
            if !self.hidden.contains(&Control::DividendAdjustment) {
                ui.space_sm();
                self.separator(ui);
                ui.space_sm();

                let adj_active = self.state.adjust_for_dividends;
                if self
                    .text_btn(ui, "ADJ", "Adjust data for dividends", adj_active)
                    .clicked()
                {
                    self.state.adjust_for_dividends = !adj_active;
                    result_action = TimeframeToolbarAction::AdjustDividendsToggled(
                        self.state.adjust_for_dividends,
                    );
                }
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
    /// Width of a [`text_btn`](Self::text_btn) labelled `text`.
    fn text_btn_width(&self, text: &str) -> f32 {
        // ~0.6 * font_size per character for proportional font
        let char_width = self.config.font_size * 0.6;
        let text_size = char_width * text.len() as f32;
        (text_size + self.config.btn_padding_h * 2.0).max(DESIGN_TOKENS.sizing.button_xl)
    }

    fn text_btn(&self, ui: &mut Ui, text: &str, tooltip: &str, is_active: bool) -> Response {
        let font_id = FontId::proportional(self.config.font_size);
        let btn_height = self.config.panel_height - DESIGN_TOKENS.spacing.sm;

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(self.text_btn_width(text), btn_height),
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
        let icon_size = icon_sizes::SM;
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
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.noninteractive.fg_stroke.color
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
    fn right_controls_stay_at_the_edge_when_text_sizes_or_visibility_change() {
        for font_size in [12.0, 14.0] {
            for hidden in [
                vec![],
                vec![Control::TimeZone],
                vec![Control::DividendAdjustment],
                vec![Control::Session, Control::DividendAdjustment],
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &Theme::dark());
                let mut toolbar = TimeframeToolbar::new();
                toolbar.config.font_size = font_size;
                toolbar.hidden.extend(hidden);
                let last = if !toolbar.hidden.contains(&Control::DividendAdjustment) {
                    "ADJ".to_owned()
                } else if !toolbar.hidden.contains(&Control::Session) {
                    toolbar.state.session_type.label().to_owned()
                } else {
                    toolbar.state.formatted_time()
                };
                // The clock also has a trailing gap when it is the last control.
                let padding =
                    DESIGN_TOKENS.spacing.lg * if last.ends_with("UTC") { 2.0 } else { 1.0 };
                let mut expected = 0.0;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 40.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        expected = ui.available_rect_before_wrap().max.x
                            - padding
                            - toolbar.text_btn_width(&last) / 2.0;
                        toolbar.show_contents(ui);
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == last => Some(text),
                        _ => None,
                    })
                    .expect("the last visible control must be drawn");
                let center = text.pos.x + text.galley.size().x / 2.0;
                output.drop_without_applying_deltas();
                assert!(
                    (center - expected).abs() < 1.0,
                    "{font_size}, {:?}: {center} != {expected}",
                    toolbar.hidden
                );
            }
        }
    }

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
    }
}
