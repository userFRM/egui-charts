//! Start selection intents; the host supplies history, randomness and saved sessions.

use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::ui_kit::{buttons::Button, modal::Modal};
use chrono::{DateTime, NaiveDate, Utc};
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
    /// First source bar opening on or after this UTC instant.
    Date(DateTime<Utc>),
    /// Restore the host's saved session.
    Resume,
}

/// Date entry dialog shared by hosts of the replay controller.
#[derive(Default)]
pub struct ReplayStartPicker {
    /// Whether the dialog is open.
    pub is_open: bool,
    date: String,
}

impl ReplayStartPicker {
    /// Opens date entry with an optional initial UTC date.
    pub fn open(&mut self, date: Option<DateTime<Utc>>) {
        self.date = date.map_or_else(String::new, |time| time.format("%Y-%m-%d").to_string());
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
                ui.label(l10n::text(ui.ctx(), "Date (UTC, YYYY-MM-DD)"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.date).desired_width(ui.available_width()),
                );
                let date = NaiveDate::parse_from_str(self.date.trim(), "%Y-%m-%d")
                    .ok()
                    .and_then(|date| date.and_hms_opt(0, 0, 0))
                    .map(|date| date.and_utc());
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
    // A plot shorter than the mark centres it.
    let half = (plot.height() / 2.0).min(12.0);
    let center = pos2(
        pointer.x,
        pointer.y.clamp(plot.top() + half, plot.bottom() - half),
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

    /// A price pane shorter than the scissors still gets the line through the bar under the
    /// pointer, with the scissors centred on it.
    #[test]
    fn start_line_on_a_plot_shorter_than_its_mark() {
        for (height, marked) in [(0.0, None), (20.0, Some(pos2(150.0, 110.0)))] {
            let plot = Rect::from_min_size(pos2(0.0, 100.0), Vec2::new(300.0, height));
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
