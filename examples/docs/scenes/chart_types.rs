use egui_charts::{Chart, ChartType};

/// Smooth the original OHLC history as Heikin-Ashi candles.
pub fn heikin() -> Chart {
    let mut chart = crate::first_chart::first_chart();
    chart.set_chart_type(ChartType::Heikin);
    chart
}

/// Build fixed half-unit Renko bricks from closing prices.
pub fn renko() -> Chart {
    let mut chart = crate::first_chart::first_chart();
    let mut params = chart.chart_type_params().clone();
    params.renko_brick_size = 0.5;
    params.renko_wicks = true;
    chart.set_chart_type_params(params);
    chart.set_chart_type(ChartType::Renko);
    chart
}

/// Build Point and Figure columns with quarter-unit boxes and a three-box reversal.
pub fn point_and_figure() -> Chart {
    let mut chart = crate::first_chart::first_chart();
    let mut params = chart.chart_type_params().clone();
    params.pnf_box_size = 0.25;
    params.pnf_reversal = 3;
    chart.set_chart_type_params(params);
    chart.set_chart_type(ChartType::PointAndFigure);
    chart
}
