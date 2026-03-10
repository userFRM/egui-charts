//! Date Picker / Go To
//!
//! A date picker dialog with calendar for navigating to specific dates.
use crate::ext::UiExt;
use crate::styles::typography;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use chrono::{Datelike, NaiveDate};
use egui::{Color32, Sense, Stroke, StrokeKind, Ui, Vec2, Window};

/// Configuration for date picker
#[derive(Debug, Clone)]
pub struct DatePickerConfig {
    pub width: f32,
    pub height: f32,
    pub bg_color: Color32,
    pub text_color: Color32,
    pub muted_color: Color32,
    pub hover_color: Color32,
    pub sel_color: Color32,
    pub today_color: Color32,
}

impl DatePickerConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.semantic.ui;
        Self {
            width: DESIGN_TOKENS.sizing.toolbar.right_panel_width,
            height: 350.0, // Date picker specific height
            bg_color: ui.panel_bg,
            text_color: ui.text,
            muted_color: ui.text_muted,
            hover_color: ui.btn_bg_hover,
            sel_color: ui.accent,
            today_color: ui.success, // Green for today indicator
        }
    }
}

impl Default for DatePickerConfig {
    fn default() -> Self {
        // Default uses light UI chrome theme
        Self::from_theme(&Theme::dark())
    }
}

/// Action from date picker
#[derive(Debug, Clone, PartialEq)]
pub enum DatePickerAction {
    None,
    GoToDate(NaiveDate),
    SetRange(NaiveDate, NaiveDate),
    Cancel,
}

/// Date picker component
pub struct DatePicker {
    /// Is dialog open
    pub is_open: bool,
    /// Selected date
    pub sel_date: NaiveDate,
    /// Currently displayed month
    displayed_month: NaiveDate,
    /// Date input text
    date_input: String,
    /// Custom range mode
    custom_range_mode: bool,
    /// Range start date
    range_start: Option<NaiveDate>,
    /// Range end date
    range_end: Option<NaiveDate>,
    /// Configuration
    config: DatePickerConfig,
}

impl Default for DatePicker {
    fn default() -> Self {
        Self::new()
    }
}

impl DatePicker {
    pub fn new() -> Self {
        let today = chrono::Local::now().date_naive();
        Self {
            is_open: false,
            sel_date: today,
            displayed_month: today,
            date_input: today.format("%Y-%m-%d").to_string(),
            custom_range_mode: false,
            range_start: None,
            range_end: None,
            config: DatePickerConfig::default(),
        }
    }

    /// Open the date picker
    pub fn open(&mut self) {
        self.is_open = true;
        self.date_input = self.sel_date.format("%Y-%m-%d").to_string();
        self.displayed_month = self.sel_date;
    }

    /// Close the date picker
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Show the date picker dialog
    pub fn show(&mut self, ctx: &egui::Context) -> DatePickerAction {
        let mut action = DatePickerAction::None;

        if !self.is_open {
            return action;
        }

        let mut is_open = self.is_open;

        Window::new("Go to")
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(self.config.width, self.config.height))
            .frame(egui::Frame::window(&ctx.style()).fill(self.config.bg_color))
            .show(ctx, |ui| {
                action = self.draw_content(ui);
            });

        self.is_open = is_open;
        if !is_open {
            action = DatePickerAction::Cancel;
        }

