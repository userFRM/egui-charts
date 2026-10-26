//! Bottom panel UI rendering
//!
//! Bottom toolbar with date range selection, timezone display,
//! session controls, and dividend adjustment.
//! Narrow bars keep date ranges in a menu beside the clock and session controls.

use super::{DateRange, SessionType, TimeframeToolbarAction, TimeframeToolbarState};
use crate::ext::UiExt;
use crate::icons::icons as embedded_icons;
use crate::localization as l10n;
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
            text_color: ui.text,
            hover_text_color: ui.text,
            active_text_color: ui.accent,
            separator_color: if theme.dark {
                ui.border
            } else {
                ui.panel_bg.lerp_to_gamma(ui.text, 0.085)
            },
            hover_bg: ui.btn_bg_hover,
            active_bg: ui.btn_bg_active,
            font_size: typography::LG,
            panel_height: DESIGN_TOKENS.sizing.toolbar.bottom_height,
            btn_padding_h: DESIGN_TOKENS.spacing.md,
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
        self.state.update_time();
        if !self.hidden.contains(&Control::TimeZone) {
            // Use the same instant for the label and the next whole-second deadline.
            let nanos = self.state.curr_time.unwrap().timestamp_subsec_nanos();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_nanos(
                    1_000_000_000 - u64::from(nanos),
                ));
        }

        // Background and border are handled by parent Frame in platform.rs
        ResponsiveToolbar::horizontal("timeframe_toolbar_scroll")
            .show(ui, |ui, _ctx| self.show_contents(ui))
    }

    /// Internal: render toolbar contents
    fn show_contents(&mut self, ui: &mut Ui) -> TimeframeToolbarAction {
        let mut result_action = TimeframeToolbarAction::None;

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.space_lg();

            // Left section: Date range btns
            let ranges = DateRange::default_presets();
            let ranges_width: f32 = ranges
                .iter()
                .map(|range| self.range_btn_width(ui, range.label()))
                .sum();
            let mut time_text = self.state.formatted_time();
            let mut reserved = DESIGN_TOKENS.spacing.lg;
            if !self.hidden.contains(&Control::TimeZone) {
                reserved += self.text_btn_width(ui, &time_text) + DESIGN_TOKENS.spacing.lg;
            }
            if !self.hidden.contains(&Control::Session) {
                reserved += self.text_btn_width(ui, self.state.session_type.label());
            }
            if !self.hidden.contains(&Control::DividendAdjustment) {
                reserved += 2.0 * DESIGN_TOKENS.spacing.sm
                    + stroke::HAIRLINE
                    + self.text_btn_width(ui, "ADJ");
            }
            let compact =
                self.range_btn_width(ui, "Ranges") + reserved + 48.0 > ui.available_width();
            if compact && !self.hidden.contains(&Control::TimeZone) {
                reserved -= self.text_btn_width(ui, &time_text);
                time_text.truncate(8);
                reserved += self.text_btn_width(ui, &time_text);
            }
            if ranges_width + reserved + 48.0 > ui.available_width() {
                let response = ui.add(
                    egui::Button::new(
                        egui::RichText::new(l10n::text(
                            ui.ctx(),
                            if compact { "…" } else { "Ranges" },
                        ))
                        .size(self.config.font_size),
                    )
                    .frame(false),
                );
                let response = response.on_hover_text(l10n::text(ui.ctx(), "Date ranges"));
                egui::Popup::menu(&response).show(|ui| {
                    for range in ranges {
                        if ui
                            .button(l10n::text(ui.ctx(), range.label()))
                            .on_hover_text(l10n::text(ui.ctx(), range.tooltip()))
                            .clicked()
                        {
                            self.state.set_date_range(range);
                            result_action = TimeframeToolbarAction::DateRangeChanged(range);
                            ui.close();
                        }
                    }
                });
            } else {
                for range in ranges {
                    if self
                        .date_range_btn(ui, range.label(), range.tooltip())
                        .clicked()
                    {
                        self.state.set_date_range(range);
                        result_action = TimeframeToolbarAction::DateRangeChanged(range);
                    }
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
            let remaining = ui.available_width() - reserved;
            if remaining > 0.0 {
                ui.add_space(remaining);
            }

            // Right section: Timezone, Session, Dividends
            // Timezone button with time
            if !self.hidden.contains(&Control::TimeZone) {
                if self
                    .text_btn(
                        ui,
                        &time_text,
                        &format!("Timezone: {}", self.state.timezone),
                        false,
                    )
                    .clicked()
                {
                    result_action = TimeframeToolbarAction::OpenTimezoneMenu;
                }
                ui.space_lg();
            }

            // RTH/Session button
            if !self.hidden.contains(&Control::Session) {
                let session_label = self.state.session_type.label();
                let session_tooltip = self.state.session_type.tooltip();
                if self
                    .text_btn(
                        ui,
                        session_label,
                        session_tooltip,
                        self.state.session_type == SessionType::RTH,
                    )
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
                    .text_btn(
                        ui,
                        "ADJ",
                        if adj_active {
                            "Dividend adjustment: on"
                        } else {
                            "Dividend adjustment: off"
                        },
                        adj_active,
                    )
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
    fn date_range_btn(&self, ui: &mut Ui, text: &str, tooltip: &str) -> Response {
        let font_id = FontId::proportional(self.config.font_size);

        let btn_width = self.range_btn_width(ui, text);
        let btn_height = self.config.panel_height - DESIGN_TOKENS.spacing.sm;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(btn_width, btn_height), Sense::click());

        // Colors based on state
        let (bg_color, text_color) = if response.hovered() {
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
            l10n::text(ui.ctx(), text),
            font_id,
            text_color,
        );

        crate::styles::focus::draw_focus_ring(ui, &response, DESIGN_TOKENS.rounding.md);
        let translated_label = l10n::text(ui.ctx(), tooltip);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &translated_label)
        });
        response.on_hover_text(l10n::text(ui.ctx(), tooltip))
    }

    fn range_btn_width(&self, ui: &Ui, text: &str) -> f32 {
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), text),
            FontId::proportional(self.config.font_size),
            self.config.text_color,
        );
        galley.size().x + self.config.btn_padding_h * 2.0
    }

    /// Width of a [`text_btn`](Self::text_btn) labelled `text`.
    fn text_btn_width(&self, ui: &Ui, text: &str) -> f32 {
        self.range_btn_width(ui, text)
            .max(DESIGN_TOKENS.sizing.button_md)
    }

    fn text_btn(&self, ui: &mut Ui, text: &str, tooltip: &str, is_active: bool) -> Response {
        let font_id = FontId::proportional(self.config.font_size);
        let btn_height = self.config.panel_height - DESIGN_TOKENS.spacing.sm;

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(self.text_btn_width(ui, text), btn_height),
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

        if is_active {
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 1.0,
                Stroke::new(2.0, self.config.active_text_color),
            );
        }

        // Draw text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            l10n::text(ui.ctx(), text),
            font_id,
            text_color,
        );

        crate::styles::focus::draw_focus_ring(ui, &response, DESIGN_TOKENS.rounding.md);
        let translated_label = l10n::text(ui.ctx(), tooltip);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                ui.is_enabled(),
                is_active,
                &translated_label,
            )
        });
        response.on_hover_text(l10n::text(ui.ctx(), tooltip))
    }

    /// Render the calendar icon button (Go to date)
    /// Uses the GoToDate SVG icon from the app's icon system
    fn icon_button_calendar(&self, ui: &mut Ui, tooltip: &str) -> Response {
        let icon_size = icon_sizes::LG;
        let btn_size = Vec2::new(
            DESIGN_TOKENS.sizing.button_md,
            self.config.panel_height - DESIGN_TOKENS.spacing.sm,
        );
        let (rect, response) = ui.allocate_exact_size(btn_size, Sense::click());

        // Draw hover background
        if self.state.date_picker_open || response.hovered() {
            ui.painter().rect_filled(
                rect,
                DESIGN_TOKENS.rounding.md,
                if self.state.date_picker_open {
                    self.config.active_bg
                } else {
                    self.config.hover_bg
                },
            );
        }

        // Center the icon in the button
        let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));

        // Render the icon with theme-aware coloring
        let icon_color = if self.state.date_picker_open {
            self.config.active_text_color
        } else if response.hovered() {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.noninteractive.fg_stroke.color
        };
        embedded_icons::GO_TO_DATE
            .as_image_tinted(Vec2::splat(icon_size), icon_color)
            .paint_at(ui, icon_rect);

        crate::styles::focus::draw_focus_ring(ui, &response, DESIGN_TOKENS.rounding.md);
        let translated_label = l10n::text(ui.ctx(), tooltip);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                ui.is_enabled(),
                self.state.date_picker_open,
                &translated_label,
            )
        });
        response.on_hover_text(l10n::text(ui.ctx(), tooltip))
    }

    /// Render a vertical separator
    fn separator(&self, ui: &mut Ui) {
        let height = 22.0;
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
    fn visible_clock_schedules_the_next_whole_second() {
        use chrono::Timelike;
        use std::time::Duration;
        for visible in [true, false] {
            let ctx = egui::Context::default();
            let mut toolbar = TimeframeToolbar::new();
            toolbar.set_control_visible(Control::TimeZone, visible);
            for frame in 0..5 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    toolbar.show(ui);
                });
                output.textures_delta.clear();
                if frame == 4 {
                    let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
                    let now = toolbar.state.curr_time.unwrap();
                    let boundary = now.with_nanosecond(0).unwrap() + chrono::Duration::seconds(1);
                    let expected = (boundary - now).to_std().unwrap();
                    assert_eq!(delay, if visible { expected } else { Duration::MAX });
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn active_modes_and_open_calendar_use_the_accent() {
        for theme in [Theme::light(), Theme::dark()] {
            for active in [false, true] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar =
                    TimeframeToolbar::with_config(TimeframeToolbarConfig::from_theme(&theme));
                toolbar.state.date_picker_open = active;
                let mut calendar = egui::Rect::NOTHING;
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    toolbar.text_btn(ui, "ADJ", "Dividend adjustment", active);
                    calendar = toolbar.icon_button_calendar(ui, "Go to").rect;
                });
                output.textures_delta.clear();
                assert_eq!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Rect(rect) if rect.rect == calendar && rect.fill == toolbar.config.active_bg)), active);
                let label = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "ADJ" => Some(text),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    label.galley.job.sections[0].format.color,
                    if active {
                        theme.ui.accent
                    } else {
                        theme.ui.text
                    }
                );
                assert_eq!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::LineSegment { stroke, .. } if stroke.width == 2.0 && stroke.color == theme.ui.accent)), active);
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn right_controls_stay_at_the_edge_when_text_sizes_or_visibility_change() {
        for (theme, width, font_size, timezone) in [
            (Theme::light(), 900.0, 12.0, "UTC"),
            (Theme::dark(), 900.0, 14.0, "UTC"),
            (Theme::light(), 600.0, 14.0, "America/New_York"),
            (Theme::dark(), 640.0, 14.0, "America/New_York"),
            (Theme::light(), 320.0, 14.0, "UTC"),
            (Theme::dark(), 320.0, 14.0, "America/New_York"),
            (Theme::light(), 320.0, 14.0, "+23:59:59"),
            (Theme::dark(), 320.0, 14.0, "-23:59:59"),
        ] {
            for hidden in [
                vec![],
                vec![Control::TimeZone],
                vec![Control::DividendAdjustment],
                vec![Control::Session, Control::DividendAdjustment],
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar =
                    TimeframeToolbar::with_config(TimeframeToolbarConfig::from_theme(&theme));
                toolbar.set_timezone(timezone.to_owned());
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
                let padding = DESIGN_TOKENS.spacing.lg
                    * if toolbar.hidden.contains(&Control::Session)
                        && toolbar.hidden.contains(&Control::DividendAdjustment)
                    {
                        2.0
                    } else {
                        1.0
                    };
                let mut expected = 0.0;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 40.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        expected = ui.available_rect_before_wrap().max.x - padding;
                        toolbar.show_contents(ui);
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.text() == last
                                || last.starts_with(&format!("{} ", text.galley.text())) =>
                        {
                            Some(text)
                        }
                        _ => None,
                    })
                    .expect("the last visible control must be drawn");
                expected -= (text.galley.size().x + 2.0 * toolbar.config.btn_padding_h)
                    .max(DESIGN_TOKENS.sizing.button_md)
                    / 2.0;
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
    fn ranges_have_no_resting_fill_and_can_be_requested_again() {
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut toolbar =
                TimeframeToolbar::with_config(TimeframeToolbarConfig::from_theme(&theme));
            let mut pos = egui::Pos2::ZERO;
            let mut requested = false;
            for pressed in [None, Some(true), Some(false)] {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 40.0),
                        )),
                        events: pressed
                            .map(|pressed| {
                                vec![
                                    egui::Event::PointerMoved(pos),
                                    egui::Event::PointerButton {
                                        pos,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: Default::default(),
                                    },
                                ]
                            })
                            .unwrap_or_default(),
                        ..Default::default()
                    },
                    |ui| {
                        requested |= toolbar.show_contents(ui)
                            == TimeframeToolbarAction::DateRangeChanged(DateRange::Month1);
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t) if t.galley.text() == "1M" => Some(t),
                        _ => None,
                    })
                    .expect("month range is visible");
                pos = text.pos + text.galley.size() / 2.0;
                let filled = output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::Rect(r) if r.rect.contains(pos) && r.fill != Color32::TRANSPARENT
                ));
                output.drop_without_applying_deltas();
                if pressed.is_none() {
                    assert!(
                        !filled,
                        "a remembered range must not claim the current viewport"
                    );
                }
            }
            assert!(
                requested,
                "a range can be requested after the view was panned"
            );
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
