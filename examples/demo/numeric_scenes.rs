//! Synthetic strike and maturity charts, independent of the price workspace.

use egui_charts::model::{Bar, BarData, ChartType};
use egui_charts::scales::{MaturityScale, PriceHorizontalScale, key_time};
use egui_charts::widget::{Chart, SeriesOptions};
use std::sync::Arc;

pub(super) fn volatility_smile() -> Chart {
    let smile = |days: f64| {
        (0..=56)
            .map(|step| {
                let strike = 3800.0 + 25.0 * f64::from(step);
                let m = (strike / 4500.0).ln();
                let flattening = (30.0 / days).sqrt();
                let iv =
                    16.0 + 4.0 * days / 365.0 - 60.0 * m * flattening + 500.0 * m * m * flattening;
                Bar::from_value(key_time(strike).expect("finite strike"), iv)
            })
            .collect()
    };
    let mut chart = Chart::new(BarData::from_bars(smile(30.0)));
    chart.set_chart_type(ChartType::Line);
    chart.set_horizontal_scale(Some(Arc::new(PriceHorizontalScale { precision: 0 })));
    chart.set_symbol("30-day IV, %");
    chart.config_mut().show_volume = false;
    chart.config_mut().show_legend_volume = false;
    chart.set_visible_bars(60);
    let later = chart.add_series(SeriesOptions {
        name: "90-day IV, %".into(),
        color: egui::Color32::from_rgb(255, 152, 0),
        ..Default::default()
    });
    chart.set_data(later, smile(90.0));
    chart
}

pub(super) fn yield_curve() -> Chart {
    let maturities = [
        1.0, 2.0, 3.0, 4.0, 6.0, 12.0, 24.0, 36.0, 60.0, 84.0, 120.0, 240.0, 360.0,
    ];
    let slots = maturities
        .into_iter()
        .map(|months| Bar::whitespace(key_time(months).expect("finite maturity")))
        .collect();
    let mut chart = Chart::new(BarData::from_bars(slots));
    chart.set_horizontal_scale(Some(Arc::new(MaturityScale::default())));
    chart.set_symbol("Yield, %");
    chart.config_mut().show_volume = false;
    chart.config_mut().show_legend_volume = false;
    for (name, color, earlier) in [
        ("Today", egui::Color32::from_rgb(41, 98, 255), false),
        ("A year ago", egui::Color32::from_rgb(242, 54, 69), true),
    ] {
        let series = chart.add_series(SeriesOptions {
            name: name.into(),
            color,
            ..Default::default()
        });
        let bars = maturities
            .into_iter()
            .map(|months| {
                let years: f64 = months / 12.0;
                let rate = if earlier {
                    3.2 + 1.1 * (1.0 - (-years / 4.0).exp())
                } else {
                    3.9 + 1.4 * (-years / 1.5).exp() + 0.012 * years
                };
                Bar::from_value(key_time(months).expect("finite maturity"), rate)
            })
            .collect();
        chart.set_data(series, bars);
    }
    chart.set_visible_bars(maturities.len() + 3);
    chart
}
