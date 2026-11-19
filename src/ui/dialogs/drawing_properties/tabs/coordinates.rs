//! Coordinates tab - staged price and UTC time points; freehand strokes have none.

use crate::drawings::{ChartPoint, Drawing, DrawingInteractionMode, DrawingToolType};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::FormGrid;
use crate::ui_kit::ext::UiExt;
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
    /// The drawing's line is fitted to the bars between its points, so their prices do not
    /// move it.
    fitted: bool,
}

/// Coordinates staged independently of the live drawing until Apply.
#[derive(Clone, Debug, Default)]
pub struct CoordinatesState {
    /// One entry for every anchor of the selected drawing; none for a freehand stroke, which
    /// moves only as a whole.
    pub points: Vec<CoordinatePoint>,
}

impl CoordinatesState {
    pub(crate) fn from_drawing(drawing: &Drawing, chart: &Chart) -> Self {
        if !CoordinatesTab::available(drawing.tool_type) {
            return Self::default();
        }
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
                        fitted: drawing.tool_type == DrawingToolType::RegressionTrend,
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
#[derive(Debug)]
pub struct CoordinatesTab;

impl CoordinatesTab {
    /// Whether the tab lists `tool`'s anchors. A freehand stroke has none to edit: moving one
    /// of its samples would break its shape, so it moves only as a whole.
    pub(crate) fn available(tool: DrawingToolType) -> bool {
        tool.interaction_mode() != DrawingInteractionMode::ContinuousDraw
    }

    pub fn show(ui: &mut Ui, state: &mut CoordinatesState) {
        ui.space_md();
        if state.points.is_empty() {
            ui.label(l10n::text(
                ui.ctx(),
                "Open a drawing to edit its coordinates.",
            ));
        }
        if state.points.first().is_some_and(|point| point.fitted) {
            ui.label(l10n::text(
                ui.ctx(),
                "The line is fitted to the bars between the points; their prices do not move it.",
            ));
            ui.space_sm();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarData;

    /// A regression trend's points take prices and bars like any two-point tool's, under a note
    /// that the prices do not move its fitted line.
    #[test]
    fn regression_coordinates_edit_prices_and_bars_like_other_tools() {
        let chart = Chart::new(BarData::from_bars(Vec::new()));
        for tool in [DrawingToolType::RegressionTrend, DrawingToolType::TrendLine] {
            let mut drawing = Drawing::new(1, tool);
            drawing.chart_points = vec![ChartPoint::new(2.0, 123.5), ChartPoint::new(8.0, 90.0)];
            let mut state = CoordinatesState::from_drawing(&drawing, &chart);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                CoordinatesTab::show(ui, &mut state)
            });
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.job.text.clone()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert_eq!(texts.iter().filter(|t| *t == "Price").count(), 2);
            assert_eq!(
                texts.iter().any(|t| t.starts_with("The line is fitted")),
                tool == DrawingToolType::RegressionTrend
            );
            state.points[0].price = "NaN".into();
            state.points[0].bar_index = "3".into();
            assert!(state.resolve(&chart).is_err());
            state.points[0].price = "120".into();
            assert_eq!(
                state.resolve(&chart).unwrap(),
                vec![ChartPoint::new(3.0, 120.0), ChartPoint::new(8.0, 90.0)]
            );
        }
    }

    /// An anchored text keeps its place in the pane through a scroll, and its coordinates
    /// follow it: the chart and the Coordinates tab report the bar under it, and points set
    /// through the chart move it there as one undoable edit.
    #[test]
    fn anchored_text_coordinates_follow_where_it_is_drawn() {
        use crate::widget::test_frames::{chart, frame};
        let day = |bar: i64| DateTime::from_timestamp(1_700_000_000 + bar * 86_400, 0).unwrap();
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let mut text = Drawing::new(0, DrawingToolType::AnchoredText);
        text.text = Some("Anchored".into());
        let id = chart.create_drawing(text, &[(day(50), 15.0)]).unwrap();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let drawn = chart.drawings().drawings[0].points[0];
        let spacing = chart.state.time_scale().bar_spacing();
        let offset = chart.state.time_scale().right_offset_precise() as f32;
        // Ten more bars of space on the right move every bar ten slots to the left.
        chart.scroll_to_position(offset + 10.0, false);
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(chart.drawings().drawings[0].points[0], drawn);
        assert_eq!(chart.drawing_points(id), Some(vec![(day(60), 15.0)]));
        let tab = CoordinatesState::from_drawing(&chart.drawings().drawings[0], &chart);
        let point = &tab.points[0];
        assert_eq!(
            [&point.date, &point.time, &point.price],
            ["2024-01-13", "22:13:20", "15"]
        );

        chart.set_drawing_points(id, &[(day(80), 18.0)]).unwrap();
        frame(&ctx, &mut chart, 0.2, vec![]);
        let moved = chart.drawings().drawings[0].points[0];
        // Bar 60 is under the old x, so bar 80 is twenty slots to the right; 18 is above 15.
        assert!(
            (moved.x - drawn.x - 20.0 * spacing).abs() < 0.5 && moved.y < drawn.y,
            "{moved:?}"
        );
        assert_eq!(chart.drawing_points(id), Some(vec![(day(80), 18.0)]));
        assert!(chart.drawings_mut().undo());
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(chart.drawings().drawings[0].points[0], drawn);
        assert_eq!(chart.drawing_points(id), Some(vec![(day(60), 15.0)]));
    }
}
