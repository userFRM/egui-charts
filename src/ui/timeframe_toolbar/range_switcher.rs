//! Range Switcher Widget
//!
//! Provides quick date range selection buttons: 1D, 5D, 1M, 6M, YTD, 1Y, 5Y, All
use crate::styles::typography;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use egui::{Color32, FontId, Pos2, Response, Stroke, Ui, Vec2};

/// Date range presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DateRange {
    /// 1 Day
    Day1,
    /// 5 Days
    Day5,
    /// 1 Month
    #[default]
    Month1,
    /// 3 Months
    Month3,
    /// 6 Months
    Month6,
    /// Year to Date (from Jan 1 of current year)
    YTD,
    /// 1 Year
    Year1,
    /// 5 Years
    Year5,
    /// All available data
    All,
}

impl DateRange {
    /// Get display label for the range
    pub fn label(&self) -> &'static str {
        match self {
            DateRange::Day1 => "1D",
            DateRange::Day5 => "5D",
            DateRange::Month1 => "1M",
            DateRange::Month3 => "3M",
            DateRange::Month6 => "6M",
            DateRange::YTD => "YTD",
            DateRange::Year1 => "1Y",
            DateRange::Year5 => "5Y",
            DateRange::All => "All",
        }
    }

    /// Get tooltip desc for the range
    pub fn tooltip(&self) -> &'static str {
        match self {
            DateRange::Day1 => "1 day in 1 minute intervals",
            DateRange::Day5 => "5 days in 5 minutes intervals",
            DateRange::Month1 => "1 month in 30 minutes intervals",
            DateRange::Month3 => "3 months in 1 hour intervals",
            DateRange::Month6 => "6 months in 2 hours intervals",
            DateRange::YTD => "Year to day in 1 day intervals",
            DateRange::Year1 => "1 year in 1 day intervals",
            DateRange::Year5 => "5 years in 1 week intervals",
            DateRange::All => "All data in 1 month intervals",
        }
    }

    /// Get all standard ranges in order
    pub fn default_presets() -> Vec<DateRange> {
        vec![
            DateRange::Day1,
            DateRange::Day5,
            DateRange::Month1,
            DateRange::Month3,
            DateRange::Month6,
            DateRange::YTD,
            DateRange::Year1,
            DateRange::Year5,
            DateRange::All,
        ]
    }

    /// Calculate the start ts for this range
    /// Returns None for DateRange::All (meaning "from earliest available")
    pub fn start_timestamp(&self) -> Option<DateTime<Utc>> {
        let now = Utc::now();

        match self {
            DateRange::Day1 => Some(now - Duration::days(1)),
            DateRange::Day5 => Some(now - Duration::days(5)),
            DateRange::Month1 => Some(now - Duration::days(30)),
            DateRange::Month3 => Some(now - Duration::days(90)),
            DateRange::Month6 => Some(now - Duration::days(180)),
            DateRange::YTD => {
                // Start of current year
                let year_start = NaiveDate::from_ymd_opt(now.year(), 1, 1)?;
                Some(DateTime::from_naive_utc_and_offset(
                    year_start.and_hms_opt(0, 0, 0)?,
                    Utc,
                ))
            }
            DateRange::Year1 => Some(now - Duration::days(365)),
            DateRange::Year5 => Some(now - Duration::days(365 * 5)),
            DateRange::All => None,
        }
    }

    /// Get the date range as (start, end) ts
    /// Returns (None, now) for All (meaning earliest to now)
    pub fn to_timestamps(&self) -> (Option<DateTime<Utc>>, DateTime<Utc>) {
        (self.start_timestamp(), Utc::now())
    }

    /// Calculate approximate number of bars needed for this range at a given timeframe
    /// Useful for fetching appropriate amount of historical data
    pub fn estimated_bars(&self, bar_duration_seconds: u64) -> usize {
        let duration_seconds = match self {
            DateRange::Day1 => 24 * 3600,
            DateRange::Day5 => 5 * 24 * 3600,
            DateRange::Month1 => 30 * 24 * 3600,
            DateRange::Month3 => 90 * 24 * 3600,
            DateRange::Month6 => 180 * 24 * 3600,
            DateRange::YTD => {
                let now = Utc::now();
                let days_since_year_start = now.ordinal() as u64;
                days_since_year_start * 24 * 3600
            }
            DateRange::Year1 => 365 * 24 * 3600,
            DateRange::Year5 => 5 * 365 * 24 * 3600,
            DateRange::All => 10 * 365 * 24 * 3600, // Estimate: 10 years
        };

        (duration_seconds / bar_duration_seconds.max(1)) as usize
    }
}

