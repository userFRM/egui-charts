//! Host drawing through primitives, built from the `widget` feature alone:
//!
//! ```sh
//! cargo run --release --example primitives --no-default-features --features widget
//! ```
//!
//! A session highlight shades the regular trading hours behind the bars and labels each session
//! open on the time axis. A custom series draws the range of each bar as a lollipop in a pane of
//! its own: a study supplies the values, and its renderer draws them, keeps the zero baseline in
//! the pane's range and shows a pointing hand over each head. The prices are generated sample
//! data, not market data.
//!
//! The Categorical keys tab supplies an ordered enum directly to a horizontal scale. Bars,
//! a marker, axis labels and the crosshair use those keys without host-generated dates.

use chrono::{DateTime, Duration, TimeZone, Utc};
use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Stroke};
use egui_charts::model::{Bar, BarData};
use egui_charts::scales::CustomHorizontalScale;
use egui_charts::studies::{CustomIndicator, IndicatorValue};
use egui_charts::widget::{
    Chart, Primitive, PrimitivePass, PrimitiveView, SeriesId, TimeAxisLabel,
};
use std::sync::Arc;

fn main() -> eframe::Result {
    let mut app = PrimitivesExample::new();
    eframe::run_ui_native(
        "Primitives (sample data)",
        Default::default(),
        move |ui, _| app.ui(ui),
    )
}

pub(crate) struct PrimitivesExample {
    pub(crate) chart: Chart,
    categories: Chart,
    show_categories: bool,
}

impl PrimitivesExample {
    pub(crate) fn new() -> Self {
        let mut chart = Chart::new(BarData::from_bars((0..480).map(sample_bar).collect()));
        chart.set_symbol("SAMPLE");
        chart.attach_primitive(
            SeriesId::MAIN,
            SessionHighlight {
                // 14:30 to 21:00 UTC, the regular hours of a US exchange in winter.
                open: Duration::minutes(14 * 60 + 30),
                close: Duration::hours(21),
                sessions: Vec::new(),
            },
        );
        let range = CustomIndicator::new("Bar Range", |bars| {
            bars.iter()
                .map(|bar| IndicatorValue::Single(bar.high - bar.low))
                .collect()
        })
        .with_overlay(false)
        .with_color(Color32::from_rgb(255, 152, 0));
        chart.add_custom_series(Box::new(range), Lollipops);
        Self {
            chart,
            categories: categorical_chart(),
            show_categories: false,
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.show_categories, false, "Time primitives");
                ui.selectable_value(&mut self.show_categories, true, "Categorical keys");
            });
            if self.show_categories {
                self.categories.show(ui);
            } else {
                self.chart.show(ui);
            }
        });
    }
}

/// Domain order is independent of the alphabetical order of the displayed labels.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Open,
    Morning,
    Auction,
    Afternoon,
    Close,
}

fn categorical_chart() -> Chart {
    let keys = [
        Phase::Open,
        Phase::Morning,
        Phase::Auction,
        Phase::Afternoon,
        Phase::Close,
    ];
    let scale = Arc::new(CustomHorizontalScale::new(keys, |phase| {
        match phase {
            Phase::Open => "Open",
            Phase::Morning => "Morning",
            Phase::Auction => "Auction",
            Phase::Afternoon => "Afternoon",
            Phase::Close => "Close",
        }
        .to_owned()
    }));
    let mut chart = Chart::new(BarData::new());
    chart.set_symbol("SAMPLE PHASES");
    chart.set_chart_type(egui_charts::ChartType::LineWithMarkers);
    chart.set_horizontal_scale(Some(scale.clone()));
    chart.set_data(
        SeriesId::MAIN,
        keys.iter()
            .zip([100.0, 104.0, 102.0, 108.0, 106.0])
            .map(|(key, value)| scale.value(key, value).unwrap())
            .collect(),
    );
    // Repeated keys update their existing slot instead of adding a category.
    chart.update(SeriesId::MAIN, scale.value(&Phase::Auction, 103.0).unwrap());
    chart.set_markers(
        SeriesId::MAIN,
        vec![
            scale
                .marker(&Phase::Auction)
                .unwrap()
                .with_text("Auction".to_owned())
                .with_color(Color32::from_rgb(177, 52, 210)),
        ],
    );
    chart.set_visible_bars(8);
    chart
}

