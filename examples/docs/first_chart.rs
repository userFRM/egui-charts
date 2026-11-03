use chrono::{DateTime, TimeDelta};
use egui_charts::{
    Chart,
    model::{Bar, BarData, Timeframe},
};

/// Create an interactive chart from a fixed, oldest-first history.
pub fn first_chart() -> Chart {
    let start = DateTime::UNIX_EPOCH + TimeDelta::days(20_000);
    let bars = (0..120)
        .map(|i| {
            let open = 100.0 + f64::from(i) * 0.1;
            let close = open + if i % 3 == 0 { -0.6 } else { 0.8 };
            Bar::new(
                start + TimeDelta::minutes(i.into()),
                open,
                open.max(close) + 0.4,
                open.min(close) - 0.4,
                close,
                800.0 + f64::from(i % 7) * 100.0,
            )
        })
        .collect();
    let mut chart = Chart::new(BarData::from_bars(bars));
    chart.set_symbol("SAMPLE");
    chart.set_timeframe(Timeframe::Min1);
    chart.set_visible_bars(60);
    chart
}
