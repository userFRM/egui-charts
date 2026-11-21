//! Start selection intents; the host supplies history, randomness and saved sessions.

use crate::localization as l10n;
use crate::scales::TimezoneMode;
use crate::theme::ThemeExt;
use crate::ui_kit::{buttons::Button, modal::Modal};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use egui::{Painter, Pos2, Rect, Stroke, Vec2, pos2};

/// A replay start requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ReplayStartAction {
    /// No selection, including cancellation.
    #[default]
    None,
    /// Select the start by clicking a chart bar.
    SelectBar,
    /// Ask for a start date, as [`ReplayStartPicker`] does.
    SelectDate,
    /// First available source bar.
    First,
    /// A random source bar selected by the host.
    Random,
    /// First source bar opening on or after this instant. [`ReplayStartPicker`] gives the start
    /// of the chosen day on its calendar.
    Date(DateTime<Utc>),
    /// Restore the host's saved session.
    Resume,
}

/// Date entry dialog shared by hosts of the replay controller.
#[derive(Debug, Default)]
pub struct ReplayStartPicker {
    /// Whether the dialog is open.
    pub is_open: bool,
    date: String,
    /// The calendar the date is read on.
    timezone: TimezoneMode,
}

impl ReplayStartPicker {
    /// Opens date entry on the UTC calendar with an optional initial date.
    /// [`Self::open_for_chart`] reads the date in a chart's time zone instead.
    pub fn open(&mut self, date: Option<DateTime<Utc>>) {
        self.open_in(TimezoneMode::Utc, date);
    }

    /// Opens date entry on the calendar of `chart`, the one
    /// [`Chart::go_to_local_time`](crate::widget::Chart::go_to_local_time) reads: the zone the
    /// chart [shows](crate::widget::Chart::display_timezone), or UTC where supplied bars of a day
    /// or longer carry their trading date at 00:00 UTC. `date` fills the field with its date in
    /// that zone. The confirmed date gives [`ReplayStartAction::Date`] at the first instant of
    /// that day there; a midnight the clocks skip gives the instant of the jump.
    pub fn open_for_chart(&mut self, chart: &crate::widget::Chart, date: Option<DateTime<Utc>>) {
        self.open_in(chart.bar_date_timezone(), date);
    }

    fn open_in(&mut self, timezone: TimezoneMode, date: Option<DateTime<Utc>>) {
        self.date = date.map_or_else(String::new, |time| {
            timezone
                .wall_time(time)
                .date()
                .format("%Y-%m-%d")
                .to_string()
        });
        self.timezone = timezone;
        self.is_open = true;
    }

    /// Shows date entry, and returns [`ReplayStartAction::Date`] once the user confirms one.
    pub fn show(&mut self, ctx: &egui::Context) -> ReplayStartAction {
        if !self.is_open {
            return ReplayStartAction::None;
        }
        let mut action = ReplayStartAction::None;
        Modal::new("replay_start", "Select date")
            .width(360.0)
            .show(ctx, &mut self.is_open, |ui| {
                let label = ui.label(l10n::text(ui.ctx(), "Date"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.date)
                        .hint_text(l10n::text(ui.ctx(), "YYYY-MM-DD"))
                        .desired_width(ui.available_width()),
                )
                .labelled_by(label.id);
                let date = NaiveDate::parse_from_str(self.date.trim(), "%Y-%m-%d")
                    .ok()
                    .and_then(|date| self.timezone.instant(date.and_time(NaiveTime::MIN)));
                if Button::new("Start at date")
                    .secondary()
                    .enabled(date.is_some())
                    .show(ui)
                    .clicked()
                    && let Some(date) = date
                {
                    action = ReplayStartAction::Date(date);
                }
            });
        if action != ReplayStartAction::None {
            self.is_open = false;
        }
        action
    }
}