/// Shades the regular session of every day behind the bars and labels its open on the time axis.
struct SessionHighlight {
    /// Open and close after midnight UTC.
    open: Duration,
    close: Duration,
    /// Open and close of each day the bars cover.
    sessions: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl Primitive for SessionHighlight {
    fn data_changed(&mut self, bars: &[Bar]) {
        self.sessions.clear();
        let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
            return;
        };
        let mut day = first.time.date_naive();
        while day <= last.time.date_naive() {
            let midnight = day.and_hms_opt(0, 0, 0).unwrap().and_utc();
            self.sessions
                .push((midnight + self.open, midnight + self.close));
            day = day.succ_opt().unwrap();
        }
    }

    fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        if pass != PrimitivePass::Below {
            return;
        }
        let plot = view.mapping.rect;
        // Bars are centred on their open times, so a session starts half a bar before its first.
        let half = view.mapping.bar_spacing / 2.0;
        for (open, close) in &self.sessions {
            if let (Some(left), Some(right)) = (view.time_to_x(*open), view.time_to_x(*close)) {
                painter.rect_filled(
                    Rect::from_x_y_ranges(left - half..=right - half, plot.y_range()),
                    0.0,
                    Color32::from_rgba_unmultiplied(41, 98, 255, 24),
                );
            }
        }
    }

    fn time_axis_labels(&self, _view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
        self.sessions
            .iter()
            .map(|(open, _)| {
                TimeAxisLabel::new(
                    *open,
                    format!("Open {}", open.format("%b %-d")),
                    Color32::from_rgb(41, 98, 255),
                )
            })
            .collect()
    }
}

/// Draws the values of its study as stems from zero, each with a round head.
struct Lollipops;

impl Lollipops {
    fn head(view: &PrimitiveView<'_>, index: usize) -> Option<(Pos2, f32, Color32)> {
        let study = view.study?;
        let value = study.plot_value(0, index)?;
        let radius = (view.mapping.bar_spacing * 0.3).clamp(1.5, 4.0);
        let color = study.colors().first().copied().unwrap_or(Color32::GRAY);
        let centre = Pos2::new(
            view.index_to_x(index as f64),
            view.mapping.price_to_y(value),
        );
        Some((centre, radius, color))
    }
}

impl Primitive for Lollipops {
    fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        if pass != PrimitivePass::Series {
            return;
        }
        let zero = view.mapping.price_to_y(0.0);
        for index in view.visible.clone() {
            if let Some((centre, radius, color)) = Self::head(view, index) {
                painter.line_segment([Pos2::new(centre.x, zero), centre], Stroke::new(1.0, color));
                painter.circle_filled(centre, radius, color);
            }
        }
    }

    fn autoscale(&self, _bars: &[Bar], _visible: std::ops::Range<usize>) -> Option<(f64, f64)> {
        // The study's values set the top of the pane; the stems start at zero.
        Some((0.0, 0.0))
    }

    fn hit_test(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<CursorIcon> {
        let index = view.mapping.x_to_idx_f64(pos.x).round();
        if index < 0.0 || !view.visible.contains(&(index as usize)) {
            return None;
        }
        let (centre, radius, _) = Self::head(view, index as usize)?;
        (centre.distance(pos) <= radius + 3.0).then_some(CursorIcon::PointingHand)
    }
}

/// Sample bar `i` of fifteen minutes: a deterministic wave, not market data.
pub(crate) fn sample_bar(i: usize) -> Bar {
    let t = i as f64;
    let open = 100.0 + 8.0 * (t / 50.0).sin() + 2.0 * (t / 9.0).sin();
    let close = open + 1.2 * (t * 1.3).sin();
    let spread = 0.4 + 0.6 * (t / 6.0).sin().abs();
    let time = Utc
        .timestamp_opt(1_700_006_400 + 900 * i as i64, 0)
        .unwrap();
    Bar::new(
        time,
        open,
        open.max(close) + spread,
        open.min(close) - spread,
        close,
        1000.0 + 500.0 * (t / 3.0).cos(),
    )
}
