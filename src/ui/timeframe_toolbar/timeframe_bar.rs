//! Bottom Timeframe Quick-Select Bar
//!
//! Shows btns for quick timeframe switching (1D, 5D, 1M, 3M, 6M, YTD, 1Y, 5Y, All)

use crate::icons::icons as embedded_icons;
use crate::styles::typography;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use chrono::{Local, Timelike};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Quick timeframe options
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickTimeframe {
    /// 1 Day
    OneDay,
    /// 5 Days
    FiveDays,
    /// 1 Month
    OneMonth,
    /// 3 Months
    ThreeMonths,
    /// 6 Months
    SixMonths,
    /// Year to Date
    YearToDate,
    /// 1 Year
    OneYear,
    /// 5 Years
    FiveYears,
    /// All available data
    All,
}

impl QuickTimeframe {
    pub fn label(&self) -> &'static str {
        match self {
            QuickTimeframe::OneDay => "1D",
            QuickTimeframe::FiveDays => "5D",
            QuickTimeframe::OneMonth => "1M",
            QuickTimeframe::ThreeMonths => "3M",
            QuickTimeframe::SixMonths => "6M",
            QuickTimeframe::YearToDate => "YTD",
            QuickTimeframe::OneYear => "1Y",
            QuickTimeframe::FiveYears => "5Y",
            QuickTimeframe::All => "All",
        }
    }
}

/// Configuration for the timeframe bar
#[derive(Clone, Debug)]
pub struct TimeframeBarConfig {
    pub height: f32,
    pub bg_color: Color32,
    pub text_color: Color32,
    pub active_text_color: Color32,
    pub active_bg: Color32,
    pub hover_bg: Color32,
    pub border_color: Color32,
    pub btn_padding: f32,
}

impl TimeframeBarConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.semantic.ui;
        Self {
            height: DESIGN_TOKENS.sizing.toolbar.bottom_height,
            bg_color: Color32::TRANSPARENT, // Let parent Frame show through
            text_color: ui.text_secondary,
            active_text_color: ui.accent,
            active_bg: ui.btn_bg_active,
            hover_bg: ui.btn_bg_hover,
            border_color: ui.border,
            btn_padding: DESIGN_TOKENS.spacing.lg,
        }
    }
}

impl Default for TimeframeBarConfig {
    fn default() -> Self {
        // Default uses light UI chrome theme
        Self::from_theme(&Theme::dark())
    }
}

/// Action returned by the timeframe bar
#[derive(Clone, Debug, PartialEq)]
pub enum TimeframeBarAction {
    /// No action taken
    None,
    /// A quick timeframe was selected
    TimeframeSelected(QuickTimeframe),
    /// The date range selector icon was clicked
    DateRangeClicked,
    /// Dividend adjustment toggled (new state)
    AdjToggled(bool),
}

/// Bottom Timeframe Quick-Select Bar
pub struct TimeframeBar {
    pub config: TimeframeBarConfig,
    pub active_timeframe: QuickTimeframe,
    pub show_adj: bool,
    pub adj_enabled: bool,
}

impl Default for TimeframeBar {
    fn default() -> Self {
        Self {
            config: TimeframeBarConfig::default(),
            active_timeframe: QuickTimeframe::OneDay,
            show_adj: true,
            adj_enabled: true,
        }
    }
}

impl TimeframeBar {
    /// Create a new timeframe bar with default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom configuration
    pub fn with_config(mut self, config: TimeframeBarConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the initially active timeframe
    pub fn with_active_timeframe(mut self, tf: QuickTimeframe) -> Self {
        self.active_timeframe = tf;
        self
    }

    /// Show the timeframe bar
    pub fn show(&mut self, ui: &mut Ui) -> TimeframeBarAction {
        let mut action = TimeframeBarAction::None;

        let (response, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), self.config.height),
            Sense::hover(),
        );
        let rect = response.rect;

        // Background - REMOVED: Let parent Frame's fill show through
        // painter.rect_filled(rect, 0.0, self.config.bg_color);

