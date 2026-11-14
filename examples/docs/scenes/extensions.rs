//! Chart setup for the horizontal-scale and rendering-extension guides.

/// Compare two generated volatility smiles by option strike, rather than by date.
pub fn volatility_smile() -> egui_charts::Chart {
    use eframe::egui;
    use egui_charts::ChartType;
    use egui_charts::model::{Bar, BarData, Marker, MarkerPos, MarkerShape, PriceLine};
    use egui_charts::scales::{PriceHorizontalScale, key_time};
    use egui_charts::widget::{Chart, SeriesId, SeriesOptions};
    use std::sync::Arc;

    const FORWARD: f64 = 4500.0;

    /// Strikes from 3,800 to 5,200, 25 apart near the forward and 100 apart in the wings.
    fn smile(days: f64) -> Vec<Bar> {
        (0..=56)
            .filter(|step| (16..=40).contains(step) || step % 4 == 0)
            .map(|step| 3800.0 + 25.0 * f64::from(step))
            .filter_map(|strike| {
                Some(Bar::from_value(
                    key_time(strike)?,
                    implied_volatility(strike, days),
                ))
            })
            .collect()
    }

    /// A sample smile in percent: a skew falling towards higher strikes and a curvature in
    /// log-moneyness, both flattening with time to expiry.
    fn implied_volatility(strike: f64, days: f64) -> f64 {
        let moneyness = (strike / FORWARD).ln();
        let flattening = (30.0 / days).sqrt();
        16.0 + 4.0 * (days / 365.0) - 60.0 * moneyness * flattening
            + 500.0 * moneyness * moneyness * flattening
    }

    let near = smile(30.0);
    let strikes = near.len();
    let mut chart = Chart::new(BarData::from_bars(near));
    chart.set_chart_type(ChartType::Line);
    // Three more bars than strikes leave room for the right offset.
    chart.set_visible_bars(strikes + 3);
    chart.set_horizontal_scale(Some(Arc::new(PriceHorizontalScale { precision: 0 })));
    chart.set_symbol("30-day IV, %");
    chart.config_mut().show_volume = false;
    let later = chart.add_series(SeriesOptions {
        name: "90-day IV, %".into(),
        color: egui::Color32::from_rgb(255, 152, 0),
        ..Default::default()
    });
    chart.set_data(later, smile(90.0));
    let at_the_money = key_time(FORWARD).expect("finite strike");
    chart.set_markers(
        SeriesId::MAIN,
        vec![
            Marker::new(at_the_money)
                .with_pos(MarkerPos::BelowBar)
                .with_shape(MarkerShape::ArrowUp)
                .with_color(egui::Color32::GRAY)
                .with_text("ATM"),
        ],
    );
    chart.set_price_lines(
        SeriesId::MAIN,
        vec![
            PriceLine::new(implied_volatility(FORWARD, 30.0), egui::Color32::GRAY)
                .with_title("ATM"),
        ],
    );
    chart
}

/// Space two generated yield curves by their actual maturity samples.
pub fn yield_curve() -> egui_charts::Chart {
    use eframe::egui;
    use egui_charts::model::{Bar, BarData};
    use egui_charts::scales::{MaturityScale, key_time};
    use egui_charts::widget::{Chart, SeriesOptions};
    use std::sync::Arc;

    const MATURITIES: [f64; 13] = [
        1.0, 2.0, 3.0, 4.0, 6.0, 12.0, 24.0, 36.0, 60.0, 84.0, 120.0, 240.0, 360.0,
    ];

    /// An inverted short end that rises again beyond five years.
    fn today(months: f64) -> f64 {
        let years = months / 12.0;
        3.9 + 1.4 * (-years / 1.5).exp() + 0.012 * years
    }

    /// A normal, upward-sloping curve.
    fn a_year_ago(months: f64) -> f64 {
        let years = months / 12.0;
        3.2 + 1.1 * (1.0 - (-years / 4.0).exp())
    }

    let bars = |curve: fn(f64) -> f64| {
        MATURITIES
            .iter()
            .filter_map(|&months| Some(Bar::from_value(key_time(months)?, curve(months))))
            .collect()
    };
    let mut chart = Chart::new(BarData::from_bars(bars(today)));
    chart.set_chart_type(egui_charts::ChartType::Line);
    chart.set_horizontal_scale(Some(Arc::new(MaturityScale::default())));
    chart.set_symbol("Today, yield %");
    chart.config_mut().show_volume = false;
    let previous = chart.add_series(SeriesOptions {
        name: "A year ago".into(),
        color: egui::Color32::from_rgb(242, 54, 69),
        ..Default::default()
    });
    chart.set_data(previous, bars(a_year_ago));
    // Three spacing units beyond the samples leave room for the right offset.
    chart.set_visible_bars(MATURITIES.len() + 3);
    chart
}

/// Shade regular sessions and label their opens on the shared time axis.
pub fn primitives() -> egui_charts::Chart {
    use chrono::{DateTime, Duration, Utc};
    use eframe::egui::{Color32, Painter, Rect};
    use egui_charts::model::Bar;
    use egui_charts::widget::{Primitive, PrimitivePass, PrimitiveView, SeriesId, TimeAxisLabel};

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

    let mut chart = sample_chart();
    chart.attach_primitive(
        SeriesId::MAIN,
        SessionHighlight {
            // 14:30 to 21:00 UTC, the regular hours of a US exchange in winter.
            open: Duration::minutes(14 * 60 + 30),
            close: Duration::hours(21),
            sessions: Vec::new(),
        },
    );
    chart
}

/// Draw each bar's high minus low as a lollipop in a separate pane.
pub fn custom_series() -> egui_charts::Chart {
    use eframe::egui::{Color32, CursorIcon, Painter, Pos2, Stroke};
    use egui_charts::model::Bar;
    use egui_charts::studies::{CustomIndicator, IndicatorValue};
    use egui_charts::widget::{Primitive, PrimitivePass, PrimitiveView};

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
                    painter
                        .line_segment([Pos2::new(centre.x, zero), centre], Stroke::new(1.0, color));
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

    let mut chart = sample_chart();
    let range = CustomIndicator::new("Bar Range", |bars| {
        bars.iter()
            .map(|bar| IndicatorValue::Single(bar.high - bar.low))
            .collect()
    })
    .with_overlay(false)
    .with_color(Color32::from_rgb(255, 152, 0));
    chart.add_custom_series(Box::new(range), Lollipops);
    chart
}

/// Shared fixed fifteen-minute history for the two primitive scenes.
fn sample_chart() -> egui_charts::Chart {
    use egui_charts::{Chart, model::BarData};

    let mut chart = Chart::new(BarData::from_bars((0..480).map(sample_bar).collect()));
    chart.set_symbol("SAMPLE");
    chart.set_visible_bars(80);
    chart.config_mut().show_volume = false;
    chart
}

/// Sample bar `i` of fifteen minutes: a deterministic wave, not market data.
fn sample_bar(i: usize) -> egui_charts::model::Bar {
    use chrono::{TimeZone, Utc};
    use egui_charts::model::Bar;

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