impl std::fmt::Display for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Configuration for the Range Switcher widget
#[derive(Debug, Clone)]
pub struct RangeSwitcherConfig {
    /// Available date ranges to show
    pub ranges: Vec<DateRange>,
    /// Button height in pixels
    pub btn_height: f32,
    /// Button min width in pixels
    pub btn_min_width: f32,
    /// Spacing between btns
    pub spacing: f32,
    /// Background color (inactive button)
    pub background_color: Color32,
    /// Active button background color
    pub active_color: Color32,
    /// Hover background color
    pub hover_color: Color32,
    /// Text color (inactive)
    pub text_color: Color32,
    /// Text color (active)
    pub active_text_color: Color32,
    /// Border radius
    pub border_radius: f32,
    /// Font size
    pub font_size: f32,
    /// Show border around btns
    pub show_border: bool,
    /// Border color
    pub border_color: Color32,
}

impl RangeSwitcherConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.semantic.ui;
        Self {
            ranges: DateRange::default_presets(),
            btn_height: DESIGN_TOKENS.sizing.button_sm,
            btn_min_width: DESIGN_TOKENS.sizing.button_md + DESIGN_TOKENS.spacing.sm, // ~32px
            spacing: DESIGN_TOKENS.spacing.xs,
            background_color: Color32::TRANSPARENT, // Let parent show through
            active_color: ui.btn_bg_active,
            hover_color: ui.btn_bg_hover,
            text_color: ui.text_secondary,
            active_text_color: ui.text,
            border_radius: DESIGN_TOKENS.rounding.md,
            font_size: typography::SM,
            show_border: true,
            border_color: ui.border,
        }
    }

    /// Create config with standard presets
    pub fn standard() -> Self {
        Self::default()
    }

    /// Create config with minimal presets (1D, 1W, 1M, 1Y)
    pub fn minimal() -> Self {
        Self {
            ranges: vec![
                DateRange::Day1,
                DateRange::Month1,
                DateRange::Year1,
                DateRange::All,
            ],
            ..Default::default()
        }
    }

    /// Create config with custom ranges
    pub fn with_ranges(ranges: Vec<DateRange>) -> Self {
        Self {
            ranges,
            ..Default::default()
        }
    }
}

impl Default for RangeSwitcherConfig {
    fn default() -> Self {
        // Default uses light UI chrome theme
        Self::from_theme(&Theme::dark())
    }
}

/// Range Switcher Widget
///
/// Displays horizontal row of date range btns (1D, 5D, 1M, etc.)
/// Returns the selected range when a button is clicked.
pub struct RangeSwitcher {
    /// Currently selected range
    selected: DateRange,
    /// Configuration
    config: RangeSwitcherConfig,
    /// Unique ID for the widget
    id: egui::Id,
}

impl Default for RangeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeSwitcher {
    /// Create a new range switcher with default config
    pub fn new() -> Self {
        Self {
            selected: DateRange::Month1,
            config: RangeSwitcherConfig::default(),
            id: egui::Id::new("range_switcher"),
        }
    }

    /// Create with custom config
    pub fn with_config(config: RangeSwitcherConfig) -> Self {
        Self {
            selected: config.ranges.first().copied().unwrap_or(DateRange::Month1),
            config,
            id: egui::Id::new("range_switcher"),
        }
    }

    /// Create with custom ID (for multiple instances)
    pub fn with_id(mut self, id: impl std::hash::Hash) -> Self {
        self.id = egui::Id::new(id);
        self
    }