        action
    }

    fn draw_content(&mut self, ui: &mut Ui) -> DatePickerAction {
        let mut action = DatePickerAction::None;

        ui.space_lg();

        // Date input field
        ui.horizontal(|ui| {
            ui.label("Date:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.date_input)
                    .desired_width(120.0)
                    .hint_text("YYYY-MM-DD"),
            );

            if response.changed()
                && let Ok(date) = NaiveDate::parse_from_str(&self.date_input, "%Y-%m-%d")
            {
                self.sel_date = date;
                self.displayed_month = date;
            }
        });

        ui.space_lg();

        // Custom range toggle
        ui.checkbox(&mut self.custom_range_mode, "Custom range");

        ui.separator_with_margin(DESIGN_TOKENS.spacing.lg);

        // Month navigation
        ui.horizontal(|ui| {
            // Previous month
            if ui.button("<").clicked() {
                self.displayed_month = self
                    .displayed_month
                    .checked_sub_months(chrono::Months::new(1))
                    .unwrap_or(self.displayed_month);
            }

            ui.space_lg();

            // Month/Year display
            let month_year = self.displayed_month.format("%B %Y").to_string();
            ui.strong_label(month_year);

            ui.space_lg();

            // Next month
            if ui.button(">").clicked() {
                self.displayed_month = self
                    .displayed_month
                    .checked_add_months(chrono::Months::new(1))
                    .unwrap_or(self.displayed_month);
            }
        });

        ui.space_lg();

        // Calendar grid
        if let Some(date) = self.draw_calendar(ui) {
            self.sel_date = date;
            self.date_input = date.format("%Y-%m-%d").to_string();

            if self.custom_range_mode {
                if self.range_start.is_none() {
                    self.range_start = Some(date);
                } else if self.range_end.is_none() {
                    self.range_end = Some(date);
                    // Swap if start > end
                    if let (Some(start), Some(end)) = (self.range_start, self.range_end)
                        && start > end
                    {
                        self.range_start = Some(end);
                        self.range_end = Some(start);
                    }
                } else {
                    // Reset selection
                    self.range_start = Some(date);
                    self.range_end = None;
                }
            }
        }

        ui.space_lg();

        // Today button and action btns
        ui.horizontal(|ui| {
            if ui.button("Today").clicked() {
                let today = chrono::Local::now().date_naive();
                self.sel_date = today;
                self.displayed_month = today;
                self.date_input = today.format("%Y-%m-%d").to_string();
            }

            ui.right_aligned(|ui| {
                if ui.button("  Go  ").clicked() {
                    if self.custom_range_mode {
                        if let (Some(start), Some(end)) = (self.range_start, self.range_end) {
                            action = DatePickerAction::SetRange(start, end);
                        }
                    } else {
                        action = DatePickerAction::GoToDate(self.sel_date);
                    }
                    self.is_open = false;
                }

                if ui.button("Cancel").clicked() {
                    action = DatePickerAction::Cancel;
                    self.is_open = false;
                }
            });
        });

        action
    }

    fn draw_calendar(&mut self, ui: &mut Ui) -> Option<NaiveDate> {
        let mut selected = None;

        let today = chrono::Local::now().date_naive();
        let cell_size = 32.0;
        let grid_width = cell_size * 7.0;

        // Weekday headers
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - grid_width) / 2.0);
            for day in &["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"] {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(cell_size), Sense::hover());
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    *day,
                    egui::FontId::proportional(typography::SM),
                    self.config.muted_color,
                );
            }
        });

        // Calculate first day of month and days in month
        // Since displayed_month is always a valid NaiveDate, day 1 of that month is always valid
        let first_of_month =
            NaiveDate::from_ymd_opt(self.displayed_month.year(), self.displayed_month.month(), 1)
                .unwrap_or(self.displayed_month);

        let first_weekday = first_of_month.weekday().num_days_from_sunday() as i32;

        // Calculate days in month by finding the first day of the next month
        let next_month_first = if self.displayed_month.month() == 12 {
            NaiveDate::from_ymd_opt(self.displayed_month.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(
                self.displayed_month.year(),
                self.displayed_month.month() + 1,
                1,
            )
        };

        // Fallback to 28 days (minimum for any month) if date calculation fails
        let days_in_month = next_month_first
            .map(|d| d.signed_duration_since(first_of_month).num_days() as i32)
            .unwrap_or(28);

        // Draw calendar grid
        let mut day = 1 - first_weekday;

        for _week in 0..6 {
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - grid_width) / 2.0);

                for _weekday in 0..7 {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(cell_size), Sense::click());

                    // Only render valid days within the month
                    if day >= 1
                        && day <= days_in_month
                        && let Some(date) = NaiveDate::from_ymd_opt(
                            self.displayed_month.year(),
                            self.displayed_month.month(),
                            day as u32,
                        )
                    {
                        let is_sel = date == self.sel_date;
                        let is_today = date == today;
                        let is_in_range = if self.custom_range_mode {
                            match (self.range_start, self.range_end) {
                                (Some(start), Some(end)) => date >= start && date <= end,
                                (Some(start), None) => date == start,
                                _ => false,
                            }
                        } else {
                            false
                        };

                        // Background
                        if is_sel {
                            ui.painter().rect_filled(
                                rect,
                                DESIGN_TOKENS.rounding.md,
                                self.config.sel_color,
                            );
                        } else if is_in_range {
                            // Range selection uses selection color with reduced alpha
                            let range_color = ui.visuals().selection.bg_fill.gamma_multiply(0.5);
                            ui.painter()
                                .rect_filled(rect, DESIGN_TOKENS.rounding.sm, range_color);
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                DESIGN_TOKENS.rounding.md,
                                self.config.hover_color,
                            );
                        }

                        // Today indicator
                        if is_today && !is_sel {
                            ui.painter().rect_stroke(
                                rect.shrink(2.0),
                                DESIGN_TOKENS.rounding.md,
                                Stroke::new(DESIGN_TOKENS.stroke.thick, self.config.today_color),
                                StrokeKind::Outside,
                            );
                        }

                        // Day number
                        let text_color = if is_sel {
                            ui.visuals().selection.stroke.color
                        } else if is_today {
                            self.config.today_color
                        } else {
                            self.config.text_color
                        };

                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{}", day),
                            egui::FontId::proportional(typography::MD),
                            text_color,
                        );

                        if response.clicked() {
                            selected = Some(date);
                        }
                    }

                    day += 1;
                }
            });

            // Stop if we've finished the month
            if day > days_in_month + 7 {
                break;
            }
        }

        selected
    }
}
