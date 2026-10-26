//! Implied volatility smiles of two expiries, by option strike:
//!
//! ```sh
//! cargo run --example volatility_smile
//! ```
//!
//! The horizontal axis is a [`PriceHorizontalScale`]: each bar's time holds a strike encoded with
//! [`key_time`], so the axis, the crosshair, the marker and the price line all work by strike.
//! The volatilities are generated sample data, not market data.

use egui_charts::ChartType;
use egui_charts::model::{Bar, BarData, Marker, MarkerPos, MarkerShape, PriceLine};
use egui_charts::scales::{PriceHorizontalScale, key_time};
use egui_charts::widget::{Chart, SeriesId, SeriesOptions};
use std::sync::Arc;

/// The forward price the sample smiles centre on.
const FORWARD: f64 = 4500.0;

fn main() -> eframe::Result {
    let mut app = VolatilitySmile::new();
    eframe::run_ui_native(
        "Implied volatility smile (sample data)",
        Default::default(),
        move |ui, _| app.ui(ui),
    )
}

pub(crate) struct VolatilitySmile {
    pub(crate) chart: Chart,
}

impl VolatilitySmile {
    pub(crate) fn new() -> Self {
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
        Self { chart }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            self.chart.show(ui);
        });
    }
}

/// One value per strike from 3,800 to 5,200 in steps of 25, for an expiry `days` away.
fn smile(days: f64) -> Vec<Bar> {
    (0..=56)
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
