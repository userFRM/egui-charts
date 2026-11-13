//! Host drawing through primitives, built from the `widget` feature alone:
//!
//! ```sh
//! cargo run --release --example primitives --no-default-features --features widget
//! ```
//!
//! A session highlight shades the regular trading hours behind the bars and labels each session
//! open on the time axis. A custom series draws the range of each bar as a lollipop in a pane of
//! its own: a study supplies the values, and its renderer draws them, keeps the zero baseline in
//! the pane's range and shows a pointing hand over each head. Another custom series owns typed
//! points the host feeds independently of the bars: a band with a lower bound, a middle and an
//! upper bound at each key, some of them whitespace, drawn in a third pane. The line above the
//! chart reads the point under the pointer. The prices are generated sample data, not market
//! data.
//!
//! The Categorical keys tab supplies an ordered enum directly to a horizontal scale. Bars,
//! a marker, axis labels and the crosshair use those keys without host-generated dates.

use chrono::{DateTime, Duration, TimeZone, Utc};
use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Stroke};
use egui_charts::model::{Bar, BarData};
use egui_charts::scales::CustomHorizontalScale;
use egui_charts::studies::{CustomIndicator, IndicatorValue};
use egui_charts::widget::{
    Chart, ChartEvent, CustomPoint, PriceAxisLabel, Primitive, PrimitivePass, PrimitiveView,
    SeriesId, TimeAxisLabel,
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
    bands: SeriesId,
    /// The band under the pointer, as of the last event.
    hovered: Option<BandPoint>,
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
        let band_color = Color32::from_rgb(38, 166, 154);
        let bands = chart.add_custom_data_series::<BandPoint>(
            "Band",
            |study| study.with_overlay(false).with_color(band_color),
            Bands { color: band_color },
        );
        // The series takes its points in any order. A point with no values keeps its key empty.
        let mut points: Vec<_> = (0..120).rev().map(sample_band).collect();
        points.push(sample_band(100).without_values());
        chart.set_custom_data(bands, points);
        // A later point at a stored key replaces it; a new key extends the series.
        chart.update_custom_data(
            bands,
            BandPoint {
                values: Some([40.0, 62.0, 52.0]),
                ..sample_band(119)
            },
        );
        chart.update_custom_data(bands, sample_band(120));
        Self {
            chart,
            categories: categorical_chart(),
            show_categories: false,
            bands,
            hovered: None,
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.show_categories, false, "Time primitives");
                ui.selectable_value(&mut self.show_categories, true, "Categorical keys");
                if let Some(BandPoint {
                    time,
                    values: Some([low, high, middle]),
                }) = &self.hovered
                {
                    ui.label(format!(
                        "Band {}: {low:.1} / {middle:.1} / {high:.1}",
                        time.format("%b %-d %H:%M")
                    ));
                }
            });
            if self.show_categories {
                self.categories.show(ui);
            } else {
                self.chart.show(ui);
                let bands = self.bands;
                for event in self.chart.drain_events() {
                    if let ChartEvent::CrosshairMoved(pointer) = event {
                        self.hovered = pointer
                            .and_then(|pointer| pointer.custom_point::<BandPoint>(bands).cloned());
                    }
                }
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

/// A host-defined point with several values, unrelated to the chart's bars.
#[derive(Clone)]
struct BandPoint {
    time: DateTime<Utc>,
    /// Lower bound, upper bound and middle, the order [`CustomPoint::prices`] returns them in;
    /// `None` leaves the key empty.
    values: Option<[f64; 3]>,
}

impl BandPoint {
    fn without_values(self) -> Self {
        Self {
            values: None,
            ..self
        }
    }
}

impl CustomPoint for BandPoint {
    fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// All three fit the pane's scale, and the last, the middle, is the current value.
    fn prices(&self) -> Vec<f64> {
        self.values.map_or_else(Vec::new, Vec::from)
    }
}

/// Draws each band as a vertical bar between its bounds with a tick at its middle.
struct Bands {
    color: Color32,
}

impl Primitive for Bands {
    fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        if pass != PrimitivePass::Series {
            return;
        }
        let half = (view.mapping.bar_spacing * 1.2).clamp(1.5, 6.0);
        let points = view.custom_points::<BandPoint>();
        for point in &points[view.visible_custom_points()] {
            let (Some(x), Some([low, high, middle])) = (view.time_to_x(point.time), point.values)
            else {
                continue;
            };
            let [low, high, middle] = [low, high, middle].map(|y| view.mapping.price_to_y(y));
            painter.line_segment(
                [Pos2::new(x, low), Pos2::new(x, high)],
                Stroke::new(1.5, self.color),
            );
            painter.line_segment(
                [Pos2::new(x - half, middle), Pos2::new(x + half, middle)],
                Stroke::new(1.5, self.color),
            );
        }
    }

    /// The chart's own tag reads the last bar, which has no band: these keys are hourly.
    fn price_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
        let points = view.custom_points::<BandPoint>();
        points
            .iter()
            .rev()
            .find_map(|point| point.values)
            .map(|[.., middle]| PriceAxisLabel::new(middle, format!("{middle:.2}"), self.color))
            .into_iter()
            .collect()
    }
}

/// Sample band `i`, an hour apart from the first bar's time: a deterministic wave, not market
/// data.
fn sample_band(i: usize) -> BandPoint {
    let t = i as f64;
    let middle = 50.0 + 6.0 * (t / 11.0).sin();
    let spread = 2.0 + 1.5 * (t / 5.0).cos().abs();
    BandPoint {
        time: Utc
            .timestamp_opt(1_700_006_400 + 3600 * i as i64, 0)
            .unwrap(),
        values: Some([middle - spread, middle + spread, middle]),
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
