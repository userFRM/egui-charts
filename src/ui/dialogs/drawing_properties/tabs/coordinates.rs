//! Coordinates tab - staged price and UTC time points.

use crate::drawings::{ChartPoint, Drawing};
use crate::ext::UiExt;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::FormGrid;
use crate::widget::Chart;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use egui::Ui;

/// Editable coordinate fields. Times are UTC and retain subsecond precision.
#[derive(Clone, Debug, Default)]
pub struct CoordinatePoint {
    /// Price as entered by the user.
    pub price: String,
    /// UTC calendar date, YYYY-MM-DD; empty for an unmapped anchor.
    pub date: String,
    /// UTC time, HH:MM[:SS[.fraction]].
    pub time: String,
    /// Fractional bar index, used when no timestamp is available.
    pub bar_index: String,
    original: Option<(ChartPoint, Option<DateTime<Utc>>)>,
}

/// Coordinates staged independently of the live drawing until Apply.
#[derive(Clone, Debug, Default)]
pub struct CoordinatesState {
    /// One entry for every anchor of the selected drawing.
    pub points: Vec<CoordinatePoint>,
}

impl CoordinatesState {
    pub(crate) fn from_drawing(drawing: &Drawing, chart: &Chart) -> Self {
        Self {
            points: drawing
                .chart_points
                .iter()
                .map(|point| {
                    let time = chart.drawing_point_time(point);
                    let mut original = *point;
                    original.time_anchor = time.map(|time| (time, point.bar_idx));
                    CoordinatePoint {
                        price: point.price.to_string(),
                        bar_index: point.bar_idx.to_string(),
                        date: time.map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string()),
                        time: time
                            .map_or_else(String::new, |t| t.format("%H:%M:%S%.f").to_string()),
                        original: Some((original, time)),
                    }
                })
                .collect(),
        }
    }

    pub(crate) fn resolve(&self, chart: &Chart) -> Result<Vec<ChartPoint>, &'static str> {
        if self.points.iter().all(|point| {
            point.original.is_some_and(|(original, time)| {
                point.price == original.price.to_string()
                    && point.bar_index == original.bar_idx.to_string()
                    && point.date
                        == time.map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string())
                    && point.time
                        == time.map_or_else(String::new, |t| t.format("%H:%M:%S%.f").to_string())
            })
        }) {
            return Ok(Vec::new());
        }

        self.points
            .iter()
            .map(|point| {
                let price = point
                    .price
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or("Enter a finite price for every point")?;
                if point.date.is_empty() && point.time.is_empty() {
                    let index = point
                        .bar_index
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or("Enter a finite bar index for every point")?;
                    return Ok(ChartPoint::new(index, price));
                }
                let date = NaiveDate::parse_from_str(&point.date, "%Y-%m-%d")
                    .map_err(|_| "Enter dates as YYYY-MM-DD")?;
                let time = NaiveTime::parse_from_str(&point.time, "%H:%M:%S%.f")
                    .or_else(|_| NaiveTime::parse_from_str(&point.time, "%H:%M"))
                    .map_err(|_| "Enter UTC times as HH:MM or HH:MM:SS")?;
                let time = date.and_time(time).and_utc();
                if let Some((original, Some(old_time))) = point.original
                    && old_time == time
                {
                    let current = ChartPoint::new(original.bar_idx, price);
                    if chart
                        .drawing_point_time(&current)
                        .is_some_and(|current| current != time)
                    {
                        return chart
                            .drawing_point_at(time, price)
                            .ok_or("Load chart data before editing times");
                    }
                    return Ok(ChartPoint { price, ..original });
                }
                chart
                    .drawing_point_at(time, price)
                    .ok_or("Load chart data before editing times")
            })
            .collect()
    }
}

/// Coordinates tab renderer.
pub struct CoordinatesTab;

impl CoordinatesTab {
    pub fn show(ui: &mut Ui, state: &mut CoordinatesState) {
        ui.space_md();
        if state.points.is_empty() {
            ui.label(l10n::text(
                ui.ctx(),
                "Open a drawing to edit its coordinates.",
            ));
        }
        for (i, point) in state.points.iter_mut().enumerate() {
            if i > 0 {
                ui.space_sm();
            }
            ui.group(|ui| {
                ui.strong(l10n::format_text(
                    ui.ctx(),
                    "Point {0}",
                    &[("0", format!("{}", i + 1))],
                ));
                ui.space_sm();
                FormGrid::new(format!("coord_grid_{i}"))
                    .spacing(ui.active_theme().spacing.lg, ui.active_theme().spacing.sm)
                    .show(ui, |ui| {
                        field(ui, "Price", &mut point.price, "");
                        if point.original.is_some_and(|(_, time)| time.is_none()) {
                            field(ui, "Bar index", &mut point.bar_index, "");
                        } else {
                            field(ui, "Date (UTC)", &mut point.date, "YYYY-MM-DD");
                            field(ui, "Time (UTC)", &mut point.time, "HH:MM:SS");
                        }
                    });
            });
        }
    }
}

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    let label = ui.label(l10n::text(ui.ctx(), label));
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(DESIGN_TOKENS.sizing.dialog.input_width)
            .hint_text(l10n::text(ui.ctx(), hint)),
    )
    .labelled_by(label.id);
    ui.end_row();
}
