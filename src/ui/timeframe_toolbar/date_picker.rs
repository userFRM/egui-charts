//! Go to date picker
//!
//! A modal dialog with a date field and a month calendar for moving the chart to a date or a
//! range of dates.
use crate::icons::icons;
use crate::localization as l10n;
use crate::scales::TimezoneMode;
use crate::styles::focus::draw_focus_ring;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use chrono::{DateTime, Datelike, Days, Months, NaiveDate, NaiveTime, Utc};
use egui::text::CCursorRange;
use egui::{
    Align, Align2, FontId, Key, Label, Layout, Modifiers, RichText, Role, Sense, Stroke,
    StrokeKind, TextEdit, Ui, Vec2, WidgetInfo,
};
use std::fmt::Write;

/// Side of a day cell.
const CELL: f32 = 32.0;
/// Height of the line under the date field and of the weekday row.
const LINE: f32 = 16.0;
/// Format of the date field.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Format of the time fields.
const TIME_FORMAT: &str = "%H:%M";

/// The date at `now` in `timezone`.
fn today_in(now: DateTime<Utc>, timezone: &TimezoneMode) -> NaiveDate {
    match timezone {
        TimezoneMode::Local => crate::model::local_time(chrono::Local, now),
        zone => crate::model::local_time(zone.to_tz().unwrap_or_default(), now),
    }
    .date()
}

/// The date in `text`, if it is a valid date between the years 1 and 9999.
fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), DATE_FORMAT)
        .ok()
        .filter(|date| (1..=9999).contains(&date.year()))
}

/// The time in `text`: empty means the day's boundary.
fn parse_time(text: &str) -> Result<Option<NaiveTime>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    NaiveTime::parse_from_str(text, TIME_FORMAT)
        .map(Some)
        .map_err(|_| ())
}

/// Action from date picker
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DatePickerAction {
    /// No action requested.
    None,
    /// Go to `date`, at `time` when entered, in the time zone the chart shows.
    GoToDate {
        /// The day.
        date: NaiveDate,
        /// The time of day, or its start. Always `None` on daily and longer intervals.
        time: Option<NaiveTime>,
    },
    /// Show the days from `start` to `end`, both included, in the time zone the chart shows,
    /// between the entered times or the days' boundaries.
    SetRange {
        /// The first day.
        start: NaiveDate,
        /// The time on the first day, or its start.
        start_time: Option<NaiveTime>,
        /// The last day, not before `start`.
        end: NaiveDate,
        /// The time on the last day, or its end.
        end_time: Option<NaiveTime>,
    },
    /// Cancel the current interaction.
    Cancel,
}

/// Date picker component
#[derive(Debug)]
pub struct DatePicker {
    /// Is dialog open
    pub is_open: bool,
    /// Selected date
    pub sel_date: NaiveDate,
    /// The zone whose date is today, from the last open.
    timezone: TimezoneMode,
    /// The date of the last Go, which the next open shows.
    went_to: Option<NaiveDate>,
    /// Currently displayed month
    displayed_month: NaiveDate,
    /// Date input text
    date_input: String,
    /// Whether the chart takes times within a day.
    intraday: bool,
    /// Optional time on the selected date or the range's first day.
    time_input: String,
    /// Optional time on the range's last day.
    end_time_input: String,
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
    /// Create a [`DatePicker`] with default settings.
    pub fn new() -> Self {
        let today = today_in(Utc::now(), &TimezoneMode::Utc);
        Self {
            is_open: false,
            sel_date: today,
            timezone: TimezoneMode::Utc,
            went_to: None,
            displayed_month: today,
            date_input: today.format(DATE_FORMAT).to_string(),
            intraday: false,
            time_input: String::new(),
            end_time_input: String::new(),
            custom_range_mode: false,
            range_start: None,
            range_end: None,
            focus_field: false,
        }
    }

    /// Opens on today in `timezone`, the zone the chart shows its times in, or on the date
    /// the picker last went to once Go was used.
    /// With `intraday`, Go to date and both ends of a custom range take an optional time of day
    /// as `HH:MM`; pass `false` for daily and longer intervals.
    pub fn open_in(&mut self, timezone: TimezoneMode, intraday: bool) {
        self.timezone = timezone;
        self.intraday = intraday;
        self.time_input.clear();
        self.end_time_input.clear();
        self.sel_date = self.went_to.unwrap_or_else(|| self.today());
        self.is_open = true;
        self.date_input = self.sel_date.format(DATE_FORMAT).to_string();
        self.displayed_month = self.sel_date;
        self.range_start = None;
        self.range_end = None;
        self.focus_field = true;
    }

