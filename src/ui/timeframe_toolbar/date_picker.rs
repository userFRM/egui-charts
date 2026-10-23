//! Go to date picker
//!
//! A modal dialog with a date field and a month calendar for moving the chart to a date or a
//! range of dates.
use crate::icons::icons;
use crate::styles::focus::draw_focus_ring;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use chrono::{Datelike, Days, Months, NaiveDate};
use egui::text::CCursorRange;
use egui::{
    Align, Align2, FontId, Key, Label, Layout, Modifiers, RichText, Role, Sense, Stroke,
    StrokeKind, TextEdit, Ui, Vec2, WidgetInfo,
};

/// Side of a day cell.
const CELL: f32 = 32.0;
/// Height of the line under the date field and of the weekday row.
const LINE: f32 = 16.0;
/// Format of the date field.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Today in UTC, the time zone the chart shows times in by default.
fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

/// The date in `text`, if it is a valid date between the years 1 and 9999.
fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), DATE_FORMAT)
        .ok()
        .filter(|date| (1..=9999).contains(&date.year()))
}

/// Action from date picker
#[derive(Debug, Clone, PartialEq)]
pub enum DatePickerAction {
    None,
    GoToDate(NaiveDate),
    /// The first and the last day of the range, both included.
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
    /// Focus the date field on the next frame the dialog is drawn.
    focus_field: bool,
}

impl Default for DatePicker {
    fn default() -> Self {
        Self::new()
    }
}

impl DatePicker {
    pub fn new() -> Self {
        let today = today();
        Self {
            is_open: false,
            sel_date: today,
            displayed_month: today,
            date_input: today.format(DATE_FORMAT).to_string(),
            custom_range_mode: false,
            range_start: None,
            range_end: None,
            focus_field: false,
        }
    }

    /// Open the date picker
    pub fn open(&mut self) {
        self.is_open = true;
        self.date_input = self.sel_date.format(DATE_FORMAT).to_string();
        self.displayed_month = self.sel_date;
        self.range_start = None;
        self.range_end = None;
        self.focus_field = true;
    }

    /// Close the date picker
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Show the date picker dialog
    pub fn show(&mut self, ctx: &egui::Context) -> DatePickerAction {
        let theme = ctx.active_theme();
        let grid_width = 7.0 * CELL + 6.0 * theme.spacing.sm;
        // `Modal::width` is the outer width: the padding and the frame's stroke on both sides.
        let stroke = ctx.global_style().visuals.window_stroke.width;
        let width = grid_width + 2.0 * (theme.spacing.xxl + stroke);

        let mut open = self.is_open;
        let action =
            match Modal::new("go_to_date", "Go to date")
                .width(width)
                .show(ctx, &mut open, |ui| self.draw_content(ui))
            {
                Some(response) if response.close_requested => DatePickerAction::Cancel,
                Some(response) => response.inner,
                None => DatePickerAction::None,
            };
        // Go, Cancel, the close button and Escape all close the picker.
        self.is_open = open && action == DatePickerAction::None;
        action
    }