    /// Set the selected range
    pub fn set_sel(&mut self, range: DateRange) {
        self.selected = range;
    }

    /// Get the currently selected range
    pub fn selected(&self) -> DateRange {
        self.selected
    }

    /// Calculate the total width needed for the widget
    pub fn preferred_width(&self, _ui: &Ui) -> f32 {
        let mut total_width = 0.0;

        for range in &self.config.ranges {
            // Estimate width based on character count (roughly 7px per char at 11pt)
            let char_width = self.config.font_size * 0.6;
            let text_width = char_width * range.label().len() as f32;
            total_width += text_width.max(self.config.btn_min_width) + 16.0; // padding
            total_width += self.config.spacing;
        }

        total_width - self.config.spacing // Remove last spacing
    }

    /// Render the range switcher and return the newly selected range (if changed)
    pub fn show(&mut self, ui: &mut Ui) -> Option<DateRange> {
        let mut new_selection = None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = self.config.spacing;

            for &range in &self.config.ranges {
                let is_sel = self.selected == range;
                let response = self.render_btn(ui, range, is_sel);

                if response.clicked() && !is_sel {
                    self.selected = range;
                    new_selection = Some(range);
                }
            }
        });

        new_selection
    }

    /// Render at a specific position (for overlay positioning)
    pub fn show_at(&mut self, ui: &mut Ui, pos: Pos2) -> Option<DateRange> {
        let mut new_selection = None;

        egui::Area::new(self.id)
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE.fill(Color32::TRANSPARENT).show(ui, |ui| {
                    new_selection = self.show(ui);
                });
            });

        new_selection
    }

    /// Render a single button
    fn render_btn(&self, ui: &mut Ui, range: DateRange, is_sel: bool) -> Response {
        let label = range.label();
        let font_id = FontId::proportional(self.config.font_size);

        // Calculate button size (estimate width based on character count)
        let char_width = self.config.font_size * 0.6;
        let text_width = char_width * label.len() as f32;
        let btn_width = (text_width + 16.0).max(self.config.btn_min_width);
        let btn_size = Vec2::new(btn_width, self.config.btn_height);

        // Allocate space and get response
        let (rect, response) = ui.allocate_exact_size(btn_size, egui::Sense::click());

        // Determine colors based on state
        let (bg_color, text_color) = if is_sel {
            (self.config.active_color, self.config.active_text_color)
        } else if response.hovered() {
            (self.config.hover_color, self.config.text_color)
        } else {
            (self.config.background_color, self.config.text_color)
        };

        // Draw background
        ui.painter()
            .rect_filled(rect, self.config.border_radius, bg_color);

        // Draw border if enabled
        if self.config.show_border {
            ui.painter().rect_stroke(
                rect,
                self.config.border_radius,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, self.config.border_color),
                egui::epaint::StrokeKind::Inside,
            );
        }

        // Draw text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font_id,
            text_color,
        );

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_range_labels() {
        assert_eq!(DateRange::Day1.label(), "1D");
        assert_eq!(DateRange::YTD.label(), "YTD");
        assert_eq!(DateRange::All.label(), "All");
    }

    #[test]
    fn test_standard_presets() {
        let presets = DateRange::default_presets();
        assert_eq!(presets.len(), 9);
        assert_eq!(presets[0], DateRange::Day1);
        assert_eq!(presets[8], DateRange::All);
    }

    #[test]
    fn test_estimated_bars() {
        // 1 minute bars
        let minute_bars = DateRange::Day1.estimated_bars(60);
        assert_eq!(minute_bars, 24 * 60); // 1440 bars for 1 day

        // 1 hour bars
        let hour_bars = DateRange::Month1.estimated_bars(3600);
        assert_eq!(hour_bars, 30 * 24); // 720 bars for 1 month
    }

    #[test]
    fn test_start_timestamp() {
        // All should return None
        assert!(DateRange::All.start_timestamp().is_none());

        // Others should return Some
        assert!(DateRange::Day1.start_timestamp().is_some());
        assert!(DateRange::Year1.start_timestamp().is_some());
    }
}