    /// [`open_in`](Self::open_in) the zone `chart` shows: its
    /// [`display_timezone`](crate::widget::Chart::display_timezone), with time fields below a
    /// day's interval, on trade-based intervals and when the chart has no interval.
    pub fn open_for_chart(&mut self, chart: &crate::widget::Chart) {
        self.open_in(
            chart.display_timezone(),
            chart
                .timeframe()
                .is_none_or(|tf| tf.duration_ms() < 86_400_000),
        );
    }

    fn today(&self) -> NaiveDate {
        today_in(Utc::now(), &self.timezone)
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
        let mut time = Ok(None);
        let mut end_time = Ok(None);

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing.lg;
            let row = Vec2::new(ui.available_width(), row_height);

            ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
                let label = ui.label(l10n::text(ui.ctx(), "Date"));
                let mut field = TextEdit::singleline(&mut self.date_input)
                    .desired_width(f32::INFINITY)
                    .min_size(Vec2::new(0.0, row_height))
                    .align(Align2::LEFT_CENTER)
                    .hint_text(l10n::text(ui.ctx(), "YYYY-MM-DD"))
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

            if self.intraday {
                let field = |ui: &mut Ui, label: &str, input: &mut String| {
                    let label = ui.label(l10n::text(ui.ctx(), label));
                    let response = TextEdit::singleline(input)
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, row_height))
                        .align(Align2::LEFT_CENTER)
                        .hint_text(l10n::text(ui.ctx(), "HH:MM"))
                        .show(ui)
                        .response
                        .response
                        .labelled_by(label.id);
                    if parse_time(input).is_err() {
                        ui.painter().rect_stroke(
                            response.rect,
                            ui.visuals().widgets.inactive.corner_radius,
                            Stroke::new(1.0, theme.ui.error),
                            StrokeKind::Inside,
                        );
                    }
                };
                if self.custom_range_mode {
                    ui.columns(2, |columns| {
                        field(&mut columns[0], "Start time", &mut self.time_input);
                        field(&mut columns[1], "End time", &mut self.end_time_input);
                    });
                } else {
                    field(ui, "Time", &mut self.time_input);
                }
                time = parse_time(&self.time_input);
                if self.custom_range_mode {
                    end_time = parse_time(&self.end_time_input);
                }
            }
            let ordered = self.range_start != self.range_end
                || !matches!((time, end_time), (Ok(Some(start)), Ok(Some(end))) if end <= start);

            // Always one line, so the dialog keeps its height while a range is picked.
            let (status, color) = if !field_valid {
                (
                    l10n::text(ui.ctx(), "Enter a date as YYYY-MM-DD"),
                    theme.ui.error,
                )
            } else if time.is_err() || end_time.is_err() {
                (
                    l10n::text(ui.ctx(), "Enter the time as HH:MM"),
                    theme.ui.error,
                )
            } else if self.custom_range_mode && self.range_start.is_some() && !ordered {
                (
                    l10n::text(ui.ctx(), "The end must be after the start"),
                    theme.ui.error,
                )
            } else if !self.custom_range_mode {
                let mut text = self.sel_date.format("%A %-d %B %Y").to_string();
                if let Ok(Some(time)) = time {
                    let _ = write!(text, " {}", time.format(TIME_FORMAT));
                }
                (text, theme.ui.text_secondary)
            } else {
                let text = match (self.range_start, self.range_end) {
                    (Some(start), Some(end)) => {
                        let mut start_text = String::with_capacity(10);
                        let mut end_text = String::with_capacity(10);
                        let _ = write!(start_text, "{start}");
                        let _ = write!(end_text, "{end}");
                        if let Ok(Some(time)) = time {
                            let _ = write!(start_text, " {}", time.format(TIME_FORMAT));
                        }
                        if let Ok(Some(time)) = end_time {
                            let _ = write!(end_text, " {}", time.format(TIME_FORMAT));
                        }
                        l10n::format_text(
                            ui.ctx(),
                            "From {start} to {end}",
                            &[("start", start_text.as_str()), ("end", end_text.as_str())],
                        )
                    }
                    (Some(start), None) => {
                        let mut start_text = String::with_capacity(10);
                        let _ = write!(start_text, "{start}");
                        if let Ok(Some(time)) = time {
                            let _ = write!(start_text, " {}", time.format(TIME_FORMAT));
                        }
                        l10n::format_text(
                            ui.ctx(),
                            "From {start}, pick the last day",
                            &[("start", start_text.as_str())],
                        )
                    }
                    _ => l10n::text(ui.ctx(), "Pick the first day"),
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
                    .checkbox(
                        &mut self.custom_range_mode,
                        l10n::text(ui.ctx(), "Custom range"),
                    )
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
                                RichText::new(l10n::text(
                                    ui.ctx(),
                                    self.displayed_month.format("%B %Y").to_string(),
                                ))
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
        let ordered = range.is_none_or(|(start, end)| {
            start != end
                || !matches!((time, end_time), (Ok(Some(start)), Ok(Some(end))) if end <= start)
        });
        let can_go = field_valid
            && time.is_ok()
            && end_time.is_ok()
            && if self.custom_range_mode {
                range.is_some() && ordered
            } else {
                true
            };

        let mut today_clicked = false;
        let (go, cancel) = DialogFooter::new("Go")
            .secondary("Cancel")
            .primary_enabled(can_go)
            .show_with_left(ui, |ui| {
                today_clicked = Button::new("Today").ghost().show(ui).clicked();
            });
        if today_clicked {
            self.pick(self.today());
        }

        // Enter applies Go unless a focused control takes it; the field gives up its focus on Enter.
        let enter = can_go
            && ui.memory(|m| m.focused().is_none())
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));

        if go || enter {
            match range {
                Some((start, end)) if self.custom_range_mode => {
                    self.went_to = Some(start);
                    DatePickerAction::SetRange {
                        start,
                        start_time: time.ok().flatten(),
                        end,
                        end_time: end_time.ok().flatten(),
                    }
                }
                _ => {
                    self.went_to = Some(self.sel_date);
                    DatePickerAction::GoToDate {
                        date: self.sel_date,
                        time: time.ok().flatten(),
                    }
                }
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
        let today = self.today();
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
                            RichText::new(l10n::text(ui.ctx(), day))
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
                        let name = date.format("%A %-d %B %Y").to_string();
                        let name = if date == today {
                            l10n::format_text(ui.ctx(), "{name}, today", &[("name", name)])
                        } else {
                            name
                        };
                        response.widget_info(|| {
                            WidgetInfo::selected(Role::Button, true, is_end, &name)
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
                            l10n::text(ui.ctx(), date.day().to_string()),
                            FontId::proportional(theme.text.body),
                            text,
                        );
                        draw_focus_ring(ui, &response, theme.radius.sm);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// EST is UTC-5 in November; Tokyo is UTC+9, including when used as exchange time.
    #[cfg(feature = "timezones")]
    #[test]
    fn today_is_the_date_in_the_chart_time_zone() {
        use crate::model::TimezoneId;
        for (now, zone, expected) in [
            (
                "2026-11-19T02:00:00Z",
                TimezoneMode::Timezone(TimezoneId::named("America/New_York").unwrap()),
                "2026-11-18",
            ),
            (
                "2026-11-18T23:00:00Z",
                TimezoneMode::Exchange(TimezoneId::named("Asia/Tokyo").unwrap()),
                "2026-11-19",
            ),
            ("2026-11-18T23:00:00Z", TimezoneMode::Utc, "2026-11-18"),
        ] {
            assert_eq!(
                today_in(now.parse().unwrap(), &zone).to_string(),
                expected,
                "{zone:?}"
            );
        }
    }

    /// Intraday times reach the appropriate action end; invalid times cannot navigate, while
    /// daily charts retain whole-day actions. Only a date applied with Go survives reopening.
    #[test]
    fn go_to_date_takes_a_time_on_intraday_charts() {
        use crate::model::{BarData, Timeframe};
        use crate::theme::Theme;
        use crate::widget::Chart;
        use egui::Role;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };

        let day = |day| NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        let time = |hour, minute| Some(NaiveTime::from_hms_opt(hour, minute, 0).unwrap());
        let single = |time| {
            Some(DatePickerAction::GoToDate {
                date: day(17),
                time,
            })
        };
        let range = |start_time, end_time| {
            Some(DatePickerAction::SetRange {
                start: day(10),
                start_time,
                end: day(17),
                end_time,
            })
        };
        for theme in [Theme::light(), Theme::dark()] {
            for (interval, days, start_time, end_time, expected) in [
                (
                    Some(Timeframe::Min1),
                    None,
                    "14:30",
                    "",
                    single(time(14, 30)),
                ),
                (Some(Timeframe::Min1), None, "", "", single(None)),
                (Some(Timeframe::Min1), None, "25:00", "", None),
                (Some(Timeframe::Day1), None, "", "", single(None)),
                (None, None, "14:30", "", single(time(14, 30))),
                (
                    Some("2t".parse().unwrap()),
                    None,
                    "14:30",
                    "",
                    single(time(14, 30)),
                ),
                (
                    Some(Timeframe::Min1),
                    Some((10, 17)),
                    "09:30",
                    "16:00",
                    range(time(9, 30), time(16, 0)),
                ),
                (
                    Some(Timeframe::Min1),
                    Some((17, 17)),
                    "16:00",
                    "09:30",
                    None,
                ),
                (
                    Some(Timeframe::Min1),
                    Some((17, 17)),
                    "09:30",
                    "09:30",
                    None,
                ),
                (
                    Some(Timeframe::Min1),
                    Some((10, 17)),
                    "09:30",
                    "25:00",
                    None,
                ),
                (
                    Some(Timeframe::Day1),
                    Some((10, 17)),
                    "",
                    "",
                    range(None, None),
                ),
            ] {
                let mut chart = Chart::new(BarData::new());
                if let Some(interval) = interval {
                    chart.set_timeframe(interval);
                }
                let intraday = interval.is_none_or(|tf| tf.duration_ms() < 86_400_000);
                let mut picker = DatePicker::new();
                picker.open_for_chart(&chart);
                let theme = theme.clone();
                let mut h = Harness::builder()
                    .allow_missing_glyphs()
                    .with_size(Vec2::new(1440.0, 900.0))
                    .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
                    .build_ui_state(
                        |ui, state: &mut (DatePicker, Option<DatePickerAction>)| {
                            let action = state.0.show(ui.ctx());
                            if action != DatePickerAction::None {
                                state.1 = Some(action);
                            }
                        },
                        (picker, None),
                    );
                h.run();
                h.get_by_role_and_label(Role::TextInput, "Date")
                    .type_text("2026-11-17");
                h.run();
                if let Some((start, end)) = days {
                    h.get_by_role_and_label(Role::CheckBox, "Custom range")
                        .click();
                    h.run();
                    for date in [day(start), day(end)] {
                        h.get_by_role_and_label(
                            Role::Button,
                            &date.format("%A %-d %B %Y").to_string(),
                        )
                        .click();
                        h.run();
                    }
                }
                for (label, text) in if days.is_some() {
                    [("Start time", start_time), ("End time", end_time)]
                } else {
                    [("Time", start_time), ("Time", "")]
                } {
                    if intraday {
                        if !text.is_empty() {
                            h.get_by_role_and_label(Role::TextInput, label).focus();
                            h.run();
                            h.get_by_role_and_label(Role::TextInput, label)
                                .type_text(text);
                            h.run();
                        }
                    } else {
                        assert!(h.query_by_role_and_label(Role::TextInput, label).is_none());
                    }
                }
                let go = h.get_by_role_and_label(Role::Button, "Go");
                assert_eq!(
                    go.accesskit_node().is_disabled(),
                    expected.is_none(),
                    "{interval:?} {days:?} {start_time} {end_time}"
                );
                if let Some(expected) = expected {
                    go.click();
                    h.run();
                    assert_eq!(h.state().1.as_ref(), Some(&expected));
                    let went_to = match expected {
                        DatePickerAction::GoToDate { date, .. } => date,
                        DatePickerAction::SetRange { start, .. } => start,
                        _ => unreachable!(),
                    };
                    h.state_mut().0.open_for_chart(&chart);
                    h.run();
                    assert_eq!(h.state().0.sel_date, went_to);
                    if intraday {
                        let label = if days.is_some() { "Start time" } else { "Time" };
                        assert_eq!(
                            h.get_by_role_and_label(Role::TextInput, label)
                                .accesskit_node()
                                .value()
                                .as_deref(),
                            Some("")
                        );
                    }
                    h.get_by_role_and_label(Role::TextInput, "Date")
                        .type_text("2026-11-18");
                    h.run();
                    h.get_by_role_and_label(Role::Button, "Cancel").click();
                    h.run();
                    h.state_mut().0.open_for_chart(&chart);
                    h.run();
                    assert_eq!(
                        h.state().0.sel_date,
                        went_to,
                        "Cancel preserves the last Go"
                    );
                } else {
                    assert_eq!(h.state().1, None);
                }
            }
        }
    }
}