    fn draw_content(&mut self, ui: &mut Ui) -> DatePickerAction {
        let theme = ui.active_theme();
        let row_height = ButtonSize::MD.pixels();
        let mut field_valid = true;
        let mut typed = None;

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing.lg;
            let row = Vec2::new(ui.available_width(), row_height);

            ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
                let label = ui.label("Date");
                let mut field = TextEdit::singleline(&mut self.date_input)
                    .desired_width(f32::INFINITY)
                    .min_size(Vec2::new(0.0, row_height))
                    .align(Align2::LEFT_CENTER)
                    .hint_text("YYYY-MM-DD")
                    .show(ui);
                let date = parse_date(&self.date_input);
                if let Some(date) = date {
                    if field.response.changed() {
                        self.displayed_month = date;
                        if !self.custom_range_mode {
                            self.sel_date = date;
                        }
                    }
                    // In range mode a typed date counts as a pick once the field is left.
                    if self.custom_range_mode
                        && field.response.lost_focus()
                        && date != self.sel_date
                    {
                        typed = Some(date);
                    }
                }
                field_valid = date.is_some();
                if !field_valid {
                    ui.painter().rect_stroke(
                        field.response.rect,
                        ui.visuals().widgets.inactive.corner_radius,
                        Stroke::new(1.0, theme.ui.error),
                        StrokeKind::Inside,
                    );
                }
                // The sizing pass draws nothing, so take the focus on the first visible frame.
                if self.focus_field && !ui.is_sizing_pass() {
                    self.focus_field = false;
                    field.response.request_focus();
                    field
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::select_all(&field.galley)));
                    field.state.store(ui.ctx(), field.response.id);
                }
                field.response.response.labelled_by(label.id);
            });
            if let Some(date) = typed {
                self.pick(date);
            }

            // Always one line, so the dialog keeps its height while a range is picked.
            let (status, color) = if !field_valid {
                ("Enter a date as YYYY-MM-DD".to_owned(), theme.ui.error)
            } else if !self.custom_range_mode {
                (
                    self.sel_date.format("%A %-d %B %Y").to_string(),
                    theme.ui.text_secondary,
                )
            } else {
                let text = match (self.range_start, self.range_end) {
                    (Some(start), Some(end)) => format!("From {start} to {end}"),
                    (Some(start), None) => format!("From {start}, pick the last day"),
                    _ => "Pick the first day".to_owned(),
                };
                (text, theme.ui.text_secondary)
            };
            ui.allocate_ui_with_layout(
                Vec2::new(row.x, LINE),
                Layout::left_to_right(Align::Center),
                |ui| ui.label(RichText::new(status).size(theme.text.label).color(color)),
            );

            ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
                if ui
                    .checkbox(&mut self.custom_range_mode, "Custom range")
                    .changed()
                {
                    self.range_start = None;
                    self.range_end = None;
                }
            });

            ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
                let previous = IconButton::new(&icons::CHEVRON_LEFT)
                    .tooltip("Previous month")
                    .show(ui)
                    .clicked();
                let next = ui
                    .with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let next = IconButton::new(&icons::CHEVRON_RIGHT)
                            .tooltip("Next month")
                            .show(ui)
                            .clicked();
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new(self.displayed_month.format("%B %Y").to_string())
                                    .strong()
                                    .size(theme.text.body),
                            );
                        });
                        next
                    })
                    .inner;
                let month = Months::new(1);
                let shown = if previous {
                    self.displayed_month.checked_sub_months(month)
                } else if next {
                    self.displayed_month.checked_add_months(month)
                } else {
                    None
                };
                self.displayed_month = shown.unwrap_or(self.displayed_month);
            });

            if let Some(date) = self.draw_calendar(ui) {
                self.pick(date);
                field_valid = true;
            }
        });

        let range = self.range_start.zip(self.range_end);
        let can_go = if self.custom_range_mode {
            range.is_some()
        } else {
            field_valid
        };

        let mut today_clicked = false;
        let (go, cancel) = DialogFooter::new("Go")
            .secondary("Cancel")
            .primary_enabled(can_go)
            .show_with_left(ui, |ui| {
                today_clicked = Button::new("Today").ghost().show(ui).clicked();
            });
        if today_clicked {
            self.pick(today());
        }

        // Enter applies Go unless a focused control takes it; the field gives up its focus on Enter.
        let enter = can_go
            && ui.memory(|m| m.focused().is_none())
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));

        if go || enter {
            match range {
                Some((start, end)) if self.custom_range_mode => {
                    DatePickerAction::SetRange(start, end)
                }
                _ => DatePickerAction::GoToDate(self.sel_date),
            }
        } else if cancel {
            DatePickerAction::Cancel
        } else {
            DatePickerAction::None
        }
    }

    /// Select `date` as a click on its day does; in range mode it starts a range or ends the
    /// started one.
    fn pick(&mut self, date: NaiveDate) {
        self.sel_date = date;
        // A day of the previous or next month shows that month.
        self.displayed_month = date;
        self.date_input = date.format(DATE_FORMAT).to_string();
        if self.custom_range_mode {
            (self.range_start, self.range_end) = match (self.range_start, self.range_end) {
                (Some(start), None) => (Some(start.min(date)), Some(start.max(date))),
                _ => (Some(date), None),
            };
        }
    }

    /// Six weeks from the Sunday on or before the first of the displayed month.
    fn draw_calendar(&mut self, ui: &mut Ui) -> Option<NaiveDate> {
        let theme = ui.active_theme();
        let today = today();
        let first = self.displayed_month.with_day(1)?;
        let grid_start =
            first.checked_sub_days(Days::new(first.weekday().num_days_from_sunday().into()))?;
        let ends = if self.custom_range_mode {
            [self.range_start, self.range_end]
        } else {
            [Some(self.sel_date), None]
        };

        let mut picked = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(theme.spacing.sm);

            ui.horizontal(|ui| {
                for day in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"] {
                    ui.add_sized(
                        [CELL, LINE],
                        Label::new(
                            RichText::new(day)
                                .size(theme.text.small)
                                .color(theme.ui.text_muted),
                        ),
                    );
                }
            });

            for week in 0..6 {
                ui.horizontal(|ui| {
                    for weekday in 0..7 {
                        let Some(date) = grid_start.checked_add_days(Days::new(week * 7 + weekday))
                        else {
                            continue;
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(CELL), Sense::click());
                        let is_end = ends.contains(&Some(date));
                        let is_between =
                            matches!(ends, [Some(start), Some(end)] if start < date && date < end);
                        response.widget_info(|| {
                            let name = date.format("%A %-d %B %Y");
                            let name = if date == today {
                                format!("{name}, today")
                            } else {
                                name.to_string()
                            };
                            WidgetInfo::selected(Role::Button, true, is_end, name)
                        });

                        let (fill, text) = if is_end {
                            (Some(theme.ui.accent), theme.ui.accent_text)
                        } else if is_between {
                            (Some(theme.ui.accent.gamma_multiply(0.35)), theme.ui.text)
                        } else if response.hovered() {
                            (Some(theme.ui.btn_bg_hover), theme.ui.text)
                        } else if date.month() == first.month() {
                            (None, theme.ui.text)
                        } else {
                            (None, theme.ui.text_muted)
                        };
                        let painter = ui.painter();
                        if let Some(fill) = fill {
                            painter.rect_filled(rect, theme.radius.sm, fill);
                        }
                        if date == today && !is_end && !is_between {
                            painter.rect_stroke(
                                rect,
                                theme.radius.sm,
                                Stroke::new(1.0, theme.ui.text_muted),
                                StrokeKind::Inside,
                            );
                        }
                        painter.text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            date.day().to_string(),
                            FontId::proportional(theme.text.body),
                            text,
                        );
                        draw_focus_ring(ui, &response);

                        if response.clicked() {
                            picked = Some(date);
                        }
                    }
                });
            }
        });
        picked
    }
}