        // Top border
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, self.config.border_color),
        );

        let mut x = rect.min.x + 8.0;
        let center_y = rect.center().y;

        // Date range selector icon
        let date_icon_rect = Rect::from_center_size(
            Pos2::new(x + 10.0, center_y),
            Vec2::splat(DESIGN_TOKENS.sizing.icon_md),
        );
        let date_res = ui.allocate_rect(date_icon_rect, Sense::click());
        if date_res.hovered() {
            painter.rect_filled(
                date_icon_rect,
                DESIGN_TOKENS.rounding.sm,
                self.config.hover_bg,
            );
        }
        // Render calendar icon
        let icon_size = 12.0;
        let icon_rect = Rect::from_center_size(date_icon_rect.center(), Vec2::splat(icon_size));
        embedded_icons::WIDGET_BAR_CALENDAR
            .as_image_tinted(Vec2::splat(icon_size), self.config.text_color)
            .paint_at(ui, icon_rect);
        if date_res.clicked() {
            action = TimeframeBarAction::DateRangeClicked;
        }

        x += 28.0;

        // Separator
        painter.line_segment(
            [
                Pos2::new(x, rect.min.y + 4.0),
                Pos2::new(x, rect.max.y - 4.0),
            ],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, self.config.border_color),
        );
        x += 8.0;

        // Timeframe btns
        let timeframes = [
            QuickTimeframe::OneDay,
            QuickTimeframe::FiveDays,
            QuickTimeframe::OneMonth,
            QuickTimeframe::ThreeMonths,
            QuickTimeframe::SixMonths,
            QuickTimeframe::YearToDate,
            QuickTimeframe::OneYear,
            QuickTimeframe::FiveYears,
            QuickTimeframe::All,
        ];

        for tf in timeframes.iter() {
            let label = tf.label();
            let galley = painter.layout_no_wrap(
                label.to_string(),
                egui::FontId::proportional(typography::SM),
                self.config.text_color,
            );

            let btn_width = galley.size().x + self.config.btn_padding * 2.0;
            let btn_rect = Rect::from_min_size(
                Pos2::new(x, rect.min.y + 4.0),
                Vec2::new(btn_width, self.config.height - 8.0),
            );

            let btn_res = ui.allocate_rect(btn_rect, Sense::click());
            let is_active = *tf == self.active_timeframe;
            let is_hovered = btn_res.hovered();
            let is_pressed = btn_res.is_pointer_button_down_on();

            // Button background (with press feedback)
            if is_active {
                painter.rect_filled(btn_rect, DESIGN_TOKENS.rounding.sm, self.config.active_bg);
            } else if is_pressed {
                // Pressed state - use active background
                painter.rect_filled(btn_rect, DESIGN_TOKENS.rounding.sm, self.config.active_bg);
            } else if is_hovered {
                painter.rect_filled(btn_rect, DESIGN_TOKENS.rounding.sm, self.config.hover_bg);
            }

            // Button text
            let text_color = if is_active || is_pressed {
                self.config.active_text_color
            } else {
                self.config.text_color
            };

            let text_galley = painter.layout_no_wrap(
                label.to_string(),
                egui::FontId::proportional(typography::SM),
                text_color,
            );
            painter.galley(
                Pos2::new(
                    btn_rect.center().x - text_galley.size().x / 2.0,
                    center_y - text_galley.size().y / 2.0,
                ),
                text_galley,
                Color32::TRANSPARENT,
            );

            if btn_res.clicked() {
                self.active_timeframe = *tf;
                action = TimeframeBarAction::TimeframeSelected(*tf);
            }

            x += btn_width + 4.0;
        }

        // Right side - current time and ADJ toggle
        let right_x = rect.max.x - 8.0;

        // ADJ toggle
        if self.show_adj {
            let adj_text = "ADJ";
            let adj_galley = painter.layout_no_wrap(
                adj_text.to_string(),
                egui::FontId::proportional(typography::XS),
                if self.adj_enabled {
                    self.config.active_text_color
                } else {
                    self.config.text_color
                },
            );
            let adj_button_rect = Rect::from_min_size(
                Pos2::new(
                    right_x - adj_galley.size().x - 12.0 - 70.0,
                    rect.min.y + 4.0,
                ),
                Vec2::new(adj_galley.size().x + 12.0, self.config.height - 8.0),
            );
            let adj_res = ui.allocate_rect(adj_button_rect, Sense::click());

            if adj_res.hovered() || self.adj_enabled {
                painter.rect_filled(
                    adj_button_rect,
                    DESIGN_TOKENS.rounding.sm,
                    self.config.active_bg,
                );
            }

            let adj_draw_galley = painter.layout_no_wrap(
                adj_text.to_string(),
                egui::FontId::proportional(typography::XS),
                if self.adj_enabled {
                    self.config.active_text_color
                } else {
                    self.config.text_color
                },
            );
            painter.galley(
                Pos2::new(
                    adj_button_rect.center().x - adj_draw_galley.size().x / 2.0,
                    center_y - adj_draw_galley.size().y / 2.0,
                ),
                adj_draw_galley,
                Color32::TRANSPARENT,
            );

            if adj_res.clicked() {
                self.adj_enabled = !self.adj_enabled;
                action = TimeframeBarAction::AdjToggled(self.adj_enabled);
            }
        }

        // Current time (UTC)
        let now = Local::now();
        let time_str = format!(
            "{:02}:{:02}:{:02} UTC",
            now.hour(),
            now.minute(),
            now.second()
        );
        let time_galley = painter.layout_no_wrap(
            time_str,
            egui::FontId::proportional(typography::XS),
            self.config.text_color,
        );
        painter.galley(
            Pos2::new(
                right_x - time_galley.size().x,
                center_y - time_galley.size().y / 2.0,
            ),
            time_galley,
            Color32::TRANSPARENT,
        );

        action
    }

    pub fn set_active_timeframe(&mut self, tf: QuickTimeframe) {
        self.active_timeframe = tf;
    }
}