/// Paints the hint of bar selection: a vertical line through `plot` at the bar under the pointer,
/// with scissors at the pointer's height, marking where the replay cuts the history.
pub fn paint_start_line(painter: &Painter, plot: Rect, pointer: Pos2) {
    let theme = painter.ctx().active_theme();
    let color = theme.ui.accent;
    // A plot shorter than the mark centres it. Its bounds then meet, and where f32 rounds the
    // plot's height they cross by an ulp, which `clamp` would panic on.
    let half = (plot.height() / 2.0).min(12.0);
    let center = pos2(
        pointer.x,
        pointer.y.max(plot.top() + half).min(plot.bottom() - half),
    );
    painter.vline(center.x, plot.y_range(), Stroke::new(1.0, color));
    painter.circle_filled(center, 11.0, theme.chart.bg);
    let stroke = Stroke::new(1.5, color);
    for side in [-1.0, 1.0] {
        let handle = center + Vec2::new(side * 3.0, 4.5);
        painter.circle_stroke(handle, 2.25, stroke);
        painter.line_segment(
            [
                handle + Vec2::new(0.0, -2.0),
                center + Vec2::new(-side * 3.0, -8.0),
            ],
            stroke,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The typed day starts at its midnight in the chart's zone, and the field is found by its
    /// caption under the default accessibility check. EDT is UTC-4 in October; Tokyo is UTC+9,
    /// including when used as exchange time.
    #[cfg(feature = "timezones")]
    #[test]
    fn select_date_reads_the_day_in_the_chart_zone() {
        use crate::model::{BarData, TimezoneId};
        use crate::widget::Chart;
        use egui::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        for (zone, expected, initial) in [
            (
                TimezoneMode::Timezone(TimezoneId::named("America/New_York").unwrap()),
                "2026-10-23T04:00:00Z",
                // 22:00 EDT on 30 September.
                Some(("2026-10-01T02:00:00Z", "2026-09-30")),
            ),
            (
                TimezoneMode::Exchange(TimezoneId::named("Asia/Tokyo").unwrap()),
                "2026-10-22T15:00:00Z",
                None,
            ),
            (TimezoneMode::Utc, "2026-10-23T00:00:00Z", None),
        ] {
            let mut chart = Chart::new(BarData::new());
            chart.chart_options.time_scale.timezone = zone.clone();
            let mut picker = ReplayStartPicker::default();
            if let Some((time, date)) = initial {
                picker.open_for_chart(&chart, Some(time.parse().unwrap()));
                let mut h = Harness::builder()
                    .allow_missing_glyphs()
                    .with_size(Vec2::new(1440.0, 900.0))
                    .build_ui_state(
                        |ui, picker: &mut ReplayStartPicker| {
                            picker.show(ui.ctx());
                        },
                        picker,
                    );
                h.run();
                assert_eq!(
                    h.get_by_role_and_label(Role::TextInput, "Date").value(),
                    Some(date.to_owned()),
                    "{zone:?}"
                );
                picker = ReplayStartPicker::default();
            }
            picker.open_for_chart(&chart, None);
            let mut h = Harness::builder()
                .allow_missing_glyphs()
                .with_size(Vec2::new(1440.0, 900.0))
                .build_ui_state(
                    |ui, state: &mut (ReplayStartPicker, ReplayStartAction)| {
                        let action = state.0.show(ui.ctx());
                        if action != ReplayStartAction::None {
                            state.1 = action;
                        }
                    },
                    (picker, ReplayStartAction::None),
                );
            h.run();
            h.get_by_role_and_label(Role::TextInput, "Date").focus();
            h.run();
            h.get_by_role_and_label(Role::TextInput, "Date")
                .type_text("2026-10-23");
            h.run();
            h.get_by_role_and_label(Role::Button, "Start at date")
                .click();
            h.run();
            let (picker, action) = h.state();
            assert_eq!(
                *action,
                ReplayStartAction::Date(expected.parse().unwrap()),
                "{zone:?}"
            );
            assert!(!picker.is_open, "{zone:?}");
        }
    }

    /// A price pane shorter than the scissors still gets the line through the bar under the
    /// pointer, with the scissors centred on it.
    #[test]
    fn start_line_on_a_plot_shorter_than_its_mark() {
        for (top, height, marked) in [
            (100.0, 0.0, None),
            (100.0, 20.0, Some(pos2(150.0, 110.0))),
            // Edges between pixels near the top of the window: f32 rounds the height up.
            (1.6, 5.6, None),
        ] {
            let plot = Rect::from_min_size(pos2(0.0, top), Vec2::new(300.0, height));
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                paint_start_line(ui.painter(), plot, pos2(150.0, 101.0));
            });
            if let Some(marked) = marked {
                assert!(output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    egui::Shape::LineSegment { points, .. }
                        if points[0].x == marked.x && points[1].x == marked.x
                )));
                assert!(output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    egui::Shape::Circle(circle) if circle.center == marked
                )));
            }
            output.drop_without_applying_deltas();
        }
    }
}
